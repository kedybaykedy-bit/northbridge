use serde_json::{Value, json};

use crate::server::ScreenServer;

pub const CAP: &str = "nb.screen";
pub const FRAME_OPEN_KIND: &str = "nb.screen.frame.open";

pub const DEFAULT_MAX_W: u16 = 1280;
pub const DEFAULT_FPS: u8 = 12;

pub fn normalize_max_w(raw: u64) -> u16 {
    if raw == 0 {
        DEFAULT_MAX_W
    } else {
        raw.clamp(160, 1920) as u16
    }
}

pub fn normalize_fps(raw: u64) -> u8 {
    if raw == 0 {
        DEFAULT_FPS
    } else {
        raw.clamp(1, 30) as u8
    }
}

pub fn handle_control(server: &ScreenServer, body: &Value) -> Option<Value> {
    match body.get("t").and_then(Value::as_str) {
        Some("open") => Some(json!({"t": "opened", "port": server.frame_port()})),
        Some("release") => {
            server.close_any();
            Some(json!({"t": "closed"}))
        }
        _ => None,
    }
}

pub fn handle_control_from(
    server: &ScreenServer,
    peer_fp: &str,
    body: &Value,
) -> Option<Value> {
    match body.get("t").and_then(Value::as_str) {
        Some("open") => {
            let max_w = normalize_max_w(
                body
                    .get("max_w")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::from(DEFAULT_MAX_W)),
            );
            let fps = normalize_fps(
                body
                    .get("fps")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::from(DEFAULT_FPS)),
            );
            let port = server.open(peer_fp, max_w, fps);
            Some(json!({"t": "opened", "port": port}))
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

    fn server(offset: u16) -> ScreenServer {
        let id = Identity {
            alias: "test".into(),
            cert_pem: String::new(),
            key_pem: String::new(),
            fingerprint: "TEST".into(),
            port: test_port(offset),
        };
        ScreenServer::start(id)
    }

    #[test]
    fn control_open_replies_with_port() {
        let srv = server(0);
        let out = handle_control(&srv, &json!({"t": "open", "max_w": 1280, "fps": 12}))
            .expect("reply");
        assert_eq!(out["t"], "opened");
        assert_eq!(out["port"], srv.frame_port());
    }

    #[test]
    fn control_release_closes_and_unknown_is_none() {
        let srv = server(10);
        let out = handle_control_from(&srv, "PEER", &json!({"t": "open"}))
            .expect("reply");
        assert_eq!(out["t"], "opened");
        assert!(srv.is_open());
        let out = handle_control(&srv, &json!({"t": "release"})).expect("reply");
        assert_eq!(out["t"], "closed");
        assert!(!srv.is_open());
        assert!(handle_control(&srv, &json!({"t": "bogus"})).is_none());
        assert!(handle_control_from(&srv, "PEER", &json!({})).is_none());
    }

    #[test]
    fn control_from_binds_peer_and_honors_params() {
        let srv = server(20);
        let out =
            handle_control_from(&srv, "PEER", &json!({"t": "open", "max_w": 640, "fps": 10}))
                .expect("reply");
        assert_eq!(out["port"], srv.frame_port());
        assert_eq!(srv.session_peer().as_deref(), Some("PEER"));
        handle_control_from(&srv, "OTHER", &json!({"t": "release"}));
        assert!(srv.is_open());
        handle_control_from(&srv, "PEER", &json!({"t": "release"}));
        assert!(!srv.is_open());
    }
}
