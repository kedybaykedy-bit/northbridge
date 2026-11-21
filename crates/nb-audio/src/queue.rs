use std::collections::VecDeque;

pub const AUDIO_QUEUE_CAP: usize = 3;

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

    pub fn take_newest(&mut self) -> Option<QueuedAudio> {
        while self.frames.len() > 1 {
            self.frames.pop_front();
        }
        self.frames.pop_front()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::CODEC_PCM;

    fn queued(tag: u8) -> QueuedAudio {
        QueuedAudio {
            codec: CODEC_PCM,
            flags: 0,
            channels: 2,
            payload: vec![tag],
        }
    }

    #[test]
    fn push_drops_oldest_when_full() {
        let mut q = AudioQueue::new(AUDIO_QUEUE_CAP);
        q.push(queued(1));
        q.push(queued(2));
        q.push(queued(3));
        assert_eq!(q.len(), 3);
        q.push(queued(4));
        assert_eq!(q.len(), 3);
        let newest = q.take_newest().expect("frame");
        assert_eq!(newest.payload, vec![4]);
        assert!(q.is_empty());
    }

    #[test]
    fn take_newest_skips_stale_frames() {
        let mut q = AudioQueue::new(3);
        assert!(q.take_newest().is_none());
        q.push(queued(1));
        q.push(queued(2));
        let newest = q.take_newest().expect("frame");
        assert_eq!(newest.payload, vec![2]);
        assert!(q.take_newest().is_none());
    }
}
