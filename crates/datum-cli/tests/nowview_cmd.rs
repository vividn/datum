//! `datum nowview` CLI integration tests (da-vw4).

mod common;

use chrono::{Duration, Utc};
use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, fresh_db};

#[test]
fn nowview_default_history_renders_envelope() {
    // da-6jy: nowview shares dayview's render path scoped to the last
    // `history_minutes` (default 30m); there's no separate "now panel"
    // anymore. With empty data the envelope still renders.
    let db = fresh_db();
    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(&["nowview", "--output-file", &path], &db).expect("nowview dispatch");

    let svg = std::fs::read_to_string(&path).expect("read svg");
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg "));
    assert!(svg.ends_with("</svg>"));
    // da-34w: default width = default_width_for_history(30m=400) +
    // panel-bump (120) = 520 px.
    assert!(svg.contains(r#"width="520""#));
    assert!(svg.contains(r#"height="300""#));
    assert!(svg.contains(r#"fill="black""#));
    // Even on an empty DB the now-line marks the right edge of the data
    // area (now_ms == day.end_ms).
    assert!(svg.contains("current-time-line"));
    assert!(svg.contains("#ffcc00"));
    // Overlay clock label is always emitted (HH:MM never empty).
    assert!(svg.contains("current-time-text"));
    // da-34w: date label dropped — no `<text class="date-label">` group.
    assert!(!svg.contains("date-label"));
}

#[test]
fn nowview_with_history_renders_dayview_style_grid_and_now_line() {
    // da-6jy: nowview reuses dayview's renderer (gridlines, axis, yellow
    // now-line) plus a relative-minute axis override (`-15m`, `-30m`, …).
    let db = fresh_db();
    let now = Utc::now();

    // An ongoing switch — its switch-region should extend to the right
    // edge (now), filling the field band right up to the now-line.
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(now - Duration::minutes(30))
        .insert(&db);

    // A point 10 min ago should land inside the data area as a circle.
    entry_builder("coffee")
        .verb(Verb::Point)
        .state(serde_json::json!(1))
        .time_start_utc(now - Duration::minutes(10))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(&["nowview", "--history", "1h", "--output-file", &path], &db)
        .expect("nowview dispatch");

    let svg = std::fs::read_to_string(&path).expect("read svg");

    // Dayview-style now-line (single class, not the legacy *-vertical /
    // *-horizontal split that the old standalone nowview emitted).
    assert!(svg.contains("current-time-line"));
    assert!(!svg.contains("current-time-line-vertical"));
    assert!(svg.contains("#ffcc00"));

    // Dayview-style gridline class (one per axis tick).
    let gridline_count = svg.matches(r#"class="gridline""#).count();
    // 60m history with `tick_config(60)` → minor=5 → ticks every 5m for
    // 60m + a trailing "now" tick = 13 gridlines total.
    assert_eq!(
        gridline_count, 13,
        "expected 13 gridlines for 60m history, got {gridline_count}"
    );
    // Major opacity 0.4 marks the every-15m positions (-15/-30/-45/-60)
    // plus the trailing "now" edge → 5 majors.
    let major_count = svg.matches(r#"stroke-opacity="0.4""#).count();
    assert_eq!(
        major_count, 5,
        "expected 5 major gridlines, got {major_count}"
    );

    assert!(svg.contains(r#"data-field="mood""#));
    assert!(svg.contains(r#"data-field="coffee""#));
    // da-34w: 60m history bumps to 520 (400 + 120 panel).
    assert!(svg.contains(r#"width="520""#));

    // Overlay clock label injected inside the plot group.
    assert!(svg.contains("current-time-text"));
    // da-34w: state panel renders one swatch + label per band.
    assert!(svg.contains(r#"class="nowview-state-panel""#));
    assert!(svg.contains(r#"class="state-swatch" data-field="mood""#));
    assert!(svg.contains(r#"class="state-swatch" data-field="coffee""#));
    assert!(svg.contains("mood: focused"));
}

#[test]
fn nowview_html_output_wraps_svg_in_document() {
    let db = fresh_db();
    let tmp = tempfile::NamedTempFile::with_suffix(".html").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(&["nowview", "--output-file", &path], &db).expect("nowview dispatch");

    let html = std::fs::read_to_string(&path).expect("read html");
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<svg "));
    assert!(html.contains("</svg>"));
}

#[test]
fn nowview_png_output_is_valid_png() {
    let db = fresh_db();
    let now = Utc::now();
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(now - Duration::minutes(20))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".png").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(
        &[
            "nowview",
            "--history",
            "30m",
            "--width",
            "300",
            "--height",
            "150",
            "--output-file",
            &path,
        ],
        &db,
    )
    .expect("nowview dispatch (png)");

    let bytes = std::fs::read(&path).expect("read png");
    // PNG magic: 89 50 4E 47 0D 0A 1A 0A
    let magic = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    assert!(
        bytes.len() > magic.len() && bytes[..magic.len()] == magic,
        "expected PNG magic header, got {:02x?}",
        &bytes[..magic.len().min(bytes.len())]
    );
}

#[test]
fn nowview_rejects_unknown_extension() {
    let db = fresh_db();
    let tmp = tempfile::NamedTempFile::with_suffix(".jpg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    let err = run_cli(&["nowview", "--output-file", &path], &db).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains(".svg") && msg.contains(".png") && msg.contains(".html"),
        "expected error to mention valid extensions, got: {msg}"
    );
}

#[test]
fn nowview_block_entry_renders_in_history_window() {
    // A closed block entirely within the history window should render
    // a rect inside the history panel.
    let db = fresh_db();
    let now = Utc::now();
    entry_builder("workout")
        .verb(Verb::Switch)
        .time_start_utc(now - Duration::minutes(45))
        .time_end_utc(now - Duration::minutes(30))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(&["nowview", "--history", "1h", "--output-file", &path], &db)
        .expect("nowview dispatch");
    let svg = std::fs::read_to_string(&path).expect("read svg");
    assert!(svg.contains(r#"data-field="workout""#));
    assert!(svg.contains(r#"class="block""#));
}

#[test]
fn nowview_rejects_terminal_flag_after_split() {
    // da-raz: `--terminal` was ripped from `nowview` and moved to `now`.
    // The flag is now a hard parse error here. Same for `--no-color` and
    // `--cols`, which only exist on `now`.
    let db = fresh_db();
    let err = run_cli(&["nowview", "--terminal"], &db).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("--terminal") || msg.to_lowercase().contains("unknown"),
        "expected unknown-flag error for --terminal, got: {msg}"
    );
}

/// da-57dj: `dm nowview` (no `--output-file`) defaults to a pure visual
/// terminal band stack. The dispatch path must succeed without error
/// (rendering itself is covered by `nowview_terminal`'s unit tests). The
/// `--cols`/`--rows`/`--no-color` knobs must parse cleanly.
#[test]
fn nowview_terminal_snapshot_dispatches() {
    let db = fresh_db();
    let now = Utc::now();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(now - Duration::minutes(15))
        .insert(&db);

    run_cli(
        &["nowview", "--cols", "20", "--rows", "12", "--no-color"],
        &db,
    )
    .expect("nowview terminal snapshot dispatch");
}

/// da-57dj: `dm nowview --window 30m` selects horizontal Flow mode (no
/// `--output-file`). Verifies the new `--window` flag parses + dispatches.
#[test]
fn nowview_terminal_flow_dispatches() {
    let db = fresh_db();
    let now = Utc::now();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(now - Duration::minutes(10))
        .insert(&db);

    run_cli(
        &[
            "nowview",
            "--window",
            "30m",
            "--cols",
            "40",
            "--rows",
            "11",
            "--no-color",
        ],
        &db,
    )
    .expect("nowview terminal flow dispatch");
}

/// `--window 0m` and other zero/negative durations must be rejected with
/// a clear error (so users don't silently get a degenerate render).
#[test]
fn nowview_terminal_rejects_zero_window() {
    let db = fresh_db();
    let err = run_cli(&["nowview", "--window", "0m"], &db).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("--window") || msg.contains("positive"),
        "expected positive-window error, got: {msg}"
    );
}

#[test]
fn nowview_no_field_data_still_renders_envelope() {
    let db = fresh_db();
    // Track a field but never log to it in the visible window.
    entry_builder("yesterday-only")
        .verb(Verb::Point)
        .time_start_utc(Utc::now() - Duration::hours(48))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(
        &["nowview", "--history", "30m", "--output-file", &path],
        &db,
    )
    .expect("nowview dispatch");
    let svg = std::fs::read_to_string(&path).expect("read svg");
    // Envelope is always there even without data inside the window.
    assert!(svg.contains("<svg "));
    assert!(svg.ends_with("</svg>"));
    // da-34w: 30m history → 400 base + 120 panel = 520.
    assert!(svg.contains(r#"width="520""#));
}
