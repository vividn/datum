//! da-lvyt0: a state that is `{id: x}` alone is `x` — however it arrives.
//! The parser files `state=X` under `state.id`, so a typed JSON object
//! `state={"id":"z"}` reached the shared boundary as `{id: {id: z}}` and
//! one collapse left `{id: z}`; and `db v2import` kept v2's states verbatim.

use crate::common;

use common::run_cli;
use datum_core::db::{Db, IGNORED_STATE_VALUE};
use datum_core::test_util::{fresh_db, logged_all};
use serde_json::{json, Value as JsonValue};
use std::io::Write;

/// `field`'s latest entry's (state, following_state), as stored.
fn last(db: &Db, field: &str) -> (Option<JsonValue>, Option<JsonValue>) {
    let e = logged_all(db)
        .into_iter()
        .filter(|e| e.field == field)
        .max_by_key(|e| e.create_time)
        .unwrap_or_else(|| panic!("no {field} entry"));
    (e.state, e.following_state)
}

#[test]
fn update_with_a_json_id_object_state_stores_the_id() {
    let db = fresh_db();
    run_cli(&["switch", "f", "x"], &db).unwrap();
    run_cli(&["update", r#"state={"id":"z"}"#], &db).unwrap();
    assert_eq!(last(&db, "f").0, Some(json!("z")));

    run_cli(&["update", r#"state={"id":"z","n":1}"#], &db).unwrap();
    assert_eq!(last(&db, "f").0, Some(json!({"id": "z", "n": 1})));
}

/// The control: switch already stored the id; it still does.
#[test]
fn switch_with_a_json_id_object_state_is_unchanged() {
    let db = fresh_db();
    run_cli(&["switch", "f", r#"{"id":"x"}"#], &db).unwrap();
    assert_eq!(last(&db, "f").0, Some(json!("x")));
}

#[test]
fn following_state_takes_the_same_rule_on_update_and_add() {
    let db = fresh_db();
    run_cli(&["switch", "f", "b"], &db).unwrap();
    run_cli(&["update", r#"following_state={"id":"q"}"#, "dur=5m"], &db).unwrap();
    assert_eq!(last(&db, "f").1, Some(json!("q")));

    run_cli(
        &[
            "add",
            "g",
            "--id",
            "two",
            "state=m",
            r#"following_state={"id":"k"}"#,
            "dur=5m",
        ],
        &db,
    )
    .unwrap();
    assert_eq!(last(&db, "g"), (Some(json!("m")), Some(json!("k"))));
}

fn v2_backup() -> tempfile::NamedTempFile {
    let doc = |n: u32, state: JsonValue| {
        json!({"_id": format!("s:2020-03-0{n}T09:00:00.000Z"),
         "data": {"field": format!("s{n}"), "state": state, "dur": null,
                  "occurTime": {"utc": format!("2020-03-0{n}T09:00:00.000Z"), "o": 0, "tz": "UTC"}},
         "meta": {"humanId": format!("s{n}"),
                  "createTime": {"utc": format!("2020-03-0{n}T09:00:01.000Z"), "o": 0, "tz": "UTC"}}})
    };
    let backup = json!({"backupTime": "2025-02-02T12:26:32.488Z", "docs": [
        doc(1, json!({"id": "done"})),
        doc(2, json!(["a", {"id": "b"}])),
        doc(3, json!({"id": "x", "n": 1})),
        doc(4, JsonValue::Null),
    ]});
    let mut f = tempfile::NamedTempFile::new().unwrap();
    write!(f, "{backup}").unwrap();
    f
}

#[test]
fn v2import_normalizes_id_states_per_array_element() {
    let db = fresh_db();
    let f = v2_backup();
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    assert_eq!(last(&db, "s1").0, Some(json!("done")));
    assert_eq!(last(&db, "s2").0, Some(json!(["a", "b"])));
    assert_eq!(last(&db, "s3").0, Some(json!({"id": "x", "n": 1})));
    assert_eq!(last(&db, "s4").0, Some(json!(IGNORED_STATE_VALUE)));
}

/// `--dry-run` says how many states it would collapse — the `{id}` and
/// the array; not the structured object, not the null.
#[test]
fn v2import_dry_run_reports_the_normalized_count() {
    let f = v2_backup();
    let dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("t.db"))
        .env("NO_COLOR", "1")
        .args(["db", "v2import", "--dry-run", f.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("states normalized: 2 ")),
        "{stdout}"
    );
}
