//! The dayview marks what `datum check integrity` finds (da-0lful): on
//! the day the flagged entry happened, not the day the warning was filed,
//! repeated states included, a resolved warning not.
#![allow(clippy::disallowed_types)] // native tests: `chrono::Local` is the host zone the binary reads too

use std::path::Path;
use std::process::Command;

use chrono::{Local, NaiveDate};
use datum_core::db::Db;
use datum_core::integrity::{IntegrityKind, ListFilter, Resolution};
use datum_core::time::Zone;
use serde_json::Value;
use tempfile::TempDir;

fn datum(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "datum {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn yesterday() -> NaiveDate {
    Local::now().date_naive() - chrono::Days::new(1)
}

/// `HH:MM` yesterday, as `-t` takes it and as the dayview's `data-at` says it.
fn at(hm: &str) -> (String, String) {
    let t = format!("{}T{hm}", yesterday());
    let naive = chrono::NaiveDateTime::parse_from_str(&format!("{t}:00"), "%Y-%m-%dT%H:%M:%S");
    let utc = Zone::host().resolve(naive.unwrap()).for_boundary();
    (t, utc.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

/// A day as [`marks`] reads it: its date, whether its label carries the
/// icon, and its errors as `(field, what, time_ms)`.
type Day = (String, bool, Vec<(String, String, i64)>);

/// Yesterday and today, as the dayview's JSON.
fn marks(db: &Path) -> Vec<Day> {
    let json: Value =
        serde_json::from_str(&datum(db, &["dayview", "--days", "2", "--json"])).unwrap();
    let days = json["days"].as_array().unwrap();
    days.iter()
        .map(|d| {
            let errors = d.get("errors").and_then(Value::as_array);
            let errors = errors.into_iter().flatten().map(|e| {
                let s = |k: &str| e[k].as_str().unwrap().to_string();
                (s("field"), s("what"), e["time_ms"].as_i64().unwrap())
            });
            let date = d["date"].as_str().unwrap().to_string();
            (date, d["has_error"].as_bool().unwrap(), errors.collect())
        })
        .collect()
}

fn ms(iso: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(iso)
        .unwrap()
        .timestamp_millis()
}

fn db_in(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("t.db")
}

#[test]
fn a_double_start_backfilled_onto_yesterday_marks_yesterday_not_today() {
    let dir = TempDir::new().unwrap();
    let db = db_in(&dir);
    let (ten, _) = at("10:00");
    let (eleven, eleven_utc) = at("11:00");
    datum(&db, &["start", "work", "-t", &ten]);
    datum(&db, &["start", "work", "-t", &eleven]);

    let today = Local::now().date_naive().to_string();
    let error = (
        "work".into(),
        "opened while already open".into(),
        ms(&eleven_utc),
    );
    assert_eq!(
        marks(&db),
        [
            (yesterday().to_string(), true, vec![error]),
            (today, false, vec![])
        ]
    );

    // In the picture: the day's icon by yesterday's label alone, and the
    // line, its label and the track's icon at 11:00.
    let svg = datum(&db, &["dayview", "--days", "2"]);
    let label = |day: usize| {
        let from = svg
            .find(&format!(r#"<g class="day-label" data-day="{day}""#))
            .unwrap();
        let end = from + svg[from..].find(r#"<g class="day-data""#).unwrap();
        svg[from..end].contains("day-error")
    };
    assert!(
        label(0) && !label(1),
        "the icon by yesterday's label only: {svg}"
    );
    assert_eq!(svg.matches(r#"class="integrity-error""#).count(), 1);
    assert!(svg.contains(&format!(
        r#"<g class="integrity-error" data-field="work" data-at="{eleven_utc}"><title>work: opened while already open</title>"#
    )));
    assert!(svg.contains(r#"class="error-line""#));
    assert!(svg.contains(r#"class="error-label""#));
    assert!(svg.contains(r#"class="error-track""#));
}

#[test]
fn a_repeated_state_alone_marks_its_day() {
    let dir = TempDir::new().unwrap();
    let db = db_in(&dir);
    let (nine, _) = at("09:00");
    let (half, half_utc) = at("09:30");
    datum(&db, &["switch", "mood", "happy", "-t", &nine]);
    datum(&db, &["switch", "mood", "happy", "-t", &half]);

    let days = marks(&db);
    let what = "switched into the state it was already in";
    assert_eq!(days[0].2, [("mood".into(), what.into(), ms(&half_utc))]);
    assert!(days[0].1 && !days[1].1);
}

#[test]
fn a_resolved_warning_marks_nothing() {
    let dir = TempDir::new().unwrap();
    let db = db_in(&dir);
    let (ten, _) = at("10:00");
    let (eleven, _) = at("11:00");
    datum(&db, &["start", "work", "-t", &ten]);
    datum(&db, &["start", "work", "-t", &eleven]);
    assert!(
        marks(&db)[0].1,
        "the control: the double start marks its day"
    );

    let open = Db::open(db.to_str().unwrap()).unwrap();
    let pending = ListFilter {
        kind: Some(IntegrityKind::DoubleOpen),
        pending: Some(true),
    };
    let warning = open.list_integrity_warnings(&pending).unwrap().remove(0);
    open.resolve_integrity_warning(&warning.hid, Resolution::KeepBoth)
        .unwrap();
    drop(open);

    assert!(
        marks(&db)
            .iter()
            .all(|(_, flagged, errors)| !flagged && errors.is_empty()),
        "{:?}",
        marks(&db)
    );
}

/// `check integrity` and the dayview read one function, so they name the
/// same entries: here a stored double start, an orphan end and a live
/// repeated state.
#[test]
fn check_integrity_and_the_dayview_agree() {
    let dir = TempDir::new().unwrap();
    let db = db_in(&dir);
    for (args, hm) in [
        (&["start", "work"][..], "10:00"),
        (&["start", "work"], "11:00"),
        (&["end", "walk"], "12:00"),
        (&["switch", "mood", "happy"], "09:00"),
        (&["switch", "mood", "happy"], "09:30"),
    ] {
        let t = at(hm).0;
        datum(&db, &[args, &["-t", &t]].concat());
    }

    let check: Value =
        serde_json::from_str(&datum(&db, &["check", "integrity", "--json"])).unwrap();
    let mut checked: Vec<String> = (check["issues"].as_array().unwrap().iter())
        .flat_map(|i| i["entries"].as_array().unwrap())
        .map(|e| e["field"].as_str().unwrap().to_string())
        .collect();
    let mut drawn: Vec<String> = (marks(&db).into_iter())
        .flat_map(|(_, _, errors)| errors)
        .map(|(field, _, _)| field)
        .collect();
    checked.sort();
    drawn.sort();
    assert_eq!(checked, ["mood", "walk", "work"]);
    assert_eq!(drawn, checked);
}
