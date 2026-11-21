use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use nb_core::identity::Identity;
use nb_core::packet::{NetworkPacket, now_ms};
use nb_transport::{tls, InputRegistry};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::KeyButMask;

use crate::control::{CAP, DEFAULT_FPS, DEFAULT_MAX_W, FRAME_OPEN_KIND};
use crate::decode::H264Decoder;
use crate::display::MirrorWindow;
use crate::events::{
    KeyAction, button_name, key_action, normalize_coord, scroll_delta, xkey_to_evdev,
};
use crate::frame::{self, FrameMeta};

pub const MAX_LINE: usize = 65536;
pub const FRAME_QUEUE_CAP: usize = 512;
pub const MOTION_THROTTLE: Duration = Duration::from_millis(16);

struct SessionState {
    peer_fp: Option<String>,
    max_w: u16,
    fps: u8,
    generation: u64,
}

#[derive(Default)]
struct Stats {
    frames: u64,
    gaps: u64,
}

struct RawFrame {
    meta: FrameMeta,
    payload: Vec<u8>,
}

struct FrameSlot {
    frames: VecDeque<RawFrame>,
}

impl FrameSlot {
    fn push(&mut self, frame: RawFrame) -> u32 {
        let mut dropped = 0;
        while self.frames.len() >= FRAME_QUEUE_CAP {
            self.frames.pop_front();
            dropped += 1;
        }
        self.frames.push_back(frame);
        dropped
    }

    fn len(&self) -> usize {
        self.frames.len()
    }

    fn drain(&mut self) -> Vec<RawFrame> {
        self.frames.drain(..).collect()
    }
}

struct ServerInner {
    frame_port: u16,
    tls: Option<Arc<rustls::ServerConfig>>,
    registry: InputRegistry,
    session: Mutex<SessionState>,
    stats: Mutex<Stats>,
}

#[derive(Clone)]
pub struct MirrorServer {
    inner: Arc<ServerInner>,
}

impl MirrorServer {
    pub fn start(identity: Identity, registry: InputRegistry) -> MirrorServer {
        let frame_port = identity.port.saturating_add(4);
        let tls_cfg = match tls::server_config(&identity) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                eprintln!("[mirror] TLS config: {e:#}");
                None
            }
        };
        let inner = Arc::new(ServerInner {
            frame_port,
            tls: tls_cfg,
            registry,
            session: Mutex::new(SessionState {
                peer_fp: None,
                max_w: DEFAULT_MAX_W,
                fps: DEFAULT_FPS,
                generation: 0,
            }),
            stats: Mutex::new(Stats::default()),
        });
        let worker = inner.clone();
        let _ = std::thread::Builder::new()
            .name("nb-mirror".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("[mirror] runtime: {e:#}");
                        return;
                    }
                };
                rt.block_on(accept_loop(worker));
            });
        MirrorServer { inner }
    }

    pub fn open(&self, peer_fp: &str, max_w: u16, fps: u8) -> u16 {
        let mut session = self.inner.session.lock().unwrap();
        session.peer_fp = Some(peer_fp.to_string());
        session.max_w = if max_w == 0 {
            DEFAULT_MAX_W
        } else {
            max_w.clamp(160, 1920)
        };
        session.fps = if fps == 0 { DEFAULT_FPS } else { fps.clamp(1, 30) };
        session.generation = session.generation.wrapping_add(1);
        eprintln!(
            "[mirror] session open {} gen {}",
            &peer_fp[..12.min(peer_fp.len())],
            session.generation
        );
        self.inner.frame_port
    }

    pub fn close(&self, peer_fp: &str) {
        let mut session = self.inner.session.lock().unwrap();
        if session.peer_fp.as_deref() == Some(peer_fp) {
            session.peer_fp = None;
            session.generation = session.generation.wrapping_add(1);
            eprintln!("[mirror] release {}", &peer_fp[..12.min(peer_fp.len())]);
        }
    }

    pub fn close_generation(&self, peer_fp: &str, generation: u64) {
        let mut session = self.inner.session.lock().unwrap();
        if session.generation == generation && session.peer_fp.as_deref() == Some(peer_fp) {
            session.peer_fp = None;
            session.generation = session.generation.wrapping_add(1);
            eprintln!(
                "[mirror] frame eof -> idle {} (gen {generation})",
                &peer_fp[..12.min(peer_fp.len())]
            );
        }
    }

    pub fn close_any(&self) {
        let mut session = self.inner.session.lock().unwrap();
        if session.peer_fp.is_some() {
            session.peer_fp = None;
            session.generation = session.generation.wrapping_add(1);
        }
    }

    pub fn is_open(&self) -> bool {
        self.inner.session.lock().unwrap().peer_fp.is_some()
    }

    pub fn frame_port(&self) -> u16 {
        self.inner.frame_port
    }

    pub fn session_peer(&self) -> Option<String> {
        self.inner.session.lock().unwrap().peer_fp.clone()
    }

    pub fn frames_received(&self) -> u64 {
        self.inner.stats.lock().unwrap().frames
    }

    pub fn seq_gaps(&self) -> u64 {
        self.inner.stats.lock().unwrap().gaps
    }

    pub fn handle_control_from(&self, peer_fp: &str, body: &Value) -> Option<Value> {
        crate::control::handle_control_from(self, peer_fp, body)
    }

    fn matches(&self, generation: u64, peer_fp: &str) -> bool {
        let session = self.inner.session.lock().unwrap();
        session.generation == generation && session.peer_fp.as_deref() == Some(peer_fp)
    }
}

fn gate_allows(current: Option<&str>, client_fp: &str) -> bool {
    match current {
        Some(expected) if !client_fp.is_empty() => expected == client_fp,
        _ => false,
    }
}

async fn accept_loop(inner: Arc<ServerInner>) {
    let tls_cfg = match inner.tls.clone() {
        Some(cfg) => cfg,
        None => return,
    };
    let acceptor = TlsAcceptor::from(tls_cfg);
    let listener = match TcpListener::bind(("0.0.0.0", inner.frame_port)).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("[mirror] bind frame port {}: {e:#}", inner.frame_port);
            return;
        }
    };
    loop {
        let (stream, remote) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(e) => {
                eprintln!("[mirror] accept: {e:#}");
                continue;
            }
        };
        let worker = inner.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_one(worker, acceptor, stream).await {
                let msg = format!("{e:#}");
                if !msg.contains("early eof") && !msg.contains("connection reset") {
                    eprintln!("[mirror] {remote}: {msg}");
                }
            }
        });
    }
}

async fn serve_one(
    inner: Arc<ServerInner>,
    acceptor: TlsAcceptor,
    stream: TcpStream,
) -> Result<()> {
    let tls_stream = acceptor.accept(stream).await.context("TLS handshake")?;
    let peer_fp = {
        let (_, conn) = tls_stream.get_ref();
        tls::server_peer_fingerprint(conn)
    }
    .context("no client certificate presented")?;
    let gate = {
        let session = inner.session.lock().unwrap();
        if gate_allows(session.peer_fp.as_deref(), &peer_fp) {
            Some(session.generation)
        } else {
            None
        }
    };
    let Some(generation) = gate else {
        eprintln!("[mirror] rejected frame connection from {peer_fp}");
        let (read_half, mut write_half) = tokio::io::split(tls_stream);
        let mut reader = BufReader::new(read_half);
        let mut line = String::new();
        let _ = tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line)).await;
        write_half.shutdown().await.ok();
        return Ok(());
    };

    let (read_half, mut write_half) = tokio::io::split(tls_stream);
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let n = reader.read_line(&mut line).await.context("read frame open")?;
    if n == 0 {
        return Ok(());
    }
    if line.len() > MAX_LINE {
        anyhow::bail!("frame open too large ({} bytes)", line.len());
    }
    let pkt = NetworkPacket::from_line(&line).context("malformed frame open")?;
    if pkt.kind != FRAME_OPEN_KIND {
        anyhow::bail!("unexpected frame open kind: {}", pkt.kind);
    }
    if !inner
        .session
        .lock()
        .map(|s| s.generation == generation && s.peer_fp.as_deref() == Some(peer_fp.as_str()))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let ack = NetworkPacket::new(FRAME_OPEN_KIND, json!({"ok": true}));
    let mut out = ack.to_line();
    out.push('\n');
    write_half
        .write_all(out.as_bytes())
        .await
        .context("write frame open ack")?;
    write_half.flush().await.ok();

    let queue = Arc::new(Mutex::new(FrameSlot {
        frames: VecDeque::new(),
    }));
    spawn_ui(inner.clone(), queue.clone(), peer_fp.clone(), generation);

    let server = MirrorServer {
        inner: inner.clone(),
    };
    let mut last_seq: Option<u32> = None;
    let mut dumped = 0u32;
    let mut overflow_total = 0u64;
    loop {
        if !server.matches(generation, &peer_fp) {
            break;
        }
        let mut header = [0u8; 28];
        if reader.read_exact(&mut header).await.is_err() {
            break;
        }
        let (meta, len) = match frame::parse_header(&header) {
            Ok(parsed) => parsed,
            Err(e) => {
                eprintln!("[mirror] bad frame header: {e:#}");
                break;
            }
        };
        let mut payload = vec![0u8; len as usize];
        if reader.read_exact(&mut payload).await.is_err() {
            break;
        }
        if dumped < 40 {
            dumped += 1;
            let _ = std::fs::create_dir_all("/tmp/nb-frame-dump");
            let _ = std::fs::write(format!("/tmp/nb-frame-dump/{dumped:06}.264"), &payload);
        }
        {
            let mut stats = inner.stats.lock().unwrap();
            stats.frames += 1;
            if let Some(last) = last_seq {
                if meta.seq != last.wrapping_add(1) {
                    stats.gaps += 1;
                    let g = stats.gaps;
                    if g <= 20 || g % 100 == 0 {
                        eprintln!(
                            "[mirror] seq gap: last={last} now={} (gaps={g})",
                            meta.seq
                        );
                    }
                }
            }
        }
        last_seq = Some(meta.seq);
        let ts_ms = meta.ts_ms;
        let (dropped, depth) = {
            let mut q = queue.lock().unwrap();
            let dropped = q.push(RawFrame { meta, payload });
            (dropped, q.len())
        };
        overflow_total += dropped as u64;
        let frames_n = inner.stats.lock().unwrap().frames;
        if frames_n % 60 == 0 || dropped > 0 {
            eprintln!(
                "[mirror] lat={}ms depth={} dropped={} overflow_total={}",
                now_ms().saturating_sub(ts_ms),
                depth,
                dropped,
                overflow_total
            );
        }
    }
    server.close_generation(&peer_fp, generation);
    write_half.shutdown().await.ok();
    Ok(())
}

fn spawn_ui(
    inner: Arc<ServerInner>,
    queue: Arc<Mutex<FrameSlot>>,
    peer_fp: String,
    generation: u64,
) {
    let server = MirrorServer { inner };
    let _ = std::thread::Builder::new()
        .name("nb-mirror-ui".into())
        .spawn(move || {
            ui_loop(server, queue, peer_fp, generation);
        });
}

struct UiForwarder<'a> {
    server: &'a MirrorServer,
    peer_fp: &'a str,
    can_send: bool,
    pressed: HashSet<u8>,
    pointer: Option<(f64, f64)>,
}

impl<'a> UiForwarder<'a> {
    fn push(&mut self, body: Value) {
        if !self.can_send {
            return;
        }
        let pkt = NetworkPacket::new(CAP, body);
        if !self.server.inner.registry.send(self.peer_fp, pkt) {
            eprintln!("[mirror] input stream gone, stop forwarding");
            self.can_send = false;
        }
    }

    fn release_all(&mut self) {
        for detail in std::mem::take(&mut self.pressed) {
            if let Some(name) = button_name(detail) {
                self.push(json!({"t": "button", "b": name, "down": false}));
                if !self.can_send {
                    break;
                }
            }
        }
    }
}

fn ui_loop(
    server: MirrorServer,
    queue: Arc<Mutex<FrameSlot>>,
    peer_fp: String,
    generation: u64,
) {
    let mut window = match MirrorWindow::connect() {
        Ok(window) => window,
        Err(e) => {
            eprintln!("[mirror] no X display: {e:#}");
            while server.matches(generation, &peer_fp) {
                queue.lock().unwrap().drain();
                std::thread::sleep(Duration::from_millis(20));
            }
            return;
        }
    };
    let mut decoder = match H264Decoder::new() {
        Ok(decoder) => Some(decoder),
        Err(e) => {
            eprintln!("[mirror] decoder init: {e:#}");
            None
        }
    };
    let mut cached: Option<(u16, u16, Vec<u8>)> = None;
    let mut fwd = UiForwarder {
        server: &server,
        peer_fp: &peer_fp,
        can_send: true,
        pressed: HashSet::new(),
        pointer: None,
    };
    let mut shown = 0u64;
    let mut waits = 0u64;
    let mut frames_logged = 0u32;
    let mut error_streak = false;
    let mut last_show = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let mut shown_1s = 0u32;
    let mut t1s = Instant::now();
    let mut last_lat = 0u64;
    let mut last_frame_at = Instant::now();
    let mut ever_framed = false;
    loop {
        if !server.matches(generation, &peer_fp) {
            eprintln!("[mirror] ui thread end (gen {generation} replaced)");
            break;
        }
        let batch = queue.lock().unwrap().drain();
        if batch.is_empty() {
            if !ever_framed && last_frame_at.elapsed() >= Duration::from_secs(10) {
                eprintln!("[mirror] no frames in10s — closing empty window");
                break;
            }
            std::thread::sleep(Duration::from_millis(4));
        } else {
            last_frame_at = Instant::now();
            ever_framed = true;
            let mut latest: Option<(Vec<u8>, u16, u16)> = None;
            let n = batch.len();
            for (idx, raw) in batch.into_iter().enumerate() {
                if frames_logged < 3 {
                    frames_logged += 1;
                    eprintln!("[mirror] frame seq={} len={}", raw.meta.seq, raw.payload.len());
                }
                if error_streak && !contains_idr(&raw.payload) {
                    waits += 1;
                    continue;
                }
                if let Some(decoder) = decoder.as_mut() {
                    let errs_before = decoder.errors;
                    let dec_before = decoder.decoded;
                    let want_convert = idx + 1 == n
                        && last_show.elapsed() >= Duration::from_millis(40);
                    if want_convert {
                        last_lat = now_ms().saturating_sub(raw.meta.ts_ms);
                        match decoder.decode(&raw.payload) {
                            Some(frame) => {
                                error_streak = false;
                                shown += 1;
                                shown_1s += 1;
                                let (w, h) = (frame.width, frame.height);
                                if shown <= 3 || shown % 60 == 0 {
                                    eprintln!("[mirror] shown x{shown} {w}x{h}");
                                }
                                latest = Some((frame.bgra, w, h));
                            }
                            None => {
                                waits += 1;
                                if decoder.errors > errs_before {
                                    error_streak = true;
                                }
                                if waits <= 5 || waits % 60 == 0 {
                                    eprintln!(
                                        "[mirror] decode none x{waits} (decoded={} errors={}) streak={error_streak}",
                                        decoder.decoded, decoder.errors
                                    );
                                }
                            }
                        }
                    } else {
                        decoder.feed(&raw.payload);
                        if decoder.decoded > dec_before {
                            error_streak = false;
                        } else if decoder.errors > errs_before {
                            error_streak = true;
                        }
                    }
                }
            }
            if let Some((bgra, w, h)) = latest {
                if last_show.elapsed() >= Duration::from_millis(25) {
                    if let Err(e) = window.show_frame(&bgra, w, h) {
                        eprintln!("[mirror] show frame: {e:#}");
                    } else {
                        cached = Some((w, h, bgra));
                    }
                    last_show = std::time::Instant::now();
                }
            }
        }
        drain_events(&mut window, &mut fwd, &mut cached);
        if t1s.elapsed() >= Duration::from_secs(1) {
            let depth = queue.lock().unwrap().len();
            let title = format!(
                "Northbridge — mirror · {} fps · {} ms · q{}",
                shown_1s, last_lat, depth
            );
            window.set_title(title.as_bytes());
            if shown_1s > 0 || t1s.elapsed() >= Duration::from_secs(5) {
                eprintln!("[mirror] fps={shown_1s} lat={last_lat}ms depth={depth}");
            }
            shown_1s = 0;
            t1s = Instant::now();
        }
    }
    fwd.release_all();
    window.destroy();
}

fn contains_idr(payload: &[u8]) -> bool {
    let mut i = 0;
    while i + 4 < payload.len() {
        if payload[i] == 0 && payload[i + 1] == 0 {
            let (start, hdr) = if payload[i + 2] == 1 {
                (i + 3, i + 3)
            } else if payload[i + 2] == 0 && payload[i + 3] == 1 {
                (i + 4, i + 4)
            } else {
                i += 1;
                continue;
            };
            if hdr < payload.len() && (payload[hdr] & 0x1F) == 5 {
                return true;
            }
            i = start + 1;
            continue;
        }
        i += 1;
    }
    false
}

fn drain_events(
    window: &mut MirrorWindow,
    fwd: &mut UiForwarder<'_>,
    cached: &mut Option<(u16, u16, Vec<u8>)>,
) {
    loop {
        let event = match window.poll_event() {
            Ok(event) => event,
            Err(e) => {
                eprintln!("[mirror] poll event: {e:#}");
                return;
            }
        };
        let Some(event) = event else { break };
        match event {
            Event::MotionNotify(ev) => {
                if !fwd.can_send {
                    continue;
                }
                let (w, h) = window.size();
                let nx = normalize_coord(ev.event_x as f64, w as f64);
                let ny = normalize_coord(ev.event_y as f64, h as f64);
                fwd.pointer = Some((nx, ny));
            }
            Event::EnterNotify(ev) => {
                if !fwd.can_send {
                    continue;
                }
                let (w, h) = window.size();
                let nx = normalize_coord(ev.event_x as f64, w as f64);
                let ny = normalize_coord(ev.event_y as f64, h as f64);
                fwd.pointer = Some((nx, ny));
            }
            Event::ButtonPress(ev) => {
                if !fwd.can_send {
                    continue;
                }
                if let Some((dx, dy)) = scroll_delta(ev.detail) {
                    fwd.push(json!({"t": "scroll", "dx": dx, "dy": dy}));
                } else if let Some(name) = button_name(ev.detail) {
                    let (w, h) = window.size();
                    let nx = normalize_coord(ev.event_x as f64, w as f64);
                    let ny = normalize_coord(ev.event_y as f64, h as f64);
                    fwd.pointer = Some((nx, ny));
                    fwd.pressed.insert(ev.detail);
                    fwd.push(json!({"t": "button", "b": name, "down": true, "x": nx, "y": ny}));
                }
            }
            Event::ButtonRelease(ev) => {
                if !fwd.can_send {
                    continue;
                }
                if scroll_delta(ev.detail).is_none() {
                    if let Some(name) = button_name(ev.detail) {
                        fwd.pressed.remove(&ev.detail);
                        fwd.push(json!({"t": "button", "b": name, "down": false}));
                    }
                }
            }
            Event::KeyPress(ev) => {
                if !fwd.can_send {
                    continue;
                }
                let code = xkey_to_evdev(ev.detail as u16);
                match primary_keysym(window, ev.detail, ev.state).map(key_action) {
                    Some(KeyAction::Type(ch)) => {
                        fwd.push(json!({"t": "type", "s": ch.to_string()}));
                    }
                    _ => {
                        fwd.push(json!({"t": "key", "code": code, "down": true}));
                    }
                }
            }
            Event::KeyRelease(ev) => {
                if !fwd.can_send {
                    continue;
                }
                let code = xkey_to_evdev(ev.detail as u16);
                let is_type = matches!(
                    primary_keysym(window, ev.detail, ev.state).map(key_action),
                    Some(KeyAction::Type(_))
                );
                if !is_type {
                    fwd.push(json!({"t": "key", "code": code, "down": false}));
                }
            }
            Event::FocusOut(_) => {
                fwd.release_all();
            }
            Event::Expose(_) => {
                if let Some((w, h, bgra)) = cached.as_ref() {
                    let _ = window.show_frame(bgra, *w, *h);
                }
            }
            _ => {}
        }
    }
}

fn primary_keysym(window: &mut MirrorWindow, keycode: u8, state: KeyButMask) -> Option<u32> {
    let keysyms = window.keysyms(keycode);
    if keysyms.is_empty() {
        return None;
    }
    let shift = u16::from(state) & u16::from(KeyButMask::SHIFT) != 0;
    if shift && keysyms.len() > 1 {
        Some(keysyms[1])
    } else {
        Some(keysyms[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_allows_only_bound_peer() {
        assert!(gate_allows(Some("A"), "A"));
        assert!(!gate_allows(Some("A"), "B"));
        assert!(!gate_allows(None, "A"));
        assert!(!gate_allows(Some("A"), ""));
        assert!(!gate_allows(None, ""));
    }

    #[test]
    fn open_replaces_close_clears() {
        let id = Identity {
            alias: "test".into(),
            cert_pem: String::new(),
            key_pem: String::new(),
            fingerprint: "TEST".into(),
            port: 44850 + (std::process::id() as u16 % 500),
        };
        let srv = MirrorServer::start(id, InputRegistry::new());
        assert!(!srv.is_open());
        let port = srv.open("PEER-A", 1080, 20);
        assert_eq!(port, srv.frame_port());
        assert!(srv.is_open());
        assert_eq!(srv.session_peer().as_deref(), Some("PEER-A"));
        srv.open("PEER-B", 640, 10);
        assert_eq!(srv.session_peer().as_deref(), Some("PEER-B"));
        srv.close("PEER-A");
        assert!(srv.is_open());
        srv.close("PEER-B");
        assert!(!srv.is_open());
    }
}
