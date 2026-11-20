//! Peer registry + trust store (`peers.json`).
//!
//! Keyed by certificate fingerprint — channel-independent identity. Each
//! observation records the channel it arrived on, so one device accumulates
//! `lan`, `tailscale`, `usb` addresses over time. `trusted` is set by pairing
//! and never cleared by a device simply appearing on a new channel.

use crate::channel::Channel;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One reachable address of a peer, plus when we last heard from it there.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PeerAddr {
    pub host: String,
    /// LocalSend HTTP port (packet port = http_port + 1).
    pub http_port: u16,
    pub last_seen: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Peer {
    /// Display name (last announcement wins).
    pub alias: String,
    /// Set by `pair`; the device is "tanıdık" on every channel.
    #[serde(default)]
    pub trusted: bool,
    /// Unix seconds of successful pairing.
    #[serde(default)]
    pub paired_at: Option<u64>,
    /// Channel the pairing happened on (informational).
    #[serde(default)]
    pub paired_via: Option<Channel>,
    /// Channel each address was observed on.
    pub addrs: BTreeMap<Channel, PeerAddr>,
    /// Remote capability lists from the pair exchange (empty until paired).
    #[serde(default)]
    pub incoming_capabilities: Vec<String>,
    #[serde(default)]
    pub outgoing_capabilities: Vec<String>,
}

impl Peer {
    fn new(alias: &str) -> Self {
        Self {
            alias: alias.to_string(),
            trusted: false,
            paired_at: None,
            paired_via: None,
            addrs: BTreeMap::new(),
            incoming_capabilities: Vec::new(),
            outgoing_capabilities: Vec::new(),
        }
    }

    /// Best address to dial: lowest-priority channel that we have seen.
    pub fn best_addr(&self) -> Option<(Channel, &PeerAddr)> {
        self.addrs
            .iter()
            .min_by_key(|(ch, _)| ch.dial_priority())
            .map(|(ch, addr)| (*ch, addr))
    }

    /// All addresses in dial order (lan → usb → tailscale → …).
    pub fn dial_order(&self) -> Vec<(Channel, &PeerAddr)> {
        let mut v: Vec<_> = self
            .addrs
            .iter()
            .map(|(ch, addr)| (*ch, addr))
            .collect();
        v.sort_by_key(|(ch, _)| ch.dial_priority());
        v
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Peers {
    pub peers: BTreeMap<String, Peer>,
    #[serde(skip)]
    path: PathBuf,
}

/// Read the on-disk peer map (empty when absent/corrupt — a corrupt file
/// must not brick the daemon; the next save rewrites it).
fn load_disk_map(path: &Path) -> Result<BTreeMap<String, Peer>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    match serde_json::from_str::<Peers>(&raw) {
        Ok(p) => Ok(p.peers),
        Err(e) => {
            eprintln!("[peers] ignoring corrupt {}: {e}", path.display());
            Ok(BTreeMap::new())
        }
    }
}

/// Union `src` into `dst`: addresses keep the newest `last_seen` per channel,
/// trust and capabilities are sticky (once set, never cleared by a peer that
/// does not know about them).
fn merge_peer(dst: &mut Peer, src: &Peer) {
    for (ch, addr) in &src.addrs {
        match dst.addrs.get(ch) {
            Some(existing) if existing.last_seen >= addr.last_seen => {}
            _ => {
                dst.addrs.insert(*ch, addr.clone());
            }
        }
    }
    if src.trusted {
        dst.trusted = true;
        if src.paired_at.is_some() {
            dst.paired_at = src.paired_at;
            dst.paired_via = src.paired_via;
        }
    }
    if dst.alias.is_empty() && !src.alias.is_empty() {
        dst.alias = src.alias.clone();
    }
    if dst.incoming_capabilities.is_empty() {
        dst.incoming_capabilities = src.incoming_capabilities.clone();
    }
    if dst.outgoing_capabilities.is_empty() {
        dst.outgoing_capabilities = src.outgoing_capabilities.clone();
    }
}

impl Peers {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                peers: BTreeMap::new(),
                path: path.to_path_buf(),
            });
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        let mut peers: Peers = serde_json::from_str(&raw)
            .with_context(|| format!("parse {}", path.display()))?;
        peers.path = path.to_path_buf();
        Ok(peers)
    }

    /// Persist the store, **merging** with whatever is on disk first.
    ///
    /// The running `listen` instance and one-shot commands (`pair`, `ping`,
    /// `discover`) all write this file; a plain overwrite would drop the
    /// other writer's observations (and, worse, a freshly trusted peer).
    pub fn save(&self) -> Result<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut merged = load_disk_map(&self.path)?;
        for (fp, mine) in &self.peers {
            match merged.get_mut(fp) {
                Some(theirs) => merge_peer(theirs, mine),
                None => {
                    merged.insert(fp.clone(), mine.clone());
                }
            }
        }
        let out = Peers {
            peers: merged,
            path: PathBuf::new(),
        };
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&out)?)
            .with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("rename {}", self.path.display()))?;
        Ok(())
    }

    /// Fold the on-disk state into the in-memory store without dropping any
    /// local observation — the running `listen` may have learned new peers
    /// (or trusted one) since we loaded the file.
    pub fn merge_disk(&mut self) -> Result<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let disk = load_disk_map(&self.path)?;
        for (fp, theirs) in disk {
            match self.peers.get_mut(&fp) {
                Some(mine) => merge_peer(mine, &theirs),
                None => {
                    self.peers.insert(fp, theirs);
                }
            }
        }
        Ok(())
    }

    /// Record an observation (announce, register, or probe) on `channel`.
    /// Creates the peer when new; never touches `trusted`.
    pub fn observe(
        &mut self,
        fingerprint: &str,
        alias: &str,
        channel: Channel,
        host: &str,
        http_port: u16,
    ) {
        if fingerprint.is_empty() {
            return;
        }
        let now = crate::packet::now_ms() / 1000;
        let peer = self
            .peers
            .entry(fingerprint.to_string())
            .or_insert_with(|| Peer::new(alias));
        if !alias.is_empty() {
            peer.alias = alias.to_string();
        }
        peer.addrs.insert(
            channel,
            PeerAddr {
                host: host.to_string(),
                http_port,
                last_seen: now,
            },
        );
    }

    /// Mark a peer as trusted after a successful pair exchange.
    pub fn mark_trusted(
        &mut self,
        fingerprint: &str,
        alias: &str,
        via: Channel,
        incoming: Vec<String>,
        outgoing: Vec<String>,
    ) {
        let now = crate::packet::now_ms() / 1000;
        let peer = self
            .peers
            .entry(fingerprint.to_string())
            .or_insert_with(|| Peer::new(alias));
        if !alias.is_empty() {
            peer.alias = alias.to_string();
        }
        peer.trusted = true;
        peer.paired_at = Some(now);
        peer.paired_via = Some(via);
        peer.incoming_capabilities = incoming;
        peer.outgoing_capabilities = outgoing;
    }

    /// Resolve a user-supplied target: exact fingerprint, fingerprint prefix
    /// (≥6 chars), or alias (case-insensitive). `Err` lists ambiguity.
    pub fn resolve(&self, target: &str) -> Result<(&String, &Peer)> {
        let t = target.trim();
        if t.is_empty() {
            anyhow::bail!("empty device target");
        }

        // Exact fingerprint.
        if let Some((fp, peer)) = self.peers.get_key_value(t) {
            return Ok((fp, peer));
        }

        let lower = t.to_ascii_lowercase();

        // Fingerprint prefix.
        if t.len() >= 6 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
            let matches: Vec<_> = self
                .peers
                .keys()
                .filter(|fp| fp.to_ascii_lowercase().starts_with(&lower))
                .collect();
            match matches.len() {
                0 => {}
                1 => {
                    let fp = matches[0];
                    return Ok((fp, &self.peers[fp]));
                }
                n => {
                    let list: Vec<_> = matches.iter().map(|f| f.as_str()).collect();
                    anyhow::bail!("fingerprint prefix {t:?} matches {n} devices: {}", list.join(", "));
                }
            }
        }

        // Alias (case-insensitive).
        let matches: Vec<_> = self
            .peers
            .iter()
            .filter(|(_, p)| p.alias.to_ascii_lowercase() == lower)
            .collect();
        match matches.len() {
            1 => Ok(matches[0]),
            0 => anyhow::bail!(
                "no device matches {t:?} — run `northbridge discover` first"
            ),
            n => {
                let list: Vec<_> = matches
                    .iter()
                    .map(|(fp, p)| format!("{} ({})", p.alias, &fp[..8]))
                    .collect();
                anyhow::bail!("{t:?} matches {n} devices: {}", list.join(", "));
            }
        }
    }

    pub fn trusted_count(&self) -> usize {
        self.peers.values().filter(|p| p.trusted).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Peers {
        let mut p = Peers::default();
        p.observe("AABBCCDDEEFF00112233", "phone", Channel::Lan, "192.168.1.5", 53317);
        p.observe("AABBCCDDEEFF00112233", "phone", Channel::Tailscale, "100.1.2.3", 53317);
        p.observe("11223344556677889900", "laptop", Channel::Lan, "192.168.1.6", 53317);
        p
    }

    #[test]
    fn observe_accumulates_channels_without_trusting() {
        let p = sample();
        let (_, phone) = p.resolve("phone").unwrap();
        assert_eq!(phone.addrs.len(), 2);
        assert!(!phone.trusted);
        assert_eq!(phone.best_addr().unwrap().0, Channel::Lan);
    }

    #[test]
    fn resolve_by_alias_and_prefix() {
        let p = sample();
        assert_eq!(p.resolve("PHONE").unwrap().1.alias, "phone");
        assert_eq!(p.resolve("aabbcc").unwrap().1.alias, "phone");
        assert!(p.resolve("nobody").is_err());
    }

    #[test]
    fn trust_is_channel_independent() {
        let mut p = sample();
        p.mark_trusted("AABBCCDDEEFF00112233", "phone", Channel::Lan, vec![], vec![]);
        let (_, phone) = p.resolve("phone").unwrap();
        assert!(phone.trusted);
        assert_eq!(phone.addrs.len(), 2); // both channels intact
    }
}
