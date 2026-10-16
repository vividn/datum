//! `datum info` and `datum db migrate` integration tests (da-ndd.11).

use crate::common;

use common::run_cli;
use datum_core::migrations::{
    apply_migrations, read_schema_version, set_schema_version, CURRENT_SCHEMA_VERSION, MIGRATIONS,
};
use datum_core::test_util::{entry_builder, fresh_db};

#[test]
fn bare_migrate_is_idempotent_on_current_db() {
    let db = fresh_db();
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION)
    );
    run_cli(&["db", "migrate"], &db).expect("first migrate");
    run_cli(&["db", "migrate"], &db).expect("second migrate is a no-op");
    // Schema version unchanged.
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION)
    );
}

#[test]
fn bare_migrate_rejects_newer_schema_version() {
    let db = fresh_db();
    // Stamp a version the binary cannot handle.
    set_schema_version(db.conn(), CURRENT_SCHEMA_VERSION + 1).unwrap();
    let err = run_cli(&["db", "migrate"], &db).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("newer"),
        "expected 'newer' message, got: {msg}"
    );
}

#[test]
fn info_runs_against_active_db() {
    // A file, named with `--db`: `info` reports the resolved path, and an
    // in-memory db has no file there to size (da-gffn4).
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("active.db");
    let path = path.to_str().unwrap();
    let db = datum_core::db::Db::open(path).unwrap();
    // da-2sr: writing an entry is the only thing needed to make a field
    // tracked. No separate set_tracked call.
    entry_builder("coffee")
        .state(serde_json::json!(2))
        .insert(&db);
    // info should dispatch cleanly.
    run_cli(&["--db", path, "info"], &db).expect("info dispatch");
}

#[test]
fn migrate_status_lists_registered_migrations() {
    // End-to-end: `datum migrate status` dispatches cleanly against a fresh DB,
    // and the core helper it calls reports one status row per registered
    // migration. With MIGRATIONS empty (initial shape is v1), that's zero rows;
    // the shape check still proves the plumbing is wired.
    let db = fresh_db();

    run_cli(&["db", "migrate", "status"], &db).expect("migrate status dispatch");

    let (current, target, rows) =
        datum_core::migrations::migration_status(db.conn(), datum_core::migrations::MIGRATIONS)
            .unwrap();
    assert_eq!(current, CURRENT_SCHEMA_VERSION);
    assert_eq!(target, CURRENT_SCHEMA_VERSION);
    assert_eq!(rows.len(), MIGRATIONS.len());
    for (row, expected) in rows.iter().zip(MIGRATIONS.iter()) {
        assert_eq!(row.from, expected.from);
        assert_eq!(row.to, expected.to);
        assert_eq!(row.summary, expected.summary);
        assert_eq!(row.applied, expected.to <= current);
    }
}

#[test]
fn migrate_run_is_idempotent_on_current_db() {
    // `datum migrate run` against an already-current DB reports no pending
    // migrations and leaves schema_version unchanged.
    let db = fresh_db();
    let before = read_schema_version(db.conn()).unwrap();
    run_cli(&["db", "migrate", "run"], &db).expect("migrate run dispatch");
    let after = read_schema_version(db.conn()).unwrap();
    assert_eq!(before, after);
    assert_eq!(after, Some(CURRENT_SCHEMA_VERSION));
}

#[test]
fn apply_migrations_walks_registered_steps() {
    // End-to-end check that the real MIGRATIONS slice is coherent. We call
    // apply_migrations with (current, target) = (CURRENT, CURRENT) and
    // expect a clean no-op — proves `MIGRATIONS` is consistent with
    // `CURRENT_SCHEMA_VERSION`.
    let db = fresh_db();
    let applied = apply_migrations(
        db.conn(),
        MIGRATIONS,
        CURRENT_SCHEMA_VERSION,
        CURRENT_SCHEMA_VERSION,
    )
    .unwrap();
    assert!(applied.is_empty());
}

/// da-3n9t1: `info` reports on a database without migrating it. One
/// older and one newer than this binary sit in the data directory; `info
/// --all` and `info <path>` must leave both files byte for byte and read
/// each version as it is stored — which is the only way either warning row
/// can ever print. The older one is in rollback-journal mode, as a
/// pre-migration backup is: the journal mode is in the file's header, and
/// a connection that sets WAL rewrites it (da-1duhj).
#[test]
fn info_reports_without_rewriting() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let stamp = |name: &str, version: u32, journal: &str| {
        let path = home.join(name);
        let db = datum_core::db::Db::open(path.to_str().unwrap()).unwrap();
        set_schema_version(db.conn(), version).unwrap();
        db.conn()
            .execute_batch(&format!("PRAGMA journal_mode = {journal}"))
            .unwrap();
        drop(db);
        (path.clone(), std::fs::read(&path).unwrap())
    };
    let (old_path, old_bytes) = stamp("old.db", CURRENT_SCHEMA_VERSION - 1, "DELETE");
    let (new_path, new_bytes) = stamp("new.db", CURRENT_SCHEMA_VERSION + 1, "WAL");
    // Bytes 18 and 19 of the header: 1 is a rollback journal, 2 is WAL.
    assert_eq!(old_bytes[18..20], [1, 1]);
    assert_eq!(new_bytes[18..20], [2, 2]);
    let old_row = format!(
        "v{} ⚠ (older than current v{CURRENT_SCHEMA_VERSION})",
        CURRENT_SCHEMA_VERSION - 1
    );
    let new_row = format!(
        "v{} ✗ (newer than binary (v{CURRENT_SCHEMA_VERSION}))",
        CURRENT_SCHEMA_VERSION + 1
    );

    for (args, rows) in [
        (vec!["info", "--all"], vec![&old_row, &new_row]),
        (vec!["info", old_path.to_str().unwrap()], vec![&old_row]),
        (vec!["info", new_path.to_str().unwrap()], vec![&new_row]),
    ] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env_remove("DATUM_DB")
            .env("DATUM_HOME", &home)
            .env("DATUM_CONFIG_HOME", tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .args(&args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{args:?}: {out:?}");
        for row in rows {
            assert!(stdout.contains(row), "{args:?}: no {row:?} in:\n{stdout}");
        }
        assert!(
            std::fs::read(&old_path).unwrap() == old_bytes,
            "{args:?}: old.db rewritten"
        );
        assert!(
            std::fs::read(&new_path).unwrap() == new_bytes,
            "{args:?}: new.db rewritten"
        );
        let beside: Vec<_> = std::fs::read_dir(&home)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(beside.len(), 2, "{args:?}: files left beside: {beside:?}");
    }
}

/// da-lrn59: `last write` is the newest live row's `modify_time`, read in
/// the host zone — the query named a column that does not exist, so the
/// row printed `(none)` on every database.
#[test]
fn info_last_write_reads_modify_time() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("w.db");
    let info = || {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env_remove("DATUM_DB")
            .env("DATUM_HOME", tmp.path())
            .env("DATUM_CONFIG_HOME", tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .env("TZ", "UTC")
            .args(["info", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let db = datum_core::db::Db::open(path.to_str().unwrap()).unwrap();
    assert!(info().contains("last write: (none)"));

    entry_builder("coffee").insert(&db);
    entry_builder("tea").insert(&db);
    db.conn()
        .execute_batch(
            "UPDATE entries SET modify_time = '2024-03-16T09:15:00+00:00' WHERE field = 'coffee';
             UPDATE entries SET modify_time = '2025-01-01T00:00:00+00:00',
                                deleted_at = '2025-01-01T00:00:00+00:00' WHERE field = 'tea';",
        )
        .unwrap();
    let stdout = info();
    assert!(
        stdout.contains("last write: 2024-03-16 09:15"),
        "no newest live write in:\n{stdout}"
    );
}

/// `datum info <path>` with colour forced `color`: (exit ok, stdout, stderr).
fn info_on(tmp: &std::path::Path, path: &std::path::Path, color: &str) -> (bool, String, String) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
        .env_remove("DATUM_DB")
        .env("DATUM_HOME", tmp)
        .env("DATUM_CONFIG_HOME", tmp.join("config"))
        .env("FORCE_COLOR", color)
        .env_remove("NO_COLOR")
        .env("TZ", "UTC")
        .args(["info", path.to_str().unwrap()])
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// da-1duhj: a file with no `_meta`, and one that is no database, are
/// said to be what they are and left as they are. Reading the version
/// created `_meta` in the first, which a read-only handle refuses.
#[test]
fn info_leaves_a_file_that_is_no_datum_database() {
    let tmp = tempfile::tempdir().unwrap();
    for (bytes, row) in [
        (&b""[..], "schema:   ? ⚠ (unversioned"),
        (
            &[0xa5; 5000][..],
            "schema:   ✗ (Database error: file is not a database)",
        ),
    ] {
        let path = tmp.path().join("x.db");
        std::fs::write(&path, bytes).unwrap();
        let (_, stdout, stderr) = info_on(tmp.path(), &path, "0");
        assert!(stdout.contains(row), "no {row:?} in:\n{stdout}\n{stderr}");
        assert!(std::fs::read(&path).unwrap() == bytes, "{row}: rewritten");
    }
}

/// da-gffn4: a line whose query fails prints `QUERY ERROR` (red when colour
/// is on), never the `0` or `(none)` of an empty database; the reason goes
/// to stderr, the lines that did read still print, and the exit is non-zero.
#[test]
fn info_failed_read_is_query_error() {
    // (what breaks, lines that fail, lines that still read)
    let cases: &[(&str, &[&str], &[&str])] = &[
        (
            "ALTER TABLE entries RENAME COLUMN modify_time TO mt",
            &["last write"],
            &["entries:  1 (1 deleted)", "fields:   1 tracked, 0 ignored"],
        ),
        // (A spec is an entry since da-lc26c, so there is no spec table to
        // break apart from `entries`: the renames below cover `fields`.)
        (
            "ALTER TABLE entries RENAME COLUMN superseded_by TO sb",
            &["entries", "fields"],
            &[
                "entries:  QUERY ERROR (1 deleted)",
                "last write: 2024-03-16 09:15",
            ],
        ),
        (
            "ALTER TABLE entries RENAME COLUMN deleted_at TO da",
            &["entries", "deleted", "fields", "last write"],
            &["entries:  QUERY ERROR (QUERY ERROR deleted)"],
        ),
    ];
    for (sql, failed, read) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("q.db");
        let db = datum_core::db::Db::open(path.to_str().unwrap()).unwrap();
        entry_builder("coffee").insert(&db);
        entry_builder("tea").insert(&db);
        db.conn()
            .execute_batch(
                "UPDATE entries SET modify_time = '2024-03-16T09:15:00+00:00' WHERE field = 'coffee';
                 UPDATE entries SET deleted_at = '2025-01-01T00:00:00+00:00' WHERE field = 'tea';",
            )
            .unwrap();
        db.conn().execute_batch(sql).unwrap();

        let (ok, stdout, stderr) = info_on(tmp.path(), &path, "0");
        assert!(!ok, "{sql}: exit 0 with a failed line:\n{stdout}");
        for line in *failed {
            assert!(
                stderr
                    .lines()
                    .any(|l| l.starts_with(&format!("info: {line}: ")) && l.contains("no such")),
                "{sql}: no reason for {line} in:\n{stderr}"
            );
        }
        let label = |l: &str| format!("{}:", if l == "deleted" { "entries" } else { l });
        for line in ["entries", "fields", "last write"] {
            let printed = stdout
                .lines()
                .find(|l| l.trim_start().starts_with(&label(line)))
                .unwrap_or_else(|| panic!("{sql}: no {line} line in:\n{stdout}"));
            assert_eq!(
                printed.contains("QUERY ERROR"),
                failed.contains(&line) || (line == "entries" && failed.contains(&"deleted")),
                "{sql}: {line} line is {printed:?}"
            );
        }
        for want in *read {
            assert!(stdout.contains(want), "{sql}: no {want:?} in:\n{stdout}");
        }
        assert!(
            stdout.contains("size:"),
            "{sql}: no size line in:\n{stdout}"
        );
        assert!(!stdout.contains("(none)"), "{sql}: `(none)` in:\n{stdout}");

        let (_, stdout, _) = info_on(tmp.path(), &path, "1");
        assert!(
            stdout.contains("\x1b[31mQUERY ERROR\x1b[0m"),
            "{sql}: QUERY ERROR not red in:\n{stdout:?}"
        );
    }
}

/// da-gffn4: an empty database is a read that found nothing — `0` and
/// `(none)`, exit 0 — so it cannot be mistaken for a failed one.
#[test]
fn info_empty_db_reads_zero_and_none() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("e.db");
    drop(datum_core::db::Db::open(path.to_str().unwrap()).unwrap());
    let (ok, stdout, stderr) = info_on(tmp.path(), &path, "0");
    assert!(ok, "{stdout}\n{stderr}");
    assert!(stderr.is_empty(), "stderr: {stderr}");
    for want in [
        "entries:  0 (0 deleted)",
        "fields:   0 tracked, 0 ignored",
        "last write: (none)",
    ] {
        assert!(stdout.contains(want), "no {want:?} in:\n{stdout}");
    }
    assert!(!stdout.contains("QUERY ERROR"), "{stdout}");
}
