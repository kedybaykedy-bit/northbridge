//! Plugin trait + capability matching (KDE Connect PluginLoader semantics).
//!
//! A plugin declares which packet types it can receive (`incoming`) and send
//! (`outgoing`). Two devices exchange these lists during pairing; a plugin is
//! active for a peer when the intersection test passes — exactly KDE Connect's
//! `pluginsForCapabilities` (pluginloader.cpp:89-120).

use crate::packet::NetworkPacket;
use serde_json::Value;

/// Everything a plugin may need while handling a packet.
pub struct PluginCtx {
    /// Our own alias.
    pub own_alias: String,
    /// Our fingerprint.
    pub own_fingerprint: String,
    /// The peer's fingerprint (from the TLS handshake, not the packet).
    pub peer_fingerprint: String,
    /// The peer's alias (from pairing/identity).
    pub peer_alias: String,
    /// Channel the packet arrived on.
    pub channel: crate::channel::Channel,
}

pub trait Plugin: Send {
    /// Stable plugin id, e.g. `nb.ping`. Also its packet-type namespace root.
    fn id(&self) -> &str;

    /// Packet types this plugin can receive.
    fn incoming(&self) -> &[&str];

    /// Packet types this plugin may send.
    fn outgoing(&self) -> &[&str];

    /// Handle an incoming packet. Returning `Some(resp)` sends `resp` back on
    /// the same connection (request/response model of the M0 packet channel).
    fn on_packet(&mut self, ctx: &PluginCtx, pkt: &NetworkPacket) -> Option<NetworkPacket>;

    /// Called when a peer becomes reachable+paired (KDE Connect `connected()`).
    fn on_connected(&mut self, _ctx: &PluginCtx) {}
}

/// KDE Connect `pluginsForCapabilities` ported to slices.
///
/// - Both remote lists empty → load all (pre-capability fallback).
/// - Plugin with no declared capabilities → always loads.
/// - Otherwise: load iff `remote_outgoing ∩ plugin_incoming ≠ ∅`
///   OR `remote_incoming ∩ plugin_outgoing ≠ ∅`.
pub fn plugins_for_capabilities(
    remote_incoming: &[String],
    remote_outgoing: &[String],
    plugins: &[Box<dyn Plugin>],
) -> Vec<usize> {
    if remote_incoming.is_empty() && remote_outgoing.is_empty() {
        return (0..plugins.len()).collect();
    }
    plugins
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            let my_in = p.incoming();
            let my_out = p.outgoing();
            if my_in.is_empty() && my_out.is_empty() {
                return true;
            }
            let hits_remote_out = remote_outgoing.iter().any(|r| my_in.contains(&r.as_str()));
            let hits_remote_in = remote_incoming.iter().any(|r| my_out.contains(&r.as_str()));
            hits_remote_out || hits_remote_in
        })
        .map(|(i, _)| i)
        .collect()
}

/// Union of a plugin set's incoming capabilities (for the pair exchange).
pub fn union_incoming(plugins: &[Box<dyn Plugin>]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in plugins {
        for t in p.incoming() {
            if !out.iter().any(|x| x == t) {
                out.push((*t).to_string());
            }
        }
    }
    out
}

/// Union of a plugin set's outgoing capabilities.
pub fn union_outgoing(plugins: &[Box<dyn Plugin>]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in plugins {
        for t in p.outgoing() {
            if !out.iter().any(|x| x == t) {
                out.push((*t).to_string());
            }
        }
    }
    out
}

/// Dispatch an incoming packet to the first plugin that declares `kind` in its
/// incoming capabilities. `active` indexes into `plugins` (capability filter).
pub fn dispatch(
    plugins: &mut [Box<dyn Plugin>],
    active: &[usize],
    ctx: &PluginCtx,
    pkt: &NetworkPacket,
) -> Option<NetworkPacket> {
    for &i in active {
        let p = &mut plugins[i];
        if p.incoming().contains(&pkt.kind.as_str()) {
            return p.on_packet(ctx, pkt);
        }
    }
    None
}

/// Helper for plugin bodies: read a string field with default.
pub fn body_str(pkt: &NetworkPacket, key: &str) -> Option<String> {
    pkt.body.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Echo {
        hits: usize,
    }

    impl Plugin for Echo {
        fn id(&self) -> &str {
            "test.echo"
        }
        fn incoming(&self) -> &[&str] {
            &["test.echo"]
        }
        fn outgoing(&self) -> &[&str] {
            &["test.echo"]
        }
        fn on_packet(&mut self, ctx: &PluginCtx, pkt: &NetworkPacket) -> Option<NetworkPacket> {
            self.hits += 1;
            Some(NetworkPacket::new(
                "test.echo",
                json!({"echo": pkt.body, "from": ctx.own_alias}),
            ))
        }
    }

    struct Other;

    impl Plugin for Other {
        fn id(&self) -> &str {
            "test.other"
        }
        fn incoming(&self) -> &[&str] {
            &["test.other"]
        }
        fn outgoing(&self) -> &[&str] {
            &["test.other"]
        }
        fn on_packet(&mut self, _: &PluginCtx, _: &NetworkPacket) -> Option<NetworkPacket> {
            None
        }
    }

    fn plugins() -> Vec<Box<dyn Plugin>> {
        vec![Box::new(Echo { hits: 0 }), Box::new(Other)]
    }

    #[test]
    fn kc_capability_matching() {
        let p = plugins();

        // Remote speaks both ways → both load.
        let active = plugins_for_capabilities(
            &["test.echo".into(), "test.other".into()],
            &["test.echo".into(), "test.other".into()],
            &p,
        );
        assert_eq!(active, vec![0, 1]);

        // Remote only sends test.other → only Other's incoming matches.
        let active = plugins_for_capabilities(&[], &["test.other".into()], &p);
        assert_eq!(active, vec![1]);

        // Remote only receives test.echo → only Echo's outgoing matches.
        let active = plugins_for_capabilities(&["test.echo".into()], &[], &p);
        assert_eq!(active, vec![0]);

        // Empty remote caps → load all (legacy fallback).
        let active = plugins_for_capabilities(&[], &[], &p);
        assert_eq!(active, vec![0, 1]);

        // No overlap → nothing loads.
        let active = plugins_for_capabilities(&["nope".into()], &["nope2".into()], &p);
        assert!(active.is_empty());
    }

    #[test]
    fn dispatch_routes_by_incoming() {
        let mut p = plugins();
        let active = plugins_for_capabilities(&[], &[], &p);
        let ctx = PluginCtx {
            own_alias: "me".into(),
            own_fingerprint: "F".into(),
            peer_fingerprint: "P".into(),
            peer_alias: "peer".into(),
            channel: crate::channel::Channel::Lan,
        };
        let pkt = NetworkPacket::new("test.echo", json!({"x": 1}));
        let resp = dispatch(&mut p, &active, &ctx, &pkt).expect("echo responds");
        assert_eq!(resp.kind, "test.echo");
        assert_eq!(resp.body["from"], "me");

        let missing = NetworkPacket::new("test.unknown", json!({}));
        assert!(dispatch(&mut p, &active, &ctx, &missing).is_none());
    }
}
