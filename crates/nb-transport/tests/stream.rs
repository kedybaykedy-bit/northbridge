use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use nb_transport::{INPUT_OPEN_KIND, InputRegistry, start_packet_server, tls};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

fn test_port(offset: u16) -> u16 {
    40000 + (std::process::id() as u16 % 9000) + offset
}

fn temp_identity(name: &str, port: u16) -> (Identity, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "nb-stream-test-{}-{}",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let id = Identity::load_or_generate(&dir, Some(name.into()), Some(port)).unwrap();
    (id, dir)
}

async fn read_line(
    reader: &mut BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
) -> NetworkPacket {
    let mut line = String::new();
    let n = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
        .await
        .expect("timed out waiting for a packet line")
        .expect("read line");
    assert!(n > 0, "server closed the stream");
    NetworkPacket::from_line(&line).expect("valid packet JSON")
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
async fn stream_open_ping_push_and_close() {
    let (srv, srv_dir) = temp_identity("srv", test_port(0));
    let packet_port = srv.packet_port();
    let registry = InputRegistry::new();
    let reg_for_server = registry.clone();

    let handler: nb_transport::PacketHandler = Arc::new(
        move |_fp, _channel, _addr, pkt: NetworkPacket| {
            if pkt.kind == "nb.touchpad"
                && pkt.body.get("t").and_then(|v| v.as_str()) == Some("ping")
            {
                return Some(NetworkPacket::new("nb.touchpad", json!({"t": "pong"})));
            }
            None
        },
    );

    let server_identity = srv.clone();
    tokio::spawn(async move {
        let _ = start_packet_server(server_identity, handler, reg_for_server).await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (cli, cli_dir) = temp_identity("cli", test_port(1));
    let cfg = tls::client_config(&cli, None).unwrap();
    let connector = TlsConnector::from(cfg);
    let stream = TcpStream::connect(("127.0.0.1", packet_port)).await.unwrap();
    let name = tls::server_name("127.0.0.1").unwrap();
    let tls_stream = connector.connect(name, stream).await.unwrap();
    let (read_half, write_half) = tokio::io::split(tls_stream);
    let mut reader = BufReader::new(read_half);
    let mut writer = write_half;

    let open = NetworkPacket::new(INPUT_OPEN_KIND, json!({}));
    write_line(&mut writer, &open).await;
    let ack = read_line(&mut reader).await;
    assert_eq!(ack.kind, INPUT_OPEN_KIND);
    assert_eq!(ack.body["ok"], true);
    assert!(registry.has(&cli.fingerprint), "stream registered on open");

    let ping = NetworkPacket::new("nb.touchpad", json!({"t": "ping"}));
    write_line(&mut writer, &ping).await;
    let pong = read_line(&mut reader).await;
    assert_eq!(pong.kind, "nb.touchpad");
    assert_eq!(pong.body["t"], "pong");

    let pushed = NetworkPacket::new("nb.kvm", json!({"t": "enter", "x": 0.0, "y": 0.5}));
    assert!(registry.send(&cli.fingerprint, pushed.clone()));
    let got = read_line(&mut reader).await;
    assert_eq!(got.kind, "nb.kvm");
    assert_eq!(got.body["t"], "enter");
    assert_eq!(got.body["y"], 0.5);

    drop(writer);
    drop(reader);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !registry.has(&cli.fingerprint),
        "stream deregistered after close"
    );

    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&cli_dir);
}
