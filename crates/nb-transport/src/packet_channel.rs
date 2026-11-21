//! The northbridge packet channel: mTLS TCP + newline-delimited NetworkPacket.
//!
//! Two modes, chosen by the first packet on the connection:
//! - classic (M0): one request → one response → close (pair, ping, clipboard).
//! - stream (`nb.touchpad.open`): bidirectional line stream — the phone pushes
//!   touchpad input lines while the desktop pushes `nb.kvm` cursor lines back
//!   on the same connection.

use anyhow::{Context, Result};
use nb_core::channel::Channel;
use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::tls;

/// Max packet line: 1 MiB (KDE Connect allows 32 MiB; we start conservative).
const MAX_LINE: usize = 1024 * 1024;
const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// First line a phone sends to switch the connection into stream mode.
pub const INPUT_OPEN_KIND: &str = "nb.touchpad.open";

/// What the server does with an incoming packet. Returns the response to send
/// back (`None` = no line in stream mode / close without a response in classic
/// mode).
pub type PacketHandler = Arc<
    dyn Fn(&str /*peer_fp*/, Channel, &SocketAddr, NetworkPacket) -> Option<NetworkPacket>
        + Send
        + Sync,
>;

struct StreamHandle {
    conn_id: u64,
    tx: mpsc::UnboundedSender<NetworkPacket>,
}

#[derive(Default)]
struct RegistryInner {
    next_id: u64,
    streams: HashMap<String, StreamHandle>,
}

/// Maps a peer fingerprint to its live input stream. The packet server
/// registers streams here; the KVM watcher uses it to push cursor lines to the
/// phone, and a failed `send` tells it the phone went away.
#[derive(Clone, Default)]
pub struct InputRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

impl InputRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn open(&self, fp: &str) -> (u64, mpsc::UnboundedSender<NetworkPacket>, mpsc::UnboundedReceiver<NetworkPacket>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut inner = self.inner.lock().unwrap();
        inner.next_id += 1;
        let conn_id = inner.next_id;
        inner
            .streams
            .insert(fp.to_string(), StreamHandle { conn_id, tx: tx.clone() });
        (conn_id, tx, rx)
    }

    fn close(&self, fp: &str, conn_id: u64) {
        let mut inner = self.inner.lock().unwrap();
        if inner.streams.get(fp).map(|h| h.conn_id) == Some(conn_id) {
            inner.streams.remove(fp);
        }
    }

    /// Queue `pkt` for the peer's live stream. False when there is no stream
    /// (or its task has already exited).
    pub fn send(&self, fp: &str, pkt: NetworkPacket) -> bool {
        let inner = self.inner.lock().unwrap();
        inner
            .streams
            .get(fp)
            .map(|h| h.tx.send(pkt).is_ok())
            .unwrap_or(false)
    }

    pub fn has(&self, fp: &str) -> bool {
        self.inner.lock().unwrap().streams.contains_key(fp)
    }
}

/// Serve the packet channel on `0.0.0.0:identity.packet_port()`.
/// Runs until the process exits (accept loop; per-connection tasks).
pub async fn start_packet_server(
    identity: Identity,
    handler: PacketHandler,
    registry: InputRegistry,
) -> Result<()> {
    let cfg = tls::server_config(&identity)?;
    let acceptor = TlsAcceptor::from(cfg);
    let listener = TcpListener::bind(("0.0.0.0", identity.packet_port()))
        .await
        .with_context(|| format!("bind packet port {}", identity.packet_port()))?;

    loop {
        let (stream, remote) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("[packet] accept error: {e:#}");
                continue;
            }
        };
        let acceptor = acceptor.clone();
        let handler = handler.clone();
        let registry = registry.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_one(acceptor, stream, remote, handler, registry).await {
                // Peer disconnects mid-handshake are routine; keep the noise low.
                let msg = format!("{e:#}");
                if !msg.contains("early eof") && !msg.contains("connection reset") {
                    eprintln!("[packet] {remote}: {msg}");
                }
            }
        });
    }
}

type ServerTls = tokio_rustls::server::TlsStream<TcpStream>;

async fn serve_one(
    acceptor: TlsAcceptor,
    stream: TcpStream,
    remote: SocketAddr,
    handler: PacketHandler,
    registry: InputRegistry,
) -> Result<()> {
    let tls_stream = acceptor.accept(stream).await.context("TLS handshake")?;
    let peer_fp = {
        let (_, conn) = tls_stream.get_ref();
        tls::server_peer_fingerprint(conn)
    }
    .context("no client certificate presented")?;

    let (read_half, write_half) = tokio::io::split(tls_stream);
    let mut reader = BufReader::new(read_half);

    let first = match read_packet(&mut reader).await? {
        Some(p) => p,
        None => return Ok(()), // connected but said nothing
    };
    let channel = Channel::classify(&remote.ip().to_string());

    if first.kind == INPUT_OPEN_KIND {
        return serve_stream(&peer_fp, channel, &remote, handler, &registry, &mut reader, write_half)
            .await;
    }

    let mut write_half = write_half;
    if let Some(resp) = handler(&peer_fp, channel, &remote, first) {
        let mut out = resp.to_line();
        out.push('\n');
        write_half.write_all(out.as_bytes()).await.context("write response")?;
        write_half.flush().await.ok();
    }
    Ok(())
}

async fn serve_stream(
    peer_fp: &str,
    channel: Channel,
    remote: &SocketAddr,
    handler: PacketHandler,
    registry: &InputRegistry,
    reader: &mut BufReader<tokio::io::ReadHalf<ServerTls>>,
    write_half: tokio::io::WriteHalf<ServerTls>,
) -> Result<()> {
    let (conn_id, tx, rx) = registry.open(peer_fp);
    eprintln!(
        "[input] stream open {} conn={conn_id} from {remote}",
        &peer_fp[..12.min(peer_fp.len())]
    );
    let writer = tokio::spawn(writer_loop(rx, write_half));

    let run = async {
        let ack = NetworkPacket::new(INPUT_OPEN_KIND, json!({"ok": true}));
        tx.send(ack).context("stream closed during open ack")?;

        loop {
            let Some(pkt) = read_packet(reader).await? else {
                break;
            };
            if let Some(resp) = handler(peer_fp, channel, remote, pkt) {
                tx.send(resp).context("stream closed during response")?;
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    registry.close(peer_fp, conn_id);
    drop(tx);
    let _ = writer.await;
    run
}

async fn writer_loop(
    mut rx: mpsc::UnboundedReceiver<NetworkPacket>,
    mut write_half: tokio::io::WriteHalf<ServerTls>,
) {
    while let Some(pkt) = rx.recv().await {
        let mut out = pkt.to_line();
        out.push('\n');
        if write_half.write_all(out.as_bytes()).await.is_err() {
            break;
        }
        if write_half.flush().await.is_err() {
            break;
        }
    }
}

/// Read one line; `Ok(None)` on clean EOF. No timeout: stream connections may
/// legitimately idle for minutes between input events.
pub async fn read_packet<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<NetworkPacket>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).await.context("read packet line")?;
    if n == 0 {
        return Ok(None);
    }
    if line.len() > MAX_LINE {
        anyhow::bail!("packet too large ({} bytes)", line.len());
    }
    NetworkPacket::from_line(&line)
        .map(Some)
        .context("malformed packet JSON")
}

/// Send one packet to `host:packet_port`, wait for the response packet.
/// `expected_fingerprint` must come from discovery (pinned in handshake).
pub async fn send_packet(
    identity: &Identity,
    host: &str,
    packet_port: u16,
    expected_fingerprint: &str,
    pkt: &NetworkPacket,
) -> Result<NetworkPacket> {
    let cfg = tls::client_config(identity, Some(expected_fingerprint.to_string()))?;
    let connector = TlsConnector::from(cfg);

    let stream = tokio::time::timeout(DIAL_TIMEOUT, TcpStream::connect((host, packet_port)))
        .await
        .map_err(|_| anyhow::anyhow!("timeout connecting to {host}:{packet_port}"))?
        .with_context(|| format!("connect {host}:{packet_port}"))?;

    let name = tls::server_name(host)?;
    let tls_stream = connector
        .connect(name, stream)
        .await
        .context("TLS handshake (fingerprint pin failed or peer not speaking nb)")?;

    let (read_half, mut write_half) = tokio::io::split(tls_stream);
    let mut out = pkt.to_line();
    out.push('\n');
    write_half.write_all(out.as_bytes()).await.context("write request")?;
    write_half.flush().await.ok();

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let n = tokio::time::timeout(DIAL_TIMEOUT, reader.read_line(&mut line))
        .await
        .map_err(|_| anyhow::anyhow!("timeout waiting for response from {host}"))?
        .context("read response")?;
    if n == 0 {
        anyhow::bail!("peer closed without responding");
    }
    NetworkPacket::from_line(&line).context("malformed response JSON")
}
