use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SpeedTracker {
    window: VecDeque<(Instant, u64)>,
    window_duration: Duration,
    ema_speed: f64,
}

impl Default for SpeedTracker {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl SpeedTracker {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            window: VecDeque::new(),
            window_duration,
            ema_speed: 0.0,
        }
    }

    pub fn update(&mut self, total_bytes: u64, now: Instant) -> u64 {
        self.window.push_back((now, total_bytes));
        while let Some(&(time, _)) = self.window.front() {
            if now.saturating_duration_since(time) > self.window_duration {
                self.window.pop_front();
            } else {
                break;
            }
        }

        if self.window.len() < 2 {
            return self.ema_speed.round() as u64;
        }

        let (first_time, first_bytes) = *self.window.front().unwrap();
        let (last_time, last_bytes) = *self.window.back().unwrap();

        let elapsed = last_time
            .saturating_duration_since(first_time)
            .as_secs_f64();
        if elapsed > 0.05 {
            let instant_speed = (last_bytes.saturating_sub(first_bytes) as f64) / elapsed;
            if self.ema_speed == 0.0 {
                self.ema_speed = instant_speed;
            } else {
                self.ema_speed = 0.35 * instant_speed + 0.65 * self.ema_speed;
            }
        }

        self.ema_speed.round() as u64
    }

    pub fn current_speed(&self) -> u64 {
        self.ema_speed.round() as u64
    }

    pub fn calculate_eta(&self, downloaded: u64, total: u64) -> u64 {
        let speed = self.current_speed();
        if speed > 0 && total > downloaded {
            (total - downloaded) / speed
        } else {
            0
        }
    }
}
