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
//! - the switch regions, read off the state oracle ([`switch_regions`]),
//!   and the emit helpers ([`push_block_window`], [`push_switch_region`]),
//! - small entry/color utilities shared by both surfaces.
//!
//! nowview's *terminal*-shape walk (`render_field_visuals_terminal` and its
//! push helpers, emitting `NowviewBlock`/`NowviewPoint`) is NOT duplicated —
//! it exists once in `commands/nowview.rs` and stays there.

use chrono::{DateTime, Utc};
use datum_core::db::IGNORED_STATE_VALUE;
use datum_core::entry::{Entry, Verb};
use datum_core::field_spec::FieldSpec;
use datum_core::shape::is_off_state;
use datum_core::shape::{is_block_verb, is_unrendered_switch_value, render_shape, RenderShape};
use datum_core::state::FieldStateRows;
use datum_core::time::Zone;
use datum_layout::dayview::{BlockFill, DayviewBlock, DayviewPoint};
use serde_json::Value as JsonValue;

use crate::render::action::clock_label;
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
    is_off_state(v) || matches!(v, JsonValue::String(s) if s == IGNORED_STATE_VALUE)
}

/// A span of one state as the timelines draw it, named by the row that
/// decided it.
pub(crate) struct SwitchRegion {
    pub start_ms: i64,
    pub end_ms: i64,
    pub state: JsonValue,
    pub id: Option<String>,
}

/// The field's switch regions from `from` until `open_end_ms`: its state
/// changes in `[from, until]` as the one oracle reads them
/// ([`FieldStateRows::changes_between`], da-795cy), one region per run of
/// one state, the last left open. Only the geometry is decided here. A
/// block's own span is the block's to draw ([`render_field_visuals`]), so
/// the region it interrupted runs on under it up to its trailing edge, and
/// what holds past that edge (a resumed state continuing the same region)
/// is drawn from there.
pub(crate) fn switch_regions(
    rows: &FieldStateRows,
    (from, until): (DateTime<Utc>, DateTime<Utc>),
    open_end_ms: i64,
) -> Vec<SwitchRegion> {
    let changes = rows.changes_between(from, until);
    let mut out: Vec<SwitchRegion> = Vec::new();
    for (i, c) in changes.iter().enumerate() {
        let mut start_ms = c.at.timestamp_millis();
        if start_ms > open_end_ms {
            break;
        }
        let next_ms = changes.get(i + 1).map(|next| next.at.timestamp_millis());
        let end_ms = next_ms.unwrap_or(open_end_ms);
        if let Some(trailing) = c
            .row
            .occur
            .filter(|t| is_block_verb(c.row.verb) && c.at < *t)
        {
            let trailing = trailing.timestamp_millis();
            if let Some(under) = out.last_mut().filter(|r| r.end_ms == start_ms) {
                under.end_ms = end_ms.min(trailing);
            }
            start_ms = trailing;
        }
        // Empty, except the state left open: switched to at the open end
        // itself, it still holds (a held clock renders at that instant).
        if start_ms > open_end_ms || (start_ms >= end_ms && next_ms.is_some()) {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.end_ms == start_ms && last.state == c.state => last.end_ms = end_ms,
            _ => out.push(SwitchRegion {
                start_ms,
                end_ms,
                state: c.state.clone(),
                id: c.governing.map(|g| g.id.clone()),
            }),
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

/// The instant a row's span BEGINS: `occur − dur`, or `occur` when the
/// row carries no duration. da-zwct: `occur_time` is a block's trailing
/// edge and the duration is the span that precedes it, so the leading
/// edge is the derived one — the same event [`datum_core::state`]
/// compiles at `occur − dur`. A row's trailing edge needs no helper: it
/// is `occur_ms` itself.
pub(crate) fn leading_edge_ms(entry: &Entry, occur_ms: i64) -> i64 {
    occur_ms - dur_seconds(entry).unwrap_or(0) * 1000
}

/// Verb-driven render of one field's blocks and points in the dayview shape.
///
/// Used by `datum dayview` for each calendar day, and by `datum nowview`'s
/// SVG path with the nowview scope `[end - history, end]` as the window
/// (per da-6jy nowview reuses the dayview pipeline).
///
/// Spec mapping (per da-5ns.5):
/// - **Block**: emit a colored block for `[occur − dur, occur]` using
///   `state` as the fill (da-zwct: the duration precedes `occur`).
///   Unless the state is OFF (`false` / TRACKED) — then the span is a
///   HOLE (da-6gvw): nothing is painted there and whatever block
///   encloses it is cut in two, each side keeping its own label and
///   colour. It is the same rule a switch to `false` already obeys.
/// - **Point-switch**: a dot at `occur` coloured by `state`.
/// - **Point**: a dot at `occur` in the field's color.
/// - **Switch**, and what a block-switch or point-switch switches to:
///   `regions` ([`switch_regions`]), drawn first so a block paints over
///   the region it interrupts.
pub(crate) fn render_field_visuals(
    entries: &[Entry],
    regions: &[SwitchRegion],
    field: &str,
    spec: Option<&FieldSpec>,
    day_start_ms: i64,
    day_end_ms: i64,
) -> (Vec<DayviewBlock>, Vec<DayviewPoint>) {
    let mut blocks: Vec<DayviewBlock> = Vec::new();
    for r in regions {
        push_switch_region(
            &mut blocks,
            Some(&r.state),
            r.id.as_deref(),
            r.start_ms,
            r.end_ms,
            field,
            spec,
            day_start_ms,
            day_end_ms,
        );
    }
    let mut points: Vec<DayviewPoint> = Vec::new();
    // Spans the field was explicitly OFF for (da-6gvw). Collected as the
    // walk goes and subtracted from everything it drew, because a hole may
    // sit inside a region the walk has not reached yet.
    let mut holes: Vec<(i64, i64)> = Vec::new();

    for entry in entries {
        let Some(occur_dt) = entry.occur_time.as_ref() else {
            continue;
        };
        let occur_ms = occur_dt.utc.timestamp_millis();

        let verb = canonical_verb(entry);
        // Primary visual dispatch flows from the shape table; verb-specific
        // data sourcing (block fill vs switch region, a dot's colour) is
        // resolved via verb-keyed predicate helpers in `datum_core::shape`.
        // Adding a new verb requires updating the helpers, not this match.
        let shape = render_shape(verb, entry.shape(), &entry.state);
        match shape {
            RenderShape::Hidden | RenderShape::Marker => {}
            RenderShape::Region if is_block_verb(verb) => {
                if is_hole_state(entry.state.as_ref()) {
                    // da-6gvw: `end work 5` stores a Block with `state =
                    // false`. It says the field was OFF for those five
                    // minutes, so it is a hole, not a mark.
                    if let Some(hole) = block_span(entry, occur_ms) {
                        holes.push(hole);
                    }
                } else {
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
            }
            // A switch's region is one of `regions`.
            RenderShape::Region => {}
            RenderShape::Dot => {
                if occur_ms >= day_start_ms && occur_ms <= day_end_ms {
                    // A moment's dot is its state's colour (da-22750); an
                    // occur's, with no state or `true`, the field's.
                    let fg = entry
                        .state
                        .as_ref()
                        .and_then(|v| state_value_rgb(v, field, spec))
                        .unwrap_or_else(|| field_rgb(field, spec));
                    points.push(DayviewPoint {
                        time_ms: occur_ms,
                        fg_color: rgb_to_hex(fg),
                        state_label: mark_state(entry.state.as_ref()),
                        id: Some(entry.hid.to_string()),
                        title: None,
                        glyph: dot_glyph(entry, spec),
                        icon: spec.is_some_and(|s| {
                            (s.presentation)
                                .icon_for(entry.state.as_ref(), entry.data.as_ref())
                                .is_some()
                        }),
                    });
                }
            }
        }
    }
    (
        punch_holes(blocks, &holes, dayview_block_span, set_dayview_block_span),
        points,
    )
}

/// The glyph an occur dot of `entry` draws (da-gzowt), `None` for the
/// circle. One resolver for both timeline walks, the SVG and the terminal.
pub(crate) fn dot_glyph(entry: &Entry, spec: Option<&FieldSpec>) -> Option<String> {
    spec?
        .presentation
        .glyph_for(entry.state.as_ref(), entry.data.as_ref())
        .map(str::to_string)
}

/// True for the states that make a block a hole rather than a mark: the
/// OFF states (`false`, TRACKED). `null`/absent is not one — a block
/// logged without a state still happened, it just has nothing to say
/// about what it was.
pub(crate) fn is_hole_state(value: Option<&JsonValue>) -> bool {
    value.is_some_and(is_off_state)
}

/// A block row's `[occur − dur, occur]` span, or `None` when it carries
/// no duration to span (da-zwct).
pub(crate) fn block_span(entry: &Entry, occur_ms: i64) -> Option<(i64, i64)> {
    match dur_seconds(entry) {
        Some(secs) if secs > 0 => Some((leading_edge_ms(entry, occur_ms), occur_ms)),
        _ => None,
    }
}

/// Subtract the holes from the blocks. A hole inside a block splits it in
/// two, each half keeping the block's own fill and label; a hole at an
/// edge trims it; a hole covering it removes it altogether (da-6gvw).
///
/// Generic over the block shape so the dayview's `DayviewBlock` walk and
/// nowview's terminal `NowviewBlock` walk punch by the one routine —
/// they already share every other decision through this module, and a
/// second copy of this one is how the two surfaces drift apart.
pub(crate) fn punch_holes<T: Clone>(
    blocks: Vec<T>,
    holes: &[(i64, i64)],
    span: impl Fn(&T) -> (i64, i64),
    set_span: impl Fn(&mut T, i64, i64),
) -> Vec<T> {
    if holes.is_empty() {
        return blocks;
    }
    let mut out: Vec<T> = Vec::with_capacity(blocks.len());
    for block in blocks {
        let mut pieces = vec![block];
        for &(hole_start, hole_end) in holes {
            let mut next = Vec::with_capacity(pieces.len() + 1);
            for piece in pieces {
                let (start, end) = span(&piece);
                if hole_end <= start || hole_start >= end {
                    next.push(piece);
                    continue;
                }
                if start < hole_start {
                    let mut left = piece.clone();
                    set_span(&mut left, start, hole_start);
                    next.push(left);
                }
                if hole_end < end {
                    let mut right = piece;
                    set_span(&mut right, hole_end, end);
                    next.push(right);
                }
            }
            pieces = next;
        }
        out.extend(pieces);
    }
    out
}

/// The `span` / `set_span` pair [`punch_holes`] needs for a [`DayviewBlock`].
pub(crate) fn dayview_block_span(b: &DayviewBlock) -> (i64, i64) {
    (b.start_ms, b.end_ms)
}

pub(crate) fn set_dayview_block_span(b: &mut DayviewBlock, start_ms: i64, end_ms: i64) {
    b.start_ms = start_ms;
    b.end_ms = end_ms;
}

/// Emit a block clipped to the day window for a `[occur − dur, occur]`
/// interval (Block / BlockSwitch). `occur_time` is the block's TRAILING
/// edge and the duration is how long it lasted up to that instant — the
/// original datum's `durationBlocks` reading, and the one
/// `datum_core::state` compiles by (da-zwct). No-op if `dur` is missing
/// or the block falls entirely outside the visible window.
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
    let end_ms = occur_ms;
    let start_ms = leading_edge_ms(entry, occur_ms);
    if end_ms <= day_start_ms || start_ms >= day_end_ms {
        return;
    }
    let fill = block_fill_for_value(entry.state.as_ref(), field, spec);
    blocks.push(DayviewBlock {
        start_ms,
        end_ms,
        fill,
        state_label: mark_state(entry.state.as_ref()),
        id: Some(entry.hid.to_string()),
        title: None,
    });
}

/// Emit a switch-region block from `start_ms` to `region_end_ms`, colored by
/// `value`. `id` is the hid of the entry that OPENED the region — a switch
/// region runs FORWARD from its entry, so the entry is at the region's
/// `data-from` end, where a duration block's is at its `data-to` (da-6hg6v).
/// No-op when the value is the "ended" sentinel (null/false/IGNORED)
/// or the region falls outside the day window.
// Internal helper — eight params group three independently varying axes
// (block bounds, day window, field/spec). Folding them into a struct
// here would just rename the locals at the call sites.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_switch_region(
    blocks: &mut Vec<DayviewBlock>,
    value: Option<&JsonValue>,
    id: Option<&str>,
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
        state_label: mark_state(value),
        id: id.map(str::to_string),
        title: None,
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

/// A mark's state as the tail prints it — `start` / `end` for a bool, `∅`
/// for null — so the mark and the row it came from say the same word
/// (da-s5da). [`state_label`] stays the raw value: `stats` keys on it.
fn mark_state(v: Option<&JsonValue>) -> Option<String> {
    v.map(crate::render::state_truncate::state_label)
}

/// Give each mark the `<title>` a hover shows (da-s5da): `field: state`,
/// and for a block its span in the tail's clock — `work: coding
/// 14:00–14:30`. `cut` is where the fetch began and the window stops: a
/// block starting at the first began before it (`mood: happy …–14:30`,
/// the leading region nowview synthesizes), and one ending at the second
/// is still going, as an open state is (`mood: happy 14:00 →`). A dot is
/// named alone; its time is the viewer's to show.
pub(crate) fn title_marks(
    field: &str,
    blocks: &mut [DayviewBlock],
    points: &mut [DayviewPoint],
    cut: (i64, i64),
) {
    let zone = Zone::host();
    let clock = |ms: i64| {
        DateTime::from_timestamp_millis(ms)
            .map(|t| clock_label(&zone.local_naive(t)))
            .unwrap_or_default()
    };
    let name = |state: &Option<String>| match state {
        Some(s) => format!("{field}: {s}"),
        None => field.to_string(),
    };
    for b in blocks {
        let start = if b.start_ms == cut.0 {
            "…".to_string()
        } else {
            clock(b.start_ms)
        };
        let end = if b.end_ms == cut.1 {
            " →".to_string()
        } else {
            format!("–{}", clock(b.end_ms))
        };
        b.title = Some(format!("{} {start}{end}", name(&b.state_label)));
    }
    for p in points {
        p.title = Some(name(&p.state_label));
    }
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

/// Test helper: the regions stub entries draw, from before their first
/// row. Shared with the nowview tests.
#[cfg(test)]
pub(crate) fn stub_regions(entries: &[Entry], open_end_ms: i64) -> Vec<SwitchRegion> {
    let rows = FieldStateRows::from_entries(entries);
    switch_regions(
        &rows,
        (DateTime::UNIX_EPOCH, DateTime::<Utc>::MAX_UTC),
        open_end_ms,
    )
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

    fn spans(regions: &[SwitchRegion]) -> Vec<(i64, i64, String)> {
        regions
            .iter()
            .map(|r| (r.start_ms, r.end_ms, r.state.to_string()))
            .collect()
    }

    /// da-795cy: every switching verb opens a region that the next change
    /// ends — Switch its `state`, BlockSwitch and PointSwitch their
    /// `following_state` from the trailing edge, the block itself being
    /// the block's to draw — and the last runs to the open end.
    #[test]
    fn switch_regions_chain_across_all_switching_verbs() {
        let t = |h: u32| Utc.with_ymd_and_hms(2026, 4, 27, h, 0, 0).unwrap();
        let ms = |h: u32| t(h).timestamp_millis();
        let s = |v: &str| Some(JsonValue::String(v.into()));
        let entries = vec![
            stub_entry("f", Verb::Switch, t(8), None, s("a"), None),
            stub_entry(
                "f",
                Verb::BlockSwitch,
                t(12),
                Some("PT1H"),
                s("via"),
                s("b"),
            ),
            stub_entry(
                "f",
                Verb::PointSwitch,
                t(16),
                Some("PT0S"),
                s("via"),
                s("c"),
            ),
        ];
        let end = ms(23);
        assert_eq!(
            spans(&stub_regions(&entries, end)),
            vec![
                (ms(8), ms(12), "\"a\"".into()),
                (ms(12), ms(16), "\"b\"".into()),
                (ms(16), end, "\"c\"".into()),
            ]
        );
    }

    /// A switch at the open end itself still draws its region, empty as it
    /// is — the field holds that state from now on (the page's held clock
    /// renders at the instant it logged).
    #[test]
    fn a_switch_at_the_open_end_still_draws() {
        let t = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let s = Some(JsonValue::String("happy".into()));
        let entries = vec![stub_entry("mood", Verb::Switch, t, None, s, None)];
        let now = t.timestamp_millis();
        assert_eq!(
            spans(&stub_regions(&entries, now)),
            vec![(now, now, "\"happy\"".into())]
        );
    }

    /// A block interrupts the region under it and hands the field back:
    /// the oracle's state before and after it is one state, so it is one
    /// region, running on under the block (which paints over it). A
    /// block-switch with no `following_state` keeps its own state past
    /// its trailing edge, and a region begins there.
    #[test]
    fn a_block_continues_the_region_it_interrupts() {
        let t = |h: u32| Utc.with_ymd_and_hms(2026, 4, 27, h, 0, 0).unwrap();
        let ms = |h: u32| t(h).timestamp_millis();
        let s = |v: &str| Some(JsonValue::String(v.into()));
        let entries = vec![
            stub_entry("f", Verb::Switch, t(8), None, s("a"), None),
            stub_entry("f", Verb::Block, t(10), Some("PT1H"), s("x"), None),
            stub_entry("f", Verb::BlockSwitch, t(14), Some("PT1H"), s("y"), None),
        ];
        let end = ms(23);
        let got = stub_regions(&entries, end);
        assert_eq!(
            spans(&got),
            vec![
                (ms(8), ms(14), "\"a\"".into()),
                (ms(14), end, "\"y\"".into())
            ]
        );
        assert_eq!(got[0].id, Some(entries[0].hid.to_string()));
        assert_eq!(got[1].id, Some(entries[2].hid.to_string()));
    }

    // ─── da-5ns.5: verb-driven render tests ────────────────────────────
    //
    // These construct Entry structs directly and call the pure render
    // function — no database is involved.

    /// Block at [occur − dur, occur] — da-zwct: `occur_time` is the
    /// TRAILING edge and the duration is how long it lasted up to it.
    #[test]
    fn block_renders_the_duration_before_its_occur_time() {
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
        let regions = stub_regions(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, &regions, "lunch", None, day_start, day_end);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].start_ms,
            occur.timestamp_millis() - 30 * 60 * 1000
        );
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].state_label.as_deref(), Some("eating"));
    }

    /// Block whose `[occur − dur, occur]` interval crosses the day
    /// boundary. The renderer emits the block; the layout clips it to the
    /// visible window (verified separately in
    /// `dayview.rs::tests::blocks_clip_to_day_window`).
    #[test]
    fn block_crossing_day_boundary_emits_full_interval() {
        // 2-hour block ENDING at 01:00 on day-2 → it began at 23:00 on
        // day-1 and reaches back over the boundary (da-zwct).
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
        let regions = stub_regions(&entries, day_end);
        let (blocks, _points) =
            render_field_visuals(&entries, &regions, "sleep", None, day_start, day_end);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].start_ms,
            occur.timestamp_millis() - 2 * 60 * 60 * 1000
        );
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
        // Block start sits in the prior day — confirms the renderer doesn't
        // self-clip; the layout owns clipping.
        assert!(blocks[0].start_ms < day_start);
    }

    /// Block-switch emits the block AND opens a new switch region at the
    /// block's trailing edge, colored by `following_state`. With visibly
    /// distinct colors, the two emitted regions must hold distinct fills.
    #[test]
    fn block_switch_emits_block_plus_following_region_with_distinct_colors() {
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
        let regions = stub_regions(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, &regions, "mood", None, day_start, day_end);
        assert!(points.is_empty());
        // The region is drawn first, so the block paints over it.
        assert_eq!(blocks.len(), 2);
        // Block: [occur − 1h, occur] colored "groggy".
        assert_eq!(
            blocks[1].start_ms,
            occur.timestamp_millis() - 60 * 60 * 1000
        );
        assert_eq!(blocks[1].end_ms, occur.timestamp_millis());
        assert_eq!(blocks[1].state_label.as_deref(), Some("groggy"));
        // Following region opens where the block ends, not where it starts
        // — da-zwct: that is `occur` itself. [occur, day_end] "focused".
        assert_eq!(blocks[0].start_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].end_ms, day_end);
        assert_eq!(blocks[0].state_label.as_deref(), Some("focused"));
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
        let regions = stub_regions(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, &regions, "task", None, day_start, day_end);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].time_ms, occur.timestamp_millis());
        assert_eq!(points[0].state_label.as_deref(), Some("commit"));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].end_ms, day_end);
        assert_eq!(blocks[0].state_label.as_deref(), Some("review"));
    }

    /// A block-switch followed later by a switch ends the following-state
    /// region at the switch's occur_time, not at day_end.
    #[test]
    fn block_switch_following_region_ends_at_next_switch() {
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
        let regions = stub_regions(&entries, day_end);
        let (blocks, _points) =
            render_field_visuals(&entries, &regions, "mood", None, day_start, day_end);
        // 3 blocks expected: the "groggy" block 8:30→9:00, the "focused"
        // region from its trailing edge 9:00→14:00, and the "tired" switch
        // region from 14:00→day_end.
        assert_eq!(blocks.len(), 3);
        // Find the focused region.
        let focused = blocks
            .iter()
            .find(|b| b.state_label.as_deref() == Some("focused"))
            .expect("focused region present");
        assert_eq!(focused.start_ms, bs_occur.timestamp_millis());
        assert_eq!(focused.end_ms, switch_occur.timestamp_millis());
    }

    /// A block-switch whose trailing edge lands at or past the end of the
    /// window leaves its `following_state` band no room; the band is
    /// dropped rather than emitted backwards.
    ///
    /// da-zwct moved the reachable case: the band now opens at the row's
    /// own `occur_time`, so a later switching row can never squeeze it
    /// (rows arrive in `occur` order). What still can is a row logged
    /// past the window's own end — a nowview strip stops at `now`, and
    /// `-t +30` puts a row beyond it.
    #[test]
    fn block_switch_outlasting_the_window_emits_no_following_band() {
        // Block-switch at 23:00 — two hours of "groggy" reaching back to
        // 21:00 — in a strip that stops at 22:00.
        let bs_occur = Utc.with_ymd_and_hms(2026, 4, 27, 23, 0, 0).unwrap();
        let entries = vec![stub_entry(
            "mood",
            Verb::BlockSwitch,
            bs_occur,
            Some("PT2H"),
            Some(JsonValue::String("groggy".into())),
            Some(JsonValue::String("focused".into())),
        )];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let window_end = Utc
            .with_ymd_and_hms(2026, 4, 27, 22, 0, 0)
            .unwrap()
            .timestamp_millis();
        let regions = stub_regions(&entries, window_end);
        let (blocks, _points) =
            render_field_visuals(&entries, &regions, "mood", None, day_start, window_end);
        // The "groggy" block alone — no "focused" band at all.
        assert_eq!(blocks.len(), 1);
        assert!(
            !blocks
                .iter()
                .any(|b| b.state_label.as_deref() == Some("focused")),
            "a band with no room must not be drawn"
        );
        assert!(
            blocks.iter().all(|b| b.end_ms > b.start_ms),
            "no block may run backwards"
        );
    }

    // ─── da-6gvw: a false block is a hole ─────────────────────────────

    /// The three rows `start work -t 09:00`, `end work 5 -t 10:00`,
    /// `end work -t 12:00` as the CLI stores them.
    fn work_with_a_false_block(hole_occur: chrono::DateTime<Utc>) -> Vec<Entry> {
        vec![
            stub_entry(
                "work",
                Verb::Switch,
                Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap(),
                None,
                Some(JsonValue::Bool(true)),
                None,
            ),
            stub_entry(
                "work",
                Verb::Block,
                hole_occur,
                Some("PT5M"),
                Some(JsonValue::Bool(false)),
                None,
            ),
            stub_entry(
                "work",
                Verb::Switch,
                Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap(),
                None,
                Some(JsonValue::Bool(false)),
                None,
            ),
        ]
    }

    fn render_work(entries: &[Entry]) -> Vec<DayviewBlock> {
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let regions = stub_regions(entries, day_end);
        let (blocks, points) =
            render_field_visuals(entries, &regions, "work", None, day_start, day_end);
        assert!(points.is_empty());
        blocks
    }

    fn ms(hour: u32, minute: u32) -> i64 {
        Utc.with_ymd_and_hms(2026, 4, 27, hour, minute, 0)
            .unwrap()
            .timestamp_millis()
    }

    /// `end work 5 -t 10:00` inside a running `work` cuts the enclosing
    /// region in two rather than painting a mark of its own.
    #[test]
    fn false_block_is_a_hole_that_cuts_the_enclosing_region() {
        let blocks = render_work(&work_with_a_false_block(
            Utc.with_ymd_and_hms(2026, 4, 27, 10, 0, 0).unwrap(),
        ));
        assert_eq!(blocks.len(), 2, "the region is split, not painted over");
        assert_eq!(
            (blocks[0].start_ms, blocks[0].end_ms),
            (ms(9, 0), ms(9, 55))
        );
        assert_eq!(
            (blocks[1].start_ms, blocks[1].end_ms),
            (ms(10, 0), ms(12, 0))
        );
        // Both halves keep the region's own label and colour.
        for b in &blocks {
            assert_eq!(b.state_label.as_deref(), Some("start"));
        }
        match (&blocks[0].fill, &blocks[1].fill) {
            (BlockFill::Single(a), BlockFill::Single(b)) => assert_eq!(a, b),
            _ => panic!("expected single-fill halves"),
        }
        // Nothing carries the hole's own word.
        assert!(
            !blocks
                .iter()
                .any(|b| b.state_label.as_deref() == Some("end")),
            "a hole is not a mark"
        );
    }

    /// A hole at a region's leading edge trims it instead of splitting it.
    #[test]
    fn false_block_at_a_regions_edge_trims_it() {
        let blocks = render_work(&work_with_a_false_block(
            Utc.with_ymd_and_hms(2026, 4, 27, 9, 5, 0).unwrap(),
        ));
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            (blocks[0].start_ms, blocks[0].end_ms),
            (ms(9, 5), ms(12, 0))
        );
    }

    /// A hole covering the whole region leaves nothing behind.
    #[test]
    fn false_block_covering_the_region_leaves_nothing() {
        let entries = vec![
            stub_entry(
                "work",
                Verb::Switch,
                Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap(),
                None,
                Some(JsonValue::Bool(true)),
                None,
            ),
            stub_entry(
                "work",
                Verb::Block,
                Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap(),
                Some("PT4H"),
                Some(JsonValue::Bool(false)),
                None,
            ),
            stub_entry(
                "work",
                Verb::Switch,
                Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap(),
                None,
                Some(JsonValue::Bool(false)),
                None,
            ),
        ];
        assert!(render_work(&entries).is_empty());
    }

    /// The TRACKED sentinel reads as off everywhere else; a block wearing
    /// it is a hole too.
    #[test]
    fn tracked_block_is_a_hole_as_false_is() {
        let entries = vec![stub_entry(
            "work",
            Verb::Block,
            Utc.with_ymd_and_hms(2026, 4, 27, 10, 0, 0).unwrap(),
            Some("PT5M"),
            Some(JsonValue::String(
                datum_core::shape::TRACKED_STATE_VALUE.into(),
            )),
            None,
        )];
        assert!(render_work(&entries).is_empty());
    }

    /// Block-switch where `following_state` is the "ended" sentinel (false /
    /// null / IGNORED) emits the leading block but no following region.
    #[test]
    fn block_switch_with_unrendered_following_state_emits_only_leading_block() {
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
        let regions = stub_regions(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, &regions, "mood", None, day_start, day_end);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].state_label.as_deref(), Some("hungry"));
    }
}
