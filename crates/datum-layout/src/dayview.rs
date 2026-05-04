//! Pure layout for `datum dayview` — multi-day stacked timeline (datum
//! parity + horizontal orientation per bead da-a7f).
//!
//! Reference: `/tmp/datum-survey/src/dayview/{dayview,singleday,allFieldsSvg,fieldSvgBlocks}.ts`.
//!
//! The CLI builds a [`DayviewConfig`] (per-day windows + per-field blocks
//! and points with pre-resolved colors), this module computes pixel-space
//! geometry, and the renderer in [`crate::dayview_svg`] paints it.
//!
//! ## Coordinate model
//!
//! Output structures use orientation-neutral coordinates:
//! - **time-axis** — runs along time (00:00 → 24:00)
//! - **cross-axis** — perpendicular to time; field bands stack on this
//!   axis within a day, AND days stack on this axis across rows/cols
//!
//! The renderer maps `(time, cross)` → SVG `(x, y)` per orientation:
//! - [`Orientation::Horizontal`]: time→x, cross→y (datum's original layout —
//!   time axis runs left-to-right, days stack top-to-bottom)
//! - [`Orientation::Vertical`]: time→y, cross→x (time axis runs top-to-bottom,
//!   each day is a column)
//!
//! No I/O, no datum-core, no system deps — compiles cleanly to
//! `wasm32-unknown-unknown` so the SvelteKit PWA can render the same view
//! client-side.
//!
//! TODO: Timezone handling. v1 renders in the host's local time. A day with
//! entries in multiple timezones (DST or travel) should bifurcate visually
//! during the ambiguous overlap window — see bead da-vp7's spec for the
//! long-term plan. Bifurcation will be a separate visualization bead.

use serde::{Deserialize, Serialize};

/// Visual orientation of the dayview. Named by the time-axis direction.
/// Default is [`Orientation::Horizontal`] per the kit-spec design
/// discussion (2026-04-30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    /// Time runs vertically (Y axis, top→bottom); days stack horizontally
    /// (X axis). Date labels on top of each column, time-axis ticks on
    /// the left. Each day is a column.
    Vertical,
    /// Time runs horizontally (X axis, left→right); days stack vertically
    /// (Y axis). Date labels on the left, time-axis ticks at the bottom.
    /// This is datum's original layout.
    Horizontal,
}

impl Default for Orientation {
    fn default() -> Self {
        Orientation::Horizontal
    }
}

/// One day's window + per-field input data. The caller (CLI) resolves the
/// label strings, day boundaries (in UTC ms), and the field data —
/// including state colors — before passing to [`compute_dayview`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewDayInput {
    /// Pre-formatted label lines for the date axis on the left
    /// (e.g. `["Mon", "Apr 27", "2026"]`). The layout auto-shrinks from
    /// 3 → 2 → 1 line based on row height + minimum font size.
    pub label_lines: Vec<String>,
    /// Day window start (inclusive) in UTC ms.
    pub start_ms: i64,
    /// Day window end (exclusive) in UTC ms.
    pub end_ms: i64,
    /// True if this day has any unresolved conflicts (drives the warning
    /// icon next to the date label). Source: the conflicts queue.
    pub has_error: bool,
    /// Per-field data for this day, with positioning and colors already
    /// resolved.
    pub fields: Vec<DayviewFieldInput>,
}

/// One field's data for a day. Position/height/z-index are fractions of
/// the day's data-row (not pixels) — the layout multiplies by `day_height`
/// and sorts by `z_index` (low to high → low renders first / underneath).
///
/// `y_frac` and `height_frac` describe **perpendicular-to-time** positioning,
/// which the render layer rotates per orientation (per bead da-a7f).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewFieldInput {
    pub field: String,
    /// Position along the cross-axis (perpendicular to time) as a fraction
    /// (0.0 = near edge, 1.0 = far edge) of the day's data area. In horizontal
    /// mode 0.0 = top of the row, in vertical mode 0.0 = left of the day's
    /// column. Caller resolves from [`FieldSpec`] (y, height, z_index columns)
    /// with deterministic-hash fallback.
    pub y_frac: f64,
    /// Cross-axis extent as a fraction of the day's data area.
    pub height_frac: f64,
    /// Stacking order; higher = on top. Defaults to `y_frac` when unset
    /// per the original datum behavior.
    pub z_index: f64,
    pub blocks: Vec<DayviewBlock>,
    pub points: Vec<DayviewPoint>,
}

/// One block (closed or ongoing) on a field row. Blocks may extend beyond
/// the day's window; [`compute_dayview`] clips them to the visible domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewBlock {
    pub start_ms: i64,
    pub end_ms: i64,
    pub fill: BlockFill,
    pub state_label: Option<String>,
}

/// Fill mode for a block. Multi-state values (arrays) render as diagonal
/// stripes — one stripe per sub-state, colored by its hue. Mirrors the
/// original `fieldSvgBlocks.ts` lines 105–177.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockFill {
    /// Single solid hex color (e.g. `#a1b2c3`).
    Single(String),
    /// Multi-state — each entry is a hex color for one stripe (in order).
    MultiState(Vec<String>),
}

/// A point-in-time marker (occur dot). `fg_color` is the state's color,
/// `bg_color` is the underlying ongoing-state color (per `da-k5z` rule).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewPoint {
    pub time_ms: i64,
    pub fg_color: String,
    pub bg_color: String,
    pub state_label: Option<String>,
}

/// Top-level dayview knobs. Per-day data is in `days`. The `now_ms` field
/// drives the yellow dashed now-line which appears only on the day whose
/// window contains `now_ms`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewConfig {
    pub days: Vec<DayviewDayInput>,
    pub now_ms: i64,
    /// Total SVG width in pixels (including margins + label area).
    pub width: f64,
    /// Total SVG height in pixels. None → auto from `day_height + n_days`
    /// in horizontal mode; defaults to a reasonable time-axis extent in
    /// vertical mode.
    pub height: Option<f64>,
    /// Per-day cross-axis extent in pixels (the size perpendicular to
    /// time). Horizontal mode: per-day band height. Vertical mode: per-day
    /// column width. Default 100.
    pub day_height: f64,
    /// Pixel gap between adjacent days along the day-stacking axis.
    /// Default 15 (matches datum).
    pub interday_margin: f64,
    /// Pixel size of the time-axis strip (perpendicular to time). Horizontal
    /// mode: height of bottom strip. Vertical mode: width of left strip.
    /// Default 15.
    pub time_axis_height: f64,
    /// Outer margin around the plot. Default 2.
    pub margin: f64,
    /// Optional fixed date-label strip size on the side adjacent to the
    /// time-axis-perpendicular edge. Horizontal: width on the left.
    /// Vertical: height on top. When None, derived from `font_size * 4`
    /// (matches datum).
    pub label_width: Option<f64>,
    /// Render orientation. Default [`Orientation::Horizontal`].
    pub orientation: Orientation,
    /// Pre-computed axis ticks. When `None`, [`compute_dayview`] auto-fills
    /// 25 hourly ticks across the data area (legacy 24h behavior). When
    /// `Some`, the layout uses the supplied vector verbatim — this is how
    /// `nowview` injects its sub-hour relative-minute ticks while reusing
    /// the dayview render path (per bead da-6jy).
    pub axis_ticks_override: Option<Vec<DayviewAxisTick>>,
}

impl Default for DayviewConfig {
    fn default() -> Self {
        Self {
            days: Vec::new(),
            now_ms: 0,
            width: 2000.0,
            height: None,
            day_height: 100.0,
            interday_margin: 15.0,
            time_axis_height: 15.0,
            margin: 2.0,
            label_width: None,
            orientation: Orientation::default(),
            axis_ticks_override: None,
        }
    }
}

/// Computed pixel-space layout. Coords are in the orientation-neutral
/// `(time_pos, cross_pos)` space; the renderer maps to SVG `(x, y)` per
/// `orientation`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewLayout {
    pub orientation: Orientation,
    pub width: f64,
    pub height: f64,
    pub margin: f64,
    /// Resolved date-label strip size (in the dimension perpendicular to
    /// the day-stack axis).
    pub label_width: f64,
    pub day_height: f64,
    pub interday_margin: f64,
    pub time_axis_height: f64,
    /// Font size used for the date labels (after 3→2→1 shrink).
    pub label_font_size: f64,
    /// Number of lines actually shown in the date label (1, 2, or 3).
    pub label_lines_shown: usize,
    /// Inner data-area rect (relative to the SVG origin). The full plot
    /// area, including label/axis strips.
    pub plot: Rect,
    /// Total pixel extent along the time axis (across the data area).
    pub time_axis_extent: f64,
    pub days: Vec<RenderedDay>,
    /// Hourly axis ticks shared across every day, in time-axis pixels
    /// (relative to the plot's data-area origin on the time axis).
    pub axis_ticks: Vec<DayviewAxisTick>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One day's geometry within the layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedDay {
    /// Truncated (when shrinking) date labels actually shown.
    pub label_lines: Vec<String>,
    /// Offset of this day along the day-stacking (cross) axis, in pixels,
    /// relative to the data-area origin.
    pub day_offset: f64,
    pub has_error: bool,
    pub fields: Vec<RenderedField>,
    /// Time-axis pixel position of the now-line within the data area. Some
    /// only on the day whose window contains `now_ms`.
    pub now_time_pos: Option<f64>,
    /// Far end of the now-line along the day-stack/cross axis, relative to
    /// the data-area origin. For the last day this extends through the
    /// time-axis tick strip; otherwise stops at the day's far edge.
    pub now_extent_far: f64,
}

/// One field's positioned blocks + points within a day.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedField {
    pub field: String,
    /// Cross-axis offset within the day's band, in pixels.
    pub cross_offset: f64,
    /// Cross-axis extent in pixels.
    pub cross_extent: f64,
    pub z_index: f64,
    pub blocks: Vec<RenderedBlock>,
    pub points: Vec<RenderedPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedBlock {
    /// Time-axis offset of the block (clipped to the day window), in pixels.
    pub time_offset: f64,
    /// Time-axis pixel extent (block width along time).
    pub time_extent: f64,
    pub fill: BlockFill,
    pub state_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedPoint {
    /// Time-axis pixel position.
    pub time_pos: f64,
    /// Cross-axis pixel position.
    pub cross_pos: f64,
    pub r: f64,
    pub fg_color: String,
    pub bg_color: String,
    pub state_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewAxisTick {
    /// Time-axis pixel position.
    pub time_pos: f64,
    pub hour: u8,
    pub label: String,
    /// Major ticks render with a brighter gridline / are eligible for axis
    /// labels. The hourly default marks every 3rd hour as major; sub-hour
    /// callers (e.g. nowview's relative-minute ticks) supply their own.
    #[serde(default)]
    pub is_major: bool,
}

/// Minimum font size below which we drop a label line.
const MIN_LABEL_FONT: f64 = 7.0;
const MAX_LABEL_FONT: f64 = 20.0;
/// Padding subtracted from `day_height / n_lines` when picking font size
/// (matches datum's `Math.min(dayHeight / nLabels - 6, 20)`).
const LABEL_FONT_PAD: f64 = 6.0;

/// Default time-axis extent (px) when caller doesn't pass `height` in
/// vertical mode (time runs along Y). Roughly matches the default
/// horizontal-mode density (24 hours over ~2000px → ~83 px/hour) scaled
/// to a portrait aspect.
const DEFAULT_VERTICAL_TIME_EXTENT: f64 = 1200.0;

/// Compute the dayview layout.
pub fn compute_dayview(config: &DayviewConfig) -> DayviewLayout {
    let n_days = config.days.len().max(1);

    // Pick label font size by trying 3 → 2 → 1 lines, taking the largest
    // that yields a usable font size.
    let mut label_lines_shown = 1usize;
    let mut label_font_size = MIN_LABEL_FONT;
    for n_lines in (1..=3).rev() {
        let fs = (config.day_height / n_lines as f64 - LABEL_FONT_PAD).min(MAX_LABEL_FONT);
        if fs >= MIN_LABEL_FONT {
            label_lines_shown = n_lines;
            label_font_size = fs;
            break;
        }
    }

    let label_width = config
        .label_width
        .unwrap_or_else(|| label_font_size * 4.0);

    let data_rows_extent =
        n_days as f64 * config.day_height + (n_days - 1) as f64 * config.interday_margin;

    // Resolve total SVG dimensions + plot rect + time-axis extent based on
    // orientation. The layout always lays out `(time, cross)` logically;
    // the renderer maps to SVG (x, y).
    let (width, height, plot, time_axis_extent) = match config.orientation {
        Orientation::Horizontal => {
            let plot_width = (config.width - 2.0 * config.margin).max(0.0);
            let time_axis_extent = (plot_width - label_width).max(0.0);
            let plot_height = data_rows_extent + config.time_axis_height;
            let height = plot_height + 2.0 * config.margin;
            let plot = Rect {
                x: config.margin,
                y: config.margin,
                width: plot_width,
                height: plot_height,
            };
            (config.width, height, plot, time_axis_extent)
        }
        Orientation::Vertical => {
            // Days stack along x; time runs along y. The data area sits
            // below the date-label strip and to the right of the time-axis
            // strip.
            let plot_width = (config.width - 2.0 * config.margin).max(0.0);
            // Day-stack extent is determined by data_rows_extent; the
            // configured `width` is informational and may not match (the
            // renderer pads or letterboxes if so).
            let _ = plot_width;
            let total_height_resolved = config
                .height
                .unwrap_or(DEFAULT_VERTICAL_TIME_EXTENT + label_width + 2.0 * config.margin);
            let plot_height = (total_height_resolved - 2.0 * config.margin).max(0.0);
            let time_axis_extent = (plot_height - label_width).max(0.0);
            // Recompute total width from the day-stack data plus axis strip.
            let resolved_plot_width = config.time_axis_height + data_rows_extent;
            let width = resolved_plot_width + 2.0 * config.margin;
            let plot = Rect {
                x: config.margin,
                y: config.margin,
                width: resolved_plot_width,
                height: plot_height,
            };
            (width, total_height_resolved, plot, time_axis_extent)
        }
    };

    // Axis ticks: caller may supply pre-computed ticks (used by nowview
    // for sub-hour relative-minute ticks). Otherwise hourly defaults across
    // `time_axis_extent` — same positions for every day.
    let axis_ticks = config
        .axis_ticks_override
        .clone()
        .unwrap_or_else(|| hourly_axis_ticks(time_axis_extent));

    let mut days: Vec<RenderedDay> = Vec::with_capacity(config.days.len());

    for (i, day) in config.days.iter().enumerate() {
        let day_offset = i as f64 * (config.day_height + config.interday_margin);

        // Truncate label_lines to `label_lines_shown` (preserve top-down order).
        let label_lines: Vec<String> = day
            .label_lines
            .iter()
            .take(label_lines_shown)
            .cloned()
            .collect();

        let span_ms = (day.end_ms - day.start_ms).max(1) as f64;
        let time_to_pos = |t_ms: i64| -> f64 {
            let dt = (t_ms - day.start_ms) as f64;
            (dt / span_ms) * time_axis_extent
        };

        // Sort fields by z_index ascending (low first → renders below).
        let mut field_order: Vec<&DayviewFieldInput> = day.fields.iter().collect();
        field_order.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap_or(std::cmp::Ordering::Equal));

        let mut fields: Vec<RenderedField> = Vec::with_capacity(field_order.len());

        for f in field_order {
            // Skip empty fields per spec ("Empty fields are SKIPPED").
            if f.blocks.is_empty() && f.points.is_empty() {
                continue;
            }

            let cross_offset = f.y_frac * config.day_height;
            // Match original's `Math.ceil(pHeight * height)`.
            let cross_extent = (f.height_frac * config.day_height).ceil();

            let mut blocks: Vec<RenderedBlock> = Vec::with_capacity(f.blocks.len());
            for b in &f.blocks {
                // Clip to the day's window.
                let s = b.start_ms.max(day.start_ms);
                let e = b.end_ms.min(day.end_ms);
                if e <= s {
                    continue;
                }
                let t_off = time_to_pos(s);
                let t_ext = (time_to_pos(e) - t_off).max(0.5);
                blocks.push(RenderedBlock {
                    time_offset: t_off,
                    time_extent: t_ext,
                    fill: b.fill.clone(),
                    state_label: b.state_label.clone(),
                });
            }

            let mut points: Vec<RenderedPoint> = Vec::with_capacity(f.points.len());
            // Heuristic point radius: same intent as datum's
            // `Math.min(five_minutes_px, height/2, width/4, 10)`.
            let five_min_ms = 5.0 * 60_000.0;
            let five_min_px = (five_min_ms / span_ms) * time_axis_extent;
            let r = five_min_px
                .min(cross_extent / 2.0)
                .min(time_axis_extent / 4.0)
                .min(10.0)
                .max(1.0);

            for p in &f.points {
                if p.time_ms < day.start_ms || p.time_ms > day.end_ms {
                    continue;
                }
                let t = time_to_pos(p.time_ms);
                let c = cross_offset + cross_extent / 2.0;
                points.push(RenderedPoint {
                    time_pos: t,
                    cross_pos: c,
                    r,
                    fg_color: p.fg_color.clone(),
                    bg_color: p.bg_color.clone(),
                    state_label: p.state_label.clone(),
                });
            }

            fields.push(RenderedField {
                field: f.field.clone(),
                cross_offset,
                cross_extent,
                z_index: f.z_index,
                blocks,
                points,
            });
        }

        // Now-line: only on the day whose window contains now_ms.
        let now_time_pos = if config.now_ms >= day.start_ms && config.now_ms <= day.end_ms {
            Some(time_to_pos(config.now_ms))
        } else {
            None
        };
        // Last day extends now-line through the axis; otherwise stops at
        // day's far edge along the cross/day-stack axis.
        let is_last_day = i + 1 == config.days.len();
        let now_extent_far = if is_last_day {
            day_offset + config.day_height + config.time_axis_height
        } else {
            day_offset + config.day_height
        };

        days.push(RenderedDay {
            label_lines,
            day_offset,
            has_error: day.has_error,
            fields,
            now_time_pos,
            now_extent_far,
        });
    }

    DayviewLayout {
        orientation: config.orientation,
        width,
        height,
        margin: config.margin,
        label_width,
        day_height: config.day_height,
        interday_margin: config.interday_margin,
        time_axis_height: config.time_axis_height,
        label_font_size,
        label_lines_shown,
        plot,
        time_axis_extent,
        days,
        axis_ticks,
    }
}

/// Build the 25 hourly tick positions across the data-area time-axis
/// extent. Uses integer-hour boundaries 0..24 mapped proportionally —
/// independent of each day's exact UTC ms range so all days align (the
/// shared axis assumption matches the original datum).
fn hourly_axis_ticks(time_axis_extent: f64) -> Vec<DayviewAxisTick> {
    let mut ticks = Vec::with_capacity(25);
    for h in 0..=24 {
        let frac = h as f64 / 24.0;
        let time_pos = frac * time_axis_extent;
        let hour = (h % 24) as u8;
        ticks.push(DayviewAxisTick {
            time_pos,
            hour,
            label: format!("{:02}", hour),
            is_major: hour.is_multiple_of(3),
        });
    }
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY_START: i64 = 1_745_712_000_000; // 2025-04-27T00:00:00Z
    const HOUR: i64 = 60 * 60 * 1000;

    fn empty_day(label: &[&str]) -> DayviewDayInput {
        DayviewDayInput {
            label_lines: label.iter().map(|s| s.to_string()).collect(),
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            has_error: false,
            fields: vec![],
        }
    }

    /// Helper: build a horizontal-orientation config (datum's original
    /// layout: time runs L→R, days stack top-to-bottom) for the legacy
    /// tests.
    fn horizontal(cfg: DayviewConfig) -> DayviewConfig {
        DayviewConfig { orientation: Orientation::Horizontal, ..cfg }
    }

    #[test]
    fn default_orientation_is_horizontal() {
        let cfg = DayviewConfig::default();
        assert_eq!(cfg.orientation, Orientation::Horizontal);
    }

    #[test]
    fn label_font_picks_three_lines_when_fits() {
        let cfg = horizontal(DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        // 100/3 - 6 = ~27.3, capped at 20 → 20.
        assert_eq!(layout.label_lines_shown, 3);
        assert!((layout.label_font_size - 20.0).abs() < 1e-6);
        assert_eq!(layout.days[0].label_lines.len(), 3);
    }

    #[test]
    fn label_font_shrinks_to_two_lines_then_one() {
        let cfg = horizontal(DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 18.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert_eq!(layout.label_lines_shown, 1);
        assert!((layout.label_font_size - 12.0).abs() < 1e-6);
        assert_eq!(layout.days[0].label_lines.len(), 1);
        assert_eq!(layout.days[0].label_lines[0], "Mon");
    }

    #[test]
    fn label_font_shrinks_to_two_lines() {
        let cfg = horizontal(DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 30.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert_eq!(layout.label_lines_shown, 2);
        assert!((layout.label_font_size - 9.0).abs() < 1e-6);
    }

    #[test]
    fn n_day_stack_height_includes_axis_and_margins() {
        let day = empty_day(&["Mon", "Apr 27", "2026"]);
        let cfg = horizontal(DayviewConfig {
            days: vec![day.clone(), day.clone(), day.clone()],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            interday_margin: 10.0,
            time_axis_height: 15.0,
            margin: 2.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        // 3*100 + 2*10 + 15 + 2*2 = 300 + 20 + 15 + 4 = 339.
        assert!((layout.height - 339.0).abs() < 1e-6, "height was {}", layout.height);
        assert_eq!(layout.days.len(), 3);
        assert!((layout.days[0].day_offset - 0.0).abs() < 1e-6);
        assert!((layout.days[1].day_offset - 110.0).abs() < 1e-6);
        assert!((layout.days[2].day_offset - 220.0).abs() < 1e-6);
    }

    #[test]
    fn now_line_only_on_today_row() {
        let day1 = DayviewDayInput {
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            has_error: false,
            fields: vec![],
        };
        let day2 = DayviewDayInput {
            label_lines: vec!["Tue".into()],
            start_ms: DAY_START + 24 * HOUR,
            end_ms: DAY_START + 48 * HOUR,
            has_error: false,
            fields: vec![],
        };
        let now_ms = DAY_START + 24 * HOUR + 12 * HOUR;
        let cfg = horizontal(DayviewConfig {
            days: vec![day1, day2],
            now_ms,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert!(layout.days[0].now_time_pos.is_none(), "day 1 should NOT have now");
        assert!(layout.days[1].now_time_pos.is_some(), "day 2 should have now");
    }

    #[test]
    fn now_line_extends_through_axis_on_last_day_only() {
        let day1 = DayviewDayInput {
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            has_error: false,
            fields: vec![],
        };
        let day2 = DayviewDayInput {
            label_lines: vec!["Tue".into()],
            start_ms: DAY_START + 24 * HOUR,
            end_ms: DAY_START + 48 * HOUR,
            has_error: false,
            fields: vec![],
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day1, day2],
            now_ms: DAY_START + 24 * HOUR + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            interday_margin: 15.0,
            time_axis_height: 15.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        // Last day: now_extent_far = 115 + 100 + 15 = 230.
        let last = &layout.days[1];
        assert!((last.now_extent_far - 230.0).abs() < 1e-6, "got {}", last.now_extent_far);
    }

    #[test]
    fn empty_fields_are_skipped() {
        let day = DayviewDayInput {
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            has_error: false,
            fields: vec![
                DayviewFieldInput {
                    field: "empty".into(),
                    y_frac: 0.1,
                    height_frac: 0.1,
                    z_index: 0.1,
                    blocks: vec![],
                    points: vec![],
                },
                DayviewFieldInput {
                    field: "has_data".into(),
                    y_frac: 0.5,
                    height_frac: 0.1,
                    z_index: 0.5,
                    blocks: vec![],
                    points: vec![DayviewPoint {
                        time_ms: DAY_START + 6 * HOUR,
                        fg_color: "#ff0000".into(),
                        bg_color: "#000000".into(),
                        state_label: None,
                    }],
                },
            ],
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        let names: Vec<&str> = layout.days[0]
            .fields
            .iter()
            .map(|f| f.field.as_str())
            .collect();
        assert_eq!(names, vec!["has_data"]);
    }

    #[test]
    fn fields_render_in_z_index_order_low_to_high() {
        let mk_field = |name: &str, z: f64| DayviewFieldInput {
            field: name.into(),
            y_frac: 0.0,
            height_frac: 0.1,
            z_index: z,
            blocks: vec![DayviewBlock {
                start_ms: DAY_START + 1 * HOUR,
                end_ms: DAY_START + 2 * HOUR,
                fill: BlockFill::Single("#abcdef".into()),
                state_label: None,
            }],
            points: vec![],
        };
        let day = DayviewDayInput {
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            has_error: false,
            fields: vec![mk_field("c", 0.9), mk_field("a", 0.1), mk_field("b", 0.5)],
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        let order: Vec<&str> = layout.days[0]
            .fields
            .iter()
            .map(|f| f.field.as_str())
            .collect();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn blocks_clip_to_day_window() {
        let day = DayviewDayInput {
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            has_error: false,
            fields: vec![DayviewFieldInput {
                field: "long".into(),
                y_frac: 0.0,
                height_frac: 0.1,
                z_index: 0.0,
                blocks: vec![DayviewBlock {
                    start_ms: DAY_START - HOUR,
                    end_ms: DAY_START + 25 * HOUR,
                    fill: BlockFill::Single("#abcdef".into()),
                    state_label: None,
                }],
                points: vec![],
            }],
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            label_width: Some(0.0),
            margin: 0.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        let block = &layout.days[0].fields[0].blocks[0];
        // Should start at time_offset=0 (clipped to day start).
        assert!(block.time_offset.abs() < 1e-6, "time_offset was {}", block.time_offset);
        // And span the full data width (800 since label_width=0, margin=0).
        assert!((block.time_extent - 800.0).abs() < 1.0, "time_extent was {}", block.time_extent);
    }

    #[test]
    fn axis_ticks_emit_25_hourly_positions() {
        let cfg = horizontal(DayviewConfig {
            days: vec![empty_day(&["Mon"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            label_width: Some(0.0),
            margin: 0.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert_eq!(layout.axis_ticks.len(), 25);
        assert_eq!(layout.axis_ticks[0].hour, 0);
        assert_eq!(layout.axis_ticks[12].hour, 12);
        // 12:00 → time_pos = 800 * 12/24 = 400.
        assert!((layout.axis_ticks[12].time_pos - 400.0).abs() < 1e-6);
        assert_eq!(layout.axis_ticks[24].hour, 0);
    }

    #[test]
    fn multi_state_block_preserves_stripe_colors() {
        let day = DayviewDayInput {
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            has_error: false,
            fields: vec![DayviewFieldInput {
                field: "mood".into(),
                y_frac: 0.0,
                height_frac: 0.1,
                z_index: 0.0,
                blocks: vec![DayviewBlock {
                    start_ms: DAY_START + 6 * HOUR,
                    end_ms: DAY_START + 7 * HOUR,
                    fill: BlockFill::MultiState(vec![
                        "#ff0000".into(),
                        "#00ff00".into(),
                        "#0000ff".into(),
                    ]),
                    state_label: Some("happy,calm,focused".into()),
                }],
                points: vec![],
            }],
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        let block = &layout.days[0].fields[0].blocks[0];
        match &block.fill {
            BlockFill::MultiState(colors) => {
                assert_eq!(colors, &vec![
                    "#ff0000".to_string(),
                    "#00ff00".to_string(),
                    "#0000ff".to_string(),
                ]);
            }
            _ => panic!("expected MultiState fill"),
        }
    }

    #[test]
    fn has_error_propagates_to_rendered_day() {
        let mut day = empty_day(&["Mon", "Apr 27", "2026"]);
        day.has_error = true;
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert!(layout.days[0].has_error);
    }

    #[test]
    fn vertical_orientation_propagates_to_layout() {
        let cfg = DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            day_height: 100.0,
            orientation: Orientation::Vertical,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        assert_eq!(layout.orientation, Orientation::Vertical);
    }

    #[test]
    fn vertical_total_width_derives_from_day_stack() {
        // 3 days × 100 + 2 × 15 (interday) + 15 (time-axis strip) + 2 × 2 (margin)
        // = 300 + 30 + 15 + 4 = 349.
        let day = empty_day(&["Mon"]);
        let cfg = DayviewConfig {
            days: vec![day.clone(), day.clone(), day.clone()],
            now_ms: DAY_START + 12 * HOUR,
            day_height: 100.0,
            interday_margin: 15.0,
            time_axis_height: 15.0,
            margin: 2.0,
            height: Some(800.0),
            orientation: Orientation::Vertical,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        assert!((layout.width - 349.0).abs() < 1e-6, "width was {}", layout.width);
        // Total height is the explicit `height` config.
        assert!((layout.height - 800.0).abs() < 1e-6, "height was {}", layout.height);
        // Days stack on the cross axis in 100+15 increments.
        assert!((layout.days[0].day_offset - 0.0).abs() < 1e-6);
        assert!((layout.days[1].day_offset - 115.0).abs() < 1e-6);
        assert!((layout.days[2].day_offset - 230.0).abs() < 1e-6);
    }

    #[test]
    fn vertical_time_axis_extent_is_data_height() {
        // height 800 - 2*margin(2) - label_width(label_font*4 ~=80) = 716.
        let cfg = DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            day_height: 100.0,
            margin: 2.0,
            height: Some(800.0),
            orientation: Orientation::Vertical,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let expected = 800.0 - 4.0 - layout.label_width;
        assert!(
            (layout.time_axis_extent - expected).abs() < 1e-6,
            "time_axis_extent was {}, expected {}",
            layout.time_axis_extent, expected
        );
        // Axis ticks: 12:00 → time_pos = expected * 0.5.
        assert!((layout.axis_ticks[12].time_pos - expected / 2.0).abs() < 1e-6);
    }

    #[test]
    fn vertical_blocks_clip_to_day_window() {
        let day = DayviewDayInput {
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            has_error: false,
            fields: vec![DayviewFieldInput {
                field: "long".into(),
                y_frac: 0.0,
                height_frac: 0.1,
                z_index: 0.0,
                blocks: vec![DayviewBlock {
                    start_ms: DAY_START - HOUR,
                    end_ms: DAY_START + 25 * HOUR,
                    fill: BlockFill::Single("#abcdef".into()),
                    state_label: None,
                }],
                points: vec![],
            }],
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            day_height: 100.0,
            margin: 0.0,
            label_width: Some(0.0),
            time_axis_height: 0.0,
            height: Some(800.0),
            orientation: Orientation::Vertical,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let block = &layout.days[0].fields[0].blocks[0];
        assert!(block.time_offset.abs() < 1e-6, "time_offset was {}", block.time_offset);
        assert!((block.time_extent - 800.0).abs() < 1.0, "time_extent was {}", block.time_extent);
    }
}
