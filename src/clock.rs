use std::time::Instant;

pub struct Clock {
    last_frame: Instant,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            last_frame: Instant::now(),
        }
    }
}

impl Clock {
    pub fn tick(&mut self) -> f32 {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        delta.as_secs_f32()
    }

    pub fn reset(&mut self) {
        self.last_frame = Instant::now();
    }
}
