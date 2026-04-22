//! Quiet-window debounce: coalesce a burst of writes into a single push.
//!
//! The daemon sees one fs-notify event per WAL checkpoint, which in turn
//! fires on every CLI write. Without debouncing, typing `datum occur x`
//! ten times in a second would pack and push ten separate changesets.
//! The [`Debouncer`] collapses that burst — `note_write` advances a
//! quiet-window deadline; once the window elapses with no further writes,
//! [`Debouncer::is_due`] returns `true` and the caller pushes.
//!
//! The type is deliberately pure (no `Instant::now()`, no timers) so
//! `scheduler` tests can drive it with a mock clock.

use std::time::{Duration, Instant};

/// Stateful quiet-window debounce.
///
/// Typical use:
/// ```ignore
/// let mut d = Debouncer::new(Duration::from_millis(1500));
/// d.note_write(now);              // fs-notify fired
/// // ... more writes in the window reset the deadline ...
/// if d.is_due(now) { push(); d.clear(); }
/// ```
#[derive(Debug, Clone)]
pub struct Debouncer {
    /// Minimum quiet interval after the last write before we fire.
    quiet_window: Duration,
    /// Earliest instant at which the next push is allowed. `None` means
    /// nothing is pending.
    deadline: Option<Instant>,
}

impl Debouncer {
    pub fn new(quiet_window: Duration) -> Self {
        Self {
            quiet_window,
            deadline: None,
        }
    }

    /// Record that a write happened. Advances the deadline to
    /// `now + quiet_window`, which means a fresh burst resets the timer
    /// (that's the whole point of debounce).
    pub fn note_write(&mut self, now: Instant) {
        self.deadline = Some(now + self.quiet_window);
    }

    /// True when a push is pending AND the quiet window has elapsed.
    pub fn is_due(&self, now: Instant) -> bool {
        matches!(self.deadline, Some(d) if now >= d)
    }

    /// Remaining time until the debounce fires, or `None` if nothing is
    /// pending / the deadline has already passed.
    pub fn time_until_due(&self, now: Instant) -> Option<Duration> {
        self.deadline.map(|d| d.saturating_duration_since(now))
    }

    /// Whether a write is pending at all (due or not).
    pub fn is_pending(&self) -> bool {
        self.deadline.is_some()
    }

    /// Clear the pending deadline — call after a successful push.
    pub fn clear(&mut self) {
        self.deadline = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    /// A rapid burst of writes must collapse into a single fire — the
    /// deadline keeps getting pushed back until the writer pauses.
    #[test]
    fn debounce_coalesces_rapid_writes() {
        let base = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(1500));

        // Ten writes spaced 100ms apart — total elapsed = 900ms.
        for i in 0..10 {
            d.note_write(t(base, i * 100));
        }
        // Still within a quiet window — NOT due 500ms after the last write.
        assert!(!d.is_due(t(base, 900 + 500)));
        // Due once the full quiet window elapses past the last write.
        assert!(d.is_due(t(base, 900 + 1500)));
    }

    #[test]
    fn debounce_fires_after_quiet_window() {
        let base = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(1500));

        d.note_write(base);
        assert!(!d.is_due(base));
        assert!(!d.is_due(t(base, 1499)));
        assert!(d.is_due(t(base, 1500)));
        assert!(d.is_due(t(base, 5000)));
    }

    #[test]
    fn clear_cancels_pending() {
        let base = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(100));
        d.note_write(base);
        assert!(d.is_pending());
        d.clear();
        assert!(!d.is_pending());
        assert!(!d.is_due(t(base, 10_000)));
    }

    #[test]
    fn time_until_due_reports_remaining() {
        let base = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(1500));
        assert_eq!(d.time_until_due(base), None);
        d.note_write(base);
        assert_eq!(
            d.time_until_due(t(base, 500)),
            Some(Duration::from_millis(1000))
        );
        // Past the deadline: saturate at zero.
        assert_eq!(
            d.time_until_due(t(base, 5000)),
            Some(Duration::ZERO)
        );
    }
}
