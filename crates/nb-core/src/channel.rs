//! Channel classification: which path did we reach a peer on?
//!
//! The channel is metadata about an observation, never part of the device
//! identity. The same fingerprint seen on LAN and Tailscale is one device
//! with two channels (this is what makes "WiFi'de eşleşti, USB'de tanıdı"
//! work: trust follows the fingerprint, channels accumulate).

use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Same local network (or USB tethering — classified below).
    Lan,
    /// Over the tailnet (100.64.0.0/10, fd7a:115c:a1e0::/48).
    Tailscale,
    /// Via a USB tethering interface (enx*, rndis*, usb*, ap*).
    Usb,
    /// Via Bluetooth RFCOMM (M5).
    Bluetooth,
    /// Could not classify.
    Unknown,
}

impl Channel {
    /// Classify a peer IP string (scope id like `fe80::1%3` tolerated).
    pub fn classify(host: &str) -> Channel {
        let bare = host.split('%').next().unwrap_or(host);
        let Ok(ip) = bare.parse::<IpAddr>() else {
            return Channel::Unknown;
        };
        if is_tailscale(ip) {
            return Channel::Tailscale;
        }
        if is_usb_endpoint(ip) {
            return Channel::Usb;
        }
        Channel::Lan
    }

    /// Preference order when dialing: cheapest local path first.
    pub fn dial_priority(self) -> u8 {
        match self {
            Channel::Lan => 0,
            Channel::Usb => 1,
            Channel::Tailscale => 2,
            Channel::Bluetooth => 3,
            Channel::Unknown => 4,
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Channel::Lan => "lan",
            Channel::Tailscale => "tailscale",
            Channel::Usb => "usb",
            Channel::Bluetooth => "bluetooth",
            Channel::Unknown => "unknown",
        };
        f.write_str(s)
    }
}

/// Tailscale CGNAT range 100.64.0.0/10 + ULA fd7a:115c:a1e0::/48.
fn is_tailscale(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 100 && (o[1] & 0xC0) == 64 // 100.64.0.0 – 100.127.255.255
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

/// True when `ip` sits on a USB-tethering-style local interface subnet.
fn is_usb_endpoint(ip: IpAddr) -> bool {
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return false;
    };
    for iface in ifaces {
        if !usb_iface_name(&iface.name) {
            continue;
        }
        if same_subnet(ip, &iface.addr) {
            return true;
        }
    }
    false
}

fn usb_iface_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    // Linux USB tethering: enx<mac>, rndis0, usb0; Android hotspot-ish: ap0.
    n.starts_with("enx") || n.starts_with("rndis") || n.starts_with("usb") || n.starts_with("ap")
}

fn same_subnet(ip: IpAddr, addr: &if_addrs::IfAddr) -> bool {
    match (ip, addr) {
        (IpAddr::V4(ip), if_addrs::IfAddr::V4(local)) => {
            let mask = u32::from(local.netmask);
            mask != 0 && (u32::from(ip) & mask) == (u32::from(local.ip) & mask)
        }
        (IpAddr::V6(ip), if_addrs::IfAddr::V6(local)) => {
            // /64 heuristic — USB/tether IPv6 setups are all /64 in practice.
            ip.octets()[..8] == local.ip.octets()[..8]
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailscale_v4() {
        assert_eq!(Channel::classify("100.103.43.100"), Channel::Tailscale);
        assert_eq!(Channel::classify("100.64.0.1"), Channel::Tailscale);
        assert_ne!(Channel::classify("100.128.0.1"), Channel::Tailscale);
        assert_ne!(Channel::classify("192.168.1.5"), Channel::Tailscale);
    }

    #[test]
    fn tailscale_v6_with_scope() {
        assert_eq!(
            Channel::classify("fd7a:115c:a1e0::d2c:2b65"),
            Channel::Tailscale
        );
        assert_ne!(Channel::classify("fe80::1%3"), Channel::Tailscale);
    }

    #[test]
    fn plain_lan() {
        assert_eq!(Channel::classify("192.168.1.5"), Channel::Lan);
        assert_eq!(Channel::classify("not-an-ip"), Channel::Unknown);
    }
}
