//! Minimal SVG renderer for [`Layout`].
//!
//! Output is a self-contained SVG string suitable for `--output day.svg` or
//! visual regression. No external CSS or JS — datum-layout is a leaf crate.

use std::fmt::Write;

use crate::Layout;

/// Render `layout` to an SVG document string.
pub fn render_svg(layout: &Layout) -> String {
    let mut out = String::with_capacity(2048);
    let _ = write!(
        &mut out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = layout.width,
        h = layout.height,
    );

    // Black background — matches datum's dayview.
    let _ = write!(
        &mut out,
        r#"<rect class="bg" width="100%" height="100%" fill="black"/>"#
    );

    // Hourly grid lines (every-3-hour stronger, like datum).
    for tick in &layout.axis_ticks {
        let opacity = if tick.hour % 3 == 0 { 0.4 } else { 0.1 };
        let _ = write!(
            &mut out,
            r#"<line class="gridline" x1="{x}" x2="{x}" y1="0" y2="{y2}" stroke="white" stroke-opacity="{op}"/>"#,
            x = tick.x,
            y2 = (layout.height - layout.axis_height).max(0.0),
            op = opacity,
        );
    }

    // Field rows: label + blocks.
    for row in &layout.fields {
        let label_y = row.y + row.height / 2.0;
        let _ = write!(
            &mut out,
            r#"<text class="field-label" x="{lx}" y="{ly}" dy="0.35em" fill="white" text-anchor="end" font-size="12">{name}</text>"#,
            lx = (layout.label_width - 4.0).max(0.0),
            ly = label_y,
            name = escape_xml(&row.field),
        );
        for block in &row.blocks {
            let fill = state_color(block.state.as_deref());
            let _ = write!(
                &mut out,
                r#"<rect class="block" field="{f}" x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#,
                f = escape_xml(&row.field),
                x = block.x,
                y = row.y,
                w = block.width,
                h = row.height,
                fill = fill,
            );
        }
    }

    // Bottom axis: hourly tick labels.
    if layout.axis_height > 0.0 {
        let axis_y = layout.height - layout.axis_height;
        let _ = write!(
            &mut out,
            r#"<line class="axis" x1="{x0}" x2="{x1}" y1="{y}" y2="{y}" stroke="white"/>"#,
            x0 = layout.label_width,
            x1 = layout.width,
            y = axis_y,
        );
        let label_size = (layout.axis_height - 4.0).max(6.0);
        for tick in &layout.axis_ticks {
            let _ = write!(
                &mut out,
                r#"<text class="axis-label" x="{x}" y="{y}" text-anchor="middle" fill="white" font-size="{fs}">{label}</text>"#,
                x = tick.x,
                y = layout.height - 2.0,
                fs = label_size,
                label = tick.label,
            );
        }
    }

    out.push_str("</svg>");
    out
}

/// Map a state to a stable fill color. Unknown states get a deterministic
/// pastel from the hash of the name; `None` is a neutral gray.
fn state_color(state: Option<&str>) -> String {
    match state {
        None => "#888".to_string(),
        Some(s) => {
            // Tiny FNV-1a hash → HSL hue.
            let mut h: u32 = 0x811c_9dc5;
            for b in s.bytes() {
                h ^= b as u32;
                h = h.wrapping_mul(0x0100_0193);
            }
            let hue = h % 360;
            format!("hsl({hue},60%,55%)")
        }
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
    use crate::{layout_day, LayoutConfig, LayoutEntry};

    #[test]
    fn render_svg_emits_expected_structure() {
        let day_start = 0i64;
        let hour = 60 * 60 * 1000i64;
        let cfg = LayoutConfig {
            day_start_ms: day_start,
            day_end_ms: day_start + 24 * hour,
            width: 240.0,
            height: 100.0,
            label_width: 40.0,
            axis_height: 12.0,
            min_block_px: 1.0,
        };
        let entries = vec![LayoutEntry {
            field: "mood".into(),
            start_ms: 6 * hour,
            end_ms: Some(7 * hour),
            state: Some("happy".into()),
        }];
        let svg = render_svg(&layout_day(&entries, &cfg));

        assert!(svg.starts_with("<svg "));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains(r#"width="240""#));
        assert!(svg.contains(r#"class="block""#));
        assert!(svg.contains(r#"field="mood""#));
        assert!(svg.contains("mood")); // label too
    }
}
