use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

#[derive(Clone, Default)]
pub struct RateLimiter {
    speed_limit_bytes_per_sec: Option<u64>,
    state: Option<Arc<Mutex<RateLimiterState>>>,
}

struct RateLimiterState {
    last_tick: Instant,
    bytes_transferred_in_window: u64,
}

impl RateLimiter {
    pub fn new(speed_limit: Option<u64>) -> Self {
        let (limit, state) = match speed_limit {
            Some(l) if l > 0 => (
                Some(l),
                Some(Arc::new(Mutex::new(RateLimiterState {
                    last_tick: Instant::now(),
                    bytes_transferred_in_window: 0,
                }))),
            ),
            _ => (None, None),
        };

        Self {
            speed_limit_bytes_per_sec: limit,
            state,
        }
    }

    pub fn is_unlimited(&self) -> bool {
        self.speed_limit_bytes_per_sec.is_none()
    }

    pub async fn throttle(&self, bytes_transferred: usize, _elapsed: Duration) {
        let limit = match self.speed_limit_bytes_per_sec {
            Some(l) => l,
            None => return, // Zero overhead when unlimited
        };

        let state_arc = match &self.state {
            Some(s) => s,
            None => return,
        };

        let mut state = state_arc.lock().await;
        state.bytes_transferred_in_window += bytes_transferred as u64;

        let elapsed = state.last_tick.elapsed();
        let expected_duration =
            Duration::from_secs_f64(state.bytes_transferred_in_window as f64 / limit as f64);

        if expected_duration > elapsed {
            let delay = expected_duration - elapsed;
            if delay > Duration::from_millis(5) {
                sleep(delay).await;
            }
        }

        // Reset window every 1 second to avoid drifting
        if state.last_tick.elapsed() >= Duration::from_secs(1) {
            state.last_tick = Instant::now();
            state.bytes_transferred_in_window = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_rate_limiter_is_unlimited() {
        let limiter = RateLimiter::new(None);
        assert!(limiter.is_unlimited(), "Default limiter must be unlimited");

        let start = Instant::now();
        // Simulate transferring 100 MB across 100 chunks
        for _ in 0..100 {
            limiter
                .throttle(1024 * 1024, Duration::from_millis(1))
                .await;
        }
        let elapsed = start.elapsed();
        // Must complete almost instantaneously (under 20ms) with zero artificial delay
        assert!(
            elapsed < Duration::from_millis(50),
            "Unlimited rate limiter must not introduce sleep delay, took {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_active_rate_limiter_throttles() {
        let limiter = RateLimiter::new(Some(50_000)); // 50 KB/sec
        assert!(!limiter.is_unlimited());

        let start = Instant::now();
        // 50 KB transferred
        limiter.throttle(50_000, Duration::from_millis(10)).await;
        let elapsed = start.elapsed();
        // Should have throttled towards ~1 second
        assert!(
            elapsed >= Duration::from_millis(700),
            "Expected throttling around 1s, got {:?}",
            elapsed
        );
    }
}
