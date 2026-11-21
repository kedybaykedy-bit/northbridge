use std::time::Duration;

pub const MIN_QUALITY: u8 = 35;
pub const QUALITY_STEP_DOWN: u8 = 10;
pub const QUALITY_STEP_UP: u8 = 5;
pub const MIN_FPS: u8 = 6;
pub const RECOVER_STREAK: u32 = 40;

pub struct AdaptState {
    init_quality: u8,
    init_fps: u8,
    quality: u8,
    fps: u8,
    fast_streak: u32,
}

impl AdaptState {
    pub fn new(quality: u8, fps: u8) -> Self {
        let quality = quality.clamp(MIN_QUALITY, 100);
        let fps = fps.clamp(MIN_FPS, 30);
        Self {
            init_quality: quality,
            init_fps: fps,
            quality,
            fps,
            fast_streak: 0,
        }
    }

    pub fn quality(&self) -> u8 {
        self.quality
    }

    pub fn fps(&self) -> u8 {
        self.fps
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs_f32(1.0 / f32::from(self.fps.max(1)))
    }

    pub fn note_frame(&mut self, work: Duration, budget: Duration, congested: bool) {
        if congested || work > budget {
            self.fast_streak = 0;
            if self.quality > MIN_QUALITY {
                self.quality = self.quality.saturating_sub(QUALITY_STEP_DOWN).max(MIN_QUALITY);
            } else if self.fps > MIN_FPS {
                self.fps = self.fps.saturating_sub(2).max(MIN_FPS);
            }
            return;
        }
        if work.mul_f32(2.0) < budget {
            self.fast_streak += 1;
            if self.fast_streak >= RECOVER_STREAK {
                self.fast_streak = 0;
                if self.quality < self.init_quality {
                    self.quality = (self.quality + QUALITY_STEP_UP).min(self.init_quality);
                } else if self.fps < self.init_fps {
                    self.fps = (self.fps + 1).min(self.init_fps);
                }
            }
        } else {
            self.fast_streak = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast() -> Duration {
        Duration::from_millis(10)
    }

    fn slow() -> Duration {
        Duration::from_millis(200)
    }

    fn budget() -> Duration {
        Duration::from_millis(83)
    }

    #[test]
    fn new_clamps_inputs() {
        let low = AdaptState::new(0, 0);
        assert_eq!(low.quality(), MIN_QUALITY);
        assert_eq!(low.fps(), MIN_FPS);
        let high = AdaptState::new(255, 255);
        assert_eq!(high.quality(), 100);
        assert_eq!(high.fps(), 30);
    }

    #[test]
    fn congestion_steps_quality_down_first() {
        let mut adapt = AdaptState::new(70, 12);
        adapt.note_frame(fast(), budget(), true);
        assert_eq!(adapt.quality(), 60);
        assert_eq!(adapt.fps(), 12);
    }

    #[test]
    fn over_budget_work_steps_down() {
        let mut adapt = AdaptState::new(70, 12);
        adapt.note_frame(slow(), budget(), false);
        assert_eq!(adapt.quality(), 60);
    }

    #[test]
    fn floor_is_min_quality_then_min_fps() {
        let mut adapt = AdaptState::new(70, 12);
        for _ in 0..10 {
            adapt.note_frame(slow(), budget(), true);
        }
        assert_eq!(adapt.quality(), MIN_QUALITY);
        assert!(adapt.fps() < 12);
        for _ in 0..10 {
            adapt.note_frame(slow(), budget(), true);
        }
        assert_eq!(adapt.fps(), MIN_FPS);
        assert_eq!(adapt.quality(), MIN_QUALITY);
    }

    #[test]
    fn sustained_fast_frames_recover_to_init() {
        let mut adapt = AdaptState::new(70, 12);
        adapt.note_frame(slow(), budget(), true);
        adapt.note_frame(slow(), budget(), true);
        assert!(adapt.quality() < 70);
        for _ in 0..RECOVER_STREAK {
            adapt.note_frame(fast(), budget(), false);
        }
        assert!(adapt.quality() > MIN_QUALITY);
        for _ in 0..(RECOVER_STREAK * 10) {
            adapt.note_frame(fast(), budget(), false);
        }
        assert_eq!(adapt.quality(), 70);
        assert_eq!(adapt.fps(), 12);
    }

    #[test]
    fn borderline_work_neither_degrades_nor_recovers() {
        let mut adapt = AdaptState::new(70, 12);
        let mid = Duration::from_millis(60);
        for _ in 0..(RECOVER_STREAK * 2) {
            adapt.note_frame(mid, budget(), false);
        }
        assert_eq!(adapt.quality(), 70);
        assert_eq!(adapt.fps(), 12);
    }

    #[test]
    fn interval_matches_fps() {
        let adapt = AdaptState::new(70, 10);
        let interval = adapt.interval();
        assert!(interval.as_millis() >= 95 && interval.as_millis() <= 105);
    }
}
