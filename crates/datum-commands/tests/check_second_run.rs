//! da-3zq6g: a second `datum check` reads what the first found and what
//! changed since, and no history-sized table whole. Every statement the run
//! makes is traced and its plan asked of SQLite: none may scan `entries`,
//! `ops` or `stats_cache` except through a covering index, which reads no row.
//! The first run, which walks every entry, is the control: the same reading
//! must find its scans.

use std::ffi::{c_int, c_uint, c_void, CStr};
use std::sync::Mutex;

use chrono::{DateTime, Duration, TimeZone, Utc};
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use rusqlite::ffi;

/// The statements the traced run made, as written.
static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());

unsafe extern "C" fn traced(_: c_uint, _: *mut c_void, stmt: *mut c_void, _: *mut c_void) -> c_int {
    let sql = ffi::sqlite3_sql(stmt as *mut ffi::sqlite3_stmt);
    if !sql.is_null() {
        let sql = CStr::from_ptr(sql).to_string_lossy().into_owned();
        RAN.lock().unwrap().push(sql);
    }
    0
}

fn at(i: usize) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap() - Duration::minutes(10 * i as i64)
}

fn zoned(i: usize, zone: &str) -> DatumTime {
    DatumTime::from_utc_and_tz(at(i), zone).unwrap()
}

/// `n` rows over a few fields, all in one zone but for a two-row spike, a
/// state repeated and a flag: findings for `tz`, `integrity` and `flags`
/// to keep, and no error, which would end the process.
fn store(n: usize) -> Db {
    let db = Db::open_in_memory().unwrap();
    for i in 1..=n {
        let zone = match i {
            1000 | 1001 => "America/New_York",
            _ => "Europe/Berlin",
        };
        let state = ["on", "off"][(i / 6) % 2];
        let mut e = Entry::new(
            format!("f{}", i % 6),
            Verb::Switch,
            Some(state.into()),
            None,
            Some(zoned(i, zone)),
            None,
            "bench",
        );
        if i == 500 {
            e.extra_metadata = Some(serde_json::json!({ "flag": true }));
        }
        db.insert_entry(&e).unwrap();
    }
    db
}

/// The statements `datum check` makes on `db`.
fn traced_check(db: &Db) -> Vec<String> {
    RAN.lock().unwrap().clear();
    unsafe {
        let handle = db.conn().handle();
        ffi::sqlite3_trace_v2(
            handle,
            ffi::SQLITE_TRACE_STMT as c_uint,
            Some(traced),
            std::ptr::null_mut(),
        );
    }
    let argv = ["datum", "check", "--show", "none"]
        .map(String::from)
        .to_vec();
    datum_commands::cmd::dispatch_argv(argv, db).unwrap();
    unsafe {
        ffi::sqlite3_trace_v2(db.conn().handle(), 0, None, std::ptr::null_mut());
    }
    let mut ran = std::mem::take(&mut *RAN.lock().unwrap());
    ran.sort();
    ran.dedup();
    ran
}

/// The tables a run may read whole: as long as the device's settings, its
/// peers and notices, its fields, its aliases, never as the history. And
/// what is no table: a CTE, a subquery's rows, a JSON array.
const SMALL: &[&str] = &[
    "sqlite_master",
    // One row per AUTOINCREMENT table: the change log's mark.
    "sqlite_sequence",
    "sync_local",
    "sync_notices",
    "sync_peers",
    "stats_cache_dirty",
    "stats_cache_through",
    "spec_alias_names",
    "f",
    "t",
    "held",
    "CONSTANT",
];

/// The partial indices a run may read whole: each holds only what is wrong
/// or rare — the conflicted rows, the template specs, the pending warnings,
/// the rows a `+=` read a base for.
const FEW: &[&str] = &[
    "entries_conflicts",
    "entries_spec_templates",
    "integrity_warnings_pending",
    "entries_asserted_base",
];

/// Each statement's plan lines that read a table whole, but a [`SMALL`] one.
fn whole_reads(db: &Db, ran: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    for sql in ran.iter().filter(|s| !s.trim_start().starts_with("--")) {
        let Ok(mut plan) = db.conn().prepare(&format!("EXPLAIN QUERY PLAN {sql}")) else {
            continue;
        };
        // Unbound: a plan does not wait on the values.
        let mut rows = plan.raw_query();
        let mut lines: Vec<String> = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            lines.push(row.get(3).unwrap());
        }
        for line in lines {
            let Some(table) = line.strip_prefix("SCAN ").and_then(|t| t.split(' ').next()) else {
                continue;
            };
            let few = FEW.iter().any(|i| line.ends_with(&format!("INDEX {i}")));
            let no_table = line.contains("VIRTUAL TABLE") || table.starts_with('(');
            if !SMALL.contains(&table) && !few && !no_table && !line.contains("COVERING INDEX") {
                found.push(format!(
                    "{line}  ←  {}",
                    sql.split_whitespace().collect::<Vec<_>>().join(" ")
                ));
            }
        }
    }
    found
}

#[test]
fn a_second_check_reads_no_table_whole() {
    let db = store(3_000);
    let first = traced_check(&db);
    // A write between the runs, which the second reads again.
    let late = Entry::new(
        "f1",
        Verb::Switch,
        Some("on".into()),
        None,
        Some(zoned(0, "Europe/Berlin")),
        None,
        "bench",
    );
    db.insert_entry(&late).unwrap();
    let second = traced_check(&db);

    let control = whole_reads(&db, &first);
    assert!(
        control.iter().any(|l| l.starts_with("SCAN entries ")),
        "the first run walks every entry, and the reading must see it: {control:#?}"
    );
    assert!(second.len() > 20, "the run was traced: {second:#?}");
    let scans = whole_reads(&db, &second);
    assert!(
        scans.is_empty(),
        "a second run read a table whole: {scans:#?}"
    );
}
