//! Device identity: self-signed cert + fingerprint (LocalSend model).
//!
//! The fingerprint (uppercase-hex SHA-256 of the cert DER) is the device ID on
//! the wire. It is channel-independent: a peer seen over LAN, Tailscale or USB
//! with the same fingerprint is the same device.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Identity as persisted in `identity.json`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Identity {
    /// Display name of this device.
    pub alias: String,
    /// PEM-encoded self-signed certificate.
    pub cert_pem: String,
    /// PEM-encoded private key (PKCS#8).
    pub key_pem: String,
    /// SHA-256 of the cert DER, uppercase hex. Stable device ID.
    pub fingerprint: String,
    /// LocalSend-compatible HTTP port (announced on discovery).
    pub port: u16,
}

impl Identity {
    /// The packet-channel port: HTTP port + 1 (convention documented in PLAN.md).
    pub fn packet_port(&self) -> u16 {
        self.port.saturating_add(1)
    }

    /// Load from `dir/identity.json`, generating a new one when absent.
    ///
    /// `alias`/`port` overrides apply on generation only; an existing identity
    /// is never rewritten (the fingerprint must stay stable or trust breaks).
    /// Creation is atomic and never overwrites: when two processes race to
    /// create the identity, both end up using the winner's file — otherwise a
    /// late writer would leave the running server announcing a fingerprint
    /// that no longer matches its own certificate.
    pub fn load_or_generate(dir: &Path, alias: Option<String>, port: Option<u16>) -> Result<Self> {
        let path = dir.join("identity.json");
        if path.exists() {
            return Self::load(&path);
        }

        let cert = localsend::crypto::cert::generate_self_signed()?;
        let alias = alias.unwrap_or_else(|| default_alias(&cert.fingerprint));
        let identity = Identity {
            alias,
            cert_pem: cert.certificate_pem,
            key_pem: cert.private_key_pem,
            fingerprint: cert.fingerprint,
            port: port.unwrap_or(53317),
        };
        if identity.save_if_absent(dir)? {
            Ok(identity)
        } else {
            // Another process won the race — use its identity.
            Self::load(&path)
        }
    }

    fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
    }

    /// Write `dir/identity.json` only when it does not exist yet.
    /// Returns `true` when this call created the file.
    pub fn save_if_absent(&self, dir: &Path) -> Result<bool> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("identity.json");
        let tmp = dir.join(format!(".identity.json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("write {}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        // hard_link fails with AlreadyExists when the destination exists —
        // an atomic no-overwrite publish (same filesystem, both paths in dir).
        match std::fs::hard_link(&tmp, &path) {
            Ok(()) => {
                let _ = std::fs::remove_file(&tmp);
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(&tmp);
                Ok(false)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e).with_context(|| format!("publish {}", path.display()))
            }
        }
    }
}

/// `$XDG_CONFIG_HOME/northbridge` or `~/.config/northbridge`.
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
        .join("northbridge")
}

fn default_alias(fingerprint: &str) -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| format!("nb-{}", &fingerprint[..8.min(fingerprint.len())]))
}
