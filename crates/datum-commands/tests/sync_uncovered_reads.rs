//! da-8hfta: a sync session finds the entries no op covers (the baselines it
//! signs) without reading every entry. `sync pipe ops --since` and `sync
//! status`, on a chained copy whose every entry is signed, are traced and each
//! statement's plan asked of SQLite: none may scan `entries` or `ops` but
//! through a covering index. The first `sync pipe ops`, which signs a baseline
//! for each entry, finds them through the same set; `datum check sync`, which
//! reads every row, is the control, and must find its scans.

use std::ffi::{c_int, c_uint, c_void, CStr};
use std::sync::Mutex;

use chrono::{Duration, TimeZone, Utc};
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use rusqlite::ffi;

static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());

unsafe extern "C" fn traced(_: c_uint, _: *mut c_void, stmt: *mut c_void, _: *mut c_void) -> c_int {
    let sql = ffi::sqlite3_sql(stmt as *mut ffi::sqlite3_stmt);
    if !sql.is_null() {
        let sql = CStr::from_ptr(sql).to_string_lossy().into_owned();
        RAN.lock().unwrap().push(sql);
    }
    0
}

fn entry(i: usize) -> Entry {
    let at =
        Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap() - Duration::minutes(10 * i as i64);
    Entry::new(
        format!("f{}", i % 6),
        Verb::Switch,
        Some(["on", "off"][(i / 6) % 2].into()),
        None,
        Some(DatumTime::from_utc_and_tz(at, "Europe/Berlin").unwrap()),
        None,
        "bench",
    )
}

fn run(db: &Db, argv: &[&str]) {
    let argv = argv.iter().map(|s| s.to_string()).collect();
    datum_commands::cmd::dispatch_argv(argv, db).unwrap();
}

/// The statements `argv` makes on `db`, each once.
fn statements(db: &Db, argv: &[&str]) -> Vec<String> {
    RAN.lock().unwrap().clear();
    unsafe {
        ffi::sqlite3_trace_v2(
            db.conn().handle(),
            ffi::SQLITE_TRACE_STMT as c_uint,
            Some(traced),
            std::ptr::null_mut(),
        );
    }
    run(db, argv);
    unsafe {
        ffi::sqlite3_trace_v2(db.conn().handle(), 0, None, std::ptr::null_mut());
    }
    let mut ran = std::mem::take(&mut *RAN.lock().unwrap());
    ran.sort();
    ran.dedup();
    ran
}

/// Each statement's plan lines that read `entries` or `ops` whole.
fn whole_reads(db: &Db, ran: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    for sql in ran.iter().filter(|s| !s.trim_start().starts_with("--")) {
        let Ok(mut plan) = db.conn().prepare(&format!("EXPLAIN QUERY PLAN {sql}")) else {
            continue;
        };
        let mut rows = plan.raw_query();
        while let Some(row) = rows.next().unwrap() {
            let line: String = row.get(3).unwrap();
            let table = line.strip_prefix("SCAN ").and_then(|t| t.split(' ').next());
            // `ops_held` is partial, the ops waiting on a gap alone.
            let few = line.ends_with("INDEX ops_held");
            if matches!(table, Some("entries" | "ops")) && !line.contains("COVERING INDEX") && !few
            {
                found.push(format!(
                    "{line}  ←  {}",
                    sql.split_whitespace().collect::<Vec<_>>().join(" ")
                ));
            }
        }
    }
    found
}

fn from_before_ops(db: &Db) {
    db.conn()
        .execute_batch("DELETE FROM ops; DELETE FROM sync_local;")
        .unwrap();
}

fn uncovered(db: &Db) -> i64 {
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE hid NOT IN (SELECT hid FROM ops)",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn a_session_finds_its_baselines_without_reading_every_entry() {
    let db = Db::open_in_memory().unwrap();
    for i in 1..=2_000 {
        db.insert_entry(&entry(i)).unwrap();
    }
    // As a database written before ops: entries, and no op at all.
    from_before_ops(&db);
    assert_eq!(uncovered(&db), 2_000);
    run(&db, &["datum", "sync", "new-invite"]);
    run(&db, &["datum", "sync", "pipe", "ops"]);
    assert_eq!(uncovered(&db), 0, "the first session signs every baseline");

    let vector = serde_json::to_string(&db.sync_vector().unwrap()).unwrap();
    let ops = statements(&db, &["datum", "sync", "pipe", "ops", "--since", &vector]);
    let status = statements(&db, &["datum", "sync", "status", "--json", "--no-grants"]);
    let check = statements(&db, &["datum", "check", "sync", "--show", "none"]);

    assert!(
        whole_reads(&db, &check)
            .iter()
            .any(|l| l.starts_with("SCAN entries")),
        "`check sync` reads every entry, and the reading must see it: {check:#?}"
    );
    for (what, ran) in [("sync pipe ops --since", &ops), ("sync status", &status)] {
        assert!(ran.len() > 3, "{what} was traced: {ran:#?}");
        let scans = whole_reads(&db, ran);
        assert!(scans.is_empty(), "{what} read a table whole: {scans:#?}");
    }
}

/// The hids the kept set names that still have no op, as its readers read it.
fn kept(db: &Db) -> Vec<String> {
    db.conn()
        .prepare(
            "SELECT u.hid FROM sync_uncovered u JOIN entries e ON e.hid = u.hid
              WHERE NOT EXISTS (SELECT 1 FROM ops WHERE ops.hid = u.hid) ORDER BY 1",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The entries no op covers, read from every entry.
fn truth(db: &Db) -> Vec<String> {
    db.conn()
        .prepare("SELECT hid FROM entries WHERE hid NOT IN (SELECT hid FROM ops) ORDER BY 1")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// An import writes its entries with no op (vremya's review: on staging a
/// fresh `db v2import` of the 3-day fixture leaves all of them uncovered), so
/// they reach the set by the insert trigger alone, and the first session signs
/// a baseline for each.
#[test]
fn an_imports_entries_are_in_the_set_and_the_first_session_signs_them() {
    let db = Db::open_in_memory().unwrap();
    let backup = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../datum-cli/tests/fixtures/datum_backup_3days.json"
    );
    run(&db, &["datum", "db", "v2import", backup]);
    let imported = truth(&db);
    assert!(
        imported.len() >= 120,
        "the import made no op: {}",
        imported.len()
    );
    assert_eq!(kept(&db), imported, "the set names every imported entry");

    run(&db, &["datum", "sync", "new-invite"]);
    run(&db, &["datum", "sync", "pipe", "ops"]);
    assert!(
        truth(&db).is_empty(),
        "the first session signs every baseline"
    );
    let baselines: Vec<String> = db
        .conn()
        .prepare("SELECT hid FROM ops WHERE kind = 'baseline' ORDER BY 1")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(baselines, imported, "a baseline for each imported entry");
}

/// Every way an entry comes to have no op, and stops having one, keeps the
/// set to what `datum check sync` finds.
#[test]
fn the_kept_set_is_what_the_entries_say() {
    let db = Db::open_in_memory().unwrap();
    let agree = |db: &Db, what: &str| assert_eq!(kept(db), truth(db), "after {what}");

    for i in 1..=5 {
        db.insert_entry(&entry(i)).unwrap();
    }
    agree(&db, "a write");
    from_before_ops(&db);
    agree(&db, "every op deleted");
    assert_eq!(truth(&db).len(), 5);
    run(&db, &["datum", "sync", "new-invite"]);
    run(&db, &["datum", "sync", "pipe", "ops"]);
    agree(&db, "the baselines");
    assert!(truth(&db).is_empty());

    // A log signs its op.
    run(&db, &["datum", "switch", "mood", "happy"]);
    agree(&db, "a log");
    assert!(truth(&db).is_empty());

    // An entry's ops gone (what `db restore --hard` left, a chain left).
    let hid: String = db
        .conn()
        .query_row("SELECT hid FROM entries ORDER BY hid LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    db.conn()
        .execute("DELETE FROM ops WHERE hid = ?1", [&hid])
        .unwrap();
    agree(&db, "its ops deleted");
    assert_eq!(truth(&db), vec![hid.clone()]);

    // An op taking another hid, as a purge's stub does.
    let other: String = db
        .conn()
        .query_row(
            "SELECT hid FROM entries WHERE hid > ?1 ORDER BY hid LIMIT 1",
            [&hid],
            |r| r.get(0),
        )
        .unwrap();
    db.conn()
        .execute("UPDATE ops SET hid = 'stub' WHERE hid = ?1", [&other])
        .unwrap();
    agree(&db, "an op's hid moved");
    assert_eq!(truth(&db).len(), 2);

    // Deleted outright, it is nothing to sign.
    db.conn()
        .execute("DELETE FROM entries WHERE hid = ?1", [&other])
        .unwrap();
    agree(&db, "the entry deleted");

    // And the next session signs what is left.
    let vector = serde_json::to_string(&db.sync_vector().unwrap()).unwrap();
    run(&db, &["datum", "sync", "pipe", "ops", "--since", &vector]);
    agree(&db, "the next session");
    assert!(truth(&db).is_empty());
}
