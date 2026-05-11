//! Integration tests for the da-gh0.C multi-entry dispatch path.
//!
//! Drives the public [`datum_commands::cmd::dispatch_argv`] entry point
//! with comma-split argvs (Rule 14: standalone `,` is a top-level parser
//! feature) and asserts the resulting rows in `entries` match the
//! spec's atomicity / same-field-merge / _LAST behavior.

use datum_commands::cmd::dispatch_argv;
use datum_core::test_util::fresh_db;
use serde_json::Value as JsonValue;

fn live_rows(db: &datum_core::db::Db) -> Vec<(String, String, Option<String>, Option<String>)> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT field, occur_time, state, data \
             FROM entries WHERE deleted_at IS NULL ORDER BY rowid",
        )
        .unwrap();
    stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })
    .unwrap()
    .map(|r| r.unwrap())
    .collect()
}

/// Section O — `oov` round-trip:
/// `dm switch environment outside , switch body vertical` writes two
/// rows with the same `occur_time`, distinct fields, distinct states.
#[test]
fn oov_full_invocation_writes_two_rows_atomically() {
    let db = fresh_db();
    let argv: Vec<String> = vec![
        "datum",
        "switch",
        "environment",
        "outside",
        ",",
        "switch",
        "body",
        "vertical",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    dispatch_argv(argv, &db).unwrap();

    let rows = live_rows(&db);
    assert_eq!(rows.len(), 2, "expected two rows from comma-split multi");
    // Shared occur_time.
    assert_eq!(rows[0].1, rows[1].1);
    let fields: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert!(fields.contains(&"environment"));
    assert!(fields.contains(&"body"));
}

/// Section G — `switch body vertical , switch body sit` collapses to
/// one row whose state is the JSON array `["vertical", "sit"]`.
#[test]
fn same_field_two_switches_merges_to_parallel_via_dispatch() {
    let db = fresh_db();
    let argv: Vec<String> = vec![
        "datum", "switch", "body", "vertical", ",", "switch", "body", "sit",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    dispatch_argv(argv, &db).unwrap();

    let rows = live_rows(&db);
    assert_eq!(rows.len(), 1, "same-field merge should yield one row");
    let state_json: JsonValue = serde_json::from_str(rows[0].2.as_ref().unwrap()).unwrap();
    assert_eq!(state_json, serde_json::json!(["vertical", "sit"]));
}

/// Rule 9 via dispatch: repeated same-field same-state collapses.
#[test]
fn same_field_repeated_state_collapses_via_dispatch() {
    let db = fresh_db();
    let argv: Vec<String> = vec![
        "datum", "switch", "body", "vertical", ",", "switch", "body", "vertical",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    dispatch_argv(argv, &db).unwrap();

    let rows = live_rows(&db);
    assert_eq!(rows.len(), 1);
    let state_json: JsonValue = serde_json::from_str(rows[0].2.as_ref().unwrap()).unwrap();
    // Rule 9: collapses to single state, not Parallel array.
    assert_eq!(state_json, serde_json::json!("vertical"));
}

/// Section N — `_LAST` after a multi-write holds every resulting hid.
#[test]
fn last_holds_all_n_entries_after_dispatch_multi() {
    let db = fresh_db();
    let argv: Vec<String> = vec![
        "datum",
        "switch",
        "environment",
        "outside",
        ",",
        "switch",
        "body",
        "vertical",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    dispatch_argv(argv, &db).unwrap();

    let last = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
    assert_eq!(last.hids.len(), 2);
}

/// Section N — a single-row write after a multi resets `_LAST` to the
/// single hid.
#[test]
fn last_after_multi_then_single_write_holds_only_single_via_dispatch() {
    let db = fresh_db();
    let multi: Vec<String> = vec![
        "datum",
        "switch",
        "environment",
        "outside",
        ",",
        "switch",
        "body",
        "vertical",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    dispatch_argv(multi, &db).unwrap();
    assert_eq!(
        datum_core::last_docs::get_last_docs(&db)
            .unwrap()
            .unwrap()
            .hids
            .len(),
        2
    );

    let single: Vec<String> = vec!["datum", "switch", "body", "sit"]
        .into_iter()
        .map(String::from)
        .collect();
    dispatch_argv(single, &db).unwrap();
    assert_eq!(
        datum_core::last_docs::get_last_docs(&db)
            .unwrap()
            .unwrap()
            .hids
            .len(),
        1
    );
}

/// Sanity: a non-comma argv still routes through the existing
/// single-row codepath unchanged.
#[test]
fn no_comma_routes_through_single_row_path() {
    let db = fresh_db();
    let argv: Vec<String> = vec!["datum", "switch", "body", "vertical"]
        .into_iter()
        .map(String::from)
        .collect();
    dispatch_argv(argv, &db).unwrap();

    let rows = live_rows(&db);
    assert_eq!(rows.len(), 1);
    let state_json: JsonValue = serde_json::from_str(rows[0].2.as_ref().unwrap()).unwrap();
    assert_eq!(state_json, serde_json::json!("vertical"));
}

/// Section M (covered at the splitter layer in da-hjcg; reverified
/// at the dispatch layer here): a leading-empty side errors cleanly
/// instead of writing anything.
#[test]
fn empty_leading_side_errors_via_dispatch() {
    let db = fresh_db();
    let argv: Vec<String> = vec!["datum", ",", "switch", "body", "vertical"]
        .into_iter()
        .map(String::from)
        .collect();
    let err = dispatch_argv(argv, &db).err();
    assert!(err.is_some(), "expected empty-leading-side error");
    assert_eq!(live_rows(&db).len(), 0, "rollback should leave no rows");
}

/// Regression for da-nj20: every side of a multi-write must use the
/// canonical millisecond-Z slug-time format, not the nanosecond+offset
/// rfc3339 that the rebuild path used to emit.
#[test]
fn multi_write_slugs_share_canonical_time_format() {
    let db = fresh_db();
    let argv: Vec<String> = vec![
        "datum",
        "switch",
        "environment",
        "outside",
        ",",
        "switch",
        "body",
        "sit",
        ",",
        "occur",
        "alcohol",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    dispatch_argv(argv, &db).unwrap();

    let mut stmt = db
        .conn()
        .prepare("SELECT slug FROM entries WHERE deleted_at IS NULL ORDER BY rowid")
        .unwrap();
    let slugs: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(slugs.len(), 3, "expected three rows from comma-split multi");

    // Each slug must end with `Z` and contain exactly millisecond precision
    // (three digits after `.`, then `Z`). None may carry a `+00:00` offset
    // or nanosecond precision.
    let canon =
        regex_lite::Regex::new(r"^[^:]+:\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$").unwrap();
    for s in &slugs {
        assert!(canon.is_match(s), "slug {s:?} not in canonical ms-Z format");
        assert!(!s.contains("+00:00"), "slug {s:?} carries rfc3339 offset");
    }

    // All three sides share the same time component.
    let time_of = |s: &str| s.split_once(':').unwrap().1.to_string();
    let t0 = time_of(&slugs[0]);
    for s in &slugs[1..] {
        assert_eq!(time_of(s), t0, "side time mismatch: {s} vs {}", slugs[0]);
    }
}

/// Section I — distinct fields with the same data key name remain
/// independent (no Section H merge).
#[test]
fn different_field_same_data_key_independent_no_conflict_via_dispatch() {
    let db = fresh_db();
    let argv: Vec<String> = vec![
        "datum",
        "occur",
        "body",
        "doseMg=80",
        ",",
        "occur",
        "caffeine",
        "doseMg=100",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    dispatch_argv(argv, &db).unwrap();

    let rows = live_rows(&db);
    assert_eq!(rows.len(), 2);
    let by_field: std::collections::HashMap<&str, &str> = rows
        .iter()
        .map(|r| (r.0.as_str(), r.3.as_deref().unwrap_or("")))
        .collect();
    let body_data: JsonValue = serde_json::from_str(by_field["body"]).unwrap();
    let caf_data: JsonValue = serde_json::from_str(by_field["caffeine"]).unwrap();
    // Datum coerces numeric string tokens to JSON numbers.
    assert_eq!(body_data, serde_json::json!({"doseMg": 80}));
    assert_eq!(caf_data, serde_json::json!({"doseMg": 100}));
}
