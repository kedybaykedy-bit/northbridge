use anyhow::Result;
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

pub struct DecodedFrame {
    pub width: u16,
    pub height: u16,
    pub bgra: Vec<u8>,
}

pub struct H264Decoder {
    inner: Decoder,
    pub decoded: u64,
    pub errors: u64,
}

impl H264Decoder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: Decoder::new()?,
            decoded: 0,
            errors: 0,
        })
    }

    pub fn decode(&mut self, annexb: &[u8]) -> Option<DecodedFrame> {
        let yuv = match self.inner.decode(annexb) {
            Ok(Some(yuv)) => yuv,
            Ok(None) => return None,
            Err(e) => {
                self.errors += 1;
                if self.errors <= 5 || self.errors % 120 == 0 {
                    eprintln!("[mirror] decode error: {e}");
                }
                return None;
            }
        };
        let (w, h) = yuv.dimensions();
        if w == 0 || h == 0 || w > u16::MAX as usize || h > u16::MAX as usize {
            self.errors += 1;
            return None;
        }
        let mut rgba = vec![0u8; w * h * 4];
        yuv.write_rgba8(&mut rgba);
        rgba_to_bgra_in_place(&mut rgba);
        self.decoded += 1;
        Some(DecodedFrame {
            width: w as u16,
            height: h as u16,
            bgra: rgba,
        })
    }
}

pub fn rgba_to_bgra_in_place(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_swaps_to_bgra() {
        let mut buf = vec![10u8, 20, 30, 255, 1, 2, 3, 4];
        rgba_to_bgra_in_place(&mut buf);
        assert_eq!(buf, vec![30u8, 20, 10, 255, 3, 2, 1, 4]);
    }

    #[test]
    fn decoder_starts_empty() {
        let dec = H264Decoder::new().expect("decoder");
        assert_eq!(dec.decoded, 0);
        assert_eq!(dec.errors, 0);
    }

    #[test]
    fn decoder_tolerates_garbage() {
        let mut dec = H264Decoder::new().expect("decoder");
        assert!(dec.decode(&[0u8; 64]).is_none());
        assert!(dec.decode(&[]).is_none());
    }
}
