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
