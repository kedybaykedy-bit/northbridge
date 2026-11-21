use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use nb_mirror::{FRAME_OPEN_KIND, FrameMeta, MirrorServer, build_header};
use nb_transport::tls;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

fn test_port(offset: u16) -> u16 {
    45400 + (std::process::id() as u16 % 300) + offset
}

fn temp_identity(name: &str, port: u16) -> (Identity, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "nb-mirror-test-{}-{}",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let id = Identity::load_or_generate(&dir, Some(name.into()), Some(port)).unwrap();
    (id, dir)
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
async fn frame_open_ack_then_synthetic_nbm1() {
    let (srv_id, srv_dir) = temp_identity("srv", test_port(0));
    let frame_port = srv_id.port + 4;
    let server = MirrorServer::start(srv_id.clone(), nb_transport::InputRegistry::new());
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (cli_id, cli_dir) = temp_identity("cli", test_port(1));
    let port = server.open(&cli_id.fingerprint, 1080, 30);
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

    let meta = FrameMeta {
        seq: 0,
        ts_ms: 1795142400123,
        width: 320,
        height: 240,
        flags: 0,
    };
    let payload = vec![0xABu8; 256];
    let header = build_header(&meta, payload.len() as u32);
    writer.write_all(&header).await.unwrap();
    writer.write_all(&payload).await.unwrap();
    writer.flush().await.unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if server.frames_received() >= 1 {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("synthetic NBM1 frame was not counted");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(server.seq_gaps(), 0);

    drop(writer);
    drop(reader);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if !server.is_open() {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("frame EOF did not mark session idle");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&cli_dir);
}

#[tokio::test]
async fn wrong_fingerprint_is_rejected() {
    let (srv_id, srv_dir) = temp_identity("srv2", test_port(100));
    let server = MirrorServer::start(srv_id.clone(), nb_transport::InputRegistry::new());
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (allowed_id, allowed_dir) = temp_identity("allowed", test_port(101));
    let (denied_id, denied_dir) = temp_identity("denied", test_port(102));
    server.open(&allowed_id.fingerprint, 1080, 30);

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
