use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use nb_screen::{FRAME_OPEN_KIND, ScreenServer, parse_header};
use nb_transport::tls;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

fn test_port(offset: u16) -> u16 {
    45200 + (std::process::id() as u16 % 300) + offset
}

fn temp_identity(name: &str, port: u16) -> (Identity, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "nb-screen-test-{}-{}",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let id = Identity::load_or_generate(&dir, Some(name.into()), Some(port)).unwrap();
    (id, dir)
}

fn test_jpeg() -> Vec<u8> {
    let w = 64u32;
    let h = 48u32;
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            bgra.push((x * 4) as u8);
            bgra.push((y * 5) as u8);
            bgra.push(128);
            bgra.push(255);
        }
    }
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 70)
        .encode(&bgra, w as u16, h as u16, jpeg_encoder::ColorType::Bgra)
        .unwrap();
    out
}

async fn connect_tls(
    cli: &Identity,
    port: u16,
) -> (
    BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
    tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
) {
    let cfg = tls::client_config(cli, None).unwrap();
    let connector = TlsConnector::from(cfg);
    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let name = tls::server_name("127.0.0.1").unwrap();
    let tls_stream = connector.connect(name, stream).await.unwrap();
    let (read_half, write_half) = tokio::io::split(tls_stream);
    (BufReader::new(read_half), write_half)
}

async fn write_line(
    writer: &mut tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
    pkt: &NetworkPacket,
) {
    let mut out = pkt.to_line();
    out.push('\n');
    writer.write_all(out.as_bytes()).await.unwrap();
    writer.flush().await.unwrap();
}

#[tokio::test]
async fn frame_open_ack_then_synthetic_jpeg() {
    let (srv_id, srv_dir) = temp_identity("srv", test_port(0));
    let frame_port = srv_id.port + 2;
    let server = ScreenServer::start(srv_id.clone());
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (cli_id, cli_dir) = temp_identity("cli", test_port(1));
    let port = server.open(&cli_id.fingerprint, 1280, 12);
    assert_eq!(port, frame_port);
    assert!(server.is_open());

    let (mut reader, mut writer) = connect_tls(&cli_id, port).await;
    write_line(&mut writer, &NetworkPacket::new(FRAME_OPEN_KIND, json!({}))).await;
    let mut line = String::new();
    let n = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
        .await
        .expect("ack timeout")
        .expect("read ack");
    assert!(n > 0);
    let ack = NetworkPacket::from_line(&line).expect("ack json");
    assert_eq!(ack.kind, FRAME_OPEN_KIND);
    assert_eq!(ack.body["ok"], true);

    let jpeg = test_jpeg();
    assert_eq!(&jpeg[0..2], &[0xff, 0xd8]);
    assert!(server.inject_frame(jpeg.clone(), 64, 48));

    let mut header = [0u8; 28];
    tokio::time::timeout(Duration::from_secs(5), reader.read_exact(&mut header))
        .await
        .expect("frame timeout")
        .expect("read header");
    let (meta, len) = parse_header(&header).expect("header");
    assert_eq!(meta.width, 64);
    assert_eq!(meta.height, 48);
    assert_eq!(len as usize, jpeg.len());
    let mut payload = vec![0u8; len as usize];
    tokio::time::timeout(Duration::from_secs(5), reader.read_exact(&mut payload))
        .await
        .expect("payload timeout")
        .expect("read payload");
    assert_eq!(&payload[0..2], &[0xff, 0xd8]);
    assert_eq!(payload, jpeg);

    drop(writer);
    drop(reader);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(server.is_open());

    server.close(&cli_id.fingerprint);
    assert!(!server.is_open());

    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&cli_dir);
}

#[tokio::test]
async fn wrong_fingerprint_is_rejected() {
    let (srv_id, srv_dir) = temp_identity("srv2", test_port(100));
    let server = ScreenServer::start(srv_id.clone());
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (allowed_id, allowed_dir) = temp_identity("allowed", test_port(101));
    let (denied_id, denied_dir) = temp_identity("denied", test_port(102));
    server.open(&allowed_id.fingerprint, 1280, 12);

    let (mut reader, mut writer) = connect_tls(&denied_id, server.frame_port()).await;
    write_line(&mut writer, &NetworkPacket::new(FRAME_OPEN_KIND, json!({}))).await;
    let mut line = String::new();
    let n = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
        .await
        .expect("no hang on reject")
        .expect("read after reject");
    assert_eq!(n, 0);
    assert!(server.is_open());

    server.close(&allowed_id.fingerprint);
    assert!(!server.is_open());

    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&allowed_dir);
    let _ = std::fs::remove_dir_all(&denied_dir);
}
