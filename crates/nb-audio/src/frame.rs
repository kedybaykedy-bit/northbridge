use anyhow::{Context, Result};

pub const MAGIC: [u8; 4] = *b"NBA1";
pub const VERSION: u8 = 1;
pub const CODEC_OPUS: u8 = 0;
pub const CODEC_PCM: u8 = 1;
pub const FLAG_CONFIG: u8 = 0x01;
pub const FLAG_DIR: u8 = 0x02;
pub const FLAG_DTX: u8 = 0x04;
pub const HEADER_LEN: usize = 24;
pub const MAX_PAYLOAD: u32 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    DeskToPhone,
    PhoneToDesk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameMeta {
    pub seq: u32,
    pub pts_ms: u64,
    pub codec: u8,
    pub flags: u8,
    pub channels: u8,
}

pub fn direction_of(flags: u8) -> Direction {
    if flags & FLAG_DIR == 0 {
        Direction::DeskToPhone
    } else {
        Direction::PhoneToDesk
    }
}

pub fn build_header(meta: &FrameMeta, payload_len: u32) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[0..4].copy_from_slice(&MAGIC);
    out[4] = VERSION;
    out[5] = meta.codec;
    out[6] = meta.flags;
    out[7] = meta.channels;
    out[8..12].copy_from_slice(&meta.seq.to_le_bytes());
    out[12..20].copy_from_slice(&meta.pts_ms.to_le_bytes());
    out[20..24].copy_from_slice(&payload_len.to_le_bytes());
    out
}

pub fn parse_header(buf: &[u8]) -> Result<(FrameMeta, u32)> {
    if buf.len() < HEADER_LEN {
        anyhow::bail!("audio header too short: {} bytes", buf.len());
    }
    if buf[0..4] != MAGIC {
        anyhow::bail!("bad audio magic");
    }
    if buf[4] != VERSION {
        anyhow::bail!("unsupported audio version: {}", buf[4]);
    }
    if buf[5] != CODEC_OPUS && buf[5] != CODEC_PCM {
        anyhow::bail!("unsupported audio codec: {}", buf[5]);
    }
    let len = u32::from_le_bytes(buf[20..24].try_into().context("len")?);
    if len > MAX_PAYLOAD {
        anyhow::bail!("audio payload too large: {len} bytes");
    }
    let meta = FrameMeta {
        seq: u32::from_le_bytes(buf[8..12].try_into().context("seq")?),
        pts_ms: u64::from_le_bytes(buf[12..20].try_into().context("pts")?),
        codec: buf[5],
        flags: buf[6],
        channels: buf[7],
    };
    Ok((meta, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let meta = FrameMeta {
            seq: 77,
            pts_ms: 1795142400123,
            codec: CODEC_PCM,
            flags: FLAG_DIR,
            channels: 2,
        };
        let bytes = build_header(&meta, 3840);
        assert_eq!(bytes.len(), HEADER_LEN);
        assert_eq!(&bytes[0..4], b"NBA1");
        assert_eq!(bytes[4], 1);
        assert_eq!(bytes[5], CODEC_PCM);
        assert_eq!(bytes[6], FLAG_DIR);
        assert_eq!(bytes[7], 2);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 77);
        assert_eq!(
            u64::from_le_bytes(bytes[12..20].try_into().unwrap()),
            1795142400123
        );
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 3840);
        let (back, len) = parse_header(&bytes).expect("parse");
        assert_eq!(back, meta);
        assert_eq!(len, 3840);
    }

    #[test]
    fn header_rejects_short_bad_magic_bad_version_bad_codec_big_len() {
        assert!(parse_header(&[0u8; 10]).is_err());
        let base = FrameMeta {
            seq: 0,
            pts_ms: 0,
            codec: CODEC_PCM,
            flags: 0,
            channels: 2,
        };
        let mut bad = build_header(&base, 5);
        bad[0] = b'X';
        assert!(parse_header(&bad).is_err());
        let mut bad = build_header(&base, 5);
        bad[4] = 9;
        assert!(parse_header(&bad).is_err());
        let mut bad = build_header(&base, 5);
        bad[5] = 7;
        assert!(parse_header(&bad).is_err());
        let mut bad = build_header(&base, 5);
        bad[20..24].copy_from_slice(&(MAX_PAYLOAD + 1).to_le_bytes());
        assert!(parse_header(&bad).is_err());
    }

    #[test]
    fn direction_bit_matches_research_table() {
        assert_eq!(direction_of(0x00), Direction::DeskToPhone);
        assert_eq!(direction_of(FLAG_CONFIG), Direction::DeskToPhone);
        assert_eq!(direction_of(FLAG_DIR), Direction::PhoneToDesk);
        assert_eq!(
            direction_of(FLAG_DIR | FLAG_DTX),
            Direction::PhoneToDesk
        );
    }
}
