//! `nb.clipboard` — clipboard text sync between paired devices.
//!
//! Echo prevention is state-based: every clipboard observation (local read or
//! remote packet) updates [`ClipboardState::last`]; the watcher only pushes
//! when the current clipboard differs from it. A packet we just applied is
//! therefore never pushed back.

use nb_core::packet::NetworkPacket;
use nb_core::plugin::{Plugin, PluginCtx};
use serde_json::json;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Last clipboard content we have seen (written locally or received).
#[derive(Default)]
pub struct ClipboardState {
    pub last: String,
}

pub type SharedState = Arc<Mutex<ClipboardState>>;

pub fn shared_state() -> SharedState {
    Arc::new(Mutex::new(ClipboardState::default()))
}

/// Read the system clipboard as text. `None` when no clipboard tool/display
/// is available (headless session) or the clipboard holds no text.
pub fn read() -> Option<String> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        if let Some(s) = run_capture("wl-paste", &["-n"]) {
            return Some(s);
        }
    }
    if std::env::var("DISPLAY").is_ok() {
        if let Some(s) = run_capture("xclip", &["-selection", "clipboard", "-o"])
        {
            return Some(s);
        }
        if let Some(s) = run_capture("xsel", &["--clipboard", "--output"]) {
            return Some(s);
        }
    }
    None
}

/// Write `text` to the system clipboard. Returns false when no tool worked.
pub fn write(text: &str) -> bool {
    if std::env::var("WAYLAND_DISPLAY").is_ok() && run_paste("wl-copy", &[], text) {
        return true;
    }
    if std::env::var("DISPLAY").is_ok() {
        if run_paste("xclip", &["-selection", "clipboard", "-i"], text) {
            return true;
        }
        if run_paste("xsel", &["--clipboard", "--input"], text) {
            return true;
        }
    }
    false
}

fn run_capture(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

fn run_paste(bin: &str, args: &[&str], text: &str) -> bool {
    use std::io::Write;
    let mut child = match Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let ok = child
        .stdin
        .as_mut()
        .and_then(|s| s.write_all(text.as_bytes()).ok())
        .is_some();
    let status = child.wait().ok();
    ok && status.map(|s| s.success()).unwrap_or(false)
}

/// The plugin: applies remote clipboard packets to the local clipboard.
pub struct ClipboardPlugin {
    state: SharedState,
}

impl ClipboardPlugin {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

impl Plugin for ClipboardPlugin {
    fn id(&self) -> &str {
        "nb.clipboard"
    }

    fn incoming(&self) -> &[&str] {
        &["nb.clipboard"]
    }

    fn outgoing(&self) -> &[&str] {
        &["nb.clipboard"]
    }

    fn on_packet(&mut self, _ctx: &PluginCtx, pkt: &NetworkPacket) -> Option<NetworkPacket> {
        let content = pkt.body.get("content").and_then(|v| v.as_str())?;
        {
            let mut st = self.state.lock().unwrap();
            if content == st.last {
                // Already applied (or we sent it) — ack without rewriting.
                return Some(NetworkPacket::new("nb.clipboard", json!({"ack": true})));
            }
            st.last = content.to_string();
        }
        let preview: String = content.chars().take(40).collect();
        if write(content) {
            println!("[clipboard] applied from {}: {preview}", _ctx.peer_alias);
            Some(NetworkPacket::new("nb.clipboard", json!({"ack": true})))
        } else {
            Some(NetworkPacket::new(
                "nb.clipboard",
                json!({"ack": false, "error": "no clipboard backend"}),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_core::channel::Channel;
    use nb_core::plugin::PluginCtx;

    fn ctx() -> PluginCtx {
        PluginCtx {
            own_alias: "me".into(),
            own_fingerprint: "F".into(),
            peer_fingerprint: "P".into(),
            peer_alias: "peer".into(),
            channel: Channel::Lan,
        }
    }

    #[test]
    fn echo_prevention_skips_already_seen_content() {
        let state = shared_state();
        state.lock().unwrap().last = "hello".into();
        let mut p = ClipboardPlugin::new(Arc::clone(&state));
        // Same content: must NOT attempt a clipboard write (headless test
        // would fail anyway) and must still ack.
        let pkt = NetworkPacket::new("nb.clipboard", json!({"content": "hello"}));
        let resp = p.on_packet(&ctx(), &pkt).expect("ack");
        assert_eq!(resp.body["ack"], true);
        assert_eq!(state.lock().unwrap().last, "hello");
    }

    #[test]
    fn state_updates_before_write_on_new_content() {
        let state = shared_state();
        let mut p = ClipboardPlugin::new(Arc::clone(&state));
        let pkt = NetworkPacket::new("nb.clipboard", json!({"content": "fresh"}));
        // On a headless box `write` fails, but the state must still record the
        // content so the watcher does not echo it back.
        let _ = p.on_packet(&ctx(), &pkt);
        assert_eq!(state.lock().unwrap().last, "fresh");
    }

    #[test]
    fn malformed_packet_is_ignored() {
        let state = shared_state();
        let mut p = ClipboardPlugin::new(state);
        let pkt = NetworkPacket::new("nb.clipboard", json!({"nope": 1}));
        assert!(p.on_packet(&ctx(), &pkt).is_none());
    }
}
