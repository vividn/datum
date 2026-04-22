//! S3 parity tests for batch update/merge/rekey mutations.
//!
//! Mirrors the delete-side cases but exercises the update/merge/rekey
//! dispatch paths that share the unified `cmd_update` handler after S11.
//! Each case locks in one observable behavior of the batch path:
//!   * Compound slug lists mutate every matched row.
//!   * Wildcards expand through the batch resolver.
//!   * `--dry-run` leaves the DB untouched.
//!   * `--strict` aborts on first error; default continues.
//!   * A batched rekey applies its template per-row.

mod common;

use common::run_cli;
use datum_core::test_util::{entry_builder, fresh_db, spec_builder};
use datum_core::{assert_count, entry::Verb};
use serde_json::json;

/// Compound ids applied to `update` mutate each row with the same payload.
#[test]
fn compound_update_applies_payload_to_each_row() {
    let db = fresh_db();
    let a = entry_builder("gym")
        .value(json!(1))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    let b = entry_builder("gym")
        .value(json!(2))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    let untouched = entry_builder("coffee").value(json!(1)).insert(&db);

    let compound = format!("{},{}", a.slug, b.slug);
    run_cli(&["update", &compound, "reps=20"], &db)
        .unwrap_or_else(|err| panic!("compound update must succeed: {err:#}"));

    for slug in [&a.slug, &b.slug] {
        let row = db.get_entry(slug).unwrap();
        assert_eq!(
            row.data.as_ref().unwrap()["reps"],
            json!(20),
            "reps must be bumped to 20 for {slug}",
        );
    }
    // Untouched row has its original payload.
    let sibling = db.get_entry(&untouched.slug).unwrap();
    assert!(sibling.data.is_none() || sibling.data.as_ref().unwrap().get("reps").is_none());
}

/// Wildcards expand through `merge`, and deep-merge semantics apply per-row.
#[test]
fn compound_merge_expands_wildcards_and_deep_merges() {
    let db = fresh_db();
    // Distinct slugs via different start times so wildcards can find both.
    let a = entry_builder("notes")
        .verb(Verb::Add)
        .extra_data(json!({"nested": {"a": 1}}))
        .insert(&db);
    let b = entry_builder("notes")
        .verb(Verb::Add)
        .extra_data(json!({"nested": {"b": 2}}))
        .value(json!("x"))
        .insert(&db);

    run_cli(&["merge", "notes:*", "tag=red"], &db)
        .unwrap_or_else(|err| panic!("wildcard merge must succeed: {err:#}"));

    for (slug, preserved) in [(&a.slug, "a"), (&b.slug, "b")] {
        let row = db.get_entry(slug).unwrap();
        let ex = row.data.as_ref().unwrap();
        assert_eq!(ex["tag"], json!("red"), "tag applied to {slug}");
        assert!(
            ex["nested"].get(preserved).is_some(),
            "deep-merge preserved `nested.{preserved}` on {slug}",
        );
    }
}

/// `--dry-run` on a batch update leaves the DB untouched.
#[test]
fn compound_update_dry_run_does_not_mutate() {
    let db = fresh_db();
    let a = entry_builder("gym")
        .value(json!(1))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    let b = entry_builder("gym")
        .value(json!(2))
        .extra_data(json!({"reps": 10}))
        .insert(&db);

    let compound = format!("{},{}", a.slug, b.slug);
    run_cli(&["update", &compound, "reps=20", "--dry-run"], &db)
        .unwrap_or_else(|err| panic!("dry-run must succeed: {err:#}"));

    for slug in [&a.slug, &b.slug] {
        let row = db.get_entry(slug).unwrap();
        assert_eq!(
            row.data.as_ref().unwrap()["reps"],
            json!(10),
            "reps must be unchanged on dry-run for {slug}",
        );
    }
}

/// Default (non-strict) behavior: a tombstoned row errors but the batch
/// still mutates the live siblings.
#[test]
fn compound_update_continues_past_tombstoned_entry() {
    let db = fresh_db();
    let live = entry_builder("gym")
        .value(json!(1))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    let dead = entry_builder("gym")
        .value(json!(2))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    db.soft_delete_entry(&dead.slug).unwrap();

    let compound = format!("{},{}", dead.slug, live.slug);
    let err = run_cli(&["update", &compound, "reps=20"], &db)
        .expect_err("tombstoned element must surface a non-zero error");
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("tombstoned"),
        "error must name the tombstoned entry, got: {msg}",
    );

    // Live row still got updated.
    let fresh = db.get_entry(&live.slug).unwrap();
    assert_eq!(fresh.data.as_ref().unwrap()["reps"], json!(20));
}

/// `--strict` aborts the batch on the first tombstoned match — the live
/// siblings are left unchanged.
#[test]
fn strict_compound_update_aborts_on_first_error() {
    let db = fresh_db();
    let live = entry_builder("gym")
        .value(json!(1))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    let dead = entry_builder("gym")
        .value(json!(2))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    db.soft_delete_entry(&dead.slug).unwrap();

    // Put the tombstone first so we can verify the live row was never touched.
    let compound = format!("{},{}", dead.slug, live.slug);
    let err = run_cli(&["update", &compound, "reps=99", "--strict"], &db)
        .expect_err("strict must bail on tombstoned match");
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("tombstoned"),
        "strict error must name tombstoned, got: {msg}",
    );

    let fresh = db.get_entry(&live.slug).unwrap();
    assert_eq!(
        fresh.data.as_ref().unwrap()["reps"],
        json!(10),
        "strict mode must not have mutated the live sibling",
    );
}

/// Strict mode pre-scans the batch for tombstones: even a tombstone placed
/// AFTER live siblings must abort before any live sibling is mutated.
/// This is what distinguishes strict from default semantics on a mixed batch.
#[test]
fn strict_compound_update_rejects_later_tombstone_without_partial_mutation() {
    let db = fresh_db();
    let first = entry_builder("gym")
        .value(json!(1))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    let second = entry_builder("gym")
        .value(json!(2))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    let tomb = entry_builder("gym")
        .value(json!(3))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    db.soft_delete_entry(&tomb.slug).unwrap();

    // Tombstone is LAST in the batch. Non-strict would have mutated first/second
    // before hitting the tombstone; strict must catch it before any writes.
    let compound = format!("{},{},{}", first.slug, second.slug, tomb.slug);
    let err = run_cli(&["update", &compound, "reps=99", "--strict"], &db)
        .expect_err("strict must reject the batch entirely");
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("tombstoned"),
        "strict error must name tombstoned, got: {msg}",
    );

    for slug in [&first.slug, &second.slug] {
        let row = db.get_entry(slug).unwrap();
        assert_eq!(
            row.data.as_ref().unwrap()["reps"],
            json!(10),
            "strict must not have partially-mutated {slug}",
        );
    }
}

/// Compound rekey applies the spec's `id_template` per row, leaving both
/// originals in place and creating one new row per batch element.
#[test]
fn compound_rekey_creates_new_row_per_entry() {
    let db = fresh_db();
    let mut spec = spec_builder("mood").default_verb(Verb::Switch).build();
    spec.id_template = Some("%field%:%state%".to_string());
    db.save_field_spec(&spec).unwrap();

    let happy = entry_builder("mood")
        .verb(Verb::Switch)
        .value(json!("happy"))
        .extra_data(json!({"state": "happy"}))
        .insert(&db);
    let sad = entry_builder("mood")
        .verb(Verb::Switch)
        .value(json!("sad"))
        .extra_data(json!({"state": "sad"}))
        .insert(&db);

    let compound = format!("{},{}", happy.slug, sad.slug);
    run_cli(&["rekey", &compound], &db)
        .unwrap_or_else(|err| panic!("compound rekey must succeed: {err:#}"));

    // Originals remain, plus two new rows under the templated slugs.
    assert_count!(db, "mood", 4);
    assert!(db.get_entry("mood:happy").is_ok(), "happy rekey landed");
    assert!(db.get_entry("mood:sad").is_ok(), "sad rekey landed");
    assert!(db.get_entry(&happy.slug).is_ok(), "happy original preserved");
    assert!(db.get_entry(&sad.slug).is_ok(), "sad original preserved");
}

/// Trailing empty segments are silently dropped — datum's `"abc,def,"`
/// continues to work without per-token bookkeeping at the call site.
#[test]
fn compound_update_tolerates_trailing_comma() {
    let db = fresh_db();
    let a = entry_builder("gym")
        .value(json!(1))
        .extra_data(json!({"reps": 10}))
        .insert(&db);
    let b = entry_builder("gym")
        .value(json!(2))
        .extra_data(json!({"reps": 10}))
        .insert(&db);

    let compound = format!("{},{},", a.slug, b.slug);
    run_cli(&["update", &compound, "reps=20"], &db)
        .unwrap_or_else(|err| panic!("trailing comma must not fail: {err:#}"));

    for slug in [&a.slug, &b.slug] {
        let row = db.get_entry(slug).unwrap();
        assert_eq!(row.data.as_ref().unwrap()["reps"], json!(20));
    }
}
