//! Per-verb write-action renderer.
//!
//! Implements the layout described in `docs/visual-parity-analysis.md` §§2, 6, 7,
//! revised by `da-fck` (the state-transition group):
//!
//! ```text
//! <time>  <field-badge>  <LEAD><state-cell><TRAIL>  [duration]
//! ```
//!
//! The state-transition group is `[LEAD][state cell][TRAIL]`, with no
//! whitespace inside the trio. The state cell is the existing bg-coloured
//! badge. The LEAD and TRAIL slots are single literal spaces painted with
//! the prior / continuing state's colour, replacing the old directional
//! arrows. When no prior or continuing state exists (or it's IGNORED), the
//! slot falls back to a lighter-grey bg with the `∅` glyph as fg.
//!
//! See `crates/datum-cli/tests/render_state_group.rs` for the worked
//! examples encoded as fixtures.

use chrono::{FixedOffset, Local, Timelike, Utc};
use datum_core::{
    entry::{Entry, EntryShape, Verb},
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

/// Caller-supplied rendering context — slot bg-colour inputs + first-time hint.
///
/// * `lead_state` — raw output of `state::last_state_at(field, start - ε,
///   exclude=row)`. Drives the LEAD slot's bg colour. `None` = no prior
///   history; the slot falls back to grey-`∅`.
/// * `trail_state` — the state continuing forward from this row. For a
///   `Block` shape, this is `state::last_state_at(field, end_time,
///   exclude=row)` — what resumes after the block closes. For `Point`/
///   `Ongoing`, it's the row's own emitted state (the new switch value,
///   `Bool(true)` for start, `Bool(false)` for stop). Drives the TRAIL
///   slot's bg colour, with the same grey-`∅` fallback.
/// * `is_first_occurrence` — set by the command layer when a field spec
///   is being created for the first time. Currently informational; the
///   LEAD's no-state fallback already conveys the same signal.
/// * `ongoing_state` — the field's compiled state at the row's own time,
///   excluding the row itself. Drives the bg of an occur dot per
///   `da-k5z`. State-transition verbs (switch / start / stop / block) use
///   `lead_state` and `trail_state` instead.
#[derive(Clone, Debug, Default)]
pub struct RenderContext {
    pub lead_state: Option<JsonValue>,
    pub trail_state: Option<JsonValue>,
    pub is_first_occurrence: bool,
    pub ongoing_state: Option<JsonValue>,
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
        Verb::Occur | Verb::Point => Some(render_occur(entry, spec, ctx)),
        Verb::Start
        | Verb::Stop
        | Verb::Switch
        | Verb::Block
        | Verb::BlockSwitch
        | Verb::PointSwitch => Some(render_state_transition_group(entry, spec, ctx)),
    }
}

// ─── Per-verb renderers ──────────────────────────────────────────────────────

fn render_occur(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    let mut out = String::new();
    // Occur: first-time gets ∅; otherwise no left-char.
    if ctx.is_first_occurrence {
        out.push_str(&paint(GRAY_FG, NULL_SET));
    }
    // Dot: field/state color as fg; bg encodes the field's ambient
    // state at the row's time (ongoing block / IGNORED tracking / none).
    let (fg, bg) = compute_occur_dot_colors(entry, spec, ctx.ongoing_state.as_ref());
    out.push_str(&paint_dot(fg, bg, DOT));
    // Occur is a point — no right-char.
    out
}

/// Per da-k5z: the occur "dot" paints the row's field/state color as **fg**
/// (a colored glyph against the terminal bg). `bg` is reserved for ambient
/// context derived from `state_at(field, row.start_time)`:
///
/// 1. `IGNORED` (sentinel string or `Null`) → grey (`Rgb::IGNORED`)
/// 2. Any other ongoing-state value → that state's MD5/spec color
/// 3. `None` or `Bool(false)` (no ongoing block) → `None` (default term bg)
///
/// The caller is responsible for excluding the row itself from the
/// `state_at` query so a non-IGNORED occur on an active block sees the
/// block, not its own (false-emitting) state event.
pub(crate) fn compute_occur_dot_colors(
    row: &Entry,
    spec: Option<&FieldSpec>,
    ongoing_state: Option<&JsonValue>,
) -> (Rgb, Option<Rgb>) {
    let fg = field_rgb(&row.field, spec);
    let bg = match ongoing_state {
        // No prior state, or block was explicitly stopped → no bg.
        None | Some(JsonValue::Bool(false)) => None,
        // IGNORED tracking state → grey bg (sentinel string or null).
        Some(JsonValue::Null) => Some(Rgb::IGNORED),
        Some(JsonValue::String(s)) if s == IGNORED_STATE => Some(Rgb::IGNORED),
        // Any other ongoing-state value → that state's color.
        Some(v) => state_value_rgb(v, &row.field, spec),
    };
    (fg, bg)
}

/// Paint a glyph with explicit fg and an optional bg. With `bg = None`
/// only the fg escape is emitted, leaving the terminal's default bg.
fn paint_dot(fg: Rgb, bg: Option<Rgb>, body: &str) -> String {
    match bg {
        Some(bg) => paint_bg_fg(bg, fg, body),
        None => paint(&fg.fg(), body),
    }
}

/// Render the full `[LEAD][state cell][TRAIL]` trio for switch / start /
/// stop / block. Per da-25l the three segments are flush against each
/// other — no whitespace, coloured or otherwise, between LEAD/cell/TRAIL.
/// LEAD and TRAIL are single bg-coloured cells; the cell body is the
/// state label painted with the state's bg. Outer joining to the field
/// badge is handled by the caller.
fn render_state_transition_group(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
) -> String {
    let mut out = String::new();
    out.push_str(&render_slot(ctx.lead_state.as_ref(), &entry.field, spec));
    out.push_str(&render_state_cell(entry, spec));
    out.push_str(&render_slot(ctx.trail_state.as_ref(), &entry.field, spec));
    out
}

/// Render the central state cell — a bg-coloured badge whose body is the
/// state label itself, with no inline whitespace padding. The LEAD/TRAIL
/// slots provide the visual separation around it.
fn render_state_cell(entry: &Entry, spec: Option<&FieldSpec>) -> String {
    match entry.verb {
        Verb::Start => paint_bg(field_rgb(&entry.field, spec), "start"),
        Verb::Stop => "end".to_string(),
        Verb::Switch | Verb::Block => render_switch_badge(entry.value.as_ref(), &entry.field, spec),
        // Add / Occur are handled by other branches of `format_state_group`
        // and never reach this function.
        _ => String::new(),
    }
}

/// Render a LEAD or TRAIL slot — a single bg-coloured space. Per da-25l
/// "no prior tracked state" is itself a state (IGNORED), so the slot is
/// painted with the IGNORED grey rather than the old `∅`-glyph fallback.
/// Only the explicit "field has ended" case (`Bool(false)` trail after a
/// stop) renders as an uncoloured space.
pub(crate) fn render_slot(
    state: Option<&JsonValue>,
    field: &str,
    spec: Option<&FieldSpec>,
) -> String {
    match resolve_slot(state, field, spec) {
        SlotKind::State(rgb) => paint_bg(rgb, " "),
        SlotKind::Ignored => paint_bg(Rgb::IGNORED, " "),
        SlotKind::FieldEnded => " ".to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotKind {
    State(Rgb),
    Ignored,
    FieldEnded,
}

/// Resolve a slot's colour from a raw `state_at` output. `None`,
/// `Some(Null)` and the `IGNORED` sentinel string collapse to
/// [`SlotKind::Ignored`] — a 1-cell IGNORED-bg space, treating "no prior
/// tracked state" as a regular prior state (IGNORED).
/// `Some(Bool(false))` — the explicit "field has ended" sentinel — maps to
/// [`SlotKind::FieldEnded`] so the trail renders as terminal-default
/// whitespace rather than a coloured cell. Other values defer to
/// [`state_value_rgb`].
fn resolve_slot(state: Option<&JsonValue>, field: &str, spec: Option<&FieldSpec>) -> SlotKind {
    match state {
        None => SlotKind::Ignored,
        Some(JsonValue::Null) => SlotKind::Ignored,
        Some(JsonValue::Bool(false)) => SlotKind::FieldEnded,
        Some(JsonValue::String(s)) if s == IGNORED_STATE => SlotKind::Ignored,
        Some(v) => match state_value_rgb(v, field, spec) {
            Some(rgb) => SlotKind::State(rgb),
            None => SlotKind::Ignored,
        },
    }
}

/// Compute the trail-state from an entry alone — the post-effect state
/// for `Point` / `Ongoing` shapes. Block-shaped rows can't be answered
/// from the entry alone; the caller queries `state_at(end_time)` for
/// those and supplies the result via `RenderContext::trail_state`.
pub fn entry_self_trail_state(entry: &Entry) -> Option<JsonValue> {
    if matches!(entry.entry_shape, EntryShape::Block) {
        return None;
    }
    match entry.verb {
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::PointSwitch => {
            entry.value.clone()
        }
        Verb::Start => Some(JsonValue::Bool(true)),
        Verb::Stop => Some(JsonValue::Bool(false)),
        Verb::Add | Verb::Occur | Verb::Point => None,
    }
}

/// Render the switch state-badge body: scalar = solid coloured block; array
/// = two adjacent blocks with a `state2`-coloured comma. See §7.
fn render_switch_badge(value: Option<&JsonValue>, field: &str, spec: Option<&FieldSpec>) -> String {
    match value {
        None => String::new(),
        Some(JsonValue::Bool(true)) => {
            let fc = field_rgb(field, spec);
            paint_bg(fc, "start")
        }
        Some(JsonValue::Bool(false)) => "end".to_string(),
        Some(JsonValue::Null) => paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, NULL_SET),
        Some(JsonValue::String(s)) if s == IGNORED_STATE => {
            paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, s)
        }
        Some(JsonValue::String(s)) => paint_bg(state_rgb(s, spec), s),
        Some(JsonValue::Array(items)) if items.len() >= 2 => {
            let s1 = &items[0];
            let s2 = &items[1];
            let c1 = state_value_rgb(s1, field, spec).unwrap_or(Rgb::IGNORED);
            let c2 = state_value_rgb(s2, field, spec).unwrap_or(Rgb::IGNORED);
            let mut out = String::new();
            out.push_str(&paint_bg_fg(c1, c2, &scalar_text(s1)));
            out.push_str(&paint(&c2.fg(), ","));
            out.push_str(&paint_bg_fg(c1, c2, &scalar_text(s2)));
            out
        }
        Some(JsonValue::Array(items)) if items.len() == 1 => {
            render_switch_badge(Some(&items[0]), field, spec)
        }
        Some(other) => paint_bg(state_rgb(&other.to_string(), spec), &other.to_string()),
    }
}

fn scalar_text(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ─── Field badge (§4) ────────────────────────────────────────────────────────

/// Render the field name as a bg-painted cell — no decorations, no leading
/// space. Single source of truth for "how to render a field cell"; called
/// from both the info-line (via [`format_field_badge`]) and the table path
/// (via `render::extract_with_context_at`).
///
/// Per spec §11 the field cell carries no sparkle or first-time adornment;
/// the first-tracked signal is the `∅` glyph inside the state-transition
/// group (§6), which is owned by [`format_state_group`].
pub(crate) fn format_field_cell(field: &str, spec: Option<&FieldSpec>) -> String {
    let rgb = field_rgb(field, spec);
    paint_bg(rgb, field)
}

/// Info-line wrapper for [`format_field_cell`]: prepends the single
/// uncoloured leading space the §4 layout requires. The trailing space is
/// supplied by `parts.join(" ")` in [`format_info_line_at`], yielding a
/// 1-space gap between the field name and the state group; combined with
/// the join space after the timestamp this gives the §2 2-space landmark.
pub(crate) fn format_field_badge(field: &str, spec: Option<&FieldSpec>) -> String {
    format!(" {}", format_field_cell(field, spec))
}

// ─── Timestamp (§3) ──────────────────────────────────────────────────────────

/// Format a `DatumTime` per §3: `HH:MM:SS+TZ` with the TZ offset rendered
/// in the gray slug-color and compressed (leading zeros dropped). The whole
/// stamp is underlined when `t` is in the future at render time.
///
/// When `t`'s local calendar day differs from `now`'s (per
/// [`format_relative_day_prefix`]), a gray relative-day prefix (`-1d`,
/// `2026-04-15`, etc.) is prepended so day-axis context isn't lost when
/// scanning across rows.
pub(crate) fn format_timestamp(t: &DatumTime, now: &DatumTime) -> String {
    // Render in the row's own timezone — "today" is the row's local day,
    // not the host's. See da-n1q acceptance criteria.
    let local = t.local_time();
    let base = format!(
        "{:02}:{:02}:{:02}",
        local.hour(),
        local.minute(),
        local.second()
    );
    let offset = compress_tz_offset(&local.format("%z").to_string());
    let tz = paint(GRAY_FG, &offset);
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

/// Wrap a host-local [`chrono::DateTime<Utc>`] (e.g. `create_time`,
/// `modify_time`) as a [`DatumTime`] anchored to the host's local offset, so
/// the same renderer used for occur/end times can format it.
pub(crate) fn synthesize_local_datumtime(utc: chrono::DateTime<Utc>) -> DatumTime {
    let local = utc.with_timezone(&Local);
    let offset_minutes = local.offset().local_minus_utc() / 60;
    DatumTime {
        utc,
        offset_minutes,
        timezone: "Local".to_string(),
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
pub fn format_info_line(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
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
    } else {
        // Hybrid fallback: row has no occur_time, so render create_time
        // prefixed by a gray 'c' — same convention the table renderer
        // applies via `format_gray_prefix("c", ...)` in the
        // `time_hybrid` extractor (render/mod.rs).
        let synth = synthesize_local_datumtime(entry.create_time);
        parts.push(format!("{}{}", paint(GRAY_FG, "c"), format_timestamp(&synth, now)));
    }
    parts.push(format_field_badge(&entry.field, spec));
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
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h{m}m")
        }
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
        DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        }
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
        assert!(
            out.starts_with("-3d "),
            "expected '-3d ' prefix, got {out:?}"
        );
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

    /// da-r1q acceptance: an entry stored with a non-host timezone renders
    /// in the entry's own zone, not the host's `Local`. The bead's example:
    /// `datum occur x -z America/New_York` at Berlin 12:00 CET stores UTC
    /// 10:00 and tail must show `06:00-4` (NY time), not the Berlin or
    /// host conversion.
    #[test]
    fn timestamp_renders_in_entrys_stored_timezone() {
        std::env::set_var("DATUM_COLOR", "0");
        std::env::set_var("NO_COLOR", "1");
        // Berlin 12:00 CEST (UTC+2) → UTC 10:00 → NY 06:00 EDT (UTC-4).
        let ny_target = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 10, 0, 0).unwrap(),
            offset_minutes: -4 * 60,
            timezone: "America/New_York".to_string(),
        };
        // "now" is in Berlin — used to verify the relative-day calc
        // also uses the row's local calendar, not the host's.
        let berlin_now = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 10, 0, 0).unwrap(),
            offset_minutes: 2 * 60,
            timezone: "Europe/Berlin".to_string(),
        };
        let out = format_timestamp(&ny_target, &berlin_now);
        assert!(
            out.contains("06:00:00"),
            "NY-stored row must render as NY wall-clock 06:00:00, got {out:?}"
        );
        assert!(
            out.contains("-4"),
            "NY-stored row must carry the stored -4 offset, got {out:?}"
        );
        assert!(
            !out.contains("12:00:00"),
            "must NOT render as Berlin wall-clock 12:00:00, got {out:?}"
        );
        std::env::remove_var("DATUM_COLOR");
        std::env::remove_var("NO_COLOR");
    }

    // ─── compute_occur_dot_colors (da-k5z) ───────────────────────────────

    fn occur_row(field: &str) -> Entry {
        let ts = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 9, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new(
            field.to_string(),
            Verb::Occur,
            None,
            None,
            None,
            Some(ts),
            None,
            "test-device".to_string(),
        )
    }

    /// No ongoing state and an explicit-stop both collapse to "no bg" so
    /// the dot floats on the terminal's default background.
    #[test]
    fn occur_dot_default_bg_when_no_ongoing_block() {
        let row = occur_row("coffee");
        let expected_fg = field_rgb(&row.field, None);

        let (fg_none, bg_none) = compute_occur_dot_colors(&row, None, None);
        assert_eq!(fg_none, expected_fg, "fg uses field color");
        assert_eq!(bg_none, None, "no ongoing-state → no bg");

        let stopped = JsonValue::Bool(false);
        let (fg_off, bg_off) = compute_occur_dot_colors(&row, None, Some(&stopped));
        assert_eq!(fg_off, expected_fg);
        assert_eq!(bg_off, None, "explicit stop → no bg");
    }

    /// An occur during an ongoing block adopts the block's state color as
    /// bg while keeping the field color as fg.
    #[test]
    fn occur_dot_uses_ongoing_state_color_as_bg() {
        let row = occur_row("water");
        let ongoing = JsonValue::String("sleep".to_string());
        let (fg, bg) = compute_occur_dot_colors(&row, None, Some(&ongoing));
        assert_eq!(fg, field_rgb(&row.field, None));
        assert_eq!(
            bg,
            Some(state_rgb("sleep", None)),
            "bg should be derived from the ongoing state name, not the field"
        );
        assert_ne!(fg, bg.unwrap(), "fg and bg are independently colored");
    }

    /// Both the IGNORED sentinel string and a `Null` ongoing-state map to
    /// the shared grey bg per §8.
    #[test]
    fn occur_dot_uses_grey_bg_when_field_is_ignored() {
        let row = occur_row("blocked");
        let ignored_sentinel = JsonValue::String(IGNORED_STATE.to_string());
        let (_, bg_str) = compute_occur_dot_colors(&row, None, Some(&ignored_sentinel));
        assert_eq!(bg_str, Some(Rgb::IGNORED), "IGNORED sentinel → grey bg");

        let null = JsonValue::Null;
        let (_, bg_null) = compute_occur_dot_colors(&row, None, Some(&null));
        assert_eq!(bg_null, Some(Rgb::IGNORED), "null state → grey bg");
    }
}
