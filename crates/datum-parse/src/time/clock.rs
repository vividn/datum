//! Pluggable wall-clock for `DatumTime::now()`.
//!
//! Production code installs nothing, so [`DatumTime::now`] falls through
//! to [`SystemClock`] (chrono::Local::now). Tests can install a
//! [`MockClock`] on the current thread via [`install`] / [`uninstall`]
//! to make timestamps deterministic without rewriting call sites.
//!
//! The clock is a thread-local — `cargo test` runs threads in parallel,
//! so each test owns its own mock without leaking into siblings.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use chrono::Utc;

use crate::time::datum_time::DatumTime;

/// A source of "now" for [`DatumTime::now`].
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> DatumTime;
}

/// Reads from the host's real clock + local timezone.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DatumTime {
        let local = chrono::Local::now();
        let utc = local.with_timezone(&Utc);
        let offset_minutes = local.offset().local_minus_utc() / 60;
        let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
        DatumTime {
            utc,
            offset_minutes,
            timezone,
        }
    }
}

/// Test clock backed by an interior-mutable [`DatumTime`]. `now()` always
/// returns the currently-stored instant; [`set`] and [`advance`] mutate it.
pub struct MockClock {
    time: Mutex<DatumTime>,
}

impl MockClock {
    pub fn new(t: DatumTime) -> Self {
        Self {
            time: Mutex::new(t),
        }
    }

    pub fn set(&self, t: DatumTime) {
        *self.time.lock().expect("MockClock poisoned") = t;
    }

    pub fn advance(&self, delta: chrono::Duration) {
        let mut t = self.time.lock().expect("MockClock poisoned");
        t.utc += delta;
    }
}

impl Clock for MockClock {
    fn now(&self) -> DatumTime {
        self.time.lock().expect("MockClock poisoned").clone()
    }
}

thread_local! {
    static CURRENT_CLOCK: RefCell<Option<Arc<dyn Clock>>> = const { RefCell::new(None) };
}

/// Install `clock` on the current thread. Returns the previously-installed
/// clock (if any) so callers can implement nesting / restoration.
pub fn install(clock: Arc<dyn Clock>) -> Option<Arc<dyn Clock>> {
    CURRENT_CLOCK.with(|c| c.replace(Some(clock)))
}

/// Drop the current-thread clock so [`now`] falls back to [`SystemClock`].
pub fn uninstall() -> Option<Arc<dyn Clock>> {
    CURRENT_CLOCK.with(|c| c.replace(None))
}

/// Borrow the currently-installed clock, if any.
pub fn current() -> Option<Arc<dyn Clock>> {
    CURRENT_CLOCK.with(|c| c.borrow().clone())
}

/// Return "now" via the installed clock, or [`SystemClock`] when none is set.
/// This is the entry point used by [`DatumTime::now`].
pub fn now() -> DatumTime {
    match current() {
        Some(c) => c.now(),
        None => SystemClock.now(),
    }
}
