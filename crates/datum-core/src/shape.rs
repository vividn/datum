//! Verb-shape decision table — the single source of truth for "given an
//! entry's verb, stored shape, and value, what visual category does it
//! render as?"
//!
//! Pre-da-z72b the decision was implicit and duplicated across:
//!   - `commands/dayview.rs` (`is_switching_verb`, `is_unrendered_switch_value`,
//!     and the big `match canonical_verb(entry)` dispatch)
//!   - `commands/nowview.rs` (same helpers, same dispatch)
//!   - `render/action.rs` (`format_state_group`, `render_state_cell`,
//!     `entry_self_trail_state` — all match on verb)
//!   - the SVG path consumes the dayview output, so its decision is
//!     transitively the dayview decision
//!
//! Drift produced the original switch-as-dot bug (da-7r4): each renderer
//! made its own choice about "what shape is `Verb::Switch + EntryShape::Point`"
//! and they didn't agree.
//!
//! This module hoists the decision into a single tabular rule so new verbs
//! add one match arm and new shapes add one variant. Renderers consume
//! [`render_shape`] (and the helpers [`is_switching_verb`] /
//! [`is_unrendered_switch_value`]) instead of re-implementing the tree.

use serde_json::Value as JsonValue;

use crate::db::IGNORED_STATE_VALUE;
use crate::entry::{EntryShape, Verb};

/// Visual category an entry's *primary* element renders as.
///
/// Compound verbs ([`Verb::BlockSwitch`] / [`Verb::PointSwitch`]) produce a
/// primary visual from this categorization plus a secondary switch region
/// driven by their `following_state`; renderers detect the secondary via
/// [`is_switching_verb`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderShape {
    /// Single-point marker rendered at occur_time. The primary visual for
    /// [`Verb::Point`] (with a time) and [`Verb::PointSwitch`] (the dot;
    /// the trailing region is secondary).
    Dot,
    /// Colored region. Either a duration block from `[occur - dur, occur]`
    /// ([`Verb::Block`], [`Verb::BlockSwitch`]'s primary block) or a switch
    /// region from `[occur, next-switching-event]` ([`Verb::Switch`]).
    /// Renderers determine the bounds from the verb.
    Region,
    /// Non-graphical fallback glyph — used by the action-line picker for
    /// entries that can't be placed on a timeline (e.g., legacy timeless
    /// `add` rows that still hit [`Verb::Point`] via [`EntryShape::Ongoing`]).
    /// Currently unused by the timeline renderers.
    Marker,
    /// No visual output. [`Verb::Switch`] with a null/false/IGNORED value
    /// terminates a state without opening a new region.
    Hidden,
}

/// Map (verb, entry_shape, value) → primary [`RenderShape`].
///
/// `value` is the entry's `state` (the `entry.value` compat shim aliases
/// this). For `Verb::Switch` this is what determines whether a region
/// opens; for other verbs it's informational only.
///
/// Compound verbs return their *primary* shape:
///   - [`Verb::BlockSwitch`] → [`RenderShape::Region`] (the block)
///   - [`Verb::PointSwitch`] → [`RenderShape::Dot`] (the marker)
///
/// Their secondary switch region is rendered by callers that detect it
/// via [`is_switching_verb`] and read `following_state` directly.
pub fn render_shape(verb: Verb, entry_shape: EntryShape, value: &Option<JsonValue>) -> RenderShape {
    match (verb, entry_shape) {
        // Pure switch: hidden if the value terminates state, region otherwise.
        (Verb::Switch, _) if is_unrendered_switch_value(value.as_ref()) => RenderShape::Hidden,
        (Verb::Switch, _) => RenderShape::Region,

        // Block-style verbs always render a primary block (the block fill
        // falls back to the field color if `state` is null/false/IGNORED;
        // it is never hidden).
        (Verb::Block | Verb::BlockSwitch, _) => RenderShape::Region,

        // Point-style switching verb: primary is a dot at occur.
        (Verb::PointSwitch, _) => RenderShape::Dot,

        // Plain point: dot if it has timeline placement, marker otherwise.
        (Verb::Point, EntryShape::Point) => RenderShape::Dot,
        (Verb::Point, EntryShape::Block) => RenderShape::Region,
        (Verb::Point, EntryShape::Ongoing) => RenderShape::Marker,
    }
}

// The six predicate helpers below are deliberately written as exhaustive
// `match`es rather than `matches!(...)` macros so the compiler forces a
// decision in *every* helper whenever a new `Verb` is introduced. Together
// with the exhaustive `render_shape` table above, this is the "single
// source of truth" for verb → visual dispatch: adding `Verb::Uncertain`
// would trigger compile errors here and only here — every renderer
// consumes these predicates and never matches on `Verb` directly.

/// True for verbs whose `occur_time` updates the field's prevailing state.
/// These are the verbs that may open a switch region.
pub fn is_switching_verb(verb: Verb) -> bool {
    match verb {
        Verb::Switch | Verb::BlockSwitch | Verb::PointSwitch => true,
        Verb::Block | Verb::Point => false,
    }
}

/// True for verbs whose primary visual is a dur-derived colored block over
/// `[occur - dur, occur]`, with `entry.state` as the fill. [`Verb::Block`]
/// and [`Verb::BlockSwitch`] fall here; [`Verb::BlockSwitch`]'s trailing
/// switch region is secondary and detected separately via
/// [`opens_secondary_switch_region`].
pub fn is_block_verb(verb: Verb) -> bool {
    match verb {
        Verb::Block | Verb::BlockSwitch => true,
        Verb::Switch | Verb::Point | Verb::PointSwitch => false,
    }
}

/// True for verbs whose *primary* visual is a switch region (a
/// state-prevailing band from `occur_time` to the next switching event,
/// colored by `entry.state`). Only [`Verb::Switch`] qualifies — the
/// compound switching verbs render their switch region as the *secondary*
/// element keyed off `following_state`.
pub fn is_primary_switch_region_verb(verb: Verb) -> bool {
    match verb {
        Verb::Switch => true,
        Verb::Block | Verb::BlockSwitch | Verb::Point | Verb::PointSwitch => false,
    }
}

/// True for verbs that open a *secondary* switch region from
/// `following_state` at `occur_time`. These are the compound switching
/// verbs ([`Verb::BlockSwitch`], [`Verb::PointSwitch`]) — their primary
/// visual (block or dot) is paired with a trailing region colored by
/// `entry.following_state`.
pub fn opens_secondary_switch_region(verb: Verb) -> bool {
    match verb {
        Verb::BlockSwitch | Verb::PointSwitch => true,
        Verb::Switch | Verb::Block | Verb::Point => false,
    }
}

/// True for verbs whose primary-dot bg color is sourced from
/// `entry.following_state` (the post-occur state). [`Verb::PointSwitch`]
/// uses this; plain [`Verb::Point`] falls back to ambient state at
/// occur_time via the field's prior-row lookup.
pub fn dot_bg_uses_following_state(verb: Verb) -> bool {
    match verb {
        Verb::PointSwitch => true,
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::Point => false,
    }
}

/// True for verbs whose primary-dot fg color is sourced from
/// `entry.state` (the dot itself carries the state color). Only
/// [`Verb::PointSwitch`] qualifies — plain [`Verb::Point`] paints its
/// dot in the field color so a stateless occur reads as the field.
pub fn dot_fg_uses_state(verb: Verb) -> bool {
    match verb {
        Verb::PointSwitch => true,
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::Point => false,
    }
}

/// True for verbs whose action-line rendering shows a state badge
/// (`[LEAD][state cell][TRAIL]`). All state-bearing verbs qualify
/// (switching family + [`Verb::Block`]); [`Verb::Point`] is the lone
/// exception — its action surface is a bare dot at `occur_time`.
pub fn has_state_badge(verb: Verb) -> bool {
    match verb {
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::PointSwitch => true,
        Verb::Point => false,
    }
}

/// True if a switch value terminates a state without opening a new region.
///
/// Mirrors original datum's `simplifyState === null || simplifyState ===
/// false` short-circuit in `fieldSvgBlocks.ts`: a switch *to* null /
/// false / IGNORED ends the prior state but does not start a new colored
/// region.
pub fn is_unrendered_switch_value(v: Option<&JsonValue>) -> bool {
    match v {
        None | Some(JsonValue::Null) => true,
        Some(JsonValue::Bool(false)) => true,
        Some(JsonValue::String(s)) if s == IGNORED_STATE_VALUE => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn s(v: JsonValue) -> Option<JsonValue> {
        Some(v)
    }

    // ─── render_shape: Switch ────────────────────────────────────────

    #[test]
    fn switch_with_state_renders_as_region() {
        assert_eq!(
            render_shape(Verb::Switch, EntryShape::Point, &s(json!("happy"))),
            RenderShape::Region,
        );
    }

    #[test]
    fn switch_to_null_is_hidden() {
        assert_eq!(
            render_shape(Verb::Switch, EntryShape::Point, &None),
            RenderShape::Hidden,
        );
        assert_eq!(
            render_shape(Verb::Switch, EntryShape::Point, &s(JsonValue::Null)),
            RenderShape::Hidden,
        );
    }

    #[test]
    fn switch_to_false_is_hidden() {
        assert_eq!(
            render_shape(Verb::Switch, EntryShape::Point, &s(json!(false))),
            RenderShape::Hidden,
        );
    }

    #[test]
    fn switch_to_ignored_sentinel_is_hidden() {
        assert_eq!(
            render_shape(
                Verb::Switch,
                EntryShape::Point,
                &s(JsonValue::String(IGNORED_STATE_VALUE.into())),
            ),
            RenderShape::Hidden,
        );
    }

    #[test]
    fn switch_to_true_is_region() {
        assert_eq!(
            render_shape(Verb::Switch, EntryShape::Point, &s(json!(true))),
            RenderShape::Region,
        );
    }

    // ─── render_shape: Block-family ──────────────────────────────────

    #[test]
    fn block_renders_as_region() {
        assert_eq!(
            render_shape(Verb::Block, EntryShape::Block, &s(json!("on"))),
            RenderShape::Region,
        );
    }

    #[test]
    fn block_with_null_state_still_renders_as_region() {
        // Block fill falls back to field color; the block itself is never
        // hidden by an unrendered state value.
        assert_eq!(
            render_shape(Verb::Block, EntryShape::Block, &None),
            RenderShape::Region,
        );
    }

    #[test]
    fn block_switch_primary_is_region() {
        // The block portion is the primary — the trailing switch region is
        // secondary and dispatched by callers via is_switching_verb.
        assert_eq!(
            render_shape(Verb::BlockSwitch, EntryShape::Block, &s(json!("on"))),
            RenderShape::Region,
        );
    }

    // ─── render_shape: Point-family ──────────────────────────────────

    #[test]
    fn point_with_point_shape_is_dot() {
        assert_eq!(
            render_shape(Verb::Point, EntryShape::Point, &None),
            RenderShape::Dot,
        );
    }

    #[test]
    fn point_with_block_shape_is_region() {
        assert_eq!(
            render_shape(Verb::Point, EntryShape::Block, &s(json!(2))),
            RenderShape::Region,
        );
    }

    #[test]
    fn point_with_ongoing_shape_is_marker() {
        assert_eq!(
            render_shape(Verb::Point, EntryShape::Ongoing, &None),
            RenderShape::Marker,
        );
    }

    #[test]
    fn point_switch_primary_is_dot() {
        // The dot is the primary — the trailing region is secondary.
        assert_eq!(
            render_shape(Verb::PointSwitch, EntryShape::Point, &s(json!("a"))),
            RenderShape::Dot,
        );
    }

    // ─── is_switching_verb ───────────────────────────────────────────

    #[test]
    fn switching_verbs_set() {
        assert!(is_switching_verb(Verb::Switch));
        assert!(is_switching_verb(Verb::BlockSwitch));
        assert!(is_switching_verb(Verb::PointSwitch));
        assert!(!is_switching_verb(Verb::Block));
        assert!(!is_switching_verb(Verb::Point));
    }

    // ─── verb-keyed render helpers ───────────────────────────────────

    #[test]
    fn block_verbs_set() {
        assert!(is_block_verb(Verb::Block));
        assert!(is_block_verb(Verb::BlockSwitch));
        assert!(!is_block_verb(Verb::Switch));
        assert!(!is_block_verb(Verb::Point));
        assert!(!is_block_verb(Verb::PointSwitch));
    }

    #[test]
    fn primary_switch_region_verb_is_plain_switch_only() {
        assert!(is_primary_switch_region_verb(Verb::Switch));
        assert!(!is_primary_switch_region_verb(Verb::BlockSwitch));
        assert!(!is_primary_switch_region_verb(Verb::PointSwitch));
        assert!(!is_primary_switch_region_verb(Verb::Block));
        assert!(!is_primary_switch_region_verb(Verb::Point));
    }

    #[test]
    fn secondary_switch_region_is_compound_switching_only() {
        assert!(opens_secondary_switch_region(Verb::BlockSwitch));
        assert!(opens_secondary_switch_region(Verb::PointSwitch));
        assert!(!opens_secondary_switch_region(Verb::Switch));
        assert!(!opens_secondary_switch_region(Verb::Block));
        assert!(!opens_secondary_switch_region(Verb::Point));
    }

    #[test]
    fn dot_color_helpers_only_for_point_switch() {
        assert!(dot_bg_uses_following_state(Verb::PointSwitch));
        assert!(dot_fg_uses_state(Verb::PointSwitch));
        for v in [Verb::Point, Verb::Switch, Verb::Block, Verb::BlockSwitch] {
            assert!(!dot_bg_uses_following_state(v));
            assert!(!dot_fg_uses_state(v));
        }
    }

    #[test]
    fn state_badge_excludes_only_point() {
        for v in [
            Verb::Switch,
            Verb::Block,
            Verb::BlockSwitch,
            Verb::PointSwitch,
        ] {
            assert!(has_state_badge(v), "expected badge for {v}");
        }
        assert!(!has_state_badge(Verb::Point));
    }

    // ─── is_unrendered_switch_value ──────────────────────────────────

    #[test]
    fn unrendered_for_none_null_false_ignored() {
        assert!(is_unrendered_switch_value(None));
        assert!(is_unrendered_switch_value(Some(&JsonValue::Null)));
        assert!(is_unrendered_switch_value(Some(&JsonValue::Bool(false))));
        assert!(is_unrendered_switch_value(Some(&JsonValue::String(
            IGNORED_STATE_VALUE.into()
        ))));
    }

    #[test]
    fn rendered_for_truthy_values() {
        assert!(!is_unrendered_switch_value(Some(&JsonValue::Bool(true))));
        assert!(!is_unrendered_switch_value(Some(&JsonValue::String(
            "happy".into()
        ))));
        assert!(!is_unrendered_switch_value(Some(&json!(0))));
        assert!(!is_unrendered_switch_value(Some(&json!(""))));
    }
}
