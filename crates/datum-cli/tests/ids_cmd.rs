//! Integration tests for `datum ids` (S10 / da-u4k.10).
//!
//! Covers default newest-first ordering, `--limit`, `--oldest-first`,
//! `--slug`, field filtering, and `--since`. Tests assert on ordering as
//! well as membership because the command's whole purpose is producing a
//! time-ordered slice of ids.

mod common;

use chrono::{Duration, Utc};
use common::run_cli;
use datum_cli::args::IdsArgs;
use datum_commands::commands::ids::compute_ids;
use datum_core::test_util::{entry_builder, fresh_db};
use serde_json::json;

#[test]
fn ids_default_is_newest_first() {
    let db = fresh_db();
    let now = Utc::now();
    let old = entry_builder("coffee")
        .value(json!("old"))
        .time_start_utc(now - Duration::hours(2))
        .insert(&db);
    let mid = entry_builder("coffee")
        .value(json!("mid"))
        .time_start_utc(now - Duration::hours(1))
        .insert(&db);
    let new = entry_builder("coffee")
        .value(json!("new"))
        .time_start_utc(now)
        .insert(&db);

    let ids = compute_ids(&IdsArgs::default(), &db).unwrap();
    assert_eq!(ids, vec![new.hid, mid.hid, old.hid]);
}

#[test]
fn ids_oldest_first_flips_order() {
    let db = fresh_db();
    let now = Utc::now();
    let a = entry_builder("coffee")
        .value(json!(1))
        .time_start_utc(now - Duration::hours(2))
        .insert(&db);
    let b = entry_builder("coffee")
        .value(json!(2))
        .time_start_utc(now - Duration::hours(1))
        .insert(&db);

    let ids = compute_ids(
        &IdsArgs {
            oldest_first: true,
            ..IdsArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(ids, vec![a.hid, b.hid]);
}

#[test]
fn ids_limit_caps_the_output() {
    let db = fresh_db();
    let now = Utc::now();
    for i in 0..5 {
        entry_builder("coffee")
            .value(json!(i))
            .time_start_utc(now - Duration::minutes(i * 10))
            .insert(&db);
    }
    let ids = compute_ids(
        &IdsArgs {
            limit: Some(2),
            ..IdsArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(ids.len(), 2);
}

#[test]
fn ids_field_filter_restricts_to_that_field() {
    let db = fresh_db();
    let c = entry_builder("coffee").value(json!(1)).insert(&db);
    let _t = entry_builder("tea").value(json!(1)).insert(&db);

    let ids = compute_ids(
        &IdsArgs {
            field: Some("coffee".to_string()),
            ..IdsArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(ids, vec![c.hid]);
}

#[test]
fn ids_slug_prints_slugs_instead_of_hids() {
    let db = fresh_db();
    let e = entry_builder("coffee").value(json!(1)).insert(&db);

    let hids = compute_ids(&IdsArgs::default(), &db).unwrap();
    let slugs = compute_ids(
        &IdsArgs {
            slug: true,
            ..IdsArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(hids, vec![e.hid.clone()]);
    assert_eq!(slugs, vec![e.slug.clone()]);
    assert_ne!(hids, slugs, "hid and slug must differ");
}

#[test]
fn ids_since_bounds_the_window() {
    let db = fresh_db();
    let now = Utc::now();
    entry_builder("coffee")
        .value(json!("old"))
        .time_start_utc(now - Duration::days(30))
        .insert(&db);
    let recent = entry_builder("coffee")
        .value(json!("recent"))
        .time_start_utc(now - Duration::minutes(5))
        .insert(&db);

    let ids = compute_ids(
        &IdsArgs {
            since: Some("1h".to_string()),
            ..IdsArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(ids, vec![recent.hid]);
}

#[test]
fn ids_oldest_first_with_slug_and_limit_combine() {
    let db = fresh_db();
    let now = Utc::now();
    let a = entry_builder("coffee")
        .value(json!(1))
        .time_start_utc(now - Duration::hours(3))
        .insert(&db);
    let _b = entry_builder("coffee")
        .value(json!(2))
        .time_start_utc(now - Duration::hours(2))
        .insert(&db);
    let _c = entry_builder("coffee")
        .value(json!(3))
        .time_start_utc(now - Duration::hours(1))
        .insert(&db);

    let ids = compute_ids(
        &IdsArgs {
            oldest_first: true,
            limit: Some(1),
            slug: true,
            ..IdsArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(ids, vec![a.slug]);
}

#[test]
fn ids_cli_dispatch_smoke_test() {
    let db = fresh_db();
    entry_builder("coffee").value(json!(1)).insert(&db);
    run_cli(&["ids"], &db).expect("bare ids must dispatch");
    run_cli(&["ids", "coffee", "-n", "1"], &db).expect("field + limit must dispatch");
    run_cli(&["ids", "--oldest-first", "--slug"], &db).expect("oldest-first + slug must dispatch");
    run_cli(&["ids", "--json"], &db).expect("json output must dispatch");
}
