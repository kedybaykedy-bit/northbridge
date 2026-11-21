use anyhow::{Context, Result};

pub const MAGIC: [u8; 4] = *b"NBS1";
pub const VERSION: u8 = 1;
pub const CODEC_JPEG: u8 = 0;
pub const HEADER_LEN: usize = 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameMeta {
    pub seq: u32,
    pub ts_ms: u64,
    pub width: u16,
    pub height: u16,
    pub flags: u16,
}

pub fn build_header(meta: &FrameMeta, payload_len: u32) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[0..4].copy_from_slice(&MAGIC);
    out[4] = VERSION;
    out[5] = CODEC_JPEG;
    out[6..8].copy_from_slice(&meta.flags.to_be_bytes());
    out[8..12].copy_from_slice(&meta.seq.to_be_bytes());
    out[12..20].copy_from_slice(&meta.ts_ms.to_be_bytes());
    out[20..22].copy_from_slice(&meta.width.to_be_bytes());
    out[22..24].copy_from_slice(&meta.height.to_be_bytes());
    out[24..28].copy_from_slice(&payload_len.to_be_bytes());
    out
}

pub fn parse_header(buf: &[u8]) -> Result<(FrameMeta, u32)> {
    if buf.len() < HEADER_LEN {
        anyhow::bail!("frame header too short: {} bytes", buf.len());
    }
    if buf[0..4] != MAGIC {
        anyhow::bail!("bad frame magic");
    }
    if buf[4] != VERSION {
        anyhow::bail!("unsupported frame version: {}", buf[4]);
    }
    if buf[5] != CODEC_JPEG {
        anyhow::bail!("unsupported frame codec: {}", buf[5]);
    }
    let flags = u16::from_be_bytes(buf[6..8].try_into().context("flags")?);
    let seq = u32::from_be_bytes(buf[8..12].try_into().context("seq")?);
    let ts_ms = u64::from_be_bytes(buf[12..20].try_into().context("ts")?);
    let width = u16::from_be_bytes(buf[20..22].try_into().context("width")?);
    let height = u16::from_be_bytes(buf[22..24].try_into().context("height")?);
    let len = u32::from_be_bytes(buf[24..28].try_into().context("len")?);
    let meta = FrameMeta {
        seq,
        ts_ms,
        width,
        height,
        flags,
    };
    Ok((meta, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let meta = FrameMeta {
            seq: 41,
            ts_ms: 1795142400123,
            width: 1280,
            height: 720,
            flags: 0,
        };
        let bytes = build_header(&meta, 91234);
        assert_eq!(bytes.len(), HEADER_LEN);
        assert_eq!(&bytes[0..4], b"NBS1");
        assert_eq!(bytes[4], 1);
        assert_eq!(bytes[5], 0);
        let (back, len) = parse_header(&bytes).expect("parse");
        assert_eq!(back, meta);
        assert_eq!(len, 91234);
    }

    #[test]
    fn header_rejects_short_bad_magic_bad_version() {
        assert!(parse_header(&[0u8; 10]).is_err());
        let mut bad = build_header(
            &FrameMeta {
                seq: 0,
                ts_ms: 0,
                width: 1,
                height: 1,
                flags: 0,
            },
            5,
        );
        bad[0] = b'X';
        assert!(parse_header(&bad).is_err());
        let mut bad = build_header(
            &FrameMeta {
                seq: 0,
                ts_ms: 0,
                width: 1,
                height: 1,
                flags: 0,
            },
            5,
        );
        bad[4] = 9;
        assert!(parse_header(&bad).is_err());
        let mut bad = build_header(
            &FrameMeta {
                seq: 0,
                ts_ms: 0,
                width: 1,
                height: 1,
                flags: 0,
            },
            5,
        );
        bad[5] = 7;
        assert!(parse_header(&bad).is_err());
    }
}
