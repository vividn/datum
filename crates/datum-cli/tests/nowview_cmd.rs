//! `datum nowview` CLI integration tests (da-vw4).
#![allow(clippy::disallowed_types)] // native tests: `chrono::Local` is the host zone the binary reads too

use crate::common;

use chrono::{Duration, Utc};
use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, fresh_db};

#[test]
fn nowview_context_window_renders_envelope() {
    // da-6jy: nowview shares dayview's render path scoped to the last
    // `history_minutes`; there's no separate "now panel" anymore. With
    // empty data the envelope still renders.
    let db = fresh_db();
    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();

    run_cli(
        &["nowview", "--context", "30m", "--output-file", &path],
        &db,
    )
    .expect("nowview dispatch");

    let svg = std::fs::read_to_string(&path).expect("read svg");
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg "));
    assert!(svg.ends_with("</svg>"));
    // da-nk84: the width a 30m window defaults to,
    // default_width_for_history(30m) = 400 px; the now strip is carved out
    // of it, not added to it. (Not the width of a bare `nowview`, which
    // since da-7q8r is the snapshot's 100.)
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
    // 60m history across ~370px → minor=5 (31px, room for a 30.6px label)
    // → ticks every 5m for 60m + a trailing "now" tick = 13 gridlines.
    assert_eq!(
        gridline_count, 13,
        "expected 13 gridlines for 60m history, got {gridline_count}"
    );
    // Major opacity 0.4 marks the every-10m positions (-60 … -10), 62px
    // apart, room for two labels (da-w4vfw), plus the trailing "now" edge
    // → 7 majors.
    let major_count = svg.matches(r#"stroke-opacity="0.4""#).count();
    assert_eq!(
        major_count, 7,
        "expected 7 major gridlines, got {major_count}"
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
    // A mark's hover `<title>` names it (da-s5da); no label is drawn.
    assert!(!svg.contains(">mood: focused<"));
    assert!(svg.contains(r#"class="now-chunk""#));
}

/// The snapshot at a selected time draws that time's states (da-o3sns).
/// Its chips were drawn only when now fell in the window — an instant's
/// window never holds now unless it IS now — so `-t` drew nothing, and the
/// page's line through the chips (da-rskp) was erased by the very answer
/// it asked for.
#[test]
fn nowview_snapshot_at_a_time_draws_that_times_states() {
    let db = fresh_db();
    let now = Utc::now();
    let [focused, calm] = [("focused", 3), ("calm", 1)].map(|(state, ago)| {
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(serde_json::json!(state))
            .time_start_utc(now - Duration::hours(ago))
            .insert(&db)
            .hid
    });
    let snapshot = |hours_ago: i64| {
        let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let at = (now - Duration::hours(hours_ago)).to_rfc3339();
        run_cli(
            &[
                "nowview",
                "--history",
                "0",
                "-t",
                &at,
                "--output-file",
                &path,
            ],
            &db,
        )
        .expect("nowview dispatch");
        std::fs::read_to_string(&path).expect("read svg")
    };

    // The chip names the entry whose state it is (mark_titles.rs).
    let chip = |svg: String| {
        let rect = svg
            .split(r#"<rect class="now-chunk""#)
            .nth(1)?
            .split("/>")
            .next()?
            .to_string();
        [&focused, &calm]
            .into_iter()
            .find(|hid| rect.contains(&format!(r#"data-id="{hid}""#)))
            .cloned()
    };
    assert_eq!(
        chip(snapshot(2)),
        Some(focused.clone()),
        "two hours ago is focused"
    );
    assert_eq!(chip(snapshot(0)), Some(calm.clone()), "now is calm");
    assert_eq!(chip(snapshot(4)), None, "a chip before any entry");
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
    // A closed block entirely within the window `--history` asks for below
    // should render a rect inside it. The window is asked for, not assumed:
    // bare `nowview` is the snapshot and has no band (da-7q8r).
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
    run_cli(
        &[
            "nowview",
            "--json",
            "--context",
            "30m",
            "--output-file",
            &path,
        ],
        &db,
    )
    .expect("nowview --json dispatch");

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
        let e = rest[s..].find('>').expect("rect close") + s;
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
        // ... up to now, a gap short of the now-line (da-9g3bf).
        assert!(
            (x + 2.0 - now_x).abs() < 1e-6,
            "{field}: lane ends at {x}, not a gap before the now-line {now_x}: {svg}"
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

// ---------------------------------------------------------------------
// da-pcis: `--history 0` is the snapshot, and a time flag (`-t -d -q -y -z`, da-5flt) centres the window
// on an instant instead of ending it at now.
// ---------------------------------------------------------------------

/// Run `nowview` with `args` writing to a temp file of `suffix`; its text.
fn nowview_out(db: &datum_core::db::Db, args: &[&str], suffix: &str) -> String {
    let tmp = tempfile::NamedTempFile::with_suffix(suffix).unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let mut argv = vec!["nowview"];
    argv.extend_from_slice(args);
    argv.extend_from_slice(&["--output-file", &path]);
    run_cli(&argv, db).expect("nowview dispatch");
    std::fs::read_to_string(&path).expect("read output")
}

#[test]
fn nowview_history_zero_is_the_snapshot() {
    let db = fresh_db();
    let now = Utc::now();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(now - Duration::minutes(10))
        .insert(&db);

    let svg = nowview_out(&db, &["--history", "0"], ".svg");
    // The state in its colour, and the snapshot's own width. da-7q8r: no
    // label — `0` is now also the default, and the default picture is bare.
    assert!(!svg.contains("state-label"), "unasked-for label in: {svg}");
    assert_eq!(field_rects(&svg, "mood", "now-chunk").len(), 1, "{svg}");
    assert!(svg.contains(r#"width="100""#), "{svg}");
    // No time band: no axis, no ticks, no gridlines, no now-line.
    for absent in [
        r#"class="axis""#,
        r#"class="tick""#,
        "tick-label",
        "gridline",
        "current-time-line",
    ] {
        assert!(!svg.contains(absent), "snapshot carries {absent}: {svg}");
    }

    let v: serde_json::Value =
        serde_json::from_str(&nowview_out(&db, &["--json", "--history", "0"], ".json")).unwrap();
    assert_eq!(v["history_minutes"].as_f64(), Some(0.0));
    assert!(v["at_ms"].is_null());
    assert_eq!(v["start_ms"], v["end_ms"]);
    assert_eq!(v["panel"][0]["state_label"], "focused");
}

/// A switch at 13:50 and one at 14:10, the day before the bead was filed.
fn two_switch_db() -> datum_core::db::Db {
    let db = fresh_db();
    for (state, at) in [
        ("before", "2026-09-10T13:50:00Z"),
        ("after", "2026-09-10T14:10:00Z"),
    ] {
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(serde_json::json!(state))
            .time_start_utc(at.parse().unwrap())
            .insert(&db);
    }
    db
}

fn ms(iso: &str) -> i64 {
    iso.parse::<chrono::DateTime<Utc>>()
        .unwrap()
        .timestamp_millis()
}

/// Pin the clock at `iso` (UTC) for `f` — `-q` and `-t 14:00` resolve
/// against it, exactly as a verb's would.
fn at_clock<T>(iso: &str, f: impl FnOnce() -> T) -> T {
    use datum_parse::time::clock::{install, uninstall, MockClock};
    let prev = install(std::sync::Arc::new(MockClock::new(
        datum_parse::time::DatumTime {
            utc: iso.parse().unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".into()),
            uncertainty: None,
        },
    )));
    let out = f();
    uninstall();
    if let Some(p) = prev {
        install(p);
    }
    out
}

fn nowview_json(db: &datum_core::db::Db, args: &[&str]) -> serde_json::Value {
    let mut argv = vec!["--json"];
    argv.extend_from_slice(args);
    serde_json::from_str(&nowview_out(db, &argv, ".json")).unwrap()
}

// da-5flt: the shared time flags centre the window, however they are spelled.
#[test]
fn nowview_time_flags_centre_the_window() {
    let db = two_switch_db();
    for flags in [
        &["-t", "14:00"][..],
        &["-t", "-60"],
        &["-qqqqqqqqqqqq"],
        &["-d", "2026-09-10", "-t", "14:00"],
        &["-y", "-d", "2026-09-11", "-t", "14:00"],
        &["-d", "2026-09-10", "-t", "16:00", "-z", "Europe/Berlin"],
    ] {
        let mut args = flags.to_vec();
        args.extend_from_slice(&["--history", "30m"]);
        let v = at_clock("2026-09-10T15:00:00Z", || nowview_json(&db, &args));
        assert_eq!(
            v["at_ms"].as_i64(),
            Some(ms("2026-09-10T14:00:00Z")),
            "{flags:?}"
        );
        assert_eq!(v["start_ms"].as_i64(), Some(ms("2026-09-10T13:45:00Z")));
        assert_eq!(v["end_ms"].as_i64(), Some(ms("2026-09-10T14:15:00Z")));
        assert_eq!(v["now_ms"].as_i64(), Some(ms("2026-09-10T15:00:00Z")));
        assert_eq!(v["panel"][0]["state_label"], "before", "{flags:?}");
    }

    // No flag: the window ends at now.
    let v = at_clock("2026-09-10T15:00:00Z", || {
        nowview_json(&db, &["--history", "30m"])
    });
    assert!(v["at_ms"].is_null());
    assert_eq!(v["end_ms"].as_i64(), Some(ms("2026-09-10T15:00:00Z")));

    // `--at` is gone, and the entry-shaping time flags are no view's.
    for gone in [
        &["--at", "14:00"][..],
        &["-D"],
        &["--full-day"],
        &["-T"],
        &["-m", "x"],
    ] {
        let mut argv = vec!["nowview"];
        argv.extend_from_slice(gone);
        assert!(run_cli(&argv, &db).is_err(), "{gone:?} accepted");
    }
}

#[test]
fn nowview_centred_window_draws_no_yellow_for_its_instant() {
    use chrono::Timelike;
    let db = two_switch_db();
    let at = "2026-09-10T14:00:00Z";
    let lines = |svg: &str| svg.matches(r#"class="current-time-line""#).count();

    let svg = at_clock("2026-09-10T15:00:00Z", || {
        nowview_out(
            &db,
            &["-t", "14:00", "--history", "30m", "--width", "404"],
            ".svg",
        )
    });
    // Now (15:00) is outside 13:45–14:15: no yellow line at all.
    assert_eq!(lines(&svg), 0, "{svg}");
    assert!(!svg.contains("#ffcc00"), "{svg}");
    let blocks = field_rects(&svg, "mood", "block");
    assert_eq!(blocks.len(), 2, "both switches inside the window: {svg}");
    assert!(
        blocks[0].x + 2.0 < 202.0 && blocks[1].x + 2.0 > 202.0,
        "{svg}"
    );
    assert!(field_rects(&svg, "mood", "now-chunk").is_empty(), "{svg}");
    let local = at
        .parse::<chrono::DateTime<Utc>>()
        .unwrap()
        .with_timezone(&chrono::Local);
    let clock = format!("{:02}:{:02}", local.hour(), local.minute());
    assert!(
        svg.contains(&format!(">{clock}</text>")),
        "no {clock}: {svg}"
    );

    // A centred window that still holds now draws its now strip AT now:
    // ten minutes back, a 30m window runs five minutes past now, so the
    // chunk is between two lines, the future past the second (da-9g3bf).
    // The window's 30 minutes take the 375px the strip leaves of 400.
    let svg = at_clock("2026-09-10T14:10:00Z", || {
        nowview_out(&db, &["-qq", "--history", "30m", "--width", "404"], ".svg")
    });
    assert_eq!(lines(&svg), 1, "{svg}");
    assert_eq!(svg.matches(r#"class="current-time-line-end""#).count(), 1);
    let x = now_x(&svg);
    assert!(
        (x - (375.0 * 5.0 / 6.0 + 2.0)).abs() < 1e-6,
        "now-line at {x}: {svg}"
    );
    let chunk = field_rects(&svg, "mood", "now-chunk");
    assert_eq!(chunk.len(), 1, "{svg}");
    assert!((chunk[0].x - x).abs() < 1e-6, "{svg}");
    assert!(
        (chunk[0].x + chunk[0].w - (312.5 + 25.0 - 2.0)).abs() < 1e-6,
        "{svg}"
    );

    // No time flag: the line is now, at the right edge of the band.
    let svg = at_clock("2026-09-10T14:10:00Z", || {
        nowview_out(&db, &["--context", "30m"], ".svg")
    });
    assert_eq!(lines(&svg), 1, "{svg}");

    // A window with nothing in it is a picture, not an error.
    let empty = nowview_out(
        &db,
        &["--context", "30m", "-d", "2000-01-01", "-t", "00:00"],
        ".svg",
    );
    assert!(field_rects(&empty, "mood", "block").is_empty(), "{empty}");
    assert_eq!(lines(&empty), 0, "{empty}");
}

#[test]
fn nowview_snapshot_at_an_instant_shows_the_state_then() {
    let db = two_switch_db();
    let args = [
        "--history",
        "0",
        "-z",
        "UTC",
        "-d",
        "2026-09-10",
        "-t",
        "13:55",
    ];

    // Named with `--labels`, which is the only thing that draws one (da-7q8r).
    let svg = nowview_out(&db, &[&args[..], &["--labels"]].concat(), ".svg");
    assert!(svg.contains(">mood: before</text>"), "{svg}");
    assert!(!svg.contains("mood: after"), "{svg}");

    assert_eq!(
        nowview_json(&db, &args)["panel"][0]["state_label"],
        "before"
    );
}

// ---------------------------------------------------------------------
// da-7q8r: the SVG's default is the snapshot again, the labels are behind
// `--labels` and sit off to the right, and `--context` is the name of the
// window `--history` used to be the only name for.
// ---------------------------------------------------------------------

/// vividn, 2026-09-21: «Before without --history, it would just output the
/// current state, rendered as blocks. That's what this should be.» The
/// default rode to 30m in da-6jy while the history portion moved onto
/// dayview's renderer; it is `0` again, and `0` still spells it.
#[test]
fn nowview_svg_default_is_the_snapshot() {
    let db = two_switch_db();
    // A held clock, so the three renders below are comparable byte for
    // byte: the day's `data-from`/`data-to` are the instant itself.
    let at = |args: &'static [&'static str]| {
        at_clock("2026-09-10T14:20:00Z", || nowview_out(&db, args, ".svg"))
    };
    let bare = at(&[]);
    // The snapshot's width, its one chunk, and no time band at all.
    assert!(bare.contains(r#"width="100""#), "{bare}");
    assert_eq!(field_rects(&bare, "mood", "now-chunk").len(), 1, "{bare}");
    for absent in [
        r#"class="axis""#,
        r#"class="tick""#,
        "tick-label",
        "gridline",
        "current-time-line",
    ] {
        assert!(
            !bare.contains(absent),
            "the default carries {absent}: {bare}"
        );
    }
    // And it is bare: the labels da-pcis drew are behind `--labels` now.
    assert!(!bare.contains("state-label"), "{bare}");
    assert!(!bare.contains("nowview-labels"), "{bare}");

    // `--history 0` is the same picture, not a no-op and not an error.
    assert_eq!(at(&["--history", "0"]), bare);
    assert_eq!(at(&["--context", "0"]), bare);
    // The panel the web and the watchface read says so too.
    assert_eq!(
        nowview_json(&db, &[])["history_minutes"].as_f64(),
        Some(0.0)
    );
}

/// The gutter says a running start field «on», as `tail` does, where it
/// said the stored «true»; a switch field's state reads as it is
/// (da-251lt, da-66c3l).
#[test]
fn nowview_labels_say_a_running_start_field_on() {
    let db = fresh_db();
    entry_builder("work")
        .verb(Verb::Switch)
        .state(serde_json::json!(true))
        .time_start_utc(Utc::now() - Duration::minutes(10))
        .insert(&db);
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(Utc::now() - Duration::minutes(10))
        .insert(&db);

    let svg = nowview_out(&db, &["--labels"], ".svg");
    assert!(svg.contains(">work: on</text>"), "no «work: on» in: {svg}");
    assert!(svg.contains(">mood: focused</text>"), "{svg}");
    assert!(!svg.contains("work: true"), "{svg}");
}

/// `--labels` names each field's state in a gutter PAST the picture's
/// right edge — vividn, «there can be a --labels flag, which then puts
/// labels (but off to the right)». Before the bead they were drawn over
/// the colour they named, which is what sent them behind a flag.
#[test]
fn nowview_labels_sit_in_a_gutter_off_to_the_right() {
    let db = fresh_db();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(Utc::now() - Duration::minutes(10))
        .insert(&db);

    let svg = nowview_out(&db, &["--labels"], ".svg");
    assert!(svg.contains(">mood: focused</text>"), "no label in: {svg}");

    // The label starts past the right edge of every band, and the canvas
    // grew by the gutter rather than the label lying over the colour.
    let chunk = field_rects(&svg, "mood", "now-chunk")
        .into_iter()
        .next()
        .expect("a now-chunk");
    let label_x = attr_f64(&svg, r#"<text class="state-label" x=""#);
    assert!(
        label_x >= chunk.x + chunk.w,
        "label at {label_x} is over a band ending at {}: {svg}",
        chunk.x + chunk.w
    );
    let width = attr_f64(
        &svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width=""#,
    );
    assert!(
        width > label_x,
        "the label runs off a {width}px canvas: {svg}"
    );
    assert!(
        width > 100.0,
        "the canvas did not grow for the gutter: {svg}"
    );

    // `--labels` means the same thing on the window a time flag centres —
    // the other SVG path — rather than being silently ignored there.
    let at = nowview_out(&db, &["--context", "30m", "-t", "-5", "--labels"], ".svg");
    assert!(at.contains(">mood: focused</text>"), "{at}");
    assert!(
        !nowview_out(&db, &["--context", "30m", "-t", "-5"], ".svg").contains("state-label"),
        "unasked-for label on the centred window"
    );
}

/// `--context` is the primary name and `--history` is the same flag, so
/// the two render the same picture. vividn's reason: the window is read
/// against whatever instant `-t`/`-d` names, and "history" only reads as
/// the past.
#[test]
fn nowview_context_and_history_are_one_flag() {
    let db = two_switch_db();
    let args = |flag: &'static str, spelling: String| -> Vec<String> {
        vec![
            flag.to_string(),
            spelling,
            "-z".into(),
            "UTC".into(),
            "-d".into(),
            "2026-09-10".into(),
            "-t".into(),
            "14:00".into(),
        ]
    };
    let run = |a: Vec<String>| {
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        nowview_out(&db, &refs, ".svg")
    };
    let context = run(args("--context", "30m".into()));
    assert_eq!(run(args("--history", "30m".into())), context);
    // Attached spelling too.
    assert_eq!(
        run(vec!["--context=30m".into()]
            .into_iter()
            .chain(args("--history", "30m".into()).into_iter().skip(2))
            .collect()),
        context
    );
    // And it is a real window, not the snapshot.
    assert_eq!(field_rects(&context, "mood", "block").len(), 2, "{context}");
}

/// The terminal band reads the same flag (da-7q8r), so `--context` means
/// one thing on both render paths: a window is the Flow layout, none is
/// the Snapshot. `--window` is the older spelling and still wins.
#[test]
fn nowview_terminal_reads_the_context_window() {
    let db = two_switch_db();
    for argv in [
        &["nowview", "--context", "30m"][..],
        &["nowview", "--history", "30m"],
        &["nowview", "--context", "0"],
        &["nowview", "--context", "30m", "--window", "15m"],
    ] {
        run_cli(argv, &db).unwrap_or_else(|e| panic!("{argv:?}: {e:#}"));
    }
    // An explicit `--window 0m` is still refused; `--context 0` is not.
    assert!(run_cli(&["nowview", "--window", "0m"], &db).is_err());
}

/// The first `name` attribute in `svg`, as a number.
fn attr_f64(svg: &str, key: &str) -> f64 {
    let a = svg
        .find(key)
        .map(|i| i + key.len())
        .unwrap_or_else(|| panic!("no {key} in {svg}"));
    let b = svg[a..].find('"').expect("attribute close") + a;
    svg[a..b].parse().expect("number")
}

// ---------------------------------------------------------------------
// da-78evn: `--orientation vertical` turns the picture on its side.
// ---------------------------------------------------------------------

/// The attribute strings of every tag in `svg` that opens with `open`.
fn tags<'a>(svg: &'a str, open: &str) -> Vec<&'a str> {
    svg.match_indices(open)
        .map(|(i, _)| {
            // From the space before the first attribute, so `num` finds it.
            let s = i + open.len() - 1;
            &svg[s..svg[s..].find('>').expect("tag close") + s]
        })
        .collect()
}

/// Attribute `name` of one tag's attribute string, as a number.
fn num(attrs: &str, name: &str) -> f64 {
    attr_f64(attrs, &format!(r#" {name}=""#))
}

#[test]
fn nowview_vertical_turns_the_picture_on_its_side() {
    let db = fresh_db();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(Utc::now() - Duration::minutes(20))
        .insert(&db);
    let size = |svg: &str| (num(svg, "width"), num(svg, "height"));

    // The canvas swaps, defaults included: 400×300 becomes 300×400, and
    // the snapshot's 100×300 becomes 300×100. `--vertical` is the same ask.
    let h = nowview_out(&db, &["--context", "30m"], ".svg");
    let v = nowview_out(
        &db,
        &["--context", "30m", "--orientation", "vertical"],
        ".svg",
    );
    assert_eq!(size(&h), (400.0, 300.0));
    assert_eq!(size(&v), (300.0, 400.0), "{v}");
    let short = nowview_out(&db, &["--context", "30m", "--vertical"], ".svg");
    assert_eq!(size(&short), size(&v));
    let snap = nowview_out(&db, &["--vertical"], ".svg");
    assert_eq!(size(&snap), (300.0, 100.0), "{snap}");
    // `--width`/`--height` stay the SVG's own in either orientation.
    let asked = nowview_out(
        &db,
        &[
            "--context",
            "30m",
            "--vertical",
            "--width",
            "390",
            "--height",
            "520",
        ],
        ".svg",
    );
    assert_eq!(size(&asked), (390.0, 520.0));

    // Time runs DOWN: the now-line crosses the picture, and the lane's
    // state runs on below it to the bottom edge.
    let line = tags(&v, r#"<line class="current-time-line" "#)[0];
    let now_y = num(line, "y1");
    assert_eq!(now_y, num(line, "y2"), "now-line is horizontal: {line}");
    // It crosses the axis strip on the left, as the horizontal one crosses
    // the strip below, and stops at the right edge (plot-local: the canvas
    // less its 2px margin each side).
    assert_eq!(
        (num(line, "x1"), num(line, "x2")),
        (0.0, 300.0 - 4.0),
        "{line}"
    );
    let chunk = tags(&v, r#"<rect class="now-chunk" "#)[0];
    assert!(
        (num(chunk, "y") - now_y).abs() < 1e-6,
        "chunk starts at now: {chunk}"
    );
    let bottom = num(chunk, "y") + num(chunk, "height");
    assert!(
        bottom <= 400.0 - 2.0 * 2.0 + 1e-6,
        "chunk runs off the canvas: {chunk}"
    );

    // The axis labels stack down one column, each inside the canvas: the
    // strip is as wide as its text (translate(2,2) is the margin).
    let labels = tags(&v, r#"<text class="tick-label" "#);
    assert!(labels.len() >= 3, "{v}");
    let x = num(labels[0], "x");
    for (i, l) in labels.iter().enumerate() {
        assert_eq!(num(l, "x"), x, "one column: {l}");
        if i > 0 {
            assert!(num(l, "y") > num(labels[i - 1], "y"), "time runs down: {v}");
        }
    }
    let widest = v
        .split(r#"<text class="tick-label" "#)
        .skip(1)
        .map(|t| {
            t[t.find('>').unwrap() + 1..t.find("</text>").unwrap()]
                .chars()
                .count()
        })
        .max()
        .unwrap();
    let font = num(labels[0], "font-size");
    assert!(
        x - widest as f64 * font * 0.6 >= 0.0,
        "a {widest}-char label at x={x} runs off the left edge: {v}"
    );
}

#[test]
fn nowview_vertical_labels_sit_below_the_picture() {
    let db = fresh_db();
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(serde_json::json!("focused"))
        .time_start_utc(Utc::now() - Duration::minutes(20))
        .insert(&db);
    let v = nowview_out(&db, &["--context", "30m", "--vertical", "--labels"], ".svg");
    let chunk = tags(&v, r#"<rect class="now-chunk" "#)[0];
    let label = tags(&v, r#"<text class="state-label" "#)[0];
    // Past the far end of time — below the now strip — written down the
    // lane it names, and the canvas taller by what it needs.
    assert!(
        num(label, "y") > num(chunk, "y") + num(chunk, "height"),
        "{v}"
    );
    let lane_mid = num(chunk, "x") + num(chunk, "width") / 2.0;
    assert!(
        (num(label, "x") - lane_mid).abs() < 1e-6,
        "{label} vs {chunk}"
    );
    assert!(label.contains("rotate(90 "), "{label}");
    assert_eq!(num(&v, "width"), 300.0);
    assert!(num(&v, "height") > 400.0, "{v}");
}

/// Run `datum <cmd>` on a real terminal 100 columns by 30 rows (`script` +
/// `stty`) and return its lines. `stty rows` matters: a pty of 0 rows fails
/// the terminal probe, and `$COLUMNS` would then win for the wrong reason.
fn on_a_terminal(cmd: &str) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let bin = env!("CARGO_BIN_EXE_datum");
    let out = std::process::Command::new("script")
        .args(["-qc", &format!("stty rows 30 cols 100; {cmd}"), "/dev/null"])
        .env("DATUM_DB", dir.path().join("t.db"))
        .env("HOME", dir.path())
        .env("NO_COLOR", "1")
        // A terminal is where the one-time completion tip prints (da-bu09u).
        .env("DATUM_NO_TIPS", "1")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", bin.rsplit_once('/').unwrap().0),
        )
        .env_remove("COLUMNS")
        .env_remove("LINES")
        .output()
        .expect("run script");
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim_end_matches('\r').into())
        .collect();
    assert!(lines.len() > 1, "{cmd}: {text:?}");
    lines
}

fn widest(lines: &[String]) -> usize {
    lines
        .iter()
        .map(|l| datum_cli::output::term_layout::visible_width(l))
        .max()
        .unwrap()
}

/// da-q0xe5: a ONE-SHOT terminal render sizes to what it was given —
/// `--cols`, then `$COLUMNS`/`$LINES`, then the terminal — even when the
/// terminal is wider; `--watch` asks the live terminal first, so a stale
/// `$COLUMNS` loses to it. The rule da-coi0m set for `now`.
#[test]
fn nowview_one_shot_reads_columns_and_watch_the_live_terminal() {
    let seed = "datum switch mood happy -t -20 >/dev/null; datum start work -t -10 >/dev/null";
    for (cmd, cols) in [
        ("COLUMNS=60 datum nowview", 60),
        ("COLUMNS=60 datum nowview --context 30m", 60),
        ("COLUMNS=60 datum nowview --cols 50", 50),
        ("datum nowview", 100),
        ("COLUMNS=60 timeout 2 datum nowview --watch", 100),
    ] {
        let lines = on_a_terminal(&format!("{seed}; {cmd}"));
        assert_eq!(widest(&lines), cols, "{cmd}: {lines:#?}");
        assert_eq!(lines.len(), 30, "{cmd}: {lines:#?}");
    }
    let lines = on_a_terminal(&format!("{seed}; LINES=10 datum nowview"));
    assert_eq!((lines.len(), widest(&lines)), (10, 100), "{lines:#?}");
}

/// da-qsd01: `tail`'s one-shot render is the same rule — `$COLUMNS` caps the
/// width and `$LINES` the default row count, even in a wider, taller terminal.
#[test]
fn tail_one_shot_reads_columns_and_lines_before_the_terminal() {
    let long = "a-state-long-enough-to-want-more-than-sixty-columns-of-table";
    let seed = format!(
        "datum switch mood {long} -t -60 >/dev/null; \
         for i in 1 2 3 4 5 6 7 8 9 10 11 12; do datum occur tick -t -$i >/dev/null; done"
    );
    let size = |cmd: &str| {
        let lines = on_a_terminal(&format!("{seed}; {cmd}"));
        (lines.len(), widest(&lines))
    };
    let (rows, cols) = size("datum tail mood");
    assert!(
        cols > 60,
        "the terminal's 100 leaves the state whole: {cols}"
    );
    let (fit_rows, fit_cols) = size("COLUMNS=60 datum tail mood");
    assert!(fit_rows == rows && fit_cols <= 60, "{fit_rows}x{fit_cols}");
    assert_eq!(size("datum tail").0, 14, "13 entries and the header");
    assert_eq!(
        size("LINES=8 datum tail").0,
        7,
        "LINES-2 entries and the header"
    );
}

/// da-q0xe5's guard: the terminal-first probes (`detect_cols` / `detect_rows`
/// and their `_or` forms) belong to LIVE views, where a resize must beat a
/// stale `$COLUMNS`. One-shot output reads `given_cols` / `given_rows`. A new
/// call site fails here, by name: if it is on a `--watch` path, list it.
#[test]
fn every_terminal_first_probe_is_on_a_live_path() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    let mut dirs = vec![crates.clone()];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            if p.is_dir() && !matches!(name.as_str(), "target" | "tests" | "node_modules") {
                dirs.push(p);
            } else if name.ends_with(".rs") && name != "term_layout.rs" {
                let rel = p.strip_prefix(&crates).unwrap().display().to_string();
                for line in std::fs::read_to_string(&p).unwrap().lines().map(str::trim) {
                    let probe = [
                        "detect_cols(",
                        "detect_rows(",
                        "detect_cols_or(",
                        "detect_rows_or(",
                    ];
                    if !line.starts_with("//") && probe.iter().any(|c| line.contains(c)) {
                        found.push(format!("{rel}: {line}"));
                    }
                }
            }
        }
    }
    found.sort();
    let tail = "datum-commands/src/commands/tail.rs";
    let nowview = "datum-commands/src/commands/nowview.rs";
    let mut allowed = vec![
        // `nowview --watch`.
        format!("{nowview}: term_layout::detect_cols_or(4, 40),"),
        format!("{nowview}: term_layout::detect_rows_or(3, 24),"),
        // `tail --watch`'s frame signature and repaint loop.
        format!("{tail}: (detect_cols(), detect_rows())"),
        format!("{tail}: Some(auto_fill_rows(detect_rows()))"),
        format!("{tail}: let max_total_width = if tty {{ Some(detect_cols()) }} else {{ None }};"),
    ];
    allowed.sort();
    assert_eq!(found, allowed, "a terminal-first probe off a --watch path?");
}

/// da-61nm8: `--surround` draws the time either side of the window outside
/// the SVG's box — the box and the window's ticks as they were, the coffee
/// 40 minutes back drawn though the window reaches 30.
#[test]
fn nowview_surround_draws_past_the_box() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "-t", "-40"], &db).expect("occur");
    let tmp = tempfile::NamedTempFile::with_suffix(".svg").unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let svg = |extra: &[&str]| {
        let mut argv = vec!["nowview", "--history", "30m", "--width", "400"];
        argv.extend_from_slice(extra);
        argv.extend_from_slice(&["--output-file", &path]);
        run_cli(&argv, &db).expect("nowview dispatch");
        std::fs::read_to_string(&path).expect("read svg")
    };
    let plain = svg(&[]);
    let around = svg(&["--surround", "30m"]);
    let root = |s: &str| {
        s[s.find("<svg").unwrap()..]
            .split('>')
            .next()
            .unwrap()
            .to_string()
    };
    assert!(
        root(&plain).contains(r#"viewBox="0 0 400 300""#),
        "{}",
        root(&plain)
    );
    // The box is the window's; its view starts where the window does, the
    // 30 minutes before it drawn left of that.
    let r = root(&around);
    assert!(r.contains(r#" width="400" height="300""#), "{r}");
    assert!(r.contains(r#" overflow="visible""#), "{r}");
    let view: Vec<f64> = r
        .split(r#"viewBox=""#)
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .split(' ')
        .map(|n| n.parse().unwrap())
        .collect();
    assert!(
        view[0] > 300.0 && view[0] < 400.0 && view[1..] == [0.0, 400.0, 300.0],
        "{r}"
    );
    assert!(
        !plain.contains("point-bg"),
        "the window alone drew the coffee"
    );
    assert!(
        around.contains("point-bg"),
        "the surroundings drew no coffee"
    );
    // The window's own ticks, carried on out: -10m apart, majors labelled.
    assert!(around.contains(">-60m<") && around.contains(">-30m<") && around.contains(">-10m<"));
}
