use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use nb_core::identity::Identity;
use nb_core::packet::{NetworkPacket, now_ms};
use nb_transport::tls;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::capture::{self, Grabber, Strategy};
use crate::control::{DEFAULT_FPS, DEFAULT_MAX_W, FRAME_OPEN_KIND};
use crate::frame::{self, FrameMeta};
use crate::queue::{FrameQueue, QueuedFrame};

pub const JPEG_QUALITY: u8 = 70;
pub const QUEUE_CAP: usize = 2;
pub const MAX_LINE: usize = 65536;

struct SessionState {
    peer_fp: Option<String>,
    max_w: u16,
    fps: u8,
    generation: u64,
}

struct LiveSlot {
    generation: u64,
    queue: Arc<Mutex<FrameQueue>>,
}

struct ServerInner {
    frame_port: u16,
    tls: Option<Arc<rustls::ServerConfig>>,
    session: Mutex<SessionState>,
    live: Mutex<Option<LiveSlot>>,
}

#[derive(Clone)]
pub struct ScreenServer {
    inner: Arc<ServerInner>,
}

impl ScreenServer {
    pub fn start(identity: Identity) -> ScreenServer {
        let frame_port = identity.port.saturating_add(2);
        let tls_cfg = match tls::server_config(&identity) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                eprintln!("[screen] TLS config: {e:#}");
                None
            }
        };
        let inner = Arc::new(ServerInner {
            frame_port,
            tls: tls_cfg,
            session: Mutex::new(SessionState {
                peer_fp: None,
                max_w: DEFAULT_MAX_W,
                fps: DEFAULT_FPS,
                generation: 0,
            }),
            live: Mutex::new(None),
        });
        let worker = inner.clone();
        let _ = std::thread::Builder::new()
            .name("nb-screen".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("[screen] runtime: {e:#}");
                        return;
                    }
                };
                rt.block_on(accept_loop(worker));
            });
        ScreenServer { inner }
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

    pub fn inject_frame(&self, data: Vec<u8>, width: u16, height: u16) -> bool {
        let live = self.inner.live.lock().unwrap();
        match live.as_ref() {
            Some(slot) => {
                slot.queue.lock().unwrap().push(QueuedFrame {
                    width,
                    height,
                    data,
                });
                true
            }
            None => false,
        }
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
            eprintln!("[screen] bind frame port {}: {e:#}", inner.frame_port);
            return;
        }
    };
    loop {
        let (stream, remote) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(e) => {
                eprintln!("[screen] accept: {e:#}");
                continue;
            }
        };
        let worker = inner.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_one(worker, acceptor, stream).await {
                let msg = format!("{e:#}");
                if !msg.contains("early eof") && !msg.contains("connection reset") {
                    eprintln!("[screen] {remote}: {msg}");
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
            Some((session.generation, session.max_w, session.fps))
        } else {
            None
        }
    };
    let Some((generation, max_w, fps)) = gate else {
        eprintln!("[screen] rejected frame connection from {peer_fp}");
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

    let queue = Arc::new(Mutex::new(FrameQueue::new(QUEUE_CAP)));
    {
        let mut live = inner.live.lock().unwrap();
        *live = Some(LiveSlot {
            generation,
            queue: queue.clone(),
        });
    }
    let ack = NetworkPacket::new(FRAME_OPEN_KIND, json!({"ok": true}));
    let mut out = ack.to_line();
    out.push('\n');
    write_half
        .write_all(out.as_bytes())
        .await
        .context("write frame open ack")?;
    write_half.flush().await.ok();

    spawn_capture(inner.clone(), queue.clone(), peer_fp.clone(), generation, max_w, fps);

    let server = ScreenServer {
        inner: inner.clone(),
    };
    let mut seq: u32 = 0;
    let mut probe = [0u8; 1];
    loop {
        if !server.matches(generation, &peer_fp) {
            break;
        }
        let frame = queue.lock().unwrap().take_newest();
        if let Some(frame) = frame {
            let meta = FrameMeta {
                seq,
                ts_ms: now_ms(),
                width: frame.width,
                height: frame.height,
                flags: 0,
            };
            let header = frame::build_header(&meta, frame.data.len() as u32);
            write_half
                .write_all(&header)
                .await
                .context("write frame header")?;
            write_half
                .write_all(&frame.data)
                .await
                .context("write frame payload")?;
            write_half.flush().await.ok();
            seq = seq.wrapping_add(1);
            tokio::select! {
                _ = reader.read_exact(&mut probe) => break,
                _ = tokio::time::sleep(Duration::from_millis(1)) => {}
            }
        } else {
            tokio::select! {
                _ = reader.read_exact(&mut probe) => break,
                _ = tokio::time::sleep(Duration::from_millis(5)) => {}
            }
        }
    }
    {
        let mut live = inner.live.lock().unwrap();
        if live.as_ref().map(|slot| slot.generation) == Some(generation) {
            *live = None;
        }
    }
    write_half.shutdown().await.ok();
    Ok(())
}

fn spawn_capture(
    inner: Arc<ServerInner>,
    queue: Arc<Mutex<FrameQueue>>,
    peer_fp: String,
    generation: u64,
    max_w: u16,
    fps: u8,
) {
    let _ = std::thread::Builder::new()
        .name("nb-screen-capture".into())
        .spawn(move || {
            let grabber = match Grabber::connect(Strategy::Auto) {
                Ok(grabber) => grabber,
                Err(e) => {
                    eprintln!("[screen] no X display: {e:#}");
                    return;
                }
            };
            let (w, h) = grabber.size();
            eprintln!("[screen] capture {w}x{h} shm={}", grabber.shm());
            let server = ScreenServer {
                inner: inner.clone(),
            };
            let interval = Duration::from_secs_f32(1.0 / f32::from(fps.max(1)));
            loop {
                if !server.matches(generation, &peer_fp) {
                    break;
                }
                let tick = std::time::Instant::now();
                match capture::grab_scaled(&grabber, max_w)
                    .and_then(|(w, h, bgra)| {
                        capture::to_jpeg_bgra(&bgra, w, h, JPEG_QUALITY)
                            .map(|jpeg| (w, h, jpeg))
                    }) {
                    Ok((w, h, jpeg)) => {
                        queue.lock().unwrap().push(QueuedFrame {
                            width: w as u16,
                            height: h as u16,
                            data: jpeg,
                        });
                    }
                    Err(e) => eprintln!("[screen] capture: {e:#}"),
                }
                let elapsed = tick.elapsed();
                if elapsed < interval {
                    std::thread::sleep(interval - elapsed);
                }
            }
        });
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
            port: 44800 + (std::process::id() as u16 % 500),
        };
        let srv = ScreenServer::start(id);
        assert!(!srv.is_open());
        let port = srv.open("PEER-A", 1280, 12);
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
