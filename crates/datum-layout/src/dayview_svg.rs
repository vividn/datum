//! Pure-Rust SVG renderer for [`DayviewLayout`].
//!
//! Produces a self-contained SVG document string. No external CSS/JS,
//! no popover, no hover (per bead da-vp7 — the SvelteKit frontend handles
//! interactivity).
//!
//! Visual style mirrors datum's TypeScript dayview output:
//! - Black background.
//! - Hourly gridlines (40% opacity every 3rd hour, 10% otherwise).
//! - Per-day stack with date labels on the side adjacent to the data area
//!   (left for horizontal, top for vertical — see bead da-tbe).
//! - Per-field band rendering: solid blocks, multi-state diagonal stripes,
//!   point markers (fg + bg circles per da-k5z).
//! - Yellow dashed "now" line on today's row only.
//! - Warning icon next to date labels for days with conflicts.
//! - Time-axis strip on the side opposite the date labels (bottom for
//!   horizontal, left for vertical).

use std::fmt::Write;

use crate::dayview::{BlockFill, DayviewLayout, Orientation};

const NOW_LINE_COLOR: &str = "#ffcc00";
const BG_COLOR: &str = "black";
const TEXT_COLOR: &str = "white";
const AXIS_COLOR: &str = "white";

/// SVG (x, y) origin of the data-area within the plot, plus the cross-axis
/// extent across all stacked days. Pre-computed once per render so all
/// element-emitting helpers stay terse.
struct PlotGeom {
    data_x: f64,
    data_y: f64,
    /// Cross-axis pixel extent across all days (= n*day_height + (n-1)*idm).
    cross_total: f64,
}

impl PlotGeom {
    fn new(layout: &DayviewLayout) -> Self {
        let cross_total = layout.days.len() as f64 * layout.day_height
            + (layout.days.len().saturating_sub(1)) as f64 * layout.interday_margin;
        match layout.orientation {
            // Date-label strip on the left, time-axis strip at the bottom.
            Orientation::Horizontal => Self {
                data_x: layout.label_width,
                data_y: 0.0,
                cross_total,
            },
            // Date-label strip on top, time-axis strip on the left.
            Orientation::Vertical => Self {
                data_x: layout.time_axis_height,
                data_y: layout.label_width,
                cross_total,
            },
        }
    }
}

/// Map orientation-neutral (time, cross) → SVG (x, y) within the plot.
fn to_xy(layout: &DayviewLayout, geom: &PlotGeom, time: f64, cross: f64) -> (f64, f64) {
    match layout.orientation {
        Orientation::Horizontal => (geom.data_x + time, geom.data_y + cross),
        Orientation::Vertical => (geom.data_x + cross, geom.data_y + time),
    }
}

/// Render `layout` to an SVG document string.
pub fn render_dayview_svg(layout: &DayviewLayout) -> String {
    let mut out = String::with_capacity(8192);
    let geom = PlotGeom::new(layout);

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

    // Per-day rendering.
    for (i, day) in layout.days.iter().enumerate() {
        // Date label group — positioned at the start of this day's slot
        // along the cross axis, centered on the perpendicular dim.
        let (label_group_x, label_group_y) = match layout.orientation {
            Orientation::Horizontal => (0.0, day.day_offset),
            Orientation::Vertical => (geom.data_x + day.day_offset, 0.0),
        };
        let _ = write!(
            &mut out,
            r#"<g class="day-label" data-day="{i}" transform="translate({lx},{ly})">"#,
            i = i,
            lx = label_group_x,
            ly = label_group_y,
        );
        let n_lines = day.label_lines.len().max(1);
        for (j, line) in day.label_lines.iter().enumerate() {
            let (lx, ly, anchor) = match layout.orientation {
                Orientation::Horizontal => {
                    // Right-aligned within the label strip; lines stacked
                    // vertically across the row.
                    let line_y = ((j as f64 + 0.5) * layout.day_height) / n_lines as f64;
                    (layout.label_width - 4.0, line_y, "end")
                }
                Orientation::Vertical => {
                    // Centered horizontally within the column; lines
                    // stacked vertically within the top label strip.
                    let cx = layout.day_height / 2.0;
                    let line_y = ((j as f64 + 0.5) * layout.label_width) / n_lines as f64;
                    (cx, line_y, "middle")
                }
            };
            let _ = write!(
                &mut out,
                r#"<text class="date-label" x="{lx}" y="{ly}" dy="0.35em" fill="{c}" text-anchor="{a}" font-size="{fs}">{txt}</text>"#,
                c = TEXT_COLOR,
                fs = layout.label_font_size,
                a = anchor,
                txt = escape_xml(line),
            );
        }
        // Warning icon for conflicts — placed next to the date label.
        if day.has_error {
            let icon_size = (layout.day_height * 0.4).clamp(8.0, 20.0);
            let (icon_x, icon_y) = match layout.orientation {
                Orientation::Horizontal => (
                    layout.label_width - 2.0,
                    (layout.day_height - icon_size) / 2.0,
                ),
                Orientation::Vertical => (
                    (layout.day_height - icon_size) / 2.0,
                    layout.label_width - 2.0 - icon_size,
                ),
            };
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

        // Per-field bands — emitted in plot coords (no per-day transform);
        // helpers map (time, cross) → (x, y) per orientation.
        let _ = write!(&mut out, r#"<g class="day-data" data-day="{i}">"#, i = i,);
        for field in &day.fields {
            let _ = write!(
                &mut out,
                r#"<g class="field" data-field="{f}">"#,
                f = escape_xml(&field.field),
            );
            for b in &field.blocks {
                let (rect_x, rect_y, rect_w, rect_h) = block_rect(
                    layout,
                    &geom,
                    day.day_offset + field.cross_offset,
                    field.cross_extent,
                    b.time_offset,
                    b.time_extent,
                );
                match &b.fill {
                    BlockFill::Single(color) => {
                        let _ = write!(
                            &mut out,
                            r#"<rect class="block" x="{x}" y="{y}" width="{w}" height="{h}" fill="{c}"/>"#,
                            x = rect_x,
                            y = rect_y,
                            w = rect_w,
                            h = rect_h,
                            c = color,
                        );
                    }
                    BlockFill::MultiState(colors) => {
                        if colors.is_empty() {
                            continue;
                        }
                        push_proportional_segments(
                            &mut out,
                            layout.orientation,
                            rect_x,
                            rect_y,
                            rect_w,
                            rect_h,
                            colors,
                        );
                    }
                }
            }
            for p in &field.points {
                let (cx, cy) = to_xy(layout, &geom, p.time_pos, day.day_offset + p.cross_pos);
                let outer_r = p.r;
                let inner_r = (p.r * 0.6).max(0.5);
                let _ = write!(
                    &mut out,
                    r#"<circle class="point-bg" cx="{cx}" cy="{cy}" r="{r}" fill="{c}"/>"#,
                    cx = cx,
                    cy = cy,
                    r = outer_r,
                    c = p.bg_color,
                );
                let _ = write!(
                    &mut out,
                    r#"<circle class="point-fg" cx="{cx}" cy="{cy}" r="{r}" fill="{c}"/>"#,
                    cx = cx,
                    cy = cy,
                    r = inner_r,
                    c = p.fg_color,
                );
            }
            out.push_str("</g>");
        }
        out.push_str("</g>");
    }

    // Time axis — bottom strip in horizontal, left strip in vertical.
    if layout.time_axis_height > 0.0 {
        emit_time_axis(&mut out, layout, &geom);
    }

    // Hourly gridlines spanning the full data area perpendicular to time.
    // Drawn AFTER per-day bands and the axis so the semi-transparent lines
    // overlay the data (matches original datum z-order — see dayview.ts).
    // 40% opacity every 3rd hour, 10% otherwise.
    for tick in &layout.axis_ticks {
        let opacity = if tick.is_major { 0.4 } else { 0.1 };
        let (x1, y1, x2, y2) = match layout.orientation {
            Orientation::Horizontal => {
                let x = geom.data_x + tick.time_pos;
                (x, geom.data_y, x, geom.data_y + geom.cross_total)
            }
            Orientation::Vertical => {
                let y = geom.data_y + tick.time_pos;
                (geom.data_x, y, geom.data_x + geom.cross_total, y)
            }
        };
        let _ = write!(
            &mut out,
            r#"<line class="gridline" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}" stroke="white" stroke-opacity="{op}"/>"#,
            op = opacity,
        );
    }

    // Per-day now-lines. Emitted last so they sit above gridlines and data.
    for day in &layout.days {
        if let Some(nt) = day.now_time_pos {
            let (x1, y1, x2, y2) = match layout.orientation {
                Orientation::Horizontal => {
                    let x = geom.data_x + nt;
                    (
                        x,
                        geom.data_y + day.day_offset,
                        x,
                        geom.data_y + day.now_extent_far,
                    )
                }
                Orientation::Vertical => {
                    let y = geom.data_y + nt;
                    (
                        geom.data_x + day.day_offset,
                        y,
                        geom.data_x + day.now_extent_far,
                        y,
                    )
                }
            };
            let _ = write!(
                &mut out,
                r#"<line class="current-time-line" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}" stroke="{c}" stroke-width="2" stroke-dasharray="4,2"/>"#,
                c = NOW_LINE_COLOR,
            );
        }
    }

    out.push_str("</g>");

    out.push_str("</svg>");
    out
}

/// Compute the SVG rect (x, y, w, h) for a block given its cross-axis
/// position+extent (in plot-coord cross axis) and time-axis position+extent.
fn block_rect(
    layout: &DayviewLayout,
    geom: &PlotGeom,
    cross_off: f64,
    cross_ext: f64,
    time_off: f64,
    time_ext: f64,
) -> (f64, f64, f64, f64) {
    match layout.orientation {
        Orientation::Horizontal => (
            geom.data_x + time_off,
            geom.data_y + cross_off,
            time_ext,
            cross_ext,
        ),
        Orientation::Vertical => (
            geom.data_x + cross_off,
            geom.data_y + time_off,
            cross_ext,
            time_ext,
        ),
    }
}

fn emit_time_axis(out: &mut String, layout: &DayviewLayout, geom: &PlotGeom) {
    let label_size = if layout.time_axis_height > 6.0 {
        (layout.time_axis_height - 6.0).max(6.0)
    } else {
        0.0
    };
    let n = layout.axis_ticks.len();
    match layout.orientation {
        Orientation::Horizontal => {
            // Baseline beneath the day-stack data area.
            let axis_y = geom.data_y + geom.cross_total;
            let _ = write!(
                out,
                r#"<line class="axis" x1="{x0}" x2="{x1}" y1="{y}" y2="{y}" stroke="{c}"/>"#,
                x0 = geom.data_x,
                x1 = geom.data_x + layout.time_axis_extent,
                y = axis_y,
                c = AXIS_COLOR,
            );
            if label_size > 0.0 {
                for (i, tick) in layout.axis_ticks.iter().enumerate() {
                    if i + 1 == n {
                        break;
                    }
                    let x = geom.data_x + tick.time_pos;
                    let _ = write!(
                        out,
                        r#"<line class="tick" x1="{x}" x2="{x}" y1="{y0}" y2="{y1}" stroke="{c}"/>"#,
                        x = x,
                        y0 = axis_y,
                        y1 = axis_y + 4.0,
                        c = AXIS_COLOR,
                    );
                    let _ = write!(
                        out,
                        r#"<text class="tick-label" x="{x}" y="{y}" text-anchor="middle" fill="{c}" font-size="{fs}">{label}</text>"#,
                        x = x,
                        y = axis_y + label_size + 2.0,
                        c = TEXT_COLOR,
                        fs = label_size,
                        label = tick.label,
                    );
                }
            }
        }
        Orientation::Vertical => {
            // Baseline at the right edge of the time-axis strip.
            let axis_x = geom.data_x;
            let _ = write!(
                out,
                r#"<line class="axis" x1="{x}" x2="{x}" y1="{y0}" y2="{y1}" stroke="{c}"/>"#,
                x = axis_x,
                y0 = geom.data_y,
                y1 = geom.data_y + layout.time_axis_extent,
                c = AXIS_COLOR,
            );
            if label_size > 0.0 {
                for (i, tick) in layout.axis_ticks.iter().enumerate() {
                    if i + 1 == n {
                        break;
                    }
                    let y = geom.data_y + tick.time_pos;
                    let _ = write!(
                        out,
                        r#"<line class="tick" x1="{x0}" x2="{x1}" y1="{y}" y2="{y}" stroke="{c}"/>"#,
                        x0 = axis_x - 4.0,
                        x1 = axis_x,
                        y = y,
                        c = AXIS_COLOR,
                    );
                    let _ = write!(
                        out,
                        r#"<text class="tick-label" x="{x}" y="{y}" text-anchor="end" fill="{c}" font-size="{fs}" dy="0.35em">{label}</text>"#,
                        x = axis_x - 6.0,
                        y = y,
                        c = TEXT_COLOR,
                        fs = label_size,
                        label = tick.label,
                    );
                }
            }
        }
    }
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

/// Emit one `<rect>` per state color, side-by-side along the time axis,
/// covering the same total area as a single block fill. Per da-9vop, a
/// parallel state set (`vertical,sit`) renders as proportional segments
/// (left half / right half / etc.) so each state's color is directly
/// readable rather than blended into a stripe pattern.
fn push_proportional_segments(
    out: &mut String,
    orientation: Orientation,
    rect_x: f64,
    rect_y: f64,
    rect_w: f64,
    rect_h: f64,
    colors: &[String],
) {
    let n = colors.len() as f64;
    for (i, color) in colors.iter().enumerate() {
        let frac_lo = i as f64 / n;
        let frac_hi = (i + 1) as f64 / n;
        let (sx, sy, sw, sh) = match orientation {
            Orientation::Horizontal => (
                rect_x + rect_w * frac_lo,
                rect_y,
                rect_w * (frac_hi - frac_lo),
                rect_h,
            ),
            Orientation::Vertical => (
                rect_x,
                rect_y + rect_h * frac_lo,
                rect_w,
                rect_h * (frac_hi - frac_lo),
            ),
        };
        let _ = write!(
            out,
            r#"<rect class="block multi-state" x="{x}" y="{y}" width="{w}" height="{h}" fill="{c}"/>"#,
            x = sx,
            y = sy,
            w = sw,
            h = sh,
            c = color,
        );
    }
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
        DayviewFieldInput, DayviewPoint, Orientation,
    };

    const DAY_START: i64 = 1_745_712_000_000;
    const HOUR: i64 = 60 * 60 * 1000;

    fn one_day_with_block(error: bool, orientation: Orientation) -> DayviewConfig {
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
            height: Some(800.0),
            orientation,
            ..Default::default()
        }
    }

    #[test]
    fn emits_valid_svg_envelope() {
        let layout = compute_dayview(&one_day_with_block(false, Orientation::Horizontal));
        let svg = render_dayview_svg(&layout);
        assert!(svg.starts_with("<svg "));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains(r#"width="800""#));
        assert!(svg.contains(r#"fill="black""#));
    }

    #[test]
    fn horizontal_renders_block_field_label_axis() {
        let layout = compute_dayview(&one_day_with_block(false, Orientation::Horizontal));
        let svg = render_dayview_svg(&layout);
        assert!(svg.contains(r#"data-field="mood""#));
        assert!(svg.contains(r#"class="block""#));
        assert!(svg.contains("#abcdef"));
        assert!(svg.contains("date-label"));
        assert!(svg.contains("Mon"));
        assert!(svg.contains("gridline"));
        assert!(svg.contains(r#"stroke-opacity="0.4""#));
        assert!(svg.contains(r#"stroke-opacity="0.1""#));
        assert!(svg.contains(r#"class="axis""#));
        assert!(svg.contains("tick-label"));
        assert!(svg.contains("current-time-line"));
        assert!(svg.contains("#ffcc00"));
        assert!(svg.contains("point-bg"));
        assert!(svg.contains("point-fg"));
    }

    #[test]
    fn vertical_renders_block_field_label_axis() {
        let layout = compute_dayview(&one_day_with_block(false, Orientation::Vertical));
        let svg = render_dayview_svg(&layout);
        // Same logical content present, just laid out for vertical mode.
        assert!(svg.contains(r#"data-field="mood""#));
        assert!(svg.contains(r#"class="block""#));
        assert!(svg.contains("#abcdef"));
        assert!(svg.contains("date-label"));
        assert!(svg.contains("Mon"));
        assert!(svg.contains("gridline"));
        assert!(svg.contains(r#"class="axis""#));
        assert!(svg.contains("tick-label"));
        assert!(svg.contains("current-time-line"));
        assert!(svg.contains("point-bg"));
        assert!(svg.contains("point-fg"));
    }

    #[test]
    fn warning_icon_on_error_day() {
        let layout = compute_dayview(&one_day_with_block(true, Orientation::Horizontal));
        let svg = render_dayview_svg(&layout);
        assert!(svg.contains("day-error"));
        assert!(svg.contains("#ffff00"));
    }

    #[test]
    fn no_warning_icon_when_no_error() {
        let layout = compute_dayview(&one_day_with_block(false, Orientation::Horizontal));
        let svg = render_dayview_svg(&layout);
        assert!(!svg.contains("day-error"));
    }

    #[test]
    fn multi_state_block_renders_proportional_segments() {
        // da-9vop: parallel state set (`vertical,sit`) renders as N
        // proportional rect segments side-by-side along the time axis,
        // each in its own state color. No diagonal-stripe `<pattern>`.
        let mut cfg = one_day_with_block(false, Orientation::Horizontal);
        cfg.days[0].fields[0].blocks[0].fill =
            BlockFill::MultiState(vec!["#ff0000".into(), "#00ff00".into(), "#0000ff".into()]);
        let layout = compute_dayview(&cfg);
        let svg = render_dayview_svg(&layout);
        assert!(!svg.contains("<defs>"), "no pattern defs needed: {svg}");
        assert!(!svg.contains("<pattern"), "no pattern element: {svg}");
        assert!(!svg.contains("rotate(45)"));
        assert!(svg.contains("multi-state"));
        // One rect per state color, each as its own fill (not via url(#…)).
        assert!(svg.contains(r##"fill="#ff0000""##));
        assert!(svg.contains(r##"fill="#00ff00""##));
        assert!(svg.contains(r##"fill="#0000ff""##));
        // Three multi-state rects (one per segment).
        assert_eq!(svg.matches(r#"class="block multi-state""#).count(), 3);
    }

    #[test]
    fn multi_state_segments_split_block_horizontally_for_two_states() {
        // 2 states → left half + right half. The block in `one_day_with_block`
        // spans 06:00–07:00 (1 hour) on a 24h horizontal layout — so each
        // segment occupies half the block's time-axis extent.
        let mut cfg = one_day_with_block(false, Orientation::Horizontal);
        cfg.days[0].fields[0].blocks[0].fill =
            BlockFill::MultiState(vec!["#aa3333".into(), "#33aa33".into()]);
        let layout = compute_dayview(&cfg);
        let svg = render_dayview_svg(&layout);
        // First (left) segment uses state[0]'s color, second uses state[1]'s.
        let first = svg
            .find(r##"fill="#aa3333""##)
            .expect("first segment color");
        let second = svg
            .find(r##"fill="#33aa33""##)
            .expect("second segment color");
        assert!(
            first < second,
            "first state must render to the left of the second (horizontal split)",
        );
    }

    #[test]
    fn gridlines_rendered_over_data_and_under_now_line() {
        // Gridlines must appear AFTER data blocks (so they overlay) and
        // BEFORE the now line (so the now line stays on top). Mirrors
        // original datum z-order (dayview.ts: data → axis → grid → now).
        let layout = compute_dayview(&one_day_with_block(false, Orientation::Horizontal));
        let svg = render_dayview_svg(&layout);
        let block_idx = svg.find(r#"class="block""#).expect("block present");
        let grid_idx = svg.find("gridline").expect("gridline present");
        let now_idx = svg.find("current-time-line").expect("now line present");
        assert!(
            block_idx < grid_idx,
            "gridlines must be emitted after data blocks (block@{block_idx}, grid@{grid_idx})",
        );
        assert!(
            grid_idx < now_idx,
            "now line must be emitted after gridlines (grid@{grid_idx}, now@{now_idx})",
        );
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
            now_ms: DAY_START + 100 * HOUR,
            width: 800.0,
            day_height: 100.0,
            orientation: Orientation::Horizontal,
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
