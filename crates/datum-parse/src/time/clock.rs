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
use chrono_tz::Tz;

use crate::time::datum_time::DatumTime;

/// Resolve the host's effective IANA timezone name.
///
/// Honors `TZ` first (so per-invocation overrides like
/// `TZ=America/Los_Angeles dm occur sleep` produce a matching
/// `occur_time_tz`, paralleling chrono::Local's offset behavior), then
/// falls back to [`iana_time_zone::get_timezone`], then to `"UTC"` as a
/// last resort when the host exposes no IANA name.
///
/// `TZ` values that aren't valid IANA names (POSIX forms like `PST8PDT`,
/// `EST5EDT`) are ignored — we only emit names that round-trip through
/// `chrono_tz`, since the storage column documents an IANA zone.
pub fn host_iana_tz_name() -> String {
    if let Ok(raw) = std::env::var("TZ") {
        // POSIX permits a leading ':' to hint "use the file" — strip it
        // so `TZ=:America/Los_Angeles` resolves the same as the bare form.
        let candidate = raw.strip_prefix(':').unwrap_or(&raw);
        if !candidate.is_empty() && candidate.parse::<Tz>().is_ok() {
            return candidate.to_string();
        }
    }
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string())
}

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
        let timezone = host_iana_tz_name();
        DatumTime {
            utc,
            offset_minutes,
            timezone: Some(timezone),
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

/// Test-only `TZ` override guard, shared crate-wide.
///
/// `TZ` is process-wide state and `cargo test` runs tests as parallel
/// threads in one process, so anything that mutates `TZ` must serialize on
/// the process-wide `datum_test_util::env_lock`. Living at `pub(crate)`
/// lets the `builder` DST tests reuse the same lock as the `clock` tests
/// instead of racing them across a second, independent mutex.
#[cfg(test)]
pub(crate) mod test_tz {
    use datum_test_util::{env_lock, ScopedEnv};

    pub(crate) struct TzGuard {
        // Field order: env restores before the lock releases.
        _env: ScopedEnv,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TzGuard {
        pub(crate) fn set(value: Option<&str>) -> Self {
            let lock = env_lock();
            let env = match value {
                Some(v) => ScopedEnv::set("TZ", v),
                None => ScopedEnv::remove("TZ"),
            };
            TzGuard {
                _env: env,
                _lock: lock,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_tz::TzGuard;
    use super::*;

    #[test]
    fn tz_env_iana_name_overrides_host_zone() {
        let _g = TzGuard::set(Some("America/Los_Angeles"));
        assert_eq!(host_iana_tz_name(), "America/Los_Angeles");
    }

    #[test]
    fn tz_env_leading_colon_is_stripped() {
        // POSIX permits `TZ=:America/Los_Angeles`; we honor the same name.
        let _g = TzGuard::set(Some(":America/Los_Angeles"));
        assert_eq!(host_iana_tz_name(), "America/Los_Angeles");
    }

    #[test]
    fn tz_env_unknown_form_falls_through() {
        // POSIX `TZ` permits inline DST rules (`EST5EDT,M3.2.0/2,M11.1.0/2`)
        // that chrono_tz cannot interpret — those must not leak into the
        // storage column. The fallback may be the OS zone (when present) or
        // "UTC" (stripped sandbox); either is acceptable as long as it's
        // a valid IANA name, not the raw POSIX rule string.
        let _g = TzGuard::set(Some("EST5EDT,M3.2.0/2,M11.1.0/2"));
        let got = host_iana_tz_name();
        assert_ne!(got, "EST5EDT,M3.2.0/2,M11.1.0/2");
        assert!(
            got.parse::<Tz>().is_ok(),
            "fallback must be valid IANA: {got}"
        );
    }

    #[test]
    fn tz_env_iana_legacy_alias_is_accepted() {
        // `PST8PDT`, `US/Pacific`, etc. are deprecated but still recognized
        // IANA zones — chrono_tz parses them, so we honor them as written.
        let _g = TzGuard::set(Some("PST8PDT"));
        assert_eq!(host_iana_tz_name(), "PST8PDT");
    }

    #[test]
    fn tz_env_unset_uses_iana_or_utc() {
        let _g = TzGuard::set(None);
        let got = host_iana_tz_name();
        // Either resolves to a real IANA name or the documented "UTC" fallback.
        assert!(
            got.parse::<Tz>().is_ok(),
            "fallback must be valid IANA: {got}"
        );
    }

    #[test]
    fn tz_env_sync_with_systemclock() {
        // Regression for da-flbu: SystemClock.now() must report the same
        // timezone name as host_iana_tz_name() — they're meant to agree.
        let _g = TzGuard::set(Some("America/New_York"));
        assert_eq!(
            SystemClock.now().timezone.as_deref(),
            Some("America/New_York")
        );
    }
}
