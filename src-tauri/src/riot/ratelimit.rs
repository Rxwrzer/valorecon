/// Sliding-window rate limiter.
/// Mirrors the Python RateLimiter: ~45 req/60s ceiling (under Riot's ~49/60s).
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(60);
const CEILING: usize = 45;

#[derive(Debug)]
pub struct RateLimiter {
    timestamps: VecDeque<Instant>,
    ceiling: usize,
    window: Duration,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(CEILING, WINDOW)
    }
}

impl RateLimiter {
    pub fn new(ceiling: usize, window: Duration) -> Self {
        Self {
            timestamps: VecDeque::with_capacity(ceiling + 4),
            ceiling,
            window,
        }
    }

    fn prune(&mut self) {
        let cutoff = Instant::now() - self.window;
        while self.timestamps.front().map_or(false, |&t| t < cutoff) {
            self.timestamps.pop_front();
        }
    }

    pub fn available(&mut self) -> usize {
        self.prune();
        self.ceiling.saturating_sub(self.timestamps.len())
    }

    pub fn record(&mut self) {
        self.prune();
        self.timestamps.push_back(Instant::now());
    }

    /// Seconds until `need` slots are available.
    pub fn time_until(&mut self, need: usize) -> f64 {
        self.prune();
        let used = self.timestamps.len();
        if used + need <= self.ceiling {
            return 0.0;
        }
        // Find the oldest timestamp that, once evicted, frees enough slots
        let evict = (used + need).saturating_sub(self.ceiling);
        if let Some(&oldest) = self.timestamps.get(evict.saturating_sub(1)) {
            let age = Instant::now().duration_since(oldest);
            let wait = self.window.as_secs_f64() - age.as_secs_f64();
            return wait.max(0.0);
        }
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_starts_at_ceiling() {
        let mut rl = RateLimiter::new(45, WINDOW);
        assert_eq!(rl.available(), 45);
    }

    #[test]
    fn records_reduce_available() {
        let mut rl = RateLimiter::new(10, WINDOW);
        for _ in 0..5 { rl.record(); }
        assert_eq!(rl.available(), 5);
    }

    #[test]
    fn ceiling_at_max() {
        let mut rl = RateLimiter::new(5, WINDOW);
        for _ in 0..10 { rl.record(); }
        assert_eq!(rl.available(), 0);
    }
}
