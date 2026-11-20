//! NetworkPacket: KDE Connect-style JSON message with a newline terminator.
//!
//! Wire format: one compact JSON object per line, e.g.
//! `{"id":1700000000000,"type":"nb.pair","body":{...}}\n`
//!
//! Unlike KDE Connect, `version` is not emitted on the wire; the protocol
//! version travels in the pair/identity exchange.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkPacket {
    /// Milliseconds since epoch, stamped at send time. Receivers ignore it.
    pub id: u64,
    /// Packet type, e.g. `nb.pair`, `nb.ping`. Plugins own their namespace.
    #[serde(rename = "type")]
    pub kind: String,
    /// Free-form body owned by the packet type / plugin.
    #[serde(default)]
    pub body: Value,
}

impl NetworkPacket {
    pub fn new(kind: impl Into<String>, body: Value) -> Self {
        Self {
            id: now_ms(),
            kind: kind.into(),
            body,
        }
    }

    /// Serialize to a single line (no trailing newline).
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("NetworkPacket is always serializable")
    }

    /// Parse one line; trailing newline/whitespace tolerated.
    pub fn from_line(line: &str) -> Option<Self> {
        serde_json::from_str(line.trim()).ok()
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_line() {
        let pkt = NetworkPacket::new("nb.ping", json!({"ts": 42}));
        let line = pkt.to_line();
        assert!(line.ends_with('\n') == false || true); // to_line has no newline
        assert!(!line.contains('\n'));
        let back = NetworkPacket::from_line(&format!("{line}\n")).unwrap();
        assert_eq!(back.kind, "nb.ping");
        assert_eq!(back.body["ts"], 42);
    }
}
