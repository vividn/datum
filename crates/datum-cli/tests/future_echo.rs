//! An entry logged after now is echoed FUTURE, whatever its shape, and one
//! this evening logged before noon is hinted, on stderr, toward last night;
//! the entry is stored as typed (da-85b0d). Each run is the real binary
//! under a fixed `TZ` offset that makes its clock read the time a test asks.

use std::path::Path;
use std::process::{Command, Output};

use chrono::{DateTime, Duration, FixedOffset, NaiveTime, Timelike, Utc};

const HINT: &str = "note: `retime -y` to move to yesterday";

/// A POSIX `TZ` whose clock reads about `h:m` now, and its offset.
fn clock_reading(h: i64, m: i64) -> (String, FixedOffset) {
    let now = Utc::now();
    let minutes = i64::from(now.hour() * 60 + now.minute());
    let east = (h * 60 + m - minutes).rem_euclid(1440);
    let east = if east > 720 { east - 1440 } else { east };
    // POSIX's sign is west of UTC.
    let sign = if east < 0 { '+' } else { '-' };
    let tz = format!("XXX{sign}{:02}:{:02}", east.abs() / 60, east.abs() % 60);
    (tz, FixedOffset::east_opt(east as i32 * 60).unwrap())
}

fn datum(db: &Path, tz: &str, args: &[&str]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .arg("--db")
        .arg(db)
        .args(args)
        .env("HOME", db.parent().unwrap())
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("TZ", tz)
        // Today's clock is the point here, and `TZ` sets the hour it reads.
        .env("DATUM_TEST_REFUSE_BARE_CLOCK", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn at(db: &Path, offset: FixedOffset) -> DateTime<FixedOffset> {
    let at: String = rusqlite::Connection::open(db)
        .unwrap()
        .query_row("SELECT at FROM entries WHERE field = 'x'", [], |r| r.get(0))
        .unwrap();
    DateTime::parse_from_rfc3339(&at)
        .unwrap()
        .with_timezone(&offset)
}

/// The action word of `moment x <time>`'s echo, as text and as JSON.
fn words(time: &[&str]) -> (String, String) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    let (tz, _) = clock_reading(15, 0);
    let run = |show: &[&str], field: &str| {
        let args = [&["moment", field][..], time, show].concat();
        String::from_utf8(datum(&db, &tz, &args).stdout).unwrap()
    };
    let text = run(&[], "x");
    let json: serde_json::Value = serde_json::from_str(&run(&["--show", "json"], "y")).unwrap();
    (
        text.split_whitespace().next().unwrap().to_string(),
        json["docs"][0]["action"].as_str().unwrap().to_string(),
    )
}

#[test]
fn an_entry_after_now_is_future_and_one_before_or_at_now_is_its_shape() {
    for (time, word) in [
        (&["-t", "+5"][..], "FUTURE"),
        (&["-t", "+1d"], "FUTURE"),
        (&["-t", "-5"], "MOMENT"),
        (&[], "MOMENT"),
    ] {
        assert_eq!(words(time), (word.into(), word.into()), "{time:?}");
    }
}

/// Logs `moment x -t <clock>` with the clock reading `h:m`: its stderr,
/// the instant stored, the database and `TZ`, and the run's offset.
fn log_at(
    h: i64,
    m: i64,
    clock: &str,
) -> (String, DateTime<FixedOffset>, tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    let (tz, offset) = clock_reading(h, m);
    let out = datum(&db, &tz, &["moment", "x", "-t", clock]);
    let stored = at(&db, offset);
    (String::from_utf8(out.stderr).unwrap(), stored, dir, tz)
}

#[test]
fn an_evening_logged_in_the_morning_is_hinted_and_retime_y_moves_it_to_yesterday() {
    let (stderr, stored, dir, tz) = log_at(9, 0, "23:30");
    assert_eq!(stderr.trim(), HINT);
    let today = Utc::now().with_timezone(stored.offset()).date_naive();
    let clock = NaiveTime::from_hms_opt(23, 30, 0).unwrap();
    assert_eq!((stored.date_naive(), stored.time()), (today, clock));

    let db = dir.path().join("t.db");
    datum(&db, &tz, &["retime", "-y"]);
    let moved = at(&db, *stored.offset());
    assert_eq!(moved, stored - Duration::days(1));
}

#[test]
fn the_hint_is_for_an_evening_alone_logged_before_noon() {
    assert_eq!(log_at(9, 0, "19:30").0, "");
    assert_eq!(log_at(13, 0, "23:30").0, "");
    assert_eq!(log_at(9, 0, "08:00").0, "");
}
