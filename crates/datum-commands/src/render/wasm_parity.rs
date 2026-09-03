//! The browser's drawing must be the CLI's drawing.
//!
//! `datum-layout-wasm` holds the browser's half of two things this crate
//! owns: the colour rules in `super::colors` (plus `hash_y` from
//! `datum_core::field_spec`), and the entry→marks walk in
//! `commands::field_visuals`. Neither can simply call this crate —
//! `datum-core` pulls rusqlite and cannot reach `wasm32-unknown-unknown` —
//! so the wasm crate carries its own, and this module holds the two side
//! by side.
//!
//! The colour half checks the inputs that matter: an md5-derived field
//! colour, a named state, the `IGNORED` and boolean sentinels, a parallel
//! state set, spec overrides, and the deterministic cross-axis position.
//!
//! The walk half (da-t7ud) runs the same entries through
//! `render_field_visuals` and through `datum_layout_wasm::request::
//! build_fields`, and compares the blocks and points that come out —
//! band, dot and nothing, plus the compound verbs. The verb→shape table
//! underneath is not duplicated at all: both sides call
//! `datum_parse::shape::render_shape`.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use datum_core::action::Action;
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::field_spec::FieldSpec;
use datum_layout::dayview::{DayviewBlock, DayviewPoint};
use datum_layout_wasm::colors as wasm;
use datum_layout_wasm::marks::EntryReq;
use datum_layout_wasm::request::{build_fields, FieldReq, Window};
use serde_json::{json, Value as JsonValue};

use super::colors::{field_rgb, md5_rgb, state_value_colors, state_value_rgb};
use crate::commands::field_visuals::{render_field_visuals, rgb_to_hex, switch_region_end_ms};

/// The fields whose colours a browser pane is most likely to paint.
const NAMES: &[&str] = &[
    "mood", "coffee", "sleep", "project", "word", "happy", "deep",
];

#[test]
fn md5_colours_match_the_cli() {
    for name in NAMES {
        assert_eq!(
            wasm::md5_hex(name),
            rgb_to_hex(md5_rgb(name)),
            "md5 colour drifted for {name}"
        );
    }
}

#[test]
fn cross_axis_position_matches_the_cli() {
    for name in NAMES {
        let resolved = FieldSpec::new(name.to_string(), Action::Occur)
            .presentation
            .resolve_position(name);
        assert!(
            (wasm::hash_y(name) - resolved.y).abs() < 1e-12,
            "hash_y drifted for {name}: {} vs {}",
            wasm::hash_y(name),
            resolved.y
        );
        assert_eq!(wasm::DEFAULT_FIELD_HEIGHT, resolved.height);
    }
}

#[test]
fn state_value_colours_match_the_cli() {
    let values = [
        json!("happy"),
        json!("IGNORED"),
        json!(true),
        json!(false),
        json!(null),
        json!(42),
        json!(["vertical", "sit"]),
        json!({ "id": "proj-1", "title": "ignored for colour" }),
        json!({ "no_id": 1 }),
    ];
    for value in &values {
        let theirs = state_value_rgb(value, "mood", None).map(rgb_to_hex);
        assert_eq!(
            wasm::state_value_hex(value, "mood", None),
            theirs,
            "state colour drifted for {value}"
        );

        let theirs: Vec<String> = state_value_colors(value, "mood", None)
            .into_iter()
            .map(rgb_to_hex)
            .collect();
        assert_eq!(
            wasm::state_value_colors(value, "mood", None),
            theirs,
            "parallel-state colours drifted for {value}"
        );
    }
}

#[test]
fn spec_overrides_match_the_cli() {
    let mut spec = FieldSpec::new("mood".to_string(), Action::Occur);
    spec.color = Some("#ff6600".to_string());
    spec.state_colors
        .insert("happy".to_string(), "00ff00".to_string());

    let style = wasm::FieldStyle {
        color: spec.color.clone(),
        state_colors: spec
            .state_colors
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        ..wasm::FieldStyle::default()
    };

    assert_eq!(
        wasm::field_hex("mood", Some(&style)),
        rgb_to_hex(field_rgb("mood", Some(&spec)))
    );
    assert_eq!(
        wasm::state_value_hex(&json!("happy"), "mood", Some(&style)),
        state_value_rgb(&json!("happy"), "mood", Some(&spec)).map(rgb_to_hex)
    );
    // A state with no override still hashes its own name, not the field's.
    assert_eq!(
        wasm::state_value_hex(&json!("sad"), "mood", Some(&style)),
        state_value_rgb(&json!("sad"), "mood", Some(&spec)).map(rgb_to_hex)
    );
}

// ─── the walk ────────────────────────────────────────────────────────────

/// The window both halves draw: one UTC day, 2026-04-27.
fn day_bounds() -> (i64, i64) {
    let start = at(0, 0).timestamp_millis();
    (start, start + 86_400_000)
}

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 27, hour, minute, 0).unwrap()
}

/// One entry, built the way both halves will read it.
fn entry(
    verb: Verb,
    hour: u32,
    dur: Option<&str>,
    state: Option<JsonValue>,
    following_state: Option<JsonValue>,
) -> Entry {
    let dt = datum_core::time::DatumTime::from_utc_and_tz(at(hour, 0), "UTC").unwrap();
    let mut e = Entry::new(
        "mood",
        verb,
        state,
        None,
        Some(dt),
        dur.and_then(datum_core::dur::Dur::parse),
        "test",
    );
    e.verb = verb;
    e.following_state = following_state;
    e
}

fn as_req(e: &Entry) -> EntryReq {
    EntryReq {
        occur_ms: e.occur_time.as_ref().map(|t| t.utc.timestamp_millis()),
        verb: e.verb,
        dur: e.dur.as_ref().map(|d| d.to_storage_string()),
        state: e.state.clone(),
        following_state: e.following_state.clone(),
        slug: e.slug.to_string(),
    }
}

/// Run `entries` through both halves, assert the marks agree, and hand
/// back the agreed `(blocks, points)` so the caller can say what it
/// expected to see — an assertion that two empty vectors match would pass
/// for the wrong reason.
///
/// The CLI half needs a database (a plain dot's background is
/// `Db::state_at`), so the same entries are inserted and the browser half
/// is given the same universe to scan.
fn same_marks(case: &str, entries: Vec<Entry>) -> (Vec<DayviewBlock>, Vec<DayviewPoint>) {
    let db = Db::open_in_memory().unwrap();
    for e in &entries {
        db.insert_entry(e).unwrap();
    }
    let (day_start_ms, day_end_ms) = day_bounds();
    let fallback_end_ms = day_end_ms;
    let ends = switch_region_end_ms(&entries, fallback_end_ms);
    let (cli_blocks, cli_points) =
        render_field_visuals(&entries, "mood", None, day_start_ms, day_end_ms, &ends, &db);

    let window = Window {
        start_ms: day_start_ms,
        end_ms: day_end_ms,
        fallback_end_ms,
        lead_start_ms: day_start_ms,
    };
    let fields = build_fields(
        vec![FieldReq {
            field: "mood".to_string(),
            entries: entries.iter().map(as_req).collect(),
        }],
        &BTreeMap::new(),
        &window,
        &BTreeMap::new(),
    );

    let (web_blocks, web_points): (Vec<DayviewBlock>, Vec<DayviewPoint>) =
        match fields.into_iter().next() {
            Some(f) => (f.blocks, f.points),
            // An empty field is dropped rather than drawn; the CLI's own
            // caller (`build_day_fields`) drops it at the same point.
            None => (Vec::new(), Vec::new()),
        };

    assert_eq!(
        serde_json::to_value(&web_blocks).unwrap(),
        serde_json::to_value(&cli_blocks).unwrap(),
        "{case}: blocks drifted"
    );
    assert_eq!(
        serde_json::to_value(&web_points).unwrap(),
        serde_json::to_value(&cli_points).unwrap(),
        "{case}: points drifted"
    );
    (web_blocks, web_points)
}

#[test]
fn a_switch_draws_the_same_band_in_both() {
    let (blocks, points) = same_marks(
        "one switch",
        vec![entry(Verb::Switch, 8, None, Some(json!("happy")), None)],
    );
    assert_eq!((blocks.len(), points.len()), (1, 0));

    let (blocks, points) = same_marks(
        "chained switches",
        vec![
            entry(Verb::Switch, 8, None, Some(json!("happy")), None),
            entry(Verb::Switch, 12, None, Some(json!("calm")), None),
            entry(Verb::Switch, 16, None, Some(json!(["calm", "focus"])), None),
        ],
    );
    assert_eq!((blocks.len(), points.len()), (3, 0));
}

#[test]
fn a_switch_to_nothing_draws_nothing_in_both() {
    for terminator in [
        None,
        Some(JsonValue::Null),
        Some(json!(false)),
        Some(json!("IGNORED")),
    ] {
        let (blocks, points) = same_marks(
            "terminating switch",
            vec![
                entry(Verb::Switch, 8, None, Some(json!("happy")), None),
                entry(Verb::Switch, 12, None, terminator.clone(), None),
            ],
        );
        // The band before it, and nothing for the terminator itself.
        assert_eq!((blocks.len(), points.len()), (1, 0), "{terminator:?}");
    }
}

#[test]
fn a_dot_draws_the_same_in_both() {
    let (blocks, points) = same_marks(
        "a bare point",
        vec![entry(Verb::Point, 9, None, None, None)],
    );
    assert_eq!((blocks.len(), points.len()), (0, 1));

    let (blocks, points) = same_marks(
        "a point over a state",
        vec![
            entry(Verb::Switch, 8, None, Some(json!("happy")), None),
            entry(Verb::Point, 9, None, None, None),
        ],
    );
    assert_eq!((blocks.len(), points.len()), (1, 1));
}

#[test]
fn the_compound_verbs_draw_the_same_in_both() {
    let (blocks, points) = same_marks(
        "a block",
        vec![entry(
            Verb::Block,
            10,
            Some("PT30M"),
            Some(json!("work")),
            None,
        )],
    );
    assert_eq!((blocks.len(), points.len()), (1, 0));

    // The block, plus the trailing region `following_state` opens.
    let (blocks, points) = same_marks(
        "a block-switch",
        vec![entry(
            Verb::BlockSwitch,
            10,
            Some("PT30M"),
            Some(json!("work")),
            Some(json!("break")),
        )],
    );
    assert_eq!((blocks.len(), points.len()), (2, 0));

    // The dot, plus the trailing region.
    let (blocks, points) = same_marks(
        "a point-switch",
        vec![entry(
            Verb::PointSwitch,
            10,
            Some("PT0S"),
            Some(json!("bump")),
            Some(json!("after")),
        )],
    );
    assert_eq!((blocks.len(), points.len()), (1, 1));
}

#[test]
fn an_entry_outside_the_window_is_clipped_away_in_both() {
    // A block whose trailing edge lands before the day starts, and a
    // switch opened the day before that still runs into it.
    let mut early = entry(Verb::Block, 1, Some("PT30M"), Some(json!("work")), None);
    let dt =
        datum_core::time::DatumTime::from_utc_and_tz(at(1, 0) - chrono::Duration::days(1), "UTC")
            .unwrap();
    early.occur_time = Some(dt);
    let (blocks, points) = same_marks("a block before the window", vec![early]);
    assert!(
        blocks.is_empty() && points.is_empty(),
        "the block was not clipped"
    );
}
