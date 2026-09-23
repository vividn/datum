//! Rule 15 at the written entry (da-snlq1): call-site time flags compose
//! with alias-bound ones. The side's argv carries both `-t` tokens, and
//! the entry must come out with their sum — not whichever came last.

use std::path::PathBuf;
use std::process::Command;

use chrono::{DateTime, Utc};
use rusqlite::Connection;

fn run(db: &PathBuf, args: &[&str]) {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("run datum");
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Minutes between the last row's occur_time and the moment it was written.
fn last_offset_minutes(db: &PathBuf) -> i64 {
    let (occur, create): (String, String) = Connection::open(db)
        .unwrap()
        .query_row(
            "SELECT occur_time, create_time FROM entries ORDER BY rowid DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let t = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
    ((t(&occur) - t(&create)).num_seconds() as f64 / 60.0).round() as i64
}

#[test]
fn call_site_time_flags_compose_with_alias_bound_time() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    run(&db, &["alias", "set", "s1", "occur caffeine -t +5"]);
    for (call, want) in [
        (&["s1"][..], 5),
        (&["s1", "-t", "+10"], 15),
        (&["s1", "-t", "-5"], 0),
        (&["s1", "-q"], 0),
    ] {
        run(&db, call);
        assert_eq!(last_offset_minutes(&db), want, "{call:?}");
    }
}
