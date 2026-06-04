//! `datum alias set` / `remove` guard parity and honest reporting (da-vpdz).
//!
//! Both alias save paths (`alias set` and the inline `--alias` flag) go
//! through one `save_user_alias` helper carrying the empty-name check,
//! the overwrite policy, and the built-in-shadowing warning. `alias
//! remove` reports what actually happened instead of hedging.

mod common;

use common::run_cli;
use datum_core::action::Action;
use datum_core::test_util::{fresh_db, spec_builder};

/// `alias set ""` is rejected — no empty-named row lands.
#[test]
fn alias_set_empty_name_errors() {
    let db = fresh_db();

    let err = run_cli(&["alias", "set", "", "occur coffee"], &db)
        .expect_err("empty alias name must be rejected");
    assert!(
        format!("{err:#}").contains("non-empty"),
        "unexpected error: {err:#}"
    );
    assert!(db.lookup_alias("").unwrap().is_none());
}

/// Naming the alias after a built-in subcommand still saves (per
/// da-0b4, with a stderr warning — asserted at the subprocess level in
/// `e2e_subprocess.rs`), matching the inline `--alias` path.
#[test]
fn alias_set_with_builtin_name_still_saves() {
    let db = fresh_db();

    run_cli(&["alias", "set", "tail", "occur coffee"], &db)
        .expect("collision with built-in is allowed (with warning)");

    let alias = db.lookup_alias("tail").unwrap().expect("alias saved");
    assert_eq!(alias.expands_to, "occur coffee");
}

/// The direct command overwrites an existing alias silently — `alias
/// set` IS the explicit edit, unlike inline `--alias` which requires
/// `--alias-overwrite`.
#[test]
fn alias_set_overwrites_existing_silently() {
    let db = fresh_db();

    run_cli(&["alias", "set", "cof", "occur coffee"], &db).expect("first save");
    run_cli(&["alias", "set", "cof", "occur tea"], &db).expect("overwrite");

    let alias = db.lookup_alias("cof").unwrap().expect("alias present");
    assert_eq!(alias.expands_to, "occur tea");
}

/// Removing an alias that doesn't exist is a not-found error, not a
/// hedged checkmark.
#[test]
fn alias_remove_nonexistent_reports_not_found() {
    let db = fresh_db();

    let err = run_cli(&["alias", "remove", "ghost"], &db)
        .expect_err("removing a nonexistent alias must fail");
    assert!(
        format!("{err:#}").contains("no user alias 'ghost'"),
        "unexpected error: {err:#}"
    );
}

/// Removing a spec-derived alias points at the owning field's spec —
/// those aliases live in `field_specs.spec_json`, not the `aliases`
/// table, so deleting the row can't work.
#[test]
fn alias_remove_derived_alias_points_at_owning_field() {
    let db = fresh_db();

    spec_builder("mood").default_verb(Action::Switch).save(&db);
    let mut spec = db.load_spec("mood").unwrap().unwrap();
    spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
    spec.regenerate_derived_aliases();
    db.save_field_spec(&spec).unwrap();

    let err = run_cli(&["alias", "remove", "happy"], &db)
        .expect_err("removing a derived alias must fail with guidance");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("derived alias") && msg.contains("'mood'"),
        "error must name the owning field: {msg}"
    );

    // The derived alias is untouched.
    let alias = db.lookup_alias("happy").unwrap().expect("alias intact");
    assert_eq!(alias.expands_to, "switch mood happy");
}

/// Removing a real user alias succeeds and the row is gone.
#[test]
fn alias_remove_existing_succeeds() {
    let db = fresh_db();

    run_cli(&["alias", "set", "cof", "occur coffee"], &db).expect("save");
    run_cli(&["alias", "remove", "cof"], &db).expect("remove");
    assert!(db.lookup_alias("cof").unwrap().is_none());
}
