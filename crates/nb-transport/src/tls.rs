//! TLS configs for the packet channel.
//!
//! Ported from LocalSend's verifier design (self-signed certs, identity =
//! uppercase-hex SHA-256 fingerprint of the cert DER):
//! - server: accepts any *valid* self-signed client cert (mandatory), so it
//!   can read the real peer fingerprint from the handshake;
//! - client: pins the expected server fingerprint — checked during the
//!   handshake, before any packet bytes are written.

use anyhow::{Context, Result};
use localsend::crypto::cert::{fingerprint_from_cert_der, verify_cert_from_der};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::server::WebPkiClientVerifier;
use rustls::{
    DigitallySignedStruct, DistinguishedName, Error as TlsError, RootCertStore, SignatureScheme,
};
use std::fmt::{Debug, Formatter};
use std::sync::{Arc, OnceLock};

use nb_core::identity::Identity;

/// Install the ring crypto provider exactly once (process-global).
pub fn install_crypto() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Wrap a message into rustls' `CertificateError::Other` so handshake failures
/// surface the real reason instead of a bare application-verification error.
fn other_err(msg: String) -> TlsError {
    struct Wrap(String);
    impl Debug for Wrap {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::fmt::Display for Wrap {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::error::Error for Wrap {}
    TlsError::InvalidCertificate(rustls::CertificateError::Other(
        rustls::OtherError(Arc::new(Wrap(msg))),
    ))
}

/// Server-side verifier: any *valid* self-signed client cert is accepted;
/// mandatory (we always want the peer fingerprint).
pub struct AcceptAnyClientCert {
    inner: Arc<dyn ClientCertVerifier>,
}

impl AcceptAnyClientCert {
    pub fn try_new(our_cert_pem: &str) -> Result<Self> {
        // Non-empty root store required by the builder; our own cert is never
        // used as an authority for clients (verify below does not delegate).
        let mut roots = RootCertStore::empty();
        roots.add(PemObject::from_pem_slice(our_cert_pem.as_bytes())?)?;
        Ok(Self {
            inner: WebPkiClientVerifier::builder(Arc::new(roots)).build()?,
        })
    }
}

impl Debug for AcceptAnyClientCert {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl ClientCertVerifier for AcceptAnyClientCert {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.inner.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: UnixTime,
    ) -> Result<ClientCertVerified, TlsError> {
        if let Err(e) = verify_cert_from_der(cert.as_ref(), None) {
            eprintln!("[tls] client certificate rejected: {e:#}");
            return Err(other_err(format!("client certificate invalid: {e:#}")));
        }
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Client-side verifier: pins the server cert to `expected_fingerprint`
/// (uppercase hex). `None` = trust-on-first-use (discovery probes only).
pub struct PinnedServerCert {
    inner: Arc<dyn ServerCertVerifier>,
    expected: Option<String>,
}

impl PinnedServerCert {
    pub fn try_new(our_cert_pem: &str, expected_fingerprint: Option<String>) -> Result<Self> {
        let mut roots = RootCertStore::empty();
        roots.add(PemObject::from_pem_slice(our_cert_pem.as_bytes())?)?;
        Ok(Self {
            inner: WebPkiServerVerifier::builder(Arc::new(roots)).build()?,
            expected: expected_fingerprint.map(|f| f.to_ascii_uppercase()),
        })
    }
}

impl Debug for PinnedServerCert {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl ServerCertVerifier for PinnedServerCert {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        // Hostname deliberately ignored: peers are addressed by IP and certs
        // carry no SAN. The fingerprint identifies the peer.
        verify_cert_from_der(end_entity.as_ref(), None)
            .map_err(|e| other_err(format!("server certificate invalid: {e:#}")))?;

        if let Some(expected) = &self.expected {
            let actual = fingerprint_from_cert_der(end_entity.as_ref());
            if &actual != expected {
                return Err(other_err(format!(
                    "server fingerprint mismatch: expected {expected}, got {actual}"
                )));
            }
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Build a rustls server config for the packet channel (client cert mandatory).
pub fn server_config(identity: &Identity) -> Result<Arc<rustls::ServerConfig>> {
    install_crypto();
    let certs = vec![CertificateDer::from_pem_slice(identity.cert_pem.as_bytes())
        .context("parse own certificate")?];
    let key = PrivateKeyDer::from_pem_slice(identity.key_pem.as_bytes())
        .context("parse own private key")?;
    let verifier = AcceptAnyClientCert::try_new(&identity.cert_pem)?;
    let mut cfg = rustls::ServerConfig::builder()
        .with_client_cert_verifier(Arc::new(verifier))
        .with_single_cert(certs, key)
        .context("build server TLS config")?;
    // Raw protocol: no HTTP on this port.
    cfg.alpn_protocols = Vec::new();
    Ok(Arc::new(cfg))
}

/// Build a rustls client config pinning `expected_fingerprint`.
pub fn client_config(
    identity: &Identity,
    expected_fingerprint: Option<String>,
) -> Result<Arc<rustls::ClientConfig>> {
    install_crypto();
    let certs = vec![CertificateDer::from_pem_slice(identity.cert_pem.as_bytes())
        .context("parse own certificate")?];
    let key = PrivateKeyDer::from_pem_slice(identity.key_pem.as_bytes())
        .context("parse own private key")?;
    let verifier = PinnedServerCert::try_new(&identity.cert_pem, expected_fingerprint)?;
    let mut cfg = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_client_auth_cert(certs, key)
        .context("build client TLS config")?;
    cfg.alpn_protocols = Vec::new();
    Ok(Arc::new(cfg))
}

/// Peer fingerprint from a completed server-side handshake.
pub fn server_peer_fingerprint(conn: &rustls::ServerConnection) -> Option<String> {
    conn.peer_certificates()
        .and_then(|certs| certs.first())
        .map(|c| fingerprint_from_cert_der(c.as_ref()))
}

/// Convenience: ServerName from an IP/hostname string.
pub fn server_name(host: &str) -> Result<ServerName<'static>> {
    // Strip IPv6 zone id (`fe80::1%3` → `fe80::1`).
    let bare = host.split('%').next().unwrap_or(host);
    ServerName::try_from(bare.to_string()).context("invalid server name")
}
