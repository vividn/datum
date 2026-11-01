//! What the `TZ` a process is given does to an entry's time (da-4nsb5): each
//! row runs the real binary, since chrono keeps its read of `TZ` for a second
//! and in-process tests would read each other's.

use std::process::{Command, Output};

use chrono::{DateTime, FixedOffset, Utc};

fn run(dir: &tempfile::TempDir, tz: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
    cmd.args(["--db", dir.path().join("t.db").to_str().unwrap()])
        .args(args)
        .env("HOME", dir.path())
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env_remove("TZ");
    if let Some(tz) = tz {
        cmd.env("TZ", tz);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{tz:?} {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn warnings(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter(|line| line.contains("warning"))
        .map(str::to_string)
        .collect()
}

/// Logs a moment under `tz` and reads back its zone name and offset (minutes),
/// with what the log said on stderr.
fn logged(tz: Option<&str>) -> (String, i64, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let log = run(&dir, tz, &["moment", "x"]);
    let (tz, offset) = rusqlite::Connection::open(dir.path().join("t.db"))
        .unwrap()
        .query_row(
            "SELECT COALESCE(at_tz, ''), at_offset FROM entries WHERE field = 'x'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    (tz, offset, warnings(&log))
}

/// `tail` under `tz` prints the clock of `offset`: the wrong clock is what the
/// bead saw. The minute may turn between the log and the check.
fn tail_shows(tz: &str, offset: i64) {
    let dir = tempfile::tempdir().unwrap();
    let before = Utc::now();
    run(&dir, Some(tz), &["moment", "x"]);
    let tail = stdout(&run(&dir, Some(tz), &["tail", "x"]));
    let zone = FixedOffset::east_opt(offset as i32 * 60).unwrap();
    let clock = |t: DateTime<Utc>| t.with_timezone(&zone).format("%H:%M").to_string();
    let after = Utc::now();
    assert!(
        tail.contains(&clock(before)) || tail.contains(&clock(after)),
        "TZ={tz}: {tail}"
    );
}

#[test]
fn a_name_is_named() {
    let (tz, offset, warned) = logged(Some("Asia/Tokyo"));
    assert_eq!((tz.as_str(), offset), ("Asia/Tokyo", 540));
    assert_eq!(warned, Vec::<String>::new());
    tail_shows("Asia/Tokyo", 540);
}

#[test]
fn a_posix_name_tzdb_knows_is_named() {
    let (tz, offset, warned) = logged(Some("EST5EDT"));
    assert_eq!(tz, "EST5EDT");
    assert!(offset == -300 || offset == -240, "{offset}");
    assert_eq!(warned, Vec::<String>::new());
    tail_shows("EST5EDT", offset);
}

#[test]
fn a_posix_offset_is_read_in_posix_sign_with_no_name() {
    for (posix, minutes) in [
        ("<+22>-22", 22 * 60),
        ("UTC-5", 5 * 60),
        ("AAA+3:30", -210),
        ("<-03>3", -180),
    ] {
        let (tz, offset, warned) = logged(Some(posix));
        assert_eq!((tz.as_str(), offset), ("", minutes), "TZ={posix}");
        assert_eq!(warned, Vec::<String>::new(), "TZ={posix}");
        tail_shows(posix, minutes);
    }
}

#[test]
fn a_posix_rule_is_read_at_its_offset_now() {
    // January and July both fall in it: it is daylight time all year but a day.
    let (tz, offset, warned) = logged(Some("AAA-3BBB-4,0/0,365/23"));
    assert_eq!((tz.as_str(), offset), ("", 240));
    assert_eq!(warned, Vec::<String>::new());
}

#[test]
fn a_posix_rule_of_month_week_and_day_is_read() {
    let (tz, offset, warned) = logged(Some("AAA-7BBB,M3.5.0,M10.5.0/3"));
    assert_eq!(tz, "");
    assert!(offset == 420 || offset == 480, "{offset}");
    assert_eq!(warned, Vec::<String>::new());
}

#[test]
fn an_empty_tz_is_utc() {
    let (tz, offset, warned) = logged(Some(""));
    assert_eq!((tz.as_str(), offset), ("UTC", 0));
    assert_eq!(warned, Vec::<String>::new());
}

#[test]
fn a_zoneinfo_path_is_its_zones_name() {
    let berlin = "/usr/share/zoneinfo/Europe/Berlin";
    if !std::path::Path::new(berlin).exists() {
        return; // no tzdata on this host
    }
    for tz in [berlin, &format!(":{berlin}"), "../zoneinfo/Europe/Berlin"] {
        let (name, _, warned) = logged(Some(tz));
        assert_eq!(name, "Europe/Berlin", "TZ={tz}");
        assert_eq!(warned, Vec::<String>::new(), "TZ={tz}");
    }
}

#[test]
fn what_cannot_be_read_is_the_hosts_and_says_so_once() {
    let (host_tz, host_offset, _) = logged(None);
    for unread in [
        "Nonsense/Zone",
        "AEST-10AEDT",
        ":UTC-5",
        "UTC-24",
        "/etc/passwd",
    ] {
        let (tz, offset, warned) = logged(Some(unread));
        assert_eq!((&tz, offset), (&host_tz, host_offset), "TZ={unread}");
        assert_eq!(warned.len(), 1, "TZ={unread}: {warned:?}");
        assert!(warned[0].contains(unread), "{warned:?}");
        if !host_tz.is_empty() {
            assert!(warned[0].contains(&host_tz), "{warned:?}");
        }
    }
}
