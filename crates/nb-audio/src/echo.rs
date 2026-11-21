use std::collections::VecDeque;

use crate::codec::{CHANNELS, FRAME_SAMPLES_PER_CH, SAMPLE_RATE};

pub const ECHO_HISTORY_MS: u64 = 1200;
pub const ECHO_CORR_THRESHOLD: f32 = 0.75;
pub const ECHO_MIN_MEAN_ABS: f32 = 200.0;
pub const ECHO_LAG_STEP: usize = 48;

pub const FRAME_SAMPLES: usize = FRAME_SAMPLES_PER_CH * CHANNELS;

pub struct EchoGuard {
    history: VecDeque<i16>,
    cap_samples: usize,
}

impl EchoGuard {
    pub fn new() -> Self {
        let cap = (SAMPLE_RATE as u64 * ECHO_HISTORY_MS / 1000) as usize * CHANNELS;
        EchoGuard {
            history: VecDeque::new(),
            cap_samples: cap.max(FRAME_SAMPLES),
        }
    }

    pub fn push_played(&mut self, pcm: &[i16]) {
        self.history.extend(pcm.iter().copied());
        while self.history.len() > self.cap_samples {
            self.history.pop_front();
        }
    }

    pub fn depth(&self) -> usize {
        self.history.len()
    }

    pub fn is_echo(&mut self, captured: &[i16]) -> bool {
        if captured.len() < FRAME_SAMPLES || self.history.len() < FRAME_SAMPLES {
            return false;
        }
        if mean_abs_slice(captured) < ECHO_MIN_MEAN_ABS {
            return false;
        }
        self.history.make_contiguous();
        let (head, _) = self.history.as_slices();
        if mean_abs_slice(head) < ECHO_MIN_MEAN_ABS {
            return false;
        }
        max_corr_over_lags(captured, head) >= ECHO_CORR_THRESHOLD
    }
}

impl Default for EchoGuard {
    fn default() -> Self {
        Self::new()
    }
}

fn mean_abs_slice(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: i64 = samples.iter().map(|s| s.unsigned_abs() as i64).sum();
    sum as f32 / samples.len() as f32
}

fn max_corr_over_lags(frame: &[i16], history: &[i16]) -> f32 {
    let n = FRAME_SAMPLES.min(frame.len()).min(history.len());
    if n == 0 {
        return 0.0;
    }
    let end = history.len().saturating_sub(n);
    let mut best = 0.0f32;
    let mut lag = 0;
    while lag <= end {
        let c = norm_corr(&frame[..n], &history[lag..lag + n]);
        if c > best {
            best = c;
            if best >= 0.999 {
                break;
            }
        }
        lag += ECHO_LAG_STEP;
    }
    let c = norm_corr(&frame[..n], &history[end..end + n]);
    if c > best {
        best = c;
    }
    best
}

fn norm_corr(a: &[i16], b: &[i16]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut ea = 0.0f64;
    let mut eb = 0.0f64;
    for i in 0..n {
        let x = a[i] as f64;
        let y = b[i] as f64;
        dot += x * y;
        ea += x * x;
        eb += y * y;
    }
    if ea <= 0.0 || eb <= 0.0 {
        return 0.0;
    }
    (dot / (ea.sqrt() * eb.sqrt())) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq_hz: f64, frames: usize, amp: f32) -> Vec<i16> {
        let mut out = Vec::with_capacity(frames * FRAME_SAMPLES);
        for f in 0..frames {
            for i in 0..FRAME_SAMPLES {
                let pair = (f * FRAME_SAMPLES + i) / CHANNELS;
                let t = pair as f64 / SAMPLE_RATE as f64;
                let v = (2.0 * std::f64::consts::PI * freq_hz * t).sin() * amp as f64;
                out.push(v as i16);
            }
        }
        out
    }

    fn delayed_window(src: &[i16], delay_samples: usize, scale: f32) -> Vec<i16> {
        src[delay_samples..delay_samples + FRAME_SAMPLES]
            .iter()
            .map(|s| (*s as f32 * scale) as i16)
            .collect()
    }

    #[test]
    fn delayed_copy_of_played_audio_is_echo() {
        let mut g = EchoGuard::new();
        let played = tone(440.0, 60, 8000.0);
        g.push_played(&played);
        assert!(g.is_echo(&delayed_window(&played, 4800, 0.6)));
        assert!(g.is_echo(&delayed_window(&played, 48, 1.0)));
    }

    #[test]
    fn unrelated_signal_passes_through() {
        let mut g = EchoGuard::new();
        g.push_played(&tone(440.0, 60, 8000.0));
        let desktop = tone(880.0, 1, 8000.0);
        assert!(!g.is_echo(&desktop));
    }

    #[test]
    fn silence_is_never_echo() {
        let mut g = EchoGuard::new();
        g.push_played(&tone(440.0, 60, 8000.0));
        assert!(!g.is_echo(&vec![0i16; FRAME_SAMPLES]));
        assert!(!g.is_echo(&vec![50i16; FRAME_SAMPLES]));
        let mut quiet = EchoGuard::new();
        quiet.push_played(&vec![0i16; FRAME_SAMPLES * 60]);
        assert!(!quiet.is_echo(&tone(440.0, 1, 8000.0)));
    }

    #[test]
    fn desktop_mix_with_faint_echo_passes() {
        let mut g = EchoGuard::new();
        let played = tone(440.0, 60, 8000.0);
        g.push_played(&played);
        let music = tone(660.0, 1, 9000.0);
        let echo = delayed_window(&played, 2400, 0.15);
        let mixed: Vec<i16> = music
            .iter()
            .zip(echo.iter())
            .map(|(m, e)| m.saturating_add(*e))
            .collect();
        assert!(!g.is_echo(&mixed));
    }

    #[test]
    fn short_history_does_not_false_positive() {
        let mut g = EchoGuard::new();
        assert!(!g.is_echo(&tone(440.0, 1, 8000.0)));
        assert!(!g.is_echo(&[]));
    }

    #[test]
    fn history_evicts_old_playback() {
        let mut g = EchoGuard::new();
        let played = tone(440.0, 60, 8000.0);
        g.push_played(&played);
        let captured = delayed_window(&played, 0, 1.0);
        assert!(g.is_echo(&captured));
        let filler_frames = (g.cap_samples / FRAME_SAMPLES) + 4;
        g.push_played(&vec![0i16; filler_frames * FRAME_SAMPLES]);
        assert!(!g.is_echo(&captured));
        assert!(g.depth() <= g.cap_samples);
    }
}
