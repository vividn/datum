//! Per-verb write-action renderer.
//!
//! Implements the layout described in `docs/visual-parity-analysis.md` §§2, 6, 7:
//!
//! ```text
//! <time>  <field-badge>  <state-transition-group>  [duration]
//!                        └─ <left-char><state-badge><right-char>
//! ```
//!
//! The state-transition group composes a LEFT-char (prior-state indicator),
//! a STATE-badge (the new value), and a RIGHT-char (post-entry indicator)
//! per §6. See `render_fixtures.rs` for the full 20-case acceptance suite.

use chrono::{Local, Timelike, Utc};
use datum_core::{
    entry::{Entry, Verb},
    field_spec::{FieldSpec, SpecRegistry},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use super::characters::*;
use super::colors::*;

// ─── Render context ──────────────────────────────────────────────────────────

/// Caller-supplied rendering context — prior state + first-time hint.
///
/// * `last_state` — the field's prior active state. `None` = the field has
///   no prior entries at all; `Some(false)` = prior was stopped/ended;
///   `Some(value)` = active with that value.
/// * `is_first_occurrence` — set by the command layer when a field spec
///   is being created for the first time. Triggers the `∅` first-tracked
///   marker per §6.
///
/// "Same-state repeat" (yellow `!`) is derived inside the renderer by
/// comparing `last_state` to the current entry's value, so callers don't
/// need to pre-compute it.
#[derive(Clone, Debug, Default)]
pub struct RenderContext {
    pub last_state: Option<JsonValue>,
    pub is_first_occurrence: bool,
}

// ─── State-transition group (§6 / §7) ────────────────────────────────────────

/// Render the `<left-char><state-badge><right-char>` group for an entry.
///
/// The field badge is rendered separately and prepended in the info line.
/// Returns `None` for `add` (handled inline with `¢`).
pub(crate) fn format_state_group(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
) -> Option<String> {
    match entry.verb {
        Verb::Add => Some(paint(&field_rgb(&entry.field, spec).fg(), NON_OCCUR)),
        Verb::Occur => Some(render_occur(entry, spec, ctx)),
        Verb::Start | Verb::Block => Some(render_start(entry, spec, ctx)),
        Verb::Stop => Some(render_end(entry, spec, ctx)),
        Verb::Switch => Some(render_switch(entry, spec, ctx)),
    }
}

// ─── Per-verb renderers ──────────────────────────────────────────────────────

fn render_occur(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    let mut out = String::new();
    // Occur: first-time gets ∅; otherwise no left-char.
    if ctx.is_first_occurrence {
        out.push_str(&paint(GRAY_FG, NULL_SET));
    }
    // State badge: single coloured DOT. Tracked field → field color; ignored
    // / untracked → grey.
    let dot_color = if spec.map(|s| s.ignored).unwrap_or(false) {
        Rgb::IGNORED
    } else {
        field_rgb(&entry.field, spec)
    };
    out.push_str(&paint_bg(dot_color, DOT));
    // Occur is a point — no right-char.
    out
}

fn render_start(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    let mut out = String::new();
    // "Already active" = prior was true OR prior was some non-false value
    // that represents an open block. For start, the canonical repeat is
    // `start → start` (prior == true).
    let already_active = matches!(ctx.last_state, Some(JsonValue::Bool(true)));
    if ctx.is_first_occurrence {
        out.push_str(&paint(GRAY_FG, NULL_SET));
    } else if already_active {
        out.push_str(&paint(YELLOW_FG, WARNING));
    } else if let Some(prior) = &ctx.last_state {
        if prior_has_visible_indicator(prior) {
            let c = state_value_rgb(prior, &entry.field, spec).unwrap_or(Rgb::IGNORED);
            out.push_str(&paint(&c.fg(), ARROW_LEFT));
        }
    }
    // State badge: "start" on field color.
    let fc = field_rgb(&entry.field, spec);
    out.push_str(&paint_bg(fc, " start "));
    // Right-char: true/field color arrow.
    out.push_str(&paint(&fc.fg(), ARROW_RIGHT));
    out
}

fn render_end(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    let mut out = String::new();
    // "Already ended" = prior was explicitly false.
    let already_ended = matches!(ctx.last_state, Some(JsonValue::Bool(false)));
    if already_ended {
        out.push_str(&paint(YELLOW_FG, WARNING));
    } else if ctx.is_first_occurrence {
        out.push_str(&paint(GRAY_FG, NULL_SET));
    } else if let Some(prior) = &ctx.last_state {
        if prior_has_visible_indicator(prior) {
            let c = state_value_rgb(prior, &entry.field, spec).unwrap_or(Rgb::IGNORED);
            out.push_str(&paint(&c.fg(), ARROW_LEFT));
        }
    }
    // State badge: "end" — plain (no bg).
    out.push_str(" end ");
    // No right-char on end.
    out
}

fn render_switch(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    let value = entry.value.as_ref();
    let has_duration = entry.duration_seconds().is_some();

    // Special case: IGNORED state → grey badge, no transition chars.
    if let Some(JsonValue::String(s)) = value {
        if s == IGNORED_STATE {
            return paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, &format!(" {IGNORED_STATE} "));
        }
    }
    // Special case: false value → `[end]`-style with ← from prior.
    if matches!(value, Some(JsonValue::Bool(false))) {
        return render_end(entry, spec, ctx);
    }

    let is_array = matches!(value, Some(JsonValue::Array(items)) if items.len() >= 2);
    let prior_is_array = matches!(&ctx.last_state, Some(JsonValue::Array(items)) if items.len() >= 2);

    // Same-state repeat: prior value equals current value (including
    // `prior == true`, which flags a `switch true` that duplicates the
    // active state).
    let is_same_state_repeat = match (&ctx.last_state, value) {
        (Some(p), Some(v)) => p == v,
        _ => false,
    };

    let mut out = String::new();
    // ─── Left-char ───
    if ctx.is_first_occurrence {
        out.push_str(&paint(GRAY_FG, NULL_SET));
    } else if is_same_state_repeat {
        out.push_str(&paint(YELLOW_FG, WARNING));
    } else if prior_is_array {
        // §6: array prior → SHADING colored with first element.
        let c = ctx
            .last_state
            .as_ref()
            .and_then(|v| state_value_rgb(v, &entry.field, spec))
            .unwrap_or(Rgb::IGNORED);
        out.push_str(&paint(&c.fg(), SHADING));
    } else if let Some(prior) = &ctx.last_state {
        if prior_has_visible_indicator(prior) {
            let c = state_value_rgb(prior, &entry.field, spec).unwrap_or(Rgb::IGNORED);
            out.push_str(&paint(&c.fg(), ARROW_LEFT));
        }
    }

    // ─── State badge ───
    out.push_str(&render_switch_badge(value, &entry.field, spec));

    // ─── Right-char ───
    // Determine the state that is active AFTER this entry:
    //   - Switch with dur (block) → prior resumes
    //   - Switch no dur → new state continues
    let right_color: Option<Rgb> = if has_duration {
        // Prior resumes
        if prior_is_array {
            // Emit SHADING instead of arrow to echo the checkerboard motif.
            let c = ctx
                .last_state
                .as_ref()
                .and_then(|v| state_value_rgb(v, &entry.field, spec));
            if let Some(c) = c {
                out.push_str(&paint(&c.fg(), SHADING));
                None
            } else {
                None
            }
        } else {
            ctx.last_state
                .as_ref()
                .and_then(|v| state_value_rgb(v, &entry.field, spec))
        }
    } else if is_array {
        // Array, no dur → array continues; emit arrow colored with state[0].
        value.and_then(|v| state_value_rgb(v, &entry.field, spec))
    } else {
        // Scalar, no dur → new state continues.
        value.and_then(|v| state_value_rgb(v, &entry.field, spec))
    };
    if let Some(c) = right_color {
        out.push_str(&paint(&c.fg(), ARROW_RIGHT));
    }
    out
}

/// Render the switch state-badge body: scalar = solid coloured block; array
/// = two adjacent blocks with a `state2`-coloured comma. See §7.
fn render_switch_badge(
    value: Option<&JsonValue>,
    field: &str,
    spec: Option<&FieldSpec>,
) -> String {
    match value {
        None => String::new(),
        Some(JsonValue::Bool(true)) => {
            let fc = field_rgb(field, spec);
            paint_bg(fc, " start ")
        }
        Some(JsonValue::Bool(false)) => " end ".to_string(),
        Some(JsonValue::Null) => paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, &format!(" {NULL_SET} ")),
        Some(JsonValue::String(s)) if s == IGNORED_STATE => {
            paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, &format!(" {s} "))
        }
        Some(JsonValue::String(s)) => paint_bg(state_rgb(s, spec), &format!(" {s} ")),
        Some(JsonValue::Array(items)) if items.len() >= 2 => {
            let s1 = &items[0];
            let s2 = &items[1];
            let c1 = state_value_rgb(s1, field, spec).unwrap_or(Rgb::IGNORED);
            let c2 = state_value_rgb(s2, field, spec).unwrap_or(Rgb::IGNORED);
            let mut out = String::new();
            out.push_str(&paint_bg_fg(c1, c2, &format!(" {} ", scalar_text(s1))));
            out.push_str(&paint(&c2.fg(), ","));
            out.push_str(&paint_bg_fg(c1, c2, &format!(" {} ", scalar_text(s2))));
            out
        }
        Some(JsonValue::Array(items)) if items.len() == 1 => {
            render_switch_badge(Some(&items[0]), field, spec)
        }
        Some(other) => paint_bg(state_rgb(&other.to_string(), spec), &format!(" {other} ")),
    }
}

fn scalar_text(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Whether a prior state carries a visible left-char indicator. Prior=false
/// is silent — neither an arrow nor a glyph — because the natural "stopped
/// then starts" flow shouldn't clutter the line.
fn prior_has_visible_indicator(prior: &JsonValue) -> bool {
    !matches!(prior, JsonValue::Bool(false))
}

// ─── Field badge (§4) ────────────────────────────────────────────────────────

pub(crate) fn format_field_badge(
    field: &str,
    spec: Option<&FieldSpec>,
    first_time: bool,
) -> String {
    let rgb = field_rgb(field, spec);
    let painted = paint_bg(rgb, &format!(" {field} "));
    if first_time {
        format!("{FIRST_TIME_MARKER}{painted}")
    } else {
        painted
    }
}

// ─── Timestamp (§3) ──────────────────────────────────────────────────────────

/// Format a `DatumTime` per §3: `HH:MM:SS+TZ` with the TZ offset rendered
/// dim and compressed (leading zeros dropped). The whole stamp is underlined
/// when `t` is in the future at render time.
pub(crate) fn format_timestamp(t: &DatumTime) -> String {
    let local = t.utc.with_timezone(&Local);
    let base = format!("{:02}:{:02}:{:02}", local.hour(), local.minute(), local.second());
    let offset = compress_tz_offset(&local.format("%z").to_string());
    let tz = paint(DIM, &offset);
    let composed = format!("{base}{tz}");
    if t.utc > Utc::now() && color_enabled() {
        format!("{UNDERLINE}{composed}{RESET}")
    } else if t.utc > Utc::now() {
        // Even without color, make it visually distinct via the same string;
        // the underline escape is a no-op in stripped tests.
        composed
    } else {
        composed
    }
}

/// Compress `%z` output (`+0200`) → `+2` per §3.
fn compress_tz_offset(raw: &str) -> String {
    // Expect `+HHMM` or `-HHMM`.
    if raw.len() < 3 {
        return raw.to_string();
    }
    let (sign, rest) = raw.split_at(1);
    if rest.len() < 4 {
        return raw.to_string();
    }
    let hours = &rest[0..2];
    let minutes = &rest[2..4];
    // Parse hours, drop leading zeros.
    let h: i32 = hours.parse().unwrap_or(0);
    let m: i32 = minutes.parse().unwrap_or(0);
    if m == 0 {
        format!("{sign}{h}")
    } else if h == 0 {
        format!("{sign}0:{m:02}")
    } else {
        format!("{sign}{h}:{m:02}")
    }
}

// ─── Info-line composition ───────────────────────────────────────────────────

/// Produce the main info line `<time>  <field>  <state-group>  [dur]`.
/// Returns an empty string when there's nothing to show.
pub fn format_info_line(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = &entry.start_time {
        parts.push(format_timestamp(t));
    } else if let Some(t) = &entry.end_time {
        parts.push(format_timestamp(t));
    }
    parts.push(format_field_badge(&entry.field, spec, ctx.is_first_occurrence));
    if let Some(group) = format_state_group(entry, spec, ctx) {
        if !group.is_empty() {
            parts.push(group);
        }
    }
    if let Some(secs) = entry.duration_seconds() {
        parts.push(format_duration(secs));
    }
    parts.join(" ")
}

/// Backwards-compatible "info line" used by the legacy show_action path,
/// exposed for tests and alternate front-ends.
pub fn render_entry_line(
    entry: &Entry,
    registry: Option<&SpecRegistry>,
    ctx: &RenderContext,
) -> String {
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    format_info_line(entry, spec, ctx)
}

// ─── Duration ────────────────────────────────────────────────────────────────

pub(crate) fn format_duration(secs: i64) -> String {
    let secs = secs.abs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 { format!("{h}h") } else { format!("{h}h{m}m") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_tz_offset_drops_leading_zeros() {
        assert_eq!(compress_tz_offset("+0200"), "+2");
        assert_eq!(compress_tz_offset("-0500"), "-5");
        assert_eq!(compress_tz_offset("+0130"), "+1:30");
        assert_eq!(compress_tz_offset("+0000"), "+0");
    }

    #[test]
    fn format_duration_picks_units() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(240), "4m");
        assert_eq!(format_duration(3600), "1h");
        assert_eq!(format_duration(3720), "1h2m");
    }
}
