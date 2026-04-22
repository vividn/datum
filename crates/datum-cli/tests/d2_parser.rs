//! D2 — parser acceptance tests (da-gnj.2).
//!
//! Covers the six CLI-level cases called out in the bead:
//!   * `dash_K_builds_slug_in_order`
//!   * `dash_K_with_defaults`
//!   * `dash_K_optional_absent_drops_segment`
//!   * `dash_k_affects_data_not_slug`
//!   * `retime_overwrites_precision_from_new_input`
//!   * `parser_version_increments_in_stored_entries`
#![allow(non_snake_case)]

mod common;

use common::run_cli;
use datum_core::entry::{CURRENT_PARSER_VERSION, PrecisionUnit};
use datum_core::test_util::fresh_db;

/// `-K KEY -K KEY jake berlin` → slug `person:jake__berlin`.
#[test]
fn dash_K_builds_slug_in_order() {
    let db = fresh_db();
    run_cli(
        &["add", "person", "-K", "name", "-K", "city", "jake", "berlin"],
        &db,
    )
    .expect("add with -K slug keys");

    let entries = db.entries_for_field("person", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.slug, "person:jake__berlin");
    // Both keys also surface in data.
    let data = e.data.as_ref().expect("data populated");
    assert_eq!(data["name"], serde_json::json!("jake"));
    assert_eq!(data["city"], serde_json::json!("berlin"));
}

/// `-K title=untitled` with no positional → slug falls back to the default.
#[test]
fn dash_K_with_defaults() {
    let db = fresh_db();
    run_cli(&["add", "book", "-K", "title=untitled"], &db)
        .expect("add with -K default only");

    let entries = db.entries_for_field("book", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.slug, "book:untitled");
    let data = e.data.as_ref().expect("data populated");
    assert_eq!(data["title"], serde_json::json!("untitled"));
}

/// `-K year=` with no positional → year segment dropped entirely from slug.
#[test]
fn dash_K_optional_absent_drops_segment() {
    let db = fresh_db();
    run_cli(
        &["add", "book", "-K", "title=untitled", "-K", "year=", "Goldmund"],
        &db,
    )
    .expect("add with trailing optional-no-default -K");

    let entries = db.entries_for_field("book", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    // Positional "Goldmund" fills title; year has no positional and no
    // default → omitted from slug and data alike.
    assert_eq!(e.slug, "book:Goldmund");
    let data = e.data.as_ref().expect("data populated");
    assert_eq!(data["title"], serde_json::json!("Goldmund"));
    assert!(data.get("year").is_none(), "optional-absent key not in data");
}

/// `-k KEY` (no `=`) fills from positional and lands in data only — slug
/// stays on the timestamp fallback.
#[test]
fn dash_k_affects_data_not_slug() {
    let db = fresh_db();
    run_cli(&["note", "journal", "-k", "topic", "dreams"], &db)
        .expect("note with -k slot");

    let entries = db.entries_for_field("journal", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert!(
        !e.slug.contains("dreams"),
        "data-only key must not appear in slug ({})",
        e.slug
    );
    assert!(
        e.slug.starts_with("journal:"),
        "slug uses the field:timestamp fallback"
    );
    let data = e.data.as_ref().expect("data populated");
    assert_eq!(data["topic"], serde_json::json!("dreams"));
}

/// `datum retime <id> -t <new>±<unc>` re-parses the new time and overwrites
/// precision fields regardless of the entry's previous precision.
#[test]
fn retime_overwrites_precision_from_new_input() {
    let db = fresh_db();
    // Seed an entry with a well-known precision (exact / 0s).
    run_cli(&["occur", "coffee", "-t", "12:00"], &db).expect("seed entry");
    let seeded = &db.entries_for_field("coffee", None).unwrap()[0];
    assert_eq!(seeded.start_time_precision, 0);
    assert_eq!(seeded.start_time_precision_unit, PrecisionUnit::Exact);
    let slug = seeded.slug.clone();

    // Retime with a ±5m uncertainty on the fresh input — precision replaces,
    // not merges.
    run_cli(&["retime", &slug, "-t", "13:00±5m"], &db).expect("retime -t");

    let after = db.get_entry(&slug).unwrap();
    assert_eq!(after.start_time_precision, 5 * 60);
    assert_eq!(after.start_time_precision_unit, PrecisionUnit::Minute);
}

/// Every new entry written through the parser carries the current
/// `parser_version` so a future parser-bugfix replay can reprocess them.
#[test]
fn parser_version_increments_in_stored_entries() {
    assert!(
        CURRENT_PARSER_VERSION >= 2,
        "D2 must bump the parser version above v1"
    );
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("log coffee");
    let e = &db.entries_for_field("coffee", None).unwrap()[0];
    assert_eq!(e.parser_version, CURRENT_PARSER_VERSION);
}
