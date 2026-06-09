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

    fn entry_count(&self) -> i64 {
        let conn = Connection::open(&self.db).expect("open db");
        conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
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

/// da-3vvk: declining the bare-`undo` stale-`_LAST` prompt must stop
/// outright ("Aborted."), NOT fall through to the window fallback —
/// which would silently restore a DIFFERENT, newer tombstone than the
/// one the user just said no to. Needs the real binary: the prompt
/// reads stdin, and only a piped "n" can exercise the decline branch
/// (the in-process harnesses' closed stdin reads as the yes-default).
#[test]
fn subprocess_undo_decline_stale_prompt_restores_nothing() {
    let env = Env::new();
    // Two entries, both tombstoned. The pointer will name X (coffee);
    // Y (tea) is the newer tombstone the window fallback would pick.
    assert_ok(&env.run(&["occur", "coffee"]), "create X");
    assert_ok(&env.run(&["occur", "tea", "-t", "+1"]), "create Y");

    let conn = Connection::open(&env.db).expect("open db");
    let x_hid: String = conn
        .query_row("SELECT hid FROM entries WHERE field = 'coffee'", [], |r| {
            r.get(0)
        })
        .expect("X hid");
    assert_ok(&env.run(&["del", "coffee", "--force"]), "delete X");
    assert_ok(&env.run(&["del", "tea", "--force"]), "delete Y");

    // Point `_LAST` at X and age it past the 15-minute protection
    // window so bare `undo` takes the prompt path.
    conn.execute(
        "UPDATE last_docs SET hids = ?1,
             time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-20 minutes')",
        [format!("[\"{x_hid}\"]")],
    )
    .expect("force-stale pointer at X");

    let out = env.run_with_stdin(&["undo"], "n\n");
    assert_ok(&out, "undo decline");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Aborted."),
        "decline must print Aborted.: stdout={stdout:?} stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Nothing restored — both tombstones intact (the window fallback
    // would have brought back Y).
    assert_eq!(env.entry_count(), 0, "decline must restore nothing");
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
