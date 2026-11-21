use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use localsend::discovery::DiscoveryEvent;
use localsend::http::server::common::save::FileUploadTarget;
use localsend::http::server::v2::{PrepareUploadDecisionV2, ServerEventV2};
use nb_core::channel::Channel;
use nb_core::identity::{Identity, config_dir};
use nb_core::packet::{NetworkPacket, PROTOCOL_VERSION, now_ms};
use nb_core::peer::{Peer, Peers};
use nb_core::plugin::{
    Plugin, PluginCtx, dispatch, plugins_for_capabilities, union_incoming, union_outgoing,
};
use nb_discovery::{discover_staged, probe_tailscale, tailscale_channels};
use nb_transport::ls::{
    LsHandles, UnreachableNotices, ingest_discovery, ingest_register, start_ls,
};
use nb_transport::{InputRegistry, PacketHandler, send_files, send_packet, start_packet_server};
use serde_json::json;
use tokio::sync::oneshot;

use crate::model::{
    ChannelView, DeviceView, GuiCmd, InboundPair, OpKind, SelfInfo, UiEvent, fp_short, sort_devices,
};

const PAIR_TYPE: &str = "nb.pair";
const STAGE_INTERVAL: Duration = Duration::from_secs(15);
const STAGED_TIMEOUT: Duration = Duration::from_secs(20);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Backend {
    pub cmd_tx: mpsc::UnboundedSender<GuiCmd>,
}

impl Backend {
    pub fn spawn(event_tx: std::sync::mpsc::Sender<UiEvent>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("gui tokio runtime");
            rt.block_on(run(cmd_rx, event_tx));
        });
        Self { cmd_tx }
    }
}

struct Shared {
    identity: Identity,
    peers: Mutex<Peers>,
    auto_pair: AtomicBool,
    events: Mutex<std::sync::mpsc::Sender<UiEvent>>,
    clip: nb_clipboard::SharedState,
    plugins: Mutex<Vec<Box<dyn Plugin>>>,
    incoming: Vec<String>,
    outgoing: Vec<String>,
}

fn emit(shared: &Shared, ev: UiEvent) {
    let _ = shared.events.lock().unwrap().send(ev);
}

fn emit_log(shared: &Shared, line: String) {
    emit(shared, UiEvent::Log(line));
}

fn push_snapshot(shared: &Shared) {
    let views = snapshot(&shared.peers.lock().unwrap());
    emit(shared, UiEvent::Peers(views));
}

fn snapshot(peers: &Peers) -> Vec<DeviceView> {
    let mut out: Vec<DeviceView> = peers
        .peers
        .iter()
        .map(|(fp, p)| DeviceView {
            fingerprint: fp.clone(),
            alias: p.alias.clone(),
            trusted: p.trusted,
            channels: p
                .dial_order()
                .into_iter()
                .map(|(ch, a)| ChannelView {
                    channel: ch.to_string(),
                    host: a.host.clone(),
                    http_port: a.http_port,
                    last_seen: a.last_seen,
                })
                .collect(),
            paired_via: p.paired_via.map(|c| c.to_string()),
        })
        .collect();
    sort_devices(&mut out);
    out
}

fn make_plugins(clip: nb_clipboard::SharedState) -> Vec<Box<dyn Plugin>> {
    vec![
        Box::new(nb_ping::PingPlugin),
        Box::new(nb_clipboard::ClipboardPlugin::new(clip)),
    ]
}

fn str_array(body: &serde_json::Value, key: &str) -> Vec<String> {
    body.get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn error_packet(reason: &str) -> NetworkPacket {
    NetworkPacket::new("nb.error", json!({"reason": reason}))
}

fn peers_path() -> PathBuf {
    config_dir().join("peers.json")
}

fn save_shared(shared: &Shared) {
    if let Err(e) = shared.peers.lock().unwrap().save() {
        emit_log(shared, format!("[peers] save failed: {e:#}"));
    }
}

fn make_handler(shared: Arc<Shared>) -> PacketHandler {
    let own_alias = shared.identity.alias.clone();
    let own_fp = shared.identity.fingerprint.clone();
    Arc::new(move |peer_fp: &str, channel: Channel, remote: &std::net::SocketAddr, pkt: NetworkPacket| {
        match pkt.kind.as_str() {
            "nb.addr" => {
                let port = pkt.body.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                if port != 0 {
                    let raw = remote.ip().to_string();
                    let host = raw.strip_prefix("::ffff::").unwrap_or(&raw).to_string();
                    let mut p = shared.peers.lock().unwrap();
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
                        let show = if alias.is_empty() { "peer" } else { &alias };
                        emit_log(
                            &shared,
                            format!(
                                "[addr] {show} ({}) via {channel} -> {host}:{port}",
                                fp_short(peer_fp)
                            ),
                        );
                        push_snapshot(&shared);
                    }
                }
                None
            }
            PAIR_TYPE => {
                let alias = pkt
                    .body
                    .get("alias")
                    .and_then(|v| v.as_str())
                    .unwrap_or("peer")
                    .to_string();
                let pin = pkt.body.get("pin").and_then(|v| v.as_str()).map(String::from);
                let accepted = shared.auto_pair.load(Ordering::SeqCst);
                if let Some(ref pin) = pin {
                    emit_log(
                        &shared,
                        format!("[pair] {alias} shows PIN {pin} — verify it matches the phone screen"),
                    );
                }
                if accepted {
                    let incoming = str_array(&pkt.body, "incoming");
                    let outgoing = str_array(&pkt.body, "outgoing");
                    {
                        let mut p = shared.peers.lock().unwrap();
                        p.mark_trusted(peer_fp, &alias, channel, incoming.clone(), outgoing.clone());
                        let _ = p.save();
                    }
                    emit_log(
                        &shared,
                        format!("[pair] ACCEPTED {alias} ({}) via {channel}", fp_short(peer_fp)),
                    );
                    emit(
                        &shared,
                        UiEvent::InboundPair(InboundPair {
                            alias: alias.clone(),
                            fingerprint: peer_fp.to_string(),
                            pin,
                            channel: channel.to_string(),
                            accepted: true,
                        }),
                    );
                    push_snapshot(&shared);
                    Some(NetworkPacket::new(
                        PAIR_TYPE,
                        json!({
                            "protocol": PROTOCOL_VERSION,
                            "accepted": true,
                            "alias": own_alias,
                            "timestamp": now_ms(),
                            "incoming": shared.incoming,
                            "outgoing": shared.outgoing,
                        }),
                    ))
                } else {
                    emit_log(&shared, format!("[pair] REJECTED {alias} (auto-pair disabled)"));
                    emit(
                        &shared,
                        UiEvent::InboundPair(InboundPair {
                            alias,
                            fingerprint: peer_fp.to_string(),
                            pin,
                            channel: channel.to_string(),
                            accepted: false,
                        }),
                    );
                    Some(NetworkPacket::new(
                        PAIR_TYPE,
                        json!({
                            "protocol": PROTOCOL_VERSION,
                            "accepted": false,
                            "alias": own_alias,
                            "reason": "auto-pair disabled on this device",
                        }),
                    ))
                }
            }
            _ => {
                let mut p = shared.peers.lock().unwrap();
                let needs_trust = match p.peers.get(peer_fp) {
                    Some(d) => !d.trusted,
                    None => true,
                };
                if needs_trust {
                    let _ = p.merge_disk();
                }
                let Some((_, peer)) = p.peers.get_key_value(peer_fp) else {
                    return Some(error_packet("not paired — run pair first"));
                };
                if !peer.trusted {
                    return Some(error_packet("not paired — run pair first"));
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
                let mut plugs = shared.plugins.lock().unwrap();
                let active = plugins_for_capabilities(&peer_in, &peer_out, &plugs);
                let matched = active
                    .iter()
                    .any(|&i| plugs[i].incoming().contains(&pkt.kind.as_str()));
                if !matched {
                    return Some(error_packet(&format!(
                        "unknown or inactive packet type: {}",
                        pkt.kind
                    )));
                }
                let resp = dispatch(&mut plugs, &active, &ctx, &pkt);
                if pkt.kind == "nb.clipboard" {
                    if let Some(ref r) = resp {
                        let ok = r.body.get("ack").and_then(|v| v.as_bool()).unwrap_or(false);
                        emit_log(
                            &shared,
                            format!(
                                "[clipboard] inbound from {} ({}): {}",
                                ctx.peer_alias,
                                fp_short(peer_fp),
                                if ok { "applied" } else { "no backend" }
                            ),
                        );
                    }
                }
                resp
            }
        }
    })
}

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

fn unique_download_path(dir: &Path, file_name: &str) -> PathBuf {
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
    dir.join(format!("{stem}-{}", now_ms()))
}

struct Active {
    ls: LsHandles,
    server_rx: mpsc::Receiver<ServerEventV2>,
    disc_rx: mpsc::Receiver<DiscoveryEvent>,
    packet: Option<JoinHandle<()>>,
    watcher: JoinHandle<()>,
    notices: UnreachableNotices,
    tick: tokio::time::Interval,
}

async fn start_all(shared: &Arc<Shared>) -> Active {
    let (server_tx, server_rx) = mpsc::channel::<ServerEventV2>(32);
    let (disc_tx, disc_rx) = mpsc::channel::<DiscoveryEvent>(64);
    let ls = match start_ls(&shared.identity, server_tx, disc_tx).await {
        Ok(h) => h,
        Err(e) => {
            emit_log(shared, format!("[server] start failed: {e:#} — retrying"));
            let (server_tx, server_rx) = mpsc::channel::<ServerEventV2>(32);
            let (disc_tx, disc_rx) = mpsc::channel::<DiscoveryEvent>(64);
            let ls = start_ls(&shared.identity, server_tx, disc_tx).await;
            match ls {
                Ok(h) => {
                    return finish_start(shared, h, server_rx, disc_rx, None).await;
                }
                Err(e) => {
                    emit_log(shared, format!("[server] fatal: {e:#}"));
                    std::process::exit(1);
                }
            }
        }
    };
    let probe = tokio::net::TcpListener::bind(("0.0.0.0", shared.identity.packet_port())).await;
    let packet = match probe {
        Ok(l) => {
            drop(l);
            let handler = make_handler(Arc::clone(shared));
            let packet_identity = shared.identity.clone();
            let registry = InputRegistry::new();
            let port = shared.identity.packet_port();
            Some(tokio::spawn(async move {
                emit_line(&packet_identity, &handler, &registry, port).await;
            }))
        }
        Err(_) => {
            emit_log(
                shared,
                format!(
                    "[packet] :{} in use — a `northbridge listen` instance is running; outbound only",
                    shared.identity.packet_port()
                ),
            );
            None
        }
    };
    finish_start(shared, ls, server_rx, disc_rx, packet).await
}

async fn emit_line(identity: &Identity, handler: &PacketHandler, registry: &InputRegistry, _port: u16) {
    if let Err(e) = start_packet_server(identity.clone(), handler.clone(), registry.clone()).await {
        eprintln!("[packet] server stopped: {e:#}");
    }
}

async fn finish_start(
    shared: &Arc<Shared>,
    ls: LsHandles,
    server_rx: mpsc::Receiver<ServerEventV2>,
    disc_rx: mpsc::Receiver<DiscoveryEvent>,
    packet: Option<JoinHandle<()>>,
) -> Active {
    if ls.server.is_none() {
        emit_log(
            shared,
            format!(
                "[server] http :{} in use — discovery-only mode (a `listen` instance is running)",
                shared.identity.port
            ),
        );
    }
    let watcher = spawn_clipboard_watcher(Arc::clone(shared));
    Active {
        ls,
        server_rx,
        disc_rx,
        packet,
        watcher,
        notices: UnreachableNotices::new(),
        tick: staged_tick(),
    }
}

fn staged_tick() -> tokio::time::Interval {
    let mut tick = tokio::time::interval(STAGE_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tick
}

fn spawn_clipboard_watcher(shared: Arc<Shared>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        tick.tick().await;
        loop {
            tick.tick().await;
            let Some(content) = nb_clipboard::read() else {
                continue;
            };
            let push = {
                let mut st = shared.clip.lock().unwrap();
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
            let targets: Vec<(String, Peer)> = {
                let p = shared.peers.lock().unwrap();
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
            emit_log(
                &shared,
                format!("[clipboard] local change → pushing to {} peer(s): {preview}", targets.len()),
            );
            for (fp, dev) in targets {
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
                        json!({"content": content, "ts": now_ms()}),
                    );
                    match send_packet(&shared.identity, &addr.host, addr.http_port + 1, &fp, &pkt).await {
                        Ok(resp)
                            if resp.body.get("ack").and_then(|v| v.as_bool()) == Some(true) =>
                        {
                            break;
                        }
                        Ok(_) | Err(_) => continue,
                    }
                }
            }
        }
    })
}

async fn with_timeout<T>(
    what: &str,
    limit: Duration,
    shared: &Shared,
    f: impl std::future::Future<Output = T>,
) -> Option<T> {
    match tokio::time::timeout(limit, f).await {
        Ok(v) => Some(v),
        Err(_) => {
            emit_log(shared, format!("[discovery] {what} timed out after {limit:?}"));
            None
        }
    }
}

async fn staged_pass(shared: &Arc<Shared>, active: &mut Active) {
    let known = tailscale_channels(shared.identity.port);
    if let Some(Err(e)) = with_timeout(
        "staged discovery",
        STAGED_TIMEOUT,
        shared,
        discover_staged(&active.ls.discovery, known, shared.identity.port),
    )
    .await
    {
        emit_log(shared, format!("[discovery] staged failed: {e:#}"));
    }
    let n = probe_tailscale(&active.ls.discovery, shared.identity.port).await;
    if n > 0 {
        emit_log(shared, format!("[tailscale] probed {n} online peer(s)"));
    }
    drain_all(shared, active);
}

fn drain_all(shared: &Arc<Shared>, active: &mut Active) {
    let own_fp = shared.identity.fingerprint.clone();
    let mut fresh = false;
    while let Ok(ev) = active.disc_rx.try_recv() {
        let is_fresh = {
            let mut p = shared.peers.lock().unwrap();
            ingest_discovery(&mut p, &own_fp, &ev, &mut active.notices)
        };
        if is_fresh {
            fresh = true;
            if let DiscoveryEvent::Discovered { device } | DiscoveryEvent::Updated { device } = &ev {
                let ch = device
                    .channel
                    .http()
                    .map(|h| Channel::classify(&h.host))
                    .unwrap_or(Channel::Unknown);
                emit_log(
                    shared,
                    format!(
                        "[discover] NEW {} ({ch}) — {}",
                        device.alias,
                        fp_short(&device.fingerprint)
                    ),
                );
            }
        }
    }
    while let Ok(ev) = active.server_rx.try_recv() {
        handle_server_event(shared, ev);
        fresh = true;
    }
    if fresh {
        save_shared(shared);
        push_snapshot(shared);
    }
}

fn handle_server_event(shared: &Arc<Shared>, ev: ServerEventV2) {
    match ev {
        ServerEventV2::Register { ip, info } => {
            let fresh = {
                let mut p = shared.peers.lock().unwrap();
                ingest_register(&mut p, &shared.identity.fingerprint, &ip.to_string(), &info)
            };
            if fresh {
                let ch = Channel::classify(&ip.to_string());
                emit_log(
                    shared,
                    format!(
                        "[register] NEW {} ({ch}) — {}",
                        info.alias,
                        fp_short(&info.fingerprint)
                    ),
                );
            }
        }
        ServerEventV2::PrepareUpload {
            info,
            cert_fingerprint,
            decision_tx,
            files,
            ..
        } => {
            let trusted = cert_fingerprint
                .as_deref()
                .map(|fp| {
                    shared
                        .peers
                        .lock()
                        .unwrap()
                        .peers
                        .get(fp)
                        .map(|d| d.trusted)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if trusted {
                let ids: HashSet<String> = files.keys().cloned().collect();
                emit_log(
                    shared,
                    format!("[transfer] {} sends {} file(s) — accepting", info.alias, ids.len()),
                );
                let _ = decision_tx.send(PrepareUploadDecisionV2::Accept(ids));
            } else {
                emit_log(
                    shared,
                    format!("[transfer] {} (not paired) offers files — declined", info.alias),
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
            let tx = shared.events.lock().unwrap().clone();
            tokio::spawn(async move {
                let line = match result_rx.await {
                    Ok(Ok(())) => format!("[transfer] saved {}", display.display()),
                    Ok(Err(e)) => format!("[transfer] failed {}: {e}", display.display()),
                    Err(_) => format!("[transfer] interrupted {}", display.display()),
                };
                let _ = tx.send(UiEvent::Log(line));
            });
        }
        ServerEventV2::ListenerFailed { error } => {
            emit_log(shared, format!("[server] {error}"));
        }
        _ => {}
    }
}

fn lookup_peer(shared: &Arc<Shared>, fp: &str) -> Option<(String, Peer)> {
    let mut p = shared.peers.lock().unwrap();
    let _ = p.merge_disk();
    p.peers
        .get(fp)
        .map(|d| (fp.to_string(), d.clone()))
}

fn do_pair(shared: Arc<Shared>, fp: String) {
    tokio::spawn(async move {
        let target = fp_short(&fp);
        let Some((fp, peer)) = lookup_peer(&shared, &fp) else {
            emit(
                &shared,
                UiEvent::OpResult {
                    op: OpKind::Pair,
                    target,
                    ok: false,
                    detail: "device vanished from the store".to_string(),
                },
            );
            return;
        };
        let req = NetworkPacket::new(
            PAIR_TYPE,
            json!({
                "protocol": PROTOCOL_VERSION,
                "alias": shared.identity.alias,
                "timestamp": now_ms(),
                "incoming": shared.incoming,
                "outgoing": shared.outgoing,
            }),
        );
        let mut last_err: Option<String> = None;
        for (channel, addr) in peer.dial_order() {
            let host = addr.host.clone();
            let packet_port = addr.http_port + 1;
            emit_log(
                &shared,
                format!("pairing with {} via {channel} ({host}:{packet_port})…", peer.alias),
            );
            match tokio::time::timeout(
                DIAL_TIMEOUT,
                send_packet(&shared.identity, &host, packet_port, &fp, &req),
            )
            .await
            {
                Ok(Ok(resp)) if resp.kind == PAIR_TYPE => {
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
                        {
                            let mut p = shared.peers.lock().unwrap();
                            p.mark_trusted(&fp, &alias, channel, incoming.clone(), outgoing.clone());
                            p.observe(&fp, &alias, channel, &host, http_port);
                            let _ = p.save();
                        }
                        let plugs = shared.plugins.lock().unwrap();
                        let active = plugins_for_capabilities(&incoming, &outgoing, &plugs);
                        let names: Vec<String> = active.iter().map(|&i| plugs[i].id().to_string()).collect();
                        drop(plugs);
                        emit_log(
                            &shared,
                            format!("PAIRED with {alias} ({}) via {channel}", fp_short(&fp)),
                        );
                        push_snapshot(&shared);
                        emit(
                            &shared,
                            UiEvent::OpResult {
                                op: OpKind::Pair,
                                target: alias,
                                ok: true,
                                detail: format!("via {channel}; plugins: {}", names.join(", ")),
                            },
                        );
                        return;
                    } else {
                        let reason = resp
                            .body
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("rejected");
                        emit(
                            &shared,
                            UiEvent::OpResult {
                                op: OpKind::Pair,
                                target: peer.alias.clone(),
                                ok: false,
                                detail: format!("rejected by {}: {reason}", peer.alias),
                            },
                        );
                        return;
                    }
                }
                Ok(Ok(resp)) => {
                    last_err = Some(format!("unexpected response type {}", resp.kind));
                }
                Ok(Err(e)) => {
                    last_err = Some(format!("{channel} {host}:{packet_port}: {e:#}"));
                }
                Err(_) => {
                    last_err = Some(format!("{channel} {host}:{packet_port}: dial timed out"));
                }
            }
        }
        emit(
            &shared,
            UiEvent::OpResult {
                op: OpKind::Pair,
                target: peer.alias.clone(),
                ok: false,
                detail: last_err.unwrap_or_else(|| "no addresses".to_string()),
            },
        );
    });
}

fn do_ping(shared: Arc<Shared>, fp: String) {
    tokio::spawn(async move {
        let target = fp_short(&fp);
        let Some((fp, peer)) = lookup_peer(&shared, &fp) else {
            emit(
                &shared,
                UiEvent::OpResult {
                    op: OpKind::Ping,
                    target,
                    ok: false,
                    detail: "device vanished from the store".to_string(),
                },
            );
            return;
        };
        if !peer.trusted {
            emit(
                &shared,
                UiEvent::OpResult {
                    op: OpKind::Ping,
                    target: peer.alias.clone(),
                    ok: false,
                    detail: "not paired yet — pair first".to_string(),
                },
            );
            return;
        }
        let req = NetworkPacket::new("nb.ping", json!({"message": "ping"}));
        let mut last_err: Option<String> = None;
        for (channel, addr) in peer.dial_order() {
            let host = addr.host.clone();
            let packet_port = addr.http_port + 1;
            let started = std::time::Instant::now();
            match tokio::time::timeout(
                DIAL_TIMEOUT,
                send_packet(&shared.identity, &host, packet_port, &fp, &req),
            )
            .await
            {
                Ok(Ok(resp)) if resp.kind == "nb.pong" => {
                    let rtt = started.elapsed();
                    let reply = resp.body.get("reply").and_then(|v| v.as_str()).unwrap_or("?");
                    let via = resp.body.get("channel").and_then(|v| v.as_str()).unwrap_or("?");
                    {
                        let mut p = shared.peers.lock().unwrap();
                        p.observe(&fp, &peer.alias, channel, &host, addr.http_port);
                        let _ = p.save();
                    }
                    let detail = format!("pong from {reply} via {via} — {rtt:.1?}");
                    emit_log(&shared, format!("[ping] {detail}"));
                    push_snapshot(&shared);
                    emit(
                        &shared,
                        UiEvent::OpResult {
                            op: OpKind::Ping,
                            target: peer.alias.clone(),
                            ok: true,
                            detail,
                        },
                    );
                    return;
                }
                Ok(Ok(resp)) => {
                    last_err = Some(format!("unexpected response: {} {}", resp.kind, resp.body));
                }
                Ok(Err(e)) => {
                    last_err = Some(format!("{channel} {host}:{packet_port}: {e:#}"));
                }
                Err(_) => {
                    last_err = Some(format!("{channel} {host}:{packet_port}: timed out"));
                }
            }
        }
        emit(
            &shared,
            UiEvent::OpResult {
                op: OpKind::Ping,
                target: peer.alias.clone(),
                ok: false,
                detail: last_err.unwrap_or_else(|| "no addresses".to_string()),
            },
        );
    });
}

fn do_send(shared: Arc<Shared>, fp: String, paths: Vec<String>) {
    tokio::spawn(async move {
        let target = fp_short(&fp);
        let Some((fp, peer)) = lookup_peer(&shared, &fp) else {
            emit(
                &shared,
                UiEvent::OpResult {
                    op: OpKind::Send,
                    target,
                    ok: false,
                    detail: "device vanished from the store".to_string(),
                },
            );
            return;
        };
        let files: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        for f in &files {
            if !f.is_file() {
                emit(
                    &shared,
                    UiEvent::OpResult {
                        op: OpKind::Send,
                        target: peer.alias.clone(),
                        ok: false,
                        detail: format!("not a readable file: {}", f.display()),
                    },
                );
                return;
            }
        }
        let mut last_err: Option<String> = None;
        for (channel, addr) in peer.dial_order() {
            let host = addr.host.clone();
            emit_log(
                &shared,
                format!(
                    "sending {} file(s) to {} via {channel} ({host}:{})…",
                    files.len(),
                    peer.alias,
                    addr.http_port
                ),
            );
            match tokio::time::timeout(
                Duration::from_secs(30),
                send_files(&shared.identity, &host, addr.http_port, &fp, &files),
            )
            .await
            {
                Ok(Ok(sent)) => {
                    {
                        let mut p = shared.peers.lock().unwrap();
                        p.observe(&fp, &peer.alias, channel, &host, addr.http_port);
                        let _ = p.save();
                    }
                    let detail = format!("sent {} via {channel}", sent.join(", "));
                    emit_log(&shared, format!("[send] {detail}"));
                    push_snapshot(&shared);
                    emit(
                        &shared,
                        UiEvent::OpResult {
                            op: OpKind::Send,
                            target: peer.alias.clone(),
                            ok: true,
                            detail,
                        },
                    );
                    return;
                }
                Ok(Err(e)) => {
                    last_err = Some(format!("{channel} {host}: {e:#}"));
                }
                Err(_) => {
                    last_err = Some(format!("{channel} {host}: timed out"));
                }
            }
        }
        emit(
            &shared,
            UiEvent::OpResult {
                op: OpKind::Send,
                target: peer.alias.clone(),
                ok: false,
                detail: last_err.unwrap_or_else(|| "no addresses".to_string()),
            },
        );
    });
}

fn do_push_clipboard(shared: Arc<Shared>, fp: String, text: String) {
    tokio::spawn(async move {
        let target = fp_short(&fp);
        let Some((fp, peer)) = lookup_peer(&shared, &fp) else {
            emit(
                &shared,
                UiEvent::OpResult {
                    op: OpKind::Clipboard,
                    target,
                    ok: false,
                    detail: "device vanished from the store".to_string(),
                },
            );
            return;
        };
        if !peer.trusted {
            emit(
                &shared,
                UiEvent::OpResult {
                    op: OpKind::Clipboard,
                    target: peer.alias.clone(),
                    ok: false,
                    detail: "not paired yet — pair first".to_string(),
                },
            );
            return;
        }
        {
            let mut st = shared.clip.lock().unwrap();
            st.last = text.clone();
        }
        let mut last_err: Option<String> = None;
        for (channel, addr) in peer.dial_order() {
            let pkt = NetworkPacket::new("nb.clipboard", json!({"content": text, "ts": now_ms()}));
            match tokio::time::timeout(
                DIAL_TIMEOUT,
                send_packet(&shared.identity, &addr.host, addr.http_port + 1, &fp, &pkt),
            )
            .await
            {
                Ok(Ok(resp)) if resp.body.get("ack").and_then(|v| v.as_bool()) == Some(true) => {
                    let detail = format!("pushed {} chars via {channel}", text.chars().count());
                    emit_log(&shared, format!("[clipboard] {detail}"));
                    emit(
                        &shared,
                        UiEvent::OpResult {
                            op: OpKind::Clipboard,
                            target: peer.alias.clone(),
                            ok: true,
                            detail,
                        },
                    );
                    return;
                }
                Ok(Ok(_)) | Ok(Err(_)) => {
                    last_err = Some(format!("{channel}: no ack, trying next"));
                    continue;
                }
                Err(_) => {
                    last_err = Some(format!("{channel}: timed out"));
                }
            }
        }
        emit(
            &shared,
            UiEvent::OpResult {
                op: OpKind::Clipboard,
                target: peer.alias.clone(),
                ok: false,
                detail: last_err.unwrap_or_else(|| "no addresses".to_string()),
            },
        );
    });
}

async fn run(mut cmd_rx: mpsc::UnboundedReceiver<GuiCmd>, event_tx: std::sync::mpsc::Sender<UiEvent>) {
    let identity = match Identity::load_or_generate(&config_dir(), None, None) {
        Ok(id) => id,
        Err(e) => {
            let _ = event_tx.send(UiEvent::Log(format!("[identity] fatal: {e:#}")));
            return;
        }
    };
    let peers = Peers::load(&peers_path()).unwrap_or_default();
    let clip = nb_clipboard::shared_state();
    let plugins = make_plugins(Arc::clone(&clip));
    let incoming = union_incoming(&plugins);
    let outgoing = union_outgoing(&plugins);
    let shared = Arc::new(Shared {
        identity,
        peers: Mutex::new(peers),
        auto_pair: AtomicBool::new(true),
        events: Mutex::new(event_tx.clone()),
        clip,
        plugins: Mutex::new(plugins),
        incoming,
        outgoing,
    });

    let _ = event_tx.send(UiEvent::SelfInfo(SelfInfo {
        alias: shared.identity.alias.clone(),
        fingerprint: shared.identity.fingerprint.clone(),
        http_port: shared.identity.port,
        packet_port: shared.identity.packet_port(),
    }));
    emit_log(
        &shared,
        format!(
            "northbridge gui — {} ({}) http :{} packet :{}",
            shared.identity.alias,
            fp_short(&shared.identity.fingerprint),
            shared.identity.port,
            shared.identity.packet_port()
        ),
    );

    let mut active = Some(start_all(&shared).await);
    let _ = event_tx.send(UiEvent::DaemonStatus { running: true });
    if let Some(a) = active.as_mut() {
        staged_pass(&shared, a).await;
        emit_log(&shared, "listening — discovery running".to_string());
    }
    push_snapshot(&shared);

    loop {
        let cmd = match active.as_mut() {
            Some(a) => {
                tokio::select! {
                    biased;
                    Some(cmd) = cmd_rx.recv() => Some(cmd),
                    Some(ev) = a.server_rx.recv() => {
                        handle_server_event(&shared, ev);
                        save_shared(&shared);
                        push_snapshot(&shared);
                        None
                    }
                    Some(ev) = a.disc_rx.recv() => {
                        let own = shared.identity.fingerprint.clone();
                        let fresh = {
                            let mut p = shared.peers.lock().unwrap();
                            ingest_discovery(&mut p, &own, &ev, &mut a.notices)
                        };
                        if fresh {
                            save_shared(&shared);
                            push_snapshot(&shared);
                        }
                        None
                    }
                    _ = a.tick.tick() => {
                        staged_pass(&shared, a).await;
                        None
                    }
                }
            }
            None => cmd_rx.recv().await.map(Some).unwrap_or(None),
        };
        let Some(cmd) = cmd else { continue };
        match cmd {
            GuiCmd::DiscoverNow => {
                if let Some(a) = active.as_mut() {
                    let _ = event_tx.send(UiEvent::Discovering { active: true });
                    staged_pass(&shared, a).await;
                    let _ = event_tx.send(UiEvent::Discovering { active: false });
                } else {
                    emit_log(&shared, "daemon stopped — start it before discovering".to_string());
                }
            }
            GuiCmd::Pair { fingerprint } => do_pair(Arc::clone(&shared), fingerprint),
            GuiCmd::Ping { fingerprint } => do_ping(Arc::clone(&shared), fingerprint),
            GuiCmd::SendFiles { fingerprint, paths } => {
                do_send(Arc::clone(&shared), fingerprint, paths)
            }
            GuiCmd::PushClipboard { fingerprint, text } => {
                do_push_clipboard(Arc::clone(&shared), fingerprint, text)
            }
            GuiCmd::SetAutoPair(v) => {
                shared.auto_pair.store(v, Ordering::SeqCst);
                emit_log(
                    &shared,
                    format!("auto-pair {}", if v { "ON" } else { "OFF" }),
                );
            }
            GuiCmd::StopDaemon => {
                if let Some(mut a) = active.take() {
                    if let Some(h) = a.packet.take() {
                        h.abort();
                    }
                    a.watcher.abort();
                    if with_timeout("shutdown", SHUTDOWN_TIMEOUT, &shared, a.ls.shutdown())
                        .await
                        .is_none()
                    {
                        emit_log(&shared, "[discovery] shutdown timed out".to_string());
                    }
                    emit_log(&shared, "daemon stopped".to_string());
                    let _ = event_tx.send(UiEvent::DaemonStatus { running: false });
                }
            }
            GuiCmd::StartDaemon => {
                if active.is_none() {
                    active = Some(start_all(&shared).await);
                    let _ = event_tx.send(UiEvent::DaemonStatus { running: true });
                    emit_log(&shared, "daemon started".to_string());
                    push_snapshot(&shared);
                }
            }
        }
    }
}
