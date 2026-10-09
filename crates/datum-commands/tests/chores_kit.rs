//! End-to-end test for the chores kit — v2's chores, as a kit (vremya,
//! 2026-09-27; vividn: «implement the datum v2 chores as a datum kit»).
//!
//! The semantics are v2's (reference: datum-v2 projects/datum/views/
//! chores.ts and src/chorelist.ts): per field, `due` comes from the LATEST
//! entry by occur time (create time for a shirk), `last_occur` from the
//! latest occurrence only, and `iti` from that occurrence; `chorelist`
//! shows what is due or was done today, coloured by how overdue a chore is
//! against its iti.
//!
//! «Today» is the views' `local_today()`, which follows the test clock
//! (da-5fonh). A timed chore's `is_due` still reads SQLite's own `'now'`,
//! which it cannot pin, so those tests use instants far in the past.
//!
//! `last_occur` is the UTC instant, so an entry whose `last_occur` date a
//! test reads is logged `-z UTC`: in the host's zone, 10:00 at +14 is the
//! day before in UTC (da-5ig22).

use std::fs;
use std::io::Write;

use datum_commands::cmd::dispatch_argv;
use datum_core::db::{Db, ViewResult};
use datum_core::test_util::fresh_db;
use serde_json::Value as JsonValue;

/// Path to the shipped chores kit, relative to the crate.
const CHORES_KIT_PATH: &str = "../../kits/chores.toml";

fn run(db: &Db, args: &[&str]) {
    let mut argv: Vec<String> = vec!["datum".into()];
    argv.extend(args.iter().map(|s| s.to_string()));
    dispatch_argv(argv, db).unwrap();
}

fn install_chores(db: &Db) {
    let kit_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(CHORES_KIT_PATH)
        .canonicalize()
        .expect("kits/chores.toml must exist at workspace root");
    run(
        db,
        &["kit", "install", "--yes", &kit_path.to_string_lossy()],
    );
}

fn index_exists(db: &Db, on_disk_name: &str) -> bool {
    let n: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            rusqlite::params![on_disk_name],
            |row| row.get(0),
        )
        .unwrap();
    n > 0
}

fn view(db: &Db, name: &str) -> ViewResult {
    let view = db.get_view(name).unwrap().expect("view installed");
    db.run_view(&view, &Default::default()).unwrap()
}

/// The chores `chorelist <args…>` lists, in order.
fn chorelist(db: &Db, args: &[&str]) -> Vec<String> {
    let mut argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    argv.extend(["--show".into(), "json".into()]);
    let out = datum_commands::commands::kit_command::output("chorelist", &argv, db).unwrap();
    let rows: Vec<JsonValue> = serde_json::from_str(&out).unwrap();
    rows.iter()
        .map(|r| r["field"].as_str().unwrap().to_string())
        .collect()
}

/// `field`'s cell in column `col`.
fn cell<'a>(result: &'a ViewResult, field: &str, col: &str) -> &'a JsonValue {
    let f = result.columns.iter().position(|c| c == "field").unwrap();
    let c = result.columns.iter().position(|c| c == col).unwrap();
    let row = result
        .rows
        .iter()
        .find(|r| r[f].as_str() == Some(field))
        .unwrap_or_else(|| panic!("no row for {field}"));
    &row[c]
}

fn fields(result: &ViewResult) -> Vec<String> {
    let f = result.columns.iter().position(|c| c == "field").unwrap();
    result
        .rows
        .iter()
        .map(|r| r[f].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_kit_ships_its_index_views_and_commands() {
    let src =
        fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(CHORES_KIT_PATH))
            .unwrap();
    let kit = datum_core::kit::KitToml::from_str(&src).unwrap();
    assert_eq!(kit.kit.name, "chores");
    assert_eq!(kit.fields.len(), 0, "chores are the user's own fields");
    assert_eq!(kit.indices.len(), 1);
    let views: Vec<_> = kit.views.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(views, ["chores", "chores_due"]);
    for cmd in &kit.commands {
        assert!(cmd.flags.iter().any(|f| f == "watch"), "{}", cmd.name);
        assert_eq!(cmd.color_rules.len(), 3, "{}", cmd.name);
        // A command's rules and sort see only the columns it shows, so the
        // colours ride on the shown `status` column.
        assert!(cmd.columns.iter().any(|c| c == "status"), "{}", cmd.name);
    }
    let cmds: Vec<_> = kit.commands.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(cmds, ["chorelist"], "one chorelist, as v2 had (da-7ramf)");
    assert_eq!(kit.commands[0].args, ["field="]);
    let aliases: Vec<_> = kit.aliases.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(aliases, ["newchore"]);
    assert_eq!(kit.commands[0].switches, ["all"]);
    assert_eq!(
        kit.commands[0].empty.as_deref(),
        Some("🧹😄 COMPLETED! GOOD WORK! 😄🪣")
    );
}

#[test]
fn install_creates_the_index_and_uninstall_drops_everything() {
    let db = fresh_db();
    install_chores(&db);
    assert!(index_exists(&db, "kit_chores__chore_rows"));
    let chores = db.get_view("chores").unwrap().expect("view installed");
    for spelling in ["next_time", "nextTime", "next_date", "nextDate"] {
        assert!(
            chores.sql_text.contains(spelling),
            "chores reads {spelling}"
        );
    }
    // chores_due reads them through chores (da-3mzyu).
    let due = db.get_view("chores_due").unwrap().expect("view installed");
    assert_eq!(due.uses, ["chores"]);
    assert!(db.get_kit_command("chorelist").unwrap().is_some());

    run(&db, &["kit", "uninstall", "chores"]);
    assert!(!index_exists(&db, "kit_chores__chore_rows"));
    assert!(db.get_view("chores").unwrap().is_none());
    assert!(db.get_kit_command("chorelist").unwrap().is_none());
    assert!(db.list_kits().unwrap().is_empty());
}

/// v2 reduces by TIME (occur time), not by when the entry was typed: a
/// backdated occurrence logged afterwards does not take over `due`.
#[test]
fn the_latest_occurrence_by_occur_time_sets_due() {
    let db = fresh_db();
    install_chores(&db);
    run(
        &db,
        &[
            "occur",
            "laundry",
            "-d",
            "2020-05-10",
            "-t",
            "10:00",
            "-z",
            "UTC",
            "next_date=2020-05-17",
        ],
    );
    run(
        &db,
        &[
            "occur",
            "laundry",
            "-d",
            "2020-05-01",
            "-t",
            "10:00",
            "-z",
            "UTC",
            "next_date=2020-05-08",
        ],
    );
    let all = view(&db, "chores");
    assert_eq!(cell(&all, "laundry", "due"), "2020-05-17");
    assert!(cell(&all, "laundry", "last_occur")
        .as_str()
        .unwrap()
        .starts_with("2020-05-10"));
}

/// A shirk (`-T`: no occur time) moves `due` without claiming the chore
/// was done, so `last_occur` stays on the real occurrence.
#[test]
fn a_shirk_moves_due_but_not_last_occur() {
    let db = fresh_db();
    install_chores(&db);
    run(
        &db,
        &[
            "occur",
            "sweep",
            "-d",
            "2020-01-01",
            "-t",
            "08:00",
            "-z",
            "UTC",
            "next_date=2020-01-08",
        ],
    );
    run(&db, &["occur", "sweep", "-T", "next_date=2099-01-01"]);
    let all = view(&db, "chores");
    assert_eq!(cell(&all, "sweep", "due"), "2099-01-01");
    assert!(cell(&all, "sweep", "last_occur")
        .as_str()
        .unwrap()
        .starts_with("2020-01-01"));
    assert_eq!(cell(&all, "sweep", "is_due"), 0);
}

/// An unspecced chore's interval is v2's, from its latest occurrence: a
/// date's whole days, whatever hour it was done at (vividn, 09-28: the
/// partial sort went).
#[test]
fn iti_counts_whole_days_whatever_the_hour() {
    let db = fresh_db();
    install_chores(&db);
    run(
        &db,
        &[
            "occur",
            "dishes",
            "-d",
            "2020-03-01",
            "-t",
            "18:00",
            "next_date=2020-03-02",
        ],
    );
    let iti = cell(&view(&db, "chores"), "dishes", "iti")
        .as_f64()
        .unwrap();
    assert_eq!(iti, 1.0);
}

/// The four states `chorelist` colours, all against the moment the test
/// logs: overdue (by more than its iti), late (by more than half), due,
/// and done today. A chore not yet due is not in `chores_due`.
#[test]
fn status_follows_v2s_colours_and_chores_due_filters() {
    let db = fresh_db();
    install_chores(&db);
    // Due 2 days ago, iti 1: overdue.
    run(
        &db,
        &["occur", "dishes", "-d", "-3", "-t", "00:00", "next_date=+1"],
    );
    // Due 2 days ago, iti 3: late (2 > 3/2).
    run(
        &db,
        &["occur", "bins", "-d", "-5", "-t", "00:00", "next_date=+3"],
    );
    // Due today, iti 7: due, and not late.
    run(
        &db,
        &["occur", "mop", "-d", "-7", "-t", "00:00", "next_date=+7"],
    );
    // Done now, due in a week.
    run(&db, &["occur", "vacuum", "next_date=+7"]);
    // Not due for a year.
    run(
        &db,
        &["occur", "windows", "-d", "-1", "next_date=2099-01-01"],
    );

    let all = view(&db, "chores");
    assert_eq!(cell(&all, "dishes", "status"), "overdue");
    assert_eq!(cell(&all, "bins", "status"), "late");
    assert_eq!(cell(&all, "mop", "status"), "due");
    assert_eq!(cell(&all, "vacuum", "status"), "done");
    assert_eq!(cell(&all, "windows", "status"), "");

    let due = fields(&view(&db, "chores_due"));
    assert_eq!(due, ["dishes", "bins", "mop", "vacuum"], "sorted by iti");

    // v2's forms, through the one command (da-7ramf).
    assert_eq!(chorelist(&db, &[]), due);
    assert_eq!(chorelist(&db, &["vacuum"]), ["vacuum"]);
    assert_eq!(chorelist(&db, &["field=bins"]), ["bins"]);
    assert!(chorelist(&db, &["windows"]).is_empty(), "not due");
    assert_eq!(chorelist(&db, &["windows", "--all"]), ["windows"]);
    assert_eq!(chorelist(&db, &["--all"]).len(), 5, "every chore");
    assert_eq!(
        chorelist(&db, &["--all", "--sort", "due"]),
        ["dishes", "bins", "mop", "vacuum", "windows"]
    );
    // dishes and bins are both due two days ago: a tie keeps the view's
    // order, most frequent first, either way round.
    assert_eq!(
        chorelist(&db, &["--all", "--sort", "-due"]),
        ["windows", "vacuum", "mop", "dishes", "bins"]
    );
}

/// Install a clock frozen at `utc` in Europe/Berlin for this thread. That is
/// `DatumTime::now()`'s zone only: a logged entry is still stamped in the
/// HOST's zone (`TZ`), which a test must not set beside its siblings, so a
/// test that logs under this clock says `-z Europe/Berlin` (da-5fonh: one
/// that did not passed on a Berlin box and failed CI's UTC one).
fn berlin_clock(utc: &str) -> impl Drop {
    use datum_core::time::clock::{self, MockClock};
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            clock::uninstall();
        }
    }
    let at = datum_core::time::DatumTime::from_utc_and_tz(utc.parse().unwrap(), "Europe/Berlin")
        .unwrap();
    clock::install(std::sync::Arc::new(MockClock::new(at)));
    Reset
}

/// A chore done just after local midnight is done TODAY: at 00:30 in Berlin
/// the UTC day is still yesterday, which is the day SQLite's local-time
/// modifier answers on datum.day (da-5fonh).
#[test]
fn a_chore_done_just_after_local_midnight_is_done_today() {
    let db = fresh_db();
    install_chores(&db);
    let _clock = berlin_clock("2026-09-09T22:30:00Z");
    let berlin = ["-z", "Europe/Berlin"];
    run(
        &db,
        &[&["occur", "vacuum", "next_date=+7"], &berlin[..]].concat(),
    );
    // Due on the local day, a day the UTC day has not reached.
    run(
        &db,
        &[
            &["occur", "mop", "-d", "-7", "-t", "12:00", "next_date=+7"],
            &berlin[..],
        ]
        .concat(),
    );

    let all = view(&db, "chores");
    assert_eq!(cell(&all, "vacuum", "due"), "2026-09-17");
    assert_eq!(cell(&all, "vacuum", "status"), "done");
    assert_eq!(cell(&all, "mop", "due"), "2026-09-10");
    assert_eq!(cell(&all, "mop", "status"), "due");
    assert_eq!(chorelist(&db, &[]), ["mop", "vacuum"]);
}

/// A timed chore's due DAY is its instant in the offset its entry was
/// logged with: 22:30Z on the 7th is the 8th at +02, two days before the
/// 10th and not three.
#[test]
fn a_timed_chores_due_day_is_in_its_entrys_offset() {
    let db = fresh_db();
    install_chores(&db);
    let mut f = tempfile::NamedTempFile::new().unwrap();
    let docs = serde_json::json!({"docs": [
        {"_id": "mop:2026-09-01T08:00:00.000Z",
         "data": {"field": "mop",
                  "occurTime": {"utc": "2026-09-01T08:00:00.000Z", "o": 2, "tz": "Europe/Berlin"},
                  "nextTime": {"utc": "2026-09-07T22:30:00.000Z", "o": 2, "tz": "Europe/Berlin"}},
         "meta": {"humanId": "m1", "createTime": {"utc": "2026-09-01T08:00:01.000Z", "o": 2, "tz": "Europe/Berlin"}}}
    ]});
    write!(f, "{docs}").unwrap();
    run(&db, &["db", "v2import", f.path().to_str().unwrap()]);

    let _clock = berlin_clock("2026-09-09T22:30:00Z");
    assert_eq!(cell(&view(&db, "chores"), "mop", "days_overdue"), 2);
}

/// v2import keeps v2's spellings: `nextDate`, and `nextTime` as a
/// `{utc, o, tz}` block, whose `utc` is the due time.
#[test]
fn v2_imported_chores_read_nextdate_and_the_nexttime_block() {
    let db = fresh_db();
    install_chores(&db);
    let mut f = tempfile::NamedTempFile::new().unwrap();
    let docs = serde_json::json!({"docs": [
        {"_id": "mop:2020-09-20T08:00:00.000Z",
         "data": {"field": "mop",
                  "occurTime": {"utc": "2020-09-20T08:00:00.000Z", "o": 0, "tz": "UTC"},
                  "nextTime": {"utc": "2020-09-27T06:00:00.000Z", "o": 0, "tz": "UTC"}},
         "meta": {"humanId": "m1", "createTime": {"utc": "2020-09-20T08:00:01.000Z", "o": 0, "tz": "UTC"}}},
        {"_id": "oven:2020-08-01T09:00:00.000Z",
         "data": {"field": "oven",
                  "occurTime": {"utc": "2020-08-01T09:00:00.000Z", "o": 0, "tz": "UTC"},
                  "nextDate": "2020-08-15"},
         "meta": {"humanId": "o1", "createTime": {"utc": "2020-08-01T09:00:01.000Z", "o": 0, "tz": "UTC"}}}
    ]});
    write!(f, "{docs}").unwrap();
    run(&db, &["db", "v2import", f.path().to_str().unwrap()]);

    let all = view(&db, "chores");
    assert_eq!(cell(&all, "mop", "due"), "2020-09-27T06:00:00.000Z");
    let mop_iti = cell(&all, "mop", "iti").as_f64().unwrap();
    assert!((mop_iti - (6.0 + 22.0 / 24.0)).abs() < 1e-6, "{mop_iti}");
    assert_eq!(cell(&all, "oven", "due"), "2020-08-15");
    assert_eq!(cell(&all, "oven", "status"), "overdue");
}

/// da-x15of: `next_time=+Nh` logs an instant, so its due keeps its day and
/// the interval is the N hours — a clock time read as 2000-01-01 made it
/// -9776 days.
#[test]
fn a_logged_next_time_is_due_at_its_instant() {
    let db = fresh_db();
    install_chores(&db);
    run(
        &db,
        &[
            "occur",
            "plants",
            "-t",
            "2020-09-20T20:00:00Z",
            "next_time=+16h",
        ],
    );
    let all = view(&db, "chores");
    assert_eq!(cell(&all, "plants", "due"), "2020-09-21T12:00:00.000Z");
    let iti = cell(&all, "plants", "iti").as_f64().unwrap();
    assert!((iti - 16.0 / 24.0).abs() < 1e-6, "{iti}");
    assert_eq!(cell(&all, "plants", "status"), "overdue");
}

#[test]
fn entries_without_a_next_due_key_are_not_chores() {
    let db = fresh_db();
    install_chores(&db);
    run(&db, &["occur", "coffee"]);
    run(&db, &["switch", "mood", "happy"]);
    assert!(view(&db, "chores").rows.is_empty());
}

#[test]
fn chorelist_renders_its_table_and_its_empty_words() {
    let db = fresh_db();
    install_chores(&db);
    run(&db, &["occur", "laundry", "-d", "-1", "next_date=+7"]);
    let out = |args: &[&str]| {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        datum_commands::commands::kit_command::output("chorelist", &args, &db).unwrap()
    };
    assert_eq!(out(&[]), "🧹😄 COMPLETED! GOOD WORK! 😄🪣\n");
    assert!(out(&["--all"]).contains("laundry"));
}

/// A row that lost a sync merge (`superseded_by` set) is not live: it moves
/// neither `due` nor `last_occur`, as every built-in view excludes it.
#[test]
fn a_superseded_entry_does_not_count() {
    let db = fresh_db();
    install_chores(&db);
    run(
        &db,
        &[
            "occur",
            "laundry",
            "-d",
            "2020-05-01",
            "-t",
            "10:00",
            "-z",
            "UTC",
            "next_date=2020-05-08",
        ],
    );
    run(
        &db,
        &[
            "occur",
            "laundry",
            "-d",
            "2020-05-10",
            "-t",
            "10:00",
            "-z",
            "UTC",
            "next_date=2020-05-17",
        ],
    );
    db.conn()
        .execute(
            "UPDATE entries SET superseded_by = 'lost-a-merge' WHERE field = 'laundry' AND data LIKE '%2020-05-17%'",
            [],
        )
        .unwrap();
    let all = view(&db, "chores");
    assert_eq!(cell(&all, "laundry", "due"), "2020-05-08");
    assert!(cell(&all, "laundry", "last_occur")
        .as_str()
        .unwrap()
        .starts_with("2020-05-01"));
}

/// A chore never done (only shirked) has no iti; v2 sorted it last (`9e99`),
/// so «most frequent first» holds.
#[test]
fn a_never_done_chore_sorts_last() {
    let db = fresh_db();
    install_chores(&db);
    run(&db, &["occur", "never", "-T", "next_date=2020-01-01"]);
    run(
        &db,
        &[
            "occur",
            "daily",
            "-d",
            "2020-01-01",
            "-t",
            "08:00",
            "next_date=2020-01-02",
        ],
    );
    assert_eq!(fields(&view(&db, "chores")), ["daily", "never"]);
    assert_eq!(cell(&view(&db, "chores"), "never", "iti"), &JsonValue::Null);
}

/// v2 cleared a chore with a null `nextDate`, and a v2-imported null hides
/// it: the latest entry decides, and it says «no next».
#[test]
fn a_v2_imported_null_next_date_hides_the_chore() {
    let db = fresh_db();
    install_chores(&db);
    let mut f = tempfile::NamedTempFile::new().unwrap();
    let docs = serde_json::json!({"docs": [
        {"_id": "gutters:2020-03-01T09:00:00.000Z",
         "data": {"field": "gutters",
                  "occurTime": {"utc": "2020-03-01T09:00:00.000Z", "o": 0, "tz": "UTC"},
                  "nextDate": "2020-09-01"},
         "meta": {"humanId": "g1", "createTime": {"utc": "2020-03-01T09:00:01.000Z", "o": 0, "tz": "UTC"}}},
        {"_id": "gutters:2020-04-01T09:00:00.000Z",
         "data": {"field": "gutters",
                  "occurTime": {"utc": "2020-04-01T09:00:00.000Z", "o": 0, "tz": "UTC"},
                  "nextDate": null},
         "meta": {"humanId": "g2", "createTime": {"utc": "2020-04-01T09:00:01.000Z", "o": 0, "tz": "UTC"}}}
    ]});
    write!(f, "{docs}").unwrap();
    run(&db, &["db", "v2import", f.path().to_str().unwrap()]);
    assert!(
        fields(&view(&db, "chores")).is_empty(),
        "the cleared chore is hidden"
    );
}

/// A typed `next_date=null` is a JSON null too (da-revva), so it retires a
/// chore the same way; the dated chore beside it stays.
#[test]
fn a_typed_null_next_date_retires_the_chore() {
    let db = fresh_db();
    install_chores(&db);
    run(&db, &["occur", "vacuum", "-d", "-2", "next_date=+7"]);
    run(&db, &["occur", "dishes", "-d", "-2", "next_date=+1"]);
    assert_eq!(fields(&view(&db, "chores")), ["dishes", "vacuum"]);
    run(&db, &["occur", "vacuum", "next_date=null"]);
    assert_eq!(fields(&view(&db, "chores")), ["dishes"]);
}

/// A chore that declares its interval (`next_date=+10` in its spec) takes
/// ITI from the spec, not from how far apart its history happens to be.
#[test]
fn a_declared_interval_is_the_iti() {
    let db = fresh_db();
    install_chores(&db);
    run(
        &db,
        &["track", "gutters", "--verb", "occur", "-k", "next_date=+10"],
    );
    run(
        &db,
        &[
            "occur",
            "gutters",
            "-d",
            "2020-01-01",
            "next_date=2020-01-04",
        ],
    );
    assert_eq!(cell(&view(&db, "chores"), "gutters", "iti"), 10.0);
}

/// An unspecced chore keeps its history's interval, beside a specced one.
#[test]
fn an_unspecced_chore_keeps_its_history_iti() {
    let db = fresh_db();
    install_chores(&db);
    run(
        &db,
        &["track", "mop", "--verb", "occur", "-k", "next_date=+9"],
    );
    run(
        &db,
        &["occur", "oven", "-d", "2020-01-01", "next_date=2020-01-04"],
    );
    assert_eq!(cell(&view(&db, "chores"), "oven", "iti"), 3.0);
}

/// Equal intervals list alphabetically, whatever hour each was done at:
/// the old fraction-of-the-day sort would put windows (08:00) first.
#[test]
fn equal_itis_sort_by_name() {
    let db = fresh_db();
    install_chores(&db);
    for (chore, hour) in [("windows", "08:00"), ("dishes", "12:00"), ("bins", "20:00")] {
        run(
            &db,
            &[
                "occur",
                chore,
                "-d",
                "2020-01-01",
                "-t",
                hour,
                "next_date=+7",
            ],
        );
    }
    assert_eq!(fields(&view(&db, "chores")), ["bins", "dishes", "windows"]);
}

/// `newchore x 5` declares the chore: a bare `x` then logs it due five days
/// on (from the day it was done), and `chorelist` shows ITI 5.
#[test]
fn newchore_declares_the_interval() {
    let db = fresh_db();
    install_chores(&db);
    run(&db, &["newchore", "x", "5"]);
    run(&db, &["x", "-d", "2020-01-01"]);
    assert_eq!(cell(&view(&db, "chores"), "x", "due"), "2020-01-06");

    let argv = ["--all", "--show", "json"].map(String::from);
    let out = datum_commands::commands::kit_command::output("chorelist", &argv, &db).unwrap();
    let rows: Vec<JsonValue> = serde_json::from_str(&out).unwrap();
    assert_eq!(rows[0]["iti_days"], 5);
}
