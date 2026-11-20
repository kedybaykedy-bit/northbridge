use anyhow::{Context, Result, bail};
use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use nb_transport::{INPUT_OPEN_KIND, tls};
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

type Read = BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>;
type Write = tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>;

async fn connect(id: &Identity, host: &str, port: u16) -> Result<(Read, Write)> {
    let cfg = tls::client_config(id, None)?;
    let connector = TlsConnector::from(cfg);
    let stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect((host, port)))
        .await
        .context("connect timeout")??;
    let name = tls::server_name(host)?;
    let tls_stream = connector.connect(name, stream).await.context("TLS handshake")?;
    let (r, w) = tokio::io::split(tls_stream);
    Ok((BufReader::new(r), w))
}

async fn send(w: &mut Write, kind: &str, body: serde_json::Value) -> Result<()> {
    let pkt = NetworkPacket::new(kind, body);
    let mut out = pkt.to_line();
    out.push('\n');
    w.write_all(out.as_bytes()).await?;
    w.flush().await?;
    Ok(())
}

async fn recv_until(
    r: &mut Read,
    timeout: Duration,
    what: &str,
    pred: impl Fn(&NetworkPacket) -> bool,
) -> Result<NetworkPacket> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            bail!("timed out waiting for {what}");
        }
        let mut line = String::new();
        let n = tokio::time::timeout(left, r.read_line(&mut line))
            .await
            .with_context(|| format!("timeout waiting for {what}"))??;
        if n == 0 {
            bail!("server closed while waiting for {what}");
        }
        let pkt = NetworkPacket::from_line(&line).context("malformed line")?;
        println!("  <- {} {}", pkt.kind, pkt.body);
        if pred(&pkt) {
            return Ok(pkt);
        }
    }
}

fn mouse() -> (i32, i32) {
    let Ok(out) = std::process::Command::new("xdotool")
        .args(["getmouselocation", "--shell"])
        .output()
    else {
        return (-1, -1);
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut x = -1;
    let mut y = -1;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("X=") {
            x = v.trim().parse().unwrap_or(-1);
        }
        if let Some(v) = line.strip_prefix("Y=") {
            y = v.trim().parse().unwrap_or(-1);
        }
    }
    (x, y)
}

fn move_by(dx: i32, dy: i32) {
    let _ = std::process::Command::new("xdotool")
        .args(["mousemove_relative", "--", &dx.to_string(), &dy.to_string()])
        .status();
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let host = args.get(1).map(String::as_str).unwrap_or("127.0.0.1");
    let port: u16 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(53318);

    let dir = std::env::temp_dir().join("nb-fake-phone");
    std::fs::create_dir_all(&dir)?;
    let id = Identity::load_or_generate(&dir, Some("fake-phone".into()), None)?;
    println!("fake-phone fp: {}", id.fingerprint);

    println!("[1] pairing with {host}:{port}…");
    let (mut pr, mut pw) = connect(&id, host, port).await?;
    send(
        &mut pw,
        "nb.pair",
        json!({
            "protocol": nb_core::packet::PROTOCOL_VERSION,
            "alias": "fake-phone",
            "timestamp": nb_core::packet::now_ms(),
            "incoming": ["nb.kvm"],
            "outgoing": ["nb.touchpad"],
        }),
    )
    .await?;
    let resp = recv_until(&mut pr, Duration::from_secs(5), "pair response", |p| {
        p.kind == "nb.pair"
    })
    .await?;
    if resp.body.get("accepted").and_then(|v| v.as_bool()) != Some(true) {
        bail!("pairing rejected: {}", resp.body);
    }
    println!(
        "  PAIRED — desktop caps in={:?} out={:?}",
        resp.body.get("incoming"),
        resp.body.get("outgoing")
    );
    drop(pr);
    drop(pw);

    println!("[2] opening input stream…");
    let (mut r, mut w) = connect(&id, host, port).await?;
    send(&mut w, INPUT_OPEN_KIND, json!({})).await?;
    recv_until(&mut r, Duration::from_secs(5), "stream ack", |p| {
        p.kind == INPUT_OPEN_KIND
    })
    .await?;

    println!("[3] ping…");
    send(&mut w, "nb.touchpad", json!({"t": "ping"})).await?;
    recv_until(&mut r, Duration::from_secs(5), "pong", |p| {
        p.kind == "nb.touchpad" && p.body.get("t").and_then(|v| v.as_str()) == Some("pong")
    })
    .await?;

    println!("[4] capture (KVM)…");
    send(&mut w, "nb.touchpad", json!({"t": "capture"})).await?;
    recv_until(&mut r, Duration::from_secs(5), "enter", |p| {
        p.kind == "nb.kvm" && p.body.get("t").and_then(|v| v.as_str()) == Some("enter")
    })
    .await?;

    println!("[5] nudging the PC pointer with xdotool…");
    let before = mouse();
    tokio::time::sleep(Duration::from_millis(300)).await;
    move_by(70, 35);
    recv_until(&mut r, Duration::from_secs(5), "move", |p| {
        p.kind == "nb.kvm" && p.body.get("t").and_then(|v| v.as_str()) == Some("move")
    })
    .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after = mouse();
    println!("  pointer before={before:?} after={after:?} (expect ~warped back to anchor)");

    println!("[6] release…");
    send(&mut w, "nb.touchpad", json!({"t": "release"})).await?;
    recv_until(&mut r, Duration::from_secs(5), "leave", |p| {
        p.kind == "nb.kvm" && p.body.get("t").and_then(|v| v.as_str()) == Some("leave")
    })
    .await?;

    println!("FAKE PHONE OK — full KVM handshake verified");
    Ok(())
}
