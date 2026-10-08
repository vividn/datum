//! da-b2f8o: a typed key parses as its type or refuses at log time, and
//! writes nothing. Typed by the field's spec (`value_type` number, string,
//! bool, date, datetime) or by the key's suffix (`*time`, `*date`,
//! `*dur`/`*duration`, `*num`/`*number`). Until this bead an unparseable
//! value on a time or date key was stored as typed; v2 refused it.

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::{json, Value};

/// `run` with one key of each spec type.
fn typed_db() -> Db {
    let db = fresh_db();
    let spec = json!({"field": "run", "verb": "occur", "subfields": {
        "km": {"value_type": "number"},
        "code": {"value_type": "string"},
        "ok": {"value_type": "bool"},
        "on": {"value_type": "date"},
        "at": {"value_type": "datetime"},
    }});
    run_cli(
        &["spec", "set", "run", "--from-json", &spec.to_string()],
        &db,
    )
    .unwrap();
    db
}

/// The value `key=value` stores on `field`, logged on 2026-10-07 at noon,
/// or the refusal's message — which must leave nothing written.
fn log(db: &Db, field: &str, kv: &str) -> Result<Value, String> {
    let before = logged_entries(db, field).len();
    // One minute each, so no two entries share a slug.
    let at = format!("2026-10-07T12:{before:02}");
    match run_cli(&["occur", field, kv, "-t", &at], db) {
        Ok(()) => {
            let entries = logged_entries(db, field);
            let latest = entries
                .iter()
                .max_by_key(|e| e.occur_time.as_ref().map(|t| t.utc));
            let data = latest.unwrap().data.clone().unwrap();
            Ok(data[kv.split_once('=').unwrap().0].clone())
        }
        Err(e) => {
            assert_eq!(logged_entries(db, field).len(), before, "{kv} wrote");
            Err(format!("{e:#}"))
        }
    }
}

#[test]
fn a_spec_typed_key_stores_its_type() {
    let db = typed_db();
    for (kv, want) in [
        ("km=5.2", json!(5.2)),
        ("km=1_000.5", json!(1000.5)),
        ("code=007", json!("007")),
        ("code=1e5", json!("1e5")),
        ("code=a,b", json!("a,b")),
        ("ok=TRUE", json!(true)),
        ("on=yesterday", json!("2026-10-06")),
        ("on=2026-09-28", json!("2026-09-28")),
        ("at=9am", json!("2026-10-07T09:00:00")),
        ("at=yesterday 10am", json!("2026-10-06T10:00:00")),
        ("at=2026-09-28T10:00+02:00", json!("2026-09-28T08:00:00Z")),
    ] {
        assert_eq!(log(&db, "run", kv), Ok(want), "{kv}");
    }
}

#[test]
fn a_spec_typed_key_refuses_what_is_not_its_type() {
    let db = typed_db();
    for (kv, want) in [
        (
            "km=5,2",
            r#"km: "5,2" is not a number: a comma makes a list; a decimal is 5.2"#,
        ),
        ("km=,3", r#"km: ",3" is not a number: a comma makes a list"#),
        ("km=[1,2]", r#"km: "[1,2]" is not a number"#),
        ("km=abc", r#"km: "abc" is not a number"#),
        ("km=nan", r#"km: "nan" is not a number"#),
        ("ok=yes", r#"ok: "yes" is not a bool"#),
        ("on=tomorow", r#"on: "tomorow" is not a date"#),
        ("at=soonish", r#"at: "soonish" is not a datetime"#),
    ] {
        assert_eq!(log(&db, "run", kv), Err(want.to_string()), "{kv}");
    }
}

#[test]
fn a_suffix_typed_key_parses_or_refuses() {
    let db = fresh_db();
    for (kv, want) in [
        ("due_date=tomorrow", Ok(json!("2026-10-08"))),
        ("dueDate=+3", Ok(json!("2026-10-10"))),
        ("wake_time=07:15", Ok(json!("07:15:00"))),
        (
            "next_time=2026-05-03T00:00:00Z",
            Ok(json!("2026-05-03T00:00:00Z")),
        ),
        ("run_dur=1:30", Ok(json!("PT1H30M"))),
        ("km_num=3.5", Ok(json!(3.5))),
        ("stepNumber=7", Ok(json!(7))),
        (
            "due_date=tomorow",
            Err(r#"due_date: "tomorow" is not a date"#),
        ),
        (
            "wake_time=soonish",
            Err(r#"wake_time: "soonish" is not a time"#),
        ),
        ("run_dur=lots", Err(r#"run_dur: "lots" is not a duration"#)),
        (
            "km_num=3,5",
            Err(r#"km_num: "3,5" is not a number: a comma makes a list; a decimal is 3.5"#),
        ),
    ] {
        assert_eq!(log(&db, "task", kv), want.map_err(String::from), "{kv}");
    }
}

/// No boundary before the suffix is no type, an untyped key is guessed as
/// before, and a spec's type wins over the suffix.
#[test]
fn an_untyped_key_is_guessed_as_before() {
    let db = fresh_db();
    for (kv, want) in [
        ("duedate=tomorow", json!("tomorow")),
        ("enum=x", json!("x")),
        ("n=42", json!(42)),
        ("v=3,5", json!([3, 5])),
        ("v=1e5", json!(100000.0)),
    ] {
        assert_eq!(log(&db, "task", kv), Ok(want), "{kv}");
    }
    let spec = json!({"field": "f", "verb": "occur", "subfields": {
        "ref_num": {"value_type": "string"},
    }});
    run_cli(&["spec", "set", "f", "--from-json", &spec.to_string()], &db).unwrap();
    assert_eq!(log(&db, "f", "ref_num=0042x"), Ok(json!("0042x")));
}

/// The entry's own keys keep their own parsers: `dur=` takes an
/// uncertainty no data duration does.
#[test]
fn the_entrys_own_dur_keeps_its_parser() {
    let db = fresh_db();
    run_cli(&["start", "task", "dur=PT1H30M±PT5M"], &db).unwrap();
    let err = run_cli(&["start", "w", "dur=lots"], &db).unwrap_err();
    assert!(format!("{err:#}").contains("invalid dur value"), "{err:#}");
}

/// A required key refuses on a raw `occur`, not only through an alias's `-k`.
#[test]
fn a_missing_required_key_refuses_a_raw_occur() {
    let db = fresh_db();
    let spec = json!({"field": "run", "verb": "occur", "subfields": {
        "km": {"value_type": "number", "required": true},
    }});
    run_cli(
        &["spec", "set", "run", "--from-json", &spec.to_string()],
        &db,
    )
    .unwrap();
    let err = run_cli(&["occur", "run"], &db).unwrap_err();
    assert_eq!(format!("{err:#}"), "Missing required key: km");
    assert!(logged_entries(&db, "run").is_empty());
    assert_eq!(log(&db, "run", "km=4"), Ok(json!(4)));
}
