//! A bare clock time is today's, and one that lands over 12 hours ahead is
//! said on stderr in one line, the entry stored as typed (da-qo5in). Each run
//! is the real binary under a fixed `TZ` offset that makes its clock read
//! 00:07, so the hint is on stderr to read.

use std::process::Command;

use chrono::{DateTime, Duration, FixedOffset, NaiveTime, Timelike, Utc};

/// A POSIX `TZ` whose clock reads about 00:07 now, and its offset.
fn just_after_midnight() -> (String, FixedOffset) {
    let now = Utc::now();
    let minutes = i64::from(now.hour() * 60 + now.minute());
    let east = (7 - minutes).rem_euclid(1440);
    let east = if east > 720 { east - 1440 } else { east };
    // POSIX's sign is west of UTC.
    let sign = if east < 0 { '+' } else { '-' };
    let tz = format!("XXX{sign}{:02}:{:02}", east.abs() / 60, east.abs() % 60);
    (tz, FixedOffset::east_opt(east as i32 * 60).unwrap())
}

/// Logs `moment x` with `time` at about 00:07: its stderr, the instant stored,
/// and the offset the run was under, the one clock read both use.
fn log_at_0007(time: &[&str]) -> (String, DateTime<FixedOffset>, FixedOffset) {
    let dir = tempfile::tempdir().unwrap();
    let (tz, offset) = just_after_midnight();
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .args([
            "--db",
            dir.path().join("t.db").to_str().unwrap(),
            "moment",
            "x",
        ])
        .args(time)
        .env("HOME", dir.path())
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("TZ", &tz)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{time:?}: {stderr}");
    let at: String = rusqlite::Connection::open(dir.path().join("t.db"))
        .unwrap()
        .query_row("SELECT at FROM entries WHERE field = 'x'", [], |r| r.get(0))
        .unwrap();
    let at = DateTime::parse_from_rfc3339(&at)
        .unwrap()
        .with_timezone(&offset);
    (stderr, at, offset)
}

fn clock(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

#[test]
fn a_bare_clock_a_day_ahead_is_said_and_stored_for_today() {
    let (stderr, at, offset) = log_at_0007(&["-t", "23:50"]);
    // Its clock read about 00:07, so a minute later is still its today.
    let today = Utc::now().with_timezone(&offset).date_naive();
    assert_eq!(
        stderr.trim(),
        "note: 23:50 is today, 23 h from now; for last night, -y -t 23:50"
    );
    assert_eq!((at.date_naive(), at.time()), (today, clock(23, 50)));
}

#[test]
fn a_bare_clock_under_12_hours_ahead_says_nothing() {
    let (stderr, at, _) = log_at_0007(&["-t", "00:30"]);
    assert_eq!(stderr, "");
    assert!(at.with_timezone(&Utc) - Utc::now() < Duration::minutes(30));
}

#[test]
fn a_clock_with_a_day_or_a_direction_says_nothing() {
    for time in [
        &["-y", "-t", "23:50"][..],
        &["-d", "today", "-t", "23:50"],
        &["-t", "23:50+"],
    ] {
        assert_eq!(log_at_0007(time).0, "", "{time:?}");
    }
}
