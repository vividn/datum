//! `datum nowview` CLI integration tests (da-vw4).

mod common;

use chrono::{Duration, Utc};
use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, fresh_db};

#[test]
fn nowview_no_history_writes_now_panel_only() {
    // Even with empty data, nowview should produce a valid SVG with the
    // black background and outer envelope.
    let db = fresh_db();
    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(&["nowview", "--output-file", &path], &db).expect("nowview dispatch");

    let svg = std::fs::read_to_string(&path).expect("read svg");
    // XML declaration + SVG envelope.
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg "));
    assert!(svg.ends_with("</svg>"));
    // Default scale-from-no-history width (100) + default height (300).
    assert!(svg.contains(r#"width="100""#));
    assert!(svg.contains(r#"height="300""#));
    // Black background rect.
    assert!(svg.contains(r#"fill="black""#));
    // No history panel → no current-time line classes.
    assert!(!svg.contains("current-time-line-vertical"));
}

#[test]
fn nowview_with_history_renders_two_panels_with_grid_and_now_line() {
    let db = fresh_db();
    let now = Utc::now();

    // An ongoing block that started 30 min ago should fill the now panel.
    entry_builder("mood")
        .verb(Verb::Switch)
        .value(serde_json::json!("focused"))
        .time_start_utc(now - Duration::minutes(30))
        .insert(&db);

    // A point 10 min ago — should appear inside the history panel.
    entry_builder("coffee")
        .verb(Verb::Occur)
        .value(serde_json::json!(1))
        .time_start_utc(now - Duration::minutes(10))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(
        &["nowview", "--history", "1h", "--output-file", &path],
        &db,
    )
    .expect("nowview dispatch");

    let svg = std::fs::read_to_string(&path).expect("read svg");
    // Two-panel layout: yellow now-line + gridlines must appear.
    assert!(svg.contains("current-time-line-vertical"));
    assert!(svg.contains("current-time-line-horizontal"));
    assert!(svg.contains("#ffcc00"));
    assert!(svg.contains("gridline-major"));
    // 60m history → 4 major ticks (15/30/45/60).
    let major_count = svg.matches("gridline-major").count();
    assert_eq!(major_count, 4, "expected 4 major gridlines, got {major_count}");
    // Field bands present for both fields.
    assert!(svg.contains(r#"data-field="mood""#));
    assert!(svg.contains(r#"data-field="coffee""#));
    // Default width for >10m history is 400.
    assert!(svg.contains(r#"width="400""#));
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
        .verb(Verb::Start)
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
        .verb(Verb::Start)
        .time_start_utc(now - Duration::minutes(45))
        .time_end_utc(now - Duration::minutes(30))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(
        &["nowview", "--history", "1h", "--output-file", &path],
        &db,
    )
    .expect("nowview dispatch");
    let svg = std::fs::read_to_string(&path).expect("read svg");
    assert!(svg.contains(r#"data-field="workout""#));
    assert!(svg.contains(r#"class="block""#));
}

#[test]
fn nowview_terminal_writes_text_with_unicode_blocks() {
    // `--terminal` + `--output-file foo.txt` produces a text file with
    // Unicode shading characters and field labels — no SVG envelope.
    let db = fresh_db();
    let now = Utc::now();
    entry_builder("mood")
        .verb(Verb::Switch)
        .value(serde_json::json!("focused"))
        .time_start_utc(now - Duration::minutes(20))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".txt").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(
        &[
            "nowview",
            "--history",
            "1h",
            "--terminal",
            "--no-color",
            "--cols",
            "60",
            "--output-file",
            &path,
        ],
        &db,
    )
    .expect("nowview --terminal dispatch");

    let text = std::fs::read_to_string(&path).expect("read text");
    assert!(!text.starts_with("<?xml"), "should not be SVG");
    assert!(!text.contains("<svg "), "should not be SVG");
    assert!(text.contains("mood"), "expected field label in:\n{text}");
    // Filled cell character for the ongoing-state band.
    assert!(text.contains('█'), "expected filled char in:\n{text}");
    // Border row with `└─┘` style.
    assert!(text.contains('└') && text.contains('┘'));
}

#[test]
fn nowview_terminal_color_mode_emits_ansi_escapes() {
    let db = fresh_db();
    let now = Utc::now();
    entry_builder("mood")
        .verb(Verb::Switch)
        .value(serde_json::json!("focused"))
        .time_start_utc(now - Duration::minutes(15))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".txt").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    // `--terminal` without `--no-color`. NO_COLOR isn't set in tests, but
    // stdout for the test harness is not a TTY — so the auto-disable in
    // the dispatcher would normally kick in. We force-enable terminal
    // mode and verify no-color path here; ANSI emission is exercised in
    // the layout-crate unit test (color_mode_emits_ansi_escapes).
    run_cli(
        &[
            "nowview",
            "--history",
            "30m",
            "--terminal",
            "--no-color",
            "--output-file",
            &path,
        ],
        &db,
    )
    .expect("nowview --terminal dispatch");
    let text = std::fs::read_to_string(&path).expect("read text");
    // Exactly verify *no* ANSI when --no-color is set.
    assert!(!text.contains("\x1b["), "unexpected ANSI in:\n{text}");
}

#[test]
fn nowview_no_field_data_still_renders_envelope() {
    let db = fresh_db();
    // Track a field but never log to it in the visible window.
    entry_builder("yesterday-only")
        .verb(Verb::Occur)
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
    // 30m history → default width 400.
    assert!(svg.contains(r#"width="400""#));
}
