//! `datum dayview` CLI tests (datum-parity multi-day visualization).
#![allow(clippy::disallowed_types)] // native tests: `chrono::Local` is the host zone the binary reads too

use crate::common;

use chrono::{Local, Utc};
use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, freeze_time, fresh_db};
use datum_core::time::{DatumTime, Zone};

/// Noon UTC `days_back` days ago. `time_start_utc` stamps a fixture `UTC`,
/// and dayview cuts its days, today among them, on the clock the record is
/// kept by (da-1v9gp): noon on the host's date is off UTC's today wherever
/// the two dates differ, as for two hours after a Berlin summer midnight
/// (da-indmg).
fn noon_utc(days_back: u64) -> chrono::DateTime<Utc> {
    (DatumTime::now().utc.date_naive() - chrono::Days::new(days_back))
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_utc()
}

#[test]
fn dayview_writes_svg_to_output() {
    let db = fresh_db();

    let noon = noon_utc(0);
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

    // Yesterday, drawn with `-d -1`: a state is drawn up to the now-line
    // (da-zxs), so coffee — a Point with a state, which is stored as a
    // switch — logged at today's noon has nothing to draw before noon
    // (da-qicfn).
    let noon = noon_utc(1);
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

    run_cli(&["dayview", "--json", "-d", "-1", "-o", &path], &db).expect("dayview --json dispatch");

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

    let noon = noon_utc(0);
    // Three entries, three different days.
    for day_offset in 0..3 {
        let t = noon - chrono::Duration::days(day_offset);
        entry_builder("workout")
            .verb(Verb::Switch)
            .time_start_utc(t)
            .time_end_utc(t + chrono::Duration::hours(1))
            .insert(&db);
    }

    // `--n-days` is the old spelling, still read for the argv saved layouts hold.
    for flag in [
        &["--days", "3"][..],
        &["-n", "3"],
        &["--days=3"],
        &["--n-days", "3"],
    ] {
        let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let mut argv = vec!["dayview", "--json", "-o", &path];
        argv.extend_from_slice(flag);
        run_cli(&argv, &db).unwrap_or_else(|e| panic!("dayview {flag:?}: {e}"));
        let raw = std::fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            json["days"].as_array().expect("days array").len(),
            3,
            "{flag:?}"
        );
    }

    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(&["dayview", "--json", "--days", "3", "-o", &path], &db)
        .expect("dayview --days dispatch");
    let raw = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let days = json["days"].as_array().expect("days array");

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

    let noon = noon_utc(0);
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

/// The pivot really re-draws (da-a6guc): over the same db, horizontal is a
/// band wider than tall and vertical a column taller than wide. Reads the
/// SVG's own viewBox, so it fails if `--orientation` stops reaching the
/// renderer, not merely if it stops reaching the layout JSON.
#[test]
fn dayview_orientation_swaps_the_viewbox_shape() {
    let db = fresh_db();
    let view_box = |orientation: &str| {
        let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        run_cli(&["dayview", "--orientation", orientation, "-o", &path], &db)
            .expect("dayview dispatch");
        let svg = std::fs::read_to_string(&path).unwrap();
        let vb = svg.split("viewBox=\"").nth(1).expect("a viewBox");
        let dims: Vec<f64> = vb[..vb.find('"').unwrap()]
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        (dims[2], dims[3])
    };
    let (hw, hh) = view_box("horizontal");
    let (vw, vh) = view_box("vertical");
    assert!(hw > hh, "horizontal is {hw}x{hh}, not a wide band");
    assert!(vh > vw, "vertical is {vw}x{vh}, not a tall column");
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
    let noon = noon_utc(0);
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

/// `occur nap dur=2h -t 1400` draws [12:00, 14:00) — the nap LASTED two
/// hours up to 14:00. `occur_time` is the block's TRAILING edge and the
/// duration precedes it, the original datum's `durationBlocks` reading
/// and the one `datum_core::state` compiles by (da-zwct).
#[test]
fn dur_block_draws_the_two_hours_before_its_occur_time() {
    let db = fresh_db();
    run_cli(&["occur", "nap", "dur=2h", "-y", "-t", "1400"], &db).expect("occur with dur");

    let (axis, blocks) = dayview_blocks(&db, "nap");
    assert_eq!(blocks.len(), 1, "one block for one dur-bearing entry");
    let (offset, extent) = blocks[0];
    assert!(
        (hours(offset, axis) - 12.0).abs() < 0.02,
        "block starts at 12:00 — two hours before its occur_time — drew at {:.2}:00",
        hours(offset, axis)
    );
    assert!(
        (hours(offset + extent, axis) - 14.0).abs() < 0.02,
        "block ends AT its occur_time 14:00, ended at {:.2}:00",
        hours(offset + extent, axis)
    );
    assert!(
        (hours(extent, axis) - 2.0).abs() < 0.02,
        "block spans 2h, spanned {:.2}h",
        hours(extent, axis)
    );
}

/// A block-switch's following-state band opens where the block ENDS —
/// which under da-zwct is the stored `occur_time` itself. Logged at
/// 09:00 with `dur=1h`, "groggy" held [08:00, 09:00) and the "focused"
/// band runs 09:00 → midnight; the two never overlap.
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
        (spans[0].0 - 8.0).abs() < 0.02 && (spans[0].1 - 9.0).abs() < 0.02,
        "block is [08:00, 09:00), drew [{:.2}, {:.2}]",
        spans[0].0,
        spans[0].1
    );
    assert!(
        (spans[1].0 - 9.0).abs() < 0.02,
        "following band opens at 09:00 — the block's occur_time — opened at {:.2}",
        spans[1].0
    );
    assert!(
        (spans[1].1 - 24.0).abs() < 0.02,
        "the band runs to midnight, ended at {:.2}",
        spans[1].1
    );
}

/// da-zwct, measured the way vividn measured it: log `start posture 4` and
/// look at where the drawing puts it. The block must END on the now-line
/// — the four minutes it lasted lie behind it — where the forward
/// reading drew it running four minutes into the future.
#[test]
fn a_just_logged_dur_block_ends_on_the_now_line() {
    // Midday: in the first four minutes of a day the block would straddle
    // midnight into a day this one-day render does not draw (da-tyggo).
    let _g = freeze_time("2026-09-10T12:00:00");
    let db = fresh_db();
    run_cli(&["start", "posture", "4"], &db).expect("start posture 4");

    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(&["dayview", "--width", "2400", "--json", "-o", &path], &db)
        .expect("dayview --json dispatch");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

    let axis = json["time_axis_extent"].as_f64().expect("time_axis_extent");
    let day = json["days"]
        .as_array()
        .expect("days array")
        .last()
        .expect("today is the last day rendered");
    let now_pos = day["now_time_pos"]
        .as_f64()
        .expect("today carries the now-line");
    let block = day["fields"]
        .as_array()
        .expect("fields array")
        .iter()
        .find(|f| f["field"].as_str() == Some("posture"))
        .expect("posture rendered")["blocks"]
        .as_array()
        .expect("blocks array")[0]
        .clone();
    let offset = block["time_offset"].as_f64().unwrap();
    let extent = block["time_extent"].as_f64().unwrap();

    // A second's worth of pixels, as slack for float rounding: the clock
    // is frozen, so the entry and the render read the same instant.
    let one_second = axis / (24.0 * 60.0 * 60.0);
    assert!(
        (offset + extent - now_pos).abs() < 2.0 * one_second,
        "the block must END on the now-line: ends at {}, now-line at {now_pos}",
        offset + extent,
    );
    assert!(
        offset < now_pos - one_second,
        "and it must lie BEHIND the now-line, not start on it: starts at {offset}"
    );
    assert!(
        (hours(extent, axis) * 60.0 - 4.0).abs() < 0.05,
        "four minutes of it, spanned {:.2}m",
        hours(extent, axis) * 60.0
    );
}

/// The x of the first `<g class="day-data">` band — `data-lo` is the
/// leading edge of the data area, the same number `hours[0].x` reports.
fn first_band_x(svg: &str) -> f64 {
    let at = svg.find(r#"class="day-data""#).expect("a day band");
    let lo = svg[at..].find("data-lo=\"").expect("data-lo") + at + 9;
    let end = svg[lo..].find('"').unwrap() + lo;
    svg[lo..end].parse().expect("data-lo is a number")
}

/// `--label-width 0 --time-axis-height 0` — bands only, edge to edge, with
/// the gridlines still drawn (da-55ru). This is what the web asks for when
/// it draws its own date and axis labels.
#[test]
fn dayview_label_width_zero_drops_the_date_labels() {
    let db = fresh_db();
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(noon_utc(0))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(
        &[
            "dayview",
            "-n",
            "3",
            "--label-width",
            "0",
            "--time-axis-height",
            "0",
            "--margin",
            "2",
            "-o",
            &path,
        ],
        &db,
    )
    .expect("dayview --label-width 0");

    let svg = std::fs::read_to_string(&path).unwrap();
    assert!(!svg.contains("date-label"), "no date labels: {svg}");
    assert!(!svg.contains("day-label"), "and no label column at all");
    assert!(!svg.contains("tick-label"), "no axis labels either");
    assert!(svg.contains(r#"class="gridline""#), "gridlines still drawn");
    assert_eq!(
        first_band_x(&svg),
        2.0,
        "the bands start at the margin, not after a label column"
    );
}

/// `--label-width 120` — the column is exactly 120 wide, the bands start
/// after it, and the date labels sit inside it.
#[test]
fn dayview_label_width_sets_the_date_column() {
    let db = fresh_db();
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(noon_utc(0))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(
        &[
            "dayview",
            "--label-width",
            "120",
            "--margin",
            "2",
            "-o",
            &path,
        ],
        &db,
    )
    .expect("dayview --label-width 120");

    let svg = std::fs::read_to_string(&path).unwrap();
    assert_eq!(first_band_x(&svg), 122.0, "bands start at 120 + margin");
    // Right-aligned at the column's inner edge, so every label fits in 120.
    assert!(
        svg.contains(r#"<text class="date-label" x="116""#),
        "date labels sit inside the 120px column: {svg}"
    );
}

/// `--show json` carries the geometry a caller drawing its own labels
/// needs: the resolved `label_width`, the day pitch, and the SVG x of
/// every hour gridline — the same x the SVG draws them at.
#[test]
fn dayview_json_carries_the_label_geometry() {
    let db = fresh_db();
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(noon_utc(0))
        .insert(&db);

    let svg_file = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let json_file = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let argv = |out: &str| {
        vec![
            "dayview".to_string(),
            "-n".into(),
            "3".into(),
            "--label-width".into(),
            "0".into(),
            "--time-axis-height".into(),
            "0".into(),
            "-o".into(),
            out.to_string(),
        ]
    };
    let svg_argv = argv(svg_file.path().to_str().unwrap());
    run_cli(
        &svg_argv.iter().map(String::as_str).collect::<Vec<_>>(),
        &db,
    )
    .expect("dayview svg");
    let mut json_argv = argv(json_file.path().to_str().unwrap());
    json_argv.push("--show".into());
    json_argv.push("json".into());
    run_cli(
        &json_argv.iter().map(String::as_str).collect::<Vec<_>>(),
        &db,
    )
    .expect("dayview --show json");

    let svg = std::fs::read_to_string(svg_file.path()).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(json_file.path()).unwrap()).unwrap();

    assert_eq!(json["label_width"].as_f64(), Some(0.0));
    assert_eq!(json["day_height"].as_f64(), Some(100.0));
    assert_eq!(json["day_pitch"].as_f64(), Some(115.0), "band + gap");
    assert_eq!(json["margin"].as_f64(), Some(2.0));
    assert_eq!(json["time_axis_height"].as_f64(), Some(0.0));

    let hours = json["hours"].as_array().expect("hours array");
    assert_eq!(hours.len(), 25, "00..24 inclusive");
    assert_eq!(hours[0]["hour"].as_u64(), Some(0));
    let first_x = hours[0]["x"].as_f64().expect("hours[0].x");

    // The SVG is drawn inside `<g class="plot" transform="translate(m,m)">`,
    // so the gridline's own x1 plus the margin is what `hours[].x` reports.
    let at = svg.find(r#"<line class="gridline""#).expect("a gridline");
    let x1 = svg[at..].split("x1=\"").nth(1).unwrap();
    let x1: f64 = x1[..x1.find('"').unwrap()].parse().unwrap();
    assert_eq!(
        first_x,
        x1 + json["margin"].as_f64().unwrap(),
        "hours[0].x is the first gridline's x in the SVG"
    );

    // Every day carries its own date and the y its band is drawn at.
    let days = json["days"].as_array().expect("days");
    assert_eq!(days.len(), 3);
    for (i, day) in days.iter().enumerate() {
        assert!(day["date"].as_str().is_some(), "day {i} has an ISO date");
        assert_eq!(
            day["y"].as_f64(),
            Some(2.0 + i as f64 * 115.0),
            "day {i} sits a pitch below the one above it"
        );
    }
}

/// da-6gvw: `end work 5` stores a Block whose state is `false` — the
/// field was OFF for those five minutes. It must be a HOLE: nothing
/// painted, and the `work` block that encloses it cut in two.
#[test]
fn a_false_dur_block_is_a_hole_cut_out_of_the_enclosing_block() {
    let db = fresh_db();
    run_cli(&["start", "work", "-y", "-t", "0900"], &db).expect("start");
    run_cli(&["end", "work", "5", "-y", "-t", "1000"], &db).expect("end with dur");
    run_cli(&["end", "work", "-y", "-t", "1200"], &db).expect("end");

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
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
            "--label-width",
            "0",
            "-o",
            &path,
        ],
        &db,
    )
    .expect("dayview dispatch");
    let svg = std::fs::read_to_string(&path).expect("read svg");

    let states: Vec<&str> = svg
        .match_indices(r#"<rect class="block""#)
        .map(|(i, _)| {
            let rect = &svg[i..i + svg[i..].find('>').unwrap()];
            let at = rect.find("data-state=\"").expect("data-state") + 12;
            &rect[at..at + rect[at..].find('"').unwrap()]
        })
        .collect();
    assert_eq!(
        states,
        vec!["on", "on"],
        "two halves of the `work` block and nothing else"
    );

    let titles: Vec<&str> = svg
        .match_indices("<title>")
        .map(|(i, _)| {
            let rest = &svg[i + 7..];
            &rest[..rest.find("</title>").unwrap()]
        })
        .collect();
    assert_eq!(
        titles,
        vec!["work: on 09:00–09:55", "work: on 10:00–12:00"],
        "the hole runs [occur − dur, occur] and both halves keep the region's label"
    );
}

/// Local 10:00, `days` days before yesterday, as UTC.
fn ten_am_days_before_yesterday(days: i64) -> chrono::DateTime<Utc> {
    let date = Local::now().date_naive() - chrono::Duration::days(1 + days);
    Zone::host()
        .resolve(date.and_hms_opt(10, 0, 0).unwrap())
        .for_boundary()
}

fn switch_at(db: &datum_core::db::Db, field: &str, state: serde_json::Value, days: i64) {
    entry_builder(field)
        .verb(Verb::Switch)
        .state(state)
        .time_start_utc(ten_am_days_before_yesterday(days))
        .insert(db);
}

/// The fields each day of `dayview --days <n>` ending yesterday draws.
fn fields_per_day(db: &datum_core::db::Db, n: u32) -> Vec<Vec<String>> {
    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let n = n.to_string();
    run_cli(
        &[
            "dayview",
            "--end-date",
            &yesterday(),
            "--days",
            &n,
            "--json",
            "-o",
            &path,
        ],
        db,
    )
    .expect("dayview --json dispatch");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    json["days"]
        .as_array()
        .expect("days array")
        .iter()
        .map(|d| {
            d["fields"]
                .as_array()
                .expect("fields array")
                .iter()
                .map(|f| f["field"].as_str().unwrap().to_string())
                .collect()
        })
        .collect()
}

/// da-w17jo: a state switched on long before the day — past the fetch's
/// two-day buffer, with nothing logged since — is still in effect and
/// draws, as nowview draws it. One switched off since, or carried in as
/// null or IGNORED, does not.
#[test]
fn a_state_switched_on_before_the_fetch_buffer_is_carried_into_the_day() {
    let db = fresh_db();
    switch_at(&db, "mood5", serde_json::json!("happy"), 5);
    switch_at(&db, "mood30", serde_json::json!("calm"), 30);
    switch_at(&db, "gone", serde_json::json!("on"), 5);
    switch_at(&db, "gone", serde_json::json!(false), 4);
    switch_at(&db, "nothing", serde_json::Value::Null, 5);
    switch_at(&db, "ignored", serde_json::json!("IGNORED"), 5);

    assert_eq!(fields_per_day(&db, 1), [["mood30", "mood5"]]);
    // Every day of a multi-day view carries it, the first included.
    assert_eq!(fields_per_day(&db, 3), vec![vec!["mood30", "mood5"]; 3]);
}

/// A dot logged at `utc` by a clock in `zone`.
fn dot_in(db: &datum_core::db::Db, utc: &str, zone: &str) {
    let at = datum_core::time::DatumTime::from_utc_and_tz(utc.parse().unwrap(), zone).unwrap();
    entry_builder("water").time_start(at).insert(db);
}

/// `date` as `dayview` lays it out, in hours along its axis: each
/// stretch's wall clock and its seconds east of UTC, the dead zones, the
/// labels, the stretches that have a lane, and each dot's `(stretch, hour)`.
#[derive(Debug, PartialEq)]
struct Drawn {
    spans: Vec<(f64, f64)>,
    offsets: Vec<i64>,
    dead: Vec<(f64, f64)>,
    labels: Vec<String>,
    lanes: usize,
    dots: Vec<(u64, f64)>,
}

fn drawn(db: &datum_core::db::Db, date: &str) -> Drawn {
    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(&["dayview", "--json", "-d", date, "-o", &path], db).expect("dayview --json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let axis = json["time_axis_extent"].as_f64().unwrap();
    let hour = |px: &serde_json::Value| (px.as_f64().unwrap() / axis * 2400.0).round() / 100.0;
    let day = &json["days"][0];
    assert_eq!(day["date"], date);
    let list = |key: &str| day[key].as_array().cloned().unwrap_or_default();
    let from_to = |a: &serde_json::Value, b: &serde_json::Value| {
        (
            hour(a),
            hour(&(a.as_f64().unwrap() + b.as_f64().unwrap()).into()),
        )
    };
    Drawn {
        spans: (list("spans").iter())
            .map(|s| from_to(&s["time_offset"], &s["time_extent"]))
            .collect(),
        offsets: (list("spans").iter())
            .map(|s| s["offset_s"].as_i64().unwrap())
            .collect(),
        dead: (list("dead_zones").iter())
            .map(|z| from_to(&z[0], &z[1]))
            .collect(),
        labels: (list("zone_labels").iter())
            .map(|l| l["text"].as_str().unwrap().to_string())
            .collect(),
        lanes: (list("spans").iter())
            .filter(|s| s.get("lane").is_some())
            .count(),
        dots: (list("fields").iter())
            .flat_map(|f| f["points"].as_array().cloned().unwrap_or_default())
            .map(|p| (p["span"].as_u64().unwrap_or(0), hour(&p["time_pos"])))
            .collect(),
    }
}

/// da-1v9gp: a day is drawn on the clock it was lived by, whatever zone
/// the reader is in (the suite's host is not Berlin). The clock times a
/// forward jump skipped are a dead zone, and the ones a backward jump
/// repeated are a lane for each time they happened, with each entry in the
/// lane of the offset it was logged at.
#[test]
fn a_day_the_clock_jumped_in_is_drawn_on_the_clock_it_was_lived_by() {
    let _g = freeze_time("2026-11-01T12:00:00");
    let db = fresh_db();
    // 01:40 at +1, and 03:20 at +2 forty minutes later.
    dot_in(&db, "2026-03-29T00:40:00Z", "Europe/Berlin");
    dot_in(&db, "2026-03-29T01:20:00Z", "Europe/Berlin");
    // 02:20 at +2, and 02:15 at +1 fifty-five minutes later.
    dot_in(&db, "2026-10-25T00:20:00Z", "Europe/Berlin");
    dot_in(&db, "2026-10-25T01:15:00Z", "Europe/Berlin");

    let label = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
    assert_eq!(
        drawn(&db, "2026-03-29"),
        Drawn {
            spans: vec![(0.0, 2.0), (3.0, 24.0)],
            offsets: vec![3600, 7200],
            dead: vec![(2.0, 3.0)],
            labels: label("UTC+1", "UTC+2"),
            lanes: 0,
            dots: vec![(0, 1.67), (1, 3.33)],
        }
    );
    assert_eq!(
        drawn(&db, "2026-10-25"),
        Drawn {
            spans: vec![(0.0, 3.0), (2.0, 24.0)],
            offsets: vec![7200, 3600],
            dead: vec![],
            labels: label("UTC+2", "UTC+1"),
            lanes: 2,
            dots: vec![(0, 2.33), (1, 2.25)],
        }
    );
    // An ordinary day between them is one stretch, midnight to midnight.
    assert_eq!(drawn(&db, "2026-06-01").spans, [(0.0, 24.0)]);
}

/// da-1v9gp: a traveller. Moscow (+3) until the first entry logged in
/// Berlin (+1), at 08:40 there, which is 10:40 in Moscow: the clock went
/// back two hours, and those two hours are a lane each.
#[test]
fn a_traveller_whose_clock_went_back_has_a_lane_for_each_zone() {
    let _g = freeze_time("2026-02-01T12:00:00");
    let db = fresh_db();
    dot_in(&db, "2026-01-10T09:00:00Z", "Europe/Moscow");
    dot_in(&db, "2026-01-11T03:30:00Z", "Europe/Moscow"); // 06:30
    dot_in(&db, "2026-01-11T06:20:00Z", "Europe/Moscow"); // 09:20
    dot_in(&db, "2026-01-11T07:40:00Z", "Europe/Berlin"); // 08:40
    dot_in(&db, "2026-01-11T08:10:00Z", "Europe/Berlin"); // 09:10

    assert_eq!(
        drawn(&db, "2026-01-11"),
        Drawn {
            spans: vec![(0.0, 10.67), (8.67, 24.0)],
            offsets: vec![10800, 3600],
            dead: vec![],
            labels: vec!["UTC+3".to_string(), "UTC+1".to_string()],
            lanes: 2,
            dots: vec![(0, 6.5), (0, 9.33), (1, 8.67), (1, 9.17)],
        }
    );
    // The day before was all Moscow's: 12:00 there, on a whole band, which
    // says whose clock it is though no end of it is labelled (da-3ctgr).
    let before = drawn(&db, "2026-01-10");
    assert_eq!(
        (before.spans, before.offsets, before.dots),
        (vec![(0.0, 24.0)], vec![10800], vec![(0, 12.0)])
    );
}

/// 2026-01-10's band as an SVG, its entries' ids masked: a sample's are
/// made afresh each render.
fn day_svg(db: &datum_core::db::Db, extra: &[&str]) -> String {
    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap();
    let argv = [
        "dayview",
        "--end-date",
        "2026-01-10",
        "--width",
        "600",
        "-o",
        path,
    ];
    run_cli(&[&argv[..], extra].concat(), db).expect("dayview");
    let svg = std::fs::read_to_string(path).unwrap();
    let mut parts = svg.split(r#"data-id=""#);
    let mut out = parts.next().unwrap().to_string();
    for p in parts {
        out += r#"data-id="HID"#;
        out += &p[p.find('"').unwrap()..];
    }
    out
}

/// da-srrqd: `--sample SPEC` draws a field with nothing on the day as the
/// same entries logged for real would be drawn, byte for byte, in the
/// spec it is given and not the one stored, and writes nothing.
#[test]
fn a_sample_is_drawn_as_its_entries_would_be() {
    let at = |t: &str| format!("2026-01-10 {t}");
    let shapes: [(&str, &str, Vec<Vec<String>>); 4] = [
        (
            "pill",
            r##"{"field":"pill","verb":"occur","color":"#00aa00","glyph":"★","glyph_scale":1.5,"allowed_states":null,"state_colors":{}}"##,
            [
                "01:30", "04:30", "06:00", "10:30", "14:30", "16:30", "20:30", "22:30",
            ]
            .map(|t| vec!["occur".into(), "pill".into(), "-t".into(), at(t)])
            .into(),
        ),
        (
            "lamp",
            r#"{"field":"lamp","verb":"start","allowed_states":null,"state_colors":{}}"#,
            [
                "01:00", "04:00", "06:30", "11:00", "14:00", "17:30", "20:30", "23:00",
            ]
            .iter()
            .enumerate()
            .map(|(i, t)| {
                vec![
                    ["start", "end"][i % 2].into(),
                    "lamp".into(),
                    "-t".into(),
                    at(t),
                ]
            })
            .collect(),
        ),
        (
            "mood",
            r##"{"field":"mood","verb":"switch","allowed_states":["happy","sad"],"state_colors":{"happy":"#ff0000"}}"##,
            [
                "01:00", "04:00", "06:30", "11:00", "14:00", "17:30", "20:30", "23:00",
            ]
            .iter()
            .enumerate()
            .map(|(i, t)| match ["happy", "sad", "end"][i % 3] {
                "end" => vec!["end".into(), "mood".into(), "-t".into(), at(t)],
                s => vec!["switch".into(), "mood".into(), s.into(), "-t".into(), at(t)],
            })
            .collect(),
        ),
        // A switch with no states named and never off: a bare `switch` stores
        // `true`, and holds it the rest of the day.
        (
            "env",
            r#"{"field":"env","verb":"switch","allowed_states":null,"state_colors":{},"never_false":true}"#,
            vec![vec![
                "switch".into(),
                "env".into(),
                "-t".into(),
                at("01:00"),
            ]],
        ),
    ];
    for (field, spec, lines) in shapes {
        let sampled = fresh_db();
        let svg = day_svg(&sampled, &["--sample", spec]);
        assert!(
            svg.contains(&format!(r#"data-field="{field}""#)),
            "{field}: {svg}"
        );
        assert!(
            sampled.distinct_fields().unwrap().is_empty(),
            "{field}: an entry written"
        );
        assert!(
            sampled.load_spec(field).unwrap().is_none(),
            "{field}: its spec written"
        );

        let logged = fresh_db();
        run_cli(&["setup", field, "--from-json", spec], &logged).unwrap();
        for line in &lines {
            let line: Vec<&str> = line.iter().map(String::as_str).collect();
            run_cli(&line, &logged).unwrap();
        }
        assert_eq!(svg, day_svg(&logged, &[]), "{field}");
    }
}

/// da-srrqd: where the field has entries on the day the band draws those,
/// in the spec it is given, and no sample.
#[test]
fn a_field_with_entries_on_the_day_draws_them_and_no_sample() {
    let db = fresh_db();
    run_cli(&["occur", "pill", "-t", "2026-01-10 12:00"], &db).unwrap();
    let spec = r##"{"field":"pill","verb":"occur","color":"#00aa00","allowed_states":null,"state_colors":{}}"##;
    let svg = day_svg(&db, &["--sample", spec]);
    assert_eq!(svg.matches("point-fg").count(), 1, "{svg}");
    assert!(svg.contains("#00aa00"), "{svg}");
    assert!(!day_svg(&db, &[]).contains("#00aa00"));
}
