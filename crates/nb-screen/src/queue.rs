use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct QueuedFrame {
    pub width: u16,
    pub height: u16,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct FrameQueue {
    cap: usize,
    frames: VecDeque<QueuedFrame>,
    evicted: u64,
}

impl FrameQueue {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            frames: VecDeque::new(),
            evicted: 0,
        }
    }

    pub fn push(&mut self, frame: QueuedFrame) -> bool {
        let mut dropped = false;
        while self.frames.len() >= self.cap {
            self.frames.pop_front();
            self.evicted += 1;
            dropped = true;
        }
        self.frames.push_back(frame);
        dropped
    }

    pub fn take_newest(&mut self) -> Option<QueuedFrame> {
        while self.frames.len() > 1 {
            self.frames.pop_front();
            self.evicted += 1;
        }
        self.frames.pop_front()
    }

    pub fn evicted(&self) -> u64 {
        self.evicted
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

    fn frame(tag: u8) -> QueuedFrame {
        QueuedFrame {
            width: 64,
            height: 48,
            data: vec![tag],
        }
    }

    #[test]
    fn push_drops_oldest_when_full() {
        let mut q = FrameQueue::new(2);
        assert!(!q.push(frame(1)));
        assert!(!q.push(frame(2)));
        assert_eq!(q.len(), 2);
        assert!(q.push(frame(3)));
        assert_eq!(q.len(), 2);
        assert_eq!(q.evicted(), 1);
        let newest = q.take_newest().expect("frame");
        assert_eq!(newest.data, vec![3]);
        assert!(q.is_empty());
    }

    #[test]
    fn take_newest_skips_stale_frames() {
        let mut q = FrameQueue::new(2);
        assert!(q.take_newest().is_none());
        q.push(frame(1));
        q.push(frame(2));
        let newest = q.take_newest().expect("frame");
        assert_eq!(newest.data, vec![2]);
        assert_eq!(q.evicted(), 1);
        assert!(q.take_newest().is_none());
    }

    #[test]
    fn evicted_counts_every_overflow() {
        let mut q = FrameQueue::new(1);
        q.push(frame(1));
        q.push(frame(2));
        q.push(frame(3));
        assert_eq!(q.evicted(), 2);
        assert_eq!(q.len(), 1);
    }
}
