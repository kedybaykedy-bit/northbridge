//! LocalSend server + discovery lifecycle in one place.
//!
//! Reuses the `localsend` crate for: HTTPS server (register/info/prepare-upload),
//! multicast discovery, fingerprint identity. northbridge adds the packet
//! channel on top (see `packet_channel`).

use anyhow::Result;
use nb_core::channel::Channel;
use nb_core::identity::Identity;
use nb_core::peer::Peers;
use localsend::discovery::{
    DEFAULT_DISCOVERY_TIMEOUT, DeviceIdentity, DiscoveryConfig, DiscoveryEvent, DiscoveryHandle,
    DiscoveredDevice,
};
use localsend::http::dto_v2::RegisterDtoV2;
use localsend::http::server::v2::{PrepareUploadDecisionV2, ServerEventV2};
use localsend::http::server::{ServerConfigV2, ServerHandle, TlsConfig, start_with_port};
use localsend::http::state::ClientInfo;
use localsend::model::discovery::{DeviceType, ProtocolType, PROTOCOL_VERSION_V2};
use localsend::multicast::{
    DEFAULT_MULTICAST_GROUP, DEFAULT_MULTICAST_GROUP_V6, DEFAULT_PORT, MulticastDevice,
};
use localsend::util::interface::InterfaceFilter;
use tokio::sync::{mpsc, oneshot};

pub struct LsHandles {
    /// `None` when the HTTP port was already taken (discovery-only mode).
    pub server: Option<ServerHandle>,
    pub discovery: DiscoveryHandle,
    server_stop: Option<oneshot::Sender<()>>,
    discovery_stop: oneshot::Sender<()>,
}

impl LsHandles {
    pub async fn shutdown(self) {
        if let Some(tx) = self.server_stop {
            let _ = tx.send(());
        }
        let _ = self.discovery_stop.send(());
        if let Some(server) = self.server {
            let _ = server.wait_stopped().await;
        }
        let _ = self.discovery.wait_stopped().await;
    }
}

fn client_info(identity: &Identity) -> ClientInfo {
    ClientInfo {
        alias: identity.alias.clone(),
        version: PROTOCOL_VERSION_V2.to_string(),
        device_model: Some("northbridge".to_string()),
        device_type: Some(DeviceType::Desktop),
        // LocalSend uses token to merge peers across channels; the
        // fingerprint is our channel-independent ID.
        token: identity.fingerprint.clone(),
    }
}

fn multicast_device(identity: &Identity) -> MulticastDevice {
    MulticastDevice {
        alias: identity.alias.clone(),
        version: PROTOCOL_VERSION_V2.to_string(),
        device_model: Some("northbridge".to_string()),
        device_type: Some(DeviceType::Desktop),
        fingerprint: identity.fingerprint.clone(),
        port: identity.port,
        protocol: ProtocolType::Https,
        download: false,
    }
}

/// Start LocalSend HTTPS server + multicast discovery for `identity`.
/// `server_events` receives Register/PrepareUpload/… for the caller to pump.
///
/// When the HTTP port is already taken (a `northbridge listen` instance is
/// running), falls back to discovery-only: outbound probes and announcement
/// listening still work, and the running instance answers register requests
/// for us (same identity). `server_rx` then simply stays silent.
pub async fn start_ls(
    identity: &Identity,
    server_events: mpsc::Sender<ServerEventV2>,
    discovery_events: mpsc::Sender<DiscoveryEvent>,
) -> Result<LsHandles> {
    let (server, server_stop) = match try_start_server(identity, server_events).await {
        Ok(pair) => (Some(pair.0), Some(pair.1)),
        Err(e) if is_addr_in_use(&e) => {
            eprintln!(
                "[server] port {} already in use — discovery-only mode (a `listen` instance is running)",
                identity.port
            );
            (None, None)
        }
        Err(e) => return Err(e),
    };

    let (discovery_stop_tx, discovery_stop_rx) = oneshot::channel();
    let discovery = localsend::discovery::start(
        DiscoveryConfig {
            group: DEFAULT_MULTICAST_GROUP,
            group_v6: Some(DEFAULT_MULTICAST_GROUP_V6),
            port: DEFAULT_PORT,
            interface_filter: InterfaceFilter::default(),
            device: multicast_device(identity),
            identity: DeviceIdentity {
                cert_pem: identity.cert_pem.clone(),
                private_key_pem: identity.key_pem.clone(),
            },
            timeout: DEFAULT_DISCOVERY_TIMEOUT,
            event_tx: Some(discovery_events),
        },
        discovery_stop_rx,
    )
    .await;

    if let Some(err) = discovery.multicast_error() {
        eprintln!("[discovery] multicast unavailable: {err:#}");
    }

    Ok(LsHandles {
        server,
        discovery,
        server_stop,
        discovery_stop: discovery_stop_tx,
    })
}

async fn try_start_server(
    identity: &Identity,
    server_events: mpsc::Sender<ServerEventV2>,
) -> Result<(ServerHandle, oneshot::Sender<()>)> {
    let (server_stop_tx, server_stop_rx) = oneshot::channel();
    let server = start_with_port(
        identity.port,
        Some(TlsConfig {
            cert: identity.cert_pem.clone(),
            private_key: identity.key_pem.clone(),
        }),
        client_info(identity),
        None,
        Some(ServerConfigV2 {
            pin: None,
            verify_checksums: true,
            event_tx: server_events,
        }),
        Default::default(),
        server_stop_rx,
    )
    .await?;
    Ok((server, server_stop_tx))
}

fn is_addr_in_use(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>()
        .map(|e| e.kind() == std::io::ErrorKind::AddrInUse)
        .unwrap_or(false)
        || format!("{err:#}").contains("Address already in use")
}

/// Fold a LocalSend `Register` event into the peer store.
/// Returns `true` when the observation was *new* (first sighting of the
/// fingerprint, or of this channel on an known fingerprint) — callers use it
/// to decide whether to log/persist.
pub fn ingest_register(peers: &mut Peers, own_fp: &str, host: &str, info: &RegisterDtoV2) -> bool {
    if info.fingerprint == own_fp {
        return false;
    }
    let channel = Channel::classify(host);
    observe_fresh(peers, &info.fingerprint, &info.alias, channel, host, info.port)
}

/// Fold a discovery event into the peer store.
/// Returns `true` only for fresh observations (see [`ingest_register`]).
/// Unreachable events are printed at most once per (fingerprint, host) —
/// announce bursts would otherwise repeat the same failure every cycle.
pub fn ingest_discovery(
    peers: &mut Peers,
    own_fp: &str,
    event: &DiscoveryEvent,
    notices: &mut UnreachableNotices,
) -> bool {
    match event {
        DiscoveryEvent::Discovered { device } | DiscoveryEvent::Updated { device } => {
            ingest_discovered(peers, own_fp, device)
        }
        DiscoveryEvent::Unreachable { device, error } => {
            if let Some(line) = notices.notice(device, error) {
                eprintln!("[discovery] {line}");
            }
            false
        }
        DiscoveryEvent::MulticastFailed => {
            eprintln!("[discovery] multicast sockets failed permanently");
            false
        }
    }
}

/// Dedup log for unreachable-device events (mirrors LocalSend CLI's
/// `UnreachableNotices`): one line per (fingerprint, host) per run.
#[derive(Default)]
pub struct UnreachableNotices {
    logged: std::collections::HashSet<(String, String)>,
}

impl UnreachableNotices {
    pub fn new() -> Self {
        Self::default()
    }

    /// The log line for this failure, `None` when already reported.
    pub fn notice(
        &mut self,
        device: &DiscoveredDevice,
        error: &str,
    ) -> Option<String> {
        let host = device
            .channel
            .http()
            .map(|h| h.host.clone())
            .unwrap_or_else(|| "unknown address".into());
        if !self
            .logged
            .insert((device.fingerprint.clone(), host.clone()))
        {
            return None;
        }
        // Everything but the host is what the announcement claimed.
        let err = error.lines().next().unwrap_or(error);
        let err = if err.chars().count() > 160 {
            let cut: String = err.chars().take(160).collect();
            format!("{cut}…")
        } else {
            err.to_string()
        };
        Some(format!(
            "unreachable: {} ({host}): {err}",
            device.alias
        ))
    }
}

pub fn ingest_discovered(peers: &mut Peers, own_fp: &str, device: &DiscoveredDevice) -> bool {
    if device.fingerprint == own_fp {
        return false;
    }
    let Some(http) = device.channel.http() else {
        return false;
    };
    let channel = Channel::classify(&http.host);
    observe_fresh(peers, &device.fingerprint, &device.alias, channel, &http.host, http.port)
}

/// `peers.observe` + freshness check: true when this (fingerprint, channel)
/// pair was not present before.
fn observe_fresh(
    peers: &mut Peers,
    fingerprint: &str,
    alias: &str,
    channel: Channel,
    host: &str,
    http_port: u16,
) -> bool {
    let fresh = peers
        .peers
        .get(fingerprint)
        .map(|p| !p.addrs.contains_key(&channel))
        .unwrap_or(true);
    peers.observe(fingerprint, alias, channel, host, http_port);
    fresh
}

/// Answer an incoming file-transfer offer: decline (M0 has no receive UI).
pub fn decline_upload(decision: oneshot::Sender<PrepareUploadDecisionV2>) {
    let _ = decision.send(PrepareUploadDecisionV2::Decline);
}
