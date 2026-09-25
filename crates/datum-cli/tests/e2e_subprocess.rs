//! Subprocess E2E subset (da-1wat criterion 6).
//!
//! Spawns the real `dm` binary via `std::process::Command` to catch
//! process-boundary issues that an in-process call cannot:
//!   * env-var propagation (DATUM_DB, DATUM_HOME, DATUM_CONFIG_HOME)
//!   * stdout/stderr buffering
//!   * exit-code wiring
//!   * fs side-effects (db creation, migration, registry writes)
//!
//! This file is intentionally small (target: 5–10 cases). The bulk of
//! the E2E matrix lives in:
//!   * `datum-commands/tests/e2e.rs` — in-process `dispatch_argv` cases
//!   * `datum-cli/tests/e2e_bin_smoke.rs` — in-process `datum_cli::run` cases
//!
//! Add cases here only when they exercise something the in-process
//! harnesses provably cannot — typically anything that mutates files
//! outside the SQLite DB (kit install, db backup/restore, etc.).

use std::path::PathBuf;
use std::process::{Command, Output};

use rusqlite::Connection;
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    config_home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let db = tmp.path().join("subprocess.db");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        Self {
            _tmp: tmp,
            home,
            config_home,
            db,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        let bin = env!("CARGO_BIN_EXE_datum");
        Command::new(bin)
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    /// Like [`Env::run`] but with `input` piped to the child's stdin —
    /// for interactive prompts ([Y/n]) the in-process harnesses cannot
    /// drive (a closed stdin reads as the yes-default, not a decline).
    fn run_with_stdin(&self, args: &[&str], input: &str) -> Output {
        use std::io::Write;
        use std::process::Stdio;
        let bin = env!("CARGO_BIN_EXE_datum");
        let mut child = Command::new(bin)
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("DATUM_COLOR", "0")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn datum");
        child
            .stdin
            .take()
            .expect("child stdin")
            .write_all(input.as_bytes())
            .expect("write child stdin");
        child.wait_with_output().expect("wait datum")
    }

    /// Live rows a command LOGGED — the TRACK document that declares a
    /// field on its first use (da-c8mh) is not one of them.
    fn entry_count(&self) -> i64 {
        let conn = Connection::open(&self.db).expect("open db");
        conn.query_row(
            "SELECT COUNT(*) FROM entries
             WHERE deleted_at IS NULL
               AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NULL AND field != 'spec'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .expect("count entries")
    }
}

fn assert_ok(out: &Output, label: &str) {
    assert!(
        out.status.success(),
        "[{label}] non-zero exit: code={:?}\n  stdout: {}\n  stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Spawning the binary with `switch mood happy` writes one row through
/// the real process boundary — env vars resolve, db is created,
/// migration runs.
#[test]
fn subprocess_switch_writes_row() {
    let env = Env::new();
    let out = env.run(&["switch", "mood", "happy"]);
    assert_ok(&out, "subprocess_switch_writes_row");
    assert_eq!(env.entry_count(), 1);
}

/// da-9sfm: a content-identical re-log dedups to the existing row and must
/// exit **11** — the distinct, scriptable code datum v2 reserved for
/// "already have this" so idempotent loggers can skip the no-op. The first
/// log exits 0 (CREATE); the second exits 11 (EXISTS) while still rendering
/// the existing entry and writing no new row. Only the real process boundary
/// surfaces the exit code, so this lives here.
#[test]
fn subprocess_duplicate_relog_exits_11() {
    let env = Env::new();

    // Pin the time so both logs produce identical content (same slug + hash).
    let first = env.run(&["occur", "coffee", "-t", "0900"]);
    assert_ok(&first, "first occur");
    assert_eq!(env.entry_count(), 1);

    let second = env.run(&["occur", "coffee", "-t", "0900"]);
    assert_eq!(
        second.status.code(),
        Some(11),
        "re-log must exit 11 (duplicate): stdout={} stderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr),
    );
    // Benign no-op: EXISTS rendered, no second row, no scary "Error:".
    assert!(
        String::from_utf8_lossy(&second.stdout).contains("EXISTS"),
        "duplicate should still render the existing entry"
    );
    assert_eq!(env.entry_count(), 1, "re-log must not write a new row");

    // A subsequent distinct log exits 0 — the flag is per-invocation.
    let third = env.run(&["occur", "tea", "-t", "0900"]);
    assert_ok(&third, "distinct occur after duplicate");
}

/// da-9sfm: the other duplicate flavour — a slug collision with *conflicting*
/// data (`--id` reused, different payload) — is a hard `Duplicate` error. It
/// also exits 11 (v2 `DocExistsError` parity) but prints the error and writes
/// nothing.
#[test]
fn subprocess_conflicting_id_exits_11_with_error() {
    let env = Env::new();
    assert_ok(
        &env.run(&["add", "field", "--id", "doug", "foo=bar"]),
        "first add",
    );

    let collision = env.run(&["add", "field", "--id", "doug", "different=data"]);
    assert_eq!(
        collision.status.code(),
        Some(11),
        "slug collision must exit 11: stdout={} stderr={}",
        String::from_utf8_lossy(&collision.stdout),
        String::from_utf8_lossy(&collision.stderr),
    );
    assert!(
        String::from_utf8_lossy(&collision.stderr).contains("Duplicate"),
        "conflicting-data collision must report the error on stderr"
    );
    assert_eq!(env.entry_count(), 1, "collision must not write a new row");
}

/// A plain failure (missing required positional) keeps the generic exit 1 —
/// the distinct code is reserved for duplicates only.
#[test]
fn subprocess_generic_error_exits_1() {
    let env = Env::new();
    let out = env.run(&["occur"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "generic error must exit 1: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Same path as the bin-smoke `smoke_multi_alias_body` case but through
/// the real binary. Catches process-boundary regressions in the
/// multi-row commit (e.g. the multi-collector relying on thread-local
/// state that doesn't survive the subprocess fork).
#[test]
fn subprocess_multi_alias_body_writes_two_rows() {
    let env = Env::new();

    let out = env.run(&[
        "alias",
        "set",
        "oov",
        "switch environment outside , switch body vertical",
    ]);
    assert_ok(&out, "alias set");

    let out = env.run(&["oov"]);
    assert_ok(&out, "oov dispatch");

    assert_eq!(env.entry_count(), 2);
}

/// `alias set <builtin>` succeeds but warns on stderr that the alias
/// shadows a built-in subcommand (da-vpdz) — same guard as the inline
/// `--alias` path. Stderr capture needs the real process boundary.
#[test]
fn subprocess_alias_set_builtin_name_warns_on_stderr() {
    let env = Env::new();
    let out = env.run(&["alias", "set", "tail", "occur coffee"]);
    assert_ok(&out, "alias set tail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("shadows a built-in subcommand"),
        "expected shadow warning on stderr, got: {stderr}"
    );
}

/// `dm now` is a non-logging admin verb. It must exit 0 and produce
/// some stdout but write no rows.
#[test]
fn subprocess_now_emits_output_without_rows() {
    let env = Env::new();
    let out = env.run(&["now"]);
    assert_ok(&out, "now");
    assert!(
        !out.stdout.is_empty() || !out.stderr.is_empty(),
        "now should emit some output"
    );
    // `dm now` doesn't touch the entries table; the DB may not even
    // be created (no logging verb fired), so entry_count is best-effort
    // — only assert if the file exists.
    if env.db.exists() {
        assert_eq!(env.entry_count(), 0);
    }
}

/// da-fytu: `sync status` must count pending integrity warnings through
/// the canonical `Db::pending_integrity_count` (da-sidz moot-filter) so
/// it agrees with `dm now` after a warning's trigger entry is
/// soft-deleted. Previously it hand-rolled the count without the filter
/// and kept showing the warning forever.
#[test]
fn subprocess_sync_status_count_agrees_with_now_after_delete() {
    let env = Env::new();
    // Two starts a few minutes apart → one pending double_open warning.
    assert_ok(&env.run(&["start", "workout", "-t=-5"]), "first start");
    assert_ok(&env.run(&["start", "workout"]), "second start");

    let sync_status = || {
        // `--server` (any value) enables the per-DB row without needing a
        // token or network — status only reads local state.
        let out = env.run(&["sync", "status", "--server", "http://127.0.0.1:1"]);
        assert_ok(&out, "sync status");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    let before = sync_status();
    assert!(
        before.contains("⚠ 1 integrity warning"),
        "pending warning must surface before deletion: {before:?}"
    );

    // Soft-delete the trigger entry (mirrors the bead repro).
    let slug: String = Connection::open(&env.db)
        .expect("open db")
        .query_row(
            "SELECT entry_slug FROM integrity_warnings WHERE resolution IS NULL",
            [],
            |r| r.get(0),
        )
        .expect("pending warning slug");
    assert_ok(&env.run(&["del", &slug, "--force"]), "delete trigger");

    let after = sync_status();
    assert!(
        after.contains('✓') && !after.contains("integrity warning"),
        "sync status must drop the moot warning after deletion: {after:?}"
    );

    // `dm now` agrees — same canonical count, no warning header.
    let out = env.run(&["now"]);
    assert_ok(&out, "now");
    let now_out = String::from_utf8_lossy(&out.stdout);
    assert!(
        !now_out.contains("integrity warning"),
        "`now` must agree with sync status after deletion: {now_out:?}"
    );
}

/// da-s6no: a stale `_LAST` no longer stops bare `undo`. It used to prompt
/// on a tty (and refuse without one) past fifteen minutes; now it proceeds,
/// says how old the change was on stderr, and exits 0.
///
/// The da-3vvk invariant it inherits: the entry it acts on is the one the
/// POINTER names, never whichever tombstone happens to be newest. Needs the
/// real binary so the pointer can be aged in the file the process reads.
#[test]
fn subprocess_undo_past_the_window_warns_and_acts_on_the_pointer() {
    let env = Env::new();
    // Two entries, both tombstoned. The pointer will name X (coffee);
    // Y (tea) is the newer tombstone a "most recent" fallback would pick.
    assert_ok(&env.run(&["occur", "coffee"]), "create X");
    assert_ok(&env.run(&["occur", "tea", "-t", "+1"]), "create Y");

    let conn = Connection::open(&env.db).expect("open db");
    // By hid: `coffee` now names two rows — the entry and the TRACK
    // document that declared the field (da-c8mh).
    let hid_of = |field: &str| -> String {
        conn.query_row(
            "SELECT hid FROM entries
             WHERE field = ?1
               AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NULL AND field != 'spec'",
            [field],
            |r| r.get(0),
        )
        .expect("hid")
    };
    let x_hid = hid_of("coffee");
    let y_hid = hid_of("tea");
    assert_ok(&env.run(&["del", &x_hid, "--force"]), "delete X");
    assert_ok(&env.run(&["del", &y_hid, "--force"]), "delete Y");

    // Point `_LAST` at X and age it well past the ten-minute warning line.
    conn.execute(
        "UPDATE last_docs SET hids = ?1,
             time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-23 minutes')",
        [format!("[\"{x_hid}\"]")],
    )
    .expect("force-stale pointer at X");

    let out = env.run(&["undo"]);
    assert_ok(&out, "stale undo must proceed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("23 minutes ago"),
        "stale undo must name the age on stderr: {stderr:?}"
    );

    // X is back; Y — the newer tombstone — is untouched.
    assert_eq!(env.entry_count(), 1, "only the pointer's entry comes back");
    let live_field: String = conn
        .query_row(
            "SELECT field FROM entries WHERE deleted_at IS NULL AND field != 'spec'",
            [],
            |r| r.get(0),
        )
        .expect("the restored row");
    assert_eq!(
        live_field, "coffee",
        "undo followed the pointer, not the clock"
    );
}

/// da-8uig: `undo --force` is a no-op flag, not a parse error — it exits 0
/// and warns on the stale pointer exactly as the bare `undo` above does.
/// Needs the real binary for both halves: the exit code and the stderr line.
#[test]
fn subprocess_undo_force_is_inert_and_still_warns() {
    let env = Env::new();
    assert_ok(&env.run(&["occur", "coffee", "cups=2"]), "create");
    assert_ok(&env.run(&["update", "_", "cups=3"]), "update");

    Connection::open(&env.db)
        .expect("open db")
        .execute(
            "UPDATE last_docs
                SET time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-23 minutes')",
            [],
        )
        .expect("force-stale pointer");

    let out = env.run(&["undo", "--force"]);
    assert_ok(&out, "`--force` must be accepted, not a parse error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("23 minutes ago"),
        "`--force` skips nothing, so the stale warning still comes: {stderr:?}"
    );
}

/// da-s6no: inside ten minutes there is nothing to warn about, so bare
/// `undo` says nothing on stderr. The control for the test above.
#[test]
fn subprocess_undo_inside_ten_minutes_is_silent() {
    let env = Env::new();
    assert_ok(&env.run(&["occur", "coffee"]), "create");
    let out = env.run(&["undo"]);
    assert_ok(&out, "fresh undo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("warning:"),
        "a fresh undo must not warn: {stderr:?}"
    );
}

/// da-s6no: `undo` then `redo` through the real binary leaves the entry
/// byte-identical to what it was before the undo, and a second `redo` has
/// nothing ahead of it.
#[test]
fn subprocess_redo_restores_the_undone_state_exactly_once() {
    let env = Env::new();
    assert_ok(&env.run(&["occur", "coffee", "cups=2"]), "create");
    assert_ok(&env.run(&["update", "_", "cups=3"]), "update");

    // By hid. `get coffee` names the entry AND the TRACK document that
    // declared its field (da-c8mh), and a `get` stamps `_LAST` — so a bare
    // `undo` after one would step the whole pair, which is not what this
    // test is about.
    let hid: String = Connection::open(&env.db)
        .expect("open db")
        .query_row(
            "SELECT hid FROM entries
             WHERE field = 'coffee'
               AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NULL AND field != 'spec'",
            [],
            |r| r.get(0),
        )
        .expect("coffee hid");

    let before = env.run(&["get", &hid, "--show", "json"]);
    assert_ok(&before, "get before undo");

    assert_ok(&env.run(&["undo", &hid]), "undo");
    let undone = env.run(&["get", &hid, "--show", "json"]);
    assert_ne!(
        String::from_utf8_lossy(&undone.stdout),
        String::from_utf8_lossy(&before.stdout),
        "the undo must actually move the entry"
    );

    assert_ok(&env.run(&["redo", &hid]), "redo");
    let after = env.run(&["get", &hid, "--show", "json"]);
    assert_eq!(
        String::from_utf8_lossy(&after.stdout),
        String::from_utf8_lossy(&before.stdout),
        "redo must restore the pre-undo state byte for byte"
    );

    let again = env.run(&["redo", &hid]);
    assert_eq!(
        again.status.code(),
        Some(1),
        "a second redo has nothing ahead"
    );
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("Nothing to redo"),
        "the second redo must say so: {:?}",
        String::from_utf8_lossy(&again.stderr)
    );
}

/// da-s6no: `redo` of an undone CREATE brings the entry back under the same
/// hid — the undo tombstoned it, the redo un-tombstones it.
#[test]
fn subprocess_redo_of_an_undone_create_brings_the_entry_back() {
    let env = Env::new();
    assert_ok(&env.run(&["occur", "tea"]), "create");
    let conn = Connection::open(&env.db).expect("open db");
    // tea's OWN row, not the TRACK document that declared the field beside
    // it (da-c8mh) — `undo` takes both back and `redo` brings both again.
    let live = "deleted_at IS NULL
                AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NULL AND field != 'spec'";
    let hid: String = conn
        .query_row(
            &format!("SELECT hid FROM entries WHERE field = 'tea' AND {live}"),
            [],
            |r| r.get(0),
        )
        .expect("tea hid");

    assert_ok(&env.run(&["undo"]), "undo the create");
    assert_eq!(env.entry_count(), 0, "undoing a create tombstones it");

    assert_ok(&env.run(&["redo"]), "redo the create");
    assert_eq!(env.entry_count(), 1, "redo brings it back");
    let back: String = conn
        .query_row(&format!("SELECT hid FROM entries WHERE {live}"), [], |r| {
            r.get(0)
        })
        .expect("the live row");
    assert_eq!(back, hid, "the same hid comes back, not a new entry");
}

/// da-s6no (vividn, 20:19): `delete`'s own fifteen-minute protection goes the
/// same way as undo's. A stale `_LAST` no longer prompts — it deletes, and
/// names the age on stderr. Needs the real binary: the old behaviour was a
/// prompt that read stdin, so only a subprocess proves nothing is asked.
#[test]
fn subprocess_delete_past_the_window_warns_and_proceeds() {
    let env = Env::new();
    assert_ok(&env.run(&["occur", "coffee"]), "create");

    let conn = Connection::open(&env.db).expect("open db");
    conn.execute(
        "UPDATE last_docs SET time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-23 minutes')",
        [],
    )
    .expect("force-stale pointer");

    // No `--force`, and stdin is closed: under the old gate this prompted.
    let out = env.run(&["delete"]);
    assert_ok(&out, "stale delete must proceed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("deleting an entry from 23 minutes ago"),
        "stale delete must name the age on stderr: {stderr:?}"
    );
    assert!(
        !stderr.contains("[Y/n]"),
        "nothing may be asked: {stderr:?}"
    );
    assert_eq!(env.entry_count(), 0, "the entry is tombstoned");
}

/// The control: inside ten minutes `delete` says nothing at all.
#[test]
fn subprocess_delete_inside_ten_minutes_is_silent() {
    let env = Env::new();
    assert_ok(&env.run(&["occur", "coffee"]), "create");
    let out = env.run(&["delete"]);
    assert_ok(&out, "fresh delete");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("warning:"),
        "a fresh delete must not warn: {stderr:?}"
    );
}

/// `dm --version` must print the version banner to stdout and exit 0
/// without touching the DB.
#[test]
fn subprocess_version_flag() {
    let env = Env::new();
    let out = env.run(&["--version"]);
    assert_ok(&out, "--version");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("datum") || stdout.contains("dm"),
        "version output should mention the binary name: {stdout:?}"
    );
}

/// `dm --help` exits 0 with the top-level help text on stdout.
#[test]
fn subprocess_help_flag() {
    let env = Env::new();
    let out = env.run(&["--help"]);
    assert_ok(&out, "--help");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Usage") || stdout.contains("USAGE") || stdout.contains("commands"),
        "help output should describe usage: {stdout:?}"
    );
}

/// da-wdo9: `datum` with no subcommand drops into the REPL, which on a
/// closed stdin prints its banner and exits 0. This must be exercised
/// through the real binary — the parser-layer unit tests
/// (`parse_argv` → `Command::Repl`) pass either way, because the break
/// lived one layer up in `parse_invocation`, where a zero-token tail
/// reached the comma splitter and tripped the empty-side guard.
#[test]
fn subprocess_bare_invocation_enters_repl() {
    let env = Env::new();
    let out = env.run(&[]);
    assert_ok(&out, "bare invocation");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Type `help` for commands"),
        "bare `datum` must print the REPL banner: stdout={stdout:?} stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
}

/// da-wdo9 / da-g6nl: same for `datum --db NAME` — the global flag is
/// consumed and the REPL opens against that DB.
#[test]
fn subprocess_db_flag_without_subcommand_enters_repl() {
    let env = Env::new();
    for args in [&["--db", "test"][..], &["--db=test"][..]] {
        let out = env.run(args);
        assert_ok(&out, "--db without subcommand");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Type `help` for commands"),
            "`datum {args:?}` must print the REPL banner: stdout={stdout:?} stderr={}",
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

/// The counterpart the short-circuit must not weaken: a genuinely empty
/// side (trailing, doubled, or leading standalone `,`) still errors.
#[test]
fn subprocess_empty_comma_sides_still_error() {
    let env = Env::new();
    // Seed one row so the count below reads a real (migrated) db.
    assert_ok(&env.run(&["occur", "water"]), "seed row");
    for args in [
        &["occur", "coffee", ","][..],
        &["occur", "coffee", ",", ",", "occur", "tea"][..],
        &[",", "occur", "tea"][..],
    ] {
        let out = env.run(args);
        assert_eq!(
            out.status.code(),
            Some(1),
            "`datum {args:?}` must still be rejected: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("empty"),
            "rejection must name the empty side: stderr={}",
            String::from_utf8_lossy(&out.stderr),
        );
    }
    assert_eq!(env.entry_count(), 1, "no rejected side may write a row");
}

/// da-ksao: the user-facing half of the pre-migration backup. Migrations
/// are automatic — the first `datum` call after an upgrade reshapes the
/// database with no prompt — so it must say so, and it must say so on
/// **stderr**: a note on stdout would corrupt `datum tail -o … | …` for
/// exactly the one invocation that follows an upgrade.
///
/// Only a subprocess can prove this. The in-process harnesses capture a
/// single merged stream, so they cannot tell stdout from stderr, and the
/// snapshot is a file written beside the db rather than a row inside it.
#[test]
fn subprocess_migration_announces_itself_and_leaves_a_backup() {
    let env = Env::new();
    // A real db with a real row, walked back so the next open must migrate.
    assert_ok(&env.run(&["occur", "coffee"]), "seed row");
    {
        let conn = Connection::open(&env.db).expect("open db");
        datum_core::migrations::migrate_down_to(&conn, 30).expect("walk schema back");
    }

    let out = env.run(&["tail", "-o", "%field%"]);
    assert_ok(&out, "migrating open must still succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    let to = datum_core::migrations::CURRENT_SCHEMA_VERSION;
    assert!(
        stderr.contains(&format!("migrated v30 → v{to}")),
        "the note must name both versions: stderr={stderr}"
    );
    assert!(
        stderr.contains("pre-migration backup saved to"),
        "the note must point at the snapshot: stderr={stderr}"
    );
    assert!(
        !stdout.contains("migrated"),
        "the note must not pollute stdout: stdout={stdout}"
    );
    // stdout is still only the payload, machine-readable across the upgrade.
    // One row: the log declared its own field (da-gvcen).
    assert_eq!(
        stdout.trim(),
        "coffee",
        "stdout carries the data and nothing else"
    );

    // The snapshot exists beside the db and holds the pre-migration shape.
    let backup = stderr
        .lines()
        .find_map(|l| l.strip_prefix("note: pre-migration backup saved to "))
        .expect("the note names the path");
    let snap = Connection::open(backup.trim()).expect("open snapshot");
    assert_eq!(
        datum_core::migrations::peek_schema_version(&snap).unwrap(),
        30,
        "the snapshot is the OLD database, not the migrated one"
    );

    // And the migration itself landed: the row survived, at the new version.
    assert_eq!(env.entry_count(), 1);
    let conn = Connection::open(&env.db).expect("reopen db");
    assert_eq!(
        datum_core::migrations::peek_schema_version(&conn).unwrap(),
        to
    );
}

/// da-ksao, the load-bearing half: if the snapshot cannot be written the
/// migration must not run, and the refusal must reach the user.
///
/// Worth a subprocess case of its own because the pre-pass DB open
/// (`args::parse_invocation`) swallows open failures with `.ok()` to fall
/// back to alias-less parsing. That must not swallow *this* failure into a
/// silent successful run — the dispatcher's open has to surface it as a
/// non-zero exit.
///
/// The failure is forced with a database name long enough that appending
/// the `.pre-v30-to-v33-<stamp>.bak` suffix overruns the 255-byte
/// filesystem limit while `…db`, `…db-wal` and `…db-shm` all still fit: the
/// db opens and is writable, only the backup target is unreachable — the
/// disk-full / read-only shape, without needing either.
#[test]
fn subprocess_unwritable_backup_refuses_to_migrate() {
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let home = tmp.path().join("home");
    let config_home = tmp.path().join("config");
    let db = tmp.path().join(format!("{}.db", "d".repeat(240)));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&config_home).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &db)
            .env("DATUM_HOME", &home)
            .env("DATUM_CONFIG_HOME", &config_home)
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum")
    };

    assert_ok(&run(&["occur", "coffee"]), "seed row");
    {
        let conn = Connection::open(&db).expect("open db");
        datum_core::migrations::migrate_down_to(&conn, 30).expect("walk schema back");
    }

    let out = run(&["tail", "-o", "%field%"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(1),
        "an unbackupable migration must fail loudly, not proceed: stderr={stderr}"
    );
    assert!(
        stderr.contains("refusing to migrate"),
        "the refusal must say why: stderr={stderr}"
    );

    // The database is exactly as it was: old schema, row intact, no
    // half-written snapshot. A refused migration costs the user nothing.
    let conn = Connection::open(&db).expect("reopen db");
    assert_eq!(
        datum_core::migrations::peek_schema_version(&conn).unwrap(),
        30,
        "the schema must not have moved"
    );
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .expect("count entries");
    // One row: the log declared its own field (da-gvcen).
    assert_eq!(rows, 1, "the user's row must survive a refused migration");
}

/// da-c7uw: the whole point of `tail --json` is what a *script* reads off
/// stdout, so the assertion has to happen on the other side of the process
/// boundary — the in-process harness never sees the printed document.
///
/// This is the exact shape that broke: a guard asking "is this field still
/// open?" read the state out of a fixed column, a new column slid in, and
/// it silently started reading the wrong cell. Named fields make that
/// class of break impossible — the guard asks for `state_label` and gets
/// the state no matter how the table is laid out.
#[test]
fn subprocess_tail_json_is_readable_without_counting_columns() {
    let env = Env::new();
    assert_ok(&env.run(&["start", "activity", "-t", "0900"]), "start");
    assert_ok(&env.run(&["end", "activity", "-t", "0930"]), "end");

    let out = env.run(&["tail", "activity", "--json"]);
    assert_ok(&out, "tail --json");
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("tail --json must print parseable JSON");

    let rows = doc["rows"].as_array().expect("rows array");
    // Two logs, two rows: the first declared the field in its own columns
    // (da-gvcen).
    assert_eq!(rows.len(), 2, "both rows: {doc}");
    // Newest last, as the table renders them.
    assert_eq!(rows[0]["state_label"], "start");
    assert_eq!(rows[1]["state_label"], "end");
    assert_eq!(rows[1]["field"], "activity");
    // The hid is the full, prefix-addressable id — a consumer can hand it
    // straight back to the CLI rather than re-deriving one from the table.
    let hid = rows[1]["hid"].as_str().expect("hid string");
    let back = env.run(&["get", hid]);
    assert_ok(&back, "get by the hid tail --json reported");
}

/// `tail --json` carries the table's own columns beside the rows (da-g8o8):
/// one cell per row in row order, so a caller with a grid of its own shows
/// the CLI's cells rather than re-deriving them.
#[test]
fn subprocess_tail_json_carries_the_table_columns() {
    let env = Env::new();
    assert_ok(&env.run(&["occur", "coffee", "-t", "0900"]), "occur");
    assert_ok(
        &env.run(&["switch", "mood", "happy", "-t", "0930"]),
        "switch",
    );

    let out = env.run(&["tail", "--json"]);
    assert_ok(&out, "tail --json");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let columns = doc["columns"].as_array().expect("columns array");
    let column = |key: &str| {
        columns
            .iter()
            .find(|c| c["key"] == key)
            .unwrap_or_else(|| panic!("no {key} column: {doc}"))["cells"]
            .clone()
    };
    // One row per log: each declared its own field in its own columns
    // (da-gvcen), so there is no TRACK document ahead of either.
    assert_eq!(column("field"), serde_json::json!(["coffee", "mood"]));
    // da-ftg8p: and no `action` column, which is a fact about the TABLE
    // and not about what happens to be in it. These columns ARE the
    // table's, so the pane that lays them out drops it with the table.
    assert!(
        columns.iter().all(|c| c["key"] != "action"),
        "no action column: {doc}"
    );
    assert_eq!(column("hid").as_array().unwrap().len(), 2);
    // DATUM_COLOR=0: the cells are plain.
    assert!(!out.stdout.contains(&0x1b), "unpainted under DATUM_COLOR=0");
}

/// `db backup -` is the JSONL document on stdout, and `db restore -` reads
/// one from stdin — the round trip the browser makes with no filesystem.
#[test]
fn subprocess_backup_to_stdout_restores_from_stdin() {
    let from = Env::new();
    assert_ok(&from.run(&["occur", "coffee", "-t", "0900"]), "occur");
    assert_ok(&from.run(&["switch", "mood", "happy"]), "switch");
    let dump = from.run(&["db", "backup", "-"]);
    assert_ok(&dump, "db backup -");
    let text = String::from_utf8(dump.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        5,
        "a header, the two entries, and the spec each one's first log wrote \
         (specs are entries, da-lc26c): {text}"
    );
    assert!(lines[0].contains("daydumb_backup"));

    let to = Env::new();
    assert_ok(
        &to.run_with_stdin(&["db", "restore", "-"], &text),
        "db restore -",
    );
    assert_eq!(to.entry_count(), 2);
    // The specs came back with the entries: a backup carries them now.
    let spec = to.run(&["spec", "show", "mood"]);
    assert_ok(&spec, "spec show mood");
    assert!(
        String::from_utf8_lossy(&spec.stdout).contains(r#""verb": "switch""#),
        "{}",
        String::from_utf8_lossy(&spec.stdout)
    );
}

/// `kit install -` reads the kit from stdin, and `--dry-run` says what it
/// brings and installs nothing — what the browser asks before its own
/// confirm, which it answers with `--yes` (da-axrk).
#[test]
fn subprocess_kit_install_from_stdin_and_dry_run() {
    let env = Env::new();
    let kit = include_str!("fixtures/rest_kit.toml");
    let out = |o: &Output| String::from_utf8_lossy(&o.stdout).into_owned();

    let seen = env.run_with_stdin(&["kit", "install", "-", "--dry-run"], kit);
    assert_ok(&seen, "kit install - --dry-run");
    assert_eq!(
        out(&seen).trim(),
        "Would install kit 'rest': 2 field(s), 1 alias(es), 1 view(s), 1 command(s), 0 layout(s), 0 index/indices"
    );
    assert_eq!(out(&env.run(&["kit", "list"])).trim(), "No kits installed.");

    assert_ok(
        &env.run_with_stdin(&["kit", "install", "-", "--yes"], kit),
        "kit install - --yes",
    );
    let listed = out(&env.run(&["kit", "list"]));
    assert!(listed.contains("rest"), "{listed}");

    // An index still wants its trust answered, and stdin is the kit.
    let chores = include_str!("../../../kits/chores.toml");
    let refused = env.run_with_stdin(&["kit", "install", "-"], chores);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--yes"));

    let bare = env.run(&["kit", "install"]);
    assert!(!bare.status.success());
    assert!(String::from_utf8_lossy(&bare.stderr).contains("<PATH>"));
}

/// A kit's level-2 command runs against the database a leading `--db`
/// names, as every other command does — the web names one on every
/// database but the default (da-1mkej). It used to be "unknown subcommand".
#[test]
fn subprocess_kit_command_honours_a_leading_db() {
    let env = Env::new();
    let kit = include_str!("../../../kits/sample.toml");
    assert_ok(
        &env.run_with_stdin(&["kit", "install", "-"], kit),
        "kit install -",
    );
    let db = env.db.to_string_lossy().into_owned();
    let elsewhere = Env::new();
    for argv in [
        vec!["--db", db.as_str(), "hydration"],
        vec![&format!("--db={db}"), "hydration"],
    ] {
        let seen = elsewhere.run(&argv);
        assert_ok(&seen, &argv.join(" "));
        assert!(String::from_utf8_lossy(&seen.stdout).starts_with("day (UTC)"));
    }
}

/// `fields --json`: every field, logged or declared, with its spec and the
/// states it was logged with.
#[test]
fn subprocess_fields_lists_logged_and_declared_fields() {
    let env = Env::new();
    assert_ok(
        &env.run(&["switch", "mood", "happy", "-t", "0900"]),
        "switch",
    );
    assert_ok(&env.run(&["switch", "mood", "sad", "-t", "0930"]), "switch");
    assert_ok(
        &env.run(&["spec", "set", "sleep", "--verb", "start"]),
        "spec",
    );

    let out = env.run(&["fields", "--json"]);
    assert_ok(&out, "fields --json");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc[0]["field"], "mood");
    // The first `switch` declared the field (da-c8mh), so it has a spec.
    assert_eq!(doc[0]["spec"]["verb"], "switch");
    assert_eq!(doc[0]["states"], serde_json::json!(["happy", "sad"]));
    assert_eq!(doc[1]["field"], "sleep");
    assert_eq!(doc[1]["spec"]["verb"], "start");
}

/// `fields --json`'s `ignored` is the latest track/ignore status and
/// nothing else — not whether a spec exists (da-d08c).
#[test]
fn subprocess_fields_ignored_is_the_latest_status_not_the_spec() {
    let env = Env::new();
    assert_ok(&env.run(&["track", "tea", "--verb", "occur"]), "track");
    assert_ok(&env.run(&["occur", "walk"]), "occur");
    assert_ok(&env.run(&["spec", "set", "run", "--verb", "occur"]), "spec");
    assert_ok(&env.run(&["ignore", "run"]), "ignore");

    let out = env.run(&["fields", "--json"]);
    assert_ok(&out, "fields --json");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows: Vec<_> = doc
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["field"].as_str().unwrap(), r["ignored"].clone()))
        .collect();
    assert_eq!(
        rows,
        [
            ("run", serde_json::json!(true)),
            ("tea", serde_json::json!(false)),
            ("walk", serde_json::json!(false)),
        ]
    );
    assert_eq!(
        doc[0]["states"],
        serde_json::json!([]),
        "IGNORED is not a state"
    );

    assert_ok(&env.run(&["track", "run"]), "track");
    let out = env.run(&["fields", "--json"]);
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc[0]["ignored"], false, "track un-ignores");
}

/// `nowview -o -` is the SVG on stdout; bare `nowview` stays the terminal
/// band (da-57dj). The picture is what a caller with no terminal shows.
#[test]
fn subprocess_nowview_dash_output_is_svg_on_stdout() {
    let env = Env::new();
    assert_ok(&env.run(&["switch", "mood", "happy"]), "switch");
    let svg = env.run(&["nowview", "-o", "-", "--width", "400"]);
    assert_ok(&svg, "nowview -o -");
    let text = String::from_utf8(svg.stdout).unwrap();
    assert!(text.trim_start().starts_with("<svg"), "not an SVG: {text}");
    assert!(text.contains("width=\"400\""), "{text}");

    let band = env.run(&["nowview"]);
    assert_ok(&band, "nowview");
    assert!(!String::from_utf8_lossy(&band.stdout).contains("<svg"));
}

/// da-mjwq, vividn's line through the real process boundary: `start posture
/// start_time=4pm 15` is the block 16:00–16:15, so `tail --json` shows the
/// row `start posture -t 16:15 15` writes — the same in every column but
/// the hid and the write times (occur_time is the block's END; the
/// fifteen minutes precede it).
#[test]
fn subprocess_start_time_key_row_equals_its_t_twin_in_tail_json() {
    let row = |args: &[&str]| -> serde_json::Value {
        let env = Env::new();
        assert_ok(&env.run(args), "log");
        let out = env.run(&["tail", "posture", "--json"]);
        assert_ok(&out, "tail --json");
        let doc: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        // The last row is the entry; ahead of it sits the field's TRACK
        // document (da-c8mh).
        let rows = doc["rows"].as_array().expect("rows");
        let mut row = rows.last().expect("a row").clone();
        for masked in ["hid", "create_time", "modify_time"] {
            row[masked] = serde_json::Value::Null;
        }
        row
    };
    let keyed = row(&["start", "posture", "start_time=4pm", "15"]);
    let flagged = row(&["start", "posture", "-t", "16:15", "15"]);
    assert_eq!(keyed, flagged);
    assert_eq!(keyed["dur"], "PT15M");
    assert!(
        keyed["time"].as_str().unwrap().contains("T16:15:00"),
        "the block ends at 16:15: {}",
        keyed["time"]
    );
    assert_eq!(
        keyed["data"],
        serde_json::Value::Null,
        "the key is consumed"
    );
}

/// da-4k37v: a backup's header names the database the command OPENED —
/// its registry name, else the path it was given — not the default one.
/// A subprocess because the label comes from the same `--db` / `DATUM_DB`
/// resolution the binary's `main` does.
#[test]
fn subprocess_backup_header_names_the_db_it_backed_up() {
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env_remove("DATUM_DB")
            .env("DATUM_HOME", tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", tmp.path().join("config"))
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum");
        assert_ok(&out, &format!("{args:?}"));
        out
    };
    let source_db = |args: &[&str]| -> String {
        let out = run(args);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let header: serde_json::Value =
            serde_json::from_str(stdout.lines().next().expect("a header line"))
                .expect("header is JSON");
        header["source_db"].as_str().expect("source_db").to_string()
    };

    // A named db, registered by its first use.
    run(&["--db", "fitness", "occur", "pushups"]);
    assert_eq!(
        source_db(&["--db", "fitness", "db", "backup", "-"]),
        "fitness"
    );

    // An unregistered file, by path.
    let path = tmp.path().join("loose.db");
    let path = path.to_str().unwrap();
    assert_eq!(source_db(&["--db", path, "db", "backup", "-"]), path);

    // No --db: still the default.
    let default = tmp.path().join("home").join("datum.db");
    assert_eq!(source_db(&["db", "backup", "-"]), default.to_string_lossy());
}
