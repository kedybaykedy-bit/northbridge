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
}

#[cfg(feature = "pipewire-cap")]
impl PipewireSource {
    pub fn connect() -> anyhow::Result<Self> {
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
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<i16>>(8);
        let state = CaptureState {
            tx,
            format: Arc::new(std::sync::Mutex::new((
                crate::codec::SAMPLE_RATE,
                crate::codec::CHANNELS as u32,
            ))),
        };
        let format_slot = state.format.clone();
        let props = properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::STREAM_CAPTURE_SINK => "true",
        };
        let stream = pw::stream::Stream::new(&core, "northbridge-capture", props)?;
        let _listener = stream
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
        std::thread::Builder::new()
            .name("nb-audio-pipewire".into())
            .spawn(move || mainloop.run())?;
        Ok(PipewireSource {
            rx,
            stash: VecDeque::new(),
            format: format_slot,
        })
    }
}

#[cfg(feature = "pipewire-cap")]
impl AudioSource for PipewireSource {
    fn next_frame(&mut self) -> Option<Vec<i16>> {
        let (rate, channels) = *self.format.lock().unwrap();
        for chunk in self.rx.try_iter() {
            self.stash.extend(chunk);
        }
        if rate != crate::codec::SAMPLE_RATE {
            self.stash.clear();
            return None;
        }
        let want = crate::codec::FRAME_SAMPLES_PER_CH * channels.max(1) as usize;
        for chunk in self.rx.try_iter() {
            self.stash.extend(chunk);
        }
        if self.stash.len() < want {
            return None;
        }
        let mut out = Vec::with_capacity(want);
        for _ in 0..want {
            out.push(self.stash.pop_front().unwrap_or(0));
        }
        Some(out)
    }

    fn source_name(&self) -> &'static str {
        "pipewire"
    }
}
