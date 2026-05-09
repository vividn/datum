//! `datum dayview` CLI tests (datum-parity multi-day visualization).

mod common;

use chrono::{Local, TimeZone, Utc};
use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, fresh_db};

/// A UTC timestamp at local noon today — guaranteed to land in the same
/// civic day under any timezone CI runs in.
fn local_noon_today_utc() -> chrono::DateTime<Utc> {
    let today = Local::now().date_naive();
    Local
        .from_local_datetime(&today.and_hms_opt(12, 0, 0).unwrap())
        .single()
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn dayview_writes_svg_to_output() {
    let db = fresh_db();

    let noon = local_noon_today_utc();
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(noon)
        .time_end_utc(noon + chrono::Duration::hours(1))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(
        &[
            "dayview",
            "-o",
            &path,
            "--horizontal",
            "--width",
            "1200",
            "--day-height",
            "120",
        ],
        &db,
    )
    .expect("dayview dispatch");

    let svg = std::fs::read_to_string(&path).expect("read svg");
    // .svg output is wrapped with the XML declaration.
    assert!(svg.starts_with("<?xml"), "expected XML decl, got: {svg}");
    assert!(svg.contains("<svg "));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains(r#"data-field="workout""#));
    assert!(svg.contains(r#"class="block""#));
    assert!(svg.contains(r#"width="1200""#));
    // Today's row should have the now-line.
    assert!(svg.contains("current-time-line"));
    // 3-line date label rendered.
    assert!(svg.contains("date-label"));
}

#[test]
fn dayview_json_dumps_layout() {
    let db = fresh_db();

    let noon = local_noon_today_utc();
    entry_builder("coffee")
        .verb(Verb::Point)
        .value(serde_json::json!(1))
        .time_start_utc(noon)
        .insert(&db);
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(noon + chrono::Duration::hours(1))
        .time_end_utc(noon + chrono::Duration::hours(2))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(&["dayview", "--json", "-o", &path], &db).expect("dayview --json dispatch");

    let raw = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    // Top-level shape mirrors datum_layout::dayview::DayviewLayout.
    assert!(json.get("width").is_some());
    assert!(json.get("height").is_some());
    assert!(json.get("days").is_some(), "days array present");
    assert!(json.get("axis_ticks").is_some());
    assert!(json.get("label_font_size").is_some());

    let days = json["days"].as_array().expect("days is array");
    assert_eq!(days.len(), 1, "default one-day window");
    let fields = days[0]["fields"].as_array().expect("day fields is array");
    let names: Vec<&str> = fields
        .iter()
        .map(|f| f["field"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"coffee"));
    assert!(names.contains(&"workout"));

    let axis = json["axis_ticks"].as_array().expect("axis_ticks is array");
    assert_eq!(axis.len(), 25, "25 hourly ticks (00..24 inclusive)");
}

#[test]
fn dayview_n_days_stacks_multiple_rows() {
    let db = fresh_db();

    let noon = local_noon_today_utc();
    // Three entries, three different days.
    for day_offset in 0..3 {
        let t = noon - chrono::Duration::days(day_offset);
        entry_builder("workout")
            .verb(Verb::Switch)
            .time_start_utc(t)
            .time_end_utc(t + chrono::Duration::hours(1))
            .insert(&db);
    }

    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(&["dayview", "--json", "--n-days", "3", "-o", &path], &db)
        .expect("dayview --n-days dispatch");

    let raw = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let days = json["days"].as_array().expect("days array");
    assert_eq!(days.len(), 3);

    // Only the most recent day (today) should have the now-line.
    assert!(
        days[0]["now_time_pos"].is_null(),
        "day 0 (oldest) has no now"
    );
    assert!(days[1]["now_time_pos"].is_null(), "day 1 has no now");
    assert!(days[2]["now_time_pos"].is_number(), "day 2 (today) has now");
}

#[test]
fn dayview_horizontal_orientation_round_trips_to_layout() {
    let db = fresh_db();

    let noon = local_noon_today_utc();
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(noon)
        .time_end_utc(noon + chrono::Duration::hours(1))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(&["dayview", "--horizontal", "--json", "-o", &path], &db)
        .expect("dayview --horizontal dispatch");

    let raw = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    assert_eq!(json["orientation"], "horizontal");
}

#[test]
fn dayview_vertical_orientation_explicit_flag() {
    let db = fresh_db();
    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(
        &[
            "dayview",
            "--orientation",
            "vertical",
            "--json",
            "-o",
            &path,
        ],
        &db,
    )
    .expect("dayview --orientation vertical dispatch");

    let raw = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    assert_eq!(json["orientation"], "vertical");
}

#[test]
fn dayview_skips_empty_fields() {
    // No data at all → no fields anywhere in the layout.
    let db = fresh_db();
    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(&["dayview", "--json", "-o", &path], &db).expect("dayview dispatch");

    let raw = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let days = json["days"].as_array().unwrap();
    assert_eq!(days.len(), 1);
    assert!(
        days[0]["fields"].as_array().unwrap().is_empty(),
        "no empty fields injected"
    );
}

#[test]
fn dayview_html_output_wraps_svg() {
    let db = fresh_db();
    let noon = local_noon_today_utc();
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(noon)
        .time_end_utc(noon + chrono::Duration::hours(1))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".html").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(&["dayview", "-o", &path], &db).expect("dayview html dispatch");

    let html = std::fs::read_to_string(&path).unwrap();
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<svg "));
    assert!(html.contains("</svg>"));
}
