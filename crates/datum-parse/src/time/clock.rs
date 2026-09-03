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
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{FixedOffset, Offset, Utc};
use chrono_tz::Tz;

use crate::time::datum_time::DatumTime;

/// Resolve the host's effective IANA timezone name, or `None` when the host
/// cannot report one.
///
/// Honors `TZ` first (so per-invocation overrides like
/// `TZ=America/Los_Angeles dm occur sleep` produce a matching
/// `occur_time_tz`, paralleling chrono::Local's offset behavior), then
/// [`iana_time_zone::get_timezone`], then `/etc/localtime` resolved through
/// its whole symlink chain.
///
/// `TZ` values that aren't valid IANA names (POSIX forms like `EST5EDT,…`)
/// are ignored — we only emit names that round-trip through `chrono_tz`,
/// since the storage column documents an IANA zone.
///
/// `None` means *we could not find out*, and it is deliberately not spelled
/// `"UTC"`: [`DatumTime::timezone`] documents that naming a zone we were not
/// given invents history. Callers pair `None` with [`host_offset`], which is
/// read from the tzfile's contents and is correct either way (da-2pgd).
pub fn host_iana_tz_name() -> Option<String> {
    if let Ok(raw) = std::env::var("TZ") {
        // POSIX permits a leading ':' to hint "use the file" — strip it
        // so `TZ=:America/Los_Angeles` resolves the same as the bare form.
        let candidate = raw.strip_prefix(':').unwrap_or(&raw);
        if !candidate.is_empty() && candidate.parse::<Tz>().is_ok() {
            return Some(candidate.to_string());
        }
    }
    if let Ok(name) = iana_time_zone::get_timezone() {
        if name.parse::<Tz>().is_ok() {
            return Some(name);
        }
    }
    let name = tz_name_from_localtime();
    if name.is_none() {
        warn_zone_unknown_once();
    }
    name
}

/// The host's current UTC offset, read by chrono from the *contents* of the
/// tzfile rather than from a zone name.
///
/// This is the layer `chrono::Local` tries first and the one datum lost when
/// `TimeBuilder` started resolving both of its arms through a named zone: an
/// unnameable host then had no offset either, and every timestamp landed at
/// UTC. The kernel follows `/etc/localtime` on `open(2)` wherever it points,
/// so the contents are readable even on hosts whose *path* yields no name.
pub fn host_offset() -> FixedOffset {
    chrono::Local::now().offset().fix()
}

/// Zoneinfo roots an IANA name can be read off a resolved path.
const ZONEINFO_ROOTS: [&str; 3] = [
    "/usr/share/zoneinfo/",
    "/etc/zoneinfo/",
    // macOS keeps the database here and symlinks /etc/localtime into it.
    "/var/db/timezone/zoneinfo/",
];

/// Read the zone name off `/etc/localtime`, following the *whole* symlink
/// chain.
///
/// `iana-time-zone` deliberately matches only the link's immediate target
/// against these roots, so a legitimate setup that hops through an
/// intermediate — `/etc/localtime -> ~/.localtime -> /usr/share/zoneinfo/X`,
/// as cross-device timezone sync produces — yields no name at all. We choose
/// differently: canonicalizing costs one `realpath` on a path we already
/// trust, and every other program on such a host reports the zone correctly
/// because it never needs the name (da-2pgd).
fn tz_name_from_localtime() -> Option<String> {
    tz_name_from_zoneinfo_path(&std::fs::canonicalize("/etc/localtime").ok()?)
}

/// The IANA name a fully-resolved zoneinfo path denotes, if it denotes one.
fn tz_name_from_zoneinfo_path(path: &Path) -> Option<String> {
    let path = path.to_str()?;
    let name = ZONEINFO_ROOTS
        .iter()
        .find_map(|root| path.strip_prefix(root))?;
    // `posix/` and `right/` are alternate encodings of the same zones.
    let name = name
        .strip_prefix("posix/")
        .or_else(|| name.strip_prefix("right/"))
        .unwrap_or(name);
    name.parse::<Tz>().ok().map(|_| name.to_string())
}

/// Say once, on stderr, that the zone name is unknown.
///
/// Three lookups can fail here and the old code reported one word ("UTC"),
/// which cost a user an afternoon and a maintainer an hour. Once per process,
/// never on the happy path.
fn warn_zone_unknown_once() {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!(
            "datum: warning: could not determine this host's IANA timezone name \
             (TZ unset or not an IANA name, iana-time-zone found none, and \
             /etc/localtime does not resolve into a zoneinfo directory). \
             Recording the host's UTC offset with no zone name."
        );
    }
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
        DatumTime {
            utc,
            offset_minutes,
            // The offset above is authoritative; the name is a label we only
            // apply when the host actually gave us one.
            timezone: host_iana_tz_name(),
            uncertainty: None,
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
        assert_eq!(host_iana_tz_name().as_deref(), Some("America/Los_Angeles"));
    }

    #[test]
    fn tz_env_leading_colon_is_stripped() {
        // POSIX permits `TZ=:America/Los_Angeles`; we honor the same name.
        let _g = TzGuard::set(Some(":America/Los_Angeles"));
        assert_eq!(host_iana_tz_name().as_deref(), Some("America/Los_Angeles"));
    }

    #[test]
    fn tz_env_unknown_form_falls_through() {
        // POSIX `TZ` permits inline DST rules (`EST5EDT,M3.2.0/2,M11.1.0/2`)
        // that chrono_tz cannot interpret — those must not leak into the
        // storage column. The fallback is the OS zone where the host can name
        // one and `None` where it cannot; either way, never the POSIX rule.
        let _g = TzGuard::set(Some("EST5EDT,M3.2.0/2,M11.1.0/2"));
        let got = host_iana_tz_name();
        assert_ne!(got.as_deref(), Some("EST5EDT,M3.2.0/2,M11.1.0/2"));
        if let Some(name) = &got {
            assert!(
                name.parse::<Tz>().is_ok(),
                "fallback must be valid IANA: {name}"
            );
        }
    }

    #[test]
    fn tz_env_iana_legacy_alias_is_accepted() {
        // `PST8PDT`, `US/Pacific`, etc. are deprecated but still recognized
        // IANA zones — chrono_tz parses them, so we honor them as written.
        let _g = TzGuard::set(Some("PST8PDT"));
        assert_eq!(host_iana_tz_name().as_deref(), Some("PST8PDT"));
    }

    #[test]
    fn tz_env_unset_yields_a_real_name_or_nothing() {
        let _g = TzGuard::set(None);
        // Never a fabricated "UTC": either the host names its zone or we say
        // we do not know (da-2pgd).
        if let Some(name) = host_iana_tz_name() {
            assert!(
                name.parse::<Tz>().is_ok(),
                "detected name must be valid IANA: {name}"
            );
        }
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

    #[test]
    fn systemclock_offset_stands_without_a_name() {
        // The offset comes from the tzfile, so it is right even when the name
        // is missing — the pairing da-2pgd exists to restore.
        let _g = TzGuard::set(Some("America/New_York"));
        let now = SystemClock.now();
        assert_eq!(now.offset_minutes, host_offset().local_minus_utc() / 60);
    }

    /// The fixture that would have caught da-2pgd: a `/etc/localtime` that
    /// reaches zoneinfo through an intermediate hop, as cross-device timezone
    /// sync produces. Any fixture linking straight at `/usr/share/zoneinfo`
    /// passes both the old code and the new and proves nothing.
    #[test]
    fn zone_name_survives_a_symlink_hop_through_a_user_directory() {
        let zoneinfo = Path::new("/usr/share/zoneinfo/Europe/Berlin");
        if !zoneinfo.exists() {
            return; // no tzdata on this host; nothing to resolve
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let hop = dir.path().join(".localtime");
        let localtime = dir.path().join("localtime");
        std::os::unix::fs::symlink(zoneinfo, &hop).expect("hop symlink");
        std::os::unix::fs::symlink(&hop, &localtime).expect("localtime symlink");

        // The immediate target is the hop, not a zoneinfo path — which is
        // exactly what `iana-time-zone` refuses to follow.
        assert_eq!(
            tz_name_from_zoneinfo_path(&std::fs::read_link(&localtime).unwrap()),
            None
        );
        assert_eq!(
            tz_name_from_zoneinfo_path(&std::fs::canonicalize(&localtime).unwrap()).as_deref(),
            Some("Europe/Berlin"),
        );
    }

    #[test]
    fn a_path_outside_zoneinfo_names_no_zone() {
        assert_eq!(
            tz_name_from_zoneinfo_path(Path::new("/home/x/.localtime")),
            None
        );
        // Inside zoneinfo but not a zone: still no name.
        assert_eq!(
            tz_name_from_zoneinfo_path(Path::new("/usr/share/zoneinfo/Not/AZone")),
            None
        );
    }

    #[test]
    fn zoneinfo_posix_and_right_subtrees_name_the_plain_zone() {
        assert_eq!(
            tz_name_from_zoneinfo_path(Path::new("/usr/share/zoneinfo/posix/Europe/Berlin"))
                .as_deref(),
            Some("Europe/Berlin")
        );
        assert_eq!(
            tz_name_from_zoneinfo_path(Path::new("/usr/share/zoneinfo/right/Europe/Berlin"))
                .as_deref(),
            Some("Europe/Berlin")
        );
    }
}
