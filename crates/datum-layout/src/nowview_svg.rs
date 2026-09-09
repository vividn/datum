//! Thin overlay wrapper around the dayview SVG renderer (per bead da-6jy).
//!
//! Per bead da-6jy the nowview shares dayview's render path: the CLI builds
//! a single-day [`crate::dayview::DayviewConfig`] scoped to the last
//! `history_minutes`, calls [`crate::dayview::compute_dayview`], and the
//! dayview renderer paints field bands + gridlines + the yellow now-line.
//! This module adds the nowview-specific overlay on top: the strip right
//! of the now-line, where each field's lane continues in the colour of the
//! state it is in *now*, and the `HH:MM` clock label under it.
//!
//! Pure-Rust, WASM-clean.

use std::fmt::Write;

use crate::dayview::{BlockFill, DayviewLayout, Orientation};
use crate::dayview_svg::{
    render_dayview_svg_with, text_width, DayviewSvgOpts, RightPanelFill, AXIS_LABEL_GAP,
};
use crate::nowview::NowviewCurrentState;

const TEXT_COLOR: &str = "white";
/// Below this strip width the clock can't be centred inside the strip
/// without overflowing the canvas, so it centres on the now-line instead.
/// Mirrors `isNowWidthSmall` in the original datum's `nowview.ts`.
const CLOCK_CENTER_MIN_STRIP: f64 = 40.0;

/// Render `layout` (a dayview layout the CLI built for the nowview window)
/// to an SVG document string, then overlay the now strip: one chunk per
/// field carrying the state it is in at "now", drawn from the now-line to
/// the right edge, and the `HH:MM` `current_time_label` under them. An
/// empty `current_time_label` skips the clock; a field whose entry has no
/// `state_label` — it is in no state right now — gets no chunk.
pub fn render_nowview_svg(
    layout: &DayviewLayout,
    current_time_label: &str,
    current_states: &[NowviewCurrentState],
) -> String {
    let fills = now_strip_fills(current_states);
    // The clock sits in the now strip; it only steals room from the tick
    // labels when it is wider than the strip (da-sv8c).
    let opts = DayviewSvgOpts {
        axis_end_reserved: clock_reserved_px(layout, current_time_label),
        right_panel_fills: &fills,
    };
    let mut svg = render_dayview_svg_with(layout, opts);
    if !current_time_label.is_empty() {
        let overlay = build_current_time_overlay(layout, current_time_label);
        inject_inside_plot_group(&mut svg, &overlay);
    }
    svg
}

/// Turn the resolved current states into the per-field strip fills the
/// dayview renderer paints. A field in no state right now (`state_label`
/// is `None`) gets no chunk — its lane simply stops at the now-line.
fn now_strip_fills(current_states: &[NowviewCurrentState]) -> Vec<RightPanelFill> {
    current_states
        .iter()
        .filter(|e| e.state_label.is_some())
        .filter_map(|e| {
            let fill = match e.colors.as_slice() {
                [] => return None,
                [single] => BlockFill::Single(single.clone()),
                many => BlockFill::MultiState(many.to_vec()),
            };
            Some(RightPanelFill {
                field: e.field.clone(),
                fill,
            })
        })
        .collect()
}

/// Font size of the clock overlay. Scales with the time-axis strip but
/// stays readable when the strip is suppressed (callers may pass
/// `time_axis_height = 0`).
fn clock_font_size(layout: &DayviewLayout) -> f64 {
    if layout.time_axis_height > 0.0 {
        (layout.time_axis_height + 2.0).min(layout.time_axis_height * 1.2)
    } else {
        12.0
    }
}

/// Extent the clock takes along the axis's own label direction: its text
/// width when time runs horizontally, one line of it when time runs
/// vertically.
fn clock_extent_px(layout: &DayviewLayout, label: &str) -> f64 {
    let font_size = clock_font_size(layout);
    match layout.orientation {
        Orientation::Horizontal => text_width(label, font_size),
        Orientation::Vertical => font_size,
    }
}

/// Where the clock's centre sits along the time axis: the middle of the
/// now strip when the strip is wide enough to be worth centring in, the
/// now-line otherwise — then pulled back so the text never runs off the
/// plot's far edge.
fn clock_time_pos(layout: &DayviewLayout, label: &str) -> f64 {
    let strip = layout.right_panel_width.max(0.0);
    let far = layout.time_axis_extent + strip;
    let centred = if strip >= CLOCK_CENTER_MIN_STRIP {
        layout.time_axis_extent + strip / 2.0
    } else {
        layout.time_axis_extent
    };
    let half = clock_extent_px(layout, label) / 2.0;
    centred.min(far - half).max(half.min(centred))
}

/// Room the clock needs at the far end of the *time axis* — how far it
/// reaches back past the now-line, which is the only part that can collide
/// with a tick label. Centred in a roomy strip it reaches back not at all.
fn clock_reserved_px(layout: &DayviewLayout, label: &str) -> f64 {
    if label.is_empty() {
        return 0.0;
    }
    let half = clock_extent_px(layout, label) / 2.0;
    let overhang = half - (clock_time_pos(layout, label) - layout.time_axis_extent);
    (AXIS_LABEL_GAP + overhang).max(0.0)
}

/// Build the `<text>` overlay placed near the now-line. Centred in the now
/// strip when the strip is wide enough to hold it, on the now-line itself
/// otherwise — the original datum's `isNowWidthSmall` rule. Coordinates
/// are in plot-local space (the renderer's outer `<g class="plot">`
/// translates by `margin`).
fn build_current_time_overlay(layout: &DayviewLayout, label: &str) -> String {
    let n_days = layout.days.len();
    if n_days == 0 || layout.time_axis_extent <= 0.0 {
        return String::new();
    }

    // Cross-axis extent of the stacked-day data area.
    let cross_total = n_days as f64 * layout.day_height
        + (n_days.saturating_sub(1)) as f64 * layout.interday_margin;

    // Origin of the data area within the plot group, mirroring
    // `dayview_svg::PlotGeom::new`.
    let (data_x, data_y) = match layout.orientation {
        Orientation::Horizontal => (layout.label_width, 0.0),
        Orientation::Vertical => (layout.time_axis_height, layout.label_width),
    };

    let font_size = clock_font_size(layout);
    let time_pos = clock_time_pos(layout, label);

    let mut out = String::new();
    let _ = write!(&mut out, r#"<g class="nowview-overlay">"#,);
    match layout.orientation {
        Orientation::Horizontal => {
            // On the mid-line of the time-axis strip, so it reads as the
            // now-line's clock label.
            let text_x = data_x + time_pos;
            let text_y = data_y
                + cross_total
                + if layout.time_axis_height > 0.0 {
                    layout.time_axis_height * 0.65
                } else {
                    font_size
                };
            let _ = write!(
                &mut out,
                r#"<text class="current-time-text" x="{x}" y="{y}" text-anchor="middle" dominant-baseline="middle" fill="{c}" font-size="{fs}" font-weight="bold">{label}</text>"#,
                x = text_x,
                y = text_y,
                c = TEXT_COLOR,
                fs = font_size,
                label = escape_xml(label),
            );
        }
        Orientation::Vertical => {
            // Against the time-axis strip on the left, at the now-line's
            // own position along the (vertical) time axis.
            let text_x = data_x - 2.0;
            let text_y = data_y + time_pos + font_size / 2.0;
            let _ = write!(
                &mut out,
                r#"<text class="current-time-text" x="{x}" y="{y}" text-anchor="end" fill="{c}" font-size="{fs}" font-weight="bold">{label}</text>"#,
                x = text_x,
                y = text_y,
                c = TEXT_COLOR,
                fs = font_size,
                label = escape_xml(label),
            );
        }
    }
    out.push_str("</g>");
    out
}

/// Insert `overlay` inside the dayview's `<g class="plot">` group so the
/// overlay's coordinates resolve in plot-local space (mirroring how the
/// rest of the dayview SVG is emitted). Returns the original `svg` unchanged
/// when the expected closing tags can't be found (defensive — shouldn't
/// happen against valid dayview output).
fn inject_inside_plot_group(svg: &mut String, overlay: &str) {
    // The dayview SVG ends with "</g></svg>" (plot-group close, then root
    // close). Inject before the closing plot-group tag.
    let needle = "</g></svg>";
    if let Some(pos) = svg.rfind(needle) {
        svg.insert_str(pos, overlay);
    }
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
    use crate::dayview::{
        compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput,
        DayviewFieldInput, DayviewPoint, Orientation,
    };
    use crate::nowview::nowview_axis_ticks;

    /// The `<g class="field" data-field="…">` slice for one field.
    fn field_group(svg: &str, field: &str) -> String {
        let open = format!(r#"<g class="field" data-field="{field}">"#);
        let start = svg
            .find(&open)
            .unwrap_or_else(|| panic!("no band for {field} in: {svg}"))
            + open.len();
        let end = svg[start..].find("</g>").expect("band close") + start;
        svg[start..end].to_string()
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct RectBox {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    /// Parse every `<rect class="<class>" …>` in `frag` into boxes, in
    /// document order. The renderer writes the four attributes in a fixed
    /// order, which is what makes this scan enough.
    fn rects(frag: &str, class: &str) -> Vec<RectBox> {
        let open = format!(r#"<rect class="{class}" "#);
        let mut out = Vec::new();
        let mut rest = frag;
        while let Some(i) = rest.find(&open) {
            let tag_start = i + open.len();
            let tag_end = rest[tag_start..].find("/>").expect("rect close") + tag_start;
            let tag = &rest[tag_start..tag_end];
            let attr = |name: &str| -> f64 {
                let key = format!(r#"{name}=""#);
                let s = tag.find(&key).map(|k| k + key.len()).expect("attr");
                let e = tag[s..].find('"').expect("attr close") + s;
                tag[s..e].parse().expect("number")
            };
            out.push(RectBox {
                x: attr("x"),
                y: attr("y"),
                w: attr("width"),
                h: attr("height"),
            });
            rest = &rest[tag_end..];
        }
        out
    }

    fn rect_attrs(frag: &str, class: &str) -> Option<RectBox> {
        rects(frag, class).into_iter().next()
    }

    /// Build a single-day dayview config matching what the nowview command
    /// produces — 30 min window, single day with one block + one point.
    fn nowview_layout() -> crate::dayview::DayviewLayout {
        let now_ms = 1_700_000_000_000_i64;
        let history_ms = 30 * 60 * 1000;
        let day = DayviewDayInput {
            label_lines: vec!["Mon".into()],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            has_error: false,
            fields: vec![DayviewFieldInput {
                field: "mood".into(),
                y_frac: 0.1,
                height_frac: 0.2,
                z_index: 0.1,
                blocks: vec![DayviewBlock {
                    start_ms: now_ms - 20 * 60 * 1000,
                    end_ms: now_ms,
                    fill: BlockFill::Single("#abcdef".into()),
                    state_label: Some("happy".into()),
                }],
                points: vec![DayviewPoint {
                    time_ms: now_ms - 10 * 60 * 1000,
                    fg_color: "#ff0000".into(),
                    bg_color: "#000000".into(),
                    state_label: None,
                }],
            }],
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms,
            width: 600.0,
            height: Some(300.0),
            day_height: 250.0,
            interday_margin: 0.0,
            time_axis_height: 15.0,
            margin: 2.0,
            label_width: None,
            orientation: Orientation::Horizontal,
            axis_ticks_override: Some(nowview_axis_ticks(
                // approximate — exact extent is recomputed by compute_dayview
                500.0, 30.0,
            )),
            right_panel_width: 0.0,
        };
        compute_dayview(&cfg)
    }

    #[test]
    fn renders_dayview_envelope_with_current_time() {
        let layout = nowview_layout();
        let svg = render_nowview_svg(&layout, "15:30", &[]);
        // Dayview envelope intact.
        assert!(svg.starts_with("<svg "));
        assert!(svg.ends_with("</svg>"));
        // Field bands rendered by dayview path.
        assert!(svg.contains(r#"data-field="mood""#));
        // Now-line drawn by dayview (now_ms = day.end_ms).
        assert!(svg.contains("current-time-line"));
        // Overlay clock label injected inside the plot group.
        assert!(svg.contains("current-time-text"));
        assert!(svg.contains("15:30"));
        // Overlay must sit before the SVG's closing tags.
        let label_idx = svg.find("current-time-text").unwrap();
        let svg_close = svg.rfind("</svg>").unwrap();
        assert!(label_idx < svg_close);
    }

    #[test]
    fn no_current_time_label_means_no_overlay() {
        let layout = nowview_layout();
        let svg = render_nowview_svg(&layout, "", &[]);
        assert!(!svg.contains("current-time-text"));
        assert!(!svg.contains("nowview-overlay"));
    }

    #[test]
    fn now_strip_paints_one_chunk_per_field_in_a_state() {
        // Build a layout with reserved right-panel width, then assert the
        // overlay matches each entry to the field band by name.
        let now_ms = 1_700_000_000_000_i64;
        let history_ms = 30 * 60 * 1000;
        let day = DayviewDayInput {
            label_lines: vec![],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            has_error: false,
            fields: vec![
                DayviewFieldInput {
                    field: "mood".into(),
                    y_frac: 0.1,
                    height_frac: 0.2,
                    z_index: 0.1,
                    blocks: vec![DayviewBlock {
                        start_ms: now_ms - 10 * 60 * 1000,
                        end_ms: now_ms,
                        fill: BlockFill::Single("#abcdef".into()),
                        state_label: Some("focused".into()),
                    }],
                    points: vec![],
                },
                DayviewFieldInput {
                    field: "coffee".into(),
                    y_frac: 0.5,
                    height_frac: 0.2,
                    z_index: 0.5,
                    blocks: vec![],
                    points: vec![DayviewPoint {
                        time_ms: now_ms - 5 * 60 * 1000,
                        fg_color: "#ff0000".into(),
                        bg_color: "#000000".into(),
                        state_label: None,
                    }],
                },
            ],
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms,
            width: 600.0,
            height: Some(300.0),
            day_height: 250.0,
            interday_margin: 0.0,
            time_axis_height: 20.0,
            margin: 2.0,
            label_width: Some(0.0),
            orientation: Orientation::Horizontal,
            axis_ticks_override: Some(nowview_axis_ticks(400.0, 30.0)),
            right_panel_width: 120.0,
        };
        let layout = compute_dayview(&cfg);
        // Right panel reserved → time axis shrinks by 120 px.
        assert!(
            (layout.time_axis_extent - (596.0 - 120.0)).abs() < 1e-6,
            "time_axis_extent = {}",
            layout.time_axis_extent,
        );
        let entries = vec![
            NowviewCurrentState {
                field: "mood".into(),
                state_label: Some("focused".into()),
                colors: vec!["#abcdef".into()],
            },
            NowviewCurrentState {
                field: "coffee".into(),
                state_label: None,
                colors: vec!["#ff0000".into()],
            },
        ];
        let svg = render_nowview_svg(&layout, "12:00", &entries);
        // The strip carries one chunk for the field that is in a state,
        // spanning the whole reserved panel at that field's band.
        let mood = field_group(&svg, "mood");
        let chunk = rect_attrs(&mood, "now-chunk").expect("mood now-chunk");
        assert!((chunk.x - (layout.label_width + layout.time_axis_extent)).abs() < 1e-6);
        assert!((chunk.w - layout.right_panel_width).abs() < 1e-6);
        let band = &layout.days[0]
            .fields
            .iter()
            .find(|f| f.field == "mood")
            .unwrap();
        assert!((chunk.y - band.cross_offset).abs() < 1e-6);
        assert!((chunk.h - band.cross_extent).abs() < 1e-6);
        assert!(mood.contains(r##"fill="#abcdef""##));
        // The occur-only field has no state at now: no chunk, and no text.
        assert!(rect_attrs(&field_group(&svg, "coffee"), "now-chunk").is_none());
        // No label text anywhere but the axis: the panel is gone.
        assert!(!svg.contains("mood: focused"));
        assert!(!svg.contains(">coffee<"));
    }

    #[test]
    fn now_strip_skips_when_field_not_in_layout() {
        let now_ms = 1_700_000_000_000_i64;
        let history_ms = 30 * 60 * 1000;
        let day = DayviewDayInput {
            label_lines: vec![],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            has_error: false,
            fields: vec![DayviewFieldInput {
                field: "mood".into(),
                y_frac: 0.1,
                height_frac: 0.2,
                z_index: 0.1,
                blocks: vec![DayviewBlock {
                    start_ms: now_ms - 10 * 60 * 1000,
                    end_ms: now_ms,
                    fill: BlockFill::Single("#abcdef".into()),
                    state_label: None,
                }],
                points: vec![],
            }],
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms,
            width: 500.0,
            height: Some(300.0),
            day_height: 250.0,
            time_axis_height: 20.0,
            margin: 2.0,
            label_width: Some(0.0),
            orientation: Orientation::Horizontal,
            axis_ticks_override: Some(nowview_axis_ticks(300.0, 30.0)),
            right_panel_width: 120.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let entries = vec![NowviewCurrentState {
            field: "ghost".into(),
            state_label: Some("nope".into()),
            colors: vec!["#888888".into()],
        }];
        let svg = render_nowview_svg(&layout, "", &entries);
        assert!(!svg.contains(r#"data-field="ghost""#));
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

    /// Per da-9vop a parallel state set keeps every member's colour; the
    /// chunk past the now-line shows them as the same 45° stripes dayview
    /// already paints a parallel-state block with (da-nk84).
    #[test]
    fn now_chunk_stripes_a_parallel_state_set() {
        let now_ms = 1_700_000_000_000_i64;
        let history_ms = 30 * 60 * 1000;
        let day = DayviewDayInput {
            label_lines: vec![],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            has_error: false,
            fields: vec![DayviewFieldInput {
                field: "body".into(),
                y_frac: 0.1,
                height_frac: 0.2,
                z_index: 0.1,
                blocks: vec![DayviewBlock {
                    start_ms: now_ms - 10 * 60 * 1000,
                    end_ms: now_ms,
                    fill: BlockFill::Single("#aa3333".into()),
                    state_label: Some("vertical,sit".into()),
                }],
                points: vec![],
            }],
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms,
            width: 600.0,
            height: Some(300.0),
            day_height: 250.0,
            time_axis_height: 20.0,
            margin: 2.0,
            label_width: Some(0.0),
            orientation: Orientation::Horizontal,
            axis_ticks_override: Some(nowview_axis_ticks(400.0, 30.0)),
            right_panel_width: 120.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        // Parallel state set: vertical + sit → two-colour swatch.
        let entries = vec![NowviewCurrentState {
            field: "body".into(),
            state_label: Some("vertical,sit".into()),
            colors: vec!["#aa3333".into(), "#33aa33".into()],
        }];
        let svg = render_nowview_svg(&layout, "12:00", &entries);
        // The chunk is striped the way dayview stripes a parallel-state
        // block, sharing that machinery rather than inventing a swatch.
        assert!(
            field_group(&svg, "body").contains(r#"class="now-chunk multi-state""#),
            "expected a striped now-chunk in: {svg}"
        );
        assert!(svg.contains(r#"patternTransform="rotate(45)""#));
        assert!(svg.contains(r##"fill="#aa3333""##));
        assert!(svg.contains(r##"fill="#33aa33""##));
    }

    /// Single-state input renders as one solid-fill rect — the
    /// parallel-state path must not regress the common case.
    #[test]
    fn now_chunk_single_state_is_solid() {
        let now_ms = 1_700_000_000_000_i64;
        let history_ms = 30 * 60 * 1000;
        let day = DayviewDayInput {
            label_lines: vec![],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            has_error: false,
            fields: vec![DayviewFieldInput {
                field: "mood".into(),
                y_frac: 0.1,
                height_frac: 0.2,
                z_index: 0.1,
                blocks: vec![DayviewBlock {
                    start_ms: now_ms - 10 * 60 * 1000,
                    end_ms: now_ms,
                    fill: BlockFill::Single("#abcdef".into()),
                    state_label: Some("focused".into()),
                }],
                points: vec![],
            }],
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms,
            width: 600.0,
            height: Some(300.0),
            day_height: 250.0,
            time_axis_height: 20.0,
            margin: 2.0,
            label_width: Some(0.0),
            orientation: Orientation::Horizontal,
            axis_ticks_override: Some(nowview_axis_ticks(400.0, 30.0)),
            right_panel_width: 120.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let entries = vec![NowviewCurrentState {
            field: "mood".into(),
            state_label: Some("focused".into()),
            colors: vec!["#abcdef".into()],
        }];
        let svg = render_nowview_svg(&layout, "12:00", &entries);
        assert!(
            field_group(&svg, "mood").contains(r##"<rect class="now-chunk" x="##),
            "single-state chunk must be a solid rect: {svg}"
        );
        assert!(
            !svg.contains("multi-state"),
            "no stripes for a single state"
        );
    }
}
