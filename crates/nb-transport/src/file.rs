use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use nb_core::identity::Identity;
use nb_core::packet::NetworkPacket;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const FILE_OFFER_KIND: &str = "nb.file.offer";
pub const FILE_CHUNK_KIND: &str = "nb.file.chunk";
pub const FILE_DONE_KIND: &str = "nb.file.done";

pub const FILE_PORT_OFFSET: u16 = 5;

pub const FILE_CHUNK_BYTES: usize = 48 * 1024;

pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

pub const MAX_CHUNKS_PER_FILE: u64 = MAX_FILE_BYTES / FILE_CHUNK_BYTES as u64 + 1;

pub fn file_port(http_port: u16) -> u16 {
    http_port.saturating_add(FILE_PORT_OFFSET)
}

pub fn sanitize_name(raw: &str) -> String {
    let base = Path::new(raw)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty() && n != "." && n != "..")
        .unwrap_or_else(|| "download".to_string());
    base.chars()
        .map(|c| {
            if c.is_control() || c == '/' || c == '\\' {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .chars()
        .take(255)
        .collect()
}

pub fn chunk_count(size: u64) -> u64 {
    if size == 0 {
        0
    } else {
        size.div_ceil(FILE_CHUNK_BYTES as u64)
    }
}

pub fn offer_packet(name: &str, size: u64, mime: &str) -> NetworkPacket {
    NetworkPacket::new(
        FILE_OFFER_KIND,
        json!({"name": name, "size": size, "mime": mime}),
    )
}

pub struct Offer {
    pub name: String,
    pub size: u64,
    pub mime: String,
}

pub fn parse_offer(body: &Value) -> Result<Offer> {
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("offer missing name"))?;
    let size = body
        .get("size")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("offer missing size"))?;
    let mime = body
        .get("mime")
        .and_then(|v| v.as_str())
        .unwrap_or("application/octet-stream")
        .to_string();
    let name = sanitize_name(name);
    if size > MAX_FILE_BYTES {
        bail!("file too large: {size} bytes (limit {MAX_FILE_BYTES})");
    }
    Ok(Offer { name, size, mime })
}

pub fn accept_packet(transfer: &str) -> NetworkPacket {
    NetworkPacket::new(
        FILE_OFFER_KIND,
        json!({"accept": true, "transfer": transfer}),
    )
}

pub fn decline_packet(reason: &str) -> NetworkPacket {
    NetworkPacket::new(
        FILE_OFFER_KIND,
        json!({"accept": false, "reason": reason}),
    )
}

pub struct OfferAnswer {
    pub accepted: bool,
    pub transfer: String,
    pub reason: String,
}

pub fn parse_offer_answer(body: &Value) -> Result<OfferAnswer> {
    let accepted = body
        .get("accept")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| anyhow::anyhow!("offer answer missing accept"))?;
    let transfer = body
        .get("transfer")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("declined")
        .to_string();
    if accepted && transfer.is_empty() {
        bail!("offer accepted without a transfer id");
    }
    Ok(OfferAnswer {
        accepted,
        transfer,
        reason,
    })
}

pub fn chunk_packet(transfer: &str, index: u64, raw: &[u8]) -> NetworkPacket {
    NetworkPacket::new(
        FILE_CHUNK_KIND,
        json!({"transfer": transfer, "index": index, "data": B64.encode(raw)}),
    )
}

pub struct Chunk {
    pub transfer: String,
    pub index: u64,
    pub data: Vec<u8>,
}

pub fn parse_chunk(body: &Value) -> Result<Chunk> {
    let transfer = body
        .get("transfer")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("chunk missing transfer"))?
        .to_string();
    let index = body
        .get("index")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("chunk missing index"))?;
    let data_b64 = body
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("chunk missing data"))?;
    if index >= MAX_CHUNKS_PER_FILE {
        bail!("chunk index out of range: {index}");
    }
    let data = B64
        .decode(data_b64)
        .context("chunk data is not valid base64")?;
    if data.len() > FILE_CHUNK_BYTES + 16 {
        bail!("chunk too large: {} bytes", data.len());
    }
    Ok(Chunk {
        transfer,
        index,
        data,
    })
}

pub fn chunk_ack(index: u64) -> NetworkPacket {
    NetworkPacket::new(FILE_CHUNK_KIND, json!({"ok": true, "index": index}))
}

pub fn done_packet(transfer: &str, count: u64, sha256: Option<&str>) -> NetworkPacket {
    let mut body = json!({"transfer": transfer, "count": count});
    if let Some(sum) = sha256 {
        body["sha256"] = Value::String(sum.to_string());
    }
    NetworkPacket::new(FILE_DONE_KIND, body)
}

pub struct Done {
    pub transfer: String,
    pub count: u64,
    pub sha256: Option<String>,
}

pub fn parse_done(body: &Value) -> Result<Done> {
    let transfer = body
        .get("transfer")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("done missing transfer"))?
        .to_string();
    let count = body
        .get("count")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("done missing count"))?;
    if count > MAX_CHUNKS_PER_FILE {
        bail!("done count out of range: {count}");
    }
    let sha256 = body
        .get("sha256")
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(ref sum) = sha256 {
        let ok = sum.len() == 64 && sum.chars().all(|c| c.is_ascii_hexdigit());
        if !ok {
            bail!("done has a malformed sha256");
        }
    }
    Ok(Done {
        transfer,
        count,
        sha256,
    })
}

pub fn done_ok(name: &str) -> NetworkPacket {
    NetworkPacket::new(FILE_DONE_KIND, json!({"ok": true, "name": name}))
}

pub fn done_fail(reason: &str) -> NetworkPacket {
    NetworkPacket::new(FILE_DONE_KIND, json!({"ok": false, "error": reason}))
}

pub fn parse_done_answer(body: &Value) -> Result<String> {
    let ok = body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if !ok {
        let reason = body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("receiver refused the file");
        bail!("{reason}");
    }
    body.get("name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("done answer missing name"))
}

struct ActiveTransfer {
    name: String,
    size: u64,
    mime: String,
    received: u64,
    file: File,
    path: PathBuf,
}

pub struct FileSink {
    dir: PathBuf,
    active: HashMap<String, ActiveTransfer>,
}

impl FileSink {
    pub fn new(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("create sink dir {}", dir.display()))?;
        Ok(Self {
            dir,
            active: HashMap::new(),
        })
    }

    pub fn handle(&mut self, pkt: &NetworkPacket) -> Option<NetworkPacket> {
        let out = match pkt.kind.as_str() {
            FILE_OFFER_KIND => self.on_offer(&pkt.body),
            FILE_CHUNK_KIND => self.on_chunk(&pkt.body),
            FILE_DONE_KIND => self.on_done(&pkt.body),
            _ => return None,
        };
        Some(match out {
            Ok(resp) => resp,
            Err(e) => match pkt.kind.as_str() {
                FILE_OFFER_KIND => decline_packet(&format!("{e:#}")),
                FILE_DONE_KIND => done_fail(&format!("{e:#}")),
                _ => NetworkPacket::new(
                    FILE_CHUNK_KIND,
                    json!({"ok": false, "error": format!("{e:#}")}),
                ),
            },
        })
    }

    fn unique_path(&self, name: &str) -> PathBuf {
        let mut candidate = self.dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
        let stem = Path::new(name)
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.to_string());
        let ext = Path::new(name)
            .extension()
            .map(|n| n.to_string_lossy().into_owned());
        for i in 1..1000u32 {
            let next = match &ext {
                Some(e) => format!("{stem} ({i}).{e}"),
                None => format!("{stem} ({i})"),
            };
            candidate = self.dir.join(next);
            if !candidate.exists() {
                return candidate;
            }
        }
        self.dir.join(format!("{stem}-{}", nb_core::packet::now_ms()))
    }

    fn on_offer(&mut self, body: &Value) -> Result<NetworkPacket> {
        let offer = parse_offer(body)?;
        let id = uuid::Uuid::new_v4().to_string();
        let path = self.unique_path(&offer.name);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("create {}", path.display()))?;
        if offer.size > 0 {
            file.set_len(offer.size).ok();
        }
        self.active.insert(
            id.clone(),
            ActiveTransfer {
                name: offer.name,
                size: offer.size,
                mime: offer.mime,
                received: 0,
                file,
                path,
            },
        );
        Ok(accept_packet(&id))
    }

    fn on_chunk(&mut self, body: &Value) -> Result<NetworkPacket> {
        let chunk = parse_chunk(body)?;
        let t = self
            .active
            .get_mut(&chunk.transfer)
            .ok_or_else(|| anyhow::anyhow!("unknown transfer"))?;
        let offset = chunk
            .index
            .checked_mul(FILE_CHUNK_BYTES as u64)
            .ok_or_else(|| anyhow::anyhow!("chunk offset overflow"))?;
        if offset + chunk.data.len() as u64 > t.size {
            bail!("chunk past end of file");
        }
        t.file
            .seek(SeekFrom::Start(offset))
            .context("seek transfer file")?;
        t.file.write_all(&chunk.data).context("write chunk")?;
        t.received += chunk.data.len() as u64;
        Ok(chunk_ack(chunk.index))
    }

    fn on_done(&mut self, body: &Value) -> Result<NetworkPacket> {
        let done = parse_done(body)?;
        let mut t = self
            .active
            .remove(&done.transfer)
            .ok_or_else(|| anyhow::anyhow!("unknown transfer"))?;
        t.file.flush().ok();
        let len = t
            .file
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);
        if len < t.size || t.size == 0 && len != 0 {
            let _ = std::fs::remove_file(&t.path);
            bail!("incomplete file: got {len} of {} bytes", t.size);
        }
        if t.size > 0 && chunk_count(t.size) != done.count {
            let _ = std::fs::remove_file(&t.path);
            bail!(
                "chunk count mismatch: got {} want {}",
                done.count,
                chunk_count(t.size)
            );
        }
        drop(t.file);
        let name = t
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| t.name.clone());
        let _ = t.mime;
        Ok(done_ok(&name))
    }

    pub fn pending(&self) -> usize {
        self.active.len()
    }
}

pub async fn send_file_via_packet(
    identity: &Identity,
    host: &str,
    file_port: u16,
    fingerprint: &str,
    paths: &[PathBuf],
    progress: Option<&dyn Fn(&str, u64, u64)>,
) -> Result<Vec<String>> {
    if paths.is_empty() {
        bail!("no files given");
    }
    let mut sent = Vec::new();
    for path in paths {
        let meta = std::fs::metadata(path)
            .with_context(|| format!("stat {}", path.display()))?;
        if !meta.is_file() {
            bail!("not a regular file: {}", path.display());
        }
        if meta.len() > MAX_FILE_BYTES {
            bail!(
                "{} is {} bytes, over the {MAX_FILE_BYTES}-byte packet-transfer limit",
                path.display(),
                meta.len()
            );
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".to_string());
        let mime = mime_guess::from_path(path).first_or_octet_stream().to_string();
        let size = meta.len();
        let offer = offer_packet(&name, size, &mime);
        let req = crate::send_packet(identity, host, file_port, fingerprint, &offer).await?;
        if req.kind != FILE_OFFER_KIND {
            bail!("unexpected response to offer: {}", req.kind);
        }
        let answer = parse_offer_answer(&req.body)?;
        if !answer.accepted {
            bail!("{} refused {name}: {}", host, answer.reason);
        }
        let transfer = answer.transfer;
        let mut file =
            File::open(path).with_context(|| format!("open {}", path.display()))?;
        let count = chunk_count(size);
        let mut buf = vec![0u8; FILE_CHUNK_BYTES];
        let mut index = 0u64;
        let mut done_bytes = 0u64;
        use std::io::Read;
        loop {
            let n = file
                .read(&mut buf)
                .with_context(|| format!("read {}", path.display()))?;
            if n == 0 {
                break;
            }
            let pkt = chunk_packet(&transfer, index, &buf[..n]);
            let resp = crate::send_packet(identity, host, file_port, fingerprint, &pkt).await?;
            if resp.kind != FILE_CHUNK_KIND
                || resp.body.get("ok").and_then(|v| v.as_bool()) != Some(true)
            {
                bail!("chunk {index} of {name} rejected");
            }
            index += 1;
            done_bytes += n as u64;
            if let Some(cb) = progress {
                cb(&name, done_bytes, size);
            }
        }
        if index != count {
            bail!("short read of {name}: sent {index} of {count} chunks");
        }
        let done = done_packet(&transfer, count, None);
        let resp = crate::send_packet(identity, host, file_port, fingerprint, &done).await?;
        if resp.kind != FILE_DONE_KIND {
            bail!("unexpected response to done: {}", resp.kind);
        }
        let saved = parse_done_answer(&resp.body)?;
        sent.push(saved);
    }
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_dirs_and_controls() {
        assert_eq!(sanitize_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_name(""), "download");
        assert_eq!(sanitize_name("a/b\\c"), "b_c");
        assert_eq!(sanitize_name("ok adı.pdf"), "ok adı.pdf");
    }

    #[test]
    fn chunk_count_edges() {
        assert_eq!(chunk_count(0), 0);
        assert_eq!(chunk_count(1), 1);
        assert_eq!(chunk_count(FILE_CHUNK_BYTES as u64), 1);
        assert_eq!(chunk_count(FILE_CHUNK_BYTES as u64 + 1), 2);
    }

    #[test]
    fn offer_roundtrip() {
        let pkt = offer_packet("a b.pdf", 42, "application/pdf");
        assert_eq!(pkt.kind, FILE_OFFER_KIND);
        let back = parse_offer(&pkt.body).unwrap();
        assert_eq!(back.name, "a b.pdf");
        assert_eq!(back.size, 42);
        assert!(parse_offer(&json!({"size": 1})).is_err());
        assert!(parse_offer(&json!({"name": "x", "size": MAX_FILE_BYTES + 1})).is_err());
    }

    #[test]
    fn chunk_roundtrip_and_limits() {
        let raw = vec![7u8; 100];
        let pkt = chunk_packet("t", 3, &raw);
        let back = parse_chunk(&pkt.body).unwrap();
        assert_eq!(back.index, 3);
        assert_eq!(back.data, raw);
        assert!(parse_chunk(&json!({"transfer": "t", "index": 0})).is_err());
        assert!(
            parse_chunk(&json!({"transfer": "t", "index": MAX_CHUNKS_PER_FILE, "data": ""}))
                .is_err()
        );
    }

    #[test]
    fn done_roundtrip() {
        let pkt = done_packet("t", 5, None);
        let back = parse_done(&pkt.body).unwrap();
        assert_eq!(back.count, 5);
        assert!(back.sha256.is_none());
        assert!(parse_done(&json!({"transfer": "t", "count": 1, "sha256": "zz"})).is_err());
        assert_eq!(parse_done_answer(&done_ok("f").body).unwrap(), "f");
        assert!(parse_done_answer(&done_fail("no").body).is_err());
        assert!(parse_offer_answer(&decline_packet("busy").body).unwrap().accepted == false);
    }
}
