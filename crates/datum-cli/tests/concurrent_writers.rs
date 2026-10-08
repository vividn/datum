//! Concurrent writers wait, they do not lose writes (da-5pulr).
//!
//! Several `datum` processes writing one database at once must all
//! succeed: `busy_timeout` makes a writer wait for the lock. A DEFERRED
//! transaction that reads before it writes cannot wait — its upgrade to a
//! writer fails at once with SQLITE_BUSY ("database is locked") when
//! another connection committed in between — so every write transaction
//! opens IMMEDIATE (`Db::configure_connection`).

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
    assert!(spawn(&["tail".into()]).status.success());

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
