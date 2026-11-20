//! The northbridge packet channel: mTLS TCP + newline-delimited NetworkPacket.
//!
//! One request → one response → connection close (M0 model). The server reads
//! the peer fingerprint from the TLS handshake; the client pins the server
//! fingerprint. Pairing and plugin traffic ride here.

use anyhow::{Context, Result};
use nb_core::channel::Channel;
use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::tls;

/// Max packet line: 1 MiB (KDE Connect allows 32 MiB; we start conservative).
const MAX_LINE: usize = 1024 * 1024;
const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// What the server does with an incoming packet. Returns the response to send
/// back (or `None` to close without a response).
pub type PacketHandler = Arc<
    dyn Fn(&str /*peer_fp*/, Channel, &SocketAddr, NetworkPacket) -> Option<NetworkPacket>
        + Send
        + Sync,
>;

/// Serve the packet channel on `0.0.0.0:identity.packet_port()`.
/// Runs until the process exits (accept loop; per-connection tasks).
pub async fn start_packet_server(identity: Identity, handler: PacketHandler) -> Result<()> {
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
        tokio::spawn(async move {
            if let Err(e) = serve_one(acceptor, stream, remote, handler).await {
                // Peer disconnects mid-handshake are routine; keep the noise low.
                let msg = format!("{e:#}");
                if !msg.contains("early eof") && !msg.contains("connection reset") {
                    eprintln!("[packet] {remote}: {msg}");
                }
            }
        });
    }
}

async fn serve_one(
    acceptor: TlsAcceptor,
    stream: TcpStream,
    remote: SocketAddr,
    handler: PacketHandler,
) -> Result<()> {
    let tls_stream = acceptor.accept(stream).await.context("TLS handshake")?;
    let peer_fp = {
        let (_, conn) = tls_stream.get_ref();
        tls::server_peer_fingerprint(conn)
    }
    .context("no client certificate presented")?;

    let (read_half, mut write_half) = tokio::io::split(tls_stream);
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .await
        .context("read packet line")?;
    if n == 0 {
        return Ok(()); // connected but said nothing
    }
    if line.len() > MAX_LINE {
        anyhow::bail!("packet too large ({} bytes)", line.len());
    }

    let pkt = NetworkPacket::from_line(&line).context("malformed packet JSON")?;
    let channel = Channel::classify(&remote.ip().to_string());

    if let Some(resp) = handler(&peer_fp, channel, &remote, pkt) {
        let mut out = resp.to_line();
        out.push('\n');
        write_half.write_all(out.as_bytes()).await.context("write response")?;
        write_half.flush().await.ok();
    }
    Ok(())
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
    let n = tokio::time::timeout(
        DIAL_TIMEOUT,
        reader.read_line(&mut line),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timeout waiting for response from {host}"))?
    .context("read response")?;
    if n == 0 {
        anyhow::bail!("peer closed without responding");
    }
    NetworkPacket::from_line(&line).context("malformed response JSON")
}

/// Read helper exported for tests/tools.
pub async fn read_packet<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<NetworkPacket>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).await.context("read packet line")?;
    if n == 0 {
        return Ok(None);
    }
    Ok(NetworkPacket::from_line(&line))
}
