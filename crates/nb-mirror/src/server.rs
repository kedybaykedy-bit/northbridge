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
use crate::display::{KvmGrab, MirrorWindow, is_connection_dead};
use crate::events::{
    DragTracker, KeyAction, KvmHeld, button_cancel_packet, button_down_packet, button_name,
    button_up_packet, enter_packet, hover_packet, key_action, key_down_packet, key_up_packet,
    leave_packet, move_delta_packet, move_packet, normalize_coord, scroll_delta, scroll_packet, type_packet,
    xkey_to_evdev,
};
use crate::frame::{self, FrameMeta};

pub const MAX_LINE: usize = 65536;
pub const FRAME_QUEUE_CAP: usize = 512;
pub const MOTION_THROTTLE: Duration = Duration::from_millis(4);
pub const KVM_KIND: &str = "nb.kvm";
const FRAME_READ_TIMEOUT: Duration = Duration::from_secs(15);
const STALL_RESET: Duration = Duration::from_secs(10);

struct SessionState {
    peer_fp: Option<String>,
    max_w: u16,
    fps: u8,
    generation: u64,
}

struct KvmSlot {
    peer: Option<String>,
    generation: u64,
}

impl KvmSlot {
    fn new() -> Self {
        Self {
            peer: None,
            generation: 0,
        }
    }

    fn claim(&mut self, peer_fp: &str) -> u64 {
        self.peer = Some(peer_fp.to_string());
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    fn matches(&self, generation: u64, peer_fp: &str) -> bool {
        self.generation == generation && self.peer.as_deref() == Some(peer_fp)
    }

    fn release_if(&mut self, generation: u64, peer_fp: &str) {
        if self.matches(generation, peer_fp) {
            self.peer = None;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    fn release_for(&mut self, peer_fp: &str) -> bool {
        if self.peer.as_deref() == Some(peer_fp) {
            self.peer = None;
            self.generation = self.generation.wrapping_add(1);
            true
        } else {
            false
        }
    }

    fn release_all(&mut self) -> Option<String> {
        let peer = self.peer.take();
        if peer.is_some() {
            self.generation = self.generation.wrapping_add(1);
        }
        peer
    }
}

fn short_fp(fp: &str) -> &str {
    &fp[..12.min(fp.len())]
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
    kvm: Mutex<KvmSlot>,
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
            kvm: Mutex::new(KvmSlot::new()),
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
        match body.get("t").and_then(Value::as_str) {
            Some("kvm_grab") => {
                let ok = self.kvm_grab(peer_fp);
                Some(json!({"t": "kvm_grabbed", "ok": ok}))
            }
            Some("kvm_release") => {
                self.kvm_release(peer_fp);
                Some(json!({"t": "kvm_released"}))
            }
            _ => crate::control::handle_control_from(self, peer_fp, body),
        }
    }

    fn matches(&self, generation: u64, peer_fp: &str) -> bool {
        let session = self.inner.session.lock().unwrap();
        session.generation == generation && session.peer_fp.as_deref() == Some(peer_fp)
    }

    pub fn kvm_grab(&self, peer_fp: &str) -> bool {
        let generation = self.inner.kvm.lock().unwrap().claim(peer_fp);
        let grab = match KvmGrab::acquire() {
            Ok(grab) => grab,
            Err(e) => {
                eprintln!("[kvm] grab failed for {}: {e:#}", short_fp(peer_fp));
                self.inner.kvm.lock().unwrap().release_if(generation, peer_fp);
                return false;
            }
        };
        let (w, h) = grab.screen_size();
        let (nx, ny) = match grab.pointer() {
            Ok((px, py)) => (
                normalize_coord(px as f64, w as f64),
                normalize_coord(py as f64, h as f64),
            ),
            Err(_) => (0.5, 0.5),
        };
        if !self.inner.registry.send(
            peer_fp,
            NetworkPacket::new(KVM_KIND, enter_packet(nx, ny)),
        ) {
            eprintln!("[kvm] peer has no input stream");
            grab.release();
            self.inner.kvm.lock().unwrap().release_if(generation, peer_fp);
            return false;
        }
        eprintln!("[kvm] grab {} gen {generation}", short_fp(peer_fp));
        let server = self.clone();
        let peer = peer_fp.to_string();
        let _ = std::thread::Builder::new()
            .name("nb-kvm".into())
            .spawn(move || kvm_loop(server, grab, peer, generation));
        true
    }

    pub fn kvm_release(&self, peer_fp: &str) {
        let had = {
            let mut slot = self.inner.kvm.lock().unwrap();
            slot.release_for(peer_fp)
        };
        if had {
            self.inner.registry.send(
                peer_fp,
                NetworkPacket::new(KVM_KIND, leave_packet()),
            );
            eprintln!("[kvm] release {}", short_fp(peer_fp));
        }
    }

    pub fn kvm_release_any(&self) {
        let peer = {
            let mut slot = self.inner.kvm.lock().unwrap();
            slot.release_all()
        };
        if let Some(fp) = peer {
            self.inner.registry.send(&fp, NetworkPacket::new(KVM_KIND, leave_packet()));
            eprintln!("[kvm] release {}", short_fp(&fp));
        }
    }

    pub fn kvm_is_open(&self) -> bool {
        self.inner.kvm.lock().unwrap().peer.is_some()
    }

    pub fn kvm_peer(&self) -> Option<String> {
        self.inner.kvm.lock().unwrap().peer.clone()
    }

    fn kvm_matches(&self, generation: u64, peer_fp: &str) -> bool {
        self.inner.kvm.lock().unwrap().matches(generation, peer_fp)
    }

    fn kvm_released(&self, generation: u64, peer_fp: &str) {
        self.inner.kvm.lock().unwrap().release_if(generation, peer_fp);
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
        let header_read =
            tokio::time::timeout(FRAME_READ_TIMEOUT, reader.read_exact(&mut header)).await;
        if !matches!(header_read, Ok(Ok(_))) {
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
        let payload_read =
            tokio::time::timeout(FRAME_READ_TIMEOUT, reader.read_exact(&mut payload)).await;
        if !matches!(payload_read, Ok(Ok(_))) {
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
    drag: DragTracker,
    last_move: Instant,
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
        self.drag.clear();
        for detail in std::mem::take(&mut self.pressed) {
            if let Some(name) = button_name(detail) {
                self.push(button_cancel_packet(name));
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
        drag: DragTracker::new(),
        last_move: Instant::now(),
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
    loop {
        if !server.matches(generation, &peer_fp) {
            eprintln!("[mirror] ui thread end (gen {generation} replaced)");
            break;
        }
        let batch = queue.lock().unwrap().drain();
        if batch.is_empty() {
            if last_frame_at.elapsed() >= STALL_RESET {
                eprintln!("[mirror] no frames in 10s — resetting session (gen {generation})");
                server.close_generation(&peer_fp, generation);
                break;
            }
            std::thread::sleep(Duration::from_millis(4));
        } else {
            last_frame_at = Instant::now();
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
                        if is_connection_dead(&e) {
                            eprintln!(
                                "[mirror] X connection lost ({e:#}) — closing window (gen {generation})"
                            );
                            server.close_generation(&peer_fp, generation);
                            break;
                        }
                        eprintln!("[mirror] show frame: {e:#}");
                    } else {
                        cached = Some((w, h, bgra));
                    }
                    last_show = std::time::Instant::now();
                }
            }
        }
        if !drain_events(&mut window, &mut fwd, &mut cached) {
            eprintln!("[mirror] X connection lost (event poll) — closing window (gen {generation})");
            server.close_generation(&peer_fp, generation);
            break;
        }
        if t1s.elapsed() >= Duration::from_secs(1) {
            let depth = queue.lock().unwrap().len();
            let title = format!(
                "Northbridge — mirror · {} fps · {} ms · q{}",
                shown_1s, last_lat, depth
            );
            if !window.set_title(title.as_bytes()) {
                eprintln!("[mirror] X connection lost (title) — closing window (gen {generation})");
                server.close_generation(&peer_fp, generation);
                break;
            }
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

fn kvm_send(server: &MirrorServer, peer_fp: &str, body: Value) -> bool {
    server
        .inner
        .registry
        .send(peer_fp, NetworkPacket::new(KVM_KIND, body))
}

fn kvm_loop(server: MirrorServer, mut grab: KvmGrab, peer_fp: String, generation: u64) {
    let (w, h) = grab.screen_size();
    let mut held = KvmHeld::new();
    let mut last_move = Instant::now() - MOTION_THROTTLE;
    let mut motion_n = 0u32;
    let mut motion_t = Instant::now();
    let mut acc_dx = 0.0f64;
    let mut acc_dy = 0.0f64;
    let mut other_logged = 0u32;
    loop {
        if !server.kvm_matches(generation, &peer_fp) {
            break;
        }
        let event = match grab.poll_event() {
            Ok(event) => event,
            Err(e) => {
                if is_connection_dead(&e) {
                    eprintln!("[kvm] X connection lost: {e:#}");
                } else {
                    eprintln!("[kvm] poll event: {e:#}");
                }
                break;
            }
        };
        let Some(event) = event else {
            std::thread::sleep(Duration::from_millis(4));
            continue;
        };
        if other_logged < 12 {
            let name = match &event {
                Event::XinputRawMotion(_) => "raw",
                Event::MotionNotify(_) => "motion",
                Event::ButtonPress(_) => "bpress",
                Event::ButtonRelease(_) => "brel",
                Event::KeyPress(_) => "kpress",
                Event::KeyRelease(_) => "krel",
                _ => "other",
            };
            if name != "raw" {
                eprintln!("[kvm] ev {name}");
                other_logged += 1;
            }
        }
        match event {
            Event::XinputRawMotion(ev) => {
                let m = ev.valuator_mask.first().copied().unwrap_or(0);
                let mut idx = 0usize;
                let mut dx = 0.0f64;
                let mut dy = 0.0f64;
                for bit in 0..32u32 {
                    if m & (1 << bit) != 0 {
                        if let Some(raw) = ev.axisvalues_raw.get(idx) {
                            let v = f64::from(raw.integral) + f64::from(raw.frac) / 4294967296.0;
                            if bit == 0 {
                                dx = v;
                            } else if bit == 1 {
                                dy = v;
                            }
                        }
                        idx += 1;
                    }
                }
                acc_dx += dx;
                acc_dy += dy;
                if last_move.elapsed() >= MOTION_THROTTLE {
                    last_move = Instant::now();
                    motion_n += 1;
                    if motion_t.elapsed() >= Duration::from_secs(1) {
                        eprintln!("[kvm] fwd {motion_n} motion/s");
                        motion_n = 0;
                        motion_t = Instant::now();
                    }
                    if acc_dx != 0.0 || acc_dy != 0.0 {
                        if !kvm_send(&server, &peer_fp, move_delta_packet(acc_dx, acc_dy)) {
                            break;
                        }
                        acc_dx = 0.0;
                        acc_dy = 0.0;
                    }
                }
            }
            Event::ButtonPress(ev) => {
                if let Some((dx, dy)) = scroll_delta(ev.detail) {
                    if !kvm_send(&server, &peer_fp, scroll_packet(dx, dy)) {
                        break;
                    }
                } else if let Some(name) = button_name(ev.detail) {
                    held.press_button(ev.detail);
                    if !kvm_send(&server, &peer_fp, button_down_packet(name)) {
                        break;
                    }
                }
            }
            Event::ButtonRelease(ev) => {
                if scroll_delta(ev.detail).is_none() {
                    if let Some(name) = button_name(ev.detail) {
                        held.release_button(ev.detail);
                        if !kvm_send(&server, &peer_fp, button_up_packet(name)) {
                            break;
                        }
                    }
                }
            }
            Event::KeyPress(ev) => {
                let code = xkey_to_evdev(u16::from(ev.detail));
                match kvm_keysym(&mut grab, ev.detail, ev.state).map(key_action) {
                    Some(KeyAction::Type(ch)) => {
                        if !kvm_send(&server, &peer_fp, type_packet(ch)) {
                            break;
                        }
                    }
                    _ => {
                        held.press_key(code);
                        if !kvm_send(&server, &peer_fp, key_down_packet(code)) {
                            break;
                        }
                    }
                }
            }
            Event::KeyRelease(ev) => {
                let code = xkey_to_evdev(u16::from(ev.detail));
                let is_type = matches!(
                    kvm_keysym(&mut grab, ev.detail, ev.state).map(key_action),
                    Some(KeyAction::Type(_))
                );
                if !is_type {
                    held.release_key(code);
                    if !kvm_send(&server, &peer_fp, key_up_packet(code)) {
                        break;
                    }
                }
            }
            Event::FocusOut(_) => {
                let mut ok = true;
                for body in held.cancel_all() {
                    if !kvm_send(&server, &peer_fp, body) {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            _ => {}
        }
    }
    for body in held.cancel_all() {
        kvm_send(&server, &peer_fp, body);
    }
    kvm_send(&server, &peer_fp, leave_packet());
    grab.release();
    server.kvm_released(generation, &peer_fp);
    eprintln!("[kvm] end {} gen {generation}", short_fp(&peer_fp));
}

fn kvm_keysym(grab: &mut KvmGrab, keycode: u8, state: KeyButMask) -> Option<u32> {
    let keysyms = grab.keysyms(keycode);
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
) -> bool {
    loop {
        let event = match window.poll_event() {
            Ok(event) => event,
            Err(e) => {
                if is_connection_dead(&e) {
                    return false;
                }
                eprintln!("[mirror] poll event: {e:#}");
                return true;
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
                if fwd.drag.is_held() {
                    if fwd.last_move.elapsed() >= MOTION_THROTTLE {
                        fwd.last_move = Instant::now();
                        if fwd.drag.motion(nx, ny) {
                            fwd.push(move_packet(nx, ny));
                        }
                    }
                } else if fwd.last_move.elapsed() >= MOTION_THROTTLE {
                    fwd.last_move = Instant::now();
                    fwd.push(hover_packet(nx, ny));
                }
            }
            Event::EnterNotify(ev) => {
                if !fwd.can_send {
                    continue;
                }
                let (w, h) = window.size();
                let nx = normalize_coord(ev.event_x as f64, w as f64);
                let ny = normalize_coord(ev.event_y as f64, h as f64);
                fwd.pointer = Some((nx, ny));
                fwd.last_move = Instant::now();
                fwd.push(hover_packet(nx, ny));
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
                    if ev.detail == 1 {
                        fwd.drag.press(nx, ny);
                        fwd.last_move = Instant::now();
                    }
                    fwd.push(button_down_packet(name));
                }
            }
            Event::ButtonRelease(ev) => {
                if !fwd.can_send {
                    continue;
                }
                if scroll_delta(ev.detail).is_none() {
                    if let Some(name) = button_name(ev.detail) {
                        fwd.pressed.remove(&ev.detail);
                        if ev.detail == 1 {
                            let (w, h) = window.size();
                            let nx = normalize_coord(ev.event_x as f64, w as f64);
                            let ny = normalize_coord(ev.event_y as f64, h as f64);
                            fwd.drag.release(nx, ny);
                            fwd.push(button_up_packet(name));
                        } else {
                            fwd.push(button_up_packet(name));
                        }
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
                    if let Err(e) = window.show_frame(bgra, *w, *h) {
                        if is_connection_dead(&e) {
                            return false;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    true
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

    #[test]
    fn close_generation_clears_once_and_reopens() {
        let id = Identity {
            alias: "test".into(),
            cert_pem: String::new(),
            key_pem: String::new(),
            fingerprint: "TEST".into(),
            port: 45800 + (std::process::id() as u16 % 200),
        };
        let srv = MirrorServer::start(id, InputRegistry::new());
        srv.open("PEER-X", 1080, 20);
        let gen = srv.inner.session.lock().unwrap().generation;
        assert!(srv.matches(gen, "PEER-X"));
        srv.close_generation("PEER-X", gen);
        assert!(!srv.is_open());
        let bumped = srv.inner.session.lock().unwrap().generation;
        assert_eq!(bumped, gen.wrapping_add(1));
        srv.close_generation("PEER-X", gen);
        assert_eq!(srv.inner.session.lock().unwrap().generation, bumped);
        srv.open("PEER-Y", 640, 10);
        assert!(srv.is_open());
        assert_eq!(srv.session_peer().as_deref(), Some("PEER-Y"));
        assert!(srv.inner.session.lock().unwrap().generation > bumped);
    }

    #[test]
    fn kvm_slot_claim_release() {
        let mut slot = KvmSlot::new();
        assert!(!slot.matches(0, "A"));
        let gen = slot.claim("A");
        assert!(slot.matches(gen, "A"));
        assert!(!slot.matches(gen, "B"));
        assert!(!slot.matches(gen.wrapping_add(1), "A"));
        slot.release_if(gen, "B");
        assert!(slot.matches(gen, "A"));
        slot.release_if(gen.wrapping_add(1), "A");
        assert!(slot.matches(gen, "A"));
        slot.release_if(gen, "A");
        assert!(!slot.matches(gen, "A"));
        assert!(slot.peer.is_none());
    }

    #[test]
    fn kvm_slot_reclaim_invalidates_old_generation() {
        let mut slot = KvmSlot::new();
        let first = slot.claim("A");
        let second = slot.claim("A");
        assert_ne!(first, second);
        assert!(!slot.matches(first, "A"));
        assert!(slot.matches(second, "A"));
        let third = slot.claim("B");
        assert!(!slot.matches(second, "A"));
        assert!(slot.matches(third, "B"));
    }

    #[test]
    fn kvm_slot_release_for_wrong_peer_keeps_grab() {
        let mut slot = KvmSlot::new();
        slot.claim("A");
        assert!(!slot.release_for("B"));
        assert_eq!(slot.peer.as_deref(), Some("A"));
        assert!(slot.release_for("A"));
        assert!(slot.peer.is_none());
        assert!(!slot.release_for("A"));
    }

    #[test]
    fn kvm_slot_release_all_reports_peer_once() {
        let mut slot = KvmSlot::new();
        assert_eq!(slot.release_all(), None);
        slot.claim("A");
        assert_eq!(slot.release_all().as_deref(), Some("A"));
        assert_eq!(slot.release_all(), None);
    }

    #[test]
    fn kvm_release_api_clears_peer() {
        let id = Identity {
            alias: "test".into(),
            cert_pem: String::new(),
            key_pem: String::new(),
            fingerprint: "TEST".into(),
            port: 45900 + (std::process::id() as u16 % 100),
        };
        let srv = MirrorServer::start(id, InputRegistry::new());
        assert!(!srv.kvm_is_open());
        srv.inner.kvm.lock().unwrap().claim("PEER-K");
        assert!(srv.kvm_is_open());
        assert_eq!(srv.kvm_peer().as_deref(), Some("PEER-K"));
        srv.kvm_release("OTHER");
        assert!(srv.kvm_is_open());
        srv.kvm_release("PEER-K");
        assert!(!srv.kvm_is_open());
        assert!(srv.kvm_peer().is_none());
        srv.inner.kvm.lock().unwrap().claim("PEER-K");
        srv.kvm_release_any();
        assert!(!srv.kvm_is_open());
    }

    #[test]
    fn kvm_control_release_replies_and_clears() {
        let id = Identity {
            alias: "test".into(),
            cert_pem: String::new(),
            key_pem: String::new(),
            fingerprint: "TEST".into(),
            port: 45950 + (std::process::id() as u16 % 50),
        };
        let srv = MirrorServer::start(id, InputRegistry::new());
        srv.inner.kvm.lock().unwrap().claim("PEER-K");
        let out = srv
            .handle_control_from("PEER-K", &serde_json::json!({"t": "kvm_release"}))
            .expect("reply");
        assert_eq!(out["t"], "kvm_released");
        assert!(!srv.kvm_is_open());
        let out = srv
            .handle_control_from("PEER-K", &serde_json::json!({"t": "kvm_release"}))
            .expect("reply");
        assert_eq!(out["t"], "kvm_released");
        assert!(srv
            .handle_control_from("PEER-K", &serde_json::json!({"t": "bogus"}))
            .is_none());
    }
}
