//! A field name refuses a `,` — the CLI splits `undo a,b` on it, so such a
//! field could never be named in a list — and still allows whitespace
//! (da-6p3du, vividn: «Allow white space, disallow commas»).

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};

fn refusal(args: &[&str], db: &datum_core::db::Db) -> String {
    format!("{:#}", run_cli(args, db).unwrap_err())
}

#[test]
fn a_direct_name_with_a_comma_is_refused() {
    let db = fresh_db();
    let err = refusal(&["occur", "a,b"], &db);
    assert!(err.contains("cannot contain ','"), "{err}");
    assert!(logged_entries(&db, "a,b").is_empty());
}

#[test]
fn a_template_value_with_a_comma_is_refused() {
    let db = fresh_db();
    let err = refusal(&["occur", "tag__%t%", "t=a,b"], &db);
    assert!(err.contains("cannot contain ','"), "{err}");
    assert!(logged_entries(&db, "tag__a,b").is_empty());
}

#[test]
fn an_alias_expanding_to_a_comma_is_refused() {
    let db = fresh_db();
    run_cli(&["alias", "set", "ab", "occur a,b"], &db).unwrap();
    let err = refusal(&["ab"], &db);
    assert!(err.contains("cannot contain ','"), "{err}");
}

/// An update re-derives a template field, so it is a name being chosen too;
/// the entry stays where it was.
#[test]
fn an_update_rederiving_a_comma_is_refused() {
    let db = fresh_db();
    run_cli(&["occur", "tag__%t%", "t=x"], &db).unwrap();
    let slug = logged_entries(&db, "tag__x")[0].slug.to_string();
    let err = refusal(&["update", &slug, "t=a,b"], &db);
    assert!(err.contains("cannot contain ','"), "{err}");
    assert_eq!(logged_entries(&db, "tag__x").len(), 1);
}

/// `update field=` moves a plain entry to a field — a name being chosen.
#[test]
fn an_update_moving_to_a_comma_field_is_refused() {
    let db = fresh_db();
    run_cli(&["occur", "gym"], &db).unwrap();
    let slug = logged_entries(&db, "gym")[0].slug.to_string();
    let err = refusal(&["update", &slug, "field=a,b"], &db);
    assert!(err.contains("cannot contain ','"), "{err}");
    assert_eq!(logged_entries(&db, "gym").len(), 1);
}

/// A row already stored in a comma field is not moved by an update, so the
/// update goes through.
#[test]
fn an_existing_comma_entry_stays_updatable() {
    let db = fresh_db();
    let row = datum_core::test_util::EntryBuilder::new("a,b").build();
    db.insert_entry(&row).unwrap();
    run_cli(&["update", row.slug.as_str(), "x=1"], &db).unwrap();
    assert_eq!(logged_entries(&db, "a,b").len(), 1);
}

/// Declaring a field chooses its name too: `track`, `spec add-state` and a
/// kit's `[[field]]` each refuse one no log could then reach.
#[test]
fn declaring_a_comma_field_is_refused() {
    let db = fresh_db();
    let kit = "[kit]\nname = \"k\"\n\n[[field]]\nname = \"k,l\"\nverb = \"occur\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("k.toml");
    std::fs::write(&path, kit).unwrap();
    let path = path.to_string_lossy().to_string();
    for (args, field) in [
        (vec!["track", "x,y", "--verb", "occur"], "x,y"),
        (vec!["spec", "add-state", "p,q", "on"], "p,q"),
        (vec!["kit", "install", &path], "k,l"),
    ] {
        let err = refusal(&args, &db);
        assert!(err.contains("cannot contain ','"), "{args:?}: {err}");
        assert!(db.load_spec(field).unwrap().is_none(), "{args:?}");
    }
}

/// A spec a comma field already has stays editable: the check is for a
/// name being chosen, not one already stored.
#[test]
fn an_existing_comma_spec_stays_editable() {
    let db = fresh_db();
    let mut row = datum_core::test_util::EntryBuilder::new("spec")
        .no_time()
        .extra_data(serde_json::json!({"field": "a,b", "verb": "switch"}))
        .build();
    row.slug = datum_core::ids::Slug("spec:a,b".into());
    db.insert_entry(&row).unwrap();
    run_cli(&["spec", "add-state", "a,b", "on"], &db).unwrap();
}

#[test]
fn whitespace_is_allowed() {
    let db = fresh_db();
    run_cli(&["occur", "a b"], &db).unwrap();
    assert_eq!(logged_entries(&db, "a b").len(), 1);
}

#[test]
fn the_colon_refusal_is_unchanged() {
    let db = fresh_db();
    let err = refusal(&["occur", "x:y"], &db);
    assert!(err.contains("field name cannot contain ':'"), "{err}");
}

/// A spaced field's derived aliases are reached by their name as one
/// quoted token — `datum "tea time"`, `datum "tea timee"` — and log to the
/// field, not to `tea` (da-7h73h).
#[test]
fn a_spaced_fields_derived_aliases_log_to_it() {
    let db = fresh_db();
    run_cli(&["spec", "set", "tea time", "--verb", "start"], &db).unwrap();
    run_cli(&["tea time"], &db).unwrap();
    run_cli(&["tea timee"], &db).unwrap();
    assert_eq!(logged_entries(&db, "tea time").len(), 2);
    assert!(logged_entries(&db, "tea").is_empty());
}
