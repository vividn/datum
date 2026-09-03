//! `datum dayview` CLI tests (datum-parity multi-day visualization).

mod common;

use chrono::{Local, Utc};
use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, fresh_db};
use datum_core::time::Zone;

/// A UTC timestamp at local noon today — guaranteed to land in the same
/// civic day under any timezone CI runs in.
fn local_noon_today_utc() -> chrono::DateTime<Utc> {
    let today = Local::now().date_naive();
    Zone::host()
        .resolve(today.and_hms_opt(12, 0, 0).unwrap())
        .for_boundary()
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
        .state(serde_json::json!(1))
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

// ─── da-a8rs: where a block LANDS, not just how wide it is ──────────────
//
// Nothing checked a rendered block's position until this bead: dur-blocks
// drew a whole duration early (`[occur - dur, occur]`) and every existing
// assertion — extent, colour, count — still passed. The arithmetic below
// is the check that was missing: `time_offset / time_axis_extent * 24` is
// the hour a block starts at, and it must be the hour it was logged at.

/// Yesterday's local date, `YYYY-MM-DD`. Both tests below log with `-y`
/// and render that day explicitly: a switch region ends at
/// `min(day_end, now)`, so a band logged "today" has no extent yet when
/// the suite runs before the logged hour. A finished day has no such
/// dependence on the wall clock.
fn yesterday() -> String {
    (Local::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string()
}

/// Render yesterday to JSON and return `(time_axis_extent, blocks)` for
/// `field`, each block as its `(time_offset, time_extent)`.
fn dayview_blocks(db: &datum_core::db::Db, field: &str) -> (f64, Vec<(f64, f64)>) {
    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let day = yesterday();
    run_cli(
        &[
            "dayview",
            "--start-date",
            &day,
            "--end-date",
            &day,
            "--width",
            "2400",
            "--json",
            "-o",
            &path,
        ],
        db,
    )
    .expect("dayview --json dispatch");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let axis = json["time_axis_extent"].as_f64().expect("time_axis_extent");
    let blocks = json["days"][0]["fields"]
        .as_array()
        .expect("fields array")
        .iter()
        .find(|f| f["field"].as_str() == Some(field))
        .unwrap_or_else(|| panic!("field {field} rendered"))["blocks"]
        .as_array()
        .expect("blocks array")
        .iter()
        .map(|b| {
            (
                b["time_offset"].as_f64().unwrap(),
                b["time_extent"].as_f64().unwrap(),
            )
        })
        .collect();
    (axis, blocks)
}

/// Hours-from-midnight a pixel offset stands for on a 24-hour axis.
fn hours(px: f64, axis: f64) -> f64 {
    px / axis * 24.0
}

/// `occur nap dur=2h -t 1400` draws at 14:00, not 12:00. `occur_time` is
/// the block's LEADING edge — the duration runs forward from it, the same
/// way `datum_core::state` puts a block's trailing event at `occur + dur`.
#[test]
fn dur_block_draws_at_its_own_hour_not_a_duration_early() {
    let db = fresh_db();
    run_cli(&["occur", "nap", "dur=2h", "-y", "-t", "1400"], &db).expect("occur with dur");

    let (axis, blocks) = dayview_blocks(&db, "nap");
    assert_eq!(blocks.len(), 1, "one block for one dur-bearing entry");
    let (offset, extent) = blocks[0];
    assert!(
        (hours(offset, axis) - 14.0).abs() < 0.02,
        "block starts at 14:00, drew at {:.2}:00",
        hours(offset, axis)
    );
    assert!(
        (hours(extent, axis) - 2.0).abs() < 0.02,
        "block spans 2h, spanned {:.2}h",
        hours(extent, axis)
    );
}

/// A block-switch's following-state band opens where the block ENDS. With
/// the block at [09:00, 10:00), the "focused" band runs 10:00 → midnight —
/// the two never overlap.
#[test]
fn block_switch_following_band_opens_at_the_blocks_trailing_edge() {
    let db = fresh_db();
    run_cli(
        &[
            "switch",
            "mood",
            "groggy",
            "following_state=focused",
            "dur=1h",
            "-y",
            "-t",
            "0900",
        ],
        &db,
    )
    .expect("block-switch with dur");

    let (axis, blocks) = dayview_blocks(&db, "mood");
    assert_eq!(blocks.len(), 2, "the block plus its following band");
    let mut spans: Vec<(f64, f64)> = blocks
        .iter()
        .map(|(o, e)| (hours(*o, axis), hours(o + e, axis)))
        .collect();
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert!(
        (spans[0].0 - 9.0).abs() < 0.02 && (spans[0].1 - 10.0).abs() < 0.02,
        "block is [09:00, 10:00), drew [{:.2}, {:.2}]",
        spans[0].0,
        spans[0].1
    );
    assert!(
        (spans[1].0 - 10.0).abs() < 0.02,
        "following band opens at 10:00, opened at {:.2}",
        spans[1].0
    );
    assert!(
        (spans[1].1 - 24.0).abs() < 0.02,
        "the band runs to midnight, ended at {:.2}",
        spans[1].1
    );
}
