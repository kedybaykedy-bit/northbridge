use anyhow::Result;

use crate::frame::{CODEC_OPUS, CODEC_PCM};

pub const SAMPLE_RATE: u32 = 48000;
pub const CHANNELS: usize = 2;
pub const FRAME_MS: u64 = 20;
pub const FRAME_SAMPLES_PER_CH: usize = 960;
pub const PCM_FRAME_BYTES: usize = FRAME_SAMPLES_PER_CH * CHANNELS * 2;
pub const OPUS_BITRATE: i32 = 64000;
#[cfg(feature = "opus")]
pub const OPUS_MAX_PACKET: usize = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Opus,
    Pcm,
}

impl Codec {
    pub fn as_u8(self) -> u8 {
        match self {
            Codec::Opus => CODEC_OPUS,
            Codec::Pcm => CODEC_PCM,
        }
    }

    pub fn from_u8(raw: u8) -> Option<Codec> {
        match raw {
            CODEC_OPUS => Some(Codec::Opus),
            CODEC_PCM => Some(Codec::Pcm),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Codec::Opus => "opus",
            Codec::Pcm => "pcm",
        }
    }
}

pub fn default_codec() -> Codec {
    #[cfg(feature = "opus")]
    return Codec::Opus;
    #[cfg(not(feature = "opus"))]
    return Codec::Pcm;
}

pub fn negotiate_codec(want: Option<&str>) -> Codec {
    match want {
        Some("opus") => {
            #[cfg(feature = "opus")]
            return Codec::Opus;
            #[cfg(not(feature = "opus"))]
            return Codec::Pcm;
        }
        Some("pcm") => Codec::Pcm,
        _ => default_codec(),
    }
}

pub fn pcm_encode(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

pub fn pcm_decode(bytes: &[u8]) -> Result<Vec<i16>> {
    if bytes.len() % 2 != 0 {
        anyhow::bail!("pcm payload has odd length: {}", bytes.len());
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        out.push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }
    Ok(out)
}

pub fn silence_pcm() -> Vec<i16> {
    vec![0i16; FRAME_SAMPLES_PER_CH * CHANNELS]
}

pub fn encode_frame(codec: Codec, samples: &[i16]) -> Result<Vec<u8>> {
    match codec {
        Codec::Pcm => Ok(pcm_encode(samples)),
        Codec::Opus => {
            #[cfg(feature = "opus")]
            return opus_encode(samples);
            #[cfg(not(feature = "opus"))]
            anyhow::bail!("opus support not compiled into this build");
        }
    }
}

pub fn decode_frame(codec_byte: u8, payload: &[u8]) -> Result<Vec<i16>> {
    match Codec::from_u8(codec_byte) {
        Some(Codec::Pcm) => pcm_decode(payload),
        Some(Codec::Opus) => {
            #[cfg(feature = "opus")]
            return opus_decode(payload);
            #[cfg(not(feature = "opus"))]
            anyhow::bail!("opus support not compiled into this build");
        }
        None => anyhow::bail!("unknown audio codec id: {codec_byte}"),
    }
}

#[cfg(feature = "opus")]
fn opus_encode(samples: &[i16]) -> Result<Vec<u8>> {
    let mut encoder = opus::Encoder::new(
        SAMPLE_RATE,
        opus::Channels::Stereo,
        opus::Application::Audio,
    )
    .map_err(|e| anyhow::anyhow!("opus encoder: {e}"))?;
    encoder
        .set_bitrate(opus::Bitrate::Bits(OPUS_BITRATE))
        .map_err(|e| anyhow::anyhow!("opus bitrate: {e}"))?;
    encoder
        .encode_vec(samples, OPUS_MAX_PACKET)
        .map_err(|e| anyhow::anyhow!("opus encode: {e}"))
}

#[cfg(feature = "opus")]
fn opus_decode(payload: &[u8]) -> Result<Vec<i16>> {
    let mut decoder = opus::Decoder::new(SAMPLE_RATE, opus::Channels::Stereo)
        .map_err(|e| anyhow::anyhow!("opus decoder: {e}"))?;
    let mut out = vec![0i16; FRAME_SAMPLES_PER_CH * CHANNELS * 6];
    let n = decoder
        .decode(payload, &mut out, false)
        .map_err(|e| anyhow::anyhow!("opus decode: {e}"))?;
    out.truncate(n * CHANNELS);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_roundtrip() {
        let samples: Vec<i16> = (0..FRAME_SAMPLES_PER_CH * CHANNELS)
            .map(|i| (i as i16).wrapping_mul(37))
            .collect();
        let bytes = pcm_encode(&samples);
        assert_eq!(bytes.len(), PCM_FRAME_BYTES);
        let back = pcm_decode(&bytes).expect("decode");
        assert_eq!(back, samples);
    }

    #[test]
    fn pcm_rejects_odd_length() {
        assert!(pcm_decode(&[0u8, 1, 2]).is_err());
        assert!(pcm_decode(&[]).is_ok());
    }

    #[test]
    fn silence_frame_shape() {
        let s = silence_pcm();
        assert_eq!(s.len(), FRAME_SAMPLES_PER_CH * CHANNELS);
        assert!(s.iter().all(|v| *v == 0));
        assert_eq!(pcm_encode(&s).len(), PCM_FRAME_BYTES);
    }

    #[test]
    fn negotiate_prefers_pcm_without_opus_feature() {
        assert_eq!(negotiate_codec(Some("pcm")), Codec::Pcm);
        assert_eq!(negotiate_codec(None).name(), default_codec().name());
        #[cfg(not(feature = "opus"))]
        assert_eq!(negotiate_codec(Some("opus")), Codec::Pcm);
    }

    #[test]
    fn encode_decode_pcm_frame() {
        let samples = silence_pcm();
        let payload = encode_frame(Codec::Pcm, &samples).expect("encode");
        let back = decode_frame(CODEC_PCM, &payload).expect("decode");
        assert_eq!(back, samples);
    }

    #[test]
    fn decode_rejects_unknown_codec() {
        assert!(decode_frame(7, &[0u8; 4]).is_err());
    }

    #[cfg(feature = "opus")]
    #[test]
    fn opus_roundtrip_20ms() {
        let samples = silence_pcm();
        let payload = encode_frame(Codec::Opus, &samples).expect("encode");
        assert!(!payload.is_empty());
        let back = decode_frame(CODEC_OPUS, &payload).expect("decode");
        assert_eq!(back.len(), samples.len());
    }
}
