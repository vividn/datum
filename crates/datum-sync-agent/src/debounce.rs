//! Quiet-window debounce: coalesce a burst of writes into a single push.
//!
//! The daemon sees one fs-notify event per WAL checkpoint, which in turn
//! fires on every CLI write. Without debouncing, typing `datum occur x`
//! ten times in a second would pack and push ten separate changesets.
//! The [`Debouncer`] collapses that burst — `note_write` advances a
//! quiet-window deadline; once the window elapses with no further writes,
//! [`Debouncer::is_due`] returns `true` and the caller pushes.
//!
//! A max-wait cap bounds the total deferral: a writer that never pauses
//! for `quiet_window` (a scripted loop calling `datum` faster than every
//! 1.5s — sustainable indefinitely thanks to the <5ms CLI startup) would
//! otherwise keep resetting the deadline and starve the push forever.
//! The debouncer instead fires at `min(last_write + quiet_window,
//! first_write_of_burst + max_wait)`, so a sustained stream still syncs
//! at least every `max_wait`.
//!
//! The type is deliberately pure (no `Instant::now()`, no timers) so
//! `scheduler` tests can drive it with a mock clock.

use std::time::{Duration, Instant};

/// Default cap on total deferral for a single burst — a sustained write
/// stream syncs at least this often. ~6-7 quiet windows: long enough
/// that a human-paced burst never hits it, short enough that worst-case
/// sync lag during a scripted burst stays in single-digit seconds.
pub const DEFAULT_MAX_WAIT: Duration = Duration::from_secs(10);

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
    /// Maximum total deferral since the first write of the current burst.
    max_wait: Duration,
    /// Earliest instant at which the next push is allowed. `None` means
    /// nothing is pending.
    deadline: Option<Instant>,
    /// First write of the current burst — anchors the max-wait cap.
    /// Invariant: `Some` iff `deadline` is `Some`.
    burst_start: Option<Instant>,
}

impl Debouncer {
    pub fn new(quiet_window: Duration) -> Self {
        Self::with_max_wait(quiet_window, DEFAULT_MAX_WAIT)
    }

    pub fn with_max_wait(quiet_window: Duration, max_wait: Duration) -> Self {
        Self {
            quiet_window,
            max_wait,
            deadline: None,
            burst_start: None,
        }
    }

    /// Record that a write happened. Advances the deadline to
    /// `now + quiet_window`, which means a fresh burst resets the timer
    /// (that's the whole point of debounce). The first write of a burst
    /// also anchors the max-wait cap; later writes leave it in place.
    pub fn note_write(&mut self, now: Instant) {
        self.deadline = Some(now + self.quiet_window);
        self.burst_start.get_or_insert(now);
    }

    /// The instant the pending push fires: the quiet-window deadline,
    /// capped at `burst_start + max_wait`. `None` when nothing is pending.
    fn fire_at(&self) -> Option<Instant> {
        let deadline = self.deadline?;
        let cap = self.burst_start? + self.max_wait;
        Some(deadline.min(cap))
    }

    /// True when a push is pending AND either the quiet window has
    /// elapsed or the burst has been running for `max_wait`.
    pub fn is_due(&self, now: Instant) -> bool {
        matches!(self.fire_at(), Some(f) if now >= f)
    }

    /// Remaining time until the debounce fires, or `None` if nothing is
    /// pending / the deadline has already passed.
    pub fn time_until_due(&self, now: Instant) -> Option<Duration> {
        self.fire_at().map(|f| f.saturating_duration_since(now))
    }

    /// Whether a write is pending at all (due or not).
    pub fn is_pending(&self) -> bool {
        self.deadline.is_some()
    }

    /// Clear the pending deadline — call after a successful push. Also
    /// ends the burst: the next write starts a fresh max-wait window.
    pub fn clear(&mut self) {
        self.deadline = None;
        self.burst_start = None;
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

    /// A writer that NEVER pauses for the quiet window must still fire
    /// once `max_wait` elapses since the burst started — the starvation
    /// case from da-kp5l (e.g. a shell loop calling `datum` sub-1.5s).
    #[test]
    fn max_wait_caps_never_pausing_write_stream() {
        let base = Instant::now();
        let mut d =
            Debouncer::with_max_wait(Duration::from_millis(1500), Duration::from_millis(10_000));

        // Writes every 100ms, indefinitely — each one would push the
        // quiet-window deadline past any fixed horizon.
        for i in 0..200 {
            let now = t(base, i * 100);
            d.note_write(now);
            // Before the cap: never due (the stream keeps resetting the
            // quiet window).
            if (i * 100) < 10_000 {
                assert!(!d.is_due(now), "due too early at {}ms", i * 100);
            }
        }
        // At burst_start + max_wait the cap fires despite ongoing writes.
        assert!(d.is_due(t(base, 10_000)));
        // time_until_due reflects the cap, not the receding deadline.
        assert_eq!(
            d.time_until_due(t(base, 9_900)),
            Some(Duration::from_millis(100))
        );
    }

    /// `clear` ends the burst: the next write anchors a fresh max-wait
    /// window instead of inheriting the old burst's start.
    #[test]
    fn clear_resets_max_wait_anchor() {
        let base = Instant::now();
        let mut d =
            Debouncer::with_max_wait(Duration::from_millis(1500), Duration::from_millis(10_000));
        d.note_write(base);
        d.clear();

        // New burst starting at T+9s: cap is now T+19s, not T+10s.
        d.note_write(t(base, 9_000));
        assert!(!d.is_due(t(base, 10_000)));
        // Quiet window (not the cap) fires this one.
        assert!(d.is_due(t(base, 9_000 + 1500)));
    }

    /// A quiet writer fires on the quiet window as before — the cap only
    /// matters when it is EARLIER than the deadline.
    #[test]
    fn quiet_window_still_wins_when_earlier_than_cap() {
        let base = Instant::now();
        let mut d =
            Debouncer::with_max_wait(Duration::from_millis(1500), Duration::from_millis(10_000));
        d.note_write(base);
        assert!(!d.is_due(t(base, 1499)));
        assert!(d.is_due(t(base, 1500)));
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
        assert_eq!(d.time_until_due(t(base, 5000)), Some(Duration::ZERO));
    }
}
