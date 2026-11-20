//! File transfer over the LocalSend v2 HTTP API (send side).
//!
//! Reuses `LsHttpClientV2`: mTLS + fingerprint pinning, `prepare-upload` →
//! per-file `upload` with a streaming body. The receive side lives in the
//! `listen` event loop (nb-cli).

use anyhow::{Context, Result, bail};
use nb_core::identity::Identity;
use localsend::http::client::v2::LsHttpClientV2;
use localsend::http::dto_v2::{PrepareUploadRequestDtoV2, RegisterDtoV2};
use localsend::model::discovery::{DeviceType, ProtocolType, PROTOCOL_VERSION_V2};
use localsend::model::transfer::{FileContent, FileDto, FileMetadata};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Send `paths` to `host:http_port`, pinned to `fingerprint`.
/// Returns the file names that were accepted and uploaded.
pub async fn send_files(
    identity: &Identity,
    host: &str,
    http_port: u16,
    fingerprint: &str,
    paths: &[PathBuf],
) -> Result<Vec<String>> {
    if paths.is_empty() {
        bail!("no files given");
    }

    // No client-side timeout: it would bound the whole upload. Reachability
    // is handled by the dial timeout on the caller's side.
    let client = LsHttpClientV2::try_new(
        &identity.key_pem,
        &identity.cert_pem,
        Some(fingerprint.to_string()),
        None,
    )
    .map_err(|e| anyhow::anyhow!("TLS client: {e:#}"))?;

    let mut files: HashMap<String, FileDto> = HashMap::new();
    let mut by_id: HashMap<String, &PathBuf> = HashMap::new();
    for path in paths {
        let meta = std::fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?;
        if !meta.is_file() {
            bail!("not a regular file: {}", path.display());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| id.clone());
        files.insert(
            id.clone(),
            FileDto {
                id: id.clone(),
                file_name,
                size: meta.len(),
                file_type: mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string(),
                sha256: None,
                preview: None,
                metadata: FileMetadata::from_path(path),
            },
        );
        by_id.insert(id, path);
    }

    let info = RegisterDtoV2 {
        alias: identity.alias.clone(),
        version: PROTOCOL_VERSION_V2.to_string(),
        device_model: Some("northbridge".to_string()),
        device_type: Some(DeviceType::Desktop),
        fingerprint: identity.fingerprint.clone(),
        port: identity.port,
        protocol: ProtocolType::Https,
        download: false,
    };

    let result = client
        .prepare_upload(
            ProtocolType::Https,
            host,
            http_port,
            None,
            PrepareUploadRequestDtoV2 { info, files },
            None,
            CancellationToken::new(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("prepare-upload: {e:#}"))?;

    let Some(resp) = result.response else {
        bail!(
            "{} declined the transfer (HTTP {})",
            host,
            result.status_code
        );
    };

    let mut sent = Vec::new();
    for (file_id, token) in &resp.files {
        let Some(path) = by_id.get(file_id) else {
            continue;
        };
        let content = FileContent::Path(PathBuf::from(path));
        let body = localsend::reqwest::Body::wrap_stream(content.into_stream());
        client
            .upload(
                ProtocolType::Https,
                host,
                http_port,
                None,
                &resp.session_id,
                file_id,
                token,
                body,
                CancellationToken::new(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("upload {}: {e:#}", path.display()))?;
        sent.push(
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| file_id.clone()),
        );
    }
    Ok(sent)
}

/// Short dial timeout used by callers before invoking [`send_files`].
pub const DIAL_HINT: Duration = Duration::from_secs(5);
