//! Tiny token-bucket rate limiter shared across the three GitHub tools.
//!
//! Spec target: 30 calls/min. Bucket holds `capacity` tokens, refills at
//! `refill_per_second` tokens per second. Each tool call calls
//! [`RateLimiter::try_take`] before reaching out to GitHub — on `false` the
//! tool returns a polite error instead of consuming PAT quota.

use std::sync::Mutex;
use std::time::{
    Duration,
    Instant,
};

#[derive(Debug)]
pub struct RateLimiter {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    capacity: f64,
    tokens: f64,
    refill_per_second: f64,
    last_refill: Instant,
}

impl RateLimiter {
    /// 30 tokens / minute, matching spec.
    pub fn default_per_spec() -> Self {
        Self::new(30, Duration::from_secs(60))
    }

    pub fn new(capacity: u32, refill_window: Duration) -> Self {
        let cap = capacity as f64;
        Self {
            inner: Mutex::new(Inner {
                capacity: cap,
                tokens: cap,
                refill_per_second: cap / refill_window.as_secs_f64(),
                last_refill: Instant::now(),
            }),
        }
    }

    /// Take one token. Returns true if a token was available.
    pub fn try_take(&self) -> bool {
        let mut inner = self.inner.lock().expect("rate limiter poisoned");
        let now = Instant::now();
        let elapsed = now.duration_since(inner.last_refill).as_secs_f64();
        inner.tokens = (inner.tokens + elapsed * inner.refill_per_second).min(inner.capacity);
        inner.last_refill = now;
        if inner.tokens >= 1.0 {
            inner.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Tokens currently in the bucket. Exposed for diagnostics + tests.
    pub fn available(&self) -> f64 {
        let inner = self.inner.lock().expect("rate limiter poisoned");
        inner.tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_bucket_drains_to_zero_and_rejects() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        assert!(rl.try_take());
        assert!(rl.try_take());
        assert!(rl.try_take());
        assert!(!rl.try_take(), "fourth take must fail until refill");
    }

    #[test]
    fn refill_lets_take_succeed_after_window() {
        let rl = RateLimiter::new(1, Duration::from_millis(100));
        assert!(rl.try_take());
        assert!(!rl.try_take(), "second immediate take must fail");
        std::thread::sleep(Duration::from_millis(150));
        assert!(rl.try_take(), "after refill window the bucket should have a token");
    }

    #[test]
    fn capacity_caps_refill() {
        let rl = RateLimiter::new(2, Duration::from_millis(50));
        // Sleep long enough that an unconstrained refill would push above
        // capacity.
        std::thread::sleep(Duration::from_millis(200));
        // We took zero, so two should still be available — never more.
        assert!(rl.try_take());
        assert!(rl.try_take());
        assert!(!rl.try_take());
    }
}
