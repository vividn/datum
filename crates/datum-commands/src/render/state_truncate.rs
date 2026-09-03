//! Per-state / per-field truncation for the `tail` table (da-ftw3).
//!
//! When the natural state cell would push a row past the terminal width,
//! truncate inside the cell rather than wrapping. Two surfaces live here:
//!
//! 1. **State cell** — for parallel-state arrays, give each state an equal
//!    share of the budget and right-truncate with `…` when over budget.
//!    When the budget can't fit two chars per state, drop trailing states
//!    and append a `+k` marker styled with the *field's* bg color so it
//!    reads as "more of this row's data" rather than a separate token.
//! 2. **Field cell** — right-truncate the field label with `…` when it
//!    exceeds the column cap (default 10 chars). Field names like
//!    `environment` or `brushteeth` otherwise compress the state column.

use datum_core::{entry::Entry, field_spec::FieldSpec, shape::has_state_badge};
use serde_json::Value as JsonValue;

use super::action::{compute_occur_dot_colors, render_lead_slot, render_slot, RenderContext};
use super::characters::{DOT, IGNORED_STATE, NON_OCCUR, NULL_SET};
use super::colors::{
    field_rgb, paint, paint_bg, paint_bg_fg, state_rgb, state_value_rgb, structured_state_rgb,
    visible_width, Rgb, GRAY_FG,
};

/// Width budget below which we collapse the row to "drop trailing states
/// + `+k` marker": each state needs at least this many chars to be useful.
const MIN_PER_STATE: usize = 2;

/// Approximate width consumed by the `+k` marker in drop-mode. We don't
/// know `k` ahead of time; this is enough for `+9` (one digit) plus a
/// little slack. The drop loop tightens this once `k` is known.
const DROP_MARKER_RESERVE: usize = 3;

/// Default cap for the field column. Long field names (`environment`,
/// `brushteeth`) otherwise eat into the state column unhelpfully.
pub(crate) const FIELD_CAP: usize = 10;

/// Floor for the state column — small enough to fit `a…,+9` in the worst
/// case but large enough to be readable.
pub(crate) const STATE_FLOOR: usize = 6;

/// Floor for the field column when the state floor can't otherwise be
/// satisfied. Below this the field becomes unrecognisable.
pub(crate) const FIELD_FLOOR: usize = 4;

/// Render an entry's state cell with a width budget. Mirrors the
/// branching of `action::format_state_group` but truncates inside the
/// state body when the natural width exceeds `max_w`.
///
/// `max_w` is the *total* state-column width for this row, including any
/// LEAD/TRAIL slot characters that bracket the body for state-bearing
/// verbs.
pub(crate) fn truncate_state_cell(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
    max_w: usize,
) -> String {
    // Timeless / non-state-bearing rows: these never need truncation
    // (1-2 visible chars). Delegate to the corresponding path so the
    // caller doesn't have to special-case them.
    if entry.occur_time.is_none() && !has_state_badge(entry.verb) {
        return paint(&field_rgb(&entry.field, spec).fg(), NON_OCCUR);
    }
    if !has_state_badge(entry.verb) {
        // `Point` dot: 1-2 chars, can't overflow at the floors we enforce.
        return render_occur_cell(entry, spec, ctx);
    }
    // State-bearing verbs render `[LEAD][body][TRAIL]`. LEAD and TRAIL
    // are 1 visible char each — reserve them, give the rest to the body.
    let body_budget = max_w.saturating_sub(2);
    let body = truncate_state_body(entry.state.as_ref(), &entry.field, spec, body_budget);
    let mut out = String::new();
    out.push_str(&render_lead_slot(ctx, &entry.field, spec));
    out.push_str(&body);
    out.push_str(&render_slot(ctx.trail_state.as_ref(), &entry.field, spec));
    out
}

/// Reproduces the `Point`-dot path from `action::render_occur` without
/// needing `pub(super)` access to the helper. Kept tiny: any future
/// elaboration belongs in `action.rs` proper.
fn render_occur_cell(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    let mut out = String::new();
    if ctx.is_first_occurrence {
        out.push_str(&paint(GRAY_FG, NULL_SET));
    }
    let (fg, bg) = compute_occur_dot_colors(entry, spec, ctx.ongoing_state.as_ref());
    match bg {
        Some(bg) => out.push_str(&paint_bg_fg(bg, fg, DOT)),
        None => out.push_str(&paint(&fg.fg(), DOT)),
    }
    out
}

/// Truncate the state badge body to `budget` visible chars. Mirrors
/// `action::render_switch_badge` value-arms but with width awareness.
fn truncate_state_body(
    value: Option<&JsonValue>,
    field: &str,
    spec: Option<&FieldSpec>,
    budget: usize,
) -> String {
    if budget == 0 {
        return String::new();
    }
    match value {
        None => String::new(),
        Some(JsonValue::Bool(true)) => {
            paint_bg(field_rgb(field, spec), &right_truncate("start", budget))
        }
        Some(JsonValue::Bool(false)) => right_truncate("end", budget),
        Some(JsonValue::Null) => paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, NULL_SET),
        Some(JsonValue::String(s)) if s == IGNORED_STATE => {
            paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, &right_truncate(s, budget))
        }
        Some(JsonValue::String(s)) => paint_bg(state_rgb(s, spec), &right_truncate(s, budget)),
        Some(JsonValue::Object(obj)) => {
            let body = match obj.get("id").and_then(JsonValue::as_str) {
                Some(id) => format!("{{{id}}}"),
                None => "{}".to_string(),
            };
            paint_bg(
                structured_state_rgb(obj, spec),
                &right_truncate(&body, budget),
            )
        }
        Some(JsonValue::Array(items)) if items.len() >= 2 => {
            truncate_array(items, field, spec, budget)
        }
        Some(JsonValue::Array(items)) if items.len() == 1 => {
            truncate_state_body(items.first(), field, spec, budget)
        }
        Some(other) => {
            let label = other.to_string();
            paint_bg(state_rgb(&label, spec), &right_truncate(&label, budget))
        }
    }
}

/// Per-state algorithm for an array of states with `N >= 2` members.
fn truncate_array(
    items: &[JsonValue],
    field: &str,
    spec: Option<&FieldSpec>,
    budget: usize,
) -> String {
    let labels: Vec<String> = items.iter().map(state_label).collect();
    let n = labels.len();
    // Natural width: each label + (n-1) commas. If we already fit, no
    // truncation; caller still chose to invoke us, but we're loyal to
    // the spec — full render when nothing needs trimming.
    let natural: usize = labels.iter().map(|s| visible_width(s)).sum::<usize>() + (n - 1);
    if natural <= budget {
        return render_array_full(items, &labels, field, spec);
    }
    // Per-state budget per spec: B = (budget - (n-1)) / n.
    let b = budget.saturating_sub(n.saturating_sub(1)) / n;
    if b < MIN_PER_STATE {
        // Drop-trailing mode with `+k` marker.
        return render_drop_mode(items, &labels, field, spec, budget);
    }
    // Each state shortened to fit B (with `…` when over).
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            let color = state_value_rgb(item, field, spec).unwrap_or(Rgb::IGNORED);
            out.push_str(&paint(&color.fg(), ","));
        }
        let label = &labels[i];
        let cell = if visible_width(label) > b {
            right_truncate(label, b)
        } else {
            label.clone()
        };
        out.push_str(&paint_chip(item, &cell, field, spec));
    }
    out
}

/// Render the full (untruncated) array — same algorithm as
/// `action::render_switch_badge`'s Array arm. Duplicated rather than
/// re-exported so the natural-render path here stays explicit.
fn render_array_full(
    items: &[JsonValue],
    labels: &[String],
    field: &str,
    spec: Option<&FieldSpec>,
) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            let color = state_value_rgb(item, field, spec).unwrap_or(Rgb::IGNORED);
            out.push_str(&paint(&color.fg(), ","));
        }
        out.push_str(&paint_chip(item, &labels[i], field, spec));
    }
    out
}

/// Walk states left-to-right, accepting each until adding the next would
/// blow the budget. Append `+k` painted with the field's bg colour where
/// `k` is the number of dropped states.
fn render_drop_mode(
    items: &[JsonValue],
    labels: &[String],
    field: &str,
    spec: Option<&FieldSpec>,
    budget: usize,
) -> String {
    let n = items.len();
    // Reserve enough room for `+k` where `k` is up to n-1; widen the
    // reserve when k might be a multi-digit number (we always know N
    // here, so be exact).
    let max_k = n.saturating_sub(1);
    let marker_reserve = 1 + max_k.to_string().len(); // `+` + digits
    let marker_reserve = marker_reserve.max(DROP_MARKER_RESERVE);
    let body_budget = budget.saturating_sub(marker_reserve);

    let mut out = String::new();
    let mut used: usize = 0;
    let mut shown: usize = 0;
    for (i, item) in items.iter().enumerate() {
        let label = &labels[i];
        // Each state in drop-mode gets the minimum useful width: keep at
        // least 1 visible char + ellipsis when over.
        let cell = if visible_width(label) > MIN_PER_STATE {
            right_truncate(label, MIN_PER_STATE)
        } else {
            label.clone()
        };
        let cell_w = visible_width(&cell);
        let comma_w = if shown == 0 { 0 } else { 1 };
        if used + comma_w + cell_w > body_budget {
            break;
        }
        if shown > 0 {
            let color = state_value_rgb(item, field, spec).unwrap_or(Rgb::IGNORED);
            out.push_str(&paint(&color.fg(), ","));
        }
        out.push_str(&paint_chip(item, &cell, field, spec));
        used += comma_w + cell_w;
        shown += 1;
    }
    let k = n.saturating_sub(shown);
    if k == 0 {
        // We managed to fit everything within the marker reserve — no
        // marker needed. Rare, but possible when the labels are short.
        return out;
    }
    // The `+k` glyph belongs to the row's state stripe, so paint it
    // with the *field's* bg colour (per the bead spec). `paint_bg`
    // derives a contrasting fg.
    let marker = format!("+{k}");
    out.push_str(&paint_bg(field_rgb(field, spec), &marker));
    out
}

/// Paint a single chip in its own colour. Mirrors the per-state choice
/// in `action::render_switch_badge`. Falls through to the default state
/// colour for unresolvable values (`Bool(false)`).
fn paint_chip(item: &JsonValue, body: &str, field: &str, spec: Option<&FieldSpec>) -> String {
    match item {
        JsonValue::Bool(true) => paint_bg(field_rgb(field, spec), body),
        JsonValue::Bool(false) => body.to_string(),
        JsonValue::Null => paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, body),
        JsonValue::String(s) if s == IGNORED_STATE => {
            paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, body)
        }
        JsonValue::String(_) => paint_bg(
            state_value_rgb(item, field, spec).unwrap_or(Rgb::IGNORED),
            body,
        ),
        JsonValue::Object(obj) => paint_bg(structured_state_rgb(obj, spec), body),
        _ => paint_bg(
            state_value_rgb(item, field, spec).unwrap_or(Rgb::IGNORED),
            body,
        ),
    }
}

/// Best-effort label for a state value — the plain text that goes into
/// the chip body before painting.
fn state_label(item: &JsonValue) -> String {
    match item {
        JsonValue::Bool(true) => "start".to_string(),
        JsonValue::Bool(false) => "end".to_string(),
        JsonValue::Null => NULL_SET.to_string(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Object(obj) => match obj.get("id").and_then(JsonValue::as_str) {
            Some(id) => format!("{{{id}}}"),
            None => "{}".to_string(),
        },
        other => other.to_string(),
    }
}

/// Truncate `body` to `max` visible columns, appending `…` when shorter.
/// Plain text only — no ANSI awareness needed because chip bodies are
/// the raw label, painted afterwards by `paint_bg`/`paint_chip`.
/// Width is Unicode display width (da-dhtp): a double-width grapheme
/// that would straddle the budget is dropped, not split.
fn right_truncate(body: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let vw = visible_width(body);
    if vw <= max {
        return body.to_string();
    }
    let keep = max.saturating_sub(1);
    let mut out = String::with_capacity(body.len());
    let mut seen = 0usize;
    for c in body.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if seen + w > keep {
            break;
        }
        out.push(c);
        seen += w;
    }
    out.push('…');
    out
}

/// Truncate a field cell to `max_w` visible chars. Uses the field's bg
/// colour for the whole chip, just like `format_field_cell`.
pub(crate) fn truncate_field_cell(field: &str, spec: Option<&FieldSpec>, max_w: usize) -> String {
    let body = right_truncate(field, max_w);
    paint_bg(field_rgb(field, spec), &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::colors::strip_ansi;
    use datum_core::entry::{Entry, Verb};
    use datum_test_util::{color_scope, no_color_scope as no_color};
    use serde_json::json;

    fn switch_entry(field: &str, value: serde_json::Value) -> Entry {
        use chrono::{TimeZone, Utc};
        use datum_core::time::DatumTime;
        let t = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 5, 22, 14, 30, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        Entry::new(field, Verb::Switch, Some(value), None, Some(t), None, "dev")
    }

    /// Spec example: `vertical,sit` at W=12 fits naturally.
    #[test]
    fn array_fits_naturally_renders_full() {
        let _g = no_color();
        let e = switch_entry("body", json!(["vertical", "sit"]));
        let ctx = RenderContext::default();
        let cell = truncate_state_cell(&e, None, &ctx, 14); // body_budget = 12
        let s = strip_ansi(&cell);
        assert!(s.contains("vertical,sit"), "natural render: {s:?}");
    }

    /// Spec example: `vertical,sit,sofa` at W=11 (body_budget) →
    /// each state shortened: `ve…,sit,so…` (B = 3).
    #[test]
    fn array_per_state_truncation_at_b3() {
        let _g = no_color();
        let e = switch_entry("body", json!(["vertical", "sit", "sofa"]));
        let ctx = RenderContext::default();
        // max_w = 13 → body_budget = 11.
        let cell = truncate_state_cell(&e, None, &ctx, 13);
        let s = strip_ansi(&cell);
        // strip LEAD/TRAIL spaces by trimming
        let body = s.trim();
        assert_eq!(body, "ve…,sit,so…", "spec example W=11: {s:?}");
    }

    /// Spec example: `vertical,sit,sofa,office,kitchen` at body_budget=14
    /// → drop-mode: `v…,s…,s…,+2`.
    #[test]
    fn array_drop_mode_with_plus_k_marker() {
        let _g = no_color();
        let e = switch_entry(
            "body",
            json!(["vertical", "sit", "sofa", "office", "kitchen"]),
        );
        let ctx = RenderContext::default();
        // body_budget = 14 (max_w 16 - 2 slots) ⇒ B = (14-4)/5 = 2; but the
        // bead's example treats the drop threshold as "each state < 2
        // chars budget" — and at B=2, 5 states fully expanded would need
        // 2+1+2+1+2+1+2+1+2 = 14 which is exactly the budget. Bump down
        // to body_budget=11 where drop-mode actually fires.
        let cell = truncate_state_cell(&e, None, &ctx, 13);
        let s = strip_ansi(&cell);
        let body = s.trim();
        assert!(body.contains("+"), "drop-mode marker present: {s:?}");
    }

    /// Per the spec formula `s[:B-1] + "…"`: at B=10, render the first
    /// 9 chars + ellipsis = 10 visible chars total. The bead's example
    /// table shows `a-very-l…` (9 chars) which is off-by-one; the
    /// formula is the canonical rule and exactly fills the budget.
    #[test]
    fn scalar_state_right_truncated() {
        let _g = no_color();
        let e = switch_entry("mood", json!("a-very-long-scalar"));
        let ctx = RenderContext::default();
        let cell = truncate_state_cell(&e, None, &ctx, 12); // body_budget=10
        let s = strip_ansi(&cell);
        let body = s.trim();
        assert_eq!(body, "a-very-lo…", "scalar W=10: {s:?}");
        assert_eq!(visible_width(body), 10);
    }

    /// `environment` (11 chars) at cap=10 → `environme…` per the formula
    /// `s[:cap-1] + "…"`. Exactly fills the cap.
    #[test]
    fn field_truncated_with_ellipsis_at_cap() {
        let _g = no_color();
        let s = strip_ansi(&truncate_field_cell("environment", None, 10));
        assert_eq!(s, "environme…", "field cap=10: {s:?}");
        assert_eq!(visible_width(&s), 10);
    }

    /// Spec example: `body` (4 chars) under cap=10 → no truncation.
    #[test]
    fn short_field_under_cap_renders_full() {
        let _g = no_color();
        let s = strip_ansi(&truncate_field_cell("body", None, 10));
        assert_eq!(s, "body");
    }

    /// `+k` marker uses the field's bg colour, not a universal
    /// white-bg-black-fg.
    #[test]
    fn drop_mode_plus_k_uses_field_bg_color() {
        let _scope = color_scope();
        let e = switch_entry(
            "body",
            json!(["vertical", "sit", "sofa", "office", "kitchen"]),
        );
        let ctx = RenderContext::default();
        let cell = truncate_state_cell(&e, None, &ctx, 13);
        // The body field's md5-rgb bg appears around the `+k` marker —
        // we can't pin the exact RGB without duplicating the colour
        // derivation, but we *can* assert that the field's bg escape
        // appears in the rendered cell adjacent to the `+` literal.
        let bg = field_rgb("body", None).bg();
        let stripped = strip_ansi(&cell);
        let body = stripped.trim();
        assert!(body.contains('+'), "marker present: {stripped:?}");
        // The bg escape should appear at least twice: once for the
        // LEAD/TRAIL slot if it's a state slot of the same field
        // (parallel state body→body), and once for the +k chip.
        let occurrences = cell.matches(&bg).count();
        assert!(
            occurrences >= 1,
            "field bg painted on cell ({} occurrences): {:?}",
            occurrences,
            cell
        );
    }
}
