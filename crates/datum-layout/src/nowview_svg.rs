//! Thin overlay wrapper around the dayview SVG renderer (per bead da-6jy).
//!
//! Per bead da-6jy the nowview shares dayview's render path: the CLI builds
//! a single-day [`crate::dayview::DayviewConfig`] scoped to the last
//! `history_minutes`, calls [`crate::dayview::compute_dayview`], and the
//! dayview renderer paints field bands + gridlines + the yellow now-line.
//! This module adds the nowview-specific overlay on top — currently just
//! the `HH:MM` clock label near the now-line.
//!
//! Pure-Rust, WASM-clean.

use std::fmt::Write;

use crate::dayview::{DayviewLayout, Orientation};
use crate::dayview_svg::render_dayview_svg;
use crate::nowview::NowviewPanelEntry;

const TEXT_COLOR: &str = "white";
/// Side margin (px) between the now-line and the first column of panel
/// content (and between panel content and the SVG's right edge).
const PANEL_PAD_PX: f64 = 6.0;
/// Width (px) of the colored swatch that prefixes each panel row.
const PANEL_SWATCH_W: f64 = 8.0;
/// Minimum panel-row font size; rows below this won't render text.
const PANEL_MIN_FONT: f64 = 7.0;
/// Cap on panel-row font size so labels stay readable in tall single-day
/// nowviews where the band height alone would otherwise pick a huge font.
const PANEL_MAX_FONT: f64 = 14.0;

/// Render `layout` (a dayview layout the CLI built for the nowview window)
/// to an SVG document string and overlay the `current_time_label` near the
/// now-line plus, when non-empty, a current-state panel right of the
/// now-line (per bead da-34w). An empty `current_time_label` skips the
/// clock overlay; an empty `panel_entries` slice skips the panel.
pub fn render_nowview_svg(
    layout: &DayviewLayout,
    current_time_label: &str,
    panel_entries: &[NowviewPanelEntry],
) -> String {
    let mut svg = render_dayview_svg(layout);
    let mut overlay = String::new();
    if !panel_entries.is_empty() {
        overlay.push_str(&build_state_panel_overlay(layout, panel_entries));
    }
    if !current_time_label.is_empty() {
        overlay.push_str(&build_current_time_overlay(layout, current_time_label));
    }
    if !overlay.is_empty() {
        inject_inside_plot_group(&mut svg, &overlay);
    }
    svg
}

/// Build the `<text>` overlay placed near the now-line. Anchored to the
/// right edge of the data area (where the now-line sits since
/// `now_ms == day.end_ms` for the nowview window). Coordinates are in
/// plot-local space (the renderer's outer `<g class="plot">` translates by
/// `margin`).
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

    // Font size scales with the time-axis strip but stays readable when the
    // strip is suppressed (callers may pass `time_axis_height = 0`).
    let font_size = if layout.time_axis_height > 0.0 {
        (layout.time_axis_height + 2.0).min(layout.time_axis_height * 1.2)
    } else {
        12.0
    };

    let mut out = String::new();
    let _ = write!(
        &mut out,
        r#"<g class="nowview-overlay">"#,
    );
    match layout.orientation {
        Orientation::Horizontal => {
            // Centered just under the right edge of the data area, at the
            // mid-line of the time-axis strip (so it reads as the now-line's
            // clock label).
            let text_x = data_x + layout.time_axis_extent;
            let text_y = data_y
                + cross_total
                + if layout.time_axis_height > 0.0 {
                    layout.time_axis_height * 0.65
                } else {
                    font_size
                };
            let _ = write!(
                &mut out,
                r#"<text class="current-time-text" x="{x}" y="{y}" text-anchor="end" dominant-baseline="middle" fill="{c}" font-size="{fs}" font-weight="bold">{label}</text>"#,
                x = text_x,
                y = text_y,
                c = TEXT_COLOR,
                fs = font_size,
                label = escape_xml(label),
            );
        }
        Orientation::Vertical => {
            // Right-anchored against the time-axis strip at the now-line
            // (which sits at the bottom edge of the data area).
            let text_x = data_x - 2.0;
            let text_y = data_y + layout.time_axis_extent + font_size;
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

/// Build the right-of-now-line current-state panel. Each entry is matched
/// to a rendered field band by name; rows whose field isn't in the layout
/// are dropped (the dayview pipeline already skipped them). Vertical mode
/// is unsupported — panel only renders in horizontal mode (the only mode
/// nowview uses today).
fn build_state_panel_overlay(
    layout: &DayviewLayout,
    entries: &[NowviewPanelEntry],
) -> String {
    if matches!(layout.orientation, Orientation::Vertical) {
        return String::new();
    }
    if layout.right_panel_width <= 0.0 || layout.days.is_empty() {
        return String::new();
    }
    let day = &layout.days[0];
    if day.fields.is_empty() {
        return String::new();
    }

    let panel_x = layout.label_width + layout.time_axis_extent;
    let usable_w = (layout.right_panel_width - 2.0 * PANEL_PAD_PX).max(0.0);
    if usable_w <= PANEL_SWATCH_W {
        return String::new();
    }

    let mut out = String::new();
    let _ = write!(&mut out, r#"<g class="nowview-state-panel">"#);
    for entry in entries {
        let Some(field) = day.fields.iter().find(|f| f.field == entry.field) else {
            continue;
        };
        let row_top = day.day_offset + field.cross_offset;
        let row_h = field.cross_extent.max(1.0);
        let y_center = row_top + row_h / 2.0;

        let font_size = (row_h - 4.0)
            .min(PANEL_MAX_FONT)
            .max(PANEL_MIN_FONT);
        if font_size < PANEL_MIN_FONT {
            continue;
        }

        let swatch_h = (row_h * 0.6).min(font_size).max(4.0);
        let swatch_x = panel_x + PANEL_PAD_PX;
        let swatch_y = y_center - swatch_h / 2.0;
        push_state_swatch(
            &mut out,
            &entry.field,
            &entry.colors,
            swatch_x,
            swatch_y,
            PANEL_SWATCH_W,
            swatch_h,
        );

        let text_x = swatch_x + PANEL_SWATCH_W + 4.0;
        let label = match &entry.state_label {
            Some(s) => format!("{}: {}", entry.field, s),
            None => entry.field.clone(),
        };
        let _ = write!(
            &mut out,
            r#"<text class="state-label" data-field="{f}" x="{x}" y="{y}" dominant-baseline="middle" fill="{c}" font-size="{fs}">{txt}</text>"#,
            f = escape_xml(&entry.field),
            x = text_x,
            y = y_center,
            c = TEXT_COLOR,
            fs = font_size,
            txt = escape_xml(&label),
        );
    }
    out.push_str("</g>");
    out
}

/// Per da-9vop the swatch density (cells per side) for the parallel-state
/// checkerboard. 8 → 8×8 cell grid for 2-state input; for N≥2 states the
/// cells cycle by `(row + col) % N` so every state is visible and the
/// pattern reads as a regular tiling.
const PANEL_SWATCH_CHECKER_CELLS: usize = 8;

/// Push the `<rect class="state-swatch">` element(s). Single-state entries
/// emit one solid rect (the original behaviour); parallel-state entries
/// (`switch body vertical,sit`) emit an N×N checkerboard of small rects so
/// each state's colour is visible inside the swatch (per da-9vop).
fn push_state_swatch(
    out: &mut String,
    field: &str,
    colors: &[String],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let escaped_field = escape_xml(field);
    match colors {
        [] => {
            // Defensive: no resolvable colour → no swatch (still emit the
            // bounding rect transparently so DOM consumers see the slot).
            let _ = write!(
                out,
                r#"<rect class="state-swatch" data-field="{f}" x="{x}" y="{y}" width="{w}" height="{h}" fill="none"/>"#,
                f = escaped_field,
                x = x,
                y = y,
                w = w,
                h = h,
            );
        }
        [single] => {
            let _ = write!(
                out,
                r#"<rect class="state-swatch" data-field="{f}" x="{x}" y="{y}" width="{w}" height="{h}" fill="{c}"/>"#,
                f = escaped_field,
                x = x,
                y = y,
                w = w,
                h = h,
                c = escape_xml(single),
            );
        }
        _ => {
            let n = PANEL_SWATCH_CHECKER_CELLS;
            // Wrap every cell in a single grouping rect so DOM consumers
            // can still address the swatch as one element.
            let _ = write!(
                out,
                r#"<g class="state-swatch state-swatch-checker" data-field="{f}">"#,
                f = escaped_field,
            );
            let cell_w = w / n as f64;
            let cell_h = h / n as f64;
            for row in 0..n {
                for col in 0..n {
                    let idx = (row + col) % colors.len();
                    let cx = x + col as f64 * cell_w;
                    let cy = y + row as f64 * cell_h;
                    let _ = write!(
                        out,
                        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{c}"/>"#,
                        x = cx,
                        y = cy,
                        w = cell_w,
                        h = cell_h,
                        c = escape_xml(&colors[idx]),
                    );
                }
            }
            out.push_str("</g>");
        }
    }
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
    fn state_panel_renders_swatch_and_label_per_field() {
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
            NowviewPanelEntry {
                field: "mood".into(),
                state_label: Some("focused".into()),
                colors: vec!["#abcdef".into()],
            },
            NowviewPanelEntry {
                field: "coffee".into(),
                state_label: None,
                colors: vec!["#ff0000".into()],
            },
        ];
        let svg = render_nowview_svg(&layout, "12:00", &entries);
        assert!(svg.contains(r#"class="nowview-state-panel""#));
        assert!(svg.contains(r#"class="state-swatch" data-field="mood""#));
        assert!(svg.contains(r#"class="state-swatch" data-field="coffee""#));
        assert!(svg.contains("mood: focused"));
        // Occur-only field with no state label should still show the field
        // name as a glanceable presence cue.
        assert!(svg.contains(">coffee<"));
        // Both swatches use entry colors.
        assert!(svg.contains("#abcdef"));
        assert!(svg.contains("#ff0000"));
    }

    #[test]
    fn state_panel_skips_when_field_not_in_layout() {
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
        let entries = vec![NowviewPanelEntry {
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

    /// Per da-9vop, the following-state swatch for a parallel state set
    /// renders as an N×N checkerboard (one rect per cell, alternating
    /// between the supplied colours) wrapped in a single `<g>` so DOM
    /// consumers can still address it as one swatch.
    #[test]
    fn following_state_square_renders_checkerboard_for_parallel_set() {
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
        let entries = vec![NowviewPanelEntry {
            field: "body".into(),
            state_label: Some("vertical,sit".into()),
            colors: vec!["#aa3333".into(), "#33aa33".into()],
        }];
        let svg = render_nowview_svg(&layout, "12:00", &entries);
        // Single solid-fill swatch is replaced by a grouping <g> that
        // contains the per-cell rects.
        assert!(
            svg.contains(r#"class="state-swatch state-swatch-checker" data-field="body""#),
            "expected checker grouping element in: {svg}"
        );
        // Both state colours appear at least once each.
        assert!(svg.contains(r##"fill="#aa3333""##));
        assert!(svg.contains(r##"fill="#33aa33""##));
        // 8×8 cell grid → 64 small rects, one per cell.
        let cell_count = svg.matches("<rect x=\"").count();
        assert!(
            cell_count >= 64,
            "expected ≥64 checker cells, got {cell_count} in: {svg}",
        );
    }

    /// Single-state input continues to render as one solid-fill rect —
    /// the parallel-state path must not regress the common case.
    #[test]
    fn following_state_square_single_state_still_solid() {
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
        let entries = vec![NowviewPanelEntry {
            field: "mood".into(),
            state_label: Some("focused".into()),
            colors: vec!["#abcdef".into()],
        }];
        let svg = render_nowview_svg(&layout, "12:00", &entries);
        assert!(
            svg.contains(r#"<rect class="state-swatch" data-field="mood""#),
            "single-state swatch must remain a solid rect: {svg}"
        );
        assert!(!svg.contains("state-swatch-checker"), "no checker grouping for single state");
    }
}
