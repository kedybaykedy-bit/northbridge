//! Multi-path device discovery: LocalSend multicast (LAN/USB interfaces)
//! + Tailscale peer probing.

pub mod tailscale;

pub use tailscale::{
    discover_staged, probe_tailscale, tailscale_channels, tailscale_peer_ips,
};
