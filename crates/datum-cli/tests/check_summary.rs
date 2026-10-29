//! da-3zq6g: `datum check` alone sums each checker up, and lists a finding
//! only where a checker found five or fewer; `datum check <name>` lists every
//! one, `tz` with the entries around each. And a second run, which reads only
//! what the first found and what changed since, finds what a run from
//! scratch finds.

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn datum(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("NO_COLOR", "1")
        .env("TZ", "Europe/Berlin")
        .args(args)
        .output()
        .expect("spawn datum");
    // `check` exits 1 on an error it found; nothing here should.
    assert!(
        out.status.success(),
        "datum {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Eight entries an hour apart in Berlin but the fifth, logged in New York:
/// a zone the entries around it agree is not its own.
fn spike(db: &Path, day: &str) {
    for h in 10..18 {
        let zone = if h == 14 {
            "America/New_York"
        } else {
            "Europe/Berlin"
        };
        let t = format!("{day}T{h}:00:00+00:00");
        datum(db, &["occur", "walk", "-t", &t, "-z", zone]);
    }
}

/// The lines under the summary line of `name` in a summed-up report.
fn under<'a>(report: &'a str, name: &str) -> Vec<&'a str> {
    let mut lines = report
        .lines()
        .skip_while(|l| !l.starts_with(&format!("{name} ")));
    lines.next().expect(name);
    lines.take_while(|l| l.starts_with(' ')).collect()
}

#[test]
fn alone_check_sums_up_and_a_named_check_lists_all() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("summary.db");
    spike(&db, "2026-03-02");
    for i in 0..7 {
        datum(&db, &["flag", "walk", &format!("look {i}")]);
    }

    let report = datum(&db, &["check"]);
    // Seven flags: the first five, and how to list them all.
    let flags = under(&report, "flags");
    assert!(report.contains("\nflags      7 problems\n"), "{report}");
    assert_eq!(
        flags.iter().filter(|l| l.starts_with("    ")).count(),
        5,
        "{report}"
    );
    assert_eq!(
        flags.last().copied(),
        Some("  2 more: `datum check flags` lists them all"),
        "{report}"
    );
    // One spike: it, and no hint.
    let tz = under(&report, "tz");
    assert!(report.contains("\ntz         1 problem\n"), "{report}");
    assert!(
        tz.iter()
            .any(|l| l.starts_with("    ") && l.contains("walk")),
        "{report}"
    );
    assert!(!tz.iter().any(|l| l.contains("more:")), "{report}");
    assert!(
        !tz.iter().any(|l| l.contains("↳")),
        "the neighbors are the named check's: {report}"
    );
    assert!(report.starts_with("entries    ok\n"), "{report}");

    // Named, every one.
    let all = datum(&db, &["check", "flags"]);
    assert_eq!(
        (0..7)
            .filter(|i| all.contains(&format!("look {i}")))
            .count(),
        7,
        "{all}"
    );
    // And `tz` the entries before and after it, with their zones and offsets.
    let tz = datum(&db, &["check", "tz"]);
    let before = tz.lines().find(|l| l.contains("↳ before:")).expect(&tz);
    let after = tz.lines().find(|l| l.contains("↳ after:")).expect(&tz);
    assert!(
        before.contains("2026-03-02 13:00 +01:00 Europe/Berlin"),
        "{tz}"
    );
    assert_eq!(before.matches("Europe/Berlin").count(), 3, "{tz}");
    assert!(
        after.contains("2026-03-02 16:00 +01:00 Europe/Berlin"),
        "{tz}"
    );
}

/// A finding's count is not its entries': seven keys a spec does not declare,
/// on one entry, list that entry and nothing more (PR #1400 review).
#[test]
fn more_counts_what_is_listed() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("keys.db");
    datum(
        &db,
        &[
            "occur", "tea", "a=1", "b=1", "c=1", "d=1", "e=1", "f=1", "g=1",
        ],
    );
    datum(&db, &["setup", "tea", "--only-listed-keys"]);
    let report = datum(&db, &["check"]);
    assert!(report.starts_with("entries    7 problems\n"), "{report}");
    let entries = under(&report, "entries");
    assert_eq!(
        entries.iter().filter(|l| l.starts_with("    ")).count(),
        1,
        "{report}"
    );
    assert!(!entries.iter().any(|l| l.contains("more:")), "{report}");
}

#[test]
fn a_second_run_finds_what_a_run_from_scratch_finds() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("again.db");
    spike(&db, "2026-03-02");
    datum(
        &db,
        &["switch", "mood", "calm", "-t", "2026-03-02T09:00:00+00:00"],
    );
    datum(
        &db,
        &["switch", "mood", "calm", "-t", "2026-03-02T09:30:00+00:00"],
    );
    datum(&db, &["flag", "walk", "look"]);
    datum(&db, &["occur", "tea", "cups=2"]);
    let issues = |json: &str| serde_json::from_str::<Value>(json).unwrap();
    let first = issues(&datum(&db, &["check", "--json"]));
    assert!(first["examined"].get("rest_unchanged").is_none(), "{first}");

    // Writes the second run must read: the spike goes, a new one comes in a
    // run of Berlin rows, a state repeats again, a flag is dealt with.
    let hid = |category: &str| -> String {
        let issue = first["issues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["category"] == category);
        issue.unwrap()["entries"][0]["hid"]
            .as_str()
            .unwrap()
            .to_string()
    };
    datum(&db, &["del", &hid("tz_off_neighbours")]);
    spike(&db, "2026-03-05");
    datum(
        &db,
        &["switch", "mood", "calm", "-t", "2026-03-02T10:00:00+00:00"],
    );
    datum(&db, &["update", &hid("flagged_entry"), "done=true"]);
    // A spec alone changed: its field's rows, which did not, now have a key
    // it does not declare. And a field new since, with a spec of its own.
    datum(&db, &["setup", "tea", "--only-listed-keys"]);
    datum(&db, &["occur", "coffee"]);

    let again = issues(&datum(&db, &["check", "--json"]));
    assert_eq!(again["examined"]["rest_unchanged"], true, "{again}");
    // What it kept forgotten, the same database checked from scratch.
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("DELETE FROM check_kept", []).unwrap();
    drop(conn);
    let whole = issues(&datum(&db, &["check", "--json"]));
    assert!(whole["examined"].get("rest_unchanged").is_none(), "{whole}");
    assert_eq!(again["issues"], whole["issues"]);
    let categories: Vec<&str> = (whole["issues"].as_array().unwrap().iter())
        .map(|i| i["category"].as_str().unwrap())
        .collect();
    assert_eq!(
        categories,
        [
            "orphaned_extra_data_key",
            "repeated_state",
            "tz_off_neighbours"
        ],
        "{whole}"
    );
    assert_eq!(whole["issues"][1]["count"], 2, "{whole}");
    // Neither the spec nor the new field read every row again.
    let read = |run: &Value| run["examined"]["entries"].as_u64().unwrap();
    assert!(read(&again) < read(&whole), "{again}\n{whole}");
}

/// A row moved in place (a retime keeps its hid) leaves findings where it
/// was, which the log's old instant alone reaches (vremya, PR #1400): a run
/// of three New York rows among Berlin ones is a step, and with one moved
/// days and a day of rows away, the two left are off their neighbors.
#[test]
fn a_moved_row_is_read_again_where_it_was() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("moved.db");
    for h in 10..19 {
        let zone = match h {
            13..=15 => "America/New_York",
            _ => "Europe/Berlin",
        };
        let t = format!("2026-03-02T{h}:00:00+00:00");
        datum(&db, &["occur", "walk", "-t", &t, "-z", zone]);
    }
    for h in 0..24 {
        let t = format!("2026-03-05T{h:02}:00:00+00:00");
        datum(&db, &["occur", "walk", "-t", &t, "-z", "Europe/Berlin"]);
    }
    let issues = |json: &str| serde_json::from_str::<Value>(json).unwrap();
    let first = issues(&datum(&db, &["check", "--json"]));
    assert_eq!(first["issues"], serde_json::json!([]), "{first}");

    let conn = rusqlite::Connection::open(&db).unwrap();
    let hid: String = conn
        .query_row(
            "SELECT hid FROM entries WHERE occur_time LIKE '2026-03-02T13:00%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);
    datum(&db, &["retime", &hid, "-t", "2026-03-09T13:00:00+00:00"]);

    let again = issues(&datum(&db, &["check", "--json"]));
    assert_eq!(again["examined"]["rest_unchanged"], true, "{again}");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("DELETE FROM check_kept", []).unwrap();
    drop(conn);
    let whole = issues(&datum(&db, &["check", "--json"]));
    assert_eq!(again["issues"], whole["issues"]);
    let off = (whole["issues"].as_array().unwrap().iter())
        .find(|i| i["category"] == "tz_off_neighbours")
        .expect("the two left behind");
    assert_eq!(off["count"], 2, "{whole}");
}
