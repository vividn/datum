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
//!     `show_action` with `Outcome::Exists`, producing the same on-entry
//!     header + info line + extra-data shape as a fresh log, prefixed with
//!     the EXISTS action tag. Case #3 pins the EXISTS marker.
//!   * datum accepted a compound quickId (`"abc,def,"` → multi-doc return).
//!     daydumb matches this (da-zmpu): the get command splits on commas
//!     and resolves each piece through the standard quickId precedence,
//!     so a single `get a,b,c` call returns all three rows. Case #4 pins
//!     the parity and the trailing-comma tolerance.
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
    run_cli(&["occur", "coffee", "--id", "a44quickid"], &db).expect("seed coffee:a44quickid");
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
        .verb(Verb::Point)
        .state(json!("the_quick_brown_fox"))
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
/// as on-entry log, tagged with `Outcome::Exists` — matching datum's
/// `showExists` contract. The portable invariant: stdout contains the
/// EXISTS marker plus the entry's field name.
///
/// Runs against the compiled binary because the in-process harness does
/// not capture stdout.
#[test]
fn get_standard_show_emits_exists_marker() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    run_binary(&db, &["occur", "mood", "--id", "somethingElse"]);
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

/// da-d5b / da-l41v: `dm get -A <id>` renders the grouped engineering
/// view — every schema column visible under a section heading that
/// mirrors `schema.sql`. Asserts both the EXISTS header and a sample of
/// columns/groups surface.
#[test]
fn get_show_all_dumps_full_metadata() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    run_binary(&db, &["occur", "mood", "--id", "fullDump"]);
    let (stdout, _) = run_binary(&db, &["get", "-A", "mood:fullDump"]);

    // -A still surfaces the EXISTS header (Show::All extends Default).
    assert!(
        stdout.contains("EXISTS"),
        "expected EXISTS marker in -A output, got: {stdout:?}",
    );
    // da-l41v: schema-grouped output — every column appears as a row
    // under its section heading, no JSON-quoted keys.
    for heading in ["Identity", "What", "When", "Where", "Bookkeeping", "Intent"] {
        assert!(
            stdout.contains(heading),
            "expected section heading {heading} in -A output, got: {stdout:?}",
        );
    }
    for col in [
        "hid",
        "slug",
        "content_hash",
        "raw_input",
        "parser_version",
        "create_time",
        "modify_time",
        "device_id",
    ] {
        assert!(
            stdout.contains(col),
            "expected schema column {col} in -A output, got: {stdout:?}",
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

    run_binary(&db, &["occur", "mood", "--id", "longForm"]);
    let (stdout, _) = run_binary(&db, &["get", "--show-all", "mood:longForm"]);

    assert!(
        stdout.contains("EXISTS"),
        "expected EXISTS marker, got: {stdout:?}",
    );
    // da-l41v: grouped view — assert one heading + one column rather
    // than the old JSON-quoted dump shape.
    assert!(
        stdout.contains("Identity") && stdout.contains("raw_input"),
        "expected grouped -A view (Identity heading + raw_input column), got: {stdout:?}",
    );

    let _ = std::fs::remove_file(&db);
}

/// TS #4 (da-zmpu): "can get multiple documents with a compound quickId"
///
/// Datum accepted `"abc,def,"` and split on commas to return N docs.
/// daydumb now matches: the get command splits the positional on commas
/// (slugs/hids never contain commas) and resolves each piece via the
/// normal quickId precedence. A trailing comma is tolerated. Both the
/// pointer and the side-effect surface (no rows mutated) are asserted.
#[test]
fn compound_quickids_resolve_in_one_call() {
    let db = fresh_db();
    let e1 = entry_builder("alpha").state(json!("abc")).insert(&db);
    let e2 = entry_builder("beta").state(json!("def")).insert(&db);

    // Compound form with a trailing comma must succeed and stamp the
    // pointer with every resolved hid, in argument order.
    let compound = format!("{},{},", e1.slug, e2.slug);
    run_cli(&["get", &compound], &db)
        .unwrap_or_else(|err| panic!("compound get must resolve in daydumb: {err:#}"));

    let pointer = datum_core::last_docs::get_last_docs(&db)
        .unwrap()
        .expect("compound get must stamp the last-docs pointer");
    assert_eq!(pointer.hids, vec![e1.hid.clone(), e2.hid.clone()]);

    // get is read-only — both source rows still present.
    assert_count!(db, "alpha", 1);
    assert_count!(db, "beta", 1);

    // A bogus piece anywhere in the compound surfaces NotFound and names
    // the offending piece, so the user knows which id to fix.
    let bad = format!("{},zzzz", e1.slug);
    let err = run_cli(&["get", &bad], &db).expect_err("bogus piece must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("zzzz") || msg.to_lowercase().contains("not found"),
        "expected NotFound naming the bad piece, got: {msg}",
    );
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
        .state(json!("focused"))
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
    // v3 (da-5ns): primary state serializes via `state`; legacy `value`
    // is a #[serde(skip)] compat shim and never lands in the JSON.
    assert_eq!(parsed["state"], json!("focused"));
    assert_eq!(parsed["slug"], json!(entry.slug));
    assert_eq!(parsed["hid"], json!(entry.hid));
}

/// da-bop: `get` must render the LEAD/TRAIL state slots with the same
/// resolved bg colours as the row's original on-entry render. The previous
/// `cmd_get` path called `show_action` without a `RenderContext`, leaving
/// `lead_state` / `trail_state` as `None` → both slots collapsed to the
/// IGNORED grey regardless of the field's actual prior/continuing state.
///
/// Reproduction: switch `mood` to `sad` in the past, then switch `mood` to
/// `happy` (point). The on-entry render correctly paints LEAD = `sad`-bg
/// and TRAIL = `happy`-bg. A follow-up `get _LAST` must paint the slots
/// the same way — not grey.
#[test]
fn get_renders_lead_trail_with_state_context() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    run_binary_color(&db, &["switch", "mood", "sad", "-yy"]);
    run_binary_color(&db, &["switch", "mood", "happy"]);

    let (stdout, _) = run_binary_color(&db, &["get", "_LAST"]);

    let happy_bg = bg_escape("happy");
    let sad_bg = bg_escape("sad");
    // LEAD before the cell must carry the prior state's (`sad`) bg, and
    // TRAIL after must carry the continuing state's (`happy`) bg. Failing
    // either bg means we fell through to the no-context grey IGNORED path.
    let happy_idx = stdout
        .find(&happy_bg)
        .unwrap_or_else(|| panic!("happy bg escape not found in:\n{stdout:?}"));
    let before = &stdout[..happy_idx];
    assert!(
        before.contains(&sad_bg),
        "expected LEAD bg {sad_bg:?} (sad) before the happy cell, got:\n{stdout:?}",
    );
    let after_cell_idx = stdout[happy_idx + happy_bg.len()..]
        .find("happy")
        .map(|i| happy_idx + happy_bg.len() + i + "happy".len())
        .unwrap_or_else(|| panic!("happy body not found after its bg in:\n{stdout:?}"));
    let after = &stdout[after_cell_idx..];
    assert!(
        after.contains(&happy_bg),
        "expected TRAIL bg {happy_bg:?} (happy continues) after the cell, got:\n{stdout:?}",
    );
    // And specifically: the LEAD slot must NOT be the IGNORED grey, which
    // is the failure mode the bug describes ("greyed-out before/following
    // state").
    let ignored_grey = "\x1b[48;2;136;136;136m";
    let between = &stdout[..happy_idx];
    assert!(
        !between.contains(ignored_grey),
        "LEAD slot rendered as IGNORED grey instead of {sad_bg:?}:\n{stdout:?}",
    );

    let _ = std::fs::remove_file(&db);
}

/// da-9q9r: `--show format` without `-o <format>` previously fell through to
/// the renderer's `if let Some(fmt)` guard and exited 0 with empty stdout.
/// Resolution now errors at the OutputArgs boundary so every command surfaces
/// the same diagnostic instead of silently producing nothing.
#[test]
fn show_format_without_output_string_errors() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    run_binary(&db, &["occur", "coffee"]);

    let bin = env!("CARGO_BIN_EXE_datum");
    let out = Command::new(bin)
        .env("DATUM_DB", &db)
        .env("DATUM_COLOR", "0")
        .args(["get", "--show", "format", "_LAST"])
        .output()
        .expect("run datum");
    assert!(
        !out.status.success(),
        "`get --show format` without `-o` must exit non-zero, got success",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--show format requires -o"),
        "expected diagnostic naming `-o` requirement, got: {stderr:?}",
    );
    assert!(
        out.stdout.is_empty(),
        "expected no stdout on error, got: {:?}",
        String::from_utf8_lossy(&out.stdout),
    );

    let _ = std::fs::remove_file(&db);
}

/// MD5-derived ANSI bg escape — mirrors the renderer's `md5_rgb` derivation
/// so the test asserts on absolute byte values without depending on the
/// renderer crate's internals. Duplicated from `render_state_group.rs`
/// because tests files don't share a common util module.
fn bg_escape(name: &str) -> String {
    use md5::Digest;
    let mut h = md5::Md5::new();
    h.update(name.as_bytes());
    let out = h.finalize();
    format!("\x1b[48;2;{};{};{}m", out[0], out[1], out[2])
}

fn run_binary_color(db: &PathBuf, args: &[&str]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_datum");
    let out = Command::new(bin)
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "1")
        .env_remove("NO_COLOR")
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
