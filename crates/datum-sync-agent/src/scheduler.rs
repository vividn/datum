//! Per-DB scheduling policy — decides at each "tick" whether the worker
//! should push, pull, or sleep.
//!
//! The scheduler is a pure state machine. It does NOT execute HTTP,
//! touch the DB, or call `Instant::now()` itself — the caller owns the
//! clock and feeds a `now` into [`Scheduler::poll`]. This makes the
//! scheduler fully testable without any async runtime.
//!
//! Priority order when multiple actions are ready:
//!
//! 1. **Push** — a debounced write has been waiting; data-loss risk is
//!    highest if we drop it, so push wins over pull when both are ready
//!    in the same tick.
//! 2. **Pull** — the 30s interval timer (default) fired.
//! 3. **Sleep** — idle; caller should wait at least this long before
//!    polling again.
//!
//! Each direction has its own independent [`Backoff`]: a relay that's
//! rejecting pushes (auth expired, say) shouldn't also starve pulls,
//! because a successful pull can still keep the device in sync with
//! peers.

use std::time::{Duration, Instant};

use crate::backoff::Backoff;
use crate::debounce::Debouncer;

/// What the agent worker should do next, as decided by [`Scheduler::poll`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Push is ready — debounce elapsed, not in push-backoff.
    Push,
    /// Pull is ready — interval elapsed, not in pull-backoff.
    Pull,
    /// Nothing to do right now. Sleep at most this long before re-polling
    /// — a new write coming in on the channel will typically wake the
    /// worker sooner. Bounded so a misbehaving timer doesn't cause an
    /// infinite sleep.
    Sleep(Duration),
}

/// Default push debounce window — matches the bead ("Debounce writes:
/// 1.5s window").
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(1500);
/// Default pull interval — matches the bead ("Every 30s → pull").
pub const DEFAULT_PULL_INTERVAL: Duration = Duration::from_secs(30);
/// Upper bound on idle sleep between polls — so the scheduler still wakes
/// up periodically even if nothing's ready, in case we missed a signal.
pub const MAX_IDLE_SLEEP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct Scheduler {
    pull_interval: Duration,
    /// Instant of the last successful pull, or the scheduler start-time
    /// if no pull has completed yet (so the first pull fires one interval
    /// after startup, not immediately).
    last_pull: Instant,
    debouncer: Debouncer,
    push_backoff: Backoff,
    pull_backoff: Backoff,
}

impl Scheduler {
    pub fn new(start: Instant) -> Self {
        Self::with_params(start, DEFAULT_DEBOUNCE, DEFAULT_PULL_INTERVAL)
    }

    pub fn with_params(
        start: Instant,
        debounce_window: Duration,
        pull_interval: Duration,
    ) -> Self {
        Self {
            pull_interval,
            last_pull: start,
            debouncer: Debouncer::new(debounce_window),
            push_backoff: Backoff::default_schedule(),
            pull_backoff: Backoff::default_schedule(),
        }
    }

    /// The fs-watcher saw a write; arm the debounce window.
    pub fn note_write(&mut self, now: Instant) {
        self.debouncer.note_write(now);
    }

    pub fn note_push_success(&mut self) {
        self.debouncer.clear();
        self.push_backoff.note_success();
    }

    pub fn note_push_failure(&mut self, now: Instant) {
        // Keep the debounce armed so we'll retry after backoff.
        self.push_backoff.note_failure(now);
    }

    pub fn note_pull_success(&mut self, now: Instant) {
        self.last_pull = now;
        self.pull_backoff.note_success();
    }

    pub fn note_pull_failure(&mut self, now: Instant) {
        self.pull_backoff.note_failure(now);
    }

    /// Inspect the current state and decide what to do. Mutation-free —
    /// safe to call repeatedly. After executing the returned action,
    /// call the appropriate `note_*_success` / `note_*_failure`.
    pub fn poll(&self, now: Instant) -> Action {
        let push_ready =
            self.debouncer.is_due(now) && !self.push_backoff.is_blocked(now);
        let pull_ready = now.saturating_duration_since(self.last_pull)
            >= self.pull_interval
            && !self.pull_backoff.is_blocked(now);

        if push_ready {
            return Action::Push;
        }
        if pull_ready {
            return Action::Pull;
        }
        Action::Sleep(self.sleep_for(now))
    }

    /// Minimum of (pull-timer eta, push-debounce eta, push-backoff eta,
    /// pull-backoff eta, MAX_IDLE_SLEEP). Used so the worker's select!
    /// times out at exactly the right moment rather than spinning.
    fn sleep_for(&self, now: Instant) -> Duration {
        let mut deadlines: Vec<Duration> = Vec::new();

        // Pull timer: how long until `last_pull + pull_interval`.
        let next_pull_at = self.last_pull + self.pull_interval;
        deadlines.push(next_pull_at.saturating_duration_since(now));

        // Push debounce: how long until the pending write fires.
        if let Some(until) = self.debouncer.time_until_due(now) {
            deadlines.push(until);
        }

        // Backoff blockers: wait until they clear, otherwise we'd spin.
        if let Some(until) = self.push_backoff.time_until_ready(now) {
            deadlines.push(until);
        }
        if let Some(until) = self.pull_backoff.time_until_ready(now) {
            deadlines.push(until);
        }

        deadlines.push(MAX_IDLE_SLEEP);
        let min = deadlines.into_iter().min().unwrap_or(MAX_IDLE_SLEEP);
        // Clamp below 1ms to avoid hot spin, still fast enough for tests.
        min.max(Duration::from_millis(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(base: Instant, secs: u64) -> Instant {
        base + Duration::from_secs(secs)
    }
    fn tms(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    /// A write + the debounce window → poll returns Push. This is the
    /// canonical `scheduler_pushes_on_write` case from the bead.
    #[test]
    fn scheduler_pushes_on_write() {
        let base = Instant::now();
        let mut s = Scheduler::with_params(
            base,
            Duration::from_millis(1500),
            Duration::from_secs(30),
        );

        // Idle: no pending write → sleep.
        assert!(matches!(s.poll(base), Action::Sleep(_)));

        s.note_write(base);
        // During quiet window: not ready.
        assert!(matches!(s.poll(tms(base, 500)), Action::Sleep(_)));
        // After the window: push ready.
        assert_eq!(s.poll(tms(base, 1500)), Action::Push);
        assert_eq!(s.poll(tms(base, 5000)), Action::Push);

        // After recording success, debounce clears → no longer Push.
        s.note_push_success();
        assert!(matches!(s.poll(tms(base, 5000)), Action::Sleep(_)));
    }

    /// Every `pull_interval` seconds the scheduler must return Pull —
    /// matches `scheduler_pulls_on_interval` in the bead.
    #[test]
    fn scheduler_pulls_on_interval() {
        let base = Instant::now();
        let mut s = Scheduler::with_params(
            base,
            Duration::from_millis(1500),
            Duration::from_secs(30),
        );

        // Not yet 30s elapsed.
        assert!(matches!(s.poll(t(base, 29)), Action::Sleep(_)));

        // 30s elapsed → pull.
        assert_eq!(s.poll(t(base, 30)), Action::Pull);
        assert_eq!(s.poll(t(base, 60)), Action::Pull);

        // Recording a pull at T+30 → not due again until T+60.
        s.note_pull_success(t(base, 30));
        assert!(matches!(s.poll(t(base, 31)), Action::Sleep(_)));
        assert!(matches!(s.poll(t(base, 59)), Action::Sleep(_)));
        assert_eq!(s.poll(t(base, 60)), Action::Pull);
    }

    /// Push wins over pull when both are ready — data-loss risk is higher.
    #[test]
    fn push_wins_over_pull_in_same_tick() {
        let base = Instant::now();
        let mut s = Scheduler::with_params(
            base,
            Duration::from_millis(1500),
            Duration::from_secs(30),
        );
        s.note_write(base);
        // Advance enough that BOTH pull (30s) and push (1.5s) are ready.
        assert_eq!(s.poll(t(base, 30)), Action::Push);
    }

    /// A push failure must suppress subsequent Push actions until the
    /// backoff window elapses, even if new writes keep coming in.
    #[test]
    fn push_failure_suppresses_pushes_until_backoff_clears() {
        let base = Instant::now();
        let mut s = Scheduler::with_params(
            base,
            Duration::from_millis(1500),
            Duration::from_secs(30),
        );
        s.note_write(base);
        assert_eq!(s.poll(tms(base, 1500)), Action::Push);

        // Push fails → 5s backoff.
        s.note_push_failure(tms(base, 1500));
        // Fresh write comes in — debounce re-arms to 1500+1500 = 3000.
        s.note_write(tms(base, 1500));

        // At T=3s (1500 + 1500ms) debounce would be ready again, but the
        // push backoff until 1500+5000 = 6500ms still blocks.
        assert!(matches!(s.poll(tms(base, 3000)), Action::Sleep(_)));
        // Still blocked at 6000ms.
        assert!(matches!(s.poll(tms(base, 6000)), Action::Sleep(_)));
        // At 6500ms backoff clears → push ready.
        assert_eq!(s.poll(tms(base, 6500)), Action::Push);
    }

    /// Pull backoff is independent of push — auth issues on push should
    /// not starve pull traffic.
    #[test]
    fn pull_and_push_backoffs_are_independent() {
        let base = Instant::now();
        let mut s = Scheduler::with_params(
            base,
            Duration::from_millis(1500),
            Duration::from_secs(30),
        );
        s.note_push_failure(base);
        // Push is backed off, but no pull failures → pull should still
        // fire at its normal interval.
        assert_eq!(s.poll(t(base, 30)), Action::Pull);
    }

    /// The idle sleep must be bounded by MAX_IDLE_SLEEP.
    #[test]
    fn idle_sleep_is_bounded() {
        let base = Instant::now();
        let s = Scheduler::with_params(
            base,
            Duration::from_millis(1500),
            // Massive interval: without the bound, sleep would pick this.
            Duration::from_secs(24 * 60 * 60),
        );
        let Action::Sleep(d) = s.poll(base) else {
            panic!("expected Sleep");
        };
        assert!(d <= MAX_IDLE_SLEEP);
    }
}
