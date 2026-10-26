//! Concurrent writers wait, they do not lose writes (da-5pulr).
//!
//! Several `datum` processes writing one database at once must all
//! succeed: `busy_timeout` makes a writer wait for the lock. A DEFERRED
//! transaction that reads before it writes cannot wait — its upgrade to a
//! writer fails at once with SQLITE_BUSY ("database is locked") when
//! another connection committed in between — so every write transaction
//! opens IMMEDIATE (`db::write`, da-8i7f0).

use std::process::Command;

use rusqlite::Connection;

const PROCS: usize = 4;
const WRITES: usize = 10;

#[test]
fn concurrent_writer_processes_lose_no_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("concurrent.db");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let spawn = |args: &[String]| {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &db)
            .env("DATUM_HOME", &home)
            .env("DATUM_CONFIG_HOME", &home)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum")
    };
    // Create and migrate the database first, so what races is the writes.
    assert!(spawn(&["db".into(), "migrate".into()]).status.success());

    let failures: Vec<String> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..PROCS)
            .map(|p| {
                s.spawn(move || {
                    (0..WRITES)
                        .filter_map(|i| {
                            let out = spawn(&[
                                "occur".into(),
                                format!("flake{p}"),
                                format!("n={i}"),
                                "-t".into(),
                                format!("-{}", i + 1),
                            ]);
                            (!out.status.success())
                                .then(|| String::from_utf8_lossy(&out.stderr).into_owned())
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect()
    });
    assert!(
        failures.is_empty(),
        "{} of {} writes failed: {:?}",
        failures.len(),
        PROCS * WRITES,
        failures.first()
    );

    // A write that waited past its turn was queued, not lost (da-9q3fr):
    // the next command lands it, and the count is what landed.
    assert!(spawn(&["tail".into()]).status.success());
    let count: usize = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE field NOT IN ('spec', 'alias')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, PROCS * WRITES);
}

/// A write beside a large sync apply waits a chunk, not the apply
/// (da-2ejbs). The apply held one transaction for its whole batch, so a
/// write landing in it waited out `busy_timeout` and was lost; one that
/// got in had still stalled for seconds. The writes above race each
/// other, briefly; this one races a LONG holder.
#[test]
fn a_write_beside_a_large_sync_apply_waits_a_chunk() {
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::process::Stdio;

    use datum_core::db::{ConflictStrategy, Db};
    use datum_core::entry::{Entry, Verb};
    use datum_core::time::DatumTime;

    const OPS: usize = 2000;

    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("b.db");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let datum = |args: &[&str]| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", &db)
            .env("DATUM_HOME", &home)
            .env("DATUM_CONFIG_HOME", &home)
            .env("NO_COLOR", "1")
            .args(args);
        c
    };
    assert!(datum(&["db", "migrate"]).output().unwrap().status.success());

    // Another copy's history, as a first sync brings it: seconds of apply
    // in one transaction on a debug build. A minute apart, so no two share
    // a slug and every entry is an op.
    let a = Db::open_in_memory().unwrap();
    let base = chrono::Utc::now() - chrono::Duration::days(30);
    let entries: Vec<Entry> = (0..OPS)
        .map(|i| {
            let at = DatumTime {
                utc: base + chrono::Duration::minutes(i as i64),
                offset_minutes: 0,
                timezone: Some("UTC".into()),
                uncertainty: None,
            };
            let state = Some(serde_json::json!(i));
            Entry::new("f", Verb::Point, state, None, Some(at), None, "t")
        })
        .collect();
    a.insert_entries_bulk(&entries, ConflictStrategy::Ignore, &mut |_| {})
        .unwrap();
    let ops = a.sync_ops_since(&BTreeMap::new(), false, None).unwrap();
    assert_eq!(ops.len(), OPS);

    let mut apply = datum(&["sync", "pipe", "apply"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    apply
        .stdin
        .take()
        .unwrap()
        .write_all(ops.join("\n").as_bytes())
        .unwrap();
    let (mut writes, mut failed, mut queued) = (0, Vec::new(), 0);
    // Writes that got in while the apply still ran: one transaction for
    // the batch lets at most the one before it, whatever the box's speed.
    let mut beside = 0;
    while apply.try_wait().unwrap().is_none() {
        let out = datum(&["occur", "busyprobe", &format!("n={writes}")])
            .output()
            .unwrap();
        writes += 1;
        queued += usize::from(String::from_utf8_lossy(&out.stdout).contains("QUEUED"));
        beside += usize::from(apply.try_wait().unwrap().is_none());
        if !out.status.success() {
            failed.push(String::from_utf8_lossy(&out.stderr).into_owned());
        }
    }
    assert!(apply.wait().unwrap().success());
    assert!(
        failed.is_empty(),
        "{} of {writes} writes failed: {:?}",
        failed.len(),
        failed[0]
    );
    // A chunk holds the lock 500 ms, and a write waiting longer than 750 ms
    // spools instead (da-9q3fr), so how long a write took says nothing of
    // the chunks: a fixed cap only ever failed under load (da-d3hte). That
    // it waited a chunk and landed, not queued, does: measured, 0 of 28
    // writes queued, and 28 of 33 with the apply's pause taken out.
    assert!(
        queued * 4 < writes,
        "{queued} of {writes} writes were queued beside the apply"
    );
    assert!(
        beside >= 5,
        "{beside} of {writes} writes got in beside the apply"
    );

    // One that missed every pause was queued (da-9q3fr): the next command
    // lands it, and the count is what landed.
    assert!(datum(&["tail"]).output().unwrap().status.success());
    let n: usize = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE field = 'busyprobe'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, writes);
}

/// A write that waits out the lock names who held it and for how long
/// (da-hyem1): here this test's own process, through datum-core, holding a
/// transaction open while `datum kit install` waits its 5 s. A kit's write,
/// since an entry's no longer fails on the lock but spools (da-9q3fr).
#[test]
fn a_write_timed_out_on_the_lock_names_the_holder() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("held.db");
    let holder = datum_core::db::Db::open(path.to_str().unwrap()).unwrap();
    let out = holder
        .write_with(|_| {
            let mut kit = Command::new(env!("CARGO_BIN_EXE_datum"))
                .env("DATUM_DB", &path)
                .env("DATUM_HOME", tmp.path())
                .env("DATUM_CONFIG_HOME", tmp.path())
                .env("NO_COLOR", "1")
                .args(["kit", "install", "-", "--yes"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            std::io::Write::write_all(
                &mut kit.stdin.take().unwrap(),
                b"[kit]\nname = \"k\"\n[[field]]\nname = \"nap\"\nverb = \"start\"\n",
            )
            .unwrap();
            Ok::<_, datum_core::error::DatumError>(kit.wait_with_output().unwrap())
        })
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{err}");
    let test = std::env::args().next().unwrap();
    let test = std::path::Path::new(&test)
        .file_name()
        .unwrap()
        .to_string_lossy();
    let named = format!("Error: the database is locked by `{test}");
    assert!(err.starts_with(&named), "{err}");
    // It waited out its 5 s, and the holder held for at least as long.
    let pid = format!("(pid {}) for ", std::process::id());
    let held = err.split(&pid).nth(1).and_then(|r| r.split(' ').next());
    let held: f64 = held.and_then(|h| h.parse().ok()).expect(&err);
    assert!(held >= 5.0, "{err}");
}
