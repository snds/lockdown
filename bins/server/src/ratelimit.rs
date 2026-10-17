//! A token bucket per key, in memory: the brake on the login endpoints
//! (04-security §3: the OPAQUE caveat is "covered by rate limits and the
//! Secret Key"). One bucket per client address and one per login name,
//! so neither one noisy address nor one targeted account gets unlimited
//! guesses. It forgets everything on restart, which is fine for a brake.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Buckets of `capacity` tokens that refill at `per_second`.
#[derive(Debug)]
pub struct RateLimiter {
    capacity: f64,
    per_second: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

/// The most keys kept before idle ones are dropped.
const MAX_KEYS: usize = 10_000;

impl RateLimiter {
    /// A limiter allowing `capacity` calls at once and `per_second` after.
    pub fn new(capacity: u32, per_second: f64) -> Self {
        Self {
            capacity: f64::from(capacity),
            per_second,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one token for `key` at `now`. `Err` carries how long until
    /// the next token.
    pub fn check(&self, key: &str, now: Instant) -> Result<(), Duration> {
        let mut buckets = self
            .buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if buckets.len() >= MAX_KEYS && !buckets.contains_key(key) {
            // Drop whatever has refilled completely; it carries no state.
            let capacity = self.capacity;
            let per_second = self.per_second;
            buckets.retain(|_, b| {
                b.tokens + now.duration_since(b.last).as_secs_f64() * per_second < capacity
            });
        }
        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: self.capacity,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.per_second).min(self.capacity);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let wait = (1.0 - bucket.tokens) / self.per_second;
            Err(Duration::from_secs_f64(wait.max(0.001)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bucket_empties_and_refills() {
        let limiter = RateLimiter::new(3, 1.0);
        let t0 = Instant::now();
        assert!(limiter.check("a", t0).is_ok());
        assert!(limiter.check("a", t0).is_ok());
        assert!(limiter.check("a", t0).is_ok());
        let wait = limiter.check("a", t0).unwrap_err();
        assert!(wait <= Duration::from_secs(1));
        // Another key is untouched.
        assert!(limiter.check("b", t0).is_ok());
        // After a second, one more.
        assert!(limiter.check("a", t0 + Duration::from_secs(1)).is_ok());
        assert!(limiter.check("a", t0 + Duration::from_secs(1)).is_err());
        // Never over capacity.
        assert!(limiter.check("a", t0 + Duration::from_secs(100)).is_ok());
        assert!(limiter.check("a", t0 + Duration::from_secs(100)).is_ok());
        assert!(limiter.check("a", t0 + Duration::from_secs(100)).is_ok());
        assert!(limiter.check("a", t0 + Duration::from_secs(100)).is_err());
    }
}
