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
    axis_label_size, icon_id, now_chunk, render_dayview_svg_with, text_width, DayviewSvgOpts,
    FlowMark, RightPanelFill, AXIS_LABEL_GAP, NOW_LINE_COLOR,
};
use crate::nowview::NowviewCurrentState;

const TEXT_COLOR: &str = "white";
/// What [`now_label_overlay`] writes.
const NOW_LABEL: &str = "now";
/// Below this strip width the clock can't be centred inside the strip
/// without overflowing the canvas, so it centres on the now-line instead.
/// Mirrors `isNowWidthSmall` in the original datum's `nowview.ts`.
const CLOCK_CENTER_MIN_STRIP: f64 = 40.0;

/// Render `layout` (a dayview layout the CLI built for the nowview window)
/// to an SVG document string, then overlay the now strip: one chunk per
/// field carrying the state it is in at "now", drawn from the now-line to
/// the right edge, and the `HH:MM` `current_time_label` under them. An
/// empty `current_time_label` skips the clock, and `now_label` draws a
/// small «now» in its place (da-9g3bf); a field whose entry has no
/// `state_label` — it is in no state right now — gets no chunk.
pub fn render_nowview_svg(
    layout: &DayviewLayout,
    current_time_label: &str,
    current_states: &[NowviewCurrentState],
    labels: bool,
    now_label: bool,
) -> String {
    render_nowview_svg_with(
        layout,
        current_time_label,
        current_states,
        labels,
        now_label,
        None,
    )
}

/// [`render_nowview_svg`], drawn to be slid along with the clock when
/// `flow` is given (`--ahead`, da-hklaz): the instants it marks as they
/// flow past, none for a window too long to mark its minutes.
pub fn render_nowview_svg_with(
    layout: &DayviewLayout,
    current_time_label: &str,
    current_states: &[NowviewCurrentState],
    labels: bool,
    now_label: bool,
    flow: Option<&[FlowMark]>,
) -> String {
    let current_time_label = if now_label { "" } else { current_time_label };
    let fills = now_strip_fills(current_states);
    // The clock sits in the now strip; it only steals room from the tick
    // labels when it is wider than the strip (da-sv8c).
    let opts = DayviewSvgOpts {
        axis_end_reserved: clock_reserved_px(layout, current_time_label),
        axis_keep_out: strip_label_keep_out(layout, now_label.then_some(&NOW)),
        right_panel_fills: &fills,
        flow: flow.is_some(),
        flow_marks: flow.unwrap_or_default(),
        data_layer: false,
    };
    let (gutter, label_group) = label_gutter(layout, current_states, labels);
    let widened = widen_for(layout, gutter);
    let layout = widened.as_ref().unwrap_or(layout);
    let mut svg = render_dayview_svg_with(layout, opts);
    if !current_time_label.is_empty() {
        let overlay = build_current_time_overlay(layout, current_time_label);
        inject_inside_plot_group(&mut svg, &overlay);
    }
    if now_label {
        inject_inside_plot_group(&mut svg, &strip_label_overlay(layout, &NOW));
    }
    inject_inside_plot_group(&mut svg, &label_group);
    svg
}

/// Render a window placed anywhere but ending at now: centred on a selected
/// time (a time flag, da-pcis) or on wherever its viewer moved it. Its strip
/// is now's, at now when the window draws now, carrying `current_states`;
/// under it, `clock` or a «now» with `now_label`. The selected time,
/// `instant_ms`, is a dashed line in a timeline continuous either side of
/// it (da-dv45l, vividn: «For selected times, don't display the outset
/// block, just have the nowview as it would be with a dashed line and
/// continuous in time around it»).
pub fn render_nowview_at_svg(
    layout: &DayviewLayout,
    clock: &str,
    instant_ms: Option<i64>,
    current_states: &[NowviewCurrentState],
    labels: bool,
    now_label: bool,
) -> String {
    // `--labels` means one thing on every nowview picture (da-7q8r): the
    // states now, in a gutter past the right edge.
    let (gutter, label_group) = label_gutter(layout, current_states, labels);
    let widened = widen_for(layout, gutter);
    let layout = widened.as_ref().unwrap_or(layout);
    let fills = now_strip_fills(current_states);
    let label = if now_label {
        NOW
    } else {
        StripLabel {
            text: clock,
            class: "now-clock",
            fill: TEXT_COLOR,
        }
    };
    let opts = DayviewSvgOpts {
        axis_end_reserved: 0.0,
        axis_keep_out: strip_label_keep_out(layout, Some(&label)),
        right_panel_fills: &fills,
        ..Default::default()
    };
    let mut svg = render_dayview_svg_with(layout, opts);
    inject_inside_plot_group(&mut svg, &strip_label_overlay(layout, &label));
    if let Some(at) = instant_ms {
        inject_inside_plot_group(&mut svg, &instant_line(layout, at));
    }
    inject_inside_plot_group(&mut svg, &label_group);
    svg
}

/// The selected time's dashed line across the band, white over a dark
/// casing as the page draws a selection, saying its instant as a mark
/// does; nothing where the picture does not draw the instant. From the
/// picture's own edge, as datum.day's selection line is, so the two dash
/// alike where one is drawn over the other.
fn instant_line(layout: &DayviewLayout, at: i64) -> String {
    let Some(pos) = (layout.days.iter().flat_map(|d| &d.spans))
        .find(|s| s.start_ms <= at && at <= s.end_ms)
        .map(|s| {
            s.time_offset
                + (at - s.start_ms) as f64 / (s.end_ms - s.start_ms).max(1) as f64 * s.time_extent
        })
    else {
        return String::new();
    };
    let cross = layout.days.len() as f64 * layout.day_pitch - layout.interday_margin;
    let along = layout.label_width + pos;
    let edge = -layout.margin;
    let (x1, y1, x2, y2) = match layout.orientation {
        Orientation::Horizontal => (along, edge, along, cross),
        Orientation::Vertical => (edge, along, layout.time_axis_height + cross, along),
    };
    let iso = chrono::DateTime::from_timestamp_millis(at)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default();
    [("rgba(0, 0, 0, 0.6)", 3.5), ("#ffffff", 1.5)]
        .iter()
        .map(|(stroke, width)| {
            format!(
                r#"<line class="instant-line" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{stroke}" stroke-width="{width}" stroke-dasharray="3,3" pointer-events="none" data-at="{iso}"/>"#
            )
        })
        .collect()
}

/// The text under the strip, in the time-axis strip: a «now», or the clock.
struct StripLabel<'a> {
    text: &'a str,
    class: &'static str,
    fill: &'static str,
}

/// The small yellow «now» in place of the clock (da-9g3bf).
const NOW: StripLabel<'static> = StripLabel {
    text: NOW_LABEL,
    class: "now-label",
    fill: NOW_LINE_COLOR,
};

/// `label` under the strip's chunk, in the time-axis strip, and sized to lie
/// wholly between the chunk's two ends. Nothing when there is no strip, or
/// no room in it or in the axis strip to write it legibly.
fn strip_label_overlay(layout: &DayviewLayout, label: &StripLabel) -> String {
    let Some((middle, size)) = strip_label_at(layout, label.text) else {
        return String::new();
    };
    let cross_total = layout.days.len() as f64 * layout.day_pitch - layout.interday_margin;
    // Horizontally on the tick labels' baseline, as the clock is.
    let (x, y, anchor) = match layout.orientation {
        Orientation::Horizontal => (
            layout.label_width + middle,
            cross_total + layout.time_axis_height,
            r#"text-anchor="middle""#,
        ),
        Orientation::Vertical => (
            layout.time_axis_height - 1.0,
            layout.label_width + middle,
            r#"text-anchor="end" dominant-baseline="middle""#,
        ),
    };
    format!(
        r#"<text class="{class}" x="{x}" y="{y}" {anchor} fill="{fill}" font-size="{size}">{text}</text>"#,
        class = label.class,
        fill = label.fill,
        text = escape_xml(label.text),
    )
}

/// The now strip's place, empty (`nowview --overlay`, da-0e06p): a
/// `.now-strip` rect over the whole of it, across every band, for a viewer
/// that paints each field's state there itself. Nothing when the picture
/// draws no strip.
pub fn strip_slot(layout: &DayviewLayout) -> String {
    let Some(c) = layout.days.iter().find_map(|d| now_chunk(layout, d)) else {
        return String::new();
    };
    let cross = layout.days.len() as f64 * layout.day_pitch - layout.interday_margin;
    let (x, y, w, h) = match layout.orientation {
        Orientation::Horizontal => (layout.label_width + c.from, 0.0, c.to - c.from, cross),
        Orientation::Vertical => (
            layout.time_axis_height,
            layout.label_width + c.from,
            cross,
            c.to - c.from,
        ),
    };
    format!(
        r#"<rect class="now-strip" x="{x}" y="{y}" width="{w}" height="{h}" fill="none" pointer-events="none"/>"#
    )
}

/// Where along the time axis [`strip_label_overlay`] centres `text`, and
/// its font size: `None` when it writes none.
fn strip_label_at(layout: &DayviewLayout, text: &str) -> Option<(f64, f64)> {
    let c = layout.days.iter().find_map(|d| now_chunk(layout, d))?;
    let room = c.to - c.from;
    let size = axis_label_size(layout.orientation, layout.time_axis_height);
    let size = match layout.orientation {
        Orientation::Horizontal => size.min(room / text_width(text, 1.0)),
        Orientation::Vertical => size
            .min(room)
            .min((layout.time_axis_height - 2.0) / text_width(text, 1.0)),
    };
    // Smaller than any text datum draws is too small to read: none.
    (size >= 6.0).then_some(((c.from + c.to) / 2.0, size))
}

/// The stretch of the time axis the label under the strip takes, which the
/// tick labels keep off (da-ljs7b), as they keep off the clock it stands in
/// for.
fn strip_label_keep_out(layout: &DayviewLayout, label: Option<&StripLabel>) -> Option<(f64, f64)> {
    let label = label?;
    let (middle, size) = strip_label_at(layout, label.text)?;
    let half = match layout.orientation {
        Orientation::Horizontal => text_width(label.text, size),
        Orientation::Vertical => size,
    } / 2.0;
    Some((middle - half, middle + half))
}

/// da-7q8r: the labels are off unless `--labels` asks for them, and when it
/// does they sit in a gutter to the RIGHT of the picture — never over the
/// colour they name, which is what sent them behind a flag.
fn label_gutter(
    layout: &DayviewLayout,
    current_states: &[NowviewCurrentState],
    labels: bool,
) -> (f64, String) {
    if labels {
        state_labels(layout, current_states)
    } else {
        (0.0, String::new())
    }
}

/// `layout` with room for a `gutter`-wide strip past the far end of its
/// time axis — right of it, or below it when time runs down (da-78evn) —
/// or `None` when there is no gutter to make room for. Only the canvas
/// grows: every band's geometry is already resolved.
fn widen_for(layout: &DayviewLayout, gutter: f64) -> Option<DayviewLayout> {
    (gutter > 0.0).then(|| match layout.orientation {
        Orientation::Horizontal => DayviewLayout {
            width: layout.width + gutter,
            ..layout.clone()
        },
        Orientation::Vertical => DayviewLayout {
            height: layout.height + gutter,
            ..layout.clone()
        },
    })
}

/// `--labels` (da-7q8r): each field's state named beside its lane, in a
/// gutter past the picture's far edge — right of it, or below it and
/// written downwards when time runs down (da-78evn) — in the color its
/// now chunk is painted (da-8oksz). Beside them, a key of the occur fields
/// with a dot in the window, each its dot's mark and color, then its name.
/// Returns how much room that gutter needs and the `<g>` to inject — the
/// caller widens the canvas by the first and injects the second, so no
/// label ever lies over a band.
///
/// A gutter of `0.0` means there was nothing to label.
fn state_labels(layout: &DayviewLayout, current_states: &[NowviewCurrentState]) -> (f64, String) {
    let Some(day) = layout.days.first() else {
        return (0.0, String::new());
    };
    // Past the data area AND the now strip: the far edge of everything
    // the dayview renderer drew, along the time axis.
    let far =
        layout.label_width + layout.time_axis_extent + layout.right_panel_width + AXIS_LABEL_GAP;
    let mut widest: f64 = 0.0;
    let mut labelled = Vec::new();
    let mut out = String::from(r#"<g class="nowview-labels">"#);
    for state in current_states {
        let Some(label) = &state.state_label else {
            continue;
        };
        let Some(f) = day.fields.iter().find(|f| f.field == state.field) else {
            continue;
        };
        labelled.push(f.field.as_str());
        let font_size = (f.cross_extent * 0.7).min(12.0);
        let text = format!("{}: {label}", state.field);
        widest = widest.max(text_width(&text, font_size));
        let lane = day.day_offset + f.cross_offset + f.cross_extent / 2.0;
        let color = (state.colors.iter())
            .find(|c| c.as_str() != "none")
            .map_or(TEXT_COLOR, String::as_str);
        let (x, y, turn) = match layout.orientation {
            Orientation::Horizontal => (far, lane, String::new()),
            Orientation::Vertical => {
                let x = layout.time_axis_height + lane;
                (x, far, format!(r#" transform="rotate(90 {x} {far})""#))
            }
        };
        let _ = write!(
            &mut out,
            r#"<text class="state-label" x="{x}" y="{y}"{turn} dy="0.35em" fill="{color}" stroke="black" stroke-width="2" paint-order="stroke" font-size="{font_size}">{t}</text>"#,
            t = escape_xml(&text),
        );
    }

    // The key: the fields with no state row whose dots the window holds, in
    // their lanes' order, each with the first such dot as it is drawn. A
    // moment is a dotted line, no dot, unless a glyph or icon draws it.
    let mut dotted: Vec<_> = (day.fields.iter())
        .filter(|f| !labelled.contains(&f.field.as_str()))
        .filter_map(|f| {
            let icon = (f.icon.as_deref()).is_some_and(|svg| svg.starts_with("<svg"));
            let p =
                (f.points.iter()).find(|p| !p.moment || p.glyph.is_some() || (p.icon && icon))?;
            Some((f, p, p.icon && icon))
        })
        .collect();
    dotted.sort_by(|a, b| a.0.cross_offset.total_cmp(&b.0.cross_offset));
    let font_size = (layout.day_height / dotted.len().max(1) as f64 / 1.4).min(12.0);
    let line = font_size * 1.4;
    let r = font_size * 0.45;
    let along = far
        + if widest > 0.0 {
            widest + AXIS_LABEL_GAP
        } else {
            0.0
        };
    // A key row at `along` the time axis and `cross` across it, as a state
    // label is placed.
    let at = |along: f64, cross: f64| match layout.orientation {
        Orientation::Horizontal => format!("translate({along} {cross})"),
        Orientation::Vertical => format!(
            "translate({} {along}) rotate(90)",
            layout.time_axis_height + cross
        ),
    };
    let mut key_widest: f64 = 0.0;
    for (i, (f, p, icon)) in dotted.iter().enumerate() {
        let c = &p.fg_color;
        let mark = if *icon {
            format!(
                r##"<use href="#{id}" x="0" y="{y}" width="{d}" height="{d}" color="{c}"/>"##,
                id = icon_id(&f.field),
                y = -r,
                d = 2.0 * r,
            )
        } else if let Some(g) = &p.glyph {
            format!(
                r#"<text x="{r}" dy="0.35em" text-anchor="middle" font-size="{fs}" fill="{c}">{g}</text>"#,
                fs = r * 1.6,
                g = escape_xml(g),
            )
        } else {
            format!(r#"<circle cx="{r}" r="{r}" fill="{c}"/>"#)
        };
        key_widest = key_widest.max(2.0 * r + AXIS_LABEL_GAP + text_width(&f.field, font_size));
        let _ = write!(
            &mut out,
            r#"<g class="occur-key" data-field="{name}" transform="{t}">{mark}<text x="{x}" dy="0.35em" fill="{c}" stroke="black" stroke-width="2" paint-order="stroke" font-size="{font_size}">{name}</text></g>"#,
            name = escape_xml(&f.field),
            t = at(along, day.day_offset + line * (i as f64 + 0.5)),
            x = 2.0 * r + AXIS_LABEL_GAP,
        );
    }
    out.push_str("</g>");
    let gutter = if dotted.is_empty() {
        widest
    } else {
        along - far + key_widest
    };
    if gutter <= 0.0 {
        return (0.0, String::new());
    }
    (AXIS_LABEL_GAP + gutter, out)
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
                id: e.id.clone(),
            })
        })
        .collect()
}

/// Font size of the clock overlay: its tick labels' size (da-78evn,
/// da-a2cxp), and readable when the strip is suppressed (callers may pass
/// `time_axis_height = 0`).
fn clock_font_size(layout: &DayviewLayout) -> f64 {
    match axis_label_size(layout.orientation, layout.time_axis_height) {
        s if s > 0.0 => s,
        _ => 12.0,
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
            // On the tick labels' baseline, so it reads as the now-line's
            // own tick label.
            let text_x = data_x + time_pos;
            let text_y = data_y
                + cross_total
                + if layout.time_axis_height > 0.0 {
                    layout.time_axis_height
                } else {
                    font_size
                };
            let _ = write!(
                &mut out,
                r#"<text class="current-time-text" x="{x}" y="{y}" text-anchor="middle" fill="{c}" font-size="{fs}" font-weight="bold">{label}</text>"#,
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
pub fn inject_inside_plot_group(svg: &mut String, overlay: &str) {
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
        compute_dayview, BlockFill, DayviewAxisTick, DayviewBlock, DayviewConfig, DayviewDayInput,
        DayviewFieldInput, DayviewPoint, Orientation,
    };
    use crate::dayview_svg::NOW_GAP;
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
            date: None,
            label_lines: vec!["Mon".into()],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            errors: Vec::new(),
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
                    id: None,
                    title: None,
                }],
                points: vec![DayviewPoint {
                    time_ms: now_ms - 10 * 60 * 1000,
                    fg_color: "#ff0000".into(),
                    state_label: None,
                    id: None,
                    title: None,
                    glyph: None,
                    icon: false,
                    moment: false,
                }],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans: Vec::new(),
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
                500.0, 30.0, 15.0,
            )),
            right_panel_width: 0.0,
        };
        compute_dayview(&cfg)
    }

    #[test]
    fn renders_dayview_envelope_with_current_time() {
        let layout = nowview_layout();
        let svg = render_nowview_svg(&layout, "15:30", &[], false, false);
        // Dayview envelope intact, its text not selectable (da-mu5op).
        assert!(svg.starts_with("<svg "));
        let root = &svg[..svg.find('>').unwrap()];
        assert!(root.contains(&format!(r#"style="{}""#, crate::dayview_svg::NO_SELECT)));
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
        let svg = render_nowview_svg(&layout, "", &[], false, false);
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
            date: None,
            label_lines: vec![],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            errors: Vec::new(),
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
                        id: None,
                        title: None,
                    }],
                    points: vec![],
                    icon: None,
                    glyph_size: Default::default(),
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
                        state_label: None,
                        id: None,
                        title: None,
                        glyph: None,
                        icon: false,
                        moment: false,
                    }],
                    icon: None,
                    glyph_size: Default::default(),
                },
            ],
            spans: Vec::new(),
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
            axis_ticks_override: Some(nowview_axis_ticks(400.0, 30.0, 20.0)),
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
                id: None,
            },
            NowviewCurrentState {
                field: "coffee".into(),
                state_label: None,
                colors: vec!["#ff0000".into()],
                id: None,
            },
        ];
        let svg = render_nowview_svg(&layout, "12:00", &entries, false, false);
        // The strip carries one chunk for the field that is in a state,
        // a gap past now and on to the picture's edge, at that field's band.
        let mood = field_group(&svg, "mood");
        let chunk = rect_attrs(&mood, "now-chunk").expect("mood now-chunk");
        let now = layout.label_width + layout.time_axis_extent;
        assert!((chunk.x - (now + NOW_GAP)).abs() < 1e-6);
        assert!((chunk.w - (layout.right_panel_width - NOW_GAP)).abs() < 1e-6);
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

    /// da-8oksz: a label is painted its field's color, and beside the
    /// labels a key names exactly the occur fields with a dot in the
    /// window, each with its dot's mark in its color: a dot before the
    /// window keys nothing. Either way time runs.
    #[test]
    fn labels_take_their_color_and_key_the_dots_in_view() {
        for orientation in [Orientation::Horizontal, Orientation::Vertical] {
            labels_and_key(orientation);
        }
    }

    fn labels_and_key(orientation: Orientation) {
        let now_ms = 1_700_000_000_000_i64;
        let occur = |field: &str, at: i64, color: &str, glyph: Option<&str>| DayviewFieldInput {
            field: field.into(),
            y_frac: 0.5,
            height_frac: 0.1,
            z_index: 0.5,
            blocks: vec![],
            points: vec![DayviewPoint {
                time_ms: at,
                fg_color: color.into(),
                state_label: None,
                id: None,
                title: None,
                glyph: glyph.map(Into::into),
                icon: false,
                moment: false,
            }],
            icon: None,
            glyph_size: Default::default(),
        };
        let day = DayviewDayInput {
            date: None,
            label_lines: vec![],
            start_ms: now_ms - 60_000,
            end_ms: now_ms,
            errors: Vec::new(),
            fields: vec![
                DayviewFieldInput {
                    field: "mood".into(),
                    y_frac: 0.1,
                    height_frac: 0.2,
                    z_index: 0.1,
                    blocks: vec![DayviewBlock {
                        start_ms: now_ms - 60_000,
                        end_ms: now_ms,
                        fill: BlockFill::Single("#abcdef".into()),
                        state_label: Some("focused".into()),
                        id: None,
                        title: None,
                    }],
                    points: vec![],
                    icon: None,
                    glyph_size: Default::default(),
                },
                occur("coffee", now_ms - 30_000, "#123456", None),
                occur("water", now_ms - 10_000, "#654321", Some("~")),
                occur("tea", now_ms - 120_000, "#0f0f0f", None),
                {
                    let mut pic = occur("pic", now_ms - 20_000, "#00ff00", None);
                    pic.icon =
                        Some(r#"<svg viewBox="0 0 1 1"><rect width="1" height="1"/></svg>"#.into());
                    pic.points[0].icon = true;
                    pic
                },
            ],
            spans: Vec::new(),
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms,
            width: 100.0,
            height: Some(300.0),
            day_height: 250.0,
            time_axis_height: 0.0,
            margin: 2.0,
            label_width: Some(0.0),
            orientation,
            axis_ticks_override: Some(Vec::new()),
            right_panel_width: 96.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let entries = vec![
            NowviewCurrentState {
                field: "mood".into(),
                state_label: Some("focused".into()),
                colors: vec!["#abcdef".into()],
                id: None,
            },
            NowviewCurrentState {
                field: "coffee".into(),
                state_label: None,
                colors: vec![],
                id: None,
            },
        ];
        let svg = render_nowview_svg(&layout, "", &entries, true, false);
        let labels: Vec<&str> = svg.split(r#"<text class="state-label" "#).skip(1).collect();
        assert_eq!(labels.len(), 1, "{svg}");
        let label = &labels[0][..labels[0].find('>').unwrap()];
        assert!(label.contains(r##"fill="#abcdef""##), "{label}");

        let keyed: Vec<&str> = (svg.split(r#"<g class="occur-key" data-field=""#).skip(1))
            .map(|rest| &rest[..rest.find('"').unwrap()])
            .collect();
        assert_eq!(keyed, ["coffee", "water", "pic"], "{svg}");
        let row = |field: &str| {
            let open = format!(r#"<g class="occur-key" data-field="{field}""#);
            let at = svg.find(&open).expect("a key row");
            svg[at..at + svg[at..].find("</g>").unwrap()].to_string()
        };
        let coffee = row("coffee");
        assert!(coffee.contains(r##"<circle cx="##), "{coffee}");
        assert!(coffee.contains(r##"fill="#123456""##), "{coffee}");
        assert!(coffee.contains(">coffee</text>"), "{coffee}");
        let water = row("water");
        assert!(water.contains(r##"fill="#654321">~</text>"##), "{water}");
        // The icon's key mark uses the icon the dot wrote in `<defs>`.
        let pic = row("pic");
        assert!(pic.contains(r##"<use href="#icon-pic""##), "{pic}");
        assert!(svg.contains(r#"<svg id="icon-pic""#), "{svg}");
        // Written downwards when time runs down, as the labels are.
        let turned = matches!(orientation, Orientation::Vertical);
        assert_eq!(coffee.contains("rotate(90)"), turned, "{coffee}");

        // No labels asked for, no key either.
        let bare = render_nowview_svg(&layout, "", &entries, false, false);
        assert!(!bare.contains("occur-key"), "{bare}");
    }

    /// da-7q8r: the labels are `--labels`'s alone, and they sit in a
    /// gutter PAST every band — the picture grows to hold them rather
    /// than them lying over the colour they name.
    #[test]
    fn labels_are_asked_for_and_land_off_to_the_right() {
        let now_ms = 1_700_000_000_000_i64;
        let day = DayviewDayInput {
            date: None,
            label_lines: vec![],
            start_ms: now_ms - 60_000,
            end_ms: now_ms,
            errors: Vec::new(),
            fields: vec![DayviewFieldInput {
                field: "mood".into(),
                y_frac: 0.1,
                height_frac: 0.2,
                z_index: 0.1,
                blocks: vec![DayviewBlock {
                    start_ms: now_ms - 60_000,
                    end_ms: now_ms,
                    fill: BlockFill::Single("#abcdef".into()),
                    state_label: Some("focused".into()),
                    id: None,
                    title: None,
                }],
                points: vec![],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans: Vec::new(),
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms,
            width: 100.0,
            height: Some(300.0),
            day_height: 250.0,
            time_axis_height: 0.0,
            margin: 2.0,
            label_width: Some(0.0),
            orientation: Orientation::Horizontal,
            axis_ticks_override: Some(Vec::new()),
            right_panel_width: 96.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let entries = vec![NowviewCurrentState {
            field: "mood".into(),
            state_label: Some("focused".into()),
            colors: vec!["#abcdef".into()],
            id: None,
        }];

        // Unasked for, no label and no gutter: the canvas is the layout's.
        let bare = render_nowview_svg(&layout, "", &entries, false, false);
        assert!(!bare.contains("state-label"), "{bare}");
        assert!(bare.contains(r#"width="100""#), "{bare}");

        // Asked for, the text starts past the band and the canvas grew.
        let svg = render_nowview_svg(&layout, "", &entries, true, false);
        assert!(svg.contains(">mood: focused</text>"), "{svg}");
        let band_end = layout.label_width + layout.time_axis_extent + layout.right_panel_width;
        let key = r#"<text class="state-label" x=""#;
        let a = svg.find(key).map(|i| i + key.len()).expect("a label");
        let b = svg[a..].find('"').expect("x close") + a;
        let label_x: f64 = svg[a..b].parse().expect("number");
        assert!(
            label_x >= band_end,
            "label at {label_x}, band ends at {band_end}"
        );
        assert!(
            !svg.contains(r#"width="100""#),
            "the canvas did not grow: {svg}"
        );

        // Nothing in a state is nothing to label — and no gutter for it.
        let none = vec![NowviewCurrentState {
            field: "mood".into(),
            state_label: None,
            colors: vec![],
            id: None,
        }];
        let empty = render_nowview_svg(&layout, "", &none, true, false);
        assert!(!empty.contains("nowview-labels"), "{empty}");
        assert!(empty.contains(r#"width="100""#), "{empty}");
    }

    #[test]
    fn now_strip_skips_when_field_not_in_layout() {
        let now_ms = 1_700_000_000_000_i64;
        let history_ms = 30 * 60 * 1000;
        let day = DayviewDayInput {
            date: None,
            label_lines: vec![],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            errors: Vec::new(),
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
                    id: None,
                    title: None,
                }],
                points: vec![],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans: Vec::new(),
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
            axis_ticks_override: Some(nowview_axis_ticks(300.0, 30.0, 20.0)),
            right_panel_width: 120.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let entries = vec![NowviewCurrentState {
            field: "ghost".into(),
            state_label: Some("nope".into()),
            colors: vec!["#888888".into()],
            id: None,
        }];
        let svg = render_nowview_svg(&layout, "", &entries, false, false);
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
            date: None,
            label_lines: vec![],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            errors: Vec::new(),
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
                    id: None,
                    title: None,
                }],
                points: vec![],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans: Vec::new(),
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
            axis_ticks_override: Some(nowview_axis_ticks(400.0, 30.0, 20.0)),
            right_panel_width: 120.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        // Parallel state set: vertical + sit → two-colour swatch.
        let entries = vec![NowviewCurrentState {
            field: "body".into(),
            state_label: Some("vertical,sit".into()),
            colors: vec!["#aa3333".into(), "#33aa33".into()],
            id: None,
        }];
        let svg = render_nowview_svg(&layout, "12:00", &entries, false, false);
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
            date: None,
            label_lines: vec![],
            start_ms: now_ms - history_ms,
            end_ms: now_ms,
            errors: Vec::new(),
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
                    id: None,
                    title: None,
                }],
                points: vec![],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans: Vec::new(),
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
            axis_ticks_override: Some(nowview_axis_ticks(400.0, 30.0, 20.0)),
            right_panel_width: 120.0,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let entries = vec![NowviewCurrentState {
            field: "mood".into(),
            state_label: Some("focused".into()),
            colors: vec!["#abcdef".into()],
            id: None,
        }];
        let svg = render_nowview_svg(&layout, "12:00", &entries, false, false);
        assert!(
            field_group(&svg, "mood").contains(r##"<rect class="now-chunk" x="##),
            "single-state chunk must be a solid rect: {svg}"
        );
        assert!(
            !svg.contains("multi-state"),
            "no stripes for a single state"
        );
    }

    /// The value of attribute `name` on the first `<{tag} class="{class}"`.
    fn attr(svg: &str, tag: &str, class: &str, name: &str) -> Option<String> {
        let open = format!(r#"<{tag} class="{class}" "#);
        let at = svg.find(&open)? + open.len() - 1;
        let el = &svg[at..at + svg[at..].find('>')?];
        let key = format!(r#" {name}=""#);
        let v = el.find(&key)? + key.len();
        Some(el[v..v + el[v..].find('"')?].to_string())
    }

    fn num(svg: &str, tag: &str, class: &str, name: &str) -> f64 {
        attr(svg, tag, class, name)
            .unwrap_or_else(|| panic!("no {name} on {tag}.{class}: {svg}"))
            .parse()
            .unwrap()
    }

    /// da-9g3bf's picture: `mood` in a state for the 20 minutes up to now,
    /// and an entry two to four minutes ahead of it, in a window of 30
    /// minutes over 404px. Centred 5 minutes before now, the window runs
    /// 10 minutes past it; otherwise it ends at now.
    fn gap_picture(centred: bool, now_label: bool) -> (DayviewLayout, String) {
        gap_picture_with(centred, now_label, |_| {})
    }

    /// [`gap_picture`], its layout changed by `edit` before it is drawn.
    fn gap_picture_with(
        centred: bool,
        now_label: bool,
        edit: impl FnOnce(&mut DayviewLayout),
    ) -> (DayviewLayout, String) {
        picture_at(centred.then_some(-5), now_label, edit)
    }

    /// [`gap_picture`]'s entries in a window centred `at` minutes from now,
    /// or ending at now with none.
    fn picture_at(
        at: Option<i64>,
        now_label: bool,
        edit: impl FnOnce(&mut DayviewLayout),
    ) -> (DayviewLayout, String) {
        use crate::nowview::{nowview_config, NowviewGeometry};
        let centred = at.is_some();
        let now = 1_700_000_000_000_i64;
        let min = 60_000;
        let block = |from: i64, to: i64, c: &str| DayviewBlock {
            start_ms: now + from * min,
            end_ms: now + to * min,
            fill: BlockFill::Single(c.into()),
            state_label: Some(c.into()),
            id: None,
            title: None,
        };
        let fields = vec![DayviewFieldInput {
            field: "mood".into(),
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            blocks: vec![block(-20, 0, "#abcdef"), block(2, 4, "#123456")],
            points: vec![],
            icon: None,
            glyph_size: Default::default(),
        }];
        let geom = NowviewGeometry {
            width: Some(404.0),
            centred,
            ..NowviewGeometry::default()
        };
        let at = now + at.unwrap_or(0) * min;
        let mut layout = compute_dayview(&nowview_config(fields, at, now, None, &geom));
        edit(&mut layout);
        let states = [NowviewCurrentState {
            field: "mood".into(),
            state_label: Some("calm".into()),
            colors: vec!["#abcdef".into()],
            id: None,
        }];
        let svg = if centred {
            render_nowview_at_svg(&layout, "12:00", None, &states, false, now_label)
        } else {
            render_nowview_svg(&layout, "12:00", &states, false, now_label)
        };
        (layout, svg)
    }

    /// vividn, 2026-10-15: «a tiny margin between -0 and the dotted yellow
    /// lines so that now is slightly discontinuous with the past», «the now
    /// dotted lines finer» (da-9g3bf). The past ends at now and the line
    /// stands a gap past it, finer than dayview's, with the chunk from it to
    /// the edge — and no second line, there being no future to close it off.
    #[test]
    fn the_past_stops_a_gap_short_of_a_finer_now_line() {
        let (layout, svg) = gap_picture(false, false);
        let now = layout.time_axis_extent;
        assert_eq!(now, 400.0 * 4.0 / 5.0);
        let past = rects(&field_group(&svg, "mood"), "block");
        let past_end = past.iter().map(|b| b.x + b.w).fold(0.0, f64::max);
        let line = num(&svg, "line", "current-time-line", "x1");
        assert_eq!(past_end, now, "{svg}");
        assert_eq!(line, now + NOW_GAP, "{svg}");
        // ... and says where now is, for a reader measuring from it.
        assert_eq!(
            num(&svg, "line", "current-time-line", "data-now"),
            layout.margin + now
        );
        assert_eq!(
            attr(&svg, "line", "current-time-line", "stroke-width").unwrap(),
            "1"
        );
        assert_eq!(
            attr(&svg, "line", "current-time-line", "stroke-dasharray").unwrap(),
            "2,2"
        );
        let chunk = rect_attrs(&field_group(&svg, "mood"), "now-chunk").unwrap();
        assert_eq!((chunk.x, chunk.x + chunk.w), (line, 400.0), "{svg}");
        assert!(!svg.contains("current-time-line-end"), "{svg}");
    }

    /// «when the future is visible then we have the now chunk surrounded in
    /// yellow dotted lines with tiny gaps to each side and showing the
    /// future» (da-9g3bf): past, gap, line, chunk, line, gap, future — the
    /// future drawn at the past's scale, a strip further on.
    #[test]
    fn a_future_drawn_past_the_chunk_between_two_lines() {
        let (layout, svg) = gap_picture(true, false);
        let (extent, strip) = (layout.time_axis_extent, layout.right_panel_width);
        assert_eq!((extent, strip), (320.0, 80.0));
        // Now is 20 of the window's 30 minutes in.
        let now = extent * 20.0 / 30.0;
        let lead = num(&svg, "line", "current-time-line", "x1");
        let end = num(&svg, "line", "current-time-line-end", "x1");
        assert!((lead - (now + NOW_GAP)).abs() < 1e-9, "{svg}");
        assert!((end - (now + strip - NOW_GAP)).abs() < 1e-9, "{svg}");
        assert_eq!(
            attr(&svg, "line", "current-time-line-end", "stroke-width").unwrap(),
            "1"
        );
        let mood = field_group(&svg, "mood");
        let chunk = rect_attrs(&mood, "now-chunk").expect("a chunk");
        assert_eq!((chunk.x, chunk.x + chunk.w), (lead, end), "{svg}");
        // The past ends at now; the entry two minutes ahead starts past
        // the second line, 22 minutes in plus the strip.
        let blocks = rects(&svg, "block");
        assert!((blocks[0].x + blocks[0].w - now).abs() < 1e-9, "{svg}");
        assert!(
            (blocks[1].x - (extent * 22.0 / 30.0 + strip)).abs() < 1e-9,
            "{svg}"
        );
        assert!(blocks[1].x > end + NOW_GAP, "{svg}");
        // The two stretches say their own times, for a reader of the picture.
        assert_eq!(svg.matches(r#"<g class="day-data""#).count(), 2, "{svg}");
        assert!(
            !svg.contains("dead-zone") && !svg.contains("zone-label"),
            "{svg}"
        );
    }

    /// vividn, 2026-10-27 (da-dv45l): «it is no longer pinned on the side,
    /// but rather it just disappears when panning around». A window an hour
    /// either side of now draws no strip, no «now» and no now-line, and its
    /// one stretch of time runs the whole axis at the window's scale. One
    /// panned until now is just past its end draws the strip at now still,
    /// leaving the far edge with it.
    #[test]
    fn a_window_now_is_outside_draws_time_where_the_strip_was() {
        for at in [-60, 60] {
            let (layout, svg) = picture_at(Some(at), true, |_| {});
            assert_eq!(layout.right_panel_width, 80.0);
            for absent in ["now-chunk", "now-label", "current-time-line"] {
                assert!(!svg.contains(absent), "{absent}: {svg}");
            }
            assert_eq!(svg.matches(r#"<g class="day-data""#).count(), 1, "{svg}");
            let s = &layout.days[0].spans[0];
            assert_eq!((s.time_offset, s.time_extent), (0.0, 400.0), "{svg}");
            // At the window's scale: 30 minutes are its 320px.
            let ms = (s.end_ms - s.start_ms) as f64;
            assert!((ms - 37.5 * 60_000.0).abs() < 1.0, "{svg}");
        }
        let (layout, svg) = picture_at(Some(-16), true, |_| {});
        let c = now_chunk(&layout, &layout.days[0]).expect("a strip");
        assert!((c.now - 320.0 * 31.0 / 30.0).abs() < 1e-9, "{svg}");
        assert_eq!(c.to, c.now + 80.0, "{svg}");
        assert!(
            svg.contains("now-label") && svg.contains("now-chunk"),
            "{svg}"
        );
    }

    /// vividn, 2026-10-25: «the time axis numbers are way to big» (da-a2cxp).
    /// The default strip draws its ticks and clock at dayview's 11.1px, and
    /// a taller one gains room, not size.
    #[test]
    fn the_axis_numbers_are_dayviews_size_and_no_larger() {
        use crate::dayview_svg::AXIS_LABEL_MAX;
        assert!((AXIS_LABEL_MAX - 11.1).abs() < 0.05, "{AXIS_LABEL_MAX}");
        let sizes = |svg: &str, class: &str| -> Vec<f64> {
            svg.split(&format!(r#"<text class="{class}" "#))
                .skip(1)
                .map(|t| {
                    num(
                        &format!(r#"<text class="{class}" {t}"#),
                        "text",
                        class,
                        "font-size",
                    )
                })
                .collect()
        };
        for strip in [None, Some(40.0)] {
            let (_, svg) = gap_picture_with(false, false, |l| {
                if let Some(h) = strip {
                    l.time_axis_height = h;
                }
            });
            let (ticks, clock) = (sizes(&svg, "tick-label"), sizes(&svg, "current-time-text"));
            assert!(!ticks.is_empty() && clock.len() == 1, "{svg}");
            for size in ticks.iter().chain(&clock) {
                assert_eq!(*size, AXIS_LABEL_MAX, "{strip:?}: {svg}");
            }
        }
    }

    /// «a yellow now written entirely within the bounds of the yellow dotted
    /// lines underneath» (da-9g3bf), in place of the clock.
    #[test]
    fn the_now_label_lies_under_the_chunk_between_its_lines() {
        for centred in [false, true] {
            let (layout, svg) = gap_picture(centred, true);
            assert!(!svg.contains("current-time-text"), "{svg}");
            assert!(!svg.contains(">12:00<") || centred, "{svg}");
            let lead = num(&svg, "line", "current-time-line", "x1");
            let end = if centred {
                num(&svg, "line", "current-time-line-end", "x1")
            } else {
                400.0
            };
            let x = num(&svg, "text", "now-label", "x");
            let y = num(&svg, "text", "now-label", "y");
            let size = num(&svg, "text", "now-label", "font-size");
            let half = text_width("now", size) / 2.0;
            assert!(lead <= x - half && x + half <= end, "{lead}..{end}: {svg}");
            // Under the bands, in the axis strip, on the tick labels'
            // baseline: «now» has no letter taller than a digit.
            assert!(y - size * 0.72 >= layout.day_height, "{svg}");
            assert_eq!(y, layout.day_height + layout.time_axis_height, "{svg}");
            assert_eq!(
                attr(&svg, "text", "now-label", "fill").unwrap(),
                NOW_LINE_COLOR
            );
            assert!(svg.contains(">now</text>"), "{svg}");
        }
        // Not asked for, there is none.
        assert!(!gap_picture(false, false).1.contains("now-label"));
    }

    /// Each tick label's `(lo, hi, text)` along the axis, as drawn.
    fn tick_labels(svg: &str) -> Vec<(f64, f64, String)> {
        svg.split(r#"<text class="tick-label" "#)
            .skip(1)
            .map(|t| {
                let el = format!(r#"<text class="tick-label" {t}"#);
                let x = num(&el, "text", "tick-label", "x");
                let size = num(&el, "text", "tick-label", "font-size");
                let text = t[t.find('>').unwrap() + 1..t.find('<').unwrap()].to_string();
                let half = text_width(&text, size) / 2.0;
                (x - half, x + half, text)
            })
            .collect()
    }

    /// da-dv45l, vividn: «For selected times, don't display the outset
    /// block, just have the nowview as it would be with a dashed line and
    /// continuous in time around it». A window about a selected instant an
    /// hour before now draws one stretch through it, a dot at it where it
    /// falls on the timeline, a dashed line there, and no strip or label,
    /// now not being drawn. Ten minutes before now, now's strip is at now.
    #[test]
    fn a_selected_instant_is_a_dashed_line_in_a_continuous_timeline() {
        use crate::nowview::{nowview_config, NowviewGeometry};
        let now = 1_700_000_000_000_i64;
        let field = |at: i64| DayviewFieldInput {
            field: "coffee".into(),
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            blocks: vec![],
            points: vec![DayviewPoint {
                time_ms: at,
                fg_color: "#ff0000".into(),
                state_label: None,
                id: Some("atit".into()),
                title: None,
                glyph: None,
                icon: false,
                moment: false,
            }],
            icon: None,
            glyph_size: Default::default(),
        };
        let geom = NowviewGeometry {
            width: Some(404.0),
            centred: true,
            ..NowviewGeometry::default()
        };
        let state = NowviewCurrentState {
            field: "coffee".into(),
            state_label: Some("on".into()),
            colors: vec!["#00ff00".into()],
            id: None,
        };
        for back in [60, 10] {
            let at = now - back * 60_000;
            let layout =
                compute_dayview(&nowview_config(vec![field(at)], at, now, Some(at), &geom));
            let svg = render_nowview_at_svg(
                &layout,
                "13:46",
                Some(at),
                std::slice::from_ref(&state),
                false,
                true,
            );
            let (extent, strip) = (layout.time_axis_extent, layout.right_panel_width);
            assert_eq!((extent, strip), (320.0, 80.0));
            // The instant is the window's middle, on the timeline.
            let line = num(&svg, "line", "instant-line", "x1") - layout.label_width;
            assert!((line - extent / 2.0).abs() < 1e-9, "{svg}");
            assert_eq!(svg.matches(r#"class="instant-line""#).count(), 2, "{svg}");
            let cx = num(&svg, "circle", "point-bg", "cx") - layout.label_width;
            assert!((cx - line).abs() < 1e-9, "{svg}");
            assert!(!svg.contains("instant-label"), "{svg}");
            if back == 60 {
                assert_eq!(svg.matches(r#"<g class="day-data""#).count(), 1, "{svg}");
                assert!(
                    !svg.contains("now-chunk") && !svg.contains("now-label"),
                    "{svg}"
                );
                assert!(!svg.contains("current-time-line"), "{svg}");
            } else {
                let c = now_chunk(&layout, &layout.days[0]).unwrap();
                assert!((c.now - extent * 25.0 / 30.0).abs() < 1e-9, "{svg}");
                let chunk = rect_attrs(&field_group(&svg, "coffee"), "now-chunk").unwrap();
                assert_eq!((chunk.x, chunk.x + chunk.w), (c.from, c.to), "{svg}");
                let label = num(&svg, "text", "now-label", "x") - layout.label_width;
                assert!((label - (c.from + c.to) / 2.0).abs() < 1e-9, "{svg}");
            }
        }
    }

    /// da-ljs7b: the «now» stands in for the clock, and keeps the tick
    /// labels off it as the clock does, by the gap labels keep from each
    /// other: a tick at now is moved beside it — in a window ending there,
    /// and in one centred elsewhere that holds now.
    #[test]
    fn a_tick_label_keeps_off_the_now_label() {
        for centred in [false, true] {
            let drawn = "02:32";
            let at_now = |layout: &mut DayviewLayout| {
                let c = now_chunk(layout, &layout.days[0]).unwrap();
                let ticks = &mut layout.axis_ticks;
                ticks.retain(|t| (t.time_pos - c.now).abs() > 1e-9);
                ticks.push(DayviewAxisTick {
                    time_pos: c.now,
                    hour: 0,
                    label: drawn.into(),
                    is_major: true,
                });
                ticks.sort_by(|a, b| a.time_pos.total_cmp(&b.time_pos));
            };
            let (layout, svg) = gap_picture_with(centred, true, at_now);
            let x = num(&svg, "text", "now-label", "x") - layout.label_width;
            let half = text_width("now", num(&svg, "text", "now-label", "font-size")) / 2.0;
            let (now_lo, now_hi) = (x - half, x + half);
            let ticks = tick_labels(&svg);
            assert!(ticks.iter().any(|t| t.2 == drawn), "{svg}");
            for (lo, hi, text) in ticks {
                let (lo, hi) = (lo - layout.label_width, hi - layout.label_width);
                assert!(
                    hi + AXIS_LABEL_GAP <= now_lo + 1e-9 || lo >= now_hi + AXIS_LABEL_GAP - 1e-9,
                    "{text} at {lo}..{hi} runs under now at {now_lo}..{now_hi}: {svg}"
                );
            }
        }
        // With the clock in its place, the clock's own reservation stands:
        // the last tick label ends a gap short of the clock's text.
        let (layout, svg) = gap_picture(false, false);
        let clock_lo = clock_time_pos(&layout, "12:00") - clock_extent_px(&layout, "12:00") / 2.0;
        let last = tick_labels(&svg).last().unwrap().1 - layout.label_width;
        assert!(
            last + AXIS_LABEL_GAP <= clock_lo + 1e-9,
            "{last} {clock_lo}: {svg}"
        );
    }

    /// da-hklaz: drawn ahead (`--ahead`), a lane's marks are a `.flow` group
    /// under a clip ending at the now-line — the future at the past's own
    /// scale on past it, waiting there — and the now strip is outside the
    /// group, where it stays as a viewer slides the group along with the
    /// clock. The whole minutes are marked in a `.flow` group of their own,
    /// and the axis still stops at the now-line.
    #[test]
    fn a_picture_drawn_ahead_flows_behind_the_now_line() {
        use crate::dayview_svg::FlowMark;
        use crate::nowview::{nowview_config, NowviewGeometry};
        let now = 1_700_000_000_000_i64;
        let min = 60_000;
        let block = |from: i64, to: i64, c: &str| DayviewBlock {
            start_ms: now + from * min,
            end_ms: now + to * min,
            fill: BlockFill::Single(c.into()),
            state_label: Some(c.into()),
            id: None,
            title: None,
        };
        let fields = vec![DayviewFieldInput {
            field: "mood".into(),
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            blocks: vec![block(-20, 0, "#abcdef"), block(2, 4, "#123456")],
            points: vec![],
            icon: None,
            glyph_size: Default::default(),
        }];
        let geom = NowviewGeometry {
            width: Some(404.0),
            ahead_minutes: 5.0,
            ..NowviewGeometry::default()
        };
        let layout = compute_dayview(&nowview_config(fields, now, now, None, &geom));
        let states = [NowviewCurrentState {
            field: "mood".into(),
            state_label: Some("calm".into()),
            colors: vec!["#abcdef".into()],
            id: None,
        }];
        let marks = [FlowMark {
            ms: now + 3 * min,
            label: "12:03".into(),
        }];
        let svg = render_nowview_svg_with(&layout, "12:00", &states, false, false, Some(&marks));
        let extent = layout.time_axis_extent;
        let per_min = extent / 30.0;

        let band = field_group(&svg, "mood");
        assert!(
            band.starts_with(r#"<g clip-path="url(#flow-past-"#),
            "{band}"
        );
        assert!(band.contains(r#"<g class="flow">"#), "{band}");
        let blocks = rects(&band, "block");
        assert_eq!(blocks[0].x + blocks[0].w, extent, "{band}");
        assert!(
            (blocks[1].x - (extent + 2.0 * per_min)).abs() < 1e-6,
            "{band}"
        );
        let after = svg.split(band.as_str()).nth(1).unwrap();
        assert!(
            after.starts_with(r#"</g></g><rect class="now-chunk""#),
            "{after}"
        );

        // The clip is everything before the now-line.
        let clip = svg.split("<clipPath").nth(1).expect("a clip");
        let width: f64 = clip
            .split(r#"width=""#)
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(width - 1e5, extent, "{clip}");

        let marked = svg
            .split(r#"<g class="flow minute-marks">"#)
            .nth(1)
            .expect("the marks");
        assert!((num(marked, "line", "minute-mark", "x1") - (extent + 3.0 * per_min)).abs() < 1e-6);
        assert!(marked.contains(">12:03</text>"), "{marked}");
        assert!(svg.contains(&format!("H{}V", extent + 0.5)), "{svg}");

        // Not drawn ahead, nothing flows.
        assert!(!gap_picture(false, false).1.contains("flow"));
    }
}
