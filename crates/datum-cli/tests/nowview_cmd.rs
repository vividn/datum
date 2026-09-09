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
    // da-nk84: default width = default_width_for_history(30m) = 400 px;
    // the now strip is carved out of it, not added to it.
    assert!(svg.contains(r#"width="400""#));
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
    // da-nk84: 60m history → 400 px, now strip included.
    assert!(svg.contains(r#"width="400""#));

    // Overlay clock label injected inside the plot group.
    assert!(svg.contains("current-time-text"));
    // da-nk84: the labelled panel is gone; the state at now is a chunk
    // right of the now-line, and only for a field that is in one. `mood`
    // is mid-switch, `coffee` is an occur-only field.
    assert!(!svg.contains("nowview-state-panel"));
    assert!(!svg.contains("state-swatch"));
    assert!(!svg.contains("mood: focused"));
    assert!(svg.contains(r#"class="now-chunk""#));
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
    // da-nk84: 30m history → 400 px, now strip included.
    assert!(svg.contains(r#"width="400""#));
}

/// `nowview --json` emits the resolved current-state panel as structured
/// data (the shape the Pebble `nowview` watchface consumes) instead of SVG:
/// `{ now_ms, history_minutes, panel: [{ field, state_label, colors }] }`.
#[test]
fn nowview_json_emits_current_state_panel() {
    let db = fresh_db();
    let now = Utc::now();
    // Ongoing switch → resolves to a labeled current state at "now".
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(now - Duration::minutes(10))
        .insert(&db);
    // Occur-only field → no ongoing state; the panel still lists it with a
    // null label as a presence cue, which the watchface renders as the bare
    // field name.
    entry_builder("coffee")
        .verb(Verb::Point)
        .time_start_utc(now - Duration::minutes(5))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(&["nowview", "--json", "--output-file", &path], &db).expect("nowview --json dispatch");

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read json"))
            .expect("valid json");

    assert!(v["now_ms"].as_i64().is_some(), "now_ms present");
    assert_eq!(v["history_minutes"].as_f64(), Some(30.0));

    let panel = v["panel"].as_array().expect("panel array");
    let mood = panel
        .iter()
        .find(|e| e["field"] == "mood")
        .expect("mood entry present");
    assert_eq!(mood["state_label"], "focused");
    let colors = mood["colors"].as_array().expect("colors array");
    assert!(!colors.is_empty(), "mood carries at least one hex color");

    let coffee = panel
        .iter()
        .find(|e| e["field"] == "coffee")
        .expect("coffee entry present");
    assert!(
        coffee["state_label"].is_null(),
        "occur-only field has null state_label, got {:?}",
        coffee["state_label"]
    );
}

/// `nowview --json` with no `--output-file` must short-circuit to stdout —
/// never fall through to the terminal band-stack path. Just assert clean
/// dispatch (content is covered by the file-based test above).
#[test]
fn nowview_json_stdout_dispatches() {
    let db = fresh_db();
    run_cli(&["nowview", "--json"], &db).expect("nowview --json stdout dispatch");
}

// ---------------------------------------------------------------------
// da-nk84: the SVG's structure — lanes filled edge to edge, a chunk for
// each field's current state right of the now-line, and no labels but the
// axis. Nowview's parity fixtures are `uncapturable` (da-t5f5: the window
// is anchored to now), so the picture is held by structure instead. The
// shape being matched is `docs/parity-audit/nowview_with_history_reference.png`.
// ---------------------------------------------------------------------

/// One `<rect>`'s geometry and fill, parsed out of the SVG.
#[derive(Debug, Clone, PartialEq)]
struct Rect {
    x: f64,
    w: f64,
    fill: String,
}

/// Every `<rect class="{class}" …>` inside the `<g class="field">` group
/// for `field`, in document order. The renderer writes the attributes in a
/// fixed order, which is what makes this scan enough.
fn field_rects(svg: &str, field: &str, class: &str) -> Vec<Rect> {
    let open = format!(r#"<g class="field" data-field="{field}">"#);
    let Some(start) = svg.find(&open).map(|i| i + open.len()) else {
        return Vec::new();
    };
    let end = svg[start..].find("</g>").expect("field group close") + start;
    let group = &svg[start..end];

    let tag = format!(r#"<rect class="{class}" "#);
    let mut out = Vec::new();
    let mut rest = group;
    while let Some(i) = rest.find(&tag) {
        let s = i + tag.len();
        let e = rest[s..].find("/>").expect("rect close") + s;
        let attrs = &rest[s..e];
        let get = |name: &str| -> String {
            let key = format!(r#"{name}=""#);
            let a = attrs.find(&key).map(|k| k + key.len()).expect("attribute");
            let b = attrs[a..].find('"').expect("attribute close") + a;
            attrs[a..b].to_string()
        };
        out.push(Rect {
            x: get("x").parse().expect("number"),
            w: get("width").parse().expect("number"),
            fill: get("fill"),
        });
        rest = &rest[e..];
    }
    out
}

/// The now-line's x, read off the dashed marker the dayview renderer draws.
fn now_x(svg: &str) -> f64 {
    let key = r#"<line class="current-time-line" x1=""#;
    let a = svg.find(key).map(|i| i + key.len()).expect("now-line");
    let b = svg[a..].find('"').expect("x1 close") + a;
    svg[a..b].parse().expect("number")
}

/// A db carrying the three cases the bead names: a field that switched
/// twice inside the window, a steady field whose state predates it, and a
/// field with no state at now. Returns the rendered SVG.
fn three_field_nowview() -> String {
    let db = fresh_db();
    let now = Utc::now();

    // Switched twice inside the 15m window, over a state that predates
    // it — so the lane is filled from the left edge and then changes.
    entry_builder("activity")
        .verb(Verb::Switch)
        .state(serde_json::json!("email"))
        .time_start_utc(now - Duration::minutes(45))
        .insert(&db);
    entry_builder("activity")
        .verb(Verb::Switch)
        .state(serde_json::json!("coding"))
        .time_start_utc(now - Duration::minutes(12))
        .insert(&db);
    entry_builder("activity")
        .verb(Verb::Switch)
        .state(serde_json::json!("meeting"))
        .time_start_utc(now - Duration::minutes(3))
        .insert(&db);

    // Steady: switched before the window opened, so the lane is filled
    // from the left edge by the lead-in row alone.
    entry_builder("posture")
        .verb(Verb::Switch)
        .state(serde_json::json!("sit"))
        .time_start_utc(now - Duration::minutes(90))
        .insert(&db);

    // Occur-only: a dot, and no state at now.
    entry_builder("coffee")
        .verb(Verb::Point)
        .time_start_utc(now - Duration::minutes(8))
        .insert(&db);

    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    run_cli(
        &["nowview", "--history", "15m", "--output-file", &path],
        &db,
    )
    .expect("nowview dispatch");
    std::fs::read_to_string(&path).expect("read svg")
}

#[test]
fn nowview_lanes_fill_edge_to_edge_and_carry_a_now_chunk() {
    let svg = three_field_nowview();
    let now_x = now_x(&svg);

    // Nothing is labelled but the axis: the state panel is gone.
    for text in svg.split(r#"<text class=""#).skip(1) {
        let class = &text[..text.find('"').expect("class close")];
        assert!(
            class == "tick-label" || class == "current-time-text",
            "unexpected <text class=\"{class}\"> — nowview labels only the axis: {svg}"
        );
    }

    for field in ["activity", "posture"] {
        // The lane tiles the whole history window with no gap.
        let blocks = field_rects(&svg, field, "block");
        assert!(!blocks.is_empty(), "{field} has no blocks in: {svg}");
        let mut x = 0.0;
        for b in &blocks {
            assert!(
                (b.x - x).abs() < 1e-6,
                "{field}: gap before block at x={} (expected {x}) in: {svg}",
                b.x
            );
            x = b.x + b.w;
        }
        assert!(
            (x - now_x).abs() < 1e-6,
            "{field}: lane ends at {x}, not at the now-line {now_x}: {svg}"
        );

        // Exactly one chunk past the now-line, running to the plot's edge.
        let chunks = field_rects(&svg, field, "now-chunk");
        assert_eq!(chunks.len(), 1, "{field}: expected one now-chunk: {svg}");
        assert!(
            (chunks[0].x - now_x).abs() < 1e-6,
            "{field}: chunk starts at {}",
            chunks[0].x
        );
        assert!(chunks[0].w > 0.0, "{field}: chunk has no width");
    }

    // The occur-only field is in no state now, so it gets no chunk.
    assert!(
        field_rects(&svg, "coffee", "now-chunk").is_empty(),
        "occur-only field must not get a now-chunk: {svg}"
    );
}

#[test]
fn nowview_chunk_carries_the_state_switched_into_inside_the_window() {
    let svg = three_field_nowview();

    let blocks = field_rects(&svg, "activity", "block");
    assert_eq!(blocks.len(), 3, "activity switched twice inside the window");
    let chunk = &field_rects(&svg, "activity", "now-chunk")[0];

    // Right of the now-line: the state it is in now — the same colour as
    // the block that runs up to the line.
    let last = blocks.last().unwrap();
    assert_eq!(chunk.fill, last.fill, "chunk must carry the new state");
    // Left of it: what it was. A switch that changed nothing would not be
    // a switch, so the two colours differ.
    assert_ne!(
        blocks[blocks.len() - 2].fill,
        last.fill,
        "the previous state keeps its own colour: {svg}"
    );
}
