//! Pure-Rust SVG renderer for [`DayviewLayout`].
//!
//! Produces a self-contained SVG document string. No external CSS/JS,
//! no popover, no hover (per bead da-vp7 — the SvelteKit frontend handles
//! interactivity).
//!
//! Visual style mirrors datum's TypeScript dayview output:
//! - Black background.
//! - Hourly gridlines (40% opacity every 3rd hour, 10% otherwise).
//! - Per-day stack with date labels on the left.
//! - Per-field band rendering: solid blocks, multi-state diagonal stripes,
//!   point markers (fg + bg circles per da-k5z).
//! - Yellow dashed "now" line on today's row only.
//! - Warning icon next to date labels for days with conflicts.
//! - Bottom hourly time axis.

use std::fmt::Write;

use crate::dayview::{BlockFill, DayviewLayout};

const NOW_LINE_COLOR: &str = "#ffcc00";
const BG_COLOR: &str = "black";
const TEXT_COLOR: &str = "white";
const AXIS_COLOR: &str = "white";
/// Per-stripe width (px) in multi-state diagonal patterns. Matches the
/// original TS `stripeWidth = 8`.
const STRIPE_WIDTH_PX: f64 = 8.0;

/// Render `layout` to an SVG document string.
pub fn render_dayview_svg(layout: &DayviewLayout) -> String {
    let mut out = String::with_capacity(8192);
    let mut defs = String::new();
    let mut pattern_seq: usize = 0;

    let _ = write!(
        &mut out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = layout.width,
        h = layout.height,
    );
    let _ = write!(
        &mut out,
        r#"<rect class="bg" width="100%" height="100%" fill="{bg}"/>"#,
        bg = BG_COLOR,
    );

    // Plot group: translate inward by margin so plot coords are 0,0-origin.
    let _ = write!(
        &mut out,
        r#"<g class="plot" transform="translate({mx},{my})">"#,
        mx = layout.margin,
        my = layout.margin,
    );

    let data_rows_height = layout.days.len() as f64 * layout.day_height
        + (layout.days.len().saturating_sub(1)) as f64 * layout.interday_margin;

    // Hourly gridlines spanning the full day-stack height (datum behavior:
    // gridlines extend through every row down to the axis baseline). 40%
    // opacity every 3rd hour (every 3rd hour of UTC) — but we don't have
    // UTC mapping here, just position; mimic by using `tick.hour % 3`.
    for tick in &layout.axis_ticks {
        let opacity = if tick.hour % 3 == 0 { 0.4 } else { 0.1 };
        let _ = write!(
            &mut out,
            r#"<line class="gridline" x1="{x}" x2="{x}" y1="0" y2="{y2}" stroke="white" stroke-opacity="{op}"/>"#,
            x = tick.x,
            y2 = data_rows_height,
            op = opacity,
        );
    }

    // Per-day rendering.
    for (i, day) in layout.days.iter().enumerate() {
        let day_y = day.y_offset;

        // Date label group (left side, before the data area).
        let _ = write!(
            &mut out,
            r#"<g class="day-label" data-day="{i}" transform="translate(0,{dy})">"#,
            i = i,
            dy = day_y,
        );
        let n_lines = day.label_lines.len().max(1);
        for (j, line) in day.label_lines.iter().enumerate() {
            // Center each line vertically in its slice of the row.
            let line_y = ((j as f64 + 0.5) * layout.day_height) / n_lines as f64;
            let _ = write!(
                &mut out,
                r#"<text class="date-label" x="{lx}" y="{ly}" dy="0.35em" fill="{c}" text-anchor="end" font-size="{fs}">{txt}</text>"#,
                lx = layout.label_width - 4.0,
                ly = line_y,
                c = TEXT_COLOR,
                fs = layout.label_font_size,
                txt = escape_xml(line),
            );
        }
        // Warning icon next to the date label (right side of the label
        // column, vertically centered).
        if day.has_error {
            let icon_size = (layout.day_height * 0.4).min(20.0).max(8.0);
            let icon_x = layout.label_width - 2.0;
            let icon_y = (layout.day_height - icon_size) / 2.0;
            let _ = write!(
                &mut out,
                r#"<g class="day-error" transform="translate({x},{y})">"#,
                x = icon_x,
                y = icon_y,
            );
            push_warning_icon(&mut out, icon_size);
            out.push_str("</g>");
        }
        out.push_str("</g>");

        // Per-field bands.
        let _ = write!(
            &mut out,
            r#"<g class="day-data" data-day="{i}" transform="translate(0,{dy})">"#,
            i = i,
            dy = day_y,
        );
        for field in &day.fields {
            let _ = write!(
                &mut out,
                r#"<g class="field" data-field="{f}">"#,
                f = escape_xml(&field.field),
            );
            for b in &field.blocks {
                match &b.fill {
                    BlockFill::Single(color) => {
                        let _ = write!(
                            &mut out,
                            r#"<rect class="block" x="{x}" y="{y}" width="{w}" height="{h}" fill="{c}"/>"#,
                            x = b.x,
                            y = field.y,
                            w = b.width,
                            h = field.height,
                            c = color,
                        );
                    }
                    BlockFill::MultiState(colors) => {
                        if colors.is_empty() {
                            continue;
                        }
                        let pattern_id = format!("ds-{}", pattern_seq);
                        pattern_seq += 1;
                        push_diagonal_stripe_pattern(
                            &mut defs,
                            &pattern_id,
                            colors,
                            field.height,
                        );
                        let _ = write!(
                            &mut out,
                            r#"<rect class="block multi-state" x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{pid})"/>"#,
                            x = b.x,
                            y = field.y,
                            w = b.width,
                            h = field.height,
                            pid = pattern_id,
                        );
                    }
                }
            }
            for p in &field.points {
                // Two-circle stack: outer (bg) ring + inner (fg) dot. Mirrors
                // the da-k5z fg/bg rule.
                let outer_r = p.r;
                let inner_r = (p.r * 0.6).max(0.5);
                let _ = write!(
                    &mut out,
                    r#"<circle class="point-bg" cx="{cx}" cy="{cy}" r="{r}" fill="{c}"/>"#,
                    cx = p.cx,
                    cy = p.cy,
                    r = outer_r,
                    c = p.bg_color,
                );
                let _ = write!(
                    &mut out,
                    r#"<circle class="point-fg" cx="{cx}" cy="{cy}" r="{r}" fill="{c}"/>"#,
                    cx = p.cx,
                    cy = p.cy,
                    r = inner_r,
                    c = p.fg_color,
                );
            }
            out.push_str("</g>");
        }
        out.push_str("</g>");

        // Per-day now-line (vertical, in plot-coords). Drawn *after* the
        // bands so it sits visibly on top.
        if let Some(nx) = day.now_x {
            let _ = write!(
                &mut out,
                r#"<line class="current-time-line" x1="{x}" x2="{x}" y1="{y0}" y2="{y1}" stroke="{c}" stroke-width="2" stroke-dasharray="4,2"/>"#,
                x = nx,
                y0 = day_y,
                y1 = day.now_y_bottom,
                c = NOW_LINE_COLOR,
            );
        }
    }

    // Bottom time axis: baseline + tick labels (at the bottom of the plot,
    // *below* the data rows).
    if layout.time_axis_height > 0.0 {
        let axis_y = data_rows_height;
        let _ = write!(
            &mut out,
            r#"<line class="axis" x1="{x0}" x2="{x1}" y1="{y}" y2="{y}" stroke="{c}"/>"#,
            x0 = layout.label_width,
            x1 = layout.plot.width,
            y = axis_y,
            c = AXIS_COLOR,
        );
        if layout.time_axis_height > 6.0 {
            let label_size = (layout.time_axis_height - 6.0).max(6.0);
            // Skip the trailing 24:00 tick — datum's d3 axisBottom doesn't
            // emit it (it's the same instant as 00:00 the next day).
            let n = layout.axis_ticks.len();
            for (i, tick) in layout.axis_ticks.iter().enumerate() {
                if i + 1 == n {
                    break;
                }
                // Tick line down.
                let _ = write!(
                    &mut out,
                    r#"<line class="tick" x1="{x}" x2="{x}" y1="{y0}" y2="{y1}" stroke="{c}"/>"#,
                    x = tick.x,
                    y0 = axis_y,
                    y1 = axis_y + 4.0,
                    c = AXIS_COLOR,
                );
                let _ = write!(
                    &mut out,
                    r#"<text class="tick-label" x="{x}" y="{y}" text-anchor="middle" fill="{c}" font-size="{fs}">{label}</text>"#,
                    x = tick.x,
                    y = axis_y + label_size + 2.0,
                    c = TEXT_COLOR,
                    fs = label_size,
                    label = tick.label,
                );
            }
        }
    }

    out.push_str("</g>");

    if !defs.is_empty() {
        out.insert_str(
            after_first_tag(&out),
            &format!(r#"<defs>{}</defs>"#, defs),
        );
    }

    out.push_str("</svg>");
    out
}

/// Wrap an SVG string in a minimal HTML document — used for `--output-file
/// foo.html`.
pub fn wrap_dayview_svg_in_html(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len() + 256);
    out.push_str("<!DOCTYPE html><html><head><meta charset=\"UTF-8\"><title>datum dayview</title>");
    out.push_str("<style>body{margin:0;background:black;}</style>");
    out.push_str("</head><body>");
    out.push_str(svg);
    out.push_str("</body></html>");
    out
}

fn after_first_tag(s: &str) -> usize {
    // Inserts after `<svg ... >` so `<defs>` becomes the first child.
    s.find('>').map(|i| i + 1).unwrap_or(0)
}

/// Push the warning icon SVG into `out` at the current scale. Ports the
/// shape from `/tmp/datum-survey/src/dayview/symbols/warningIcon.ts` —
/// yellow triangle + red exclamation. Kept inline so the layout crate
/// stays leaf (no asset pipeline).
fn push_warning_icon(out: &mut String, size: f64) {
    let _ = write!(
        out,
        r##"<svg width="{s}" height="{s}" viewBox="0 0 24 24"><path d="M10.25,4.19,2.63,18a2,2,0,0,0,1.75,3H19.62a2,2,0,0,0,1.75-3L13.75,4.19A2,2,0,0,0,10.25,4.19Z" fill="#ffff00" stroke="#ff0000" stroke-width="1.5"/><line x1="12" y1="9" x2="12" y2="13" stroke="#ff0000" stroke-width="2" stroke-linecap="round"/><circle cx="12" cy="17" r="1" fill="#ff0000"/></svg>"##,
        s = size,
    );
}

/// Push a `<pattern>` into `defs` with one diagonal-stripe rect per state
/// color. Stripes are 8px wide, rotated 45°. Tile width = `n_colors * 8`,
/// tile height = full row height. Mirrors original `fieldSvgBlocks.ts`
/// lines 105–177.
fn push_diagonal_stripe_pattern(
    defs: &mut String,
    pattern_id: &str,
    colors: &[String],
    row_height: f64,
) {
    let n = colors.len() as f64;
    let tile_w = STRIPE_WIDTH_PX * n;
    let _ = write!(
        defs,
        r#"<pattern id="{id}" patternUnits="userSpaceOnUse" width="{tw}" height="{th}" patternTransform="rotate(45)">"#,
        id = pattern_id,
        tw = tile_w,
        th = row_height,
    );
    for (i, color) in colors.iter().enumerate() {
        // -row_height to +2*row_height so the pattern fills past edges
        // when rotated (matches original's `y=-height, height=height*3`).
        let _ = write!(
            defs,
            r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{c}"/>"#,
            x = i as f64 * STRIPE_WIDTH_PX,
            y = -row_height,
            w = STRIPE_WIDTH_PX,
            h = row_height * 3.0,
            c = color,
        );
    }
    defs.push_str("</pattern>");
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dayview::{
        compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput,
        DayviewFieldInput, DayviewPoint,
    };

    const DAY_START: i64 = 1_745_712_000_000;
    const HOUR: i64 = 60 * 60 * 1000;

    fn one_day_with_block(error: bool) -> DayviewConfig {
        DayviewConfig {
            days: vec![DayviewDayInput {
                label_lines: vec!["Mon".into(), "Apr 27".into(), "2026".into()],
                start_ms: DAY_START,
                end_ms: DAY_START + 24 * HOUR,
                has_error: error,
                fields: vec![DayviewFieldInput {
                    field: "mood".into(),
                    y_frac: 0.1,
                    height_frac: 0.15,
                    z_index: 0.1,
                    blocks: vec![DayviewBlock {
                        start_ms: DAY_START + 6 * HOUR,
                        end_ms: DAY_START + 7 * HOUR,
                        fill: BlockFill::Single("#abcdef".into()),
                        state_label: Some("happy".into()),
                    }],
                    points: vec![DayviewPoint {
                        time_ms: DAY_START + 8 * HOUR,
                        fg_color: "#ff0000".into(),
                        bg_color: "#000000".into(),
                        state_label: None,
                    }],
                }],
            }],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        }
    }

    #[test]
    fn emits_valid_svg_envelope() {
        let layout = compute_dayview(&one_day_with_block(false));
        let svg = render_dayview_svg(&layout);
        assert!(svg.starts_with("<svg "));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains(r#"width="800""#));
        assert!(svg.contains(r#"fill="black""#));
    }

    #[test]
    fn renders_block_field_label_axis() {
        let layout = compute_dayview(&one_day_with_block(false));
        let svg = render_dayview_svg(&layout);
        // Field band rendered.
        assert!(svg.contains(r#"data-field="mood""#));
        assert!(svg.contains(r#"class="block""#));
        assert!(svg.contains("#abcdef"));
        // Date label.
        assert!(svg.contains("date-label"));
        assert!(svg.contains("Mon"));
        // Hourly gridlines (24 distinct hours; opacity 0.4 every 3rd).
        assert!(svg.contains("gridline"));
        assert!(svg.contains(r#"stroke-opacity="0.4""#));
        assert!(svg.contains(r#"stroke-opacity="0.1""#));
        // Time axis baseline + tick labels.
        assert!(svg.contains(r#"class="axis""#));
        assert!(svg.contains("tick-label"));
        // Now-line (today's window).
        assert!(svg.contains("current-time-line"));
        assert!(svg.contains("#ffcc00"));
        // Point bg+fg circles.
        assert!(svg.contains("point-bg"));
        assert!(svg.contains("point-fg"));
    }

    #[test]
    fn warning_icon_on_error_day() {
        let layout = compute_dayview(&one_day_with_block(true));
        let svg = render_dayview_svg(&layout);
        assert!(svg.contains("day-error"));
        // Warning icon SVG path includes the yellow triangle fill.
        assert!(svg.contains("#ffff00"));
    }

    #[test]
    fn no_warning_icon_when_no_error() {
        let layout = compute_dayview(&one_day_with_block(false));
        let svg = render_dayview_svg(&layout);
        assert!(!svg.contains("day-error"));
    }

    #[test]
    fn multi_state_block_emits_pattern_def() {
        let mut cfg = one_day_with_block(false);
        cfg.days[0].fields[0].blocks[0].fill = BlockFill::MultiState(vec![
            "#ff0000".into(),
            "#00ff00".into(),
            "#0000ff".into(),
        ]);
        let layout = compute_dayview(&cfg);
        let svg = render_dayview_svg(&layout);
        // Pattern is defined in <defs>.
        assert!(svg.contains("<defs>"));
        assert!(svg.contains("<pattern"));
        assert!(svg.contains(r#"patternTransform="rotate(45)""#));
        // All 3 stripe colors appear.
        assert!(svg.contains("#ff0000"));
        assert!(svg.contains("#00ff00"));
        assert!(svg.contains("#0000ff"));
        // Block uses url(#...) fill.
        assert!(svg.contains("multi-state"));
        assert!(svg.contains("url(#"));
    }

    #[test]
    fn now_line_omitted_when_no_today() {
        let cfg = DayviewConfig {
            days: vec![DayviewDayInput {
                label_lines: vec!["Mon".into()],
                start_ms: DAY_START,
                end_ms: DAY_START + 24 * HOUR,
                has_error: false,
                fields: vec![],
            }],
            now_ms: DAY_START + 100 * HOUR, // far in the future
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let svg = render_dayview_svg(&layout);
        assert!(!svg.contains("current-time-line"));
    }

    #[test]
    fn html_wrap_includes_doctype() {
        let html = wrap_dayview_svg_in_html("<svg></svg>");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<svg></svg>"));
    }
}
