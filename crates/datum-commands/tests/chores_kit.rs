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
//! ⚠ The views read SQLite's `date('now', 'localtime')`, which the test
//! clock cannot pin. So the tests either use dates far from today or
//! dates relative to the moment they log, which puts a millisecond window
//! around local midnight.

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
    db.execute_view(&view.sql_text).unwrap()
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
    assert_eq!(cmds, ["chorelist", "chorelist-all"]);
    let empty: Vec<_> = kit.commands.iter().map(|c| c.empty.as_deref()).collect();
    assert_eq!(empty, [Some("🧹😄 COMPLETED! GOOD WORK! 😄🪣"), None]);
}

#[test]
fn install_creates_the_index_and_uninstall_drops_everything() {
    let db = fresh_db();
    install_chores(&db);
    assert!(index_exists(&db, "kit_chores__chore_rows"));
    for name in ["chores", "chores_due"] {
        let v = db.get_view(name).unwrap().expect("view installed");
        for spelling in ["next_time", "nextTime", "next_date", "nextDate"] {
            assert!(v.sql_text.contains(spelling), "{name} reads {spelling}");
        }
    }
    assert!(db.get_kit_command("chorelist").unwrap().is_some());

    run(&db, &["kit", "uninstall", "chores"]);
    assert!(!index_exists(&db, "kit_chores__chore_rows"));
    assert!(db.get_view("chores").unwrap().is_none());
    assert!(db.get_kit_command("chorelist-all").unwrap().is_none());
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

/// v2's inter-time interval: a date's days plus the fraction of the day it
/// was done at, in the entry's own local day (18:00 is three quarters of it).
#[test]
fn iti_counts_days_plus_the_hour_it_was_done() {
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
    assert!((iti - 1.75).abs() < 1e-6, "{iti}");
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

#[test]
fn entries_without_a_next_due_key_are_not_chores() {
    let db = fresh_db();
    install_chores(&db);
    run(&db, &["occur", "coffee"]);
    run(&db, &["switch", "mood", "happy"]);
    assert!(view(&db, "chores").rows.is_empty());
}

#[test]
fn both_commands_render() {
    let db = fresh_db();
    install_chores(&db);
    run(&db, &["occur", "laundry", "next_date=+7"]);
    datum_commands::commands::kit_command::run("chorelist", &[], &db).unwrap();
    datum_commands::commands::kit_command::run("chorelist-all", &[], &db).unwrap();
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
/// it: the latest entry decides, and it says «no next». (v3 cannot write a
/// JSON null yet, so this is the only clear there is — #267.)
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
