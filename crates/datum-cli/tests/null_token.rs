//! da-revva: `null` is a JSON null again, as in v2 (vividn, 09-28). A data
//! key's `null` stores null and `'"null"'` the string; a `null` switch
//! state is IGNORED, which is what it meant in v2 — typed or imported.

use crate::common;

use common::run_cli;
use datum_commands::commands::fields::compute_fields;
use datum_core::db::{Db, NowFilter, IGNORED_STATE_VALUE};
use datum_core::test_util::{fresh_db, logged_all};
use serde_json::{json, Value as JsonValue};
use std::io::Write;

/// `field`'s latest entry's state, as stored.
fn last_state(db: &Db, field: &str) -> Option<JsonValue> {
    logged_all(db)
        .into_iter()
        .filter(|e| e.field == field)
        .max_by_key(|e| e.create_time)
        .unwrap_or_else(|| panic!("no {field} entry"))
        .state
}

/// Whether `fields --json` and `now --json -A` both call `field` ignored;
/// panics when they disagree.
fn ignored(db: &Db, field: &str) -> bool {
    let listing = compute_fields(db).unwrap();
    let by_fields = listing
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["field"] == field)
        .unwrap_or_else(|| panic!("`fields` does not list {field}"))["ignored"]
        .as_bool()
        .unwrap();
    let filter = NowFilter {
        include_tracked_false: true,
        include_ignored: true,
        ..NowFilter::default()
    };
    let by_now = db
        .current_now(&filter)
        .unwrap()
        .ignored
        .iter()
        .any(|f| f.field == field);
    assert_eq!(by_fields, by_now, "`fields` and `now` disagree on {field}");
    by_fields
}

#[test]
fn a_data_key_null_is_json_null_and_the_quoted_one_the_string() {
    let db = fresh_db();
    // argv as the shell leaves `k=null s='"null"'`.
    run_cli(&["occur", "x", "k=null", "s=\"null\""], &db).unwrap();
    let data = logged_all(&db).remove(0).data.unwrap();
    assert_eq!(data, json!({"k": null, "s": "null"}));
}

#[test]
fn a_null_switch_state_is_ignore() {
    let typed = fresh_db();
    run_cli(&["switch", "mood", "happy", "-t", "-2"], &typed).unwrap();
    // Control: a state is tracked.
    assert!(!ignored(&typed, "mood"));
    run_cli(&["switch", "mood", "null"], &typed).unwrap();

    let ignore = fresh_db();
    run_cli(&["switch", "mood", "happy", "-t", "-2"], &ignore).unwrap();
    run_cli(&["ignore", "mood"], &ignore).unwrap();

    for db in [&typed, &ignore] {
        assert_eq!(last_state(db, "mood"), Some(json!(IGNORED_STATE_VALUE)));
        assert!(ignored(db, "mood"));
    }
}

/// The JSON-quote escape reaches the string state too: `'"null"'` is a
/// state named "null", not IGNORED.
#[test]
fn a_quoted_null_switch_state_is_the_string() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "\"null\""], &db).unwrap();
    assert_eq!(last_state(&db, "mood"), Some(json!("null")));
    assert!(!ignored(&db, "mood"));
}

/// `update … state=null` on a switch agrees with `switch f null`; on an
/// entry with no state (an occur) the null stays a null.
#[test]
fn updating_a_state_to_null_is_ignore() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).unwrap();
    run_cli(&["update", "state=null"], &db).unwrap();
    assert_eq!(last_state(&db, "mood"), Some(json!(IGNORED_STATE_VALUE)));
    assert!(ignored(&db, "mood"));

    run_cli(&["occur", "x"], &db).unwrap();
    run_cli(&["update", "state=null"], &db).unwrap();
    assert_eq!(last_state(&db, "x"), Some(JsonValue::Null));
}

/// Switches: no `dur`. v2's `dur: null` is a moment, which moves no
/// field's state (da-22750).
#[test]
fn a_v2_null_state_imports_as_ignored() {
    let db = fresh_db();
    let backup = json!({"backupTime": "2025-02-02T12:26:32.488Z", "docs": [
        {"_id": "mood:2020-03-01T09:00:00.000Z",
         "data": {"field": "mood", "state": "happy",
                  "occurTime": {"utc": "2020-03-01T09:00:00.000Z", "o": 0, "tz": "UTC"}},
         "meta": {"humanId": "m1", "createTime": {"utc": "2020-03-01T09:00:01.000Z", "o": 0, "tz": "UTC"}}},
        {"_id": "mood:2020-03-02T09:00:00.000Z",
         "data": {"field": "mood", "state": null,
                  "occurTime": {"utc": "2020-03-02T09:00:00.000Z", "o": 0, "tz": "UTC"}},
         "meta": {"humanId": "m2", "createTime": {"utc": "2020-03-02T09:00:01.000Z", "o": 0, "tz": "UTC"}}}
    ]});
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(f, "{backup}").unwrap();
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    assert_eq!(last_state(&db, "mood"), Some(json!(IGNORED_STATE_VALUE)));
    assert!(ignored(&db, "mood"));
}
