//! Integration tests for `datum count` (S10 / da-u4k.10).
//!
//! Cover total counts, field-filtered counts, `--group-by` grouping, and
//! `--since` time-bounding. All tests drive through the CLI dispatch path
//! so argument parsing is exercised alongside the query logic.

mod common;

use chrono::{Duration, Utc};
use common::run_cli;
use datum_cli::args::CountArgs;
use datum_commands::commands::count::{compute_count, CountResult};
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, fresh_db};
use serde_json::json;

#[test]
fn count_total_returns_all_live_entries() {
    let db = fresh_db();
    entry_builder("coffee").state(json!(1)).insert(&db);
    entry_builder("coffee").state(json!(2)).insert(&db);
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("good"))
        .insert(&db);

    let result = compute_count(&CountArgs::default(), &db).unwrap();
    assert_eq!(result, CountResult::Total(3));
}

#[test]
fn count_with_field_filters_to_that_field() {
    let db = fresh_db();
    entry_builder("coffee").state(json!(1)).insert(&db);
    entry_builder("coffee").state(json!(2)).insert(&db);
    entry_builder("tea").state(json!(1)).insert(&db);

    let result = compute_count(
        &CountArgs {
            field: Some("coffee".to_string()),
            ..CountArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(result, CountResult::Total(2));
}

#[test]
fn count_group_by_state_aggregates_per_value() {
    let db = fresh_db();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("good"))
        .insert(&db);
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("good"))
        .insert(&db);
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("bad"))
        .insert(&db);

    let result = compute_count(
        &CountArgs {
            field: Some("mood".to_string()),
            group_by: Some("state".to_string()),
            ..CountArgs::default()
        },
        &db,
    )
    .unwrap();
    let rows = match result {
        CountResult::Grouped(rows) => rows,
        other => panic!("expected Grouped, got {other:?}"),
    };
    // Ordered by count DESC, then key ASC.
    assert_eq!(rows[0], ("\"good\"".to_string(), 2));
    assert_eq!(rows[1], ("\"bad\"".to_string(), 1));
}

#[test]
fn count_group_by_value_is_rejected() {
    // da-igwt regression guard: entry "value" vocab is gone. "state" is the
    // only group-by key; the old "value" alias must be rejected.
    let db = fresh_db();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("up"))
        .insert(&db);

    assert!(
        compute_count(
            &CountArgs {
                group_by: Some("state".to_string()),
                ..CountArgs::default()
            },
            &db,
        )
        .is_ok(),
        "`--group-by state` must work"
    );
    assert!(
        compute_count(
            &CountArgs {
                group_by: Some("value".to_string()),
                ..CountArgs::default()
            },
            &db,
        )
        .is_err(),
        "`--group-by value` must be rejected (da-igwt: value is gone)"
    );
}

#[test]
fn count_help_group_by_keys_match_accepted_keys() {
    // da-3z0f anti-drift guard: the `--group-by` help summary must list
    // exactly the keys the command accepts — no more (help can't advertise
    // an unimplemented key like the old "value"), no fewer.
    use datum_core::db::GroupColumn;
    use datum_parse::help::find_command;

    let (spec, _) = find_command("count").expect("count command spec");
    let summary = spec
        .flags
        .iter()
        .find(|f| f.long == "group-by")
        .expect("--group-by flag spec")
        .summary;

    let mut advertised: Vec<&str> = summary.split('|').collect();
    advertised.sort_unstable();

    // Every advertised key must parse (advertised ⊆ accepted).
    for key in &advertised {
        assert!(
            key.parse::<GroupColumn>().is_ok(),
            "help advertises unaccepted group-by key '{key}'"
        );
    }

    // Every accepted key must be advertised (accepted ⊆ advertised).
    let mut accepted: Vec<&str> = GroupColumn::ALL.iter().map(|c| c.as_sql()).collect();
    accepted.sort_unstable();

    assert_eq!(
        advertised, accepted,
        "count --help group-by keys ({advertised:?}) must match accepted keys ({accepted:?})"
    );
}

#[test]
fn count_group_by_rejects_unknown_key() {
    let db = fresh_db();
    entry_builder("x").insert(&db);
    let err = compute_count(
        &CountArgs {
            group_by: Some("garbage".to_string()),
            ..CountArgs::default()
        },
        &db,
    )
    .expect_err("unknown --group-by must fail");
    let msg = format!("{err}");
    assert!(msg.contains("unsupported"), "err was: {msg}");
}

#[test]
fn count_since_bounds_the_window() {
    let db = fresh_db();
    let old = Utc::now() - Duration::days(30);
    let recent = Utc::now() - Duration::minutes(5);
    entry_builder("coffee")
        .state(json!(1))
        .time_start_utc(old)
        .insert(&db);
    entry_builder("coffee")
        .state(json!(2))
        .time_start_utc(recent)
        .insert(&db);

    let result = compute_count(
        &CountArgs {
            since: Some("1h".to_string()),
            ..CountArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(result, CountResult::Total(1));
}

#[test]
fn count_since_accepts_rfc3339_datetime() {
    let db = fresh_db();
    let cutoff = Utc::now() - Duration::days(1);
    entry_builder("coffee")
        .state(json!(1))
        .time_start_utc(Utc::now() - Duration::days(2))
        .insert(&db);
    entry_builder("coffee")
        .state(json!(2))
        .time_start_utc(Utc::now())
        .insert(&db);

    let result = compute_count(
        &CountArgs {
            since: Some(cutoff.to_rfc3339()),
            ..CountArgs::default()
        },
        &db,
    )
    .unwrap();
    assert_eq!(result, CountResult::Total(1));
}

#[test]
fn count_cli_dispatch_smoke_test() {
    // The string-level dispatch path must accept `datum count [field]`
    // without tripping the field/verb pre-pass.
    let db = fresh_db();
    entry_builder("coffee").state(json!(1)).insert(&db);
    run_cli(&["count"], &db).expect("bare count must dispatch");
    run_cli(&["count", "coffee"], &db).expect("field-filtered count must dispatch");
    run_cli(&["count", "--json"], &db).expect("json count must dispatch");
}
