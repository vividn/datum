//! Pure-Rust SVG renderer for [`NowviewLayout`].
//!
//! Produces a self-contained SVG document string. No external CSS/JS.
//! WASM-clean: `string + format!` only — no system deps.
//!
//! The visual style mirrors datum's TypeScript nowview output:
//! - Black background
//! - Per-field horizontal bands with state-colored rects
//! - White gridlines (40% opacity major, 10% minor)
//! - Yellow (#ffcc00) dashed "now" line at the panel boundary
//! - HH:MM current-time label in the time-axis strip

use std::fmt::Write;

use crate::nowview::NowviewLayout;

const NOW_LINE_COLOR: &str = "#ffcc00";
const BG_COLOR: &str = "black";
const TEXT_COLOR: &str = "white";
const AXIS_COLOR: &str = "white";

/// Render `layout` to an SVG document string. The caller is expected to
/// have populated `layout.current_time_label` (e.g. `"15:30"`); this is
/// not derived here because the layout module is timezone-agnostic.
pub fn render_nowview_svg(layout: &NowviewLayout) -> String {
    let mut out = String::with_capacity(4096);

    // Outer SVG.
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

    // Plot group: translate inward by margin so plot-coords are (0,0)-origin.
    let _ = write!(
        &mut out,
        r#"<g class="plot" transform="translate({mx},{my})">"#,
        mx = layout.margin,
        my = layout.margin,
    );

    let data_height = layout.now_panel.height;

    // Field bands: rects (blocks) + circles (points). One <g> per field
    // so the DOM is browse-able by inspector and so future styles can
    // target a field.
    for row in &layout.fields {
        let _ = write!(
            &mut out,
            r#"<g class="field" data-field="{f}">"#,
            f = escape_xml(&row.field),
        );
        for b in &row.blocks {
            let _ = write!(
                &mut out,
                r#"<rect class="block" x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#,
                x = b.x,
                y = row.y,
                w = b.width,
                h = row.height,
                fill = b.color,
            );
        }
        for p in &row.points {
            // Two-circle stack: outer (bg) ring + inner (fg) dot.
            // Mirrors the da-k5z fg/bg rule.
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

    // History gridlines. Major: 40% opacity. Minor: 10% opacity. Drawn
    // after blocks so they sit on top.
    if layout.history_panel.is_some() {
        for t in &layout.minor_ticks {
            let _ = write!(
                &mut out,
                r#"<line class="gridline-minor" x1="{x}" x2="{x}" y1="0" y2="{y2}" stroke="white" stroke-opacity="0.1"/>"#,
                x = t.x,
                y2 = data_height,
            );
        }
        for t in &layout.major_ticks {
            let _ = write!(
                &mut out,
                r#"<line class="gridline-major" x1="{x}" x2="{x}" y1="0" y2="{y2}" stroke="white" stroke-opacity="0.4"/>"#,
                x = t.x,
                y2 = data_height,
            );
        }
    }

    // "Now" markers: vertical and horizontal yellow dashed lines (only
    // when there's a history panel — without history the now panel is
    // the whole plot).
    if layout.history_panel.is_some() {
        // Vertical line at panel boundary.
        let _ = write!(
            &mut out,
            r#"<line class="current-time-line-vertical" x1="{x}" x2="{x}" y1="0" y2="{y2}" stroke="{c}" stroke-width="2" stroke-dasharray="4,2"/>"#,
            x = layout.now_x,
            y2 = data_height + layout.time_axis_height,
            c = NOW_LINE_COLOR,
        );
        // Horizontal line across the now panel at the top of the time axis.
        let _ = write!(
            &mut out,
            r#"<line class="current-time-line-horizontal" x1="{x1}" x2="{x2}" y1="{y}" y2="{y}" stroke="{c}" stroke-width="2" stroke-dasharray="4,2"/>"#,
            x1 = layout.now_x,
            x2 = layout.now_x + layout.now_panel.width,
            y = data_height,
            c = NOW_LINE_COLOR,
        );
    }

    // Time axis: a horizontal baseline + tick labels. Drawn only when
    // time_axis_height is large enough to fit text.
    if layout.time_axis_height > 0.0 {
        let axis_y = data_height;
        let _ = write!(
            &mut out,
            r#"<line class="axis" x1="0" x2="{x1}" y1="{y}" y2="{y}" stroke="{c}"/>"#,
            x1 = layout.plot.width,
            y = axis_y,
            c = AXIS_COLOR,
        );

        if layout.time_axis_height > 6.0 {
            let label_size = (layout.time_axis_height - 6.0).max(6.0);
            for t in &layout.major_ticks {
                // Tick line down.
                let _ = write!(
                    &mut out,
                    r#"<line class="tick" x1="{x}" x2="{x}" y1="{y0}" y2="{y1}" stroke="{c}"/>"#,
                    x = t.x,
                    y0 = axis_y,
                    y1 = axis_y + 4.0,
                    c = AXIS_COLOR,
                );
                let _ = write!(
                    &mut out,
                    r#"<text class="tick-label" x="{x}" y="{y}" text-anchor="middle" fill="{c}" font-size="{fs}">{label}</text>"#,
                    x = t.x,
                    y = axis_y + label_size + 2.0,
                    c = TEXT_COLOR,
                    fs = label_size,
                    label = t.label,
                );
            }
        }
    }

    // Current-time label centered in the now panel (bottom, in the axis strip).
    if !layout.current_time_label.is_empty() && layout.time_axis_height > 0.0 {
        let text_size = (layout.time_axis_height + 2.0).min(layout.time_axis_height * 1.2);
        let text_x = layout.now_panel.x + layout.now_panel.width / 2.0;
        let text_y = data_height + layout.time_axis_height * 0.65;
        // Behind the text: a small black rect for readability when the
        // now panel is narrow (datum's nowview does this when nowWidth<40).
        if layout.now_panel.width < 40.0 {
            let _ = write!(
                &mut out,
                r#"<rect class="current-time-bg" x="{x}" y="{y}" width="44" height="{h}" fill="black"/>"#,
                x = text_x - 22.0,
                y = data_height + 1.0,
                h = layout.time_axis_height - 1.0,
            );
        }
        let _ = write!(
            &mut out,
            r#"<text class="current-time-text" x="{x}" y="{y}" text-anchor="middle" dominant-baseline="middle" fill="{c}" font-size="{fs}" font-weight="bold">{label}</text>"#,
            x = text_x,
            y = text_y,
            c = TEXT_COLOR,
            fs = text_size,
            label = escape_xml(&layout.current_time_label),
        );
    }

    out.push_str("</g></svg>");
    out
}

/// Wrap an SVG string in a minimal HTML document — used for `--output-file
/// foo.html`.
pub fn wrap_svg_in_html(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len() + 256);
    out.push_str("<!DOCTYPE html><html><head><meta charset=\"UTF-8\"><title>datum nowview</title>");
    out.push_str("<style>body{margin:0;background:black;}</style>");
    out.push_str("</head><body>");
    out.push_str(svg);
    out.push_str("</body></html>");
    out
}

/// Prepend the XML declaration to an SVG string. Required for files saved
/// to disk (browsers / image viewers expect it).
pub fn with_xml_declaration(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len() + 64);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(svg);
    out
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
    use crate::nowview::{
        compute_nowview, NowviewBlock, NowviewConfig, NowviewField, NowviewPoint,
    };

    #[test]
    fn renders_valid_svg_envelope() {
        let layout = compute_nowview(
            &[],
            &NowviewConfig {
                end_ms: 1_700_000_000_000,
                history_minutes: 0.0,
                now_width_minutes: 5.0,
                width: 100.0,
                height: 300.0,
                margin: 2.0,
                time_axis_height: 15.0,
            },
        );
        let svg = render_nowview_svg(&layout);
        assert!(svg.starts_with("<svg "));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains(r#"width="100""#));
        assert!(svg.contains(r#"height="300""#));
        assert!(svg.contains(r#"fill="black""#));
    }

    #[test]
    fn history_panel_emits_gridlines_and_now_line() {
        let now = 1_700_000_000_000_i64;
        let layout = compute_nowview(
            &[NowviewField {
                field: "mood".into(),
                blocks: vec![NowviewBlock {
                    start_ms: now - 30 * 60_000,
                    end_ms: now,
                    color: "#abcdef".into(),
                    state_label: Some("happy".into()),
                }],
                points: vec![NowviewPoint {
                    time_ms: now - 10 * 60_000,
                    fg_color: "#ff0000".into(),
                    bg_color: "#000000".into(),
                    state_label: None,
                }],
            }],
            &NowviewConfig {
                end_ms: now,
                history_minutes: 60.0,
                now_width_minutes: 5.0,
                width: 650.0,
                height: 300.0,
                margin: 2.0,
                time_axis_height: 15.0,
            },
        );
        let svg = render_nowview_svg(&layout);
        assert!(svg.contains("gridline-major"));
        assert!(svg.contains("gridline-minor"));
        assert!(svg.contains("current-time-line-vertical"));
        assert!(svg.contains("current-time-line-horizontal"));
        assert!(svg.contains("#ffcc00"));
        // Field bands.
        assert!(svg.contains(r#"data-field="mood""#));
        assert!(svg.contains(r#"class="block""#));
        // Point markers (bg + fg).
        assert!(svg.contains("point-bg"));
        assert!(svg.contains("point-fg"));
    }

    #[test]
    fn no_history_omits_now_lines() {
        let layout = compute_nowview(
            &[],
            &NowviewConfig {
                end_ms: 1_700_000_000_000,
                history_minutes: 0.0,
                now_width_minutes: 5.0,
                width: 100.0,
                height: 300.0,
                margin: 2.0,
                time_axis_height: 15.0,
            },
        );
        let svg = render_nowview_svg(&layout);
        assert!(!svg.contains("current-time-line-vertical"));
        assert!(!svg.contains("gridline-major"));
    }

    #[test]
    fn current_time_label_renders_when_provided() {
        let mut layout = compute_nowview(
            &[],
            &NowviewConfig {
                end_ms: 1_700_000_000_000,
                history_minutes: 0.0,
                now_width_minutes: 5.0,
                width: 200.0,
                height: 300.0,
                margin: 2.0,
                time_axis_height: 15.0,
            },
        );
        layout.current_time_label = "15:30".into();
        let svg = render_nowview_svg(&layout);
        assert!(svg.contains("15:30"));
        assert!(svg.contains("current-time-text"));
    }

    #[test]
    fn xml_declaration_prepends() {
        let s = with_xml_declaration("<svg/>");
        assert!(s.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(s.contains("<svg/>"));
    }

    #[test]
    fn html_wrap_includes_doctype() {
        let html = wrap_svg_in_html("<svg></svg>");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<svg></svg>"));
    }
}
