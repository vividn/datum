//! Exponential retry schedule for failed push / pull operations.
//!
//! Per the spec: 5s → 15s → 60s → 5min cap. Each failure steps the
//! schedule forward; any success resets it. The [`Backoff`] is a pure
//! state machine — no timers, no I/O — so the scheduler tests can
//! assert "after 3 failures, the next retry is at T+80s" without sleeping.

use std::time::{Duration, Instant};

/// Default backoff ladder from the bead:
/// `5s → 15s → 60s → 5min cap` (last step repeats forever).
pub const DEFAULT_STEPS: &[Duration] = &[
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(60),
    Duration::from_secs(300),
];

#[derive(Debug, Clone)]
pub struct Backoff {
    steps: Vec<Duration>,
    /// Index of the NEXT step to wait. `0` means the next failure waits
    /// `steps[0]`; saturates at `steps.len() - 1`.
    next_idx: usize,
    /// Instant before which retries are blocked, set when a failure is
    /// recorded. `None` means "not currently in backoff".
    until: Option<Instant>,
}

impl Backoff {
    pub fn new(steps: &[Duration]) -> Self {
        assert!(!steps.is_empty(), "backoff needs at least one step");
        Self {
            steps: steps.to_vec(),
            next_idx: 0,
            until: None,
        }
    }

    pub fn default_schedule() -> Self {
        Self::new(DEFAULT_STEPS)
    }

    /// Record a failure at `now`. Sets the next allowed retry to
    /// `now + steps[next_idx]` and advances the step counter (saturating).
    pub fn note_failure(&mut self, now: Instant) -> Duration {
        let delay = self.steps[self.next_idx];
        self.until = Some(now + delay);
        // Saturate at the last step so we stay at the 5min cap forever.
        if self.next_idx + 1 < self.steps.len() {
            self.next_idx += 1;
        }
        delay
    }

    /// Any success clears the backoff and rewinds the step counter.
    pub fn note_success(&mut self) {
        self.next_idx = 0;
        self.until = None;
    }

    /// True while we're still within the current cool-down window.
    pub fn is_blocked(&self, now: Instant) -> bool {
        matches!(self.until, Some(u) if now < u)
    }

    /// Remaining time until the next retry is allowed (or `None` when we
    /// aren't in backoff).
    pub fn time_until_ready(&self, now: Instant) -> Option<Duration> {
        self.until.and_then(|u| {
            if now < u {
                Some(u.saturating_duration_since(now))
            } else {
                None
            }
        })
    }

    /// Index into the step ladder that the NEXT failure will use —
    /// exposed for tests; callers shouldn't need it.
    #[cfg(test)]
    pub fn next_step_index(&self) -> usize {
        self.next_idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(base: Instant, secs: u64) -> Instant {
        base + Duration::from_secs(secs)
    }

    /// Each failure must step one rung up the ladder and saturate at the
    /// last rung — this is the canonical exponential-backoff acceptance
    /// test named in the bead.
    #[test]
    fn exponential_backoff_on_errors() {
        let base = Instant::now();
        let mut b = Backoff::default_schedule();

        // Clean state → not blocked.
        assert!(!b.is_blocked(base));

        // First failure waits 5s.
        let d = b.note_failure(base);
        assert_eq!(d, Duration::from_secs(5));
        assert!(b.is_blocked(t(base, 4)));
        assert!(!b.is_blocked(t(base, 5)));

        // Second failure waits 15s from its own `now`, not cumulative.
        let d = b.note_failure(t(base, 5));
        assert_eq!(d, Duration::from_secs(15));
        // Third: 60s.
        let d = b.note_failure(t(base, 20));
        assert_eq!(d, Duration::from_secs(60));
        // Fourth: 5min.
        let d = b.note_failure(t(base, 80));
        assert_eq!(d, Duration::from_secs(300));
        // Fifth and beyond: still 5min (saturated).
        let d = b.note_failure(t(base, 380));
        assert_eq!(d, Duration::from_secs(300));
        let d = b.note_failure(t(base, 680));
        assert_eq!(d, Duration::from_secs(300));
    }

    #[test]
    fn success_rewinds_the_ladder() {
        let base = Instant::now();
        let mut b = Backoff::default_schedule();

        b.note_failure(base); // step 0 → advances to 1
        b.note_failure(t(base, 5)); // step 1 → advances to 2
        assert_eq!(b.next_step_index(), 2);

        b.note_success();
        assert_eq!(b.next_step_index(), 0);
        assert!(!b.is_blocked(t(base, 6)));

        // Next failure is 5s again, not 60s.
        let d = b.note_failure(t(base, 6));
        assert_eq!(d, Duration::from_secs(5));
    }

    #[test]
    fn time_until_ready_reports_remaining() {
        let base = Instant::now();
        let mut b = Backoff::default_schedule();
        assert_eq!(b.time_until_ready(base), None);
        b.note_failure(base);
        assert_eq!(b.time_until_ready(t(base, 2)), Some(Duration::from_secs(3)));
        // Past the cool-down window: report None.
        assert_eq!(b.time_until_ready(t(base, 10)), None);
    }
}
