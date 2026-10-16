//! da-ovnf9: `-L` / `--ensure-last-state V` is `-l V` as a claim: the write
//! lands only if the field holds V from just before the entry on, checked in
//! the transaction that writes it.

use std::process::Command;

use crate::common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::json;

#[test]
fn a_claim_that_holds_writes_and_records() {
    let db = fresh_db();
    run_cli(&["switch", "issue", "queued", "-t", "-5"], &db).unwrap();
    run_cli(&["switch", "issue", "in_progress", "-L", "queued"], &db).expect("the claim holds");
    let claimed = logged_entries(&db, "issue")
        .into_iter()
        .find(|e| e.state == Some(json!("in_progress")))
        .expect("written");
    assert_eq!(claimed.user_asserted_last_state, Some(json!("queued")));

    // `end` claims the block it closes.
    run_cli(&["start", "work", "-t", "-5"], &db).unwrap();
    run_cli(&["end", "work", "--ensure-last-state", "true"], &db).expect("end -L true");
    assert!(logged_entries(&db, "work")
        .iter()
        .any(|e| e.state == Some(json!(false))));
}

#[test]
fn a_claim_that_fails_writes_nothing_and_names_the_state() {
    let db = fresh_db();
    run_cli(&["switch", "issue", "in_progress", "-t", "-5"], &db).unwrap();
    let before = logged_entries(&db, "issue").len();

    let err = run_cli(&["switch", "issue", "in_progress", "-L", "queued"], &db)
        .expect_err("the field is not in queued");
    assert!(
        format!("{err:#}").contains("issue is in in_progress"),
        "{err:#}"
    );
    // A dry run asks the same question.
    run_cli(&["switch", "issue", "x", "-L", "queued", "--dry-run"], &db)
        .expect_err("a dry run refuses as the run would");
    assert_eq!(
        logged_entries(&db, "issue").len(),
        before,
        "nothing written"
    );

    // `-l` records the same disagreement and writes.
    run_cli(&["switch", "issue", "done", "-l", "queued"], &db).expect("-l records");
    assert_eq!(logged_entries(&db, "issue").len(), before + 1);
}

/// The field must hold V from the entry's time ON: a change logged after a
/// backdated claim refuses it, which is what makes two racing claims made
/// now one claim (each reads its time before it waits for the lock).
#[test]
fn a_claim_refuses_when_the_field_changed_since() {
    let db = fresh_db();
    run_cli(&["switch", "issue", "queued", "-t", "-10"], &db).unwrap();
    run_cli(&["switch", "issue", "in_progress", "-t", "-2"], &db).unwrap();
    let err = run_cli(
        &["switch", "issue", "in_progress", "-L", "queued", "-t", "-5"],
        &db,
    )
    .expect_err("changed at -2");
    assert!(
        format!("{err:#}").contains("issue has changed since, to in_progress"),
        "{err:#}"
    );
}

#[test]
fn of_two_processes_racing_one_claim_exactly_one_lands() {
    const PROCS: usize = 8;
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("claim.db");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &db)
            .env("DATUM_HOME", &home)
            .env("DATUM_CONFIG_HOME", &home)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum")
    };
    assert!(run(&["switch", "issue", "queued", "-t", "-1"])
        .status
        .success());

    let barrier = std::sync::Barrier::new(PROCS);
    let outs: Vec<_> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..PROCS)
            .map(|p| {
                let (run, barrier) = (&run, &barrier);
                s.spawn(move || {
                    let by = format!("by={p}");
                    barrier.wait();
                    run(&["switch", "issue", "in_progress", &by, "-L", "queued"])
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let won = outs.iter().filter(|o| o.status.success()).count();
    let refusals: Vec<_> = outs
        .iter()
        .filter(|o| !o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
        .collect();
    assert_eq!(won, 1, "exactly one claim lands: {refusals:?}");
    for err in &refusals {
        assert!(err.contains("so nothing is written"), "{err}");
    }

    let rows: usize = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE field = 'issue' AND state = '\"in_progress\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1);
}
