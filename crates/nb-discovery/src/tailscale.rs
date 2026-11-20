//! Tailscale peer discovery.
//!
//! Multicast does not cross the tailnet, so Tailscale peers are found by
//! asking the local tailscaled for its peer list and probing each peer's
//! LocalSend HTTP port directly over its 100.x / fd7a:… address. Those
//! observations are classified `Channel::Tailscale` by IP range.

use anyhow::Result;
use localsend::discovery::DiscoveryHandle;
use localsend::model::discovery::ProtocolType;
use std::net::IpAddr;
use std::process::Command;

/// Parse `tailscale status --json` → IPv4/IPv6 addresses of online peers
/// (excluding self). Empty vec when tailscale is absent/not running.
pub fn tailscale_peer_ips() -> Vec<IpAddr> {
    let output = match Command::new("tailscale")
        .args(["status", "--json"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    parse_tailscale_status(&output.stdout)
}

fn parse_tailscale_status(json: &[u8]) -> Vec<IpAddr> {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let mut ips = Vec::new();

    // Online peers only.
    if let Some(peers) = v.get("Peer").and_then(|p| p.as_object()) {
        for peer in peers.values() {
            let online = peer.get("Online").and_then(|o| o.as_bool()).unwrap_or(false);
            if !online {
                continue;
            }
            collect_ips(peer, &mut ips);
        }
    }
    ips
}

fn collect_ips(peer: &serde_json::Value, out: &mut Vec<IpAddr>) {
    if let Some(arr) = peer.get("TailscaleIPs").and_then(|a| a.as_array()) {
        for ip in arr {
            if let Some(s) = ip.as_str() {
                if let Ok(addr) = s.parse::<IpAddr>() {
                    if !out.contains(&addr) {
                        out.push(addr);
                    }
                }
            }
        }
    }
}

/// Probe every online Tailscale peer's LocalSend HTTP port.
/// Feeds confirmed devices into `discovery`'s store (events flow to the
/// caller's DiscoveryEvent channel). Returns how many probes succeeded.
pub async fn probe_tailscale(discovery: &DiscoveryHandle, http_port: u16) -> usize {
    let ips = tailscale_peer_ips();
    if ips.is_empty() {
        return 0;
    }
    let mut ok = 0;
    for ip in ips {
        let host = ip.to_string();
        match discovery.discover(&host, http_port, ProtocolType::Https).await {
            Ok(Some(_)) => ok += 1,
            Ok(None) => {}
            Err(e) => {
                // Unreachable tailnet peers are normal (offline/asleep).
                eprintln!("[tailscale] probe {host} failed: {e}");
            }
        }
    }
    ok
}

/// Run a one-shot staged discovery: announce + probe known channels +
/// subnet fallback. Blocks until the stage completes.
pub async fn discover_staged(
    discovery: &DiscoveryHandle,
    known: Vec<localsend::discovery::HttpChannel>,
    http_port: u16,
) -> Result<()> {
    use localsend::util::interface::{InterfaceFilter, local_interface_addresses};

    let ips = local_interface_addresses(&InterfaceFilter::default()).unwrap_or_default();
    discovery
        .discover_staged(
            known,
            ips,
            http_port,
            ProtocolType::Https,
            std::time::Duration::from_secs(1),
        )
        .await?;
    Ok(())
}

/// Build `HttpChannel` entries for Tailscale peers (used as `known` in staged
/// discovery so subnet scan is skipped for them).
pub fn tailscale_channels(http_port: u16) -> Vec<localsend::discovery::HttpChannel> {
    tailscale_peer_ips()
        .into_iter()
        .map(|ip| localsend::discovery::HttpChannel {
            host: ip.to_string(),
            port: http_port,
            protocol: ProtocolType::Https,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tailscale_json() {
        let json = br#"{
            "Version": "1.102.5",
            "BackendState": "Running",
            "Self": {"HostName": "me", "TailscaleIPs": ["100.103.43.100"], "Online": true},
            "Peer": {
                "nodekey:abc": {
                    "HostName": "phone",
                    "Online": true,
                    "TailscaleIPs": ["100.64.0.9", "fd7a:115c:a1e0::1"]
                },
                "nodekey:def": {
                    "HostName": "asleep-laptop",
                    "Online": false,
                    "TailscaleIPs": ["100.64.0.10"]
                }
            }
        }"#;
        let ips = parse_tailscale_status(json);
        // Self is not in Peer; offline peer excluded.
        assert_eq!(ips.len(), 2);
        assert!(ips.contains(&"100.64.0.9".parse().unwrap()));
        assert!(ips.contains(&"fd7a:115c:a1e0::1".parse().unwrap()));
    }

    #[test]
    fn garbage_is_empty() {
        assert!(parse_tailscale_status(b"not json").is_empty());
        assert!(parse_tailscale_status(b"{}").is_empty());
    }
}
