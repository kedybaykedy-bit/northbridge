#[cfg(feature = "cpal-out")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(feature = "cpal-out")]
use std::collections::VecDeque;
#[cfg(feature = "cpal-out")]
use std::sync::{Arc, Mutex};

pub trait AudioSink: Send {
    fn push(&mut self, samples: &[i16]);
    fn sink_name(&self) -> &'static str;
}

pub struct NullSink {
    pub accepted_samples: u64,
}

impl NullSink {
    pub fn new() -> Self {
        NullSink {
            accepted_samples: 0,
        }
    }
}

impl Default for NullSink {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioSink for NullSink {
    fn push(&mut self, samples: &[i16]) {
        self.accepted_samples += samples.len() as u64;
    }

    fn sink_name(&self) -> &'static str {
        "none"
    }
}

#[cfg(feature = "cpal-out")]
pub struct CpalSink {
    ring: Arc<Mutex<VecDeque<f32>>>,
    _stream: cpal::Stream,
}

#[cfg(feature = "cpal-out")]
impl CpalSink {
    pub fn open() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("no default output device"))?;
        let supported = device.default_output_config()?;
        let config: cpal::StreamConfig = supported.into();
        let ring: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let producer = ring.clone();
        let cap = config.sample_rate as usize * config.channels as usize;
        let stream = device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut guard = producer.lock().unwrap();
                for slot in data.iter_mut() {
                    *slot = guard.pop_front().unwrap_or(0.0);
                }
            },
            |err| eprintln!("[audio] output stream error: {err}"),
            None,
        )?;
        stream.play()?;
        Ok(CpalSink {
            ring,
            _stream: stream,
        })
    }
}

#[cfg(feature = "cpal-out")]
impl AudioSink for CpalSink {
    fn push(&mut self, samples: &[i16]) {
        let mut guard = self.ring.lock().unwrap();
        let cap = crate::codec::SAMPLE_RATE as usize * crate::codec::CHANNELS * 2;
        for s in samples {
            if guard.len() >= cap {
                guard.pop_front();
            }
            guard.push_back(*s as f32 / 32768.0);
        }
    }

    fn sink_name(&self) -> &'static str {
        "cpal"
    }
}
