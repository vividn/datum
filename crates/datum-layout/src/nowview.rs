//! Pure layout for `datum nowview` — datum's "current state + recent
//! history" timeline.
//!
//! Reference: `/tmp/datum-survey/src/dayview/nowview.ts` (the TypeScript
//! original). This module computes the panel split, axis ticks, current-time
//! marker geometry, and per-field band positions; the renderer (SVG today,
//! WASM/D3 tomorrow) consumes the resulting [`NowviewLayout`] without
//! re-doing math.
//!
//! No I/O, no datum-core, no system deps — compiles cleanly to
//! `wasm32-unknown-unknown` so the SvelteKit PWA can render the same view
//! client-side.

use serde::{Deserialize, Serialize};

/// One block (closed or ongoing) inside a field row.
///
/// `start_ms` / `end_ms` are absolute epoch-ms in UTC. The caller is
/// responsible for clipping to the visible window if desired (the layout
/// will clip to the data-area at render time anyway).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewBlock {
    pub start_ms: i64,
    pub end_ms: i64,
    /// Hex fill color (e.g. `#a1b2c3`). The caller resolves this from the
    /// state via the project's color algorithm — the layout is color-blind.
    pub color: String,
    /// Opaque label for hover / accessibility. Optional.
    pub state_label: Option<String>,
}

/// One point-in-time marker (occur dot) inside a field row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewPoint {
    pub time_ms: i64,
    /// Foreground fill — typically the state's color.
    pub fg_color: String,
    /// Background ring/halo color — typically the underlying ongoing-state
    /// color at this instant (per the `da-k5z` fg/bg rule).
    pub bg_color: String,
    pub state_label: Option<String>,
}

/// One field's data for the nowview window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewField {
    pub field: String,
    pub blocks: Vec<NowviewBlock>,
    pub points: Vec<NowviewPoint>,
}

/// Inputs to [`compute_nowview`]. The datum-CLI command resolves all of
/// these from clap flags + the local clock; the layout itself is pure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewConfig {
    /// "Now" — UTC epoch ms. End of the visible window.
    pub end_ms: i64,
    /// Total visible history in minutes (left panel). 0 → no history panel.
    pub history_minutes: f64,
    /// Width in time units (minutes) of the right "now" panel.
    pub now_width_minutes: f64,
    /// SVG width in pixels.
    pub width: f64,
    /// SVG height in pixels.
    pub height: f64,
    /// Margin around the plot (datum default: 2).
    pub margin: f64,
    /// Pixel height of the time-axis strip (datum default: 15).
    pub time_axis_height: f64,
}

impl Default for NowviewConfig {
    fn default() -> Self {
        Self {
            end_ms: 0,
            history_minutes: 0.0,
            now_width_minutes: 5.0,
            width: 100.0,
            height: 300.0,
            margin: 2.0,
            time_axis_height: 15.0,
        }
    }
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
    if history_minutes <= 15.0 {
        TickConfig { major_minutes: 5.0, minor_minutes: None }
    } else if history_minutes <= 60.0 {
        TickConfig { major_minutes: 15.0, minor_minutes: Some(5.0) }
    } else if history_minutes <= 180.0 {
        TickConfig { major_minutes: 30.0, minor_minutes: Some(10.0) }
    } else {
        TickConfig { major_minutes: 60.0, minor_minutes: Some(15.0) }
    }
}

/// Computed pixel-space layout. All fields are pre-resolved so the renderer
/// just paints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewLayout {
    pub width: f64,
    pub height: f64,
    pub margin: f64,
    pub time_axis_height: f64,
    /// Plot rect (the inner area inside the margin).
    pub plot: Rect,
    /// Right "now" panel (always present).
    pub now_panel: Rect,
    /// Left history panel (only present when `history_minutes > 0`).
    pub history_panel: Option<Rect>,
    /// Per-field rendered rows. Sorted alphabetically (datum behavior).
    pub fields: Vec<FieldRow>,
    /// Major tick positions (x in plot-coords, label like `-15m`).
    pub major_ticks: Vec<TimeTick>,
    /// Minor tick positions (x in plot-coords).
    pub minor_ticks: Vec<TimeTick>,
    /// "HH:MM" current-time label (clock face is the caller's TZ choice;
    /// `compute_nowview` doesn't pick a TZ).
    pub current_time_label: String,
    /// X-coordinate (in plot space) of the boundary between history and
    /// now panels — i.e. the "now" vertical line. Equal to `now_panel.x`.
    pub now_x: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldRow {
    pub field: String,
    /// Y in plot coords. Row spans (y .. y+height) within the data area.
    pub y: f64,
    pub height: f64,
    /// Block rectangles already mapped to plot pixel coords.
    pub blocks: Vec<RenderedBlock>,
    /// Points already mapped to plot pixel coords.
    pub points: Vec<RenderedPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedBlock {
    pub x: f64,
    pub width: f64,
    pub color: String,
    pub state_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedPoint {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    pub fg_color: String,
    pub bg_color: String,
    pub state_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTick {
    pub x: f64,
    /// Minutes before "now" (positive). Label is `-{Nm}`.
    pub minutes_ago: f64,
    pub label: String,
}

/// Compute pixel-space layout for the nowview.
///
/// The layout splits the plot horizontally into [history | now] when
/// `history_minutes > 0`, otherwise just emits the now panel. Field rows
/// are sorted alphabetically and sized to evenly fill the data area.
pub fn compute_nowview(
    fields: &[NowviewField],
    config: &NowviewConfig,
) -> NowviewLayout {
    let plot_width = (config.width - config.margin * 2.0).max(0.0);
    let plot_height = (config.height - config.margin * 2.0).max(0.0);
    let data_height = (plot_height - config.time_axis_height).max(0.0);

    // Panel split: history + now totals plot_width by ratio of time-units.
    let (now_panel_w, history_panel_w) = if config.history_minutes > 0.0 {
        let total = config.history_minutes + config.now_width_minutes;
        let now_ratio = config.now_width_minutes / total;
        let now_w = plot_width * now_ratio;
        let hist_w = plot_width - now_w;
        (now_w, hist_w)
    } else {
        (plot_width, 0.0)
    };

    let plot = Rect {
        x: config.margin,
        y: config.margin,
        width: plot_width,
        height: plot_height,
    };

    // Coordinates inside the plot are 0..plot_width on x, 0..plot_height on y.
    let now_x = history_panel_w;
    let now_panel = Rect {
        x: now_x,
        y: 0.0,
        width: now_panel_w,
        height: data_height,
    };
    let history_panel = if history_panel_w > 0.0 {
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: history_panel_w,
            height: data_height,
        })
    } else {
        None
    };

    // Sort fields alphabetically — bead spec.
    let mut sorted: Vec<&NowviewField> = fields.iter().collect();
    sorted.sort_by(|a, b| a.field.cmp(&b.field));

    // Time-to-x mapping inside the history panel:
    //   end_ms - history_minutes*60_000  →  0
    //   end_ms                             →  history_panel_w
    let history_start_ms =
        config.end_ms - (config.history_minutes * 60_000.0) as i64;
    let history_span_ms = (config.history_minutes * 60_000.0).max(1.0);

    // For the now panel: a tiny time slice ending at end_ms. We map that
    // sliver to the full now-panel width so a single-instant block (start
    // = end - 1ms) stretches across the whole panel — matches datum's
    // "sliverBeforeEndUtc" trick.
    let now_panel_span_ms = 1.0_f64;

    // History x-mapping. Returns None for ms outside the history window.
    let history_time_to_x = |t_ms: i64| -> f64 {
        let dt = (t_ms - history_start_ms) as f64;
        (dt / history_span_ms) * history_panel_w
    };

    // Now-panel x-mapping. The visible window inside the now panel is
    // (end_ms-1 .. end_ms]; we treat anything ≥ end_ms-1 as "fill the
    // panel" and anything else as off-screen-left.
    let now_panel_time_to_x = |t_ms: i64| -> f64 {
        let dt = (t_ms - (config.end_ms - 1)) as f64;
        let frac = (dt / now_panel_span_ms).clamp(0.0, 1.0);
        now_x + frac * now_panel_w
    };

    let row_count = sorted.len().max(1);
    let row_height = if sorted.is_empty() {
        data_height
    } else {
        data_height / row_count as f64
    };

    let rows: Vec<FieldRow> = sorted
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let y = i as f64 * row_height;

            // History blocks: emit as rects in the history panel.
            let mut blocks: Vec<RenderedBlock> = Vec::new();
            // Points: history points and (if present at the now-instant)
            // a point in the now panel.
            let mut points: Vec<RenderedPoint> = Vec::new();

            // History panel renderings.
            if history_panel_w > 0.0 {
                for b in &field.blocks {
                    // Clip to history window.
                    let s = b.start_ms.max(history_start_ms);
                    let e = b.end_ms.min(config.end_ms);
                    if e <= s {
                        continue;
                    }
                    let x = history_time_to_x(s);
                    let w = (history_time_to_x(e) - x).max(0.5);
                    blocks.push(RenderedBlock {
                        x,
                        width: w,
                        color: b.color.clone(),
                        state_label: b.state_label.clone(),
                    });
                }
                for p in &field.points {
                    if p.time_ms < history_start_ms || p.time_ms > config.end_ms {
                        continue;
                    }
                    let cx = history_time_to_x(p.time_ms);
                    let cy = y + row_height / 2.0;
                    let r = point_radius(row_height, history_panel_w, history_minutes_to_px(history_panel_w, config.history_minutes));
                    points.push(RenderedPoint {
                        cx,
                        cy,
                        r,
                        fg_color: p.fg_color.clone(),
                        bg_color: p.bg_color.clone(),
                        state_label: p.state_label.clone(),
                    });
                }
            }

            // Now panel: paint the ongoing/instant state. We pick the
            // "current state" by finding the latest block that contains
            // end_ms-1, or the latest point at end_ms. Caller is expected
            // to populate one of these for accuracy; we render whichever
            // applies. If a block is ongoing (end_ms == config.end_ms),
            // fill the now panel with its color.
            let active_block_color: Option<&str> = field
                .blocks
                .iter()
                .filter(|b| b.start_ms <= config.end_ms && b.end_ms >= config.end_ms)
                .max_by_key(|b| b.start_ms)
                .map(|b| b.color.as_str());

            if let Some(c) = active_block_color {
                blocks.push(RenderedBlock {
                    x: now_x,
                    width: now_panel_w,
                    color: c.to_string(),
                    state_label: None,
                });
            }

            // Render any point that lands within ~now_width_minutes of end
            // as a marker in the now panel center. Datum's nowview emits
            // a "sliverBeforeEndUtc" pass that catches the most recent
            // point; we approximate that by placing any point in the
            // last 1ms at the now-panel center.
            let recent_pt_window_ms =
                ((config.now_width_minutes * 60_000.0).max(1.0)) as i64;
            for p in &field.points {
                if p.time_ms >= config.end_ms - recent_pt_window_ms
                    && p.time_ms <= config.end_ms
                {
                    let cx = now_panel_time_to_x(p.time_ms);
                    let cy = y + row_height / 2.0;
                    let r = point_radius(row_height, now_panel_w, recent_pt_window_ms as f64);
                    points.push(RenderedPoint {
                        cx,
                        cy,
                        r,
                        fg_color: p.fg_color.clone(),
                        bg_color: p.bg_color.clone(),
                        state_label: p.state_label.clone(),
                    });
                }
            }

            FieldRow {
                field: field.field.clone(),
                y,
                height: row_height,
                blocks,
                points,
            }
        })
        .collect();

    // Tick generation. Major and minor in minutes-ago units; positions are
    // mapped into history-panel x-coords. When history_panel_w == 0, no
    // ticks (the panel is absent).
    let mut major_ticks = Vec::new();
    let mut minor_ticks = Vec::new();
    if history_panel_w > 0.0 && config.history_minutes > 0.0 {
        let cfg = tick_config(config.history_minutes);
        let mut m = cfg.major_minutes;
        while m <= config.history_minutes + 1e-9 {
            let t_ms = config.end_ms - (m * 60_000.0) as i64;
            let x = history_time_to_x(t_ms);
            major_ticks.push(TimeTick {
                x,
                minutes_ago: m,
                label: format!("-{}m", m as i64),
            });
            m += cfg.major_minutes;
        }
        if let Some(minor) = cfg.minor_minutes {
            let mut m = minor;
            while m <= config.history_minutes + 1e-9 {
                if (m % cfg.major_minutes).abs() > 1e-9 {
                    let t_ms = config.end_ms - (m * 60_000.0) as i64;
                    let x = history_time_to_x(t_ms);
                    minor_ticks.push(TimeTick {
                        x,
                        minutes_ago: m,
                        label: format!("-{}m", m as i64),
                    });
                }
                m += minor;
            }
        }
    }

    NowviewLayout {
        width: config.width,
        height: config.height,
        margin: config.margin,
        time_axis_height: config.time_axis_height,
        plot,
        now_panel,
        history_panel,
        fields: rows,
        major_ticks,
        minor_ticks,
        current_time_label: String::new(), // caller fills this in
        now_x,
    }
}

/// Heuristic radius for an occur-dot marker — same intent as datum's
/// `Math.min(five_minutes, height/2, width/4, 10)`. `panel_w` is the panel
/// width in pixels and `window_ms` is the panel's time span in ms.
fn point_radius(row_height: f64, panel_w: f64, window_ms: f64) -> f64 {
    let five_min_px = if window_ms > 0.0 {
        panel_w * (5.0 * 60_000.0 / window_ms)
    } else {
        panel_w / 4.0
    };
    five_min_px
        .min(row_height / 2.0)
        .min(panel_w / 4.0)
        .min(10.0)
        .max(1.0)
}

/// Convert history-panel pixel width to ms-window for tick scaling. Helper
/// for [`point_radius`] callsites.
fn history_minutes_to_px(panel_w: f64, history_minutes: f64) -> f64 {
    if history_minutes <= 0.0 {
        panel_w
    } else {
        history_minutes * 60_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(history_min: f64, width: f64) -> NowviewConfig {
        NowviewConfig {
            end_ms: 1_700_000_000_000,
            history_minutes: history_min,
            now_width_minutes: 5.0,
            width,
            height: 300.0,
            margin: 2.0,
            time_axis_height: 15.0,
        }
    }

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
        assert_eq!(tick_config(15.0), TickConfig { major_minutes: 5.0, minor_minutes: None });
        assert_eq!(tick_config(60.0), TickConfig { major_minutes: 15.0, minor_minutes: Some(5.0) });
        assert_eq!(tick_config(180.0), TickConfig { major_minutes: 30.0, minor_minutes: Some(10.0) });
        assert_eq!(tick_config(360.0), TickConfig { major_minutes: 60.0, minor_minutes: Some(15.0) });
    }

    #[test]
    fn no_history_panel_when_history_zero() {
        let layout = compute_nowview(&[], &cfg(0.0, 100.0));
        assert!(layout.history_panel.is_none());
        // Now panel fills the plot.
        assert!((layout.now_panel.width - layout.plot.width).abs() < 1e-6);
        assert!(layout.major_ticks.is_empty());
        assert!(layout.minor_ticks.is_empty());
    }

    #[test]
    fn panel_split_proportional_to_time_ratio() {
        // 60 min history + 5 min now → now panel = 5/65 of plot width.
        let layout = compute_nowview(&[], &cfg(60.0, 650.0));
        let plot_w = layout.plot.width;
        let expected_now_w = plot_w * 5.0 / 65.0;
        assert!((layout.now_panel.width - expected_now_w).abs() < 1e-6,
            "now panel width was {}, expected {}", layout.now_panel.width, expected_now_w);
        let hist = layout.history_panel.unwrap();
        assert!((hist.width + layout.now_panel.width - plot_w).abs() < 1e-6);
        assert!((layout.now_x - hist.width).abs() < 1e-6);
    }

    #[test]
    fn fields_sorted_alphabetically() {
        let fields = vec![
            NowviewField {
                field: "workout".into(),
                blocks: vec![],
                points: vec![],
            },
            NowviewField {
                field: "coffee".into(),
                blocks: vec![],
                points: vec![],
            },
            NowviewField {
                field: "mood".into(),
                blocks: vec![],
                points: vec![],
            },
        ];
        let layout = compute_nowview(&fields, &cfg(60.0, 400.0));
        let names: Vec<&str> = layout.fields.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(names, vec!["coffee", "mood", "workout"]);
    }

    #[test]
    fn major_tick_count_matches_history_window() {
        // 60 min → major every 15 min → ticks at 15, 30, 45, 60 → 4 ticks.
        let layout = compute_nowview(&[], &cfg(60.0, 650.0));
        let labels: Vec<&str> = layout
            .major_ticks
            .iter()
            .map(|t| t.label.as_str())
            .collect();
        assert_eq!(labels, vec!["-15m", "-30m", "-45m", "-60m"]);

        // Minor ticks (5m) excluding major positions: 5, 10, 20, 25, 35, 40, 50, 55 → 8 ticks.
        assert_eq!(layout.minor_ticks.len(), 8);
    }

    #[test]
    fn ongoing_block_fills_now_panel() {
        let now_ms = 1_700_000_000_000_i64;
        let one_min = 60_000_i64;
        let fields = vec![NowviewField {
            field: "mood".into(),
            blocks: vec![NowviewBlock {
                start_ms: now_ms - 30 * one_min,
                end_ms: now_ms, // ongoing — extends to "now"
                color: "#abcdef".into(),
                state_label: Some("happy".into()),
            }],
            points: vec![],
        }];
        let mut config = cfg(60.0, 650.0);
        config.end_ms = now_ms;
        let layout = compute_nowview(&fields, &config);

        let row = &layout.fields[0];
        // Should have at least 2 blocks: one in history panel, one in now panel.
        assert!(row.blocks.len() >= 2,
            "expected history+now blocks, got {}", row.blocks.len());
        // The block at now_x should have the now-panel width.
        let now_block = row
            .blocks
            .iter()
            .find(|b| (b.x - layout.now_x).abs() < 1e-6)
            .expect("block at now_x");
        assert!((now_block.width - layout.now_panel.width).abs() < 1e-6);
        assert_eq!(now_block.color, "#abcdef");
    }

    #[test]
    fn point_within_history_window_is_rendered() {
        let now_ms = 1_700_000_000_000_i64;
        let fields = vec![NowviewField {
            field: "coffee".into(),
            blocks: vec![],
            points: vec![NowviewPoint {
                time_ms: now_ms - 30 * 60_000, // 30 min ago
                fg_color: "#ff0000".into(),
                bg_color: "#000000".into(),
                state_label: Some("1".into()),
            }],
        }];
        let mut config = cfg(60.0, 650.0);
        config.end_ms = now_ms;
        let layout = compute_nowview(&fields, &config);

        let row = &layout.fields[0];
        assert_eq!(row.points.len(), 1);
        // 30 min into a 60 min history → midpoint of history panel.
        let hist = layout.history_panel.unwrap();
        let expected_cx = hist.x + hist.width / 2.0;
        assert!((row.points[0].cx - expected_cx).abs() < 1.0,
            "cx {} expected {}", row.points[0].cx, expected_cx);
    }
}
