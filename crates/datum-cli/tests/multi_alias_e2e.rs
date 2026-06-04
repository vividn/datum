//! End-to-end tests for multi-entry (comma-split) invocations against
//! the real `datum` binary. The bug in da-szsl was that the binary's
//! call graph skipped the `MULTI_COMMA` check — tests against the
//! library entry point (`dispatch_argv`) passed, but
//! `datum switch body vertical , switch body sit` failed at runtime
//! because `main.rs` used `parse_with_alias_resolution` + `cmd::dispatch`
//! directly. After unification (single path through
//! `parse_invocation` for both binary and tests), these tests verify
//! the canonical comma-split behavior at the actual binary level.

use std::path::PathBuf;
use std::process::Command;

use rusqlite::Connection;

fn tmp_db() -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("datum-multi-e2e-{pid}-{nonce}.db"));
    p
}

fn run(db: &PathBuf, args: &[&str]) -> (bool, String, String) {
    let bin = env!("CARGO_BIN_EXE_datum");
    let out = Command::new(bin)
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("run datum");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (out.status.success(), stdout, stderr)
}

fn live_rows(db: &PathBuf) -> Vec<(String, String, Option<String>, Option<String>)> {
    let conn = Connection::open(db).expect("open db");
    let mut stmt = conn
        .prepare(
            "SELECT field, occur_time, state, data \
             FROM entries WHERE deleted_at IS NULL ORDER BY rowid",
        )
        .expect("prepare");
    stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })
    .expect("query")
    .map(|r| r.unwrap())
    .collect()
}

/// Acceptance #1 from da-szsl: end-to-end binary path. Two `switch
/// body` sides should collapse to one row whose state is the JSON
/// array `["vertical", "sit"]` via same-field merge (da-gh0.C).
///
/// Pre-fix this failed with the parser rejecting the bare `,` token
/// at switch's positional slot: "trailing positional ',' is not a
/// valid duration". The binary skipped the multi-comma check
/// entirely.
#[test]
fn binary_same_field_two_switches_merges_to_parallel_state() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    let (ok, _stdout, stderr) = run(
        &db,
        &["switch", "body", "vertical", ",", "switch", "body", "sit"],
    );
    assert!(
        ok,
        "binary must succeed on comma-split argv; stderr={stderr}"
    );

    let rows = live_rows(&db);
    assert_eq!(
        rows.len(),
        1,
        "same-field merge yields one row, got {} rows: {rows:?}",
        rows.len()
    );
    let state_json: serde_json::Value =
        serde_json::from_str(rows[0].2.as_ref().expect("state set")).unwrap();
    assert_eq!(state_json, serde_json::json!(["vertical", "sit"]));
}

/// Acceptance #2: `dm occur foo , occur bar` writes two rows with the
/// same `occur_time` and distinct fields.
#[test]
fn binary_two_occurs_different_fields_yields_two_rows() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    let (ok, _stdout, stderr) = run(&db, &["occur", "foo", ",", "occur", "bar"]);
    assert!(ok, "binary must succeed; stderr={stderr}");

    let rows = live_rows(&db);
    assert_eq!(rows.len(), 2, "expected two rows, got {rows:?}");
    assert_eq!(rows[0].1, rows[1].1, "shared occur_time");
    let fields: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert!(fields.contains(&"foo") && fields.contains(&"bar"));
}

/// Acceptance #3: declared multi-alias `oov = switch body vertical ,
/// switch body sit`. Invoking `dm oov` collapses via same-field merge
/// to the parallel-state row.
#[test]
fn binary_declared_multi_alias_expands_to_two_sides() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    // Register the alias first.
    let (ok, _, stderr) = run(
        &db,
        &[
            "alias",
            "set",
            "oov",
            "switch body vertical , switch body sit",
        ],
    );
    assert!(ok, "alias set should succeed; stderr={stderr}");

    // Invoke it.
    let (ok, _, stderr) = run(&db, &["oov"]);
    assert!(ok, "alias invocation should succeed; stderr={stderr}");

    let rows = live_rows(&db);
    assert_eq!(rows.len(), 1, "same-field merge → one row, got {rows:?}");
    let state_json: serde_json::Value =
        serde_json::from_str(rows[0].2.as_ref().expect("state set")).unwrap();
    assert_eq!(state_json, serde_json::json!(["vertical", "sit"]));
}

/// Acceptance #5 (negative): `dm switch body vertical,sit` (no
/// whitespace around the comma) is a single token, not a multi-split
/// trigger. Writes one row whose state is the literal string
/// `vertical,sit` — except `switch` arrays comma-separated states
/// into `["vertical", "sit"]` at S12 (da-u4k.12). Either way, this
/// must be a single-row write (no second row).
#[test]
fn binary_glued_comma_does_not_trigger_multi() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    let (ok, _, stderr) = run(&db, &["switch", "body", "vertical,sit"]);
    assert!(ok, "binary should succeed on glued comma; stderr={stderr}");

    let rows = live_rows(&db);
    assert_eq!(
        rows.len(),
        1,
        "glued comma is one shell token, not a multi-trigger; got {rows:?}"
    );
}

/// Acceptance #6 (regression): existing single-row scenarios still
/// route through the standard path without engaging the multi
/// collector.
#[test]
fn binary_single_row_switch_unchanged() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    let (ok, _, stderr) = run(&db, &["switch", "body", "vertical"]);
    assert!(ok, "single-row switch should succeed; stderr={stderr}");

    let rows = live_rows(&db);
    assert_eq!(rows.len(), 1);
    let state_json: serde_json::Value =
        serde_json::from_str(rows[0].2.as_ref().expect("state set")).unwrap();
    assert_eq!(state_json, serde_json::json!("vertical"));
}

/// Sanity: a parse error on a side should propagate without writing
/// anything. Leading empty side is the canonical Section M case —
/// the splitter errors before any dispatch runs.
#[test]
fn binary_leading_empty_side_errors_and_writes_nothing() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    let (ok, _, _stderr) = run(&db, &[",", "switch", "body", "vertical"]);
    assert!(!ok, "leading-empty-side argv should fail");

    // DB may not even exist yet — if it does, no rows.
    if db.exists() {
        let rows = live_rows(&db);
        assert!(rows.is_empty(), "rollback leaves no rows: {rows:?}");
    }
}

/// da-wie4: per-side `--show` flags must attach to their own row, not a
/// neighbor's. Pins the render-pairing invariant — the per-side
/// `OutputArgs` bucketing (`leftmost_output_per_group`) now uses core's
/// canonical `merge_group_key`, so the silenced side stays silent and
/// the default side renders, regardless of side order.
#[test]
fn binary_per_side_show_flags_attach_to_own_row() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    // Side 1 (body) silenced; side 2 (environment) renders by default.
    let (ok, stdout, stderr) = run(
        &db,
        &[
            "switch",
            "body",
            "vertical",
            "--show",
            "none",
            ",",
            "switch",
            "environment",
            "outside",
        ],
    );
    assert!(
        ok,
        "multi with per-side --show must succeed; stderr={stderr}"
    );
    assert!(
        stdout.contains("environment:"),
        "default side must render its CREATE header: {stdout:?}"
    );
    assert!(
        !stdout.contains("body:"),
        "--show none side must stay silent: {stdout:?}"
    );

    // Mirror image: silence follows the flag, not the position.
    let db2 = tmp_db();
    let _ = std::fs::remove_file(&db2);
    let (ok, stdout, stderr) = run(
        &db2,
        &[
            "switch",
            "body",
            "vertical",
            ",",
            "switch",
            "environment",
            "outside",
            "--show",
            "none",
        ],
    );
    assert!(ok, "mirror invocation must succeed; stderr={stderr}");
    assert!(
        stdout.contains("body:"),
        "default side must render its CREATE header: {stdout:?}"
    );
    assert!(
        !stdout.contains("environment:"),
        "--show none side must stay silent: {stdout:?}"
    );
}

/// da-wie4 companion: when two sides merge into one row (same field +
/// occur_time), the *leftmost* contributing side's flags drive the
/// merged row's render — `--show none` on the left silences the whole
/// merged row even though the right side asked for `--show all`.
///
/// Both sides carry a `--show` flag deliberately: with the flag on only
/// one side, `side_has_dur_signal` (datum-parse alias/expand.rs) counts
/// the flag's value as a bare positional and mis-collapses the comma
/// into a BlockSwitch "then"-chain (filed as da-k8fq);
/// symmetric flags keep the multi-entry split this test is pinning.
#[test]
fn binary_same_field_merge_leftmost_show_flag_wins() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    let (ok, stdout, stderr) = run(
        &db,
        &[
            "switch", "body", "vertical", "--show", "none", ",", "switch", "body", "sit", "--show",
            "all",
        ],
    );
    assert!(
        ok,
        "merge with leftmost --show none must succeed; stderr={stderr}"
    );
    assert_eq!(live_rows(&db).len(), 1, "same-field merge yields one row");
    assert!(
        stdout.trim().is_empty(),
        "leftmost side's --show none must silence the merged row: {stdout:?}"
    );
}
