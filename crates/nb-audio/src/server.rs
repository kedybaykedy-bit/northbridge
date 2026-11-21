use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use nb_core::identity::Identity;
use nb_core::packet::{NetworkPacket, now_ms};
use nb_transport::tls;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::capture::{AudioSource, open_source, unavailable_hint};
use crate::codec::{Codec, decode_frame, encode_frame};
use crate::control::AUDIO_OPEN_KIND;
use crate::frame::{self, Direction, FrameMeta, direction_of};
use crate::playback::{AudioSink, NullSink};
#[cfg(feature = "cpal-out")]
use crate::playback::CpalSink;
use crate::queue::{AUDIO_QUEUE_CAP, AudioQueue, QueuedAudio};

pub const PORT_OFFSET: u16 = 3;
pub const MAX_LINE: usize = 65536;
pub const UPLINK_CAP: usize = 3;

struct SessionState {
    peer_fp: Option<String>,
    codec: Codec,
    generation: u64,
}

struct LiveSlot {
    generation: u64,
    downlink: Arc<Mutex<AudioQueue>>,
}

struct ServerInner {
    audio_port: u16,
    tls: Option<Arc<rustls::ServerConfig>>,
    session: Mutex<SessionState>,
    live: Mutex<Option<LiveSlot>>,
    uplink: Mutex<VecDeque<Vec<i16>>>,
    uplink_count: Mutex<u64>,
    last_uplink: Mutex<Option<Vec<i16>>>,
}

#[derive(Clone)]
pub struct AudioServer {
    inner: Arc<ServerInner>,
}

fn log_capture_hint_once() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        eprintln!("[audio] {}", unavailable_hint());
    });
}

impl AudioServer {
    pub fn start(identity: Identity) -> AudioServer {
        let audio_port = identity.port.saturating_add(PORT_OFFSET);
        let tls_cfg = match tls::server_config(&identity) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                eprintln!("[audio] TLS config: {e:#}");
                None
            }
        };
        let inner = Arc::new(ServerInner {
            audio_port,
            tls: tls_cfg,
            session: Mutex::new(SessionState {
                peer_fp: None,
                codec: crate::codec::default_codec(),
                generation: 0,
            }),
            live: Mutex::new(None),
            uplink: Mutex::new(VecDeque::new()),
            uplink_count: Mutex::new(0),
            last_uplink: Mutex::new(None),
        });
        let worker = inner.clone();
        let _ = std::thread::Builder::new()
            .name("nb-audio".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("[audio] runtime: {e:#}");
                        return;
                    }
                };
                rt.block_on(accept_loop(worker));
            });
        spawn_playback(inner.clone());
        AudioServer { inner }
    }

    pub fn open(&self, peer_fp: &str, codec: Codec) -> u16 {
        let mut session = self.inner.session.lock().unwrap();
        session.peer_fp = Some(peer_fp.to_string());
        session.codec = codec;
        session.generation = session.generation.wrapping_add(1);
        self.inner.audio_port
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

    pub fn audio_port(&self) -> u16 {
        self.inner.audio_port
    }

    pub fn session_peer(&self) -> Option<String> {
        self.inner.session.lock().unwrap().peer_fp.clone()
    }

    pub fn session_codec(&self) -> Codec {
        self.inner.session.lock().unwrap().codec
    }

    pub fn inject_frame(&self, payload: Vec<u8>, codec: u8, flags: u8, channels: u8) -> bool {
        let live = self.inner.live.lock().unwrap();
        match live.as_ref() {
            Some(slot) => {
                slot.downlink.lock().unwrap().push(QueuedAudio {
                    codec,
                    flags,
                    channels,
                    payload,
                });
                true
            }
            None => false,
        }
    }

    pub fn uplink_received(&self) -> u64 {
        *self.inner.uplink_count.lock().unwrap()
    }

    pub fn take_last_uplink(&self) -> Option<Vec<i16>> {
        self.inner.last_uplink.lock().unwrap().clone()
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

fn spawn_playback(inner: Arc<ServerInner>) {
    let _ = std::thread::Builder::new()
        .name("nb-audio-playback".into())
        .spawn(move || {
            #[cfg(feature = "cpal-out")]
            let mut sink: Box<dyn AudioSink> = match CpalSink::open() {
                Ok(s) => Box::new(s),
                Err(e) => {
                    eprintln!("[audio] cpal sink unavailable: {e:#}");
                    Box::new(NullSink::new())
                }
            };
            #[cfg(not(feature = "cpal-out"))]
            let mut sink: Box<dyn AudioSink> = Box::new(NullSink::new());
            eprintln!("[audio] playback sink: {}", sink.sink_name());
            loop {
                std::thread::sleep(Duration::from_millis(crate::codec::FRAME_MS));
                let next = {
                    let mut uplink = inner.uplink.lock().unwrap();
                    while uplink.len() > 1 {
                        uplink.pop_front();
                    }
                    uplink.pop_front()
                };
                if let Some(pcm) = next {
                    sink.push(&pcm);
                }
            }
        });
}

async fn accept_loop(inner: Arc<ServerInner>) {
    let tls_cfg = match inner.tls.clone() {
        Some(cfg) => cfg,
        None => return,
    };
    let acceptor = TlsAcceptor::from(tls_cfg);
    let listener = match TcpListener::bind(("0.0.0.0", inner.audio_port)).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("[audio] bind audio port {}: {:#}", inner.audio_port, e);
            return;
        }
    };
    loop {
        let (stream, remote) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(e) => {
                eprintln!("[audio] accept: {e:#}");
                continue;
            }
        };
        let worker = inner.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_one(worker, acceptor, stream).await {
                let msg = format!("{e:#}");
                if !msg.contains("early eof") && !msg.contains("connection reset") {
                    eprintln!("[audio] {remote}: {msg}");
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
            Some((session.generation, session.codec))
        } else {
            None
        }
    };
    let Some((generation, codec)) = gate else {
        eprintln!("[audio] rejected audio connection from {peer_fp}");
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
    let n = reader.read_line(&mut line).await.context("read audio open")?;
    if n == 0 {
        return Ok(());
    }
    if line.len() > MAX_LINE {
        anyhow::bail!("audio open too large ({} bytes)", line.len());
    }
    let pkt = NetworkPacket::from_line(&line).context("malformed audio open")?;
    if pkt.kind != AUDIO_OPEN_KIND {
        anyhow::bail!("unexpected audio open kind: {}", pkt.kind);
    }
    if !inner
        .session
        .lock()
        .map(|s| s.generation == generation && s.peer_fp.as_deref() == Some(peer_fp.as_str()))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let downlink = Arc::new(Mutex::new(AudioQueue::new(AUDIO_QUEUE_CAP)));
    {
        let mut live = inner.live.lock().unwrap();
        *live = Some(LiveSlot {
            generation,
            downlink: downlink.clone(),
        });
    }
    let ack = NetworkPacket::new(AUDIO_OPEN_KIND, json!({"ok": true}));
    let mut out = ack.to_line();
    out.push('\n');
    write_half
        .write_all(out.as_bytes())
        .await
        .context("write audio open ack")?;
    write_half.flush().await.ok();

    let server = AudioServer {
        inner: inner.clone(),
    };
    let mut source: Box<dyn AudioSource> = open_source();
    if source.source_name() == "none" {
        log_capture_hint_once();
    }
    let mut ticker = tokio::time::interval(Duration::from_millis(crate::codec::FRAME_MS));
    let mut seq_out: u32 = 0;
    let mut header = [0u8; crate::frame::HEADER_LEN];
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if !server.matches(generation, &peer_fp) {
                    break;
                }
                if let Some(samples) = source.next_frame() {
                    if let Ok(payload) = encode_frame(codec, &samples) {
                        downlink.lock().unwrap().push(QueuedAudio {
                            codec: codec.as_u8(),
                            flags: 0,
                            channels: crate::codec::CHANNELS as u8,
                            payload,
                        });
                    }
                }
                let queued = downlink.lock().unwrap().take_newest();
                if let Some(queued) = queued {
                    let meta = FrameMeta {
                        seq: seq_out,
                        pts_ms: now_ms(),
                        codec: queued.codec,
                        flags: queued.flags & !crate::frame::FLAG_DIR,
                        channels: queued.channels,
                    };
                    let bytes = frame::build_header(&meta, queued.payload.len() as u32);
                    write_half.write_all(&bytes).await.context("write audio header")?;
                    write_half.write_all(&queued.payload).await.context("write audio payload")?;
                    write_half.flush().await.ok();
                    seq_out = seq_out.wrapping_add(1);
                }
            }
            res = reader.read_exact(&mut header) => {
                if res.is_err() {
                    break;
                }
                let (meta, len) = match frame::parse_header(&header) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        eprintln!("[audio] {peer_fp}: frame parse: {e:#}");
                        break;
                    }
                };
                if !server.matches(generation, &peer_fp) {
                    break;
                }
                let mut payload = vec![0u8; len as usize];
                if reader.read_exact(&mut payload).await.is_err() {
                    break;
                }
                if direction_of(meta.flags) != Direction::PhoneToDesk {
                    continue;
                }
                match decode_frame(meta.codec, &payload) {
                    Ok(pcm) => {
                        let mut uplink = inner.uplink.lock().unwrap();
                        while uplink.len() >= UPLINK_CAP {
                            uplink.pop_front();
                        }
                        uplink.push_back(pcm.clone());
                        *inner.uplink_count.lock().unwrap() += 1;
                        *inner.last_uplink.lock().unwrap() = Some(pcm);
                    }
                    Err(e) => eprintln!("[audio] {peer_fp}: decode: {e:#}"),
                }
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
        let srv = AudioServer::start(id);
        assert!(!srv.is_open());
        let port = srv.open("PEER-A", Codec::Pcm);
        assert_eq!(port, srv.audio_port());
        assert!(srv.is_open());
        assert_eq!(srv.session_codec(), Codec::Pcm);
        srv.open("PEER-B", Codec::Pcm);
        assert_eq!(srv.session_peer().as_deref(), Some("PEER-B"));
        srv.close("PEER-A");
        assert!(srv.is_open());
        srv.close("PEER-B");
        assert!(!srv.is_open());
    }
}
