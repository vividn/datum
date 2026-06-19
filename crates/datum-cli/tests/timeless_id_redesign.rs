//! da-r17h — timeless-id redesign acceptance tests.
//!
//! The redesign splits the timeless logger on the identity axis:
//!   * `note` = timeless, AUTO create_time id (`field:c<create_time>`) —
//!     "record this moment, won't look it up." Many-per-field works because
//!     distinct create_times keep slugs apart.
//!   * `add`  = timeless, REQUIRES an explicit id (`-K` / `--id` / `id=`) —
//!     "register this keyed thing, will look it up."
//!   * `-T`   = convert an occurred verb to note-data (no occurrence logged).
//!
//! Slug interpolator auto-prefixes `c`/`m` for create/modify time, mirroring
//! the display convention; the old `:unset` sentinel (which UNIQUE-collided
//! one-per-field) is gone.

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;

/// A timeless slug is `field:c<create_time>` — the body after the field
/// delimiter begins with the `c` create-time marker.
fn slug_is_timeless(slug: &str) -> bool {
    slug.split_once(':')
        .is_some_and(|(_, body)| body.starts_with('c'))
}

// ─── note: auto create_time id, many-per-field ──────────────────────────────

#[test]
fn note_auto_ids_on_create_time() {
    let db = fresh_db();
    run_cli(&["note", "coffee"], &db).expect("note coffee");
    let e = &db.entries_for_field("coffee", None).unwrap()[0];
    assert!(e.occur_time.is_none(), "note is timeless");
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "note slugs on create_time (field:c<create_time>), got {}",
        e.slug
    );
    assert_eq!(
        e.id_structure.as_deref(),
        Some("%create_time%"),
        "note's id template is %create_time%"
    );
}

/// Two distinct-content notes for one field coexist on distinct slugs — the
/// `:unset` collision (da-o7iz) is dead.
#[test]
fn note_many_per_field_distinct_slugs() {
    let db = fresh_db();
    run_cli(&["note", "mood", "x=1"], &db).expect("note mood x=1");
    run_cli(&["note", "mood", "x=2"], &db).expect("note mood x=2");
    let rows = db.entries_for_field("mood", None).unwrap();
    assert_eq!(rows.len(), 2, "both notes persist (no :unset collision)");
    assert_ne!(
        rows[0].slug.as_str(),
        rows[1].slug.as_str(),
        "distinct create_times → distinct slugs"
    );
    assert!(rows.iter().all(|e| slug_is_timeless(e.slug.as_str())));
}

/// `note f` and `occur f -T` target the SAME field but produce DISTINCT
/// timeless slugs — the original collision that motivated the redesign.
#[test]
fn note_and_occur_dash_t_do_not_collide() {
    let db = fresh_db();
    run_cli(&["note", "thing", "a=1"], &db).expect("note thing");
    run_cli(&["occur", "thing", "-T", "b=2"], &db).expect("occur thing -T");
    let rows = db.entries_for_field("thing", None).unwrap();
    assert_eq!(rows.len(), 2, "note + occur -T both land");
    assert!(rows.iter().all(|e| e.occur_time.is_none()));
    assert!(rows.iter().all(|e| slug_is_timeless(e.slug.as_str())));
}

// ─── add: requires an explicit id ───────────────────────────────────────────

#[test]
fn add_without_id_errors_descriptively() {
    let db = fresh_db();
    let err = run_cli(&["add", "widget"], &db).expect_err("bare add must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("add needs an id"),
        "error states the contract, got: {msg}"
    );
    assert!(
        msg.contains("-K") && msg.contains("--id"),
        "error names both id routes (-K and --id), got: {msg}"
    );
}

#[test]
fn add_with_slug_key_id_or_positional_works() {
    let db = fresh_db();
    // -K derives the slug from a key.
    run_cli(&["add", "widget", "-K", "kind", "kind=gear"], &db).expect("add -K");
    assert_eq!(db.get_entry("widget:gear").unwrap().field, "widget");
    // --id sets it directly.
    run_cli(&["add", "gadget", "--id", "alpha"], &db).expect("add --id");
    assert_eq!(db.get_entry("gadget:alpha").unwrap().field, "gadget");
    // id= positional promotion.
    run_cli(&["add", "book", "id=goldmund"], &db).expect("add id=");
    assert_eq!(db.get_entry("book:goldmund").unwrap().field, "book");
}

/// `add … -T` is exempt from the id requirement: `-T` explicitly converts to
/// note-data, so it takes the create_time-id branch instead of erroring.
#[test]
fn add_dash_t_is_note_data_not_an_error() {
    let db = fresh_db();
    run_cli(&["add", "widget", "-T", "note=hi"], &db).expect("add -T is note-data");
    let e = &db.entries_for_field("widget", None).unwrap()[0];
    assert!(e.occur_time.is_none());
    assert!(slug_is_timeless(e.slug.as_str()), "got {}", e.slug);
}

// ─── task 7: multiple --id compose; occurTime maps to occur_time ────────────

#[test]
fn multiple_id_flags_compose_with_occurtime_alias() {
    let db = fresh_db();
    run_cli(
        &[
            "occur",
            "tx",
            "--id",
            "%occurTime%",
            "--id",
            "%comment%",
            "comment=txn1",
        ],
        &db,
    )
    .expect("occur tx --id %occurTime% --id %comment%");
    let e = &db.entries_for_field("tx", None).unwrap()[0];
    // The slug body carries BOTH the occur_time and the comment, joined by __.
    assert!(
        e.slug.as_str().ends_with("__txn1"),
        "comment composes into the slug, got {}",
        e.slug
    );
    assert!(
        e.slug.as_str().starts_with("tx:2"),
        "occurTime resolves to occur_time in the slug, got {}",
        e.slug
    );
}

// ─── task 8/9: occur_time= overwrites the column uniformly ───────────────────

#[test]
fn occur_time_key_overwrites_column_across_verbs() {
    let db = fresh_db();
    // add: re-anchors a would-be-timeless entry to an absolute occur_time.
    run_cli(&["add", "ev", "occur_time=2026-05-01T10:00:00Z"], &db).expect("add occur_time=");
    let add_e = &db.entries_for_field("ev", None).unwrap()[0];
    assert_eq!(
        add_e.occur_time.as_ref().map(|t| t.utc.to_rfc3339()),
        Some("2026-05-01T10:00:00+00:00".to_string()),
        "occur_time= overwrites the column even on add (no -K needed once anchored)"
    );

    // switch: same override, state preserved.
    run_cli(
        &["switch", "mood", "happy", "occur_time=2026-05-01T09:00:00Z"],
        &db,
    )
    .expect("switch occur_time=");
    let sw = &db.entries_for_field("mood", None).unwrap()[0];
    assert_eq!(
        sw.occur_time.as_ref().map(|t| t.utc.to_rfc3339()),
        Some("2026-05-01T09:00:00+00:00".to_string())
    );
    assert_eq!(sw.state, Some(serde_json::json!("happy")));
}
