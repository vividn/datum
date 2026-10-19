//! Pure data + helpers for `datum nowview`.
//!
//! Per bead da-6jy the SVG render path now reuses [`crate::dayview`] —
//! nowview is a single-day dayview scoped to the last `history_minutes`
//! with a relative-minute axis. The CLI builds a [`crate::dayview::DayviewConfig`]
//! directly, so this module no longer carries its own compute/layout types.
//!
//! What remains:
//! - `NowviewField` / `NowviewBlock` / `NowviewPoint`: input shape consumed
//!   by [`crate::nowview_terminal`] for the terminal band rendered under
//!   `datum now`. The CLI's nowview command emits both shapes (terminal +
//!   dayview-style) from the same DB scan.
//! - `default_width_for_history`: the history-aware width default used when
//!   the user omits `--width`.
//! - `tick_config`: the major/minor tick interval chooser shared with the
//!   axis-tick generator below.
//! - `nowview_axis_ticks`: produces [`crate::dayview::DayviewAxisTick`]s
//!   labelled relative to "now" (`-15m`, `-30m`, …) so dayview's renderer
//!   can draw a sub-hour axis without knowing nowview exists.
//!
//! No I/O, no datum-core, no system deps — compiles cleanly to
//! `wasm32-unknown-unknown` so the SvelteKit PWA can render the same view
//! client-side.

use serde::{Deserialize, Serialize};

use crate::dayview::{
    DayviewAxisTick, DayviewConfig, DayviewDayInput, DayviewFieldInput, DayviewLayout, DayviewSpan,
    Orientation,
};
use crate::dayview_svg::{text_width, VERTICAL_AXIS_LABEL_INSET, VERTICAL_AXIS_LABEL_MAX};

/// One block (closed or ongoing) inside a field row. Consumed by the
/// terminal renderer in [`crate::nowview_terminal`].
///
/// Per da-9vop, parallel state sets (`switch body vertical,sit`) carry
/// every member's colour: a 1-element `colors` is a regular single-state
/// block; a 2+-element `colors` paints the cells as a checkerboard so
/// each state is visible inside the terminal slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewBlock {
    pub start_ms: i64,
    pub end_ms: i64,
    /// One or more hex fill colours (e.g. `#a1b2c3`) — one per active
    /// state member. The caller resolves these from the value via the
    /// project's colour algorithm; the layout is colour-blind.
    pub colors: Vec<String>,
    /// Opaque label for hover / accessibility. Optional.
    pub state_label: Option<String>,
}

/// One point-in-time marker (occur dot) inside a field row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewPoint {
    pub time_ms: i64,
    /// Foreground fill — typically the state's color.
    pub fg_color: String,
    pub state_label: Option<String>,
    /// As [`crate::dayview::DayviewPoint::glyph`]. The band draws it only
    /// when it is one column wide — see [`crate::nowview_terminal`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
}

/// One field's data for the nowview window.
///
/// The `y_frac`, `height_frac`, `z_index`, and `default_colors` fields are
/// consumed by the visual terminal modes (Snapshot / Flow in
/// [`crate::nowview_terminal::RenderMode`]) so bands can be ordered by spec
/// position and painted with a fallback color when no block is active at
/// "now". Callers that only need the labeled inline band can leave them at
/// their defaults — the `InlineBand` renderer doesn't read them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewField {
    pub field: String,
    pub blocks: Vec<NowviewBlock>,
    pub points: Vec<NowviewPoint>,
    /// Spec-resolved `y` position in `[0, 1]`. Visual modes sort bands
    /// top-to-bottom by this value (ties broken by `z_index`).
    #[serde(default)]
    pub y_frac: f64,
    /// Spec-resolved band height as a fraction of the plot area. Visual
    /// modes scale each band's row count by this.
    #[serde(default)]
    pub height_frac: f64,
    /// Spec-resolved render order. Defaults to `y_frac` when the spec
    /// leaves it unset.
    #[serde(default)]
    pub z_index: f64,
    /// Hex fill color(s) to paint when no block / point covers the
    /// rendered timestamp. Single-element for solid colors; multi-element
    /// for parallel-state defaults (rare). Empty → renderer falls back to
    /// the empty-cell shading.
    #[serde(default)]
    pub default_colors: Vec<String>,
}

/// What each field's lane is showing *right now* — the chunk drawn to the
/// right of the now-line (da-nk84, superseding the labelled panel of
/// da-34w). The caller resolves the active state per rendered field at
/// "now"; the SVG overlay paints one chunk per entry, aligned to that
/// field's band and running from the now-line to the right edge.
///
/// Per da-9vop, parallel state sets carry every member's colour in
/// `colors` so the chunk can be striped with each state's hue rather than
/// borrowing only the first state's colour.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewCurrentState {
    pub field: String,
    /// Active state value rendered as text. `None` means the field has no
    /// state at "now" (e.g. an occur-only field with no recent dot) — such
    /// a field gets no chunk. `datum nowview --json` still emits the row.
    pub state_label: Option<String>,
    /// One or more hex fill colours — one per active state member. A
    /// 1-element vector paints a solid chunk; a 2+-element vector paints
    /// the 45° stripes dayview already uses for parallel-state blocks.
    pub colors: Vec<String>,
    /// The whole hid of the entry that OPENED this state (da-6hg6v) — the
    /// chunk is that entry's open block, so it names it the way every
    /// other mark names its own entry. `None` when no row governs the
    /// field at the anchor. `datum nowview --json` carries it on the
    /// panel row, so a caller reading the panel and a caller reading the
    /// SVG select the same entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Pick the default SVG width when the user didn't pass `--width`. Mirrors
/// datum's nowviewCmd.ts §"Scale default width based on history size":
/// ≤0  → 100, ≤5m → 200, ≤10m → 300, else 400.
pub fn default_width_for_history(history_minutes: f64) -> f64 {
    if history_minutes <= 0.0 {
        100.0
    } else if history_minutes <= 5.0 {
        200.0
    } else if history_minutes <= 10.0 {
        300.0
    } else {
        400.0
    }
}

/// Major / minor tick interval in minutes. Mirrors `getTickConfig` in
/// nowview.ts. `minor_minutes` is `None` for very short histories where the
/// 5-minute major spacing is already fine-grained enough.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickConfig {
    pub major_minutes: f64,
    pub minor_minutes: Option<f64>,
}

pub fn tick_config(history_minutes: f64) -> TickConfig {
    // A window of two minutes or less is ruled at its own length alone, so
    // its far end is labelled and its whole minutes, flowing past
    // (`--ahead`), have the axis to themselves (da-hklaz).
    if history_minutes <= 2.0 {
        TickConfig {
            major_minutes: history_minutes,
            minor_minutes: None,
        }
    } else if history_minutes <= 15.0 {
        TickConfig {
            major_minutes: 5.0,
            minor_minutes: None,
        }
    } else if history_minutes <= 60.0 {
        TickConfig {
            major_minutes: 15.0,
            minor_minutes: Some(5.0),
        }
    } else if history_minutes <= 180.0 {
        TickConfig {
            major_minutes: 30.0,
            minor_minutes: Some(10.0),
        }
    } else {
        TickConfig {
            major_minutes: 60.0,
            minor_minutes: Some(15.0),
        }
    }
}

/// `m` minutes as a tick spells it: whole minutes as `5m`, anything else
/// in seconds, `30s`.
fn relative_label(m: f64) -> String {
    if (m - m.round()).abs() < 1e-6 {
        format!("{}m", m.round() as i64)
    } else {
        format!("{}s", (m * 60.0).round() as i64)
    }
}

/// Build the relative-minute axis ticks for a nowview-style window. The
/// returned ticks plug directly into
/// [`crate::dayview::DayviewConfig::axis_ticks_override`] — they share the
/// `(time_pos, label, is_major)` shape so dayview's renderer can paint
/// gridlines and labels without nowview-specific code.
///
/// `time_axis_extent` is the data-area pixel extent along the time axis.
/// The "now" instant maps to `time_axis_extent`; "history_minutes ago" maps
/// to 0. The `now` tick is included so callers (and dayview's gridlines)
/// see the right edge marked.
pub fn nowview_axis_ticks(time_axis_extent: f64, history_minutes: f64) -> Vec<DayviewAxisTick> {
    nowview_axis_ticks_around(time_axis_extent, history_minutes, 0.0)
}

/// [`nowview_axis_ticks`] for a window reaching `before` minutes back and
/// `after` minutes on past the line it is about (a time flag, da-pcis): ticks
/// step out from that line both ways, `-15m` before it and `+15m` after,
/// and the line's own tick is the one without a label.
pub fn nowview_axis_ticks_around(
    time_axis_extent: f64,
    before: f64,
    after: f64,
) -> Vec<DayviewAxisTick> {
    ticks_spaced_for(time_axis_extent, before, after, before + after)
}

/// [`nowview_axis_ticks_around`] spaced as a window of `window` minutes
/// would be, however much more the axis reaches: a picture drawn with
/// surroundings rules them as its window does (da-61nm8).
fn ticks_spaced_for(
    time_axis_extent: f64,
    before: f64,
    after: f64,
    window: f64,
) -> Vec<DayviewAxisTick> {
    let span = before + after;
    if span <= 0.0 || time_axis_extent <= 0.0 {
        return Vec::new();
    }
    let cfg = tick_config(window);
    let pos = |m: f64| (before + m) / span * time_axis_extent;
    // The line's own tick (no label — the now-line owns it).
    let mut ticks = vec![DayviewAxisTick {
        time_pos: pos(0.0),
        hour: 0,
        label: String::new(),
        is_major: true,
    }];

    // Place every minor stride (which subsumes major positions because
    // major is always a multiple of minor in `tick_config`); flag majors.
    let minor = cfg.minor_minutes.unwrap_or(cfg.major_minutes);
    for (sign, reach) in [('-', before), ('+', after)] {
        let mut m = minor;
        while m <= reach + 1e-9 {
            let is_major = (m % cfg.major_minutes).abs() < 1e-6
                || ((m % cfg.major_minutes) - cfg.major_minutes).abs() < 1e-6;
            ticks.push(DayviewAxisTick {
                time_pos: pos(if sign == '-' { -m } else { m }),
                hour: 0,
                label: format!("{sign}{}", relative_label(m)),
                is_major,
            });
            m += minor;
        }
    }
    // Sort ascending by time_pos: emit_time_axis places labels in order.
    // The now tick, unlabelled, gets no mark there; the now-line owns it.
    ticks.sort_by(|a, b| {
        a.time_pos
            .partial_cmp(&b.time_pos)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_width_scales_with_history() {
        assert_eq!(default_width_for_history(0.0), 100.0);
        assert_eq!(default_width_for_history(5.0), 200.0);
        assert_eq!(default_width_for_history(10.0), 300.0);
        assert_eq!(default_width_for_history(15.0), 400.0);
        assert_eq!(default_width_for_history(60.0), 400.0);
    }

    #[test]
    fn tick_config_picks_intervals_from_history() {
        assert_eq!(
            tick_config(15.0),
            TickConfig {
                major_minutes: 5.0,
                minor_minutes: None
            }
        );
        assert_eq!(
            tick_config(60.0),
            TickConfig {
                major_minutes: 15.0,
                minor_minutes: Some(5.0)
            }
        );
        assert_eq!(
            tick_config(180.0),
            TickConfig {
                major_minutes: 30.0,
                minor_minutes: Some(10.0)
            }
        );
        assert_eq!(
            tick_config(360.0),
            TickConfig {
                major_minutes: 60.0,
                minor_minutes: Some(15.0)
            }
        );
    }

    #[test]
    fn axis_ticks_30min_window_use_15min_majors_and_5min_minors() {
        // 30 min → major=15, minor=5. Expect ticks at -5,-10,-15,-20,-25,-30
        // labeled, plus a trailing unlabeled "now" tick at the right edge.
        let ticks = nowview_axis_ticks(600.0, 30.0);
        let labels: Vec<&str> = ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["-30m", "-25m", "-20m", "-15m", "-10m", "-5m", ""],
        );
        // -15m and -30m are the majors.
        let majors: Vec<&str> = ticks
            .iter()
            .filter(|t| t.is_major)
            .map(|t| t.label.as_str())
            .collect();
        assert_eq!(majors, vec!["-30m", "-15m", ""]);
        // -30m is at the left edge; "now" at the right.
        assert!(ticks.first().unwrap().time_pos.abs() < 1e-6);
        assert!((ticks.last().unwrap().time_pos - 600.0).abs() < 1e-6);
    }

    /// da-hklaz: two minutes or less is ruled at its own length, a part of
    /// a minute spelled in seconds, so the far end is labelled and the
    /// window's whole minutes have the axis to themselves.
    #[test]
    fn a_short_window_rules_its_own_length() {
        assert_eq!(
            tick_config(2.0),
            TickConfig {
                major_minutes: 2.0,
                minor_minutes: None
            }
        );
        let labels = |history: f64| -> Vec<String> {
            nowview_axis_ticks(600.0, history)
                .into_iter()
                .map(|t| t.label)
                .collect()
        };
        assert_eq!(labels(1.0), vec!["-1m", ""]);
        assert_eq!(labels(0.5), vec!["-30s", ""]);
        assert_eq!(labels(5.0), vec!["-5m", ""]);
    }

    #[test]
    fn axis_ticks_empty_for_zero_history() {
        assert!(nowview_axis_ticks(600.0, 0.0).is_empty());
        assert!(nowview_axis_ticks(0.0, 30.0).is_empty());
    }
}

// ─── Shared config assembly (CLI + browser) ──────────────────────────────────

/// Default history window (minutes) when the caller passes none.
pub const DEFAULT_HISTORY_MINUTES: f64 = 30.0;
/// Default per-day band height for nowview's single day.
pub const DEFAULT_DAY_HEIGHT: f64 = 250.0;
/// How much of the plot the now strip takes — the chunk beside the
/// now-line where each field's current state is drawn — whatever the
/// window: vividn, 2026-10-15, «let's keep this now sliver relatively small»
/// (da-9g3bf). It was five minutes of the history's scale, the original
/// datum's, which was most of a short window's picture.
pub const NOW_STRIP_FRACTION: f64 = 1.0 / 16.0;

/// The knobs `datum nowview` exposes that affect geometry. Everything else
/// nowview needs is derived — that derivation is [`nowview_config`], and it
/// is shared so the browser draws the same picture the CLI does.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NowviewGeometry {
    /// How far back the window reaches from "now".
    pub history_minutes: f64,
    /// Total SVG width. `None` → history-sized default plus the panel bump
    /// horizontally; vertically, [`DEFAULT_DAY_HEIGHT`] as the band.
    pub width: Option<f64>,
    /// Requested total height. `<= 0` → [`DEFAULT_DAY_HEIGHT`] as the band
    /// horizontally; vertically, the history-sized default.
    pub height: f64,
    pub margin: f64,
    pub time_axis_height: f64,
    /// Centre the window on the instant instead of ending it there
    /// (a time flag, da-pcis): half the history either side of the line, and no
    /// now strip, since the half after the line is drawn from the record.
    #[serde(default)]
    pub centred: bool,
    /// Which way time runs (da-78evn): dayview's own enum, so the two
    /// pictures cannot disagree about what vertical means. `width` and
    /// `height` stay the SVG's; the time axis takes whichever it runs
    /// along, and the defaults turn with it.
    #[serde(default)]
    pub orientation: Orientation,
    /// How much more time to draw either side of the window, at its scale
    /// and with its ticks (da-61nm8): before it, and after it too when it
    /// is centred — a window ending at now has its now strip there. The
    /// canvas grows by as much; [`crop_to_window`] crops the SVG's box back
    /// to the window and leaves the rest drawn outside it.
    #[serde(default)]
    pub surround_minutes: f64,
    /// How much time to draw on past now, behind the now line, in a window
    /// ending at now (da-hklaz): the day reaches this far, at the window's
    /// scale, as ONE stretch running on under the now strip, so a viewer
    /// sliding the picture along with the clock has what comes next to
    /// reveal. Neither the canvas nor the ticks grow by it. Nothing for a
    /// centred window or the snapshot.
    #[serde(default)]
    pub ahead_minutes: f64,
}

impl Default for NowviewGeometry {
    fn default() -> Self {
        Self {
            history_minutes: DEFAULT_HISTORY_MINUTES,
            width: None,
            height: 0.0,
            margin: 2.0,
            time_axis_height: 22.0,
            centred: false,
            orientation: Orientation::Horizontal,
            surround_minutes: 0.0,
            ahead_minutes: 0.0,
        }
    }
}

/// Build the [`DayviewConfig`] nowview renders through: one day scoped to
/// `[now - history, now]`, no date label, relative-minute axis ticks, and a
/// reservation on the right for the current-state panel.
///
/// `at_ms` is the instant the picture is about — now, or the one a time flag
/// names — and `now_ms` is now. A zero history is the snapshot (da-pcis): no
/// time band and no axis, the whole plot the strip carrying each field's
/// state at that instant. Any other window holding now has the now strip
/// at it, the future drawn on past it (da-9g3bf).
///
/// Pure: `fields` is the only data input, so the CLI (SQLite) and the PWA
/// (SQLite-wasm) reach the same geometry from their own stores.
pub fn nowview_config(
    fields: Vec<DayviewFieldInput>,
    at_ms: i64,
    now_ms: i64,
    geom: &NowviewGeometry,
) -> DayviewConfig {
    let history_minutes = geom.history_minutes.max(0.0);
    let vertical = geom.orientation == Orientation::Vertical;
    // The canvas along the time axis and across it: the SVG's width and
    // height, turned on its side when time runs down the page.
    let (along, across) = if vertical {
        (
            Some(geom.height).filter(|h| *h > 0.0),
            geom.width.unwrap_or(0.0),
        )
    } else {
        (geom.width, geom.height)
    };
    let along = along.unwrap_or_else(|| default_width_for_history(history_minutes));
    let snapshot = history_minutes <= 0.0;
    let after = if geom.centred {
        history_minutes / 2.0
    } else {
        0.0
    };
    let before = history_minutes - after;
    let (pad_before, pad_after) = surround(geom);
    let ahead = ahead(geom);

    // da-34w: nowview drops the date label — it's timestamp-focused, not
    // date-focused. The HH:MM clock overlay near the now-line takes its
    // place.
    let mut day = DayviewDayInput {
        date: None,
        label_lines: Vec::new(),
        start_ms: at_ms - ((before + pad_before) * 60_000.0) as i64,
        end_ms: at_ms + ((after + pad_after + ahead) * 60_000.0) as i64,
        errors: Vec::new(),
        fields,
        spans: Vec::new(),
    };
    // What is drawn ahead runs on past the axis's end at its scale: one
    // stretch whose far end is past 1, under the now strip and beyond it,
    // where a window holding now otherwise draws the future past the strip.
    if ahead > 0.0 {
        let hi = (day.end_ms - day.start_ms) as f64 / (at_ms - day.start_ms).max(1) as f64;
        day.spans = vec![DayviewSpan {
            start_ms: day.start_ms,
            end_ms: day.end_ms,
            lo: 0.0,
            hi,
            label: String::new(),
            offset_s: None,
        }];
    }

    // The yellow line is now, and only now: absent when now is outside.
    // The snapshot has no axis to draw it on, and its chips — the panel
    // the line gates — ARE its instant's states: at a selected time they
    // are that time's, where they were none (da-rskp's line through them).
    let now_ms = if snapshot { at_ms } else { now_ms };
    // The strip is the WINDOW's: now in the surroundings alone reserves
    // none, so the window keeps its scale whatever surrounds it.
    let window = (
        at_ms - (before * 60_000.0) as i64,
        at_ms + (after * 60_000.0) as i64,
    );
    let plot_width = (along - 2.0 * geom.margin).max(0.0);
    let now_width = if snapshot {
        plot_width
    } else if window.0 < now_ms && now_ms <= window.1 {
        plot_width * NOW_STRIP_FRACTION
    } else {
        0.0
    };
    // Pre-compute the time-axis extent so the relative-minute tick override
    // resolves to the same pixel space dayview will use. Mirrors the math in
    // `dayview::compute_dayview`. Date label is suppressed (label_width = 0)
    // so the time axis only loses the now-strip reservation.
    let window_extent = (plot_width - now_width).max(0.0);
    let pad_px = surround_px(geom, window_extent);
    let along = along + pad_px.0 + pad_px.1;
    let time_axis_extent_for_ticks = window_extent + pad_px.0 + pad_px.1;
    let mut ticks = ticks_spaced_for(
        time_axis_extent_for_ticks,
        before + pad_before,
        after + pad_after,
        history_minutes,
    );
    // The ticks past now are drawn past the strip, as the future is.
    // The ticks' axis ends where the window and its surroundings do, short
    // of what is drawn ahead.
    let axis_end_ms = day.end_ms - (ahead * 60_000.0) as i64;
    let now_pos = (now_ms - day.start_ms) as f64 / (axis_end_ms - day.start_ms).max(1) as f64
        * time_axis_extent_for_ticks;
    for t in ticks.iter_mut().filter(|t| t.time_pos > now_pos + 1e-9) {
        t.time_pos += now_width;
    }
    let time_axis_height = if snapshot {
        0.0
    } else if vertical {
        vertical_axis_strip(&ticks, geom.time_axis_height)
    } else {
        geom.time_axis_height
    };
    let axis_ticks_override = Some(ticks);

    let day_height = if across > 0.0 {
        // Use the requested extent as the per-day band, leaving room for
        // axis + margins. Floor at a sensible minimum so labels stay
        // readable.
        (across - 2.0 * geom.margin - time_axis_height).max(50.0)
    } else {
        DEFAULT_DAY_HEIGHT
    };

    DayviewConfig {
        days: vec![day],
        now_ms,
        width: if vertical { across } else { along },
        height: vertical.then_some(along),
        day_height,
        interday_margin: 0.0,
        time_axis_height,
        margin: geom.margin,
        // Date label suppressed — give all of the cross-perpendicular budget
        // to the data area (and the panel reservation).
        label_width: Some(0.0),
        orientation: geom.orientation,
        axis_ticks_override,
        right_panel_width: now_width,
    }
}

/// The minutes [`NowviewGeometry::surround_minutes`] adds before the window
/// and after it. None for the snapshot, which has no time to add to.
pub fn surround(geom: &NowviewGeometry) -> (f64, f64) {
    let s = geom.surround_minutes.max(0.0);
    if geom.history_minutes <= 0.0 {
        (0.0, 0.0)
    } else if geom.centred {
        (s, s)
    } else {
        (s, 0.0)
    }
}

/// The minutes [`NowviewGeometry::ahead_minutes`] draws past now: none for
/// a centred window or the snapshot.
pub fn ahead(geom: &NowviewGeometry) -> f64 {
    if geom.centred || geom.history_minutes <= 0.0 {
        0.0
    } else {
        geom.ahead_minutes.max(0.0)
    }
}

/// [`surround`] in px, at the scale `window_extent` px draws the window at.
fn surround_px(geom: &NowviewGeometry, window_extent: f64) -> (f64, f64) {
    let (before, after) = surround(geom);
    let per_minute = window_extent / geom.history_minutes.max(f64::MIN_POSITIVE);
    (before * per_minute, after * per_minute)
}

/// Crop an SVG drawn from [`nowview_config`]'s config back to its window: the
/// root keeps the window's size, its `viewBox` starts where the window
/// does, and `overflow="visible"` leaves the surroundings drawn past its
/// edges, for a viewer to move into view (da-61nm8). Unchanged without
/// surroundings.
pub fn crop_to_window(svg: &str, geom: &NowviewGeometry, layout: &DayviewLayout) -> String {
    let (before, after) = surround(geom);
    let window_extent = layout.time_axis_extent * geom.history_minutes
        / (geom.history_minutes + before + after).max(f64::MIN_POSITIVE);
    let (before, after) = surround_px(geom, window_extent);
    let Some(end) = svg.find('>').filter(|_| before + after > 0.0) else {
        return svg.to_string();
    };
    let root = &svg[..end];
    let value = |name: &str| root.split(&format!(" {name}=\"")).nth(1)?.split('"').next();
    let (Some(w), Some(h), Some(view)) = (value("width"), value("height"), value("viewBox")) else {
        return svg.to_string();
    };
    let (Ok(wf), Ok(hf)) = (w.parse::<f64>(), h.parse::<f64>()) else {
        return svg.to_string();
    };
    let (x, y, cw, ch) = match geom.orientation {
        Orientation::Horizontal => (before, 0.0, wf - before - after, hf),
        Orientation::Vertical => (0.0, before, wf, hf - before - after),
    };
    let root = root
        .replacen(&format!(" width=\"{w}\""), &format!(" width=\"{cw}\""), 1)
        .replacen(&format!(" height=\"{h}\""), &format!(" height=\"{ch}\""), 1)
        .replacen(
            &format!(" viewBox=\"{view}\""),
            &format!(" viewBox=\"{x} {y} {cw} {ch}\" overflow=\"visible\""),
            1,
        );
    // The ground is the viewport's 100%, which is now the window's: it
    // is laid under the whole drawing instead.
    let rest = svg[end..].replacen(
        r#"<rect class="bg" width="100%" height="100%""#,
        &format!(r#"<rect class="bg" width="{w}" height="{h}""#),
        1,
    );
    format!("{root}{rest}")
}

/// How wide a vertical time-axis strip must be (da-78evn): at least what
/// was asked for, and enough to hold its widest label — a tick's, or the
/// `HH:MM` clock the overlay sets beside the now-line — beside the
/// baseline. Horizontally a strip's height is one line of text whatever
/// it says; vertically its width is the text's own.
///
/// Sized at [`VERTICAL_AXIS_LABEL_MAX`], which is what the renderer draws
/// in any strip that holds five characters of it.
fn vertical_axis_strip(ticks: &[DayviewAxisTick], asked: f64) -> f64 {
    let size = VERTICAL_AXIS_LABEL_MAX;
    let widest = ticks
        .iter()
        .map(|t| t.label.as_str())
        .chain(["00:00"])
        .map(|l| text_width(l, size))
        .fold(0.0, f64::max);
    asked.max(widest + VERTICAL_AXIS_LABEL_INSET)
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn nowview_config_scopes_the_day_to_the_history_window() {
        let cfg = nowview_config(
            Vec::new(),
            1_000_000,
            1_000_000,
            &NowviewGeometry::default(),
        );
        assert_eq!(cfg.days.len(), 1);
        assert_eq!(cfg.days[0].end_ms, 1_000_000);
        assert_eq!(cfg.days[0].start_ms, 1_000_000 - 30 * 60_000);
        // Date label suppressed, now strip reserved, relative ticks installed.
        assert_eq!(cfg.label_width, Some(0.0));
        assert!(cfg.right_panel_width > 0.0);
        assert!(!cfg.axis_ticks_override.as_ref().unwrap().is_empty());
    }

    #[test]
    fn nowview_config_derives_width_and_band_height() {
        let cfg = nowview_config(Vec::new(), 0, 0, &NowviewGeometry::default());
        assert_eq!(
            cfg.width,
            default_width_for_history(DEFAULT_HISTORY_MINUTES)
        );
        assert_eq!(cfg.day_height, DEFAULT_DAY_HEIGHT);

        let geom = NowviewGeometry {
            height: 300.0,
            margin: 2.0,
            time_axis_height: 22.0,
            ..NowviewGeometry::default()
        };
        let cfg = nowview_config(Vec::new(), 0, 0, &geom);
        assert_eq!(cfg.day_height, 300.0 - 4.0 - 22.0);
    }

    /// da-9g3bf: the now strip is a small sliver, the same share of the
    /// plot whatever the window — where it was five minutes of the
    /// history's scale, 99px of a 15m window's 400.
    #[test]
    fn now_strip_is_a_sixteenth_of_the_plot_at_any_window() {
        for history_minutes in [5.0, 15.0, 360.0] {
            let geom = NowviewGeometry {
                history_minutes,
                width: Some(400.0),
                ..NowviewGeometry::default()
            };
            let cfg = nowview_config(Vec::new(), 0, 0, &geom);
            assert_eq!(cfg.width, 400.0);
            assert_eq!(cfg.right_panel_width, 396.0 / 16.0, "{history_minutes}m");
        }
    }

    /// da-pcis: a zero history is the snapshot — the whole plot is the
    /// strip, with no axis and no ticks, at the snapshot's own width.
    #[test]
    fn zero_history_is_the_snapshot() {
        let geom = NowviewGeometry {
            history_minutes: 0.0,
            ..NowviewGeometry::default()
        };
        let cfg = nowview_config(Vec::new(), 1_000_000, 1_000_000, &geom);
        assert_eq!(cfg.width, default_width_for_history(0.0));
        assert_eq!(cfg.days[0].start_ms, cfg.days[0].end_ms);
        assert_eq!(cfg.right_panel_width, cfg.width - 2.0 * geom.margin);
        assert_eq!(cfg.time_axis_height, 0.0);
        assert!(cfg.axis_ticks_override.unwrap().is_empty());
    }

    /// da-pcis: a time flag centres the window on the instant, with no strip
    /// when now is not in it, and the instant's unlabelled tick in the
    /// middle of the axis.
    #[test]
    fn centred_window_straddles_the_instant() {
        let geom = NowviewGeometry {
            width: Some(404.0),
            centred: true,
            ..NowviewGeometry::default()
        };
        let cfg = nowview_config(Vec::new(), 1_000_000, 1_000_000 + 3_600_000, &geom);
        assert_eq!(cfg.days[0].start_ms, 1_000_000 - 15 * 60_000);
        assert_eq!(cfg.days[0].end_ms, 1_000_000 + 15 * 60_000);
        assert_eq!(cfg.right_panel_width, 0.0);
        let ticks = cfg.axis_ticks_override.unwrap();
        let labels: Vec<&str> = ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["-15m", "-10m", "-5m", "", "+5m", "+10m", "+15m"]
        );
        assert!((ticks[3].time_pos - 200.0).abs() < 1e-9);
    }

    /// da-hklaz: drawn ahead, a window ending at now reaches on past it as
    /// ONE stretch at the window's scale — its far end past the axis's, under
    /// the strip — and neither the canvas nor the ticks move. A centred
    /// window draws nothing ahead.
    #[test]
    fn ahead_is_one_stretch_on_past_now() {
        let now = 10_000_000_i64;
        let plain = NowviewGeometry {
            width: Some(404.0),
            history_minutes: 1.0,
            surround_minutes: 1.0,
            ..NowviewGeometry::default()
        };
        let ahead = NowviewGeometry {
            ahead_minutes: 1.0,
            ..plain
        };
        let before = nowview_config(Vec::new(), now, now, &plain);
        let cfg = nowview_config(Vec::new(), now, now, &ahead);
        assert_eq!(cfg.width, before.width);
        assert_eq!(cfg.right_panel_width, before.right_panel_width);
        assert_eq!(cfg.days[0].start_ms, now - 120_000);
        assert_eq!(cfg.days[0].end_ms, now + 60_000);
        let span = &cfg.days[0].spans[..];
        assert_eq!(span.len(), 1);
        assert_eq!((span[0].lo, span[0].hi), (0.0, 1.5));
        let ticks = |c: &DayviewConfig| -> Vec<(String, f64)> {
            (c.axis_ticks_override.clone().unwrap().into_iter())
                .map(|t| (t.label, t.time_pos))
                .collect()
        };
        assert_eq!(ticks(&cfg), ticks(&before));

        let centred = NowviewGeometry {
            centred: true,
            ..ahead
        };
        let cfg = nowview_config(Vec::new(), now, now, &centred);
        assert!(cfg.days[0].spans.is_empty());
        assert_eq!(cfg.days[0].end_ms, now + 90_000);
    }

    /// da-9g3bf: a centred window holding now has the now strip at now, and
    /// what is after now — its ticks too — is drawn past the strip.
    #[test]
    fn centred_window_holding_now_has_the_strip_at_now() {
        let geom = NowviewGeometry {
            width: Some(404.0),
            centred: true,
            ..NowviewGeometry::default()
        };
        // The window is the instant ± 15m; now is 5m before the instant.
        let cfg = nowview_config(Vec::new(), 1_000_000, 1_000_000 - 300_000, &geom);
        assert_eq!(cfg.now_ms, 1_000_000 - 300_000);
        let strip = 400.0 / 16.0;
        assert_eq!(cfg.right_panel_width, strip);
        let extent = 400.0 - strip;
        let ticks = cfg.axis_ticks_override.unwrap();
        let at = |label: &str| ticks.iter().find(|t| t.label == label).unwrap().time_pos;
        // -5m is now: it stays, and what is after it is a strip further on.
        assert!((at("-10m") - extent / 6.0).abs() < 1e-9);
        assert!((at("-5m") - extent / 3.0).abs() < 1e-9);
        assert!((at("+5m") - (extent * 2.0 / 3.0 + strip)).abs() < 1e-9);
        assert!((at("+15m") - (extent + strip)).abs() < 1e-9);
    }
}
