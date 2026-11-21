use serde_json::{Value, json};

use crate::codec::negotiate_codec;
use crate::server::AudioServer;

pub const CAP: &str = "nb.audio";
pub const AUDIO_OPEN_KIND: &str = "nb.audio.open";

pub fn handle_control(server: &AudioServer, body: &Value) -> Option<Value> {
    match body.get("t").and_then(Value::as_str) {
        Some("open") => {
            let codec = negotiate_codec(body.get("codec").and_then(Value::as_str));
            Some(json!({"t": "opened", "port": server.audio_port(), "codec": codec.name()}))
        }
        Some("release") => {
            server.close_any();
            Some(json!({"t": "closed"}))
        }
        _ => None,
    }
}

pub fn handle_control_from(server: &AudioServer, peer_fp: &str, body: &Value) -> Option<Value> {
    match body.get("t").and_then(Value::as_str) {
        Some("open") => {
            let codec = negotiate_codec(body.get("codec").and_then(Value::as_str));
            let port = server.open(peer_fp, codec);
            Some(json!({"t": "opened", "port": port, "codec": codec.name()}))
        }
        Some("release") => {
            server.close(peer_fp);
            Some(json!({"t": "closed"}))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_core::identity::Identity;

    fn test_port(offset: u16) -> u16 {
        44900 + (std::process::id() as u16 % 300) + offset
    }

    fn server(offset: u16) -> AudioServer {
        let id = Identity {
            alias: "test".into(),
            cert_pem: String::new(),
            key_pem: String::new(),
            fingerprint: "TEST".into(),
            port: test_port(offset),
        };
        AudioServer::start(id)
    }

    #[test]
    fn control_open_replies_with_port_and_codec() {
        let srv = server(0);
        let out = handle_control(&srv, &json!({"t": "open", "codec": "pcm"}))
            .expect("reply");
        assert_eq!(out["t"], "opened");
        assert_eq!(out["port"], srv.audio_port());
        assert_eq!(out["codec"], "pcm");
    }

    #[test]
    fn control_from_binds_peer_and_negotiates() {
        let srv = server(10);
        let out =
            handle_control_from(&srv, "PEER", &json!({"t": "open", "codec": "opus"}))
                .expect("reply");
        assert_eq!(out["t"], "opened");
        assert_eq!(out["port"], srv.audio_port());
        assert_eq!(srv.session_peer().as_deref(), Some("PEER"));
        #[cfg(not(feature = "opus"))]
        assert_eq!(out["codec"], "pcm");
        #[cfg(feature = "opus")]
        assert_eq!(out["codec"], "opus");
        handle_control_from(&srv, "OTHER", &json!({"t": "release"}));
        assert!(srv.is_open());
        let out = handle_control(&srv, &json!({"t": "release"})).expect("reply");
        assert_eq!(out["t"], "closed");
        assert!(!srv.is_open());
    }

    #[test]
    fn garbage_is_none() {
        let srv = server(20);
        assert!(handle_control(&srv, &json!({"t": "unknown"})).is_none());
        assert!(handle_control_from(&srv, "PEER", &json!({"t": "unknown"})).is_none());
        assert!(handle_control(&srv, &json!({})).is_none());
        assert!(handle_control(&srv, &json!({"t": 42})).is_none());
    }
}
