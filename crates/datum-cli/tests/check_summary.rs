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
    assert!(!tz.contains("↳"), "{tz}");
    let rows = around_rows(&tz);
    assert_eq!(
        rows,
        [
            "    2026-03-02 12:00:00  +01:00  Europe/Berlin     ",
            "    2026-03-02 13:00:00  +01:00  Europe/Berlin     ",
            "    2026-03-02 14:00:00  +01:00  Europe/Berlin     ",
            "  > 2026-03-02 09:00:00  -05:00  America/New_York  ",
            "    2026-03-02 16:00:00  +01:00  Europe/Berlin     ",
            "    2026-03-02 17:00:00  +01:00  Europe/Berlin     ",
            "    2026-03-02 18:00:00  +01:00  Europe/Berlin     ",
        ]
        .map(|cols| (cols.to_string(), 5)),
        "{tz}"
    );
    // The entry, on its own row and no other: the rows hold a hid and stop.
    assert_eq!(
        tz.lines()
            .filter(|l| l.contains("walk"))
            .collect::<Vec<_>>(),
        tz.lines()
            .filter(|l| l.starts_with("  > "))
            .collect::<Vec<_>>(),
        "{tz}"
    );
    assert!(tz.lines().any(|l| l.ends_with("  walk ●")), "{tz}");
}

/// The rows of `report`'s first block of `tz_off_neighbours`, each as its
/// columns up to the hid and the length of its hid.
fn around_rows(report: &str) -> Vec<(String, usize)> {
    (report.lines())
        .skip_while(|l| !l.contains("tz_off_neighbours"))
        .skip(1)
        .take_while(|l| l.starts_with("    ") || l.starts_with("  > "))
        .map(|l| {
            let start = 4 + 19 + 2 + 6 + 2 + "America/New_York".len() + 2;
            let hid = l[start..].split(' ').next().unwrap();
            (l[..start].to_string(), hid.len())
        })
        .collect()
}

/// Two flagged entries whose neighbors overlap are a block each, in full,
/// a blank line between, each marking its own (da-la4n9).
#[test]
fn two_flagged_are_two_blocks() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("two.db");
    for h in 10..21 {
        let zone = match h {
            14 | 17 => "America/New_York",
            _ => "Europe/Berlin",
        };
        let t = format!("2026-03-02T{h}:00:00+00:00");
        datum(&db, &["occur", "walk", "-t", &t, "-z", zone]);
    }
    let tz = datum(&db, &["check", "tz"]);
    let blocks: Vec<&str> = (tz.split("\n\n")).filter(|b| b.contains("  > ")).collect();
    assert_eq!(blocks.len(), 2, "{tz}");
    let marked = |b: &str| {
        let row = b.lines().find(|l| l.starts_with("  > ")).expect(b);
        row[4..23].to_string()
    };
    assert_eq!(marked(blocks[0]), "2026-03-02 09:00:00", "{tz}");
    assert_eq!(marked(blocks[1]), "2026-03-02 12:00:00", "{tz}");
    for block in &blocks {
        let rows = block.lines().filter(|l| l.contains("2026-03-02")).count();
        assert_eq!(rows, 7, "{tz}");
        assert_eq!(block.matches("  > ").count(), 1, "{tz}");
    }
    // The other flagged entry stands among the first's neighbors, unmarked.
    assert!(
        blocks[0].contains("\n    2026-03-02 12:00:00  -05:00"),
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

/// A finding written before the log's cap passed a run's mark is found: the
/// run reads from scratch, not the part of the log the cap left (da-4a3cv).
#[test]
fn a_mark_the_cap_passed_finds_what_it_dropped() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("capped.db");
    datum(&db, &["occur", "tea"]);
    let check = || -> Value { serde_json::from_str(&datum(&db, &["check", "--json"])).unwrap() };
    assert!(check()["issues"].as_array().unwrap().is_empty());
    // The spike, then enough changes that the cap drops it from the log.
    spike(&db, "2026-03-02");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 40000)
         INSERT INTO check_changes (hid, field, at) SELECT 'h', 'f', '' FROM c",
        [],
    )
    .unwrap();
    drop(conn);
    let run = check();
    assert!(run["examined"].get("rest_unchanged").is_none(), "{run}");
    assert!(
        run["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["category"] == "tz_off_neighbours"),
        "{run}"
    );
}

/// A prune under another stamp that empties the log passes a mark, and leaves
/// no row to tell by: the run reads from scratch all the same (da-tuhig).
#[test]
fn a_mark_another_stamp_pruned_past_finds_what_it_dropped() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("stamps.db");
    datum(&db, &["occur", "tea"]);
    datum(&db, &["check"]);
    spike(&db, "2026-03-02");
    let other = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", &db)
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path())
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("DATUM_ID_DELIMITER", ";")
        .env("TZ", "Europe/Berlin")
        .args(["check", "integrity"])
        .output()
        .expect("spawn datum");
    assert!(other.status.success());
    let conn = rusqlite::Connection::open(&db).unwrap();
    let logged: i64 = conn
        .query_row("SELECT count(*) FROM check_changes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(logged, 0, "the other stamp's run prunes the log empty");
    drop(conn);
    let run: Value = serde_json::from_str(&datum(&db, &["check", "tz", "--json"])).unwrap();
    assert!(
        run["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["category"] == "tz_off_neighbours"),
        "{run}"
    );
    assert!(run["examined"].get("rest_unchanged").is_none(), "{run}");
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
