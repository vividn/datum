//! Parity tests for `datum delete` (P7 / da-0gjw).
//!
//! Ports the 6 cases from `deleteCmd.test.ts` in the original datum project,
//! adapted to daydumb's design. Each Rust test maps 1-to-1 to a TS case;
//! headers identify the source item.
//!
//! Divergences from datum (locked in per-test, not silently papered over):
//!   * datum resolved targets via quickId prefix matching (`"a44q"` → humanId
//!     `a44quickId`); daydumb's `delete` takes an **exact** id or slug. Cases
//!     #1 and #2 port the "resolve by prefix" semantics as exact id/slug
//!     lookups — the observable analogs.
//!   * datum spied on internal `quickId` / `deleteDoc` calls (case #4).
//!     daydumb has no equivalent wrapper; the portable invariant is that the
//!     `del` alias (A2) dispatches to the same handler as `delete`, so case
//!     #4 doubles as the acceptance criterion `del_alias_dispatches_to_delete`.
//!   * datum's compound quickId (`"abc,def,ghi,"` → 3 deletes) is now
//!     supported (S3): case #6 asserts the compound form tombstones all
//!     three targeted rows in a single invocation, with the trailing empty
//!     segment silently dropped.
//!
//! Daydumb-specific invariants asserted alongside the ports (required by
//! the bead scope):
//!   * soft-delete sets `deleted_at`, the row is excluded from list queries
//!     but still reachable via `get_entry_including_deleted` (A4 tombstone).
//!   * `--force` is always passed in tests — the non-force path reads from
//!     stdin and would block indefinitely under cargo test.

mod common;

use common::run_cli;
use datum_core::test_util::{advance_time, entry_builder, fresh_db, set_time};
use datum_core::{assert_count, entry::Verb};
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

/// TS #1: "deletes a document based on first few letters of humanId"
///
/// Datum: seeded `_id=hello, meta.humanId=a44quickId`, then `deleteCmd("a44q")`
/// resolved via quickId prefix → deleted `hello`. Daydumb has no quickId prefix
/// matching in the delete path (`get_entry` is exact on `id OR slug`), so the
/// portable analog is an exact-id lookup. The core invariants being locked in
/// (tombstone, list-exclusion, ghost reachable) transfer directly.
#[test]
fn delete_by_exact_id_tombstones_entry() {
    let db = fresh_db();
    let entry = entry_builder("coffee").state(json!(1)).insert(&db);

    run_cli(&["delete", &entry.slug, "--force"], &db).expect("delete --force");

    // Live read paths hide the tombstoned row.
    assert_count!(db, "coffee", 0);
    assert!(
        matches!(
            db.get_entry(&entry.slug),
            Err(datum_core::error::DatumError::NotFound(_)),
        ),
        "live get_entry must return NotFound after tombstone",
    );

    // But the row still exists with `deleted_at` set — soft-delete, not hard.
    let ghost = db
        .get_entry_including_deleted(&entry.slug)
        .expect("tombstoned row still reachable via including_deleted");
    assert!(
        ghost.deleted_at.is_some(),
        "deleted_at must be populated on the tombstone",
    );
    assert_eq!(ghost.slug, entry.slug);
}

/// TS #2: "deletes a document based on first few letters of _id"
///
/// Datum: id prefix `the_qu` resolved to `the_quick_brown_fox`. Daydumb's
/// exact-match alternative is the slug (`field:<templated>` / `field:<time>`)
/// — the second observable handle for an entry alongside its ULID. The case
/// exercises the slug path end-to-end and asserts tombstone semantics.
#[test]
fn delete_by_slug_tombstones_entry() {
    let db = fresh_db();

    // `--id fixed` pins a deterministic slug body so we can reference it
    // without depending on wall-clock time. This exercises the template
    // interpolation path from A1, which the slug-lookup case relies on.
    // da-ivld: id_structure is body-only; `<field>:` prefix is implicit.
    run_cli(&["occur", "event", "--id", "fixed"], &db).expect("seed event");
    let seeded = &db.entries_for_field("event", None).unwrap()[0];
    let slug = seeded.slug.clone();
    assert_eq!(slug, "event:fixed", "slug interpolation must resolve");

    run_cli(&["delete", &slug, "--force"], &db).expect("delete by slug");

    assert_count!(db, "event", 0);
    assert!(
        matches!(
            db.get_entry(&slug),
            Err(datum_core::error::DatumError::NotFound(_)),
        ),
        "live get_entry by slug must return NotFound",
    );
    let ghost = db.get_entry_including_deleted(&slug).unwrap();
    assert!(ghost.deleted_at.is_some(), "tombstone must be set");
}

/// da-95ez: a tombstoned entry no longer holds its slug, so re-`occur`-ing at
/// the same slug after a delete succeeds instead of failing with
/// `Duplicate entry: Entry already exists with slug …`. This is the exact flow
/// that bit the backfill (delete-then-recreate at the same minute). The
/// tombstoned row is revived in place — one row per slug, sync-safe.
#[test]
fn reoccur_at_same_slug_after_delete_succeeds() {
    let db = fresh_db();

    // `--id pin` forces a fixed slug so both occurs target `coffee:pin`.
    run_cli(&["occur", "coffee", "--id", "pin"], &db).expect("first occur");
    run_cli(&["delete", "coffee:pin", "--force"], &db).expect("delete the entry");

    // Re-`occur` at the very same slug — previously rejected by the slug
    // UNIQUE that the tombstone still held.
    run_cli(&["occur", "coffee", "--id", "pin"], &db)
        .unwrap_or_else(|err| panic!("re-occur at freed slug must succeed: {err:#}"));

    // Exactly one live entry holds the slug, and it is not a tombstone.
    assert_count!(db, "coffee", 1);
    let live = db
        .get_entry("coffee:pin")
        .expect("live entry at reused slug");
    assert!(
        live.deleted_at.is_none(),
        "the reused slug must resolve to a live row, not the ghost",
    );
}

/// TS #3: "only deletes the one document"
///
/// Seed two rows, delete one by id, verify the other survives. Distinct
/// `value`s avoid content_hash collision (daydumb dedups on content_hash
/// under the default Ignore strategy, which would otherwise turn the second
/// insert into `Err(Duplicate)` if the rows were fixture-identical).
#[test]
fn delete_targets_one_row_among_many() {
    let db = fresh_db();
    let doomed = entry_builder("coffee").state(json!(1)).insert(&db);
    let keeper = entry_builder("coffee").state(json!(2)).insert(&db);

    run_cli(&["delete", &doomed.slug, "--force"], &db).expect("delete doomed");

    // Only the targeted row is tombstoned; the sibling survives intact.
    assert_count!(db, "coffee", 1);
    let surviving = &db.entries_for_field("coffee", None).unwrap()[0];
    assert_eq!(surviving.slug, keeper.slug, "non-targeted row must survive");
    assert!(
        db.get_entry(&keeper.slug).is_ok(),
        "keeper remains directly fetchable",
    );
    assert!(
        matches!(
            db.get_entry(&doomed.slug),
            Err(datum_core::error::DatumError::NotFound(_)),
        ),
        "doomed row hidden from live reads",
    );
}

/// TS #4: "calls quickId and deleteDoc" → adapted to
/// `del_alias_dispatches_to_delete` (bead acceptance criterion).
///
/// Datum asserted the internal wiring (`quickId` + `deleteDoc` called per
/// invocation). Daydumb has no direct analog — the portable invariant is
/// that `del` (the visible alias declared in `args.rs`) and `delete` route
/// to the same handler and produce the same end state. Asserts two rows
/// tombstoned, one via each spelling, with identical observable outcomes.
#[test]
fn del_alias_dispatches_to_delete_handler() {
    let db = fresh_db();
    // Distinct values so content_hash differs (see case #3 rationale).
    let via_delete = entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("via-delete"))
        .insert(&db);
    let via_del = entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("via-del"))
        .insert(&db);

    run_cli(&["delete", &via_delete.slug, "--force"], &db).expect("canonical delete");
    run_cli(&["del", &via_del.slug, "--force"], &db).expect("del alias");

    // Both rows are tombstoned — same handler, same effect.
    assert_count!(db, "mood", 0);
    for id in [&via_delete.slug, &via_del.slug] {
        let ghost = db.get_entry_including_deleted(id).unwrap();
        assert!(
            ghost.deleted_at.is_some(),
            "both `delete` and `del` must produce tombstones (missing on {id})",
        );
    }
}

/// TS #5: "outputs a DELETED message when show is standard"
///
/// Datum asserted `console.log` contained "DELETE". Daydumb routes the same
/// action tag through `show_delete` → `show_action` (`Outcome::Delete.name()`
/// = "DELETE"). The in-process harness doesn't capture stdout, so this case
/// follows `output_parity.rs` and shells out to the compiled binary with
/// `DATUM_COLOR=0` for portable assertions.
///
/// Also locks in `--show none` silencing — proof the "DELETE" string above
/// comes from the output pipeline, not a stray `println!`.
#[test]
fn delete_emits_delete_tag_on_standard_show() {
    let db = tmp_db_path();
    let _ = std::fs::remove_file(&db);

    // Pinned slug so the subsequent delete can reference it deterministically.
    // da-ivld: id_structure is body-only; `<field>:` prefix is implicit.
    run_binary(&db, &["occur", "coffee", "--id", "pin"]);
    let (stdout, _) = run_binary(&db, &["delete", "coffee:pin", "--force"]);
    assert!(
        stdout.contains("DELETE"),
        "expected DELETE action tag on standard show, got: {stdout:?}",
    );

    // Second seed + delete with `--show none`: no tag, no noise.
    run_binary(&db, &["occur", "coffee", "--id", "pin2"]);
    let (silent, _) = run_binary(&db, &["delete", "coffee:pin2", "--force", "--show", "none"]);
    assert_eq!(
        silent, "",
        "--show none must silence the DELETE tag, got: {silent:?}",
    );

    let _ = std::fs::remove_file(&db);
}

/// TS #6: "can delete multiple documents using a compound quickid" (S3).
///
/// Datum accepted `"abc,def,ghi,"` and split on commas to issue N deletes.
/// Under S3 daydumb matches that surface: a comma-separated list of
/// ids/slugs/wildcards tombstones every resolved entry in one invocation.
/// Trailing empty segments (from the trailing comma) are silently dropped.
#[test]
fn compound_ids_batch_delete_in_one_invocation() {
    let db = fresh_db();
    let e1 = entry_builder("coffee").state(json!(1)).insert(&db);
    let e2 = entry_builder("coffee").state(json!(2)).insert(&db);
    let e3 = entry_builder("coffee").state(json!(3)).insert(&db);

    let compound = format!("{},{},{},", e1.slug, e2.slug, e3.slug);
    run_cli(&["delete", &compound, "--force"], &db)
        .unwrap_or_else(|err| panic!("compound delete must succeed: {err:#}"));

    assert_count!(db, "coffee", 0);
    for e in [&e1, &e2, &e3] {
        assert!(
            db.get_entry_including_deleted(&e.slug)
                .unwrap()
                .deleted_at
                .is_some(),
            "each row must carry a tombstone after compound delete",
        );
    }
}

// ─── S3: batch mutation cases ────────────────────────────────────────────────

/// Wildcard tokens expand to every live match in the batch path.
#[test]
fn compound_delete_expands_wildcards() {
    let db = fresh_db();
    let a = entry_builder("mood").state(json!("a")).insert(&db);
    let b = entry_builder("mood").state(json!("b")).insert(&db);
    let c = entry_builder("mood").state(json!("c")).insert(&db);
    let untouched = entry_builder("coffee").state(json!(1)).insert(&db);

    // `mood:*` hits all three mood rows, `coffee:*` not listed → preserved.
    run_cli(&["delete", "mood:*", "--force"], &db)
        .unwrap_or_else(|err| panic!("wildcard delete must succeed: {err:#}"));

    assert_count!(db, "mood", 0);
    assert_count!(db, "coffee", 1);
    for e in [&a, &b, &c] {
        assert!(
            db.get_entry_including_deleted(&e.slug)
                .unwrap()
                .deleted_at
                .is_some(),
            "mood row {} must be tombstoned after wildcard delete",
            e.slug,
        );
    }
    assert!(
        db.get_entry(&untouched.slug).is_ok(),
        "non-matching row survives"
    );
}

/// A comma-separated list mixing an exact slug, a hid prefix, and a wildcard
/// all resolve through the same pipeline.
///
/// Five entries are inserted with explicit, monotonically-stepped timestamps
/// via the deterministic test clock (da-5ss). The original
/// [`entry_builder`] default times depended on `DatumTime::now()` plus a
/// per-process millisecond nudge, which could let two slugs collide when
/// the suite ran under heavy load (root cause of da-bex). Freezing the
/// clock per-entry makes slug values reproducible and the test
/// non-flaky by construction.
#[test]
fn compound_delete_mixes_hid_slug_and_wildcard() {
    let db = fresh_db();
    let _g = set_time("2026-01-15T12:00:00Z");
    let by_slug = entry_builder("mood").state(json!("s")).insert(&db);
    advance_time("1m");
    let by_hid = entry_builder("mood").state(json!("h")).insert(&db);
    advance_time("1m");
    let in_wild = entry_builder("book").state(json!("the-hobbit")).insert(&db);
    advance_time("1m");
    let in_wild2 = entry_builder("book").state(json!("the-iliad")).insert(&db);
    advance_time("1m");
    let outside = entry_builder("coffee").state(json!(1)).insert(&db);

    let mixed = format!("{},{},book:*", by_slug.slug, by_hid.hid);
    run_cli(&["delete", &mixed, "--force"], &db)
        .unwrap_or_else(|err| panic!("mixed delete must succeed: {err:#}"));

    // Four deleted, one untouched.
    for e in [&by_slug, &by_hid, &in_wild, &in_wild2] {
        assert!(
            db.get_entry_including_deleted(&e.slug)
                .unwrap()
                .deleted_at
                .is_some(),
            "row {} must be tombstoned",
            e.slug,
        );
    }
    assert!(
        db.get_entry(&outside.slug).is_ok(),
        "coffee row untouched by the mixed batch",
    );
}

/// Duplicates across tokens (same slug reached twice) collapse to a single
/// delete — the second attempt should not double-fault the batch.
#[test]
fn compound_delete_collapses_duplicates() {
    let db = fresh_db();
    let e = entry_builder("mood").state(json!(1)).insert(&db);

    // Reference the same row twice: once by slug, once by hid prefix.
    let hid5: String = e.hid.chars().take(5).collect();
    let dup = format!("{},{}", e.slug, hid5);

    run_cli(&["delete", &dup, "--force"], &db)
        .unwrap_or_else(|err| panic!("duplicate-collapsed delete must succeed: {err:#}"));

    assert!(
        db.get_entry_including_deleted(&e.slug)
            .unwrap()
            .deleted_at
            .is_some(),
        "row must be tombstoned after duplicated batch",
    );
}

/// Default (non-strict) continues on per-token failure: the valid token
/// still tombstones its entry, and the overall invocation exits non-zero.
#[test]
fn compound_delete_continues_past_bad_token_by_default() {
    let db = fresh_db();
    let keep = entry_builder("coffee").state(json!(1)).insert(&db);
    let doomed = entry_builder("coffee").state(json!(2)).insert(&db);

    let compound = format!("{},definitely-not-a-real-id", doomed.slug);
    let err = run_cli(&["delete", &compound, "--force"], &db)
        .expect_err("batch with a bad token must report a non-zero error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("definitely-not-a-real-id"),
        "error must mention the unresolved token, got: {msg}",
    );

    // The good token still tombstoned its entry.
    assert!(
        db.get_entry_including_deleted(&doomed.slug)
            .unwrap()
            .deleted_at
            .is_some(),
        "valid id in the batch must still be tombstoned",
    );
    // The unrelated row was untouched.
    assert!(db.get_entry(&keep.slug).is_ok(), "sibling row untouched");
}

/// `--strict` aborts the batch on the first resolution error before touching
/// anything — no partial mutation.
#[test]
fn strict_compound_delete_aborts_without_mutating() {
    let db = fresh_db();
    let a = entry_builder("coffee").state(json!(1)).insert(&db);
    let b = entry_builder("coffee").state(json!(2)).insert(&db);

    let compound = format!("not-a-real-id,{},{}", a.slug, b.slug);
    let err = run_cli(&["delete", &compound, "--force", "--strict"], &db)
        .expect_err("strict must surface the resolution error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not-a-real-id"),
        "strict error must name the bad token, got: {msg}",
    );

    // Neither row was tombstoned — strict aborted before the mutation loop.
    assert_count!(db, "coffee", 2);
}

/// `--dry-run` reports what would be deleted without mutating the DB.
#[test]
fn compound_delete_dry_run_does_not_mutate() {
    let db = fresh_db();
    let a = entry_builder("mood").state(json!(1)).insert(&db);
    let b = entry_builder("mood").state(json!(2)).insert(&db);

    run_cli(&["delete", "mood:*", "--force", "--dry-run"], &db)
        .unwrap_or_else(|err| panic!("dry-run must succeed: {err:#}"));

    // Both rows still live.
    assert_count!(db, "mood", 2);
    for e in [&a, &b] {
        let row = db.get_entry(&e.slug).unwrap();
        assert!(row.deleted_at.is_none(), "dry-run must not tombstone");
    }
}

// ─── Binary harness (scoped to stdout-capture cases) ──────────────────────────

fn tmp_db_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("datum-delete-parity-{pid}-{nonce}.db"));
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
