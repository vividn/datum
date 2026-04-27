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

use chrono::{FixedOffset, Timelike, Utc};
use datum_core::{
    entry::{Entry, Verb},
    field_spec::{FieldSpec, SpecRegistry},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use super::characters::*;
use super::colors::*;

// ─── Relative-day prefix (single source of truth) ────────────────────────────

/// Datum-style relative-day prefix per `humanTime.ts`:
///
/// | target's local date vs. now's | prefix       |
/// |---                            |---           |
/// | same calendar day             | `None`       |
/// | 1–3 days back                 | `-Nd`        |
/// | 4+ days back                  | `YYYY-MM-DD` |
/// | 1–3 days ahead                | `+Nd`        |
/// | 4+ days ahead                 | `YYYY-MM-DD` |
///
/// Both timestamps are projected onto **target's** local calendar (the
/// `offset_minutes` recorded with the row) so "today" follows the row's
/// timezone rather than the host's. `now` is supplied by the caller so
/// callers control "what today means" (testable + zone-correct).
pub fn format_relative_day_prefix(target: &DatumTime, now: &DatumTime) -> Option<String> {
    let off = FixedOffset::east_opt(target.offset_minutes * 60)
        .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let target_date = target.utc.with_timezone(&off).date_naive();
    let now_date = now.utc.with_timezone(&off).date_naive();
    let delta = (target_date - now_date).num_days();
    if delta == 0 {
        return None;
    }
    if delta.abs() <= 3 {
        Some(if delta < 0 {
            format!("-{}d", -delta)
        } else {
            format!("+{delta}d")
        })
    } else {
        Some(target_date.format("%Y-%m-%d").to_string())
    }
}

/// Convenience: a `DatumTime` snapshot for "now" using the host's local
/// timezone. Used as the default when callers don't supply their own
/// reference instant.
pub(crate) fn now_local() -> DatumTime {
    DatumTime::now()
}

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
/// Tail and other table consumers route the state cell through this so
/// every surface that shows state shares the same left/state/right group.
pub fn format_state_group(
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
    // State badge: single coloured DOT. Tracked field → field color;
    // ignored (prior state was the IGNORED sentinel) → grey.
    let dot_color = if matches!(
        &ctx.last_state,
        Some(JsonValue::String(s)) if s == IGNORED_STATE
    ) {
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
    _first_time: bool,
) -> String {
    // The `first_time` flag is carried in `RenderContext::is_first_occurrence`
    // and drives the `∅` first-tracked left-char inside the state-transition
    // group (§6). Per spec §11, the field badge itself is NOT adorned — none
    // of the 20 acceptance fixtures include a sparkle or other badge marker.
    // Retained in the signature so callers can keep passing the flag without
    // a refactor; the state group is the authoritative emission site.
    let rgb = field_rgb(field, spec);
    paint_bg(rgb, &format!(" {field} "))
}

// ─── Timestamp (§3) ──────────────────────────────────────────────────────────

/// Format a `DatumTime` per §3: `HH:MM:SS+TZ` with the TZ offset rendered
/// dim and compressed (leading zeros dropped). The whole stamp is underlined
/// when `t` is in the future at render time.
///
/// When `t`'s local calendar day differs from `now`'s (per
/// [`format_relative_day_prefix`]), a gray relative-day prefix (`-1d`,
/// `2026-04-15`, etc.) is prepended so day-axis context isn't lost when
/// scanning across rows.
pub(crate) fn format_timestamp(t: &DatumTime, now: &DatumTime) -> String {
    // Render in the row's own timezone — "today" is the row's local day,
    // not the host's. See da-n1q acceptance criteria.
    let local = t.local_time();
    let base = format!("{:02}:{:02}:{:02}", local.hour(), local.minute(), local.second());
    let offset = compress_tz_offset(&local.format("%z").to_string());
    let tz = paint(DIM, &offset);
    let stamp = format!("{base}{tz}");
    let composed = match format_relative_day_prefix(t, now) {
        Some(prefix) => format!("{} {stamp}", paint(GRAY_FG, &prefix)),
        None => stamp,
    };
    if t.utc > Utc::now() && color_enabled() {
        format!("{UNDERLINE}{composed}{RESET}")
    } else {
        // Even without color, the underline escape is a no-op under
        // strip_ansi — return the composed string unchanged.
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
///
/// Defaults `now` to the host's local time. Callers that render a batch of
/// rows (e.g. `datum tail`) should pass a single shared `now` via
/// [`format_info_line_at`] so every row in the batch agrees on what
/// "today" means.
pub fn format_info_line(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
) -> String {
    format_info_line_at(entry, spec, ctx, &now_local())
}

/// As [`format_info_line`] but with an explicit reference instant. Use this
/// for any caller that renders multiple rows or that needs deterministic
/// "today" semantics in a test.
pub fn format_info_line_at(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
    now: &DatumTime,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = &entry.start_time {
        parts.push(format_timestamp(t, now));
    } else if let Some(t) = &entry.end_time {
        parts.push(format_timestamp(t, now));
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
    use chrono::{DateTime, TimeZone};

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

    fn dt(y: i32, mo: u32, d: u32) -> DatumTime {
        let utc: DateTime<Utc> = Utc.with_ymd_and_hms(y, mo, d, 12, 0, 0).unwrap();
        DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
    }

    #[test]
    fn relative_day_prefix_today_returns_none() {
        let now = dt(2026, 4, 22);
        assert_eq!(format_relative_day_prefix(&dt(2026, 4, 22), &now), None);
    }

    #[test]
    fn relative_day_prefix_yesterday_to_three_days_ago() {
        let now = dt(2026, 4, 22);
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 21), &now).as_deref(),
            Some("-1d")
        );
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 20), &now).as_deref(),
            Some("-2d")
        );
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 19), &now).as_deref(),
            Some("-3d")
        );
    }

    #[test]
    fn relative_day_prefix_far_past_uses_iso_date() {
        let now = dt(2026, 4, 22);
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 18), &now).as_deref(),
            Some("2026-04-18")
        );
        assert_eq!(
            format_relative_day_prefix(&dt(2024, 12, 13), &now).as_deref(),
            Some("2024-12-13")
        );
    }

    #[test]
    fn relative_day_prefix_future_one_to_three_days() {
        let now = dt(2026, 4, 22);
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 23), &now).as_deref(),
            Some("+1d")
        );
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 25), &now).as_deref(),
            Some("+3d")
        );
    }

    #[test]
    fn relative_day_prefix_far_future_uses_iso_date() {
        let now = dt(2026, 4, 22);
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 26), &now).as_deref(),
            Some("2026-04-26")
        );
    }

    #[test]
    fn relative_day_prefix_uses_targets_local_calendar() {
        // 22:00 UTC on the 21st in Auckland (UTC+12) → 10:00 on the 22nd
        // local; "today" must be 2026-04-22 by the row's local clock.
        let target = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 21, 22, 0, 0).unwrap(),
            offset_minutes: 12 * 60,
            timezone: "Pacific/Auckland".to_string(),
        };
        let now_in_utc = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 0, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        // Per UTC: target (Apr 21) is "yesterday" relative to now (Apr 22).
        // Per target's local clock: both are Apr 22 → today, no prefix.
        assert_eq!(format_relative_day_prefix(&target, &now_in_utc), None);
    }

    #[test]
    fn timestamp_prepends_prefix_when_not_today() {
        // No-color path so we can match the prefix verbatim.
        std::env::set_var("DATUM_COLOR", "0");
        std::env::set_var("NO_COLOR", "1");
        let target = dt(2026, 4, 19);
        let now = dt(2026, 4, 22);
        let out = format_timestamp(&target, &now);
        assert!(out.starts_with("-3d "), "expected '-3d ' prefix, got {out:?}");
        let target_old = dt(2024, 12, 13);
        let out_old = format_timestamp(&target_old, &now);
        assert!(
            out_old.starts_with("2024-12-13 "),
            "expected ISO-date prefix, got {out_old:?}"
        );
        std::env::remove_var("DATUM_COLOR");
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn timestamp_today_has_no_prefix() {
        std::env::set_var("DATUM_COLOR", "0");
        std::env::set_var("NO_COLOR", "1");
        let target = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 9, 15, 30).unwrap(),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let now = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 17, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let out = format_timestamp(&target, &now);
        assert!(
            out.starts_with("09:15:30"),
            "today rows must not carry a prefix: {out:?}"
        );
        std::env::remove_var("DATUM_COLOR");
        std::env::remove_var("NO_COLOR");
    }
}
