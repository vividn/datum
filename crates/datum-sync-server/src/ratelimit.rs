//! Per-IP token-bucket rate limiter for the unauthenticated auth surface
//! (da-7vc3).
//!
//! The passkey ceremonies, `/auth/refresh`, and device-code *start* are
//! reachable with no credential. Left open they let an attacker spam
//! `register/begin` + `login/begin` (growing the ceremony maps between
//! amortized sweeps), hammer refresh-token validation, or brute the
//! device-code flow. This caps each client IP to a generous but finite
//! rate.
//!
//! Design constraints (surfaced in the 2026-06-13 night-walk, on da-7vc3):
//!
//! - The device-code **poll** endpoint is high-frequency by design (RFC
//!   8628, ~1s interval) and must NOT be throttled by this limiter — it is
//!   mounted outside the limited group in [`crate::build_app`].
//! - Keying is by **peer IP only**. `X-Forwarded-For` is deliberately not
//!   trusted: behind no proxy it is attacker-controlled, so honoring it
//!   would let a client both evade its own limit and frame other IPs.
//!   Proxied deployments must terminate XFF at a trusted layer instead.
//! - A hand-rolled per-IP map is itself a memory-DoS vector if unbounded,
//!   so the map is swept on every check — a bucket idle long enough to have
//!   fully refilled is indistinguishable from absent and is dropped
//!   (mirrors the `sweep_and_insert` pattern, da-3huo).

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

/// One IP's bucket: a fractional token count and the instant it was last
/// touched (for lazy refill).
struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Token-bucket limiter keyed by client IP. Cheap to clone-share via the
/// enclosing `Arc<AppState>`; all mutation is behind an internal mutex.
pub struct RateLimiter {
    /// `None` => disabled (every check allows). Set when the configured
    /// per-minute budget is 0.
    inner: Option<Mutex<HashMap<IpAddr, Bucket>>>,
    /// Burst size — the most requests an idle client may fire back-to-back.
    capacity: f64,
    /// Sustained refill rate, tokens per second.
    refill_per_sec: f64,
    /// Idle span after which a bucket is fully refilled (`capacity /
    /// refill_per_sec`). A bucket untouched for this long is dropped by the
    /// sweep since it carries no state distinct from a fresh one.
    full_refill: Duration,
}

impl RateLimiter {
    /// Build a limiter admitting `per_min` requests per minute per IP, with
    /// a burst ceiling of a third of that budget (≈10 at the default 30).
    /// `per_min == 0` disables the limiter entirely (a no-op that always
    /// allows) — the documented off-switch for
    /// `DATUM_SYNC_AUTH_RATELIMIT_PER_MIN`.
    pub fn new(per_min: u32) -> Self {
        if per_min == 0 {
            return Self {
                inner: None,
                capacity: 0.0,
                refill_per_sec: 0.0,
                full_refill: Duration::ZERO,
            };
        }
        let refill_per_sec = per_min as f64 / 60.0;
        // Burst is a third of the sustained budget, floored at 1 so even a
        // tiny budget still admits a single request. (Note: capacity /
        // refill_per_sec == 20s for any per_min, so full_refill is constant.)
        let capacity = (per_min as f64 / 3.0).max(1.0);
        let full_refill = Duration::from_secs_f64(capacity / refill_per_sec);
        Self {
            inner: Some(Mutex::new(HashMap::new())),
            capacity,
            refill_per_sec,
            full_refill,
        }
    }

    /// Charge one token to `ip`. Returns `true` if the request is admitted,
    /// `false` if the bucket is empty (caller should reject with 429).
    pub fn check(&self, ip: IpAddr) -> bool {
        self.check_at(ip, Instant::now())
    }

    /// [`check`](Self::check) with an injectable clock so refill behavior is
    /// unit-testable without sleeping.
    fn check_at(&self, ip: IpAddr, now: Instant) -> bool {
        let Some(inner) = &self.inner else {
            return true; // disabled
        };
        let mut buckets = inner.lock().unwrap();

        // Drop fully-refilled (idle) buckets first: bounds the map to IPs
        // active within the refill window, so a flood of distinct source IPs
        // can't grow it without bound.
        buckets.retain(|_, b| now.saturating_duration_since(b.last) < self.full_refill);

        let bucket = buckets.entry(ip).or_insert(Bucket {
            tokens: self.capacity,
            last: now,
        });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        bucket.last = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(n: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, n))
    }

    #[test]
    fn disabled_limiter_always_allows() {
        let rl = RateLimiter::new(0);
        for _ in 0..1000 {
            assert!(rl.check(ip(1)));
        }
    }

    #[test]
    fn burst_then_block() {
        // 30/min => capacity 10. Ten back-to-back allowed, eleventh blocked.
        let rl = RateLimiter::new(30);
        let now = Instant::now();
        for i in 0..10 {
            assert!(
                rl.check_at(ip(1), now),
                "request {i} within burst should pass"
            );
        }
        assert!(!rl.check_at(ip(1), now), "eleventh request must be blocked");
    }

    #[test]
    fn per_ip_isolation() {
        // One IP exhausting its bucket must not affect another IP.
        let rl = RateLimiter::new(30);
        let now = Instant::now();
        for _ in 0..10 {
            assert!(rl.check_at(ip(1), now));
        }
        assert!(!rl.check_at(ip(1), now));
        assert!(rl.check_at(ip(2), now), "a different IP has its own budget");
    }

    #[test]
    fn refills_over_time() {
        // 30/min => 0.5 tokens/sec. Exhaust, then 2s later exactly one
        // token is available again.
        let rl = RateLimiter::new(30);
        let t0 = Instant::now();
        for _ in 0..10 {
            assert!(rl.check_at(ip(1), t0));
        }
        assert!(!rl.check_at(ip(1), t0));
        // 2s of refill at 0.5/s == 1 token.
        let t1 = t0 + Duration::from_secs(2);
        assert!(rl.check_at(ip(1), t1), "one token should have refilled");
        assert!(!rl.check_at(ip(1), t1), "but only one");
    }

    #[test]
    fn idle_bucket_is_swept() {
        // After a full refill window of idleness a bucket is dropped, so the
        // map does not retain every IP that ever connected.
        let rl = RateLimiter::new(30);
        let t0 = Instant::now();
        assert!(rl.check_at(ip(1), t0));
        assert_eq!(rl.inner.as_ref().unwrap().lock().unwrap().len(), 1);

        // Touch a different IP well past the 20s full-refill window; the
        // sweep in that call evicts ip(1).
        let t1 = t0 + Duration::from_secs(60);
        assert!(rl.check_at(ip(2), t1));
        let map = rl.inner.as_ref().unwrap().lock().unwrap();
        assert!(!map.contains_key(&ip(1)), "idle bucket must be swept");
        assert_eq!(map.len(), 1, "only the active IP remains");
    }
}
