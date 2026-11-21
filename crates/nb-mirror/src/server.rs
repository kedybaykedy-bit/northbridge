use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
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
pub const FRAME_QUEUE_CAP: usize = 2;
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
    fn push(&mut self, frame: RawFrame) {
        while self.frames.len() >= FRAME_QUEUE_CAP {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }

    fn take_newest(&mut self) -> Option<RawFrame> {
        while self.frames.len() > 1 {
            self.frames.pop_front();
        }
        self.frames.pop_front()
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
        self.inner.frame_port
    }

    pub fn close(&self, peer_fp: &str) {
        let mut session = self.inner.session.lock().unwrap();
        if session.peer_fp.as_deref() == Some(peer_fp) {
            session.peer_fp = None;
            session.generation = session.generation.wrapping_add(1);
        }
    }

    pub fn close_generation(&self, peer_fp: &str, generation: u64) {
        let mut session = self.inner.session.lock().unwrap();
        if session.generation == generation && session.peer_fp.as_deref() == Some(peer_fp) {
            session.peer_fp = None;
            session.generation = session.generation.wrapping_add(1);
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
        {
            let mut stats = inner.stats.lock().unwrap();
            stats.frames += 1;
            if let Some(last) = last_seq {
                if meta.seq != last.wrapping_add(1) {
                    stats.gaps += 1;
                }
            }
        }
        last_seq = Some(meta.seq);
        queue.lock().unwrap().push(RawFrame { meta, payload });
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
    last_motion: Option<Instant>,
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
                queue.lock().unwrap().take_newest();
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
        last_motion: None,
    };
    loop {
        if !server.matches(generation, &peer_fp) {
            break;
        }
        if let Some(raw) = queue.lock().unwrap().take_newest() {
            let _ = raw.meta.flags;
            if let Some(decoder) = decoder.as_mut() {
                match decoder.decode(&raw.payload) {
                    Some(frame) => {
                        if let Err(e) = window.show_frame(&frame.bgra, frame.width, frame.height) {
                            eprintln!("[mirror] show frame: {e:#}");
                        } else {
                            cached = Some((frame.width, frame.height, frame.bgra));
                        }
                    }
                    None => {}
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(4));
        }
        drain_events(&mut window, &mut fwd, &mut cached);
    }
    fwd.release_all();
    window.destroy();
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
                let now = Instant::now();
                let due = fwd
                    .last_motion
                    .map(|t| now.duration_since(t) >= MOTION_THROTTLE)
                    .unwrap_or(true);
                if !due {
                    continue;
                }
                fwd.last_motion = Some(now);
                let (w, h) = window.size();
                fwd.push(json!({
                    "t": "tap",
                    "x": normalize_coord(ev.event_x as f64, w as f64),
                    "y": normalize_coord(ev.event_y as f64, h as f64),
                }));
            }
            Event::ButtonPress(ev) => {
                if !fwd.can_send {
                    continue;
                }
                if let Some((dx, dy)) = scroll_delta(ev.detail) {
                    fwd.push(json!({"t": "scroll", "dx": dx, "dy": dy}));
                } else if let Some(name) = button_name(ev.detail) {
                    fwd.pressed.insert(ev.detail);
                    fwd.push(json!({"t": "button", "b": name, "down": true}));
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
