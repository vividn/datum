//! Inline `--alias <NAME>` flag (da-5sp).
//!
//! The alias-naming flag lets a user save the command they just typed into
//! the alias table after a successful run, so future invocations of `datum
//! <name> ...` expand to the same canonical form.

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;

/// `datum start sleep -c "naptime" --alias bedtime` succeeds, the entry
/// lands, and the alias is recorded as `start sleep -c naptime` — without
/// the alias-naming flag. A subsequent invocation of the alias name
/// expands back through the pre-pass and lands a real entry.
#[test]
fn alias_flag_records_canonical_form_and_round_trips() {
    let db = fresh_db();

    run_cli(
        &["start", "sleep", "-c", "naptime", "--alias", "bedtime"],
        &db,
    )
    .expect("start with --alias");

    let alias = db
        .lookup_alias("bedtime")
        .expect("DB read")
        .expect("alias 'bedtime' should be recorded");
    assert_eq!(alias.expands_to, "start sleep -c naptime");

    // Round-trip: the pre-pass picks up the saved alias on a fresh
    // invocation and routes `bedtime` → `start sleep -c naptime`.
    run_cli(&["bedtime"], &db).expect("alias dispatch");

    let entries = db.entries_for_field("sleep", None).unwrap();
    // Two entries: the original `start sleep -c naptime --alias bedtime`
    // and the alias-expanded `bedtime` re-run.
    assert_eq!(entries.len(), 2);
}

/// Re-using the same alias name without `--alias-overwrite` is an error.
/// The first save succeeds; the second invocation's command itself runs
/// (an entry lands), but the alias-save fails with a guidance message.
#[test]
fn alias_flag_collides_without_overwrite() {
    let db = fresh_db();

    run_cli(&["occur", "coffee", "--alias", "cof"], &db).expect("first save");

    let err = run_cli(&["occur", "tea", "--alias", "cof"], &db)
        .expect_err("collision without --alias-overwrite must fail");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("already exists") && msg.contains("--alias-overwrite"),
        "unexpected error message: {msg}"
    );

    // The original alias body is still in place — overwrite did not happen.
    let alias = db
        .lookup_alias("cof")
        .unwrap()
        .expect("alias still present");
    assert_eq!(alias.expands_to, "occur coffee");
}

/// `--alias-overwrite` permits replacing an existing alias body.
#[test]
fn alias_flag_overwrite_replaces_existing() {
    let db = fresh_db();

    run_cli(&["occur", "coffee", "--alias", "cof"], &db).expect("first save");
    run_cli(
        &["occur", "tea", "--alias", "cof", "--alias-overwrite"],
        &db,
    )
    .expect("overwrite");

    let alias = db
        .lookup_alias("cof")
        .unwrap()
        .expect("alias still present");
    assert_eq!(alias.expands_to, "occur tea");
}

/// Naming the alias after a built-in subcommand (e.g. `tail`) saves the
/// alias but the rest of the system continues to treat the verb as a
/// built-in (the alias-resolution pre-pass stops at known subcommands), so
/// the alias is silently shadowed rather than firing.
#[test]
fn alias_flag_shadow_collision_with_builtin_still_saves() {
    let db = fresh_db();

    run_cli(&["occur", "coffee", "--alias", "tail"], &db)
        .expect("collision with built-in is allowed (with warning)");

    let alias = db
        .lookup_alias("tail")
        .unwrap()
        .expect("alias 'tail' must be saved");
    assert_eq!(alias.expands_to, "occur coffee");
}
