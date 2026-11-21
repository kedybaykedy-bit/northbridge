//! `northbridge` CLI — discovery, pairing, plugins over LAN/Tailscale/USB.

mod input;
mod x11;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use input::{InputState, TouchpadPlugin};
use nb_clipboard::{ClipboardPlugin, SharedState};
use nb_core::channel::Channel;
use nb_core::identity::{Identity, config_dir};
use nb_core::packet::NetworkPacket;
use nb_core::peer::Peers;
use nb_core::plugin::{Plugin, PluginCtx, dispatch, plugins_for_capabilities, union_incoming, union_outgoing};
use nb_discovery::{discover_staged, probe_tailscale, tailscale_channels};
use nb_ping::PingPlugin;
use nb_screen::ScreenServer;
use nb_audio::AudioServer;
use nb_mirror::MirrorServer;
use nb_transport::ls::{
    UnreachableNotices, ingest_discovery, ingest_register, start_ls,
};
use nb_transport::{InputRegistry, PacketHandler, send_files, send_packet, start_packet_server};
use localsend::http::server::common::save::FileUploadTarget;
use localsend::http::server::v2::{PrepareUploadDecisionV2, ServerEventV2};
use localsend::discovery::DiscoveryEvent;
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

const PAIR_TYPE: &str = "nb.pair";
const STAGE_INTERVAL: Duration = Duration::from_secs(15);
/// Upper bound for one staged-discovery pass (announce + probes + subnet
/// scan). Keeps `pair`/`ping` from hanging when a scan stalls.
const STAGED_TIMEOUT: Duration = Duration::from_secs(20);
/// Upper bound for graceful shutdown; teardown must never block a CLI.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Run `f` with a timeout, logging and returning `Ok(())` on expiry —
/// discovery/teardown is best-effort, never worth hanging a command for.
async fn with_timeout<T>(
    what: &str,
    limit: Duration,
    f: impl std::future::Future<Output = T>,
) -> Option<T> {
    match tokio::time::timeout(limit, f).await {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("[discovery] {what} timed out after {limit:?}");
            None
        }
    }
}

#[derive(Parser)]
#[command(
    name = "northbridge",
    about = "Unified LAN hub: discover, pair and plugin-share across LAN, Tailscale and USB",
    version
)]
struct Cli {
    /// Config directory (default: ~/.config/northbridge)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the daemon: discover peers, serve pairing + plugins.
    Listen {
        /// Reject inbound pairing requests instead of auto-accepting.
        #[arg(long)]
        no_auto_pair: bool,
        /// Override the LocalSend HTTP port (default 53317).
        #[arg(long)]
        port: Option<u16>,
        /// Override the device alias (default: hostname).
        #[arg(long)]
        alias: Option<String>,
    },
    /// One-shot discovery: announce, probe, print what is out there.
    Discover {
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        alias: Option<String>,
    },
    /// List known devices from the peer store.
    Devices,
    /// Pair with a device (by alias or fingerprint prefix).
    Pair {
        /// Device alias or fingerprint prefix.
        target: String,
    },
    /// Ping a paired device over the packet channel.
    Ping {
        /// Device alias or fingerprint prefix.
        target: String,
    },
    /// Send files to a device over the LocalSend HTTP API.
    Send {
        /// Device alias or fingerprint prefix.
        target: String,
        /// Files to send.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Show local identity and store summary.
    Status,
}

fn peers_path(cfg: &Option<PathBuf>) -> PathBuf {
    cfg.clone()
        .unwrap_or_else(config_dir)
        .join("peers.json")
}

fn identity_dir(cfg: &Option<PathBuf>) -> PathBuf {
    cfg.clone().unwrap_or_else(config_dir)
}

fn load_identity(cfg: &Option<PathBuf>, alias: Option<String>, port: Option<u16>) -> Result<Identity> {
    Identity::load_or_generate(&identity_dir(cfg), alias, port)
        .context("load or generate identity")
}

fn load_peers(cfg: &Option<PathBuf>) -> Result<Peers> {
    Peers::load(&peers_path(cfg))
}

struct ScreenPlugin {
    server: Option<ScreenServer>,
}

impl Plugin for ScreenPlugin {
    fn id(&self) -> &str {
        nb_screen::CAP
    }

    fn incoming(&self) -> &[&str] {
        &[nb_screen::CAP]
    }

    fn outgoing(&self) -> &[&str] {
        &[nb_screen::CAP]
    }

    fn on_packet(&mut self, ctx: &PluginCtx, pkt: &NetworkPacket) -> Option<NetworkPacket> {
        let server = self.server.as_ref()?;
        let body = nb_screen::handle_control_from(server, &ctx.peer_fingerprint, &pkt.body)?;
        Some(NetworkPacket::new(nb_screen::CAP, body))
    }
}

struct AudioPlugin {
    server: Option<AudioServer>,
}

impl Plugin for AudioPlugin {
    fn id(&self) -> &str {
        nb_audio::CAP
    }

    fn incoming(&self) -> &[&str] {
        &[nb_audio::CAP]
    }

    fn outgoing(&self) -> &[&str] {
        &[nb_audio::CAP]
    }

    fn on_packet(&mut self, ctx: &PluginCtx, pkt: &NetworkPacket) -> Option<NetworkPacket> {
        let server = self.server.as_ref()?;
        let body = nb_audio::handle_control_from(server, &ctx.peer_fingerprint, &pkt.body)?;
        Some(NetworkPacket::new(nb_audio::CAP, body))
    }
}

struct MirrorPlugin {
    server: Option<MirrorServer>,
}

impl Plugin for MirrorPlugin {
    fn id(&self) -> &str {
        nb_mirror::CAP
    }

    fn incoming(&self) -> &[&str] {
        &[nb_mirror::CAP]
    }

    fn outgoing(&self) -> &[&str] {
        &[nb_mirror::CAP]
    }

    fn on_packet(&mut self, ctx: &PluginCtx, pkt: &NetworkPacket) -> Option<NetworkPacket> {
        let server = self.server.as_ref()?;
        let reply = server.handle_control_from(&ctx.peer_fingerprint, &pkt.body);
        eprintln!(
            "[mirror] control from {} body={} reply={}",
            &ctx.peer_fingerprint[..12.min(ctx.peer_fingerprint.len())],
            pkt.body,
            reply.is_some(),
        );
        reply.map(|body| NetworkPacket::new(nb_mirror::CAP, body))
    }
}

fn make_plugins(
    clip: SharedState,
    input: Arc<Mutex<InputState>>,
    registry: InputRegistry,
    screen: Option<ScreenServer>,
    audio: Option<AudioServer>,
    mirror: Option<MirrorServer>,
) -> Vec<Box<dyn Plugin>> {
    vec![
        Box::new(PingPlugin),
        Box::new(ClipboardPlugin::new(clip)),
        Box::new(TouchpadPlugin::new(input, registry)),
        Box::new(ScreenPlugin { server: screen }),
        Box::new(AudioPlugin { server: audio }),
        Box::new(MirrorPlugin { server: mirror }),
    ]
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Listen {
            no_auto_pair,
            port,
            alias,
        } => cmd_listen(&cli.config, no_auto_pair, port, alias).await,
        Cmd::Discover { port, alias } => cmd_discover(&cli.config, port, alias).await,
        Cmd::Devices => cmd_devices(&cli.config),
        Cmd::Pair { target } => cmd_pair(&cli.config, &target).await,
        Cmd::Ping { target } => cmd_ping(&cli.config, &target).await,
        Cmd::Send { target, files } => cmd_send(&cli.config, &target, files).await,
        Cmd::Status => cmd_status(&cli.config),
    }
}

// ---------------------------------------------------------------------------
// listen
// ---------------------------------------------------------------------------

async fn cmd_listen(
    cfg: &Option<PathBuf>,
    no_auto_pair: bool,
    port: Option<u16>,
    alias: Option<String>,
) -> Result<()> {
    let identity = load_identity(cfg, alias, port)?;
    let peers = Arc::new(Mutex::new(load_peers(cfg)?));
    let clip = nb_clipboard::shared_state();
    let registry = InputRegistry::new();
    let input = Arc::new(Mutex::new(InputState::with_display()));
    let screen = ScreenServer::start(identity.clone());
    let audio = AudioServer::start(identity.clone());
    let mirror = MirrorServer::start(identity.clone(), registry.clone());
    println!("[screen] frame port :{}   [audio] :{}   [mirror] :{}",
        identity.port + 2, identity.port + 3, identity.port + 4);
    let plugins = Arc::new(Mutex::new(make_plugins(
        Arc::clone(&clip),
        Arc::clone(&input),
        registry.clone(),
        Some(screen),
        Some(audio),
        Some(mirror),
    )));

    println!("northbridge listen");
    println!("  alias:       {}", identity.alias);
    println!("  fingerprint: {}", identity.fingerprint);
    println!("  http:        :{}   packet: :{}", identity.port, identity.packet_port());
    println!("  config:      {}", identity_dir(cfg).display());
    println!("  auto-pair:   {}", if no_auto_pair { "OFF" } else { "ON" });
    println!();

    let (server_tx, mut server_rx) = mpsc::channel::<ServerEventV2>(32);
    let (disc_tx, mut disc_rx) = mpsc::channel::<DiscoveryEvent>(64);
    let handles = start_ls(&identity, server_tx, disc_tx).await?;
    let mut notices = UnreachableNotices::new();

    // Packet channel handler: pairing + plugin dispatch.
    let handler = make_handler(&identity, Arc::clone(&peers), Arc::clone(&plugins), !no_auto_pair);
    let packet_identity = identity.clone();
    tokio::spawn(async move {
        if let Err(e) = start_packet_server(packet_identity, handler, registry).await {
            eprintln!("[packet] server stopped: {e:#}");
        }
    });

    // Clipboard watcher: push local changes to paired peers (M1).
    spawn_clipboard_watcher(identity.clone(), Arc::clone(&peers), Arc::clone(&clip));

    // Initial staged discovery + Tailscale probe.
    {
        let d = &handles.discovery;
        let known = tailscale_channels(identity.port);
        if let Some(Err(e)) =
            with_timeout("staged discovery", STAGED_TIMEOUT, discover_staged(d, known, identity.port)).await
        {
            eprintln!("[discovery] staged failed: {e:#}");
        }
        let n = probe_tailscale(d, identity.port).await;
        if n > 0 {
            println!("[tailscale] probed {n} online peer(s)");
        }
        drain_discovery(&mut disc_rx, &peers, &identity.fingerprint, &mut notices);
    }

    println!("listening — Ctrl-C to stop\n");

    let mut stage = tokio::time::interval(STAGE_INTERVAL);
    stage.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    stage.tick().await; // first tick is immediate; skip it

    loop {
        tokio::select! {
            Some(ev) = server_rx.recv() => {
                let mut p = peers.lock().unwrap();
                match ev {
                    ServerEventV2::Register { ip, info } => {
                        let host = ip.to_string();
                        let fresh = ingest_register(&mut p, &identity.fingerprint, &host, &info);
                        if fresh {
                            let ch = Channel::classify(&host);
                            println!("[register] NEW {} ({}) — {}", info.alias, ch, &info.fingerprint[..12.min(info.fingerprint.len())]);
                            save_peers(&p)?;
                        }
                    }
                    ServerEventV2::PrepareUpload {
                        info,
                        cert_fingerprint,
                        decision_tx,
                        files,
                        ..
                    } => {
                        // Only accept transfers from paired devices; the cert
                        // fingerprint is unspoofable (mTLS), unlike info.fingerprint.
                        let trusted = cert_fingerprint
                            .as_deref()
                            .map(|fp| p.peers.get(fp).map(|d| d.trusted).unwrap_or(false))
                            .unwrap_or(false);
                        if trusted {
                            let ids: HashSet<String> = files.keys().cloned().collect();
                            println!(
                                "[transfer] {} sends {} file(s) — accepting",
                                info.alias,
                                ids.len()
                            );
                            let _ = decision_tx.send(PrepareUploadDecisionV2::Accept(ids));
                        } else {
                            println!(
                                "[transfer] {} (not paired) offers files — declined",
                                info.alias
                            );
                            let _ = decision_tx.send(PrepareUploadDecisionV2::Decline);
                        }
                    }
                    ServerEventV2::FileUpload { file, target_tx, .. } => {
                        let dir = download_dir();
                        let path = unique_download_path(&dir, &file.file_name);
                        let (result_tx, result_rx) = oneshot::channel();
                        let display = path.clone();
                        let _ = target_tx.send(FileUploadTarget::Path {
                            path,
                            result_tx,
                            progress_tx: None,
                        });
                        tokio::spawn(async move {
                            match result_rx.await {
                                Ok(Ok(())) => println!("[transfer] saved {}", display.display()),
                                Ok(Err(e)) => {
                                    println!("[transfer] failed {}: {e}", display.display())
                                }
                                Err(_) => {
                                    println!("[transfer] interrupted {}", display.display())
                                }
                            }
                        });
                    }
                    ServerEventV2::ListenerFailed { error } => {
                        eprintln!("[server] {error}");
                    }
                    _ => {}
                }
            }
            Some(ev) = disc_rx.recv() => {
                let fresh = {
                    let mut p = peers.lock().unwrap();
                    ingest_discovery(&mut p, &identity.fingerprint, &ev, &mut notices)
                };
                if fresh {
                    if let DiscoveryEvent::Discovered { device } | DiscoveryEvent::Updated { device } = &ev {
                        let ch = device.channel.http()
                            .map(|h| Channel::classify(&h.host))
                            .unwrap_or(Channel::Unknown);
                        println!("[discover] NEW {} ({}) — {}", device.alias, ch, &device.fingerprint[..12.min(device.fingerprint.len())]);
                    }
                    let p = peers.lock().unwrap();
                    save_peers(&p)?;
                }
            }
            _ = stage.tick() => {
                let known = tailscale_channels(identity.port);
                if let Some(Err(e)) = with_timeout(
                    "staged discovery",
                    STAGED_TIMEOUT,
                    discover_staged(&handles.discovery, known, identity.port),
                ).await {
                    eprintln!("[discovery] staged failed: {e:#}");
                }
                probe_tailscale(&handles.discovery, identity.port).await;
                drain_discovery(&mut disc_rx, &peers, &identity.fingerprint, &mut notices);
                let p = peers.lock().unwrap();
                save_peers(&p)?;
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nshutting down…");
                let p = peers.lock().unwrap();
                save_peers(&p)?;
                drop(p);
                if with_timeout("shutdown", SHUTDOWN_TIMEOUT, handles.shutdown()).await.is_none() {
                    eprintln!("[discovery] shutdown timed out — exiting anyway");
                }
                return Ok(());
            }
        }
    }
}

fn make_handler(
    identity: &Identity,
    peers: Arc<Mutex<Peers>>,
    plugins: Arc<Mutex<Vec<Box<dyn Plugin>>>>,
    auto_pair: bool,
) -> PacketHandler {
    let own_alias = identity.alias.clone();
    let own_fp = identity.fingerprint.clone();
    Arc::new(move |peer_fp: &str, channel: Channel, remote: &std::net::SocketAddr, pkt: NetworkPacket| {
        match pkt.kind.as_str() {
            "nb.addr" => {
                let port = pkt.body.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                if port != 0 {
                    let raw = remote.ip().to_string();
                    let host = raw.strip_prefix("::ffff:").unwrap_or(&raw).to_string();
                    let mut p = peers.lock().unwrap();
                    let alias = p
                        .peers
                        .get(peer_fp)
                        .map(|e| e.alias.clone())
                        .unwrap_or_default();
                    let changed = p
                        .peers
                        .get(peer_fp)
                        .and_then(|e| e.addrs.get(&channel))
                        .map(|a| a.host != host || a.http_port != port)
                        .unwrap_or(true);
                    if changed {
                        p.observe(peer_fp, &alias, channel, &host, port);
                        let _ = p.save();
                        println!(
                            "[addr] {} ({fp12}) via {channel} -> {host}:{port}",
                            if alias.is_empty() { "peer" } else { &alias },
                            fp12 = &peer_fp[..12.min(peer_fp.len())],
                        );
                    }
                }
                None
            }
            PAIR_TYPE => {
                let mut p = peers.lock().unwrap();
                let accepted = auto_pair;
                let alias = pkt
                    .body
                    .get("alias")
                    .and_then(|v| v.as_str())
                    .unwrap_or("peer")
                    .to_string();
                if let Some(pin) = pkt.body.get("pin").and_then(|v| v.as_str()) {
                    println!(
                        "[pair] {alias} shows PIN {pin} — verify it matches the phone screen"
                    );
                }
                if accepted {
                    let incoming = str_array(&pkt.body, "incoming");
                    let outgoing = str_array(&pkt.body, "outgoing");
                    // NOTE: do not record an address from this inbound packet
                    // connection — `remote.port()` is the client's ephemeral
                    // source port, not its HTTP port. Correct addresses arrive
                    // via register/discovery events.
                    p.mark_trusted(peer_fp, &alias, channel, incoming.clone(), outgoing.clone());
                    let _ = p.save();
                    println!("[pair] ACCEPTED {} ({fp12}) via {channel}", alias, fp12 = &peer_fp[..12.min(peer_fp.len())]);
                    Some(NetworkPacket::new(
                        PAIR_TYPE,
                        json!({
                            "protocol": nb_core::packet::PROTOCOL_VERSION,
                            "accepted": true,
                            "alias": own_alias,
                            "timestamp": nb_core::packet::now_ms(),
                            "incoming": union_incoming(&plugins.lock().unwrap()),
                            "outgoing": union_outgoing(&plugins.lock().unwrap()),
                        }),
                    ))
                } else {
                    println!("[pair] REJECTED {} (auto-pair disabled)", pkt.body.get("alias").and_then(|v| v.as_str()).unwrap_or("?"));
                    Some(NetworkPacket::new(
                        PAIR_TYPE,
                        json!({
                            "protocol": nb_core::packet::PROTOCOL_VERSION,
                            "accepted": false,
                            "alias": own_alias,
                            "reason": "auto-pair disabled on this device",
                        }),
                    ))
                }
            }
            _ => {
                let mut p = peers.lock().unwrap();
                // A one-shot `pair` command may have just written trust to
                // peers.json from another process — fold that in before the
                // trust check, or outbound pairs never take effect here.
                // Input streams hit this path at ~60 Hz, so only consult the
                // disk while trust is still missing from memory.
                let needs_trust = match p.peers.get(peer_fp) {
                    Some(d) => !d.trusted,
                    None => true,
                };
                if needs_trust {
                    let _ = p.merge_disk();
                }
                let Some((_, peer)) = p.peers.get_key_value(peer_fp) else {
                    return Some(error_packet("not paired — run `northbridge pair` first"));
                };
                if !peer.trusted {
                    return Some(error_packet("not paired — run `northbridge pair` first"));
                }
                let peer_alias = peer.alias.clone();
                let peer_in = peer.incoming_capabilities.clone();
                let peer_out = peer.outgoing_capabilities.clone();
                drop(p);

                let ctx = PluginCtx {
                    own_alias: own_alias.clone(),
                    own_fingerprint: own_fp.clone(),
                    peer_fingerprint: peer_fp.to_string(),
                    peer_alias,
                    channel,
                };
                let mut plugs = plugins.lock().unwrap();
                let active = plugins_for_capabilities(&peer_in, &peer_out, &plugs);
                // dispatch() returning None means either "no plugin owns this
                // type" (an error) or "handled, no response" (normal for
                // silent stream input) — distinguish before answering.
                let matched = active
                    .iter()
                    .any(|&i| plugs[i].incoming().contains(&pkt.kind.as_str()));
                if !matched {
                    return Some(error_packet(&format!(
                        "unknown or inactive packet type: {}",
                        pkt.kind
                    )));
                }
                dispatch(&mut plugs, &active, &ctx, &pkt)
            }
        }
    })
}

fn str_array(body: &serde_json::Value, key: &str) -> Vec<String> {
    body.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn error_packet(reason: &str) -> NetworkPacket {
    NetworkPacket::new("nb.error", json!({"reason": reason}))
}

fn save_peers(peers: &Peers) -> Result<()> {
    peers.save().context("save peers.json")
}

/// Resolve `target`, folding in peers.json for a few seconds when unknown:
/// the running `listen` instance may register the device moments after our
/// discovery pass ends (it owns the HTTP server that answers announces).
async fn resolve_with_wait<'a>(
    target: &str,
    peers: &'a mut Peers,
) -> Result<(String, nb_core::peer::Peer)> {
    const WAIT: Duration = Duration::from_secs(4);
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        match peers.resolve(target) {
            Ok((fp, p)) => return Ok((fp.clone(), p.clone())),
            Err(e) if tokio::time::Instant::now() < deadline => {
                let _ = peers.merge_disk();
                if peers.resolve(target).is_ok() {
                    let (fp, p) = peers.resolve(target)?;
                    return Ok((fp.clone(), p.clone()));
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
                if tokio::time::Instant::now() >= deadline {
                    return Err(e);
                }
            }
            Err(e) => return Err(e),
        }
    }
}

fn drain_discovery(
    rx: &mut mpsc::Receiver<DiscoveryEvent>,
    peers: &Arc<Mutex<Peers>>,
    own_fp: &str,
    notices: &mut UnreachableNotices,
) {
    while let Ok(ev) = rx.try_recv() {
        let mut p = peers.lock().unwrap();
        ingest_discovery(&mut p, own_fp, &ev, notices);
    }
}

// ---------------------------------------------------------------------------
// discover
// ---------------------------------------------------------------------------

async fn cmd_discover(cfg: &Option<PathBuf>, port: Option<u16>, alias: Option<String>) -> Result<()> {
    let identity = load_identity(cfg, alias, port)?;
    let mut peers = load_peers(cfg)?;
    // The running listen instance may already know devices we don't.
    let _ = peers.merge_disk();
    let mut notices = UnreachableNotices::new();

    let (server_tx, mut server_rx) = mpsc::channel::<ServerEventV2>(32);
    let (disc_tx, mut disc_rx) = mpsc::channel::<DiscoveryEvent>(64);
    let handles = start_ls(&identity, server_tx, disc_tx).await?;

    println!("discovering…");
    let known = tailscale_channels(identity.port);
    if let Some(Err(e)) =
        with_timeout("staged discovery", STAGED_TIMEOUT, discover_staged(&handles.discovery, known, identity.port)).await
    {
        eprintln!("[discovery] staged failed: {e:#}");
    }
    let ts = probe_tailscale(&handles.discovery, identity.port).await;
    if ts > 0 {
        println!("[tailscale] probed {ts} online peer(s)");
    }

    // Let trailing events settle, then drain.
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(ev) = disc_rx.try_recv() {
        ingest_discovery(&mut peers, &identity.fingerprint, &ev, &mut notices);
    }
    while let Ok(ev) = server_rx.try_recv() {
        if let ServerEventV2::Register { ip, info } = ev {
            ingest_register(&mut peers, &identity.fingerprint, &ip.to_string(), &info);
        }
    }

    if with_timeout("shutdown", SHUTDOWN_TIMEOUT, handles.shutdown()).await.is_none() {
        eprintln!("[discovery] shutdown timed out — exiting anyway");
    }
    save_peers(&peers)?;
    print_devices(&peers);
    Ok(())
}

fn print_devices(peers: &Peers) {
    if peers.peers.is_empty() {
        println!("no devices found — are peers running `northbridge listen`?");
        return;
    }
    println!(
        "{:<20} {:<14} {:<9} CHANNELS",
        "ALIAS", "FINGERPRINT", "TRUSTED"
    );
    for (fp, p) in &peers.peers {
        let addrs: Vec<String> = p
            .dial_order()
            .iter()
            .map(|(ch, a)| format!("{ch} {}:{})", a.host, a.http_port).replace(')', ""))
            .collect();
        println!(
            "{:<20} {:<14} {:<9} {}",
            truncate(&p.alias, 20),
            &fp[..12.min(fp.len())],
            if p.trusted { "yes" } else { "no" },
            addrs.join(", ")
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

// ---------------------------------------------------------------------------
// devices / status
// ---------------------------------------------------------------------------

fn cmd_devices(cfg: &Option<PathBuf>) -> Result<()> {
    let peers = load_peers(cfg)?;
    print_devices(&peers);
    Ok(())
}

fn cmd_status(cfg: &Option<PathBuf>) -> Result<()> {
    let identity = load_identity(cfg, None, None)?;
    let peers = load_peers(cfg)?;
    let ts_ips = nb_discovery::tailscale_peer_ips();

    println!("northbridge status");
    println!("  alias:       {}", identity.alias);
    println!("  fingerprint: {}", identity.fingerprint);
    println!("  http port:   {}   packet port: {}", identity.port, identity.packet_port());
    println!("  config:      {}", identity_dir(cfg).display());
    println!("  devices:     {} known, {} trusted", peers.peers.len(), peers.trusted_count());
    println!(
        "  tailscale:   {}",
        if ts_ips.is_empty() {
            "not available".to_string()
        } else {
            format!("{} peer IP(s)", ts_ips.len())
        }
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// pair
// ---------------------------------------------------------------------------

async fn cmd_pair(cfg: &Option<PathBuf>, target: &str) -> Result<()> {
    let identity = load_identity(cfg, None, None)?;
    let mut peers = load_peers(cfg)?;

    // Resolve; when unknown, discover first (automatic recognition).
    if peers.resolve(target).is_err() {
        println!("unknown device {target:?} — discovering first…");
        quick_discover(&identity, &mut peers).await?;
    }
    let (fp, peer) = resolve_with_wait(target, &mut peers).await?;

    let plugins = make_plugins(
        nb_clipboard::shared_state(),
        Arc::new(Mutex::new(InputState::new())),
        InputRegistry::new(),
        None,
        None,
        None,
    );
    let req = NetworkPacket::new(
        PAIR_TYPE,
        json!({
            "protocol": nb_core::packet::PROTOCOL_VERSION,
            "alias": identity.alias,
            "timestamp": nb_core::packet::now_ms(),
            "incoming": union_incoming(&plugins),
            "outgoing": union_outgoing(&plugins),
        }),
    );

    let mut last_err = None;
    for (channel, addr) in peer.dial_order() {
        let host = addr.host.clone();
        let packet_port = addr.http_port + 1;
        println!("pairing with {} via {channel} ({host}:{packet_port})…", peer.alias);
        match send_packet(&identity, &host, packet_port, &fp, &req).await {
            Ok(resp) if resp.kind == PAIR_TYPE => {
                let accepted = resp.body.get("accepted").and_then(|v| v.as_bool()).unwrap_or(false);
                if accepted {
                    let alias = resp
                        .body
                        .get("alias")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&peer.alias)
                        .to_string();
                    let incoming = str_array(&resp.body, "incoming");
                    let outgoing = str_array(&resp.body, "outgoing");
                    let http_port = addr.http_port;
                    peers.mark_trusted(&fp, &alias, channel, incoming.clone(), outgoing.clone());
                    peers.observe(&fp, &alias, channel, &host, http_port);
                    save_peers(&peers)?;
                    let active = plugins_for_capabilities(&incoming, &outgoing, &plugins);
                    println!("PAIRED with {alias} ({fp12}) via {channel}", fp12 = &fp[..12.min(fp.len())]);
                    println!(
                        "  active plugins: {}",
                        active.iter().map(|&i| plugins[i].id()).collect::<Vec<_>>().join(", ")
                    );
                    return Ok(());
                } else {
                    let reason = resp
                        .body
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("rejected");
                    anyhow::bail!("pairing rejected by {}: {reason}", peer.alias);
                }
            }
            Ok(resp) => {
                last_err = Some(format!("unexpected response type {}", resp.kind));
            }
            Err(e) => {
                last_err = Some(format!("{channel} {host}:{packet_port}: {e:#}"));
            }
        }
    }
    anyhow::bail!(
        "could not reach {} on any channel — is `northbridge listen` running there?\n  {}",
        peer.alias,
        last_err.unwrap_or_else(|| "no addresses".into())
    )
}

/// Short discovery pass used when `pair`/`ping` target is unknown.
async fn quick_discover(identity: &Identity, peers: &mut Peers) -> Result<()> {
    // The running listen instance may have learned devices since we loaded.
    let _ = peers.merge_disk();
    let mut notices = UnreachableNotices::new();

    let (server_tx, mut server_rx) = mpsc::channel::<ServerEventV2>(32);
    let (disc_tx, mut disc_rx) = mpsc::channel::<DiscoveryEvent>(64);
    let handles = start_ls(identity, server_tx, disc_tx).await?;

    let known = tailscale_channels(identity.port);
    if let Some(Err(e)) =
        with_timeout("staged discovery", STAGED_TIMEOUT, discover_staged(&handles.discovery, known, identity.port)).await
    {
        eprintln!("[discovery] staged failed: {e:#}");
    }
    probe_tailscale(&handles.discovery, identity.port).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    while let Ok(ev) = disc_rx.try_recv() {
        ingest_discovery(peers, &identity.fingerprint, &ev, &mut notices);
    }
    while let Ok(ev) = server_rx.try_recv() {
        if let ServerEventV2::Register { ip, info } = ev {
            ingest_register(peers, &identity.fingerprint, &ip.to_string(), &info);
        }
    }
    if with_timeout("shutdown", SHUTDOWN_TIMEOUT, handles.shutdown()).await.is_none() {
        eprintln!("[discovery] shutdown timed out — continuing anyway");
    }
    save_peers(peers)?;
    // The running listen instance may have registered the target to
    // peers.json while we were probing (it owns the HTTP server) — fold
    // that in, or resolve() below still sees our stale snapshot.
    let _ = peers.merge_disk();
    Ok(())
}

// ---------------------------------------------------------------------------
// ping
// ---------------------------------------------------------------------------

async fn cmd_ping(cfg: &Option<PathBuf>, target: &str) -> Result<()> {
    let identity = load_identity(cfg, None, None)?;
    let mut peers = load_peers(cfg)?;

    if peers.resolve(target).is_err() {
        println!("unknown device {target:?} — discovering first…");
        quick_discover(&identity, &mut peers).await?;
    }
    let (fp, peer) = resolve_with_wait(target, &mut peers).await?;
    if !peer.trusted {
        anyhow::bail!(
            "{} is not paired yet — run `northbridge pair {target}` first",
            peer.alias
        );
    }

    let req = NetworkPacket::new("nb.ping", json!({"message": "ping"}));
    let mut last_err = None;
    for (channel, addr) in peer.dial_order() {
        let host = addr.host.clone();
        let packet_port = addr.http_port + 1;
        let started = std::time::Instant::now();
        match send_packet(&identity, &host, packet_port, &fp, &req).await {
            Ok(resp) if resp.kind == "nb.pong" => {
                let rtt = started.elapsed();
                let reply = resp.body.get("reply").and_then(|v| v.as_str()).unwrap_or("?");
                let via = resp.body.get("channel").and_then(|v| v.as_str()).unwrap_or("?");
                println!(
                    "pong from {reply} ({fp12}) via {via} — {rtt:.1?}",
                    fp12 = &fp[..12.min(fp.len())]
                );
                // Refresh the address we just used successfully.
                peers.observe(&fp, &peer.alias, channel, &host, addr.http_port);
                save_peers(&peers)?;
                return Ok(());
            }
            Ok(resp) => last_err = Some(format!("unexpected response: {} {}", resp.kind, resp.body)),
            Err(e) => last_err = Some(format!("{channel} {host}:{packet_port}: {e:#}")),
        }
    }
    anyhow::bail!(
        "ping failed for {}: {}",
        peer.alias,
        last_err.unwrap_or_else(|| "no addresses".into())
    )
}

// ---------------------------------------------------------------------------
// send (file transfer, LocalSend HTTP API)
// ---------------------------------------------------------------------------

async fn cmd_send(cfg: &Option<PathBuf>, target: &str, files: Vec<PathBuf>) -> Result<()> {
    let identity = load_identity(cfg, None, None)?;
    let mut peers = load_peers(cfg)?;

    if peers.resolve(target).is_err() {
        println!("unknown device {target:?} — discovering first…");
        quick_discover(&identity, &mut peers).await?;
    }
    let (fp, peer) = resolve_with_wait(target, &mut peers).await?;

    for f in &files {
        if !f.is_file() {
            anyhow::bail!("not a readable file: {}", f.display());
        }
    }

    let mut last_err = None;
    for (channel, addr) in peer.dial_order() {
        let host = addr.host.clone();
        println!(
            "sending {} file(s) to {} via {channel} ({host}:{})…",
            files.len(),
            peer.alias,
            addr.http_port
        );
        match send_files(&identity, &host, addr.http_port, &fp, &files).await {
            Ok(sent) => {
                for name in &sent {
                    println!("  sent {name}");
                }
                peers.observe(&fp, &peer.alias, channel, &host, addr.http_port);
                save_peers(&peers)?;
                return Ok(());
            }
            Err(e) => last_err = Some(format!("{channel} {host}: {e:#}")),
        }
    }
    anyhow::bail!(
        "send to {} failed:\n  {}",
        peer.alias,
        last_err.unwrap_or_else(|| "no addresses".into())
    )
}

// ---------------------------------------------------------------------------
// clipboard watcher (listen only)
// ---------------------------------------------------------------------------

/// Poll the local clipboard once a second; when it changes, push the text to
/// every paired peer over the packet channel. The shared `ClipboardState`
/// suppresses echoes of content we just received.
fn spawn_clipboard_watcher(identity: Identity, peers: Arc<Mutex<Peers>>, clip: SharedState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // First tick fires immediately; skip it so startup does not push
        // whatever was already in the clipboard.
        tick.tick().await;
        loop {
            tick.tick().await;
            let Some(content) = nb_clipboard::read() else {
                // No clipboard backend (headless) — retry quietly.
                continue;
            };
            let push = {
                let mut st = clip.lock().unwrap();
                if content == st.last || content.is_empty() {
                    false
                } else {
                    st.last = content.clone();
                    true
                }
            };
            if !push {
                continue;
            }
            // Snapshot paired peers with a clipboard-capable remote.
            let targets: Vec<(String, nb_core::peer::Peer)> = {
                let p = peers.lock().unwrap();
                p.peers
                    .iter()
                    .filter(|(_, d)| d.trusted)
                    .map(|(fp, d)| (fp.clone(), d.clone()))
                    .collect()
            };
            if targets.is_empty() {
                continue;
            }
            let preview: String = content.chars().take(40).collect();
            println!("[clipboard] local change → pushing to {} peer(s): {preview}", targets.len());
            for (fp, dev) in targets {
                // Empty remote caps = legacy peer that loads everything (KC rule).
                let capable = dev.incoming_capabilities.is_empty()
                    || dev
                        .incoming_capabilities
                        .iter()
                        .any(|c| c == "nb.clipboard");
                if !capable {
                    continue;
                }
                for (_channel, addr) in dev.dial_order() {
                    let pkt = NetworkPacket::new(
                        "nb.clipboard",
                        json!({"content": content, "ts": nb_core::packet::now_ms()}),
                    );
                    match send_packet(&identity, &addr.host, addr.http_port + 1, &fp, &pkt).await
                    {
                        Ok(resp) if resp.body.get("ack").and_then(|v| v.as_bool()) == Some(true) => {
                            break; // delivered on this peer
                        }
                        Ok(_) | Err(_) => continue, // try next channel
                    }
                }
            }
        }
    });
}

/// Receive directory: `$NORTHBRIDGE_DOWNLOAD_DIR` or `~/Downloads/northbridge`.
/// Falls back to `<config>/downloads` when the preferred path cannot be
/// created (e.g. `~/Downloads` exists as a file).
fn download_dir() -> PathBuf {
    let candidates = [
        std::env::var_os("NORTHBRIDGE_DOWNLOAD_DIR").map(PathBuf::from),
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join("Downloads").join("northbridge")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if std::fs::create_dir_all(&candidate).is_ok() {
            return candidate;
        }
    }
    let fallback = config_dir().join("downloads");
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

/// `dir/name`, de-duplicated to `name (1).ext`, `name (2).ext`, …
fn unique_download_path(dir: &Path, file_name: &str) -> PathBuf {
    // Strip any directory components a hostile sender might embed.
    let base = Path::new(file_name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty() && n != "." && n != "..")
        .unwrap_or_else(|| "download".to_string());
    let candidate = dir.join(&base);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(&base)
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| base.clone());
    let ext = Path::new(&base)
        .extension()
        .map(|n| n.to_string_lossy().into_owned());
    for i in 1..1000 {
        let name = match &ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}", nb_core::packet::now_ms()))
}
