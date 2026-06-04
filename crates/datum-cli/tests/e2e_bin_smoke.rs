//! Bin-smoke gate for the E2E harness (da-1wat).
//!
//! Drives a curated subset of cases through `datum_cli::run` — the
//! exact entry point `main()` uses — and asserts equivalence with the
//! corresponding `dispatch_argv`-routed cases in
//! `datum-commands/tests/e2e.rs`. The bulk of the matrix lives there;
//! this file's job is to confirm `main.rs`'s path does not silently
//! diverge from the dispatch path the rest of the suite exercises.
//!
//! Each smoke case:
//!   1. Provisions a fresh `DATUM_HOME` under a tmpdir (so the user's
//!      real DB is never touched).
//!   2. Invokes `datum_cli::run(argv)` — same function `main()` calls.
//!   3. Opens the resolved DB and asserts the expected entry row(s)
//!      landed.
//!
//! ## Why "equivalence with the bulk cohort"
//!
//! Smoke cases here re-encode argv lines that also appear in the bulk
//! cohort (`crates/datum-commands/tests/e2e.rs`). If `main.rs` adds a
//! pre-pass step or routes around the unified `dispatch_argv`, these
//! cases will diverge before the bulk cohort can warn — that is the
//! "future drift" gate the bead asks for.
//!
//! ## Why so few cases
//!
//! 5–10 cases is the deliberate ceiling. The bulk cohort already
//! validates behavior at the `dispatch_argv` layer. The smoke gate
//! exists only to ensure that layer *is* the binary's layer. Adding
//! more cases here without a corresponding `e2e.rs` row would create
//! a parallel path — the exact failure mode the harness exists to
//! prevent.

use std::path::PathBuf;

use anyhow::Result;
use datum_core::db::Db;
use serde_json::{json, Value as JsonValue};
use tempfile::TempDir;

// `datum_cli::run` consults `DATUM_HOME` / `DATUM_DB` via process-wide
// env vars. Integration tests run in parallel by default, so we
// serialize on the shared process-wide lock (da-3ywd) to keep one
// case's env from racing another's.
use datum_test_util::env_lock;

struct BinHarness {
    _home: TempDir,
    db_path: PathBuf,
}

impl BinHarness {
    fn new() -> Self {
        let home = tempfile::tempdir().expect("create DATUM_HOME tmpdir");
        // Point `DATUM_DB` at an explicit file path inside the tmpdir.
        // `looks_like_path()` returns true for anything with a `/`, so
        // resolve_db treats this as a direct path (no registry lookup,
        // no DATUM_HOME/data_dir indirection — keeps the smoke test
        // hermetic regardless of the runner's user-level env).
        let db_path = home.path().join("smoke.db");
        // The whole helper is gated by env_lock so no other thread
        // mutates these vars concurrently.
        std::env::set_var("DATUM_HOME", home.path());
        std::env::set_var("DATUM_DB", &db_path);
        Self {
            _home: home,
            db_path,
        }
    }

    fn run(&self, args: &[&str]) -> Result<()> {
        let argv: Vec<String> = std::iter::once("datum".to_string())
            .chain(args.iter().map(|s| s.to_string()))
            .collect();
        datum_cli::run(argv)
    }

    fn db(&self) -> Db {
        Db::open(&self.db_path.to_string_lossy()).expect("open bin-smoke DB")
    }
}

#[derive(Debug)]
struct Row {
    field: String,
    state_json: Option<String>,
    dur: Option<String>,
}

fn live_rows(db: &Db) -> Vec<Row> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT field, state, dur FROM entries \
             WHERE deleted_at IS NULL ORDER BY rowid",
        )
        .expect("prepare live_rows");
    stmt.query_map([], |r| {
        Ok(Row {
            field: r.get::<_, String>(0)?,
            state_json: r.get::<_, Option<String>>(1)?,
            dur: r.get::<_, Option<String>>(2)?,
        })
    })
    .expect("query")
    .map(|r| r.unwrap())
    .collect()
}

fn state(row: &Row) -> Option<JsonValue> {
    row.state_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
}

// ─── Cases ───────────────────────────────────────────────────────────
//
// Each test mirrors a case in datum-commands/tests/e2e.rs. The
// docstring names the sibling so divergence is easy to spot.

/// Mirrors `verb_switch_direct` in `datum-commands/tests/e2e.rs`.
#[test]
fn smoke_switch_direct() {
    let _g = env_lock();
    let h = BinHarness::new();
    h.run(&["switch", "mood", "happy"]).expect("dispatch");
    let rows = live_rows(&h.db());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].field, "mood");
    assert_eq!(state(&rows[0]), Some(json!("happy")));
}

/// Mirrors `verb_occur_direct` in `datum-commands/tests/e2e.rs`.
#[test]
fn smoke_occur_direct() {
    let _g = env_lock();
    let h = BinHarness::new();
    h.run(&["occur", "coffee"]).expect("dispatch");
    let rows = live_rows(&h.db());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].field, "coffee");
    assert_eq!(rows[0].dur.as_deref(), Some("MOMENT"));
}

/// Field-first form: mirrors `verb_switch_field_first`.
#[test]
fn smoke_field_first_swap() {
    let _g = env_lock();
    let h = BinHarness::new();
    h.run(&["mood", "switch", "happy"]).expect("dispatch");
    let rows = live_rows(&h.db());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].field, "mood");
    assert_eq!(state(&rows[0]), Some(json!("happy")));
}

/// Multi-direct via top-level comma: mirrors `multi_direct_two_fields`.
/// This is the case the original wiring bug missed — `main.rs` must
/// route through the multi-aware `parse_invocation` path, not a
/// single-side fallback.
#[test]
fn smoke_multi_direct_two_fields() {
    let _g = env_lock();
    let h = BinHarness::new();
    h.run(&[
        "switch",
        "environment",
        "outside",
        ",",
        "switch",
        "body",
        "vertical",
    ])
    .expect("dispatch");
    let rows = live_rows(&h.db());
    assert_eq!(rows.len(), 2, "comma-trigger multi must produce 2 rows");
    let fields: Vec<&str> = rows.iter().map(|r| r.field.as_str()).collect();
    assert!(fields.contains(&"environment"));
    assert!(fields.contains(&"body"));
}

/// Same-field multi merges to a parallel-state array: mirrors
/// `multi_same_field_parallel_array`.
#[test]
fn smoke_multi_same_field_parallel() {
    let _g = env_lock();
    let h = BinHarness::new();
    h.run(&["switch", "body", "vertical", ",", "switch", "body", "sit"])
        .expect("dispatch");
    let rows = live_rows(&h.db());
    assert_eq!(rows.len(), 1, "same-field merge yields a single row");
    assert_eq!(state(&rows[0]), Some(json!(["vertical", "sit"])));
}

/// Bare-alias single-row: mirrors `verb_switch_bare_alias_single`.
/// The alias must be installed in the DB *before* the dispatch — the
/// pre-pass reads it from `aliases` to expand `bo`.
#[test]
fn smoke_bare_alias() {
    let _g = env_lock();
    let h = BinHarness::new();
    // Install the alias first.
    h.run(&["alias", "set", "bo", "switch body"])
        .expect("alias install");
    h.run(&["bo", "vertical"]).expect("alias dispatch");
    let rows = live_rows(&h.db());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].field, "body");
    assert_eq!(state(&rows[0]), Some(json!("vertical")));
}

/// Multi-alias body: mirrors `multi_alias_body_oov`. This is the
/// equivalence gate that would have caught the original bug — the
/// alias body contains the comma trigger, and `main.rs` must drive
/// through the multi-aware path so the comma fires.
#[test]
fn smoke_multi_alias_body() {
    let _g = env_lock();
    let h = BinHarness::new();
    h.run(&[
        "alias",
        "set",
        "oov",
        "switch environment outside , switch body vertical",
    ])
    .expect("alias install");
    h.run(&["oov"]).expect("alias dispatch");
    let rows = live_rows(&h.db());
    assert_eq!(rows.len(), 2, "multi-alias body must produce 2 rows");
    let fields: Vec<&str> = rows.iter().map(|r| r.field.as_str()).collect();
    assert!(fields.contains(&"environment"));
    assert!(fields.contains(&"body"));
}

/// Admin smoke: `now` does not write entries but must dispatch
/// successfully. Confirms non-logging verbs survive the bin-entry path.
#[test]
fn smoke_admin_now() {
    let _g = env_lock();
    let h = BinHarness::new();
    h.run(&["now"]).expect("dispatch");
    let rows = live_rows(&h.db());
    assert!(rows.is_empty(), "admin verb must not write rows");
}
