//! Token-bucket rate limiter for LLM provider calls.
//!
//! Callers `try_acquire()` a token before each request. If the bucket is dry
//! they receive the `Err(Duration)` wait hint and can choose to sleep or skip.
//!
//! All arithmetic uses checked or saturating operations — no overflow panics.

use std::time::{Duration, Instant};

// ── TokenBucket ───────────────────────────────────────────────────────────────

/// A single-producer token bucket.
///
/// Tokens refill continuously at `refill_per_sec` up to `capacity`. One token
/// is consumed per call to [`TokenBucket::try_acquire`].
pub struct TokenBucket {
    capacity: u32,
    /// Fractional token balance (bounded to `[0, capacity]`).
    tokens: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a bucket that holds at most `capacity` tokens and refills at
    /// `refill_per_sec` tokens per second. Starts full.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0 or `refill_per_sec` is not positive.
    pub fn new(capacity: u32, refill_per_sec: f64) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        assert!(refill_per_sec > 0.0, "refill_per_sec must be > 0");
        Self {
            capacity,
            tokens: f64::from(capacity),
            refill_per_sec,
            last_refill: Instant::now(),
        }
    }

    /// Attempt to consume one token.
    ///
    /// Returns `Ok(())` on success. Returns `Err(wait)` with the minimum time
    /// the caller should wait before the next token becomes available.
    pub fn try_acquire(&mut self) -> Result<(), Duration> {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            let deficit = 1.0 - self.tokens;
            // seconds to wait = deficit / refill_per_sec (no overflow: both finite f64 > 0)
            let secs = deficit / self.refill_per_sec;
            let wait = duration_from_secs_f64(secs);
            Err(wait)
        }
    }

    /// Advance the token balance by the elapsed time since the last refill.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_refill);
        self.last_refill = now;

        let added = elapsed.as_secs_f64() * self.refill_per_sec;
        // Clamp to capacity so tokens never exceed the bucket size.
        let cap = f64::from(self.capacity);
        self.tokens = (self.tokens + added).min(cap);
    }

    /// Current token balance (informational, rounded down).
    pub fn available(&self) -> u32 {
        self.tokens.floor() as u32
    }

    /// Capacity of this bucket.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

// ── MultiProviderLimiter ──────────────────────────────────────────────────────

/// One bucket per LLM provider name.
pub struct MultiProviderLimiter {
    buckets: Vec<(String, TokenBucket)>,
}

impl MultiProviderLimiter {
    pub fn new() -> Self {
        Self { buckets: Vec::new() }
    }

    /// Register a provider with the given rate limits.
    pub fn add(&mut self, name: impl Into<String>, capacity: u32, refill_per_sec: f64) {
        self.buckets.push((name.into(), TokenBucket::new(capacity, refill_per_sec)));
    }

    /// Try to acquire a token for `provider`. Returns `None` if the provider is
    /// unknown (treated as unconstrained). Returns `Some(Ok(()))` on success and
    /// `Some(Err(wait))` when the bucket is dry.
    pub fn try_acquire(&mut self, provider: &str) -> Option<Result<(), Duration>> {
        self.buckets
            .iter_mut()
            .find(|(n, _)| n == provider)
            .map(|(_, b)| b.try_acquire())
    }
}

impl Default for MultiProviderLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Safe duration helper ──────────────────────────────────────────────────────

/// Convert a finite `f64` seconds value to `Duration`, saturating on overflow.
fn duration_from_secs_f64(secs: f64) -> Duration {
    if secs <= 0.0 {
        return Duration::ZERO;
    }
    // Maximum representable Duration in seconds.
    const MAX_SECS: f64 = u64::MAX as f64;
    if secs >= MAX_SECS {
        return Duration::from_secs(u64::MAX);
    }
    let whole = secs.floor() as u64;
    let frac = secs - whole as f64;
    // Nanosecond conversion: saturate at 999_999_999 to stay within Duration bounds.
    let nanos = (frac * 1_000_000_000.0).round() as u32;
    let nanos = nanos.min(999_999_999);
    Duration::new(whole, nanos)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dry_bucket() -> TokenBucket {
        let mut b = TokenBucket::new(5, 1.0);
        // Drain all tokens.
        for _ in 0..5 {
            b.try_acquire().unwrap();
        }
        b
    }

    #[test]
    fn full_bucket_grants_capacity_requests() {
        let mut b = TokenBucket::new(3, 10.0);
        assert!(b.try_acquire().is_ok());
        assert!(b.try_acquire().is_ok());
        assert!(b.try_acquire().is_ok());
    }

    #[test]
    fn exhausted_bucket_returns_wait_hint() {
        let mut b = dry_bucket();
        let err = b.try_acquire().unwrap_err();
        assert!(err > Duration::ZERO, "wait hint must be positive");
    }

    #[test]
    fn available_tracks_consumption() {
        let mut b = TokenBucket::new(4, 1.0);
        assert_eq!(b.available(), 4);
        b.try_acquire().unwrap();
        assert_eq!(b.available(), 3);
    }

    #[test]
    fn capacity_is_reported_correctly() {
        let b = TokenBucket::new(7, 2.0);
        assert_eq!(b.capacity(), 7);
    }

    #[test]
    fn wait_hint_scales_with_refill_rate() {
        let mut slow = TokenBucket::new(1, 0.5); // 1 token / 2 s
        slow.try_acquire().unwrap();
        let wait_slow = slow.try_acquire().unwrap_err();

        let mut fast = TokenBucket::new(1, 10.0); // 1 token / 0.1 s
        fast.try_acquire().unwrap();
        let wait_fast = fast.try_acquire().unwrap_err();

        assert!(wait_slow > wait_fast, "slower refill → longer wait");
    }

    #[test]
    fn duration_from_secs_zero() {
        assert_eq!(duration_from_secs_f64(0.0), Duration::ZERO);
    }

    #[test]
    fn duration_from_secs_negative() {
        assert_eq!(duration_from_secs_f64(-1.0), Duration::ZERO);
    }

    #[test]
    fn duration_from_secs_fractional() {
        let d = duration_from_secs_f64(1.5);
        assert_eq!(d, Duration::from_millis(1500));
    }

    #[test]
    fn duration_from_secs_overflow_saturates() {
        let d = duration_from_secs_f64(f64::MAX);
        assert_eq!(d, Duration::from_secs(u64::MAX));
    }

    #[test]
    fn multi_provider_unknown_provider_is_none() {
        let mut lim = MultiProviderLimiter::new();
        lim.add("claude", 10, 2.0);
        assert!(lim.try_acquire("gemini").is_none());
    }

    #[test]
    fn multi_provider_known_provider_grants() {
        let mut lim = MultiProviderLimiter::new();
        lim.add("claude", 10, 2.0);
        assert_eq!(lim.try_acquire("claude"), Some(Ok(())));
    }

    #[test]
    fn multi_provider_dry_bucket_returns_some_err() {
        let mut lim = MultiProviderLimiter::new();
        lim.add("openrouter", 1, 1.0);
        lim.try_acquire("openrouter").unwrap().unwrap(); // drain
        let result = lim.try_acquire("openrouter").unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn multi_provider_default_is_empty() {
        let mut lim = MultiProviderLimiter::default();
        assert!(lim.try_acquire("any").is_none());
    }
}
