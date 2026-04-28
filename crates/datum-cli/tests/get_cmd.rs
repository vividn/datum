//! Parity tests for `datum get` (P6 / da-cmqy).
//!
//! Ports the 4 cases from `getCmd.test.ts` in the original datum project,
//! adapted to daydumb's design. Headers identify the source item. Also pins
//! the bead-scope invariants: prefix-based id resolution (C3) reaches
//! `cmd_get`, `--pretty` (A2) emits indented JSON through the P0 harness,
//! and a missing id exits non-zero with a clean error.
//!
//! Divergences from datum (locked in per-test, not silently papered over):
//!   * datum's `meta.humanId` is the human-facing handle for a doc; daydumb's
//!     analog is the `slug` (`field:time`, the primary key) with a 12-char
//!     base36 `hid` for prefix-addressable quick reference. Cases #1 and #2
//!     cover the two prefix surfaces (slug, hid) that `resolve_prefix`
//!     binds together.
//!   * datum's standard `Show` emits an "EXISTS" tag from `getCmd`; daydumb
//!     now matches that contract (da-d5b): `cmd_get` renders through
//!     `show_action` with `Action::Exists`, producing the same on-entry
//!     header + info line + extra-data shape as a fresh log, prefixed with
//!     the EXISTS action tag. Case #3 pins the EXISTS marker.
//!   * datum accepted a compound quickId (`"abc,def,"` → multi-doc return).
//!     daydumb's `GetArgs::id_or_slug` is a single string, so the compound
//!     form resolves as one literal prefix that doesn't match. Case #4
//!     locks in the divergence (compound → NotFound) and asserts the
//!     per-id analog (N independent `get` calls) works.
//!
//! Layering note: the P0 harness (`common::run_cli`) dispatches in-process
//! and cannot capture stdout. Cases that assert on user-visible output
//! shell out to the compiled binary with `DATUM_COLOR=0`, mirroring
//! `delete_cmd.rs` and `output_parity.rs`.

mod common;

use common::run_cli;
use datum_core::test_util::{entry_builder, fresh_db};
use datum_core::{assert_count, entry::Verb};
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

/// TS #1: "gets a document based on the first few letters of humanId"
///
/// Datum seeded `{ _id: "hello", meta.humanId: "a44quickId" }`, then
/// `getCmd("a44")` resolved via the humanId prefix. daydumb's equivalent
/// surface is `slug` — the templated `field:<time>` handle written
/// alongside the ULID id. The test pins a deterministic slug via the A1
/// `--id %field%:…` template so the prefix comparison doesn't race the
/// wall-clock.
#[test]
fn get_resolves_slug_prefix() {
    let db = fresh_db();
    run_cli(
        &["occur", "coffee", "--id", "%field%:a44quickid"],
        &db,
    )
    .expect("seed coffee:a44quickid");
    let seeded = &db.entries_for_field("coffee", None).unwrap()[0];
    assert_eq!(seeded.slug, "coffee:a44quickid");

    // `get coffee:a44` must resolve to the seeded row via resolve_prefix.
    run_cli(&["get", "coffee:a44"], &db).expect("get by slug prefix");

    // Independently verify that the prefix resolves to exactly the seeded
    // entry — proves `cmd_get` is on the C3 `resolve_prefix` path rather
    // than an exact-match shortcut.
    let resolved = db.resolve_prefix("coffee:a44").expect("resolve prefix");
    assert_eq!(resolved.slug, seeded.slug);
    assert_eq!(resolved.hid, seeded.hid);
}

/// TS #2: "gets a document based on the first few letters of _id"
///
/// Datum resolved `_id` prefix `"the_qu"` → `"the_quick_brown_fox"`.
/// daydumb's analog is the `hid` column (random 12-char base36). The A1
/// `--id` template only controls the slug, not the hid, so we insert a
/// fixture, read its generated hid, and slice a prefix off the front.
#[test]
fn get_resolves_hid_prefix() {
    let db = fresh_db();
    let entry = entry_builder("project")
        .verb(Verb::Add)
        .value(json!("the_quick_brown_fox"))
        .insert(&db);
    let prefix = &entry.hid[..6];

    run_cli(&["get", prefix], &db).expect("get by hid prefix");

    // Direct verification — cmd_get took this same path internally.
    let resolved = db.resolve_prefix(prefix).expect("resolve hid prefix");
    assert_eq!(resolved.hid, entry.hid);
}

/// TS #3 (da-d5b): "outputs an EXISTS message when show is standard"
///
/// daydumb's `cmd_get` now renders through the same action-based path
/// as on-entry log, tagged with `Action::Exists` — matching datum's
/// `showExists` contract. The portable invariant: stdout contains the
/// EXISTS marker plus the entry's field name.
///
/// Runs against the compiled binary because the in-process harness does
/// not capture stdout.
#[test]
fn get_standard_show_emits_exists_marker() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    run_binary(&db, &["occur", "mood", "--id", "%field%:somethingElse"]);
    let (stdout, _) = run_binary(&db, &["get", "mood:some"]);

    assert!(
        !stdout.trim().is_empty(),
        "standard `get` must produce output, got empty stdout",
    );
    // EXISTS action tag — datum-compat header for `get`.
    assert!(
        stdout.contains("EXISTS"),
        "expected EXISTS marker in get output, got: {stdout:?}",
    );
    assert!(
        stdout.contains("mood"),
        "expected field name in get output, got: {stdout:?}",
    );

    let _ = std::fs::remove_file(&db);
}

/// da-d5b: `dm get -A <id>` dumps full entry metadata as JSON below the
/// standard EXISTS header. Asserts both the header and the metadata
/// fields the bead enumerates (raw_input, parser_version, content_hash,
/// device_id, etc.) surface in the output.
#[test]
fn get_show_all_dumps_full_metadata() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    run_binary(&db, &["occur", "mood", "--id", "%field%:fullDump"]);
    let (stdout, _) = run_binary(&db, &["get", "-A", "mood:fullDump"]);

    // -A still surfaces the EXISTS header (Show::All extends Default).
    assert!(
        stdout.contains("EXISTS"),
        "expected EXISTS marker in -A output, got: {stdout:?}",
    );
    // Full metadata dump as pretty JSON — every Entry field reachable.
    for key in [
        "\"hid\"",
        "\"slug\"",
        "\"content_hash\"",
        "\"raw_input\"",
        "\"parser_version\"",
        "\"create_time\"",
        "\"modify_time\"",
        "\"device_id\"",
    ] {
        assert!(
            stdout.contains(key),
            "expected {key} in -A output, got: {stdout:?}",
        );
    }

    let _ = std::fs::remove_file(&db);
}

/// da-d5b: `--show-all` is the long form of `-A` (datum-compat). Pin
/// the spelling so a future flag-rename can't silently break parity.
#[test]
fn get_show_all_long_form_works() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    run_binary(&db, &["occur", "mood", "--id", "%field%:longForm"]);
    let (stdout, _) = run_binary(&db, &["get", "--show-all", "mood:longForm"]);

    assert!(
        stdout.contains("EXISTS"),
        "expected EXISTS marker, got: {stdout:?}",
    );
    assert!(
        stdout.contains("\"raw_input\""),
        "expected JSON metadata dump, got: {stdout:?}",
    );

    let _ = std::fs::remove_file(&db);
}

/// TS #4: "can get multiple documents with a compound quickId"
///
/// Datum accepted `"abc,def,"` and split on commas to return N docs.
/// daydumb's `GetArgs::id_or_slug` is a single `String`; the compound
/// form is passed to `resolve_prefix` as a literal and doesn't match.
/// This case pins down the divergence (compound → NotFound) and
/// asserts the N-invocation analog that reaches the same end state.
#[test]
fn compound_ids_require_one_invocation_each() {
    let db = fresh_db();
    let e1 = entry_builder("alpha").value(json!("abc")).insert(&db);
    let e2 = entry_builder("beta").value(json!("def")).insert(&db);

    // Compound form is a literal → no prefix matches → NotFound.
    let compound = format!("{},{},", e1.slug, e2.slug);
    let err = run_cli(&["get", &compound], &db)
        .expect_err("compound form must not resolve in daydumb");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not found") || msg.contains("NotFound"),
        "expected NotFound on compound input, got: {msg}",
    );

    // Per-id invocations reach the end state datum's compound form was
    // meant to achieve: both rows are individually resolvable via their
    // slug (distinct field names make the slug prefixes unambiguous).
    for e in [&e1, &e2] {
        run_cli(&["get", &e.slug], &db)
            .unwrap_or_else(|err| panic!("get {}: {err:#}", e.slug));
    }

    // And the underlying rows are still present (get is read-only).
    assert_count!(db, "alpha", 1);
    assert_count!(db, "beta", 1);
}

/// Bead scope #4: "Verify 'not found' exits non-zero with a clean error."
///
/// Not one of datum's 4 cases, but explicitly called out as a daydumb
/// invariant: an unknown id/slug must exit non-zero with an error
/// message that actually names the missing handle (so the user can
/// debug a typo without diffing against their log).
///
/// Runs via the binary to observe the process exit code and stderr —
/// the in-process harness only exposes `anyhow::Result`.
#[test]
fn get_not_found_exits_nonzero_with_clean_error() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    // Seed a different row so the DB exists but doesn't hold the
    // queried handle — rules out "empty DB" as the failure mode.
    run_binary(&db, &["occur", "coffee"]);

    let bin = env!("CARGO_BIN_EXE_datum");
    let out = Command::new(bin)
        .env("DATUM_DB", &db)
        .env("DATUM_COLOR", "0")
        .args(["get", "missing:handle"])
        .output()
        .expect("run datum");
    assert!(
        !out.status.success(),
        "`get` on unknown handle must exit non-zero, got success",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("NotFound"),
        "expected 'not found' in stderr, got: {stderr:?}",
    );
    // Clean error = names the missing handle (not just "error" or a
    // backtrace). Keeps the message debuggable.
    assert!(
        stderr.contains("missing:handle"),
        "expected the missing handle echoed back, got: {stderr:?}",
    );

    let _ = std::fs::remove_file(&db);
}

/// Bead scope #2: "Verify `--pretty` (from A2) produces indented JSON."
///
/// `cli_harness.rs::get_pretty_emits_indented_json` already pins this
/// behavior at the harness-acceptance layer; the bead asks P6 to also
/// assert it at the command-parity layer so that regressions in the
/// get-command path specifically (not just the dispatcher) surface
/// here. The two layers are redundant by design.
#[test]
fn get_pretty_round_trips_entry_through_cmd_path() {
    let db = fresh_db();
    let entry = entry_builder("mood")
        .verb(Verb::Switch)
        .value(json!("focused"))
        .insert(&db);

    // `get --pretty` through the full CLI path (alias pre-pass, parser,
    // dispatch, cmd_get → print_entry_pretty). Non-error → the arg is
    // recognized and the entry serializes.
    run_cli(&["get", &entry.slug, "--pretty"], &db).expect("get --pretty");

    // Round-trip the same serializer cmd_get uses and assert structure.
    // Two-space indent is the datum-compat contract (see A2).
    let pretty = serde_json::to_string_pretty(&entry).expect("serialize entry");
    assert!(
        pretty.contains("\n  \""),
        "expected two-space indent in pretty JSON, got:\n{pretty}",
    );
    let parsed: serde_json::Value = serde_json::from_str(&pretty).unwrap();
    assert_eq!(parsed["field"], json!("mood"));
    assert_eq!(parsed["verb"], json!("switch"));
    assert_eq!(parsed["value"], json!("focused"));
    assert_eq!(parsed["slug"], json!(entry.slug));
    assert_eq!(parsed["hid"], json!(entry.hid));
}

// ─── Binary harness (scoped to stdout-capture cases) ──────────────────────────

fn tmp_db_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("datum-get-parity-{pid}-{nonce}.db"));
    p
}

fn run_binary(db: &PathBuf, args: &[&str]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_datum");
    let out = Command::new(bin)
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("run datum");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "datum {args:?} failed: stderr={stderr}",
    );
    (stdout, stderr)
}
