use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use nb_transport::file::{
    FILE_CHUNK_KIND, FILE_DONE_KIND, FILE_OFFER_KIND, FileSink, chunk_packet, done_packet,
    file_port, offer_packet, send_file_via_packet,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn test_port(offset: u16) -> u16 {
    41000 + (std::process::id() as u16 % 8000) + offset
}

fn temp_identity(name: &str, port: u16) -> (Identity, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "nb-file-test-{}-{}",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let id = Identity::load_or_generate(&dir, Some(name.into()), Some(port)).unwrap();
    (id, dir)
}

fn write_src(dir: &PathBuf, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

#[tokio::test]
async fn packet_file_transfer_roundtrip() {
    let (srv, srv_dir) = temp_identity("srv", test_port(0));
    let srv_fp = srv.fingerprint.clone();
    let sink_dir = std::env::temp_dir().join(format!("nb-file-sink-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sink_dir);
    let sink = Arc::new(Mutex::new(FileSink::new(sink_dir.clone()).unwrap()));
    let handler: nb_transport::PacketHandler = Arc::new(move |_fp, _ch, _addr, pkt: NetworkPacket| {
        match pkt.kind.as_str() {
            FILE_OFFER_KIND | FILE_CHUNK_KIND | FILE_DONE_KIND => {
                sink.lock().unwrap().handle(&pkt)
            }
            _ => None,
        }
    });
    let server_identity = srv.clone();
    tokio::spawn(async move {
        let _ = nb_transport::start_packet_server(
            server_identity,
            handler,
            nb_transport::InputRegistry::new(),
        )
        .await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (cli, cli_dir) = temp_identity("cli", test_port(1));
    let src_dir = std::env::temp_dir().join(format!("nb-file-src-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src_dir);
    std::fs::create_dir_all(&src_dir).unwrap();
    let small = write_src(&src_dir, "hello.txt", b"hello northbridge");
    let big_data: Vec<u8> = (0..(48 * 1024 * 2 + 100)).map(|i| (i % 251) as u8).collect();
    let big = write_src(&src_dir, "big.bin", &big_data);
    let empty = write_src(&src_dir, "empty.txt", b"");

    let got = send_file_via_packet(
        &cli,
        "127.0.0.1",
        srv.packet_port(),
        &srv_fp,
        &[small.clone(), big.clone(), empty.clone()],
        None,
    )
    .await
    .unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(
        std::fs::read(sink_dir.join("hello.txt")).unwrap(),
        b"hello northbridge"
    );
    assert_eq!(std::fs::read(sink_dir.join("big.bin")).unwrap(), big_data);
    assert_eq!(std::fs::read(sink_dir.join("empty.txt")).unwrap(), b"");

    let again = send_file_via_packet(
        &cli,
        "127.0.0.1",
        srv.packet_port(),
        &srv_fp,
        &[small.clone()],
        None,
    )
    .await
    .unwrap();
    assert_eq!(again, vec!["hello (1).txt".to_string()]);
    assert_eq!(
        std::fs::read(sink_dir.join("hello (1).txt")).unwrap(),
        b"hello northbridge"
    );

    let _ = std::fs::remove_dir_all(&srv_dir);
    let _ = std::fs::remove_dir_all(&cli_dir);
    let _ = std::fs::remove_dir_all(&src_dir);
    let _ = std::fs::remove_dir_all(&sink_dir);
}

#[tokio::test]
async fn packet_file_rejects_unknown_transfer() {
    let dir = std::env::temp_dir().join(format!("nb-file-neg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(file_port(53317), 53322);
    let mut sink = FileSink::new(dir.clone()).unwrap();
    let resp = sink
        .handle(&chunk_packet("nope", 0, b"x"))
        .expect("chunk kind must be answered");
    assert_eq!(resp.body.get("ok").and_then(|v| v.as_bool()), Some(false));
    let resp = sink
        .handle(&done_packet("nope", 0, None))
        .expect("done kind must be answered");
    assert_eq!(resp.body.get("ok").and_then(|v| v.as_bool()), Some(false));
    let resp = sink
        .handle(&offer_packet("../evil.txt", 10, "text/plain"))
        .expect("offer must be answered");
    assert_eq!(resp.body.get("accept").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(sink.pending(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
