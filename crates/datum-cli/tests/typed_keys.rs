//! da-b2f8o: a typed key parses as its type or refuses at log time, and
//! writes nothing. Typed by the field's spec (`value_type` number, string,
//! bool, date, datetime) or by the key's suffix (`*time`, `*date`,
//! `*dur`/`*duration`, `*num`/`*number`). Until this bead an unparseable
//! value on a time or date key was stored as typed; v2 refused it.

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::{fresh_db, logged_entries};
use datum_parse::time::DatumTime;
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
        ("at=2026-09-28T10:00Z", json!("2026-09-28T10:00:00Z")),
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
    // A `*_time` value is an instant {utc, o, tz} (da-x15of).
    let instant = |kv| DatumTime::from_v2_json(&log(&db, "task", kv).unwrap()).expect(kv);
    let wake = instant("wake_time=07:15").local_time().naive_local();
    assert_eq!(wake.to_string(), "2026-10-07 07:15:00");
    let next = instant("next_time=2026-05-03T00:00:00Z").utc;
    assert_eq!(next.to_rfc3339(), "2026-05-03T00:00:00+00:00");
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

/// `update` parses what a person typed, so its field's spec types it as a
/// log line is typed, and a refusal leaves the entry as it was (da-60n1b).
/// A relative value counts from the entry's own time, as on its log line.
/// A rekey's values are key names, not typed by the spec.
#[test]
fn update_types_a_key_by_its_target_fields_spec() {
    let db = typed_db();
    log(&db, "run", "km=5").unwrap();
    let data = |db: &Db| logged_entries(db, "run")[0].data.clone().unwrap();
    let err = run_cli(&["update", "run:*", "km=abc"], &db).unwrap_err();
    assert_eq!(format!("{err:#}"), r#"km: "abc" is not a number"#);
    assert_eq!(data(&db)["km"], json!(5));
    run_cli(&["update", "run:*", "code=1e5"], &db).unwrap();
    assert_eq!(data(&db)["code"], json!("1e5"));
    // Logged on 2026-10-07 at noon.
    run_cli(&["update", "run:*", "on=yesterday", "due_date=+3"], &db).unwrap();
    assert_eq!(data(&db)["on"], json!("2026-10-06"));
    assert_eq!(data(&db)["due_date"], json!("2026-10-10"));
    run_cli(&["rekey", "run:*", "km=dist"], &db).unwrap();
    assert_eq!(data(&db)["dist"], json!(5));
}

/// A timeless entry's update counts from its create time, as its log line did.
#[test]
fn update_of_a_timeless_entry_counts_from_its_create_time() {
    let db = typed_db();
    run_cli(&["occur", "run", "-T", "on=+0"], &db).unwrap();
    let entry = || logged_entries(&db, "run")[0].clone();
    let logged = entry().data.unwrap()["on"].as_str().unwrap().to_string();
    let day: chrono::NaiveDate = logged.parse().unwrap();
    run_cli(&["update", entry().hid.as_ref(), "on=+1"], &db).unwrap();
    let want = (day + chrono::Duration::days(1)).to_string();
    assert_eq!(entry().data.unwrap()["on"], json!(want));
}

/// `lift` with one key of each list type (da-jhi0x).
const LIFT: &str = r#"{"field": "lift", "verb": "occur", "subfields": {
    "reps": {"value_type": "number[]"},
    "tags": {"value_type": "string[]"},
    "done": {"value_type": "bool[]"},
    "needs": {"value_type": "ref[]"}
}}"#;

/// A list-typed key always splits on a glued comma, a single value is a
/// one-item list, the bracket form works, and each element parses as its
/// type or the whole value is refused, naming the element. A bracket value
/// that is not JSON is refused, never stored as fragments (da-igesg), with
/// an example in the key's element type that itself parses (da-80ox9).
#[test]
fn a_list_typed_key_parses_each_element_or_refuses() {
    let db = fresh_db();
    run_cli(&["spec", "set", "lift", "--from-json", LIFT], &db).unwrap();
    for (kv, want) in [
        ("reps=8,8,6", Ok(json!([8, 8, 6]))),
        ("reps=8", Ok(json!([8]))),
        ("reps=[8, 6]", Ok(json!([8, 6]))),
        ("reps=8,", Ok(json!([8]))),
        ("reps=[]", Ok(json!([]))),
        ("reps=[8,]", Ok(json!([8]))),
        (
            "reps=[8,x]",
            Err(
                r#"reps: "[8,x]" is not a number[]: the bracket form is JSON: [8,6], or write 8,6"#,
            ),
        ),
        (
            "tags=[a,b]",
            Err(
                r#"tags: "[a,b]" is not a string[]: the bracket form is JSON: ["a","b"], or write a,b"#,
            ),
        ),
        (
            "needs=[issue:a]",
            Err(
                r#"needs: "[issue:a]" is not a ref[]: the bracket form is JSON: ["issue:a","issue:b"], or write issue:a,issue:b"#,
            ),
        ),
        (
            "done=[true,x]",
            Err(
                r#"done: "[true,x]" is not a bool[]: the bracket form is JSON: [true,false], or write true,false"#,
            ),
        ),
        ("done=[true,false]", Ok(json!([true, false]))),
        (
            "reps=8,x,6",
            Err(r#"reps: "8,x,6" is not a number[]: element 2 "x" is not a number"#),
        ),
        (
            "reps=8, 6",
            Err(r#"reps: "8, 6" is not a number[]: element 1 "8, 6" is not a number"#),
        ),
        ("tags=a,b", Ok(json!(["a", "b"]))),
        ("tags=a, b", Ok(json!(["a, b"]))),
        ("tags=007", Ok(json!(["007"]))),
        (r#"tags=["a,b", 1]"#, Ok(json!(["a,b", "1"]))),
        ("done=true,FALSE", Ok(json!([true, false]))),
        (
            "done=true,yes",
            Err(r#"done: "true,yes" is not a bool[]: element 2 "yes" is not a bool"#),
        ),
        ("needs=issue:a,issue:b", Ok(json!(["issue:a", "issue:b"]))),
        (
            r#"needs=["issue:a","issue:b"]"#,
            Ok(json!(["issue:a", "issue:b"])),
        ),
    ] {
        assert_eq!(log(&db, "lift", kv), want.map_err(String::from), "{kv}");
    }
}

/// Each list type goes through `spec set`, a kit's export and its install,
/// and the installed kit logs; a `ref[]` element refers as a bare `ref`
/// value does.
#[test]
fn list_types_round_trip_through_a_kit() {
    let db = fresh_db();
    run_cli(&["spec", "set", "lift", "--from-json", LIFT], &db).unwrap();
    let spec = db.load_spec("lift").unwrap().unwrap();
    let toml = datum_core::kit::KitToml::from_field_specs("lifts", None, &[spec], &[])
        .to_toml()
        .unwrap();
    for ty in ["number[]", "string[]", "bool[]", "ref[]"] {
        assert!(toml.contains(&format!("value_type = \"{ty}\"")), "{toml}");
    }
    let dir = tempfile::tempdir().unwrap();
    let kit = dir.path().join("lifts.toml");
    std::fs::write(&kit, toml).unwrap();
    let installed = fresh_db();
    run_cli(&["kit", "install", &kit.to_string_lossy()], &installed).unwrap();
    let back = installed.load_spec("lift").unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&back.subfields).unwrap(),
        serde_json::to_value(&db.load_spec("lift").unwrap().unwrap().subfields).unwrap()
    );
    assert_eq!(log(&installed, "lift", "reps=5,5,3"), Ok(json!([5, 5, 3])));
    run_cli(&["update", "lift:*", "reps=5,5"], &installed).unwrap();
    let err = run_cli(&["update", "lift:*", "reps=5,x"], &installed).unwrap_err();
    assert_eq!(
        format!("{err:#}"),
        r#"reps: "5,x" is not a number[]: element 2 "x" is not a number"#
    );
    let data = logged_entries(&installed, "lift")[0].data.clone().unwrap();
    assert_eq!(data["reps"], json!([5, 5]));
    assert_eq!(
        datum_core::doc_ref::data_refs(Some(&back), "needs", &json!(["issue:a", "issue:b"])),
        ["issue:a", "issue:b"]
    );
}

/// A type declared over history makes `datum check` list the old values
/// that do not fit, and the update `datum help spec` gives clears one, run
/// as the page spells it (da-zetes).
#[test]
fn the_spec_help_update_clears_an_old_value_a_later_type_lists() {
    let db = fresh_db();
    // A lone value is not a list (da-jhi0x).
    log(&db, "lift", "reps=8").unwrap();
    run_cli(&["spec", "set", "lift", "--from-json", LIFT], &db).unwrap();
    let mistyped = |db: &Db| {
        let entries = db.list_including_deleted().unwrap();
        let report = datum_commands::commands::check::collect_issues(db, &entries).unwrap();
        report
            .issues
            .iter()
            .any(|i| i.category == "mistyped_data_value")
    };
    assert!(mistyped(&db), "check lists the old value");
    let page = datum_parse::help::command_help("spec");
    let at = page
        .find("`datum update <hid> ")
        .expect("no update on spec --help");
    let hid = logged_entries(&db, "lift")[0].hid.0.clone();
    let argv: Vec<&str> = page[at + 1..]
        .split('`')
        .next()
        .unwrap()
        .split_whitespace()
        .skip(1)
        .map(|w| if w == "<hid>" { hid.as_str() } else { w })
        .collect();
    run_cli(&argv, &db).unwrap();
    assert_eq!(
        logged_entries(&db, "lift")[0].data.as_ref().unwrap()["reps"],
        json!([8])
    );
    assert!(!mistyped(&db), "check is clean after the update");
}
