#[cfg(feature = "pipewire-cap")]
use std::collections::VecDeque;

pub trait AudioSource: Send {
    fn next_frame(&mut self) -> Option<Vec<i16>>;
    fn source_name(&self) -> &'static str;
}

pub struct NullSource;

impl AudioSource for NullSource {
    fn next_frame(&mut self) -> Option<Vec<i16>> {
        None
    }

    fn source_name(&self) -> &'static str {
        "none"
    }
}

pub fn default_source() -> NullSource {
    NullSource
}

pub fn open_source() -> Box<dyn AudioSource> {
    #[cfg(feature = "pipewire-cap")]
    if let Ok(source) = PipewireSource::connect() {
        return Box::new(source);
    }
    Box::new(NullSource)
}

pub fn unavailable_hint() -> &'static str {
    "no audio capture source available in this build; for live system-sound capture install pipewire-devel opus-devel alsa-lib-devel (sudo dnf install pipewire-devel opus-devel alsa-lib-devel) and rebuild with --features pipewire-cap,opus,cpal-out"
}

#[cfg(feature = "pipewire-cap")]
struct CaptureState {
    tx: std::sync::mpsc::SyncSender<Vec<i16>>,
    format: Arc<std::sync::Mutex<(u32, u32)>>,
}

#[cfg(feature = "pipewire-cap")]
use std::sync::Arc;

#[cfg(feature = "pipewire-cap")]
pub struct PipewireSource {
    rx: std::sync::mpsc::Receiver<Vec<i16>>,
    stash: VecDeque<i16>,
    format: Arc<std::sync::Mutex<(u32, u32)>>,
    phase: f64,
}

#[cfg(feature = "pipewire-cap")]
impl PipewireSource {
    pub fn connect() -> anyhow::Result<Self> {
        use anyhow::Context;
        let (samples_tx, rx) = std::sync::mpsc::sync_channel::<Vec<i16>>(8);
        let format = Arc::new(std::sync::Mutex::new((
            crate::codec::SAMPLE_RATE,
            crate::codec::CHANNELS as u32,
        )));
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<anyhow::Result<()>>(1);
        let format_in = Arc::clone(&format);
        std::thread::Builder::new()
            .name("nb-audio-pipewire".into())
            .spawn(move || run_mainloop(samples_tx, format_in, ready_tx))?;
        ready_rx
            .recv()
            .context("pipewire thread exited during setup")??;
        Ok(PipewireSource {
            rx,
            stash: VecDeque::new(),
            format,
            phase: 0.0,
        })
    }
}

#[cfg(feature = "pipewire-cap")]
fn run_mainloop(
    samples_tx: std::sync::mpsc::SyncSender<Vec<i16>>,
    format: Arc<std::sync::Mutex<(u32, u32)>>,
    ready_tx: std::sync::mpsc::SyncSender<anyhow::Result<()>>,
) {
    match build_pipeline(samples_tx, format) {
        Ok((mainloop, stream, listener)) => {
            let _ = ready_tx.send(Ok(()));
            mainloop.run();
            drop((stream, listener));
        }
        Err(err) => {
            let _ = ready_tx.send(Err(err));
        }
    }
}

#[cfg(feature = "pipewire-cap")]
fn build_pipeline(
    samples_tx: std::sync::mpsc::SyncSender<Vec<i16>>,
    format: Arc<std::sync::Mutex<(u32, u32)>>,
) -> anyhow::Result<(
    pipewire::main_loop::MainLoop,
    pipewire::stream::Stream,
    pipewire::stream::StreamListener<CaptureState>,
)> {
    use pipewire as pw;
    use pw::properties::properties;
    use pw::spa::param::format::{MediaSubtype, MediaType};
    use pw::spa::param::format_utils;
    use pw::spa::pod::Pod;
    use std::convert::TryInto;
    use std::mem;

    pw::init();
    let mainloop = pw::main_loop::MainLoop::new(None)?;
    let context = pw::context::Context::new(&mainloop)?;
    let core = context.connect(None)?;
    let state = CaptureState {
        tx: samples_tx,
        format,
    };
        let props = properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::STREAM_CAPTURE_SINK => "true",
        };
        let stream = pw::stream::Stream::new(&core, "northbridge-capture", props)?;
        let listener = stream
            .add_local_listener_with_user_data(state)
            .param_changed(|_, state, id, param| {
                let Some(param) = param else { return };
                if id != pw::spa::param::ParamType::Format.as_raw() {
                    return;
                }
                if let Ok((media_type, media_subtype)) = format_utils::parse_format(param) {
                    if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                        return;
                    }
                }
                let mut info = pw::spa::param::audio::AudioInfoRaw::new();
                if info.parse(param).is_ok() {
                    *state.format.lock().unwrap() = (info.rate(), info.channels());
                }
            })
            .process(|stream, state| match stream.dequeue_buffer() {
                None => {}
                Some(mut buffer) => {
                    let datas = buffer.datas_mut();
                    if datas.is_empty() {
                        return;
                    }
                    let data = &mut datas[0];
                    let size = data.chunk().size() as usize;
                    let stride = mem::size_of::<f32>();
                    if let Some(raw) = data.data() {
                        let mut pcm = Vec::with_capacity(size / stride);
                        for chunk in raw[..size.min(raw.len())].chunks_exact(stride) {
                            let f = f32::from_le_bytes(chunk.try_into().unwrap_or([0, 0, 0, 0]));
                            pcm.push((f.clamp(-1.0, 1.0) * 32767.0) as i16);
                        }
                        let _ = state.tx.try_send(pcm);
                    }
                }
            })
            .register()?;
        let mut audio_info = pw::spa::param::audio::AudioInfoRaw::new();
        audio_info.set_format(pw::spa::param::audio::AudioFormat::F32LE);
        let obj = pw::spa::pod::Object {
            type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: pw::spa::param::ParamType::EnumFormat.as_raw(),
            properties: audio_info.into(),
        };
        let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &pw::spa::pod::Value::Object(obj),
        )
        .unwrap()
        .0
        .into_inner();
        let mut params = [Pod::from_bytes(&values).unwrap()];
        stream.connect(
            pw::spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )?;
        Ok((mainloop, stream, listener))
    }

#[cfg(feature = "pipewire-cap")]
fn fold_to_stereo(raw: &[i16], channels: usize) -> Vec<i16> {
    let channels = channels.max(1);
    let frames = raw.len() / channels;
    let mut out = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        let base = f * channels;
        if channels == 1 {
            let s = raw[base];
            out.push(s);
            out.push(s);
        } else {
            out.push(raw[base]);
            out.push(raw[base + 1]);
        }
    }
    out
}

#[cfg(feature = "pipewire-cap")]
fn resample_to_stereo_48k(
    stash: &VecDeque<i16>,
    rate: u32,
    channels: usize,
    phase: f64,
) -> Option<(Vec<i16>, usize, f64)> {
    if rate == 0 || channels == 0 {
        return None;
    }
    let step = rate as f64 / crate::codec::SAMPLE_RATE as f64;
    let last = phase + (crate::codec::FRAME_SAMPLES_PER_CH - 1) as f64 * step;
    let need = last.floor() as usize + 1;
    if stash.len() < need * channels {
        return None;
    }
    let avail = stash.len() / channels;
    let mut out = Vec::with_capacity(crate::codec::FRAME_SAMPLES_PER_CH * 2);
    for i in 0..crate::codec::FRAME_SAMPLES_PER_CH {
        let pos = phase + i as f64 * step;
        let i0 = (pos.floor() as usize).min(avail - 1);
        let frac = (pos - pos.floor()) as f32;
        let i1 = (i0 + 1).min(avail - 1);
        for c in 0..2 {
            let col = if channels == 1 { 0 } else { c.min(channels - 1) };
            let a = stash[i0 * channels + col] as f32;
            let b = stash[i1 * channels + col] as f32;
            out.push((a + (b - a) * frac) as i16);
        }
    }
    let used_frames = need;
    let next = phase + crate::codec::FRAME_SAMPLES_PER_CH as f64 * step - used_frames as f64;
    Some((out, used_frames * channels, next))
}

#[cfg(feature = "pipewire-cap")]
impl AudioSource for PipewireSource {
    fn next_frame(&mut self) -> Option<Vec<i16>> {
        let (rate, channels) = *self.format.lock().unwrap();
        for chunk in self.rx.try_iter() {
            self.stash.extend(chunk);
        }
        let channels = channels.max(1) as usize;
        if rate == crate::codec::SAMPLE_RATE {
            let want = crate::codec::FRAME_SAMPLES_PER_CH * channels;
            if self.stash.len() < want {
                return None;
            }
            let mut raw = Vec::with_capacity(want);
            for _ in 0..want {
                raw.push(self.stash.pop_front().unwrap_or(0));
            }
            Some(fold_to_stereo(&raw, channels))
        } else {
            let (frame, used, next) =
                resample_to_stereo_48k(&self.stash, rate, channels, self.phase)?;
            let used = used.min(self.stash.len());
            for _ in 0..used {
                self.stash.pop_front();
            }
            self.phase = next;
            Some(frame)
        }
    }

    fn source_name(&self) -> &'static str {
        "pipewire"
    }
}

#[cfg(all(test, feature = "pipewire-cap"))]
mod tests {
    use super::*;

    #[test]
    fn fold_mono_duplicates_to_stereo() {
        let raw = vec![10i16, 20, 30];
        assert_eq!(fold_to_stereo(&raw, 1), vec![10, 10, 20, 20, 30, 30]);
    }

    #[test]
    fn fold_stereo_passes_through_and_truncates_extra() {
        let raw = vec![1i16, 2, 3, 4];
        assert_eq!(fold_to_stereo(&raw, 2), vec![1, 2, 3, 4]);
        let raw = vec![1i16, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(fold_to_stereo(&raw, 4), vec![1, 2, 5, 6]);
    }

    #[test]
    fn resample_same_rate_matches_fold() {
        let raw: Vec<i16> = (0..960 * 2).map(|i| i as i16).collect();
        let mut stash = VecDeque::new();
        stash.extend(raw.clone());
        let (frame, used, next) = resample_to_stereo_48k(&stash, 48000, 2, 0.0).expect("frame");
        assert_eq!(frame, raw);
        assert_eq!(used, 960 * 2);
        assert_eq!(next, 0.0);
    }

    #[test]
    fn resample_44100_constant_tone_stays_constant() {
        let raw = vec![1000i16; 900 * 2];
        let mut stash = VecDeque::new();
        stash.extend(raw);
        let (frame, used, next) = resample_to_stereo_48k(&stash, 44100, 2, 0.0).expect("frame");
        assert_eq!(frame.len(), 960 * 2);
        assert!(frame.iter().all(|&s| (s - 1000).abs() <= 1));
        assert_eq!(used, 882 * 2);
        assert_eq!(next, 0.0);
    }

    #[test]
    fn resample_short_stash_returns_none() {
        let mut stash = VecDeque::new();
        stash.extend(vec![0i16; 100]);
        assert!(resample_to_stereo_48k(&stash, 44100, 2, 0.0).is_none());
        assert!(resample_to_stereo_48k(&stash, 0, 2, 0.0).is_none());
    }
}
