//! `datum alias set` / `remove` guard parity and honest reporting (da-vpdz).
//!
//! Both alias save paths (`alias set` and the inline `--alias` flag) go
//! through one `save_user_alias` helper carrying the empty-name check,
//! the overwrite policy, and the built-in-shadowing warning. `alias
//! remove` reports what actually happened instead of hedging.

use crate::common;

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
/// those aliases live in the spec itself, not the `aliases`
/// table, so deleting the row can't work.
#[test]
fn alias_remove_derived_alias_points_at_owning_field() {
    let db = fresh_db();

    spec_builder("mood").verb(Action::Switch).save(&db);
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

/// A cycle is refused with the path it went round, in the words the
/// person sees (`datum-parse`'s own tests pin the path, not the message).
#[test]
fn alias_cycle_is_refused_naming_the_loop() {
    let db = fresh_db();

    run_cli(&["alias", "set", "loop", "knot"], &db).expect("save loop");
    run_cli(&["alias", "set", "knot", "loop"], &db).expect("save knot");

    let err = run_cli(&["loop"], &db).expect_err("a cycle must be refused");
    assert_eq!(err.to_string(), "Alias cycle: loop → knot → loop");
}

/// A flag in the NAME slot is refused, not saved as an alias named
/// `--force` that swallows the next word as its expansion (da-m99fr).
#[test]
fn alias_set_refuses_a_flag_as_its_name() {
    let db = fresh_db();

    let err = run_cli(&["alias", "set", "--force", "tea", "occur coffee"], &db)
        .expect_err("a flag is not an alias name");
    assert!(
        format!("{err:#}").contains("unexpected flag: --force"),
        "unexpected error: {err:#}"
    );
    assert!(db.lookup_alias("--force").unwrap().is_none());
    assert!(db.lookup_alias("tea").unwrap().is_none());
}

/// A third word is refused by name instead of dropped (da-m99fr).
#[test]
fn alias_set_refuses_a_third_word() {
    let db = fresh_db();

    let err = run_cli(&["alias", "set", "tea", "occur tea", "nope9"], &db)
        .expect_err("a third positional must be refused");
    assert!(
        format!("{err:#}").contains("unexpected argument: 'nope9'"),
        "unexpected error: {err:#}"
    );
    assert!(db.lookup_alias("tea").unwrap().is_none());
}

/// The EXPANSION may start with a dash: only the NAME is checked.
#[test]
fn alias_set_expansion_may_start_with_a_flag() {
    let db = fresh_db();

    run_cli(&["alias", "set", "yc", "-y occur coffee"], &db).expect("saves");

    let alias = db.lookup_alias("yc").unwrap().expect("alias saved");
    assert_eq!(alias.expands_to, "-y occur coffee");
}
