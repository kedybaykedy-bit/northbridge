use nb_audio::{
    AUDIO_OPEN_KIND, AudioServer, CODEC_PCM, FLAG_DIR, FrameMeta, build_header, direction_of,
    parse_header, pcm_decode, pcm_encode,
};
use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use nb_transport::tls;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

fn test_port(offset: u16) -> u16 {
    45400 + (std::process::id() as u16 % 300) + offset
}

fn temp_identity(name: &str, port: u16) -> (Identity, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "nb-audio-test-{}-{}",
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
async fn audio_open_ack_then_synthetic_pcm_frame() {
    let (srv_id, srv_dir) = temp_identity("srv", test_port(0));
    let audio_port = srv_id.port + 3;
    let server = AudioServer::start(srv_id.clone());
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (cli_id, cli_dir) = temp_identity("cli", test_port(1));
    let port = server.open(&cli_id.fingerprint, nb_audio::Codec::Pcm);
    assert_eq!(port, audio_port);
    assert!(server.is_open());

    let (mut reader, mut writer) = connect_tls(&cli_id, port).await;
    write_line(&mut writer, &NetworkPacket::new(AUDIO_OPEN_KIND, json!({}))).await;
    let mut line = String::new();
    let n = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
        .await
        .expect("ack timeout")
        .expect("read ack");
    assert!(n > 0);
    let ack = NetworkPacket::from_line(&line).expect("ack json");
    assert_eq!(ack.kind, AUDIO_OPEN_KIND);
    assert_eq!(ack.body["ok"], true);

    let samples: Vec<i16> = (0..960 * 2).map(|i| i as i16).collect();
    let payload = pcm_encode(&samples);
    assert!(server.inject_frame(payload.clone(), CODEC_PCM, 0, 2));

    let mut header = [0u8; 24];
    tokio::time::timeout(Duration::from_secs(5), reader.read_exact(&mut header))
        .await
        .expect("frame timeout")
        .expect("read header");
    let (meta, len) = parse_header(&header).expect("header");
    assert_eq!(meta.codec, CODEC_PCM);
    assert_eq!(meta.channels, 2);
    assert_eq!(direction_of(meta.flags), nb_audio::Direction::DeskToPhone);
    assert_eq!(len as usize, payload.len());
    let mut body = vec![0u8; len as usize];
    tokio::time::timeout(Duration::from_secs(5), reader.read_exact(&mut body))
        .await
        .expect("payload timeout")
        .expect("read payload");
    assert_eq!(body, payload);
    assert_eq!(pcm_decode(&body).unwrap(), samples);

    let uplink: Vec<i16> = (0..960 * 2).map(|i| (i as i16).wrapping_mul(-3)).collect();
    let up_meta = FrameMeta {
        seq: 9,
        pts_ms: 1795142400999,
        codec: CODEC_PCM,
        flags: FLAG_DIR,
        channels: 2,
    };
    let up_header = build_header(&up_meta, pcm_encode(&uplink).len() as u32);
    writer.write_all(&up_header).await.unwrap();
    writer.write_all(&pcm_encode(&uplink)).await.unwrap();
    writer.flush().await.unwrap();

    let mut waited = 0;
    while server.uplink_received() == 0 && waited < 50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        waited += 1;
    }
    assert_eq!(server.uplink_received(), 1);
    assert_eq!(server.take_last_uplink().as_deref(), Some(uplink.as_slice()));

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
    let server = AudioServer::start(srv_id.clone());
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (allowed_id, allowed_dir) = temp_identity("allowed", test_port(101));
    let (denied_id, denied_dir) = temp_identity("denied", test_port(102));
    server.open(&allowed_id.fingerprint, nb_audio::Codec::Pcm);

    let (mut reader, mut writer) = connect_tls(&denied_id, server.audio_port()).await;
    write_line(&mut writer, &NetworkPacket::new(AUDIO_OPEN_KIND, json!({}))).await;
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
