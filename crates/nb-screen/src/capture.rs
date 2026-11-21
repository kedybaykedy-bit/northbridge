use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::connection::RequestConnection as _;
use x11rb::protocol::shm::{self, ConnectionExt as _};
use x11rb::protocol::xproto::ImageFormat;
use x11rb::rust_connection::RustConnection;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    Auto,
    Plain,
    Shm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplaySelector {
    Default,
    Index(usize),
}

pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub depth: u8,
    pub data: Vec<u8>,
}

pub struct Grabber {
    conn: RustConnection,
    root: u32,
    width: u32,
    height: u32,
    shm: bool,
}

impl Grabber {
    pub fn connect(strategy: Strategy) -> Result<Self> {
        Self::connect_display(strategy, DisplaySelector::Default)
    }

    pub fn connect_display(strategy: Strategy, display: DisplaySelector) -> Result<Self> {
        let (conn, default_screen) = RustConnection::connect(None).context("X display")?;
        let num = match display {
            DisplaySelector::Default => default_screen,
            DisplaySelector::Index(i) => i,
        };
        let info = conn
            .setup()
            .roots
            .get(num)
            .context("display index out of range")?;
        let root = info.root;
        let width = u32::from(info.width_in_pixels);
        let height = u32::from(info.height_in_pixels);
        if width == 0 || height == 0 {
            anyhow::bail!("empty screen geometry");
        }
        let shm = match strategy {
            Strategy::Plain => false,
            Strategy::Auto | Strategy::Shm => shm_supported(&conn),
        };
        Ok(Self {
            conn,
            root,
            width,
            height,
            shm,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn shm(&self) -> bool {
        self.shm
    }

    pub fn grab(&self) -> Result<RawFrame> {
        if self.width > u32::from(u16::MAX) || self.height > u32::from(u16::MAX) {
            anyhow::bail!("screen too large for capture");
        }
        let reply = x11rb::protocol::xproto::get_image(
            &self.conn,
            ImageFormat::Z_PIXMAP,
            self.root,
            0,
            0,
            self.width as u16,
            self.height as u16,
            u32::MAX,
        )
        .context("get_image request")?
        .reply()
        .context("get_image reply")?;
        Ok(RawFrame {
            width: self.width,
            height: self.height,
            depth: reply.depth,
            data: reply.data,
        })
    }
}

fn shm_supported(conn: &RustConnection) -> bool {
    let present = conn
        .extension_information(shm::X11_EXTENSION_NAME)
        .ok()
        .flatten()
        .is_some();
    if !present {
        return false;
    }
    conn.shm_query_version()
        .ok()
        .and_then(|c| c.reply().ok())
        .is_some()
}

pub fn grab_scaled(grabber: &Grabber, max_w: u16) -> Result<(u32, u32, Vec<u8>)> {
    let raw = grabber.grab()?;
    let bgra = match raw.depth {
        24 | 32 => {
            let need = (raw.width as usize) * (raw.height as usize) * 4;
            if raw.data.len() < need {
                anyhow::bail!(
                    "short pixel buffer: {} < {}",
                    raw.data.len(),
                    need
                );
            }
            raw.data[..need].to_vec()
        }
        16 => rgb565_to_bgra(&raw.data, raw.width, raw.height)?,
        depth => anyhow::bail!("unsupported root depth {depth}"),
    };
    let target = u32::from(max_w.max(1));
    let dw = raw.width.min(target);
    let dh = ((u64::from(raw.height) * u64::from(dw)) / u64::from(raw.width)).max(1) as u32;
    let scaled = downscale_bgra(&bgra, raw.width, raw.height, dw, dh)?;
    Ok((dw, dh, scaled))
}

pub fn rgb565_to_bgra(src: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    let need = (w as usize) * (h as usize) * 2;
    if src.len() < need {
        anyhow::bail!("short rgb565 buffer: {} < {}", src.len(), need);
    }
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    for (i, dst) in out.chunks_exact_mut(4).enumerate() {
        let v = u16::from_le_bytes([src[2 * i], src[2 * i + 1]]);
        let r = ((v >> 11) & 0x1f) as u8;
        let g = ((v >> 5) & 0x3f) as u8;
        let b = (v & 0x1f) as u8;
        dst[0] = (b << 3) | (b >> 2);
        dst[1] = (g << 2) | (g >> 4);
        dst[2] = (r << 3) | (r >> 2);
        dst[3] = 255;
    }
    Ok(out)
}

pub fn downscale_bgra(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Result<Vec<u8>> {
    let need = (sw as usize) * (sh as usize) * 4;
    if src.len() < need {
        anyhow::bail!("short bgra buffer: {} < {}", src.len(), need);
    }
    if dw == 0 || dh == 0 {
        anyhow::bail!("empty downscale target");
    }
    if dw == sw && dh == sh {
        return Ok(src[..need].to_vec());
    }
    let mut out = vec![0u8; (dw as usize) * (dh as usize) * 4];
    for y in 0..dh {
        let y0 = (u64::from(y) * u64::from(sh)) / u64::from(dh);
        let y1 = ((u64::from(y + 1) * u64::from(sh)) / u64::from(dh)).max(y0 + 1);
        for x in 0..dw {
            let x0 = (u64::from(x) * u64::from(sw)) / u64::from(dw);
            let x1 = ((u64::from(x + 1) * u64::from(sw)) / u64::from(dw)).max(x0 + 1);
            let mut sum = [0u64; 4];
            let mut count = 0u64;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let o = ((sy * u64::from(sw) + sx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u64::from(src[o + c]);
                    }
                    count += 1;
                }
            }
            let o = (((u64::from(y) * u64::from(dw)) + u64::from(x)) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (sum[c] / count.max(1)) as u8;
            }
        }
    }
    Ok(out)
}

pub fn to_jpeg_bgra(bgra: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>> {
    if w == 0 || h == 0 || w > u32::from(u16::MAX) || h > u32::from(u16::MAX) {
        anyhow::bail!("bad jpeg dimensions {w}x{h}");
    }
    let need = (w as usize) * (h as usize) * 4;
    if bgra.len() < need {
        anyhow::bail!("short bgra buffer: {} < {}", bgra.len(), need);
    }
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(
            &bgra[..need],
            w as u16,
            h as u16,
            jpeg_encoder::ColorType::Bgra,
        )
        .map_err(|e| anyhow::anyhow!("jpeg encode: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            v.extend_from_slice(&px);
        }
        v
    }

    #[test]
    fn downscale_identity_returns_copy() {
        let src = solid(4, 2, [10, 20, 30, 255]);
        let out = downscale_bgra(&src, 4, 2, 4, 2).expect("copy");
        assert_eq!(out, src);
    }

    #[test]
    fn downscale_averages_blocks() {
        let mut src = Vec::new();
        for v in [0u8, 100, 0, 100] {
            src.extend_from_slice(&[v, v, v, 255]);
        }
        let out = downscale_bgra(&src, 2, 2, 1, 1).expect("scale");
        assert_eq!(out, vec![50, 50, 50, 255]);
    }

    #[test]
    fn downscale_rejects_short_input() {
        assert!(downscale_bgra(&[0u8; 8], 2, 2, 1, 1).is_err());
    }

    #[test]
    fn rgb565_converts_white_and_black() {
        let out = rgb565_to_bgra(&[0xff, 0xff], 1, 1).expect("white");
        assert_eq!(out, vec![255, 255, 255, 255]);
        let out = rgb565_to_bgra(&[0x00, 0x00], 1, 1).expect("black");
        assert_eq!(out, vec![0, 0, 0, 255]);
    }

    #[test]
    fn jpeg_encode_produces_soi() {
        let bgra = solid(64, 48, [32, 64, 128, 255]);
        let jpeg = to_jpeg_bgra(&bgra, 64, 48, 70).expect("encode");
        assert!(jpeg.len() > 4);
        assert_eq!(&jpeg[0..2], &[0xff, 0xd8]);
        assert_eq!(&jpeg[jpeg.len() - 2..], &[0xff, 0xd9]);
    }
}
