//! Shared per-field render machinery for the timeline surfaces.
//!
//! Extracted per da-p1dc from `commands/dayview.rs` and `commands/nowview.rs`,
//! which each carried a byte-identical copy of these helpers (and the copies
//! had already begun to drift — doc comments lost, query windows diverging).
//! One copy lives here now; both surfaces import it.
//!
//! What lives here:
//! - the verb-driven walk [`render_field_visuals`] emitting the
//!   [`DayviewBlock`]/[`DayviewPoint`] shapes consumed by the dayview SVG
//!   pipeline (dayview proper, and nowview's SVG path per da-6jy),
//! - the switch-region chaining ([`switch_region_end_ms`]) and emit helpers
//!   ([`push_block_window`], [`push_switch_region`]),
//! - small entry/color utilities shared by both surfaces.
//!
//! nowview's *terminal*-shape walk (`render_field_visuals_terminal` and its
//! push helpers, emitting `NowviewBlock`/`NowviewPoint`) is NOT duplicated —
//! it exists once in `commands/nowview.rs` and stays there.

use datum_core::db::{Db, IGNORED_STATE_VALUE};
use datum_core::entry::{Entry, Verb};
use datum_core::field_spec::FieldSpec;
use datum_core::shape::{
    dot_bg_uses_following_state, dot_fg_uses_state, is_block_verb, is_switching_verb,
    is_unrendered_switch_value, opens_secondary_switch_region, render_shape, RenderShape,
};
use datum_layout::dayview::{BlockFill, DayviewBlock, DayviewPoint};
use serde_json::Value as JsonValue;

use crate::render::colors::{field_rgb, md5_rgb, state_value_rgb, Rgb};

/// Resolve a block's fill from its value. Arrays → multi-state stripes
/// (one color per element); scalars → single solid color.
pub(crate) fn block_fill_for_value(
    value: Option<&JsonValue>,
    field: &str,
    spec: Option<&FieldSpec>,
) -> BlockFill {
    match value {
        Some(JsonValue::Array(items)) if !items.is_empty() => {
            let colors: Vec<String> = items
                .iter()
                .map(|v| {
                    let rgb = state_value_rgb(v, field, spec).unwrap_or(md5_rgb(field));
                    rgb_to_hex(rgb)
                })
                .collect();
            BlockFill::MultiState(colors)
        }
        Some(v) => {
            // Skip "false" and "IGNORED" by falling back to the field's
            // own md5 color. Datum's `simplifyState` returned `null|false`
            // for these and the original renderer dropped them — we keep
            // them visible but muted via the field color.
            let rgb = if is_ignored_or_false(v) {
                md5_rgb(field)
            } else {
                state_value_rgb(v, field, spec).unwrap_or(md5_rgb(field))
            };
            BlockFill::Single(rgb_to_hex(rgb))
        }
        None => BlockFill::Single(rgb_to_hex(md5_rgb(field))),
    }
}

pub(crate) fn is_ignored_or_false(v: &JsonValue) -> bool {
    matches!(v, JsonValue::Bool(false))
        || matches!(v, JsonValue::String(s) if s == IGNORED_STATE_VALUE)
}

/// For each entry, the UTC-ms end-time of its switch region (the occur_time
/// of the next switching event on the same field, or `fallback_end_ms` if
/// it's the last). Non-switching entries get `fallback_end_ms` (callers only
/// read the switching positions). Input must be sorted by occur_time
/// ascending.
///
/// A "switching event" is any entry that changes the field's prevailing
/// state at its `occur_time`: Switch (state), BlockSwitch (following_state),
/// or PointSwitch (following_state). Block / Point / Add / Occur do not
/// switch the prevailing state.
pub(crate) fn switch_region_end_ms(entries: &[Entry], fallback_end_ms: i64) -> Vec<i64> {
    let mut out = vec![fallback_end_ms; entries.len()];
    let mut next_occur_ms: Option<i64> = None;
    for (i, entry) in entries.iter().enumerate().rev() {
        if is_switching_verb(canonical_verb(entry)) {
            out[i] = next_occur_ms.unwrap_or(fallback_end_ms);
            if let Some(ms) = occur_time_ms_opt(entry) {
                next_occur_ms = Some(ms);
            }
        }
    }
    out
}

/// Canonical verb for an entry. da-5ns.3 wired `derive_verb` into the
/// insert/update path so the stored `verb` column is authoritative; the
/// render dispatches on it directly.
pub(crate) fn canonical_verb(entry: &Entry) -> Verb {
    entry.verb
}

/// Sort key for an entry's effective time. Falls back to i64::MIN so
/// time-less rows sink to the front (they're skipped during render anyway).
pub(crate) fn occur_time_ms(entry: &Entry) -> i64 {
    occur_time_ms_opt(entry).unwrap_or(i64::MIN)
}

pub(crate) fn occur_time_ms_opt(entry: &Entry) -> Option<i64> {
    entry.occur_time.as_ref().map(|t| t.utc.timestamp_millis())
}

/// Parse `entry.dur` as seconds. Returns `None` for a missing or unparseable
/// duration string. Negative durations are clamped to zero.
pub(crate) fn dur_seconds(entry: &Entry) -> Option<i64> {
    entry.duration_seconds().map(|s| s.max(0))
}

/// Verb-driven render of one field's blocks and points in the dayview shape.
///
/// Used by `datum dayview` for each calendar day, and by `datum nowview`'s
/// SVG path with the nowview scope `[end - history, end]` as the window
/// (per da-6jy nowview reuses the dayview pipeline).
///
/// Spec mapping (per da-5ns.5):
/// - **Block**: emit a colored block for `[occur - dur, occur]` using
///   `state` as the fill.
/// - **Block-switch**: same block, AND open a new switch region at
///   `occur` colored `following_state` until the next switching event.
/// - **Point-switch**: a dot at `occur` (fg = state, bg = following_state),
///   AND open a new switch region at `occur` colored `following_state`.
/// - **Point**: a dot at `occur` (fg = field color, bg = state at instant
///   from prior-row lookup).
/// - **Switch**: a colored region from `occur` to the next switching
///   event, fill = `state`.
///
/// `switch_region_ends[idx]` is the UTC-ms end-time of the switch region
/// opened by entry `idx` (or `fallback_end_ms` if it doesn't open one).
pub(crate) fn render_field_visuals(
    entries: &[Entry],
    field: &str,
    spec: Option<&FieldSpec>,
    day_start_ms: i64,
    day_end_ms: i64,
    switch_region_ends: &[i64],
    db: &Db,
) -> (Vec<DayviewBlock>, Vec<DayviewPoint>) {
    let mut blocks: Vec<DayviewBlock> = Vec::new();
    let mut points: Vec<DayviewPoint> = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        let Some(occur_dt) = entry.occur_time.as_ref() else {
            continue;
        };
        let occur_ms = occur_dt.utc.timestamp_millis();

        let verb = canonical_verb(entry);
        // Primary visual dispatch flows from the shape table; verb-specific
        // data sourcing (block fill vs switch region, dot fg/bg origin) is
        // resolved via verb-keyed predicate helpers in `datum_core::shape`.
        // Adding a new verb requires updating the helpers, not this match.
        let shape = render_shape(verb, entry.shape(), &entry.state);
        match shape {
            RenderShape::Hidden | RenderShape::Marker => {}
            RenderShape::Region if is_block_verb(verb) => {
                push_block_window(
                    &mut blocks,
                    entry,
                    occur_ms,
                    field,
                    spec,
                    day_start_ms,
                    day_end_ms,
                );
            }
            RenderShape::Region => {
                // Primary switch region (Verb::Switch).
                push_switch_region(
                    &mut blocks,
                    entry.state.as_ref(),
                    occur_ms,
                    switch_region_ends[idx],
                    field,
                    spec,
                    day_start_ms,
                    day_end_ms,
                );
            }
            RenderShape::Dot => {
                if occur_ms >= day_start_ms && occur_ms <= day_end_ms {
                    let fg = if dot_fg_uses_state(verb) {
                        entry
                            .state
                            .as_ref()
                            .and_then(|v| state_value_rgb(v, field, spec))
                            .unwrap_or_else(|| field_rgb(field, spec))
                    } else {
                        field_rgb(field, spec)
                    };
                    let bg = if dot_bg_uses_following_state(verb) {
                        entry
                            .following_state
                            .as_ref()
                            .and_then(|v| state_value_rgb(v, field, spec))
                            .unwrap_or_else(|| field_rgb(field, spec))
                    } else {
                        match db.state_at(field, occur_dt.utc).unwrap_or(None) {
                            Some(v) => state_value_rgb(&v, field, spec).unwrap_or(Rgb::BLACK),
                            None => Rgb::BLACK,
                        }
                    };
                    points.push(DayviewPoint {
                        time_ms: occur_ms,
                        fg_color: rgb_to_hex(fg),
                        bg_color: rgb_to_hex(bg),
                        state_label: state_label(entry.state.as_ref()),
                    });
                }
            }
        }
        // Secondary trailing switch region for compound switching verbs
        // (BlockSwitch, PointSwitch): a band from `occur_time` colored by
        // `following_state`.
        if opens_secondary_switch_region(verb) {
            push_switch_region(
                &mut blocks,
                entry.following_state.as_ref(),
                occur_ms,
                switch_region_ends[idx],
                field,
                spec,
                day_start_ms,
                day_end_ms,
            );
        }
    }
    (blocks, points)
}

/// Emit a block clipped to the day window for a `[occur - dur, occur]`
/// interval (Block / BlockSwitch). No-op if `dur` is missing or the block
/// falls entirely outside the visible window.
pub(crate) fn push_block_window(
    blocks: &mut Vec<DayviewBlock>,
    entry: &Entry,
    occur_ms: i64,
    field: &str,
    spec: Option<&FieldSpec>,
    day_start_ms: i64,
    day_end_ms: i64,
) {
    let Some(dur_secs) = dur_seconds(entry) else {
        return;
    };
    if dur_secs == 0 {
        return;
    }
    let start_ms = occur_ms - dur_secs * 1000;
    let end_ms = occur_ms;
    if end_ms <= day_start_ms || start_ms >= day_end_ms {
        return;
    }
    let fill = block_fill_for_value(entry.state.as_ref(), field, spec);
    blocks.push(DayviewBlock {
        start_ms,
        end_ms,
        fill,
        state_label: state_label(entry.state.as_ref()),
    });
}

/// Emit a switch-region block from `start_ms` to `region_end_ms`, colored by
/// `value`. No-op when the value is the "ended" sentinel (null/false/IGNORED)
/// or the region falls outside the day window.
// Internal helper — eight params group three independently varying axes
// (block bounds, day window, field/spec). Folding them into a struct
// here would just rename the locals at the call sites.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_switch_region(
    blocks: &mut Vec<DayviewBlock>,
    value: Option<&JsonValue>,
    start_ms: i64,
    region_end_ms: i64,
    field: &str,
    spec: Option<&FieldSpec>,
    day_start_ms: i64,
    day_end_ms: i64,
) {
    if is_unrendered_switch_value(value) {
        return;
    }
    if region_end_ms <= day_start_ms || start_ms >= day_end_ms {
        return;
    }
    let fill = block_fill_for_value(value, field, spec);
    blocks.push(DayviewBlock {
        start_ms,
        end_ms: region_end_ms,
        fill,
        state_label: state_label(value),
    });
}

pub(crate) fn rgb_to_hex(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)
}

pub(crate) fn state_label(v: Option<&JsonValue>) -> Option<String> {
    v.map(|val| match val {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    })
}

/// Test helper: build a stub Entry with the desired (verb, occur_time, dur,
/// state, following_state) shape. The slug/hid/content_hash are placeholders
/// — the renderer does not read them. Shared with the nowview tests.
#[cfg(test)]
pub(crate) fn stub_entry(
    field: &str,
    verb: Verb,
    occur_utc: chrono::DateTime<chrono::Utc>,
    dur: Option<&str>,
    state: Option<JsonValue>,
    following_state: Option<JsonValue>,
) -> Entry {
    use datum_core::time::DatumTime;
    let dt = DatumTime::from_utc_and_tz(occur_utc, "UTC").unwrap();
    let mut e = Entry::new(
        field,
        verb,
        state.clone(),
        None,
        Some(dt),
        dur.and_then(datum_core::dur::Dur::parse),
        "test",
    );
    e.verb = verb;
    e.following_state = following_state;
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn rgb_to_hex_lowercase_six_digit() {
        assert_eq!(
            rgb_to_hex(Rgb {
                r: 0xab,
                g: 0x12,
                b: 0xcd
            }),
            "#ab12cd"
        );
        assert_eq!(rgb_to_hex(Rgb { r: 0, g: 0, b: 0 }), "#000000");
    }

    #[test]
    fn state_label_serializes_non_string_values() {
        assert_eq!(
            state_label(Some(&JsonValue::String("happy".into()))),
            Some("happy".into())
        );
        assert_eq!(
            state_label(Some(&JsonValue::Number(serde_json::Number::from(7)))),
            Some("7".into())
        );
        assert_eq!(state_label(None), None);
    }

    #[test]
    fn block_fill_array_yields_multi_state() {
        let v = JsonValue::Array(vec![
            JsonValue::String("happy".into()),
            JsonValue::String("calm".into()),
        ]);
        match block_fill_for_value(Some(&v), "mood", None) {
            BlockFill::MultiState(colors) => assert_eq!(colors.len(), 2),
            _ => panic!("expected multi-state fill"),
        }
    }

    #[test]
    fn block_fill_scalar_yields_single() {
        let v = JsonValue::String("happy".into());
        match block_fill_for_value(Some(&v), "mood", None) {
            BlockFill::Single(c) => assert!(c.starts_with('#')),
            _ => panic!("expected single fill"),
        }
    }

    #[test]
    fn block_fill_empty_array_falls_back_to_field_color() {
        let v = JsonValue::Array(vec![]);
        match block_fill_for_value(Some(&v), "mood", None) {
            BlockFill::Single(_) => {}
            _ => panic!("expected single fill on empty array"),
        }
    }

    #[test]
    fn block_fill_none_uses_field_color() {
        match block_fill_for_value(None, "mood", None) {
            BlockFill::Single(c) => assert_eq!(c, rgb_to_hex(md5_rgb("mood"))),
            _ => panic!("expected single fill"),
        }
    }

    #[test]
    fn switch_region_end_ms_chains_consecutive_switches() {
        use datum_core::time::DatumTime;

        let mk = |hour: u32, value: &str| {
            let utc = Utc.with_ymd_and_hms(2026, 4, 27, hour, 0, 0).unwrap();
            let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
            Entry::new(
                "mood",
                Verb::Switch,
                Some(JsonValue::String(value.into())),
                None,
                Some(dt),
                None,
                "test",
            )
        };
        let entries = vec![mk(8, "happy"), mk(12, "calm"), mk(16, "focus")];
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        assert_eq!(ends.len(), 3);
        // First switch ends at second switch's start.
        assert_eq!(
            ends[0],
            Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        // Second switch ends at third switch's start.
        assert_eq!(
            ends[1],
            Utc.with_ymd_and_hms(2026, 4, 27, 16, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        // Last switch falls back to day_end.
        assert_eq!(ends[2], day_end);
    }

    /// `switch_region_end_ms` chains across all three switching verbs
    /// (Switch / BlockSwitch / PointSwitch). Each one terminates the prior
    /// switching event's region.
    #[test]
    fn switch_region_end_ms_chains_across_all_switching_verbs() {
        let t1 = Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 4, 27, 16, 0, 0).unwrap();
        let entries = vec![
            stub_entry(
                "f",
                Verb::Switch,
                t1,
                None,
                Some(JsonValue::String("a".into())),
                None,
            ),
            stub_entry(
                "f",
                Verb::BlockSwitch,
                t2,
                Some("PT5M"),
                Some(JsonValue::String("via".into())),
                Some(JsonValue::String("b".into())),
            ),
            stub_entry(
                "f",
                Verb::PointSwitch,
                t3,
                Some("PT0S"),
                Some(JsonValue::String("via".into())),
                Some(JsonValue::String("c".into())),
            ),
        ];
        let fallback = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, fallback);
        assert_eq!(ends[0], t2.timestamp_millis());
        assert_eq!(ends[1], t3.timestamp_millis());
        assert_eq!(ends[2], fallback);
    }

    // ─── da-5ns.5: verb-driven render tests ────────────────────────────
    //
    // These construct Entry structs directly and call the pure render
    // function. State_at queries are satisfied by an empty in-memory Db.

    /// Block at [occur - dur, occur] — `occur_time` is the trailing edge.
    #[test]
    fn block_renders_at_occur_minus_dur_to_occur() {
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 12, 30, 0).unwrap();
        let entries = vec![stub_entry(
            "lunch",
            Verb::Block,
            occur,
            Some("PT30M"),
            Some(JsonValue::String("eating".into())),
            None,
        )];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, "lunch", None, day_start, day_end, &ends, &db);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].start_ms,
            occur.timestamp_millis() - 30 * 60 * 1000
        );
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].state_label.as_deref(), Some("eating"));
    }

    /// Block whose `[occur - dur, occur]` interval crosses the day boundary.
    /// The renderer emits the block; the layout clips it to the visible
    /// window (verified separately in `dayview.rs::tests::blocks_clip_to_day_window`).
    #[test]
    fn block_crossing_day_boundary_emits_full_interval() {
        let db = Db::open_in_memory().unwrap();
        // 2-hour block ending at 01:00 on day-2 → spans the prior midnight.
        let occur = Utc.with_ymd_and_hms(2026, 4, 28, 1, 0, 0).unwrap();
        let entries = vec![stub_entry(
            "sleep",
            Verb::Block,
            occur,
            Some("PT2H"),
            Some(JsonValue::String("dozing".into())),
            None,
        )];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 29, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, _points) =
            render_field_visuals(&entries, "sleep", None, day_start, day_end, &ends, &db);
        assert_eq!(blocks.len(), 1);
        let expected_start = occur.timestamp_millis() - 2 * 60 * 60 * 1000;
        assert_eq!(blocks[0].start_ms, expected_start);
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
        // Block start sits in the prior day — confirms the renderer doesn't
        // self-clip; the layout owns clipping.
        assert!(blocks[0].start_ms < day_start);
    }

    /// Block-switch emits the leading block AND opens a new switch region
    /// at occur_time colored by `following_state`. With visibly distinct
    /// colors, the two emitted regions must hold distinct fills.
    #[test]
    fn block_switch_emits_block_plus_following_region_with_distinct_colors() {
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let entries = vec![stub_entry(
            "mood",
            Verb::BlockSwitch,
            occur,
            Some("PT1H"),
            Some(JsonValue::String("groggy".into())),
            Some(JsonValue::String("focused".into())),
        )];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, "mood", None, day_start, day_end, &ends, &db);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 2);
        // Block: [occur - 1h, occur] colored "groggy".
        assert_eq!(
            blocks[0].start_ms,
            occur.timestamp_millis() - 60 * 60 * 1000
        );
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].state_label.as_deref(), Some("groggy"));
        // Following region: [occur, day_end] colored "focused".
        assert_eq!(blocks[1].start_ms, occur.timestamp_millis());
        assert_eq!(blocks[1].end_ms, day_end);
        assert_eq!(blocks[1].state_label.as_deref(), Some("focused"));
        // Distinct colors: state ("groggy") and following_state ("focused")
        // hash to different RGBs via md5_rgb.
        match (&blocks[0].fill, &blocks[1].fill) {
            (BlockFill::Single(a), BlockFill::Single(b)) => assert_ne!(a, b),
            _ => panic!("expected single-fill blocks"),
        }
    }

    /// Point-switch: a dot at occur (state passed through) AND a switch
    /// region opening at occur colored by `following_state`.
    #[test]
    fn point_switch_emits_dot_plus_following_region() {
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 14, 30, 0).unwrap();
        let entries = vec![stub_entry(
            "task",
            Verb::PointSwitch,
            occur,
            Some("PT0S"),
            Some(JsonValue::String("commit".into())),
            Some(JsonValue::String("review".into())),
        )];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, "task", None, day_start, day_end, &ends, &db);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].time_ms, occur.timestamp_millis());
        assert_eq!(points[0].state_label.as_deref(), Some("commit"));
        // fg = state color, bg = following_state color → must differ for
        // visibly distinct labels.
        assert_ne!(points[0].fg_color, points[0].bg_color);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].end_ms, day_end);
        assert_eq!(blocks[0].state_label.as_deref(), Some("review"));
    }

    /// A block-switch followed later by a switch ends the following-state
    /// region at the switch's occur_time, not at day_end.
    #[test]
    fn block_switch_following_region_ends_at_next_switch() {
        let db = Db::open_in_memory().unwrap();
        let bs_occur = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let switch_occur = Utc.with_ymd_and_hms(2026, 4, 27, 14, 0, 0).unwrap();
        let entries = vec![
            stub_entry(
                "mood",
                Verb::BlockSwitch,
                bs_occur,
                Some("PT30M"),
                Some(JsonValue::String("groggy".into())),
                Some(JsonValue::String("focused".into())),
            ),
            stub_entry(
                "mood",
                Verb::Switch,
                switch_occur,
                None,
                Some(JsonValue::String("tired".into())),
                None,
            ),
        ];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, _points) =
            render_field_visuals(&entries, "mood", None, day_start, day_end, &ends, &db);
        // 3 blocks expected: leading "groggy" block, "focused" region from
        // 9:00→14:00, and the "tired" switch region from 14:00→day_end.
        assert_eq!(blocks.len(), 3);
        // Find the focused region.
        let focused = blocks
            .iter()
            .find(|b| b.state_label.as_deref() == Some("focused"))
            .expect("focused region present");
        assert_eq!(focused.start_ms, bs_occur.timestamp_millis());
        assert_eq!(focused.end_ms, switch_occur.timestamp_millis());
    }

    /// Block-switch where `following_state` is the "ended" sentinel (false /
    /// null / IGNORED) emits the leading block but no following region.
    #[test]
    fn block_switch_with_unrendered_following_state_emits_only_leading_block() {
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap();
        let entries = vec![stub_entry(
            "mood",
            Verb::BlockSwitch,
            occur,
            Some("PT15M"),
            Some(JsonValue::String("hungry".into())),
            Some(JsonValue::Bool(false)),
        )];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, "mood", None, day_start, day_end, &ends, &db);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].state_label.as_deref(), Some("hungry"));
    }
}
