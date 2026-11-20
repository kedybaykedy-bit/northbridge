//! `nb.ping` — the smallest possible northbridge plugin.
//!
//! Proves the capability-matching machinery end-to-end: the peer only loads
//! this plugin when our `outgoing` intersects its `incoming` (or vice versa)
//! during the pair exchange.

use nb_core::packet::NetworkPacket;
use nb_core::plugin::{Plugin, PluginCtx, body_str};
use serde_json::json;

pub struct PingPlugin;

impl Plugin for PingPlugin {
    fn id(&self) -> &str {
        "nb.ping"
    }

    fn incoming(&self) -> &[&str] {
        &["nb.ping"]
    }

    fn outgoing(&self) -> &[&str] {
        &["nb.ping", "nb.pong"]
    }

    fn on_packet(&mut self, ctx: &PluginCtx, pkt: &NetworkPacket) -> Option<NetworkPacket> {
        // Request: {"message": "..."} → pong with our alias + echoed message.
        if pkt.kind == "nb.ping" {
            let message = body_str(pkt, "message").unwrap_or_else(|| "ping".into());
            return Some(NetworkPacket::new(
                "nb.pong",
                json!({
                    "message": message,
                    "reply": ctx.own_alias,
                    "channel": ctx.channel.to_string(),
                    "at": nb_core::packet::now_ms(),
                }),
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_core::channel::Channel;
    use nb_core::plugin::plugins_for_capabilities;

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
    fn ping_pong() {
        let mut p = PingPlugin;
        let req = NetworkPacket::new("nb.ping", json!({"message": "hello"}));
        let resp = p.on_packet(&ctx(), &req).expect("pong");
        assert_eq!(resp.kind, "nb.pong");
        assert_eq!(resp.body["message"], "hello");
        assert_eq!(resp.body["reply"], "me");
    }

    #[test]
    fn capability_gate() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(PingPlugin)];
        // Peer advertises nb.ping incoming → our outgoing matches → load.
        let active = plugins_for_capabilities(&["nb.ping".into()], &[], &plugins);
        assert_eq!(active, vec![0]);
        // Peer speaks nothing we do → not loaded.
        let active = plugins_for_capabilities(&["other".into()], &["other".into()], &plugins);
        assert!(active.is_empty());
    }
}
