//! Which marks an entry makes on a timeline.
//!
//! `field_visuals::render_field_visuals` walks a field's entries and emits
//! blocks and points; this is the same walk with the colours left out,
//! because the browser's caller does not have a `Db` and does not need
//! one. The rule it dispatches on is not re-stated here: `render_shape`
//! and the verb predicates are `datum_parse::shape`, the CLI's own table,
//! reached from wasm because da-t7ud moved it out of `datum-core` (which
//! pulls rusqlite and cannot compile for `wasm32-unknown-unknown`).
//!
//! Before that move the browser held a hand-written copy of the table in
//! `web/src/lib/visuals.ts`, updated in lockstep. The sibling copy that
//! tracked `derive_verb` the same way — `web/src/lib/verb.ts` — drifted
//! three separate times in two days before da-7scm deleted it, so this
//! module exists to make the same class of drift impossible here rather
//! than merely tested for. `crates/datum-commands/src/render/wasm_parity.rs`
//! holds the walk itself against `render_field_visuals`.

use serde::Deserialize;
use serde_json::Value as JsonValue;

use datum_parse::dur::Dur;
use datum_parse::shape::{
    dot_bg_uses_following_state, dot_fg_uses_state, is_block_verb, is_switching_verb,
    is_unrendered_switch_value, opens_secondary_switch_region, render_shape, EntryShape,
    RenderShape,
};
use datum_parse::verb::Verb;

/// One stored entry, in as much of it as a timeline mark reads.
///
/// The columns are the `entries` row's own (`crates/datum-core/src/schema.sql`),
/// with `occur_time` pre-resolved to UTC milliseconds because the caller
/// already holds it that way and a millisecond is not a rule.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EntryReq {
    /// `occur_time` in UTC milliseconds. A row without one draws nothing.
    #[serde(default)]
    pub occur_ms: Option<i64>,
    #[serde(default)]
    pub verb: Verb,
    /// The `dur` column verbatim — `"PT30M"`, `"MOMENT"`, `"UNKNOWN"`.
    /// Read through [`Dur::parse`], so the browser interprets nothing.
    #[serde(default)]
    pub dur: Option<String>,
    #[serde(default)]
    pub state: Option<JsonValue>,
    #[serde(default)]
    pub following_state: Option<JsonValue>,
    /// The slug, which breaks a tie between two entries at one instant the
    /// way `STATE_ORDER_NEWEST_FIRST` breaks it.
    #[serde(default)]
    pub slug: String,
}

impl EntryReq {
    fn parsed_dur(&self) -> Option<Dur> {
        self.dur.as_deref().and_then(Dur::parse)
    }

    /// Seconds of span, clamped at zero — `field_visuals::dur_seconds`.
    fn dur_seconds(&self) -> Option<i64> {
        self.parsed_dur()?.as_seconds().map(|s| s.max(0))
    }

    fn shape(&self) -> EntryShape {
        EntryShape::from_dur(self.parsed_dur().as_ref())
    }
}

/// Which colour to use when a value resolves to nothing. The two dot
/// fallbacks in `field_visuals::render_field_visuals` differ, and only the
/// walk — which knows the verb — can pick between them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Fallback {
    /// The field's own colour (a dot whose fg is the field, or whose bg
    /// comes from `following_state`).
    #[default]
    Field,
    /// Black — a dot with no prevailing state underneath it.
    Black,
}

/// A filled span. `state` picks the fill; absent → the field colour.
#[derive(Debug, Clone)]
pub struct BlockMark {
    pub start_ms: i64,
    pub end_ms: i64,
    pub state: Option<JsonValue>,
}

/// A dot at an instant, with a foreground and a background.
#[derive(Debug, Clone)]
pub struct PointMark {
    pub time_ms: i64,
    pub fg: Option<JsonValue>,
    pub fg_fallback: Fallback,
    pub bg: Option<JsonValue>,
    pub bg_fallback: Fallback,
    pub state_label: Option<JsonValue>,
}

/// What one field draws over a window.
#[derive(Debug, Clone, Default)]
pub struct FieldMarks {
    pub blocks: Vec<BlockMark>,
    pub points: Vec<PointMark>,
}

impl FieldMarks {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.points.is_empty()
    }
}

/// A band standing in for a state that was set before the fetch window
/// (da-ncoa) — `nowview::leading_state_region`, whose two inputs (the
/// prevailing state at the window's left edge, and the entry that holds
/// it) only the caller's database can answer.
#[derive(Debug, Clone)]
pub struct LeadingRegion {
    pub start_ms: i64,
    pub state: Option<JsonValue>,
}

/// When each switching entry's region ends: at the next switching entry,
/// or at `fallback_end_ms` for the last one. `entries` must be sorted by
/// occur_ms ascending. Port of `field_visuals::switch_region_end_ms`.
fn switch_region_ends(entries: &[EntryReq], fallback_end_ms: i64) -> Vec<i64> {
    let mut out = vec![fallback_end_ms; entries.len()];
    let mut next_occur_ms: Option<i64> = None;
    for (i, entry) in entries.iter().enumerate().rev() {
        if is_switching_verb(entry.verb) {
            out[i] = next_occur_ms.unwrap_or(fallback_end_ms);
            if let Some(ms) = entry.occur_ms {
                next_occur_ms = Some(ms);
            }
        }
    }
    out
}

/// The state one entry leaves in force at `at_ms`. Port of
/// `db::now::effective_state`: past the trailing edge of a compound
/// switching verb it is `following_state` that holds, not `state`.
pub fn effective_state(entry: &EntryReq, at_ms: i64) -> JsonValue {
    if opens_secondary_switch_region(entry.verb) {
        if let (Some(occur_ms), Some(dur), Some(fs)) = (
            entry.occur_ms,
            entry.parsed_dur(),
            entry.following_state.as_ref(),
        ) {
            let trailing = occur_ms + dur.as_seconds().unwrap_or(0) * 1000;
            if at_ms >= trailing {
                return fs.clone();
            }
        }
    }
    entry.state.clone().unwrap_or(JsonValue::Null)
}

/// The field's prevailing state at `at_ms`, or `None` when nothing in
/// `entries` has set one. The in-memory form of `Db::state_at`: newest
/// entry at or before the instant, ties broken by slug the way
/// `STATE_ORDER_NEWEST_FIRST` breaks them, then [`effective_state`].
///
/// Bounded by what the caller fetched, where the CLI's query is bounded
/// by the whole store: a dot whose prevailing state was set before the
/// fetch window reads as "none" here and paints black. The panel and the
/// nowview leading region, where that difference is a band rather than a
/// shade of one dot's background, are answered from the database instead
/// and arrive as [`LeadingRegion`] / the panel entries.
fn state_at(entries: &[EntryReq], at_ms: i64) -> Option<JsonValue> {
    let mut winner: Option<&EntryReq> = None;
    for entry in entries {
        let Some(ms) = entry.occur_ms else { continue };
        if ms > at_ms {
            continue;
        }
        let better = match winner {
            None => true,
            Some(best) => {
                let best_ms = best.occur_ms.unwrap_or(i64::MIN);
                ms > best_ms || (ms == best_ms && entry.slug > best.slug)
            }
        };
        if better {
            winner = Some(entry);
        }
    }
    winner.map(|e| effective_state(e, at_ms))
}

/// The first switching entry in `entries`, whose region ends the lead-in.
/// `nowview::leading_state_region`'s `lead_end_ms`.
fn first_switch_ms(entries: &[EntryReq]) -> Option<i64> {
    entries
        .iter()
        .filter(|e| is_switching_verb(e.verb))
        .find_map(|e| e.occur_ms)
}

/// A band, clipped away when it falls entirely outside the window.
/// `field_visuals::push_switch_region` minus the fill.
fn push_region(
    blocks: &mut Vec<BlockMark>,
    state: Option<&JsonValue>,
    start_ms: i64,
    end_ms: i64,
    window_start_ms: i64,
    window_end_ms: i64,
) {
    if is_unrendered_switch_value(state) {
        return;
    }
    if end_ms <= window_start_ms || start_ms >= window_end_ms {
        return;
    }
    blocks.push(BlockMark {
        start_ms,
        end_ms,
        state: state.cloned(),
    });
}

/// One field's marks over `[window_start_ms, window_end_ms]`.
///
/// `entries` are that field's own, sorted by occur_ms ascending, and may
/// reach back before the window — an entry that started earlier still
/// draws the part of its band that lands inside it. `fallback_end_ms` is
/// where a still-open switch region stops: the end of the day clamped to
/// now for dayview, the now-line for nowview.
pub fn field_marks(
    entries: &[EntryReq],
    window_start_ms: i64,
    window_end_ms: i64,
    fallback_end_ms: i64,
    lead: Option<&LeadingRegion>,
) -> FieldMarks {
    let mut marks = FieldMarks::default();
    let ends = switch_region_ends(entries, fallback_end_ms);

    // da-ncoa: a state set before the fetch window has no in-window entry
    // to open its region, so the caller hands one in. First, so it lies
    // under whatever the in-window entries draw over it.
    if let Some(lead) = lead {
        let lead_end_ms = first_switch_ms(entries).unwrap_or(window_end_ms);
        push_region(
            &mut marks.blocks,
            lead.state.as_ref(),
            lead.start_ms,
            lead_end_ms,
            window_start_ms,
            window_end_ms,
        );
    }

    for (idx, entry) in entries.iter().enumerate() {
        let Some(at) = entry.occur_ms else { continue };
        let verb = entry.verb;

        match render_shape(verb, entry.shape(), &entry.state) {
            RenderShape::Hidden | RenderShape::Marker => {}
            RenderShape::Region if is_block_verb(verb) => {
                // A block is `[occur - dur, occur]`: occur_time is its
                // trailing edge, not its start.
                if let Some(secs) = entry.dur_seconds().filter(|s| *s > 0) {
                    let start_ms = at - secs * 1000;
                    if at > window_start_ms && start_ms < window_end_ms {
                        marks.blocks.push(BlockMark {
                            start_ms,
                            end_ms: at,
                            state: entry.state.clone(),
                        });
                    }
                }
            }
            RenderShape::Region => push_region(
                &mut marks.blocks,
                entry.state.as_ref(),
                at,
                ends[idx],
                window_start_ms,
                window_end_ms,
            ),
            RenderShape::Dot => {
                if at >= window_start_ms && at <= window_end_ms {
                    // A point-switch carries both halves itself — the state
                    // it passes through, over the state it leaves behind.
                    // A plain dot is the field's own colour over whatever
                    // state was prevailing underneath it, black when none
                    // was. Which is which is `shape.rs`'s call, not this
                    // walk's.
                    let fg = if dot_fg_uses_state(verb) {
                        entry.state.clone()
                    } else {
                        None
                    };
                    let (bg, bg_fallback) = if dot_bg_uses_following_state(verb) {
                        (entry.following_state.clone(), Fallback::Field)
                    } else {
                        (state_at(entries, at), Fallback::Black)
                    };
                    marks.points.push(PointMark {
                        time_ms: at,
                        fg,
                        fg_fallback: Fallback::Field,
                        bg,
                        bg_fallback,
                        state_label: entry.state.clone(),
                    });
                }
            }
        }

        // The trailing region a compound switching verb opens: a band from
        // occur_time coloured by `following_state`.
        if opens_secondary_switch_region(verb) {
            push_region(
                &mut marks.blocks,
                entry.following_state.as_ref(),
                at,
                ends[idx],
                window_start_ms,
                window_end_ms,
            );
        }
    }

    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DAY: i64 = 86_400_000;

    fn entry(verb: Verb, occur_ms: i64, state: Option<JsonValue>) -> EntryReq {
        EntryReq {
            occur_ms: Some(occur_ms),
            verb,
            state,
            slug: format!("f:{occur_ms}"),
            ..EntryReq::default()
        }
    }

    #[test]
    fn a_switch_opens_a_band_that_runs_to_the_next_switch() {
        let entries = [
            entry(Verb::Switch, 1_000, Some(json!("happy"))),
            entry(Verb::Switch, 5_000, Some(json!("sad"))),
        ];
        let marks = field_marks(&entries, 0, DAY, DAY, None);
        assert!(marks.points.is_empty());
        assert_eq!(marks.blocks.len(), 2);
        assert_eq!(
            (marks.blocks[0].start_ms, marks.blocks[0].end_ms),
            (1_000, 5_000)
        );
        assert_eq!(
            (marks.blocks[1].start_ms, marks.blocks[1].end_ms),
            (5_000, DAY)
        );
    }

    #[test]
    fn a_switch_to_nothing_draws_nothing_and_ends_the_band_before_it() {
        for terminator in [
            None,
            Some(JsonValue::Null),
            Some(json!(false)),
            Some(json!("IGNORED")),
        ] {
            let entries = [
                entry(Verb::Switch, 1_000, Some(json!("happy"))),
                entry(Verb::Switch, 5_000, terminator.clone()),
            ];
            let marks = field_marks(&entries, 0, DAY, DAY, None);
            assert_eq!(
                marks.blocks.len(),
                1,
                "terminator {terminator:?} drew a band"
            );
            assert_eq!(marks.blocks[0].end_ms, 5_000);
        }
    }

    #[test]
    fn a_plain_point_is_a_dot_in_the_fields_own_colour() {
        let marks = field_marks(&[entry(Verb::Point, 3_000, None)], 0, DAY, DAY, None);
        assert!(marks.blocks.is_empty());
        assert_eq!(marks.points.len(), 1);
        assert_eq!(marks.points[0].fg, None);
        assert_eq!(marks.points[0].fg_fallback, Fallback::Field);
        // Nothing prevailing underneath: the newest row at the instant is
        // the point itself, whose state is null, so the dot paints black.
        assert_eq!(marks.points[0].bg, Some(JsonValue::Null));
        assert_eq!(marks.points[0].bg_fallback, Fallback::Black);
    }

    #[test]
    fn a_dot_takes_its_background_from_the_row_state_at_would_pick() {
        // Two rows at one instant: `STATE_ORDER_NEWEST_FIRST` breaks the
        // tie on slug, so the switch — not the point beside it — prevails.
        let mut switching = entry(Verb::Switch, 3_000, Some(json!("happy")));
        switching.slug = "mood:b".into();
        let mut point = entry(Verb::Point, 3_000, None);
        point.slug = "mood:a".into();
        let marks = field_marks(&[switching, point], 0, DAY, DAY, None);
        assert_eq!(marks.points.len(), 1);
        assert_eq!(marks.points[0].bg, Some(json!("happy")));
    }

    #[test]
    fn a_block_hangs_back_from_its_occur_time() {
        let mut e = entry(Verb::Block, 3_600_000, Some(json!("work")));
        e.dur = Some("PT30M".into());
        let marks = field_marks(&[e], 0, DAY, DAY, None);
        assert!(marks.points.is_empty());
        assert_eq!(marks.blocks.len(), 1);
        assert_eq!(
            (marks.blocks[0].start_ms, marks.blocks[0].end_ms),
            (3_600_000 - 1_800_000, 3_600_000)
        );
    }

    #[test]
    fn a_point_switch_draws_a_dot_and_the_band_it_opens() {
        let mut e = entry(Verb::PointSwitch, 1_000, Some(json!("bump")));
        e.dur = Some("PT0S".into());
        e.following_state = Some(json!("after"));
        let marks = field_marks(&[e], 0, DAY, DAY, None);
        assert_eq!(marks.points.len(), 1);
        assert_eq!(marks.points[0].fg, Some(json!("bump")));
        assert_eq!(marks.points[0].bg, Some(json!("after")));
        assert_eq!(marks.blocks.len(), 1);
        assert_eq!(marks.blocks[0].state, Some(json!("after")));
    }

    #[test]
    fn a_lead_region_stops_at_the_first_switch() {
        let entries = [entry(Verb::Switch, 4_000, Some(json!("happy")))];
        let lead = LeadingRegion {
            start_ms: 0,
            state: Some(json!("earlier")),
        };
        let marks = field_marks(&entries, 0, DAY, DAY, Some(&lead));
        assert_eq!(marks.blocks.len(), 2);
        assert_eq!(marks.blocks[0].state, Some(json!("earlier")));
        assert_eq!(
            (marks.blocks[0].start_ms, marks.blocks[0].end_ms),
            (0, 4_000)
        );
    }

    #[test]
    fn effective_state_flips_at_a_block_switchs_trailing_edge() {
        let mut e = entry(Verb::BlockSwitch, 0, Some(json!("during")));
        e.dur = Some("PT1M".into());
        e.following_state = Some(json!("after"));
        assert_eq!(effective_state(&e, 59_999), json!("during"));
        assert_eq!(effective_state(&e, 60_000), json!("after"));
    }
}
