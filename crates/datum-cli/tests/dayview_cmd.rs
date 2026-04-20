//! `datum dayview` CLI tests (C5).

mod common;

use chrono::{Local, TimeZone, Utc};
use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, fresh_db};

/// Pick a UTC timestamp that lands on the same local civic date for both UTC
/// and local. Using local-noon avoids any timezone-edge surprises in CI.
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
        .verb(Verb::Start)
        .time_start_utc(noon)
        .time_end_utc(noon + chrono::Duration::hours(1))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(
        &["dayview", "--output", &path, "--width", "1200", "--height", "120"],
        &db,
    )
    .expect("dayview dispatch");

    let svg = std::fs::read_to_string(&path).expect("read svg");
    assert!(svg.starts_with("<svg "), "expected SVG, got: {svg}");
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("workout"), "field label must appear");
    assert!(svg.contains(r#"class="block""#), "block rect must render");
    assert!(svg.contains(r#"width="1200""#));
}

#[test]
fn dayview_json_dumps_layout() {
    let db = fresh_db();

    let noon = local_noon_today_utc();
    entry_builder("coffee")
        .verb(Verb::Occur)
        .value(serde_json::json!(1))
        .time_start_utc(noon)
        .insert(&db);
    entry_builder("workout")
        .verb(Verb::Start)
        .time_start_utc(noon + chrono::Duration::hours(1))
        .time_end_utc(noon + chrono::Duration::hours(2))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(&["dayview", "--json", "--output", &path], &db)
        .expect("dayview --json dispatch");

    let raw = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    // Top-level shape mirrors datum_layout::Layout.
    assert!(json.get("width").is_some());
    assert!(json.get("height").is_some());
    assert!(json.get("day_start_ms").is_some());
    assert!(json.get("day_end_ms").is_some());

    let fields = json["fields"].as_array().expect("fields is array");
    assert_eq!(fields.len(), 2, "two distinct fields");
    let names: Vec<&str> = fields
        .iter()
        .map(|f| f["field"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"coffee"));
    assert!(names.contains(&"workout"));

    let axis = json["axis_ticks"].as_array().expect("axis_ticks is array");
    assert!(!axis.is_empty(), "axis ticks present");
}
