#![doc = include_str!("../README.md")]

//! Pure layout for daydumb's dayview.
//!
//! `layout_day` takes a slice of [`LayoutEntry`] (simple POD) plus a
//! [`LayoutConfig`] and computes a [`Layout`]: per-field rows with
//! pixel-positioned blocks, plus hourly axis ticks. No I/O, no datum-core.
//!
//! Renderers (SVG today, D3/Svelte tomorrow via WASM) consume the
//! `Layout` struct without re-doing math.

use serde::{Deserialize, Serialize};

pub mod dayview;
pub mod dayview_svg;
pub mod nowview;
pub mod nowview_svg;
pub mod nowview_terminal;
pub mod svg;

/// One timed entry to lay out. Times are epoch-millis (UTC); the caller is
/// responsible for any timezone conversion when picking the day window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutEntry {
    pub field: String,
    pub start_ms: i64,
    /// Optional end. Point-in-time entries leave this `None`; they render as
    /// a thin tick.
    pub end_ms: Option<i64>,
    /// Optional state label (used as block fill / annotation).
    pub state: Option<String>,
}

/// Layout knobs. The day window (`day_start_ms`..`day_end_ms`) is the time
/// domain mapped into pixel x-range `[label_width, width]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    pub day_start_ms: i64,
    pub day_end_ms: i64,
    pub width: f64,
    pub height: f64,
    /// Width reserved on the left for field labels.
    pub label_width: f64,
    /// Height reserved on the bottom for the time axis.
    pub axis_height: f64,
    /// Minimum block width in pixels (so point-in-time entries are visible).
    pub min_block_px: f64,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            day_start_ms: 0,
            day_end_ms: 86_400_000,
            width: 2000.0,
            height: 200.0,
            label_width: 80.0,
            axis_height: 15.0,
            min_block_px: 1.0,
        }
    }
}

/// Computed layout. All coordinates are in pixels relative to an SVG of
/// size (`width`, `height`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub label_width: f64,
    pub axis_height: f64,
    pub day_start_ms: i64,
    pub day_end_ms: i64,
    pub fields: Vec<FieldRow>,
    pub axis_ticks: Vec<AxisTick>,
}

/// One horizontal lane for a field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldRow {
    pub field: String,
    pub y: f64,
    pub height: f64,
    pub blocks: Vec<Block>,
}

/// One time-bound block within a field row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub x: f64,
    pub width: f64,
    pub state: Option<String>,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
}

/// One hourly tick on the bottom time axis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisTick {
    pub x: f64,
    pub hour: u8,
    pub label: String,
}

/// Compute the dayview layout. Pure: no I/O, deterministic in inputs.
pub fn layout_day(entries: &[LayoutEntry], config: &LayoutConfig) -> Layout {
    // Stable, deterministic field order: first-seen.
    let mut fields: Vec<String> = Vec::new();
    for e in entries {
        if !fields.iter().any(|f| f == &e.field) {
            fields.push(e.field.clone());
        }
    }

    let plot_height = (config.height - config.axis_height).max(0.0);
    let row_height = if fields.is_empty() {
        plot_height
    } else {
        plot_height / fields.len() as f64
    };

    let plot_x_start = config.label_width;
    let plot_x_end = config.width;
    let plot_width = (plot_x_end - plot_x_start).max(0.0);
    let span_ms = (config.day_end_ms - config.day_start_ms).max(1) as f64;

    let time_to_x = |t_ms: i64| -> f64 {
        let dt = (t_ms - config.day_start_ms) as f64;
        plot_x_start + (dt / span_ms) * plot_width
    };

    let rows: Vec<FieldRow> = fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let y = i as f64 * row_height;
            let blocks: Vec<Block> = entries
                .iter()
                .filter(|e| &e.field == field)
                .map(|e| {
                    let x0 = time_to_x(e.start_ms);
                    let x1 = match e.end_ms {
                        Some(end) => time_to_x(end),
                        None => x0,
                    };
                    let raw_width = (x1 - x0).max(0.0);
                    let width = if e.end_ms.is_none() {
                        config.min_block_px.max(raw_width)
                    } else {
                        raw_width.max(config.min_block_px)
                    };
                    Block {
                        x: x0,
                        width,
                        state: e.state.clone(),
                        start_ms: e.start_ms,
                        end_ms: e.end_ms,
                    }
                })
                .collect();
            FieldRow {
                field: field.clone(),
                y,
                height: row_height,
                blocks,
            }
        })
        .collect();

    let axis_ticks = axis_ticks_hourly(config.day_start_ms, config.day_end_ms, plot_x_start, plot_x_end);

    Layout {
        width: config.width,
        height: config.height,
        label_width: config.label_width,
        axis_height: config.axis_height,
        day_start_ms: config.day_start_ms,
        day_end_ms: config.day_end_ms,
        fields: rows,
        axis_ticks,
    }
}

/// Hourly ticks across the time axis. One per integer-hour boundary
/// (midnight..midnight inclusive — 25 ticks for a full day window).
pub fn axis_ticks_hourly(
    day_start_ms: i64,
    day_end_ms: i64,
    x_start: f64,
    x_end: f64,
) -> Vec<AxisTick> {
    const HOUR_MS: i64 = 60 * 60 * 1000;
    let span_ms = (day_end_ms - day_start_ms).max(1) as f64;
    let plot_w = (x_end - x_start).max(0.0);

    let first_hour = (day_start_ms + HOUR_MS - 1).div_euclid(HOUR_MS);
    let last_hour = day_end_ms.div_euclid(HOUR_MS);

    let mut ticks = Vec::new();
    let mut h = first_hour;
    while h <= last_hour {
        let t = h * HOUR_MS;
        let x = x_start + ((t - day_start_ms) as f64 / span_ms) * plot_w;
        let hour = ((h.rem_euclid(24)) as u8).min(23);
        ticks.push(AxisTick {
            x,
            hour,
            label: format!("{:02}", hour),
        });
        h += 1;
    }
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    // Midnight-aligned epoch ms (1700006400000 = 2023-11-15T00:00:00Z).
    const DAY_START: i64 = 1_700_006_400_000;
    const HOUR: i64 = 60 * 60 * 1000;

    fn cfg() -> LayoutConfig {
        LayoutConfig {
            day_start_ms: DAY_START,
            day_end_ms: DAY_START + 24 * HOUR,
            width: 2400.0,
            height: 200.0,
            label_width: 0.0, // simplifies pixel math in assertions
            axis_height: 0.0,
            min_block_px: 0.0,
        }
    }

    #[test]
    fn layout_day_positions_blocks_by_time() {
        // 1h block starting at 06:00 in a 24h day mapped to 2400 px wide.
        // Each hour = 100 px; 06:00 → x=600, width=100.
        let entries = vec![LayoutEntry {
            field: "workout".into(),
            start_ms: DAY_START + 6 * HOUR,
            end_ms: Some(DAY_START + 7 * HOUR),
            state: None,
        }];
        let layout = layout_day(&entries, &cfg());

        assert_eq!(layout.fields.len(), 1);
        let row = &layout.fields[0];
        assert_eq!(row.blocks.len(), 1);
        let block = &row.blocks[0];

        assert!((block.x - 600.0).abs() < 1e-6, "x was {}", block.x);
        assert!((block.width - 100.0).abs() < 1e-6, "width was {}", block.width);
    }

    #[test]
    fn layout_day_stacks_fields_vertically() {
        // Three fields → three rows, each 1/3 of plot height; first-seen order.
        let entries = vec![
            LayoutEntry {
                field: "mood".into(),
                start_ms: DAY_START + 1 * HOUR,
                end_ms: None,
                state: Some("happy".into()),
            },
            LayoutEntry {
                field: "coffee".into(),
                start_ms: DAY_START + 2 * HOUR,
                end_ms: None,
                state: None,
            },
            LayoutEntry {
                field: "workout".into(),
                start_ms: DAY_START + 8 * HOUR,
                end_ms: Some(DAY_START + 9 * HOUR),
                state: None,
            },
            // Second mood entry must land in the existing mood row.
            LayoutEntry {
                field: "mood".into(),
                start_ms: DAY_START + 10 * HOUR,
                end_ms: None,
                state: Some("focused".into()),
            },
        ];
        let layout = layout_day(&entries, &cfg());

        assert_eq!(layout.fields.len(), 3, "deduped to 3 distinct fields");
        let names: Vec<&str> = layout.fields.iter().map(|r| r.field.as_str()).collect();
        assert_eq!(names, vec!["mood", "coffee", "workout"], "first-seen order");

        let row_h = 200.0 / 3.0;
        for (i, row) in layout.fields.iter().enumerate() {
            assert!((row.y - i as f64 * row_h).abs() < 1e-6);
            assert!((row.height - row_h).abs() < 1e-6);
        }

        // mood row holds both mood entries.
        assert_eq!(layout.fields[0].blocks.len(), 2);
        assert_eq!(layout.fields[1].blocks.len(), 1);
        assert_eq!(layout.fields[2].blocks.len(), 1);
    }

    #[test]
    fn axis_ticks_hourly_emits_one_per_hour_boundary() {
        let ticks = axis_ticks_hourly(DAY_START, DAY_START + 24 * HOUR, 0.0, 2400.0);

        // Midnight..midnight (next day) inclusive → 25 ticks.
        assert_eq!(ticks.len(), 25);
        assert_eq!(ticks[0].hour, 0);
        assert_eq!(ticks[0].label, "00");
        assert_eq!(ticks[12].hour, 12);
        assert_eq!(ticks[12].label, "12");
        // x is proportional: noon → x=1200 in a 2400px plot.
        assert!((ticks[12].x - 1200.0).abs() < 1e-6);
        // Last tick wraps to 00 (next-day midnight).
        assert_eq!(ticks[24].hour, 0);
    }
}
