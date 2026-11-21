use std::collections::VecDeque;

pub const AUDIO_QUEUE_CAP: usize = 32;
pub const JITTER_PREBUFFER: usize = 3;
pub const JITTER_MAX_CONCEAL: usize = 10;

#[derive(Clone, Debug)]
pub struct QueuedAudio {
    pub codec: u8,
    pub flags: u8,
    pub channels: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug)]
pub struct AudioQueue {
    cap: usize,
    frames: VecDeque<QueuedAudio>,
}

impl AudioQueue {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            frames: VecDeque::new(),
        }
    }

    pub fn push(&mut self, frame: QueuedAudio) {
        while self.frames.len() >= self.cap {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }

    pub fn pop_oldest(&mut self) -> Option<QueuedAudio> {
        self.frames.pop_front()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct JitterStats {
    pub received: u64,
    pub played: u64,
    pub concealed: u64,
    pub reprimes: u64,
}

#[derive(Debug)]
pub struct JitterBuffer {
    cap: usize,
    prebuffer: usize,
    max_conceal: usize,
    frames: VecDeque<Vec<i16>>,
    last: Option<Vec<i16>>,
    primed: bool,
    conceal_run: usize,
    stats: JitterStats,
}

impl JitterBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            prebuffer: JITTER_PREBUFFER,
            max_conceal: JITTER_MAX_CONCEAL,
            frames: VecDeque::new(),
            last: None,
            primed: false,
            conceal_run: 0,
            stats: JitterStats::default(),
        }
    }

    pub fn with_tuning(mut self, prebuffer: usize, max_conceal: usize) -> Self {
        self.prebuffer = prebuffer.max(1);
        self.max_conceal = max_conceal.max(1);
        self
    }

    pub fn push(&mut self, pcm: Vec<i16>) {
        while self.frames.len() >= self.cap {
            self.frames.pop_front();
        }
        self.frames.push_back(pcm);
        self.stats.received += 1;
    }

    pub fn next(&mut self) -> Vec<i16> {
        if !self.primed {
            if self.frames.len() >= self.prebuffer {
                self.primed = true;
            } else {
                return crate::codec::silence_pcm();
            }
        }
        match self.frames.pop_front() {
            Some(pcm) => {
                self.conceal_run = 0;
                self.stats.played += 1;
                self.last = Some(pcm.clone());
                pcm
            }
            None => {
                self.conceal_run += 1;
                if self.conceal_run > self.max_conceal {
                    self.primed = false;
                    self.conceal_run = 0;
                    self.stats.reprimes += 1;
                    crate::codec::silence_pcm()
                } else {
                    self.stats.concealed += 1;
                    match self.last.as_ref() {
                        Some(prev) => prev.iter().map(|s| (*s as i32 / 2) as i16).collect(),
                        None => crate::codec::silence_pcm(),
                    }
                }
            }
        }
    }

    pub fn primed(&self) -> bool {
        self.primed
    }

    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    pub fn stats(&self) -> &JitterStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{FRAME_SAMPLES_PER_CH, silence_pcm};
    use crate::frame::CODEC_PCM;

    fn queued(tag: u8) -> QueuedAudio {
        QueuedAudio {
            codec: CODEC_PCM,
            flags: 0,
            channels: 2,
            payload: vec![tag],
        }
    }

    fn tone(tag: i16) -> Vec<i16> {
        vec![tag; FRAME_SAMPLES_PER_CH * 2]
    }

    #[test]
    fn push_drops_oldest_when_full() {
        let mut q = AudioQueue::new(3);
        q.push(queued(1));
        q.push(queued(2));
        q.push(queued(3));
        assert_eq!(q.len(), 3);
        q.push(queued(4));
        assert_eq!(q.len(), 3);
        let oldest = q.pop_oldest().expect("frame");
        assert_eq!(oldest.payload, vec![2]);
        let oldest = q.pop_oldest().expect("frame");
        assert_eq!(oldest.payload, vec![3]);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn pop_oldest_is_fifo_without_loss() {
        let mut q = AudioQueue::new(4);
        assert!(q.pop_oldest().is_none());
        q.push(queued(1));
        q.push(queued(2));
        assert_eq!(q.pop_oldest().expect("frame").payload, vec![1]);
        assert_eq!(q.pop_oldest().expect("frame").payload, vec![2]);
        assert!(q.pop_oldest().is_none());
        assert!(q.is_empty());
    }

    #[test]
    fn jitter_stays_silent_until_prebuffered() {
        let mut j = JitterBuffer::new(32).with_tuning(4, 10);
        assert!(!j.primed());
        for _ in 0..3 {
            j.push(tone(7));
            assert_eq!(j.next(), silence_pcm());
            assert!(!j.primed());
        }
        j.push(tone(7));
        let out = j.next();
        assert!(j.primed());
        assert_eq!(out, tone(7));
    }

    #[test]
    fn default_prebuffer_primes_at_three_frames() {
        let mut j = JitterBuffer::new(32);
        j.push(tone(7));
        j.push(tone(7));
        assert_eq!(j.next(), silence_pcm());
        assert!(!j.primed());
        j.push(tone(7));
        assert_eq!(j.next(), tone(7));
        assert!(j.primed());
        assert_eq!(j.stats.played, 1);
    }

    #[test]
    fn jitter_burst_arrival_loses_no_frames() {
        let mut j = JitterBuffer::new(32).with_tuning(2, 10);
        j.push(tone(1));
        j.push(tone(2));
        j.push(tone(3));
        assert_eq!(j.next(), tone(1));
        assert_eq!(j.next(), tone(2));
        assert_eq!(j.next(), tone(3));
        assert_eq!(j.stats.played, 3);
    }

    #[test]
    fn jitter_single_late_frame_conceals_instead_of_gap() {
        let mut j = JitterBuffer::new(32).with_tuning(2, 10);
        j.push(tone(100));
        j.push(tone(100));
        assert_eq!(j.next(), tone(100));
        assert_eq!(j.next(), tone(100));
        let concealed: Vec<i16> = tone(100).iter().map(|s| (*s as i32 / 2) as i16).collect();
        assert_eq!(j.next(), concealed);
        assert_eq!(j.stats.concealed, 1);
        assert!(j.primed());
        j.push(tone(200));
        assert_eq!(j.next(), tone(200));
    }

    #[test]
    fn jitter_long_stall_reprimes_quietly() {
        let mut j = JitterBuffer::new(32).with_tuning(2, 3);
        j.push(tone(50));
        j.push(tone(50));
        assert_eq!(j.next(), tone(50));
        assert_eq!(j.next(), tone(50));
        for _ in 0..3 {
            let _ = j.next();
        }
        assert!(j.primed());
        assert_eq!(j.next(), silence_pcm());
        assert!(!j.primed());
        assert_eq!(j.stats.reprimes, 1);
    }

    #[test]
    fn jitter_drops_oldest_only_when_full() {
        let mut j = JitterBuffer::new(3).with_tuning(1, 10);
        j.push(tone(1));
        j.push(tone(2));
        j.push(tone(3));
        j.push(tone(4));
        assert_eq!(j.depth(), 3);
        assert_eq!(j.next(), tone(2));
        assert_eq!(j.next(), tone(3));
        assert_eq!(j.next(), tone(4));
    }
}
