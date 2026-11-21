use anyhow::{Context, Result, bail};
use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use nb_transport::{INPUT_OPEN_KIND, tls};
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
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
            "incoming": ["nb.kvm", "nb.screen", "nb.audio", "nb.mirror"],
            "outgoing": ["nb.touchpad", "nb.screen", "nb.audio", "nb.mirror"],
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

    println!("[7] touchpad rel_move injection (not capturing)…");
    let _ = std::process::Command::new("xdotool")
        .args(["mousemove", "600", "400"])
        .output();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let before = mouse();
    send(
        &mut w,
        "nb.touchpad",
        json!({"t": "rel_move", "dx": 90.0, "dy": 45.0}),
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after = mouse();
    println!("  before={before:?} after={after:?} (expect +90,+45)");
    if before.0 >= 0 && after.0 >= 0 {
        let ddx = after.0 - before.0;
        let ddy = after.1 - before.1;
        if (90 - ddx).abs() > 4 || (45 - ddy).abs() > 4 {
            bail!("rel_move injection off: got delta ({ddx},{ddy})");
        }
    }

    println!("[8] screen stream (nb.screen open → NBS1 frames)…");
    send(
        &mut w,
        "nb.screen",
        json!({"t": "open", "max_w": 1280, "fps": 12}),
    )
    .await?;
    let opened = recv_until(&mut r, Duration::from_secs(5), "screen opened", |p| {
        p.kind == "nb.screen" && p.body.get("t").and_then(|v| v.as_str()) == Some("opened")
    })
    .await?;
    let frame_port = opened.body.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    println!("  frame port {frame_port}");

    let (mut fr, mut fw) = connect(&id, host, frame_port).await?;
    send(&mut fw, "nb.screen.frame.open", json!({})).await?;
    recv_until(&mut fr, Duration::from_secs(10), "frame ack", |p| {
        p.kind == "nb.screen.frame.open"
            && p.body.get("ok").and_then(|v| v.as_bool()) == Some(true)
    })
    .await?;

    for _ in 0..2u32 {
        let mut hdr = [0u8; 28];
        tokio::time::timeout(
            Duration::from_secs(5),
            AsyncReadExt::read_exact(&mut fr, &mut hdr),
        )
        .await
        .context("frame header timeout")??;
        assert_eq!(&hdr[0..4], &b"NBS1"[..], "frame magic");
        let seq = u32::from_be_bytes([hdr[8], hdr[9], hdr[10], hdr[11]]);
        let wpx = u16::from_be_bytes([hdr[20], hdr[21]]);
        let hpx = u16::from_be_bytes([hdr[22], hdr[23]]);
        let len = u32::from_be_bytes([hdr[24], hdr[25], hdr[26], hdr[27]]) as usize;
        assert!(len > 0 && len <= 8 * 1024 * 1024, "frame len {len}");
        let mut payload = vec![0u8; len];
        tokio::time::timeout(
            Duration::from_secs(5),
            AsyncReadExt::read_exact(&mut fr, &mut payload),
        )
        .await
        .context("frame payload timeout")??;
        assert_eq!(&payload[..2], &[0xFF, 0xD8], "jpeg SOI");
        println!("  frame seq={seq} {wpx}x{hpx} {len}B jpeg");
    }

    send(&mut w, "nb.screen", json!({"t": "release"})).await?;
    recv_until(&mut r, Duration::from_secs(5), "screen closed", |p| {
        p.kind == "nb.screen" && p.body.get("t").and_then(|v| v.as_str()) == Some("closed")
    })
    .await?;
    drop(fw);
    drop(fr);

    println!("[9] audio session (nb.audio → NBA1 PCM frame)…");
    send(&mut w, "nb.audio", json!({"t": "open", "codec": "pcm"})).await?;
    let opened = recv_until(&mut r, Duration::from_secs(5), "audio opened", |p| {
        p.kind == "nb.audio" && p.body.get("t").and_then(|v| v.as_str()) == Some("opened")
    })
    .await?;
    let audio_port = opened.body.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    println!("  audio port {audio_port} codec {:?}",
        opened.body.get("codec").and_then(|v| v.as_str()));

    let (mut ar, mut aw) = connect(&id, host, audio_port).await?;
    send(&mut aw, "nb.audio.open", json!({})).await?;
    recv_until(&mut ar, Duration::from_secs(5), "audio ack", |p| {
        p.kind == "nb.audio.open"
            && p.body.get("ok").and_then(|v| v.as_bool()) == Some(true)
    })
    .await?;
    let mut audio = b"NBA1".to_vec();
    audio.push(1);
    audio.push(1);
    audio.push(0x02);
    audio.push(2);
    audio.extend_from_slice(&0u32.to_le_bytes());
    audio.extend_from_slice(&0u64.to_le_bytes());
    audio.extend_from_slice(&3840u32.to_le_bytes());
    audio.extend_from_slice(&[0u8; 3840]);
    AsyncWriteExt::write_all(&mut aw, &audio).await?;
    println!("  sent 1×3840B silence frame (phone→desk)");
    tokio::time::sleep(Duration::from_millis(400)).await;

    send(&mut w, "nb.audio", json!({"t": "release"})).await?;
    recv_until(&mut r, Duration::from_secs(5), "audio closed", |p| {
        p.kind == "nb.audio" && p.body.get("t").and_then(|v| v.as_str()) == Some("closed")
    })
    .await?;
    drop(aw);
    drop(ar);

    println!("[10] mirror session (nb.mirror → NBM1 frame)…");
    send(
        &mut w,
        "nb.mirror",
        json!({"t": "open", "fps": 30, "max_w": 1080}),
    )
    .await?;
    let opened = recv_until(&mut r, Duration::from_secs(5), "mirror opened", |p| {
        p.kind == "nb.mirror" && p.body.get("t").and_then(|v| v.as_str()) == Some("opened")
    })
    .await?;
    let mirror_port = opened.body.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
    println!("  mirror port {mirror_port}");

    let (mut mr, mut mw) = connect(&id, host, mirror_port).await?;
    send(&mut mw, "nb.mirror.frame.open", json!({})).await?;
    recv_until(&mut mr, Duration::from_secs(5), "mirror frame ack", |p| {
        p.kind == "nb.mirror.frame.open"
            && p.body.get("ok").and_then(|v| v.as_bool()) == Some(true)
    })
    .await?;
    let mut mirror = b"NBM1".to_vec();
    mirror.push(1);
    mirror.push(1);
    mirror.extend_from_slice(&0u16.to_be_bytes());
    mirror.extend_from_slice(&0u32.to_be_bytes());
    mirror.extend_from_slice(&0u64.to_be_bytes());
    mirror.extend_from_slice(&1080u16.to_be_bytes());
    mirror.extend_from_slice(&1920u16.to_be_bytes());
    mirror.extend_from_slice(&4u32.to_be_bytes());
    mirror.extend_from_slice(&[0u8; 4]);
    AsyncWriteExt::write_all(&mut mw, &mirror).await?;
    println!("  sent 1×NBM1 frame (junk payload tolerated by decoder)");
    tokio::time::sleep(Duration::from_millis(400)).await;

    send(&mut w, "nb.mirror", json!({"t": "release"})).await?;
    recv_until(&mut r, Duration::from_secs(5), "mirror closed", |p| {
        p.kind == "nb.mirror" && p.body.get("t").and_then(|v| v.as_str()) == Some("closed")
    })
    .await?;
    drop(mw);
    drop(mr);

    println!("FAKE PHONE OK — KVM + touchpad + screen + audio + mirror verified");
    Ok(())
}
