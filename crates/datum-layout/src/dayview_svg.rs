//! Pure-Rust SVG renderer for [`DayviewLayout`].
//!
//! Produces a self-contained SVG document string. No external CSS/JS,
//! no popover (per bead da-vp7 — the SvelteKit frontend handles
//! interactivity). Each mark carries what it is as data attributes and a
//! `<title>`, so any SVG viewer names it on hover (da-s5da), and each day
//! says which span of time its axis covers, for a viewer that maps a
//! pointer to an instant.
//!
//! Visual style mirrors datum's TypeScript dayview output:
//! - Black background.
//! - Hourly gridlines (40% opacity every 3rd hour, 10% otherwise).
//! - Per-day stack with date labels on the side adjacent to the data area
//!   (left for horizontal, top for vertical — see bead da-tbe).
//! - Per-field band rendering: solid blocks, multi-state diagonal stripes,
//!   point markers, each its coloured symbol alone (da-qietq).
//! - Yellow dashed "now" line on today's row only.
//! - A day the clock jumped in (da-1v9gp): the times that never happened
//!   hatched grey, the times that happened twice a lane each, and the UTC
//!   offset written at each end.
//! - Warning icon next to date labels for days with integrity findings,
//!   and a red dotted line across the day at each one (da-0lful).
//! - Time-axis strip on the side opposite the date labels (bottom for
//!   horizontal, left for vertical).

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::dayview::{
    BlockFill, DayviewAxisTick, DayviewLayout, Orientation, RenderedBlock, RenderedDay,
    RenderedField, RenderedSpan,
};

pub(crate) const NOW_LINE_COLOR: &str = "#ffcc00";
const BG_COLOR: &str = "black";
const TEXT_COLOR: &str = "white";
const AXIS_COLOR: &str = "white";
const ERROR_COLOR: &str = "#ff0000";
/// Per-stripe width (px) in multi-state diagonal patterns. Matches the
/// original TS `stripeWidth = 8`.
const STRIPE_WIDTH_PX: f64 = 8.0;

/// Advance width of one character as a fraction of the font size. The
/// renderer has no font engine, so axis-label placement (and the geometry
/// tests that check it) estimate text extents with this ratio.
///
/// Calibrated for a MONOSPACED face (0.6 is its standard advance), which is
/// what everything but the date labels and the hour axis still draws in.
/// Those two are proportional (da-l3tzj) and stay under it for the strings
/// they carry, measured once in each face named below: the hours are
/// digits, 0.556 em in Arial and its metric twins, and the date labels a
/// closed set (`Wed`, `May 22`, `2026`) whose widest is 3.08 em in Times
/// against the 4 em strip they are given. Those numbers were measured
/// outside the renderer (fontTools, against the closed label alphabet):
/// `svg_text_geometry.rs` estimates with this same ratio, so it cannot
/// check it.
pub const CHAR_ADVANCE_RATIO: f64 = 0.6;

/// The date labels' face: v2's, which set none and so took librsvg's
/// default. Named before the generic family because a PNG maps every
/// generic family to the first face it finds.
const DATE_LABEL_FONT: &str = "'Times New Roman', 'Liberation Serif', serif";
/// The hour axis's face, as v2's (d3's `sans-serif`), named for the same
/// reason. Its labels are stroked as v2's are, which thickens the ink
/// without moving the advance [`text_width`] estimates, as bold would.
const AXIS_LABEL_FONT: &str = "Arial, 'Liberation Sans', Helvetica, sans-serif";
/// A digit's height as a fraction of the font size (0.716 in Arial).
const DIGIT_HEIGHT_RATIO: f64 = 0.72;
/// How far a tick, and each of the axis's end hooks, reaches out of the
/// axis line: d3's `tickSize`, as v2 drew it.
const TICK_LENGTH: f64 = 6.0;

/// Estimated rendered width (px) of `label` at `font_size`.
pub fn text_width(label: &str, font_size: f64) -> f64 {
    label.chars().count() as f64 * font_size * CHAR_ADVANCE_RATIO
}

/// The largest a vertical time axis's labels grow (da-78evn). A vertical
/// strip's `time_axis_height` is its WIDTH, which has to hold the text
/// rather than set its size, so past this the strip only gains room. It is
/// the size nowview's default 22px horizontal strip draws at.
pub const VERTICAL_AXIS_LABEL_MAX: f64 = 16.0;

/// The time axis's label font size: in a horizontal strip the digits fill
/// it from a pixel past the ticks to its bottom edge (da-l3tzj), and in a
/// vertical one it is the strip less a little air, no larger than
/// [`VERTICAL_AXIS_LABEL_MAX`]. `0` — a strip too thin for text — draws no
/// labels.
pub fn axis_label_size(orientation: Orientation, time_axis_height: f64) -> f64 {
    if time_axis_height <= 6.0 {
        return 0.0;
    }
    match orientation {
        Orientation::Horizontal => {
            ((time_axis_height - TICK_LENGTH - 1.0) / DIGIT_HEIGHT_RATIO).max(6.0)
        }
        Orientation::Vertical => (time_axis_height - 6.0).clamp(6.0, VERTICAL_AXIS_LABEL_MAX),
    }
}

/// How far a vertical axis's labels sit left of its baseline: their
/// right-hand end, which is what a strip wide enough for them must clear.
pub const VERTICAL_AXIS_LABEL_INSET: f64 = 6.0;

/// Minimum blank space (px) between two adjacent axis labels, and between
/// a label and whatever an overlay reserves at the end of the axis.
pub(crate) const AXIS_LABEL_GAP: f64 = 4.0;

/// The space between now and the line beside it, and between the now chunk
/// and a future drawn past it (da-9g3bf): the past stops short of the line,
/// so it never reads as more time spent in a state than there was.
pub(crate) const NOW_GAP: f64 = 2.0;

/// Where a day's now strip puts its chunk along the time axis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NowChunk {
    /// Now: where the past ends.
    pub now: f64,
    /// The chunk's ends: a gap past now, and the strip's far edge — or a
    /// gap short of it, when the future is drawn on beyond.
    pub from: f64,
    pub to: f64,
    pub future: bool,
}

/// How far along the time axis anything is drawn: its extent, or past it
/// when a future is drawn beyond a now strip (da-9g3bf).
fn drawn_extent(layout: &DayviewLayout) -> f64 {
    (layout.days.iter().flat_map(|d| &d.spans))
        .map(|s| s.time_offset + s.time_extent)
        .fold(layout.time_axis_extent, f64::max)
}

/// `day`'s now chunk, or `None` when it draws no now strip. The snapshot,
/// with no time axis, is all strip and no gap.
pub(crate) fn now_chunk(layout: &DayviewLayout, day: &RenderedDay) -> Option<NowChunk> {
    let (now, w) = (day.now_time_pos?, layout.right_panel_width);
    if w <= 0.0 {
        return None;
    }
    let future = now < layout.time_axis_extent - 1e-9;
    let gap = if layout.time_axis_extent > 0.0 {
        NOW_GAP.min(w / 4.0)
    } else {
        0.0
    };
    Some(NowChunk {
        now,
        from: now + gap,
        to: now + w - if future { gap } else { 0.0 },
        future,
    })
}

/// What one field's band carries in the strip the layout reserved beside
/// the data area ([`DayviewLayout::right_panel_width`]). Nowview fills it
/// with the state each field is in *right now* (da-nk84); the fill is
/// painted with the field's own blocks, so it reads as the lane running
/// on past the now-line.
#[derive(Debug, Clone)]
pub struct RightPanelFill {
    pub field: String,
    pub fill: BlockFill,
    /// The hid of the entry that OPENED the state the strip is painted in
    /// (da-6hg6v) — the chunk is that entry's open block, so it names it
    /// like any other mark. `None` when the state came from no single row.
    pub id: Option<String>,
}

/// Knobs the nowview overlay needs the dayview renderer to honour.
#[derive(Debug, Clone, Copy, Default)]
pub struct DayviewSvgOpts<'a> {
    /// Pixels at the far end of the time axis owned by an overlay drawn on
    /// top of the axis strip (nowview's clock). Tick labels never enter it.
    pub axis_end_reserved: f64,
    /// Per-field fills for the reserved right panel, matched to bands by
    /// name. Only the day carrying the now-line gets them; a field absent
    /// from the slice leaves its strip empty.
    pub right_panel_fills: &'a [RightPanelFill],
}

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
/// `cross` is a coordinate, not an anchor: a band passes its near edge
/// (`cross_offset`) and a dot its centre (`cross_offset + cross_extent / 2`),
/// both from the one convention on `ResolvedPosition`
/// (`crates/datum-core/src/field_spec/presentation.rs`).
fn to_xy(layout: &DayviewLayout, geom: &PlotGeom, time: f64, cross: f64) -> (f64, f64) {
    match layout.orientation {
        Orientation::Horizontal => (geom.data_x + time, geom.data_y + cross),
        Orientation::Vertical => (geom.data_x + cross, geom.data_y + time),
    }
}

/// The root's style: the picture's text is not selectable (da-mu5op).
pub(crate) const NO_SELECT: &str =
    "user-select:none;-webkit-user-select:none;-webkit-touch-callout:none";

/// Render `layout` to an SVG document string.
pub fn render_dayview_svg(layout: &DayviewLayout) -> String {
    render_dayview_svg_with(layout, DayviewSvgOpts::default())
}

/// [`render_dayview_svg`] with room left for a caller-drawn overlay.
pub fn render_dayview_svg_with(layout: &DayviewLayout, opts: DayviewSvgOpts<'_>) -> String {
    let mut out = String::with_capacity(8192);
    let mut defs = String::new();
    let mut patterns: HashMap<String, String> = HashMap::new();
    let mut icons: HashSet<&str> = HashSet::new();
    let mut hatched = false;
    let geom = PlotGeom::new(layout);

    // The picture is pressed and tapped, not copied: none of its text — a
    // point's symbol least of all — is selectable, and a long press raises
    // no iOS callout (da-mu5op). An attribute and not a `<style>`, which
    // datum.day strips from an inlined SVG as page-wide.
    let _ = write!(
        &mut out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}" style="{NO_SELECT}">"#,
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
        // The date-label strip: group, lines, and the day's warning icon.
        // A zero-width strip is a caller drawing its own date labels (or
        // none at all) — there is no column to write into, so nothing is
        // emitted rather than a label at a negative x (da-55ru).
        if layout.label_width > 0.0 {
            // Positioned at the start of this day's slot along the cross
            // axis, centered on the perpendicular dim.
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
                    r#"<text class="date-label" x="{lx}" y="{ly}" dy="0.35em" fill="{c}" text-anchor="{a}" font-size="{fs}" font-family="{DATE_LABEL_FONT}">{txt}</text>"#,
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
                    // At the column's edge, clear of the centred lines.
                    Orientation::Vertical => (
                        layout.day_height - icon_size,
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
        }

        // Per-field bands — emitted in plot coords (no per-day transform);
        // helpers map (time, cross) → (x, y) per orientation.
        // A stretch's time axis in the root's user units: `data-lo` is
        // `data-from`, `data-hi` is `data-to`, along `data-axis`. A day is
        // one group per stretch (da-1v9gp), so each is a straight axis, and
        // `data-offset` is its clock in seconds east of UTC (da-3ctgr).
        let (axis, lo) = match layout.orientation {
            Orientation::Horizontal => ("x", layout.margin + geom.data_x),
            Orientation::Vertical => ("y", layout.margin + geom.data_y),
        };
        for (si, span) in day.spans.iter().enumerate() {
            let _ = write!(
                &mut out,
                r#"<g class="day-data" data-day="{i}" data-from="{from}" data-to="{to}" data-axis="{axis}" data-lo="{lo}" data-hi="{hi}"{offset}>"#,
                from = iso(span.start_ms),
                to = iso(span.end_ms),
                lo = lo + span.time_offset,
                hi = lo + span.time_offset + span.time_extent,
                offset =
                    (span.offset_s).map_or(String::new(), |s| format!(r#" data-offset="{s}""#)),
            );
            if !span.lane.is_empty() {
                let _ = write!(
                    &mut out,
                    r#"<path class="lane" d="{d}" fill="none" stroke="white" stroke-opacity="0.5" pointer-events="none"/>"#,
                    d = lane_outline(layout, &geom, day, span),
                );
            }
            for field in &day.fields {
                let here = field.blocks.iter().any(|b| b.span == si)
                    || field.points.iter().any(|p| p.span == si);
                if day.spans.len() > 1 && !here {
                    continue;
                }
                let _ = write!(
                    &mut out,
                    r#"<g class="field" data-field="{f}">"#,
                    f = escape_xml(&field.field),
                );
                for b in field.blocks.iter().filter(|b| b.span == si) {
                    let (tag, shape) = block_shape(layout, &geom, day, field, span, b);
                    push_fill(
                        &mut out,
                        &mut defs,
                        &mut patterns,
                        "block",
                        &b.fill,
                        tag,
                        &shape,
                        field.cross_extent,
                        &mark_close(
                            tag,
                            &field.field,
                            b.state_label.as_deref(),
                            b.id.as_deref(),
                            &[("from", b.start_ms), ("to", b.end_ms)],
                            b.title.as_deref(),
                        ),
                    );
                }
                // The reserved strip beyond the now-line: the lane running on
                // in whatever state it is in now (da-nk84). Painted with the
                // field's blocks so gridlines and the now-line still sit over
                // it, and positioned through `block_rect` so it follows the
                // orientation like any other block.
                let chunk = now_chunk(layout, day).filter(|c| {
                    (span.time_offset..=span.time_offset + span.time_extent).contains(&c.now)
                });
                if let Some(c) = chunk {
                    if let Some(pf) = opts
                        .right_panel_fills
                        .iter()
                        .find(|r| r.field == field.field)
                    {
                        push_fill(
                            &mut out,
                            &mut defs,
                            &mut patterns,
                            "now-chunk",
                            &pf.fill,
                            "rect",
                            &block_rect(
                                layout,
                                &geom,
                                day.day_offset + field.cross_offset,
                                field.cross_extent,
                                c.from,
                                c.to - c.from,
                            ),
                            field.cross_extent,
                            &now_chunk_close(pf.id.as_deref()),
                        );
                    }
                }
                for p in field.points.iter().filter(|p| p.span == si) {
                    // A dot in a lane is the lane's size (da-1v9gp).
                    let (near, far) = span.lane_at(p.time_pos);
                    let cross = near * layout.day_height + (far - near) * p.cross_pos;
                    let (cx, cy) = to_xy(layout, &geom, p.time_pos, day.day_offset + cross);
                    let outer_r = p.r * (far - near);
                    // The disc is the mark — the pointer target, the title, the
                    // data attributes — but it is never painted: a dot is its
                    // symbol alone, in its own colour (da-qietq). The symbol
                    // lets the pointer through to it.
                    let icon =
                        (field.icon.as_deref()).filter(|svg| p.icon && svg.starts_with("<svg"));
                    // da-c05v1: a mark wider or narrower than it is tall is
                    // stretched along x about its own centre; the disc under it,
                    // the pointer target, is not.
                    let a = field.glyph_aspect;
                    let stretch = if a == 1.0 {
                        String::new()
                    } else {
                        format!(
                            r#" transform="matrix({a} 0 0 1 {dx} 0)""#,
                            dx = cx * (1.0 - a)
                        )
                    };
                    let _ = write!(
                        &mut out,
                        r#"<circle class="point-bg" cx="{cx}" cy="{cy}" r="{r}" fill="{c}" fill-opacity="0"{close}"#,
                        cx = cx,
                        cy = cy,
                        r = outer_r,
                        c = p.fg_color,
                        close = mark_close(
                            "circle",
                            &field.field,
                            p.state_label.as_deref(),
                            p.id.as_deref(),
                            &[("at", p.time_ms)],
                            p.title.as_deref(),
                        ),
                    );
                    // da-45wgz: an icon is written ONCE, in `<defs>`, under an
                    // id named for its field — the same in every band the page
                    // stacks, so a reference can never find another field's —
                    // and each dot `<use>`s it, sized to the disc and handed the
                    // dot's colour as `currentColor`.
                    if let Some(svg) = icon {
                        let id = icon_id(&field.field);
                        if icons.insert(&field.field) {
                            let rest = &scope_ids(svg, &id)["<svg".len()..];
                            let _ = write!(&mut defs, r#"<svg id="{id}"{rest}"#);
                        }
                        let _ = write!(
                            &mut out,
                            r##"<use class="point-fg" href="#{id}" x="{x}" y="{y}" width="{d}" height="{d}" color="{c}" pointer-events="none"{stretch}/>"##,
                            x = cx - outer_r,
                            y = cy - outer_r,
                            d = outer_r * 2.0,
                            c = p.fg_color,
                        );
                        continue;
                    }
                    // da-gzowt: a glyph stands where the circle would, centred
                    // on the same point, in the same colour. Sized to the disc
                    // so it reads as the mark rather than as a label on it.
                    let _ = match p.glyph.as_deref() {
                        None => write!(
                            &mut out,
                            r#"<circle class="point-fg" cx="{cx}" cy="{cy}" r="{r}" fill="{c}" pointer-events="none"{stretch}/>"#,
                            cx = cx,
                            cy = cy,
                            r = outer_r,
                            c = p.fg_color,
                        ),
                        Some(g) => write!(
                            &mut out,
                            r#"<text class="point-fg" x="{cx}" y="{cy}" dy="0.35em" text-anchor="middle" font-size="{fs}" fill="{c}" pointer-events="none"{stretch}>{g}</text>"#,
                            cx = cx,
                            cy = cy,
                            fs = outer_r * 1.6,
                            c = p.fg_color,
                            g = escape_xml(g),
                        ),
                    };
                }
                out.push_str("</g>");
            }
            out.push_str("</g>");
        }

        // The clock's jumps (da-1v9gp): the times that never happened,
        // hatched, and the offset at each end of a stretch. Neither takes
        // the pointer: a press there is a press on the day's ground.
        for zone in &day.dead_zones {
            if !std::mem::replace(&mut hatched, true) {
                let _ = write!(
                    &mut defs,
                    r##"<pattern id="{DEAD_ZONE_ID}" patternUnits="userSpaceOnUse" width="6" height="6" patternTransform="rotate(45)"><rect width="2" height="6" fill="#808080"/></pattern>"##,
                );
            }
            let _ = write!(
                &mut out,
                r##"<rect class="dead-zone" {shape} fill="url(#{DEAD_ZONE_ID})" pointer-events="none"/>"##,
                shape = block_rect(
                    layout,
                    &geom,
                    day.day_offset,
                    layout.day_height,
                    zone[0],
                    zone[1]
                ),
            );
        }
        for label in &day.zone_labels {
            let (x, y) = to_xy(
                layout,
                &geom,
                label.time_pos,
                day.day_offset + label.cross_pos,
            );
            // Along a horizontal axis the text runs away from its end; down
            // a vertical one it sits above or hangs below it.
            let place = match (layout.orientation, label.ends) {
                (Orientation::Horizontal, true) => r#"dy="0.35em" text-anchor="end""#,
                (Orientation::Horizontal, false) => r#"dy="0.35em" text-anchor="start""#,
                (Orientation::Vertical, true) => r#"text-anchor="middle""#,
                (Orientation::Vertical, false) => r#"dy="0.8em" text-anchor="middle""#,
            };
            let _ = write!(
                &mut out,
                r#"<text class="zone-label" x="{x}" y="{y}" {place} font-size="{fs}" fill="{c}" stroke="{bg}" stroke-width="2" paint-order="stroke" pointer-events="none">{txt}</text>"#,
                fs = label.font_size,
                c = TEXT_COLOR,
                bg = BG_COLOR,
                txt = escape_xml(&label.text),
            );
        }

        // What `check integrity` finds at an instant (da-0lful): a red
        // dotted line across the whole day, the field's name at its start,
        // and v2's warning icon on the field's own track.
        for e in &day.errors {
            let field = escape_xml(&e.field);
            let (x1, y1) = to_xy(layout, &geom, e.time_pos, day.day_offset);
            let (x2, y2) = to_xy(
                layout,
                &geom,
                e.time_pos,
                day.day_offset + layout.day_height,
            );
            let fs = (layout.day_height * 0.12).clamp(7.0, 11.0);
            let (lx, ly) = match layout.orientation {
                Orientation::Horizontal => (x1 + 2.0, y1 + fs),
                Orientation::Vertical => (x1 + 2.0, y1 - 2.0),
            };
            let _ = write!(
                &mut out,
                r#"<g class="integrity-error" data-field="{field}" data-at="{at}"><title>{field}: {what}</title><line class="error-line" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}" stroke="{ERROR_COLOR}" stroke-width="1.5" stroke-dasharray="3,3" pointer-events="none"/><text class="error-label" x="{lx}" y="{ly}" font-size="{fs}" fill="{ERROR_COLOR}" stroke="{BG_COLOR}" stroke-width="2" paint-order="stroke" pointer-events="none">{field}</text>"#,
                at = iso(e.time_ms),
                what = escape_xml(&e.what),
            );
            if let Some((cross, extent)) = e.track {
                // v2's size: half again the dot the track draws.
                let size = extent * 1.5;
                let (cx, cy) = to_xy(layout, &geom, e.time_pos, day.day_offset + cross);
                let _ = write!(
                    &mut out,
                    r#"<g class="error-track" transform="translate({x},{y})">"#,
                    x = cx - size / 2.0,
                    y = cy - size / 2.0,
                );
                push_warning_icon(&mut out, size);
                out.push_str("</g>");
            }
            out.push_str("</g>");
        }
    }

    // Time axis — bottom strip in horizontal, left strip in vertical.
    if layout.time_axis_height > 0.0 {
        emit_time_axis(&mut out, layout, &geom, opts);
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
    // A plot with no time axis (nowview's snapshot) has no line to draw.
    for day in &layout.days {
        let Some(nt) = day.now_time_pos.filter(|_| layout.time_axis_extent > 0.0) else {
            continue;
        };
        // Beside a now strip the line is finer and stands a gap past now,
        // and a second closes the chunk when the future is drawn on beyond
        // it (da-9g3bf).
        let chunk = now_chunk(layout, day);
        let (stroke, dash) = if chunk.is_some() {
            (1, "2,2")
        } else {
            (2, "4,2")
        };
        let mut line = |class: &str, extra: &str, (x1, y1, x2, y2): (f64, f64, f64, f64)| {
            let _ = write!(
                &mut out,
                r#"<line class="{class}" x1="{x1}" x2="{x2}" y1="{y1}" y2="{y2}"{extra} stroke="{c}" stroke-width="{stroke}" stroke-dasharray="{dash}"/>"#,
                c = NOW_LINE_COLOR,
            );
        };
        let across = |t: f64| match layout.orientation {
            Orientation::Horizontal => {
                let x = geom.data_x + t;
                (
                    x,
                    geom.data_y + day.now_extent_near,
                    x,
                    geom.data_y + day.now_extent_far,
                )
            }
            Orientation::Vertical => {
                // The axis strip is left of the first day, so that
                // day's line crosses it (da-78evn).
                let y = geom.data_y + t;
                let near = if day.now_extent_near <= 1e-9 {
                    layout.time_axis_height
                } else {
                    0.0
                };
                (
                    geom.data_x + day.now_extent_near - near,
                    y,
                    geom.data_x + day.now_extent_far,
                    y,
                )
            }
        };
        // Standing a gap past now, the line says where now is, in the
        // root's user units as `data-lo` is, for a reader measuring from it.
        let (at, extra) = match chunk {
            Some(c) => {
                let origin = match layout.orientation {
                    Orientation::Horizontal => geom.data_x,
                    Orientation::Vertical => geom.data_y,
                };
                let now = layout.margin + origin + nt;
                (c.from, format!(r#" data-now="{now}""#))
            }
            None => (nt, String::new()),
        };
        line("current-time-line", &extra, across(at));
        if let Some(c) = chunk.filter(|c| c.future) {
            line("current-time-line-end", "", across(c.to));
        }

        // The white axis baseline stops at the now-line, so the strip
        // beyond it gets the now marker instead — the original datum's
        // horizontal `current-time-line` (da-nk84). Only the day the
        // axis strip sits against carries it: the last one horizontally,
        // where the strip is below, the first vertically, where it is
        // to the left (da-78evn).
        let is_axis_day = match layout.orientation {
            Orientation::Horizontal => {
                day.day_offset + layout.day_height >= geom.cross_total - 1e-9
            }
            Orientation::Vertical => day.day_offset <= 1e-9,
        };
        if let Some(c) = chunk.filter(|_| is_axis_day && layout.time_axis_height > 0.0) {
            line(
                "current-time-line-far",
                "",
                match layout.orientation {
                    Orientation::Horizontal => {
                        let y = geom.data_y + geom.cross_total;
                        (geom.data_x + c.from, y, geom.data_x + c.to, y)
                    }
                    Orientation::Vertical => {
                        let x = geom.data_x;
                        (x, geom.data_y + c.from, x, geom.data_y + c.to)
                    }
                },
            );
        }
    }
    out.push_str("</g>");

    if !defs.is_empty() {
        out.insert_str(after_first_tag(&out), &format!(r#"<defs>{defs}</defs>"#));
    }

    out.push_str("</svg>");
    out
}

/// The `<defs>` id of `field`'s icon: the name with anything an id or a
/// `#` reference would trip on spelled as its code point.
fn icon_id(field: &str) -> String {
    field.chars().fold(String::from("icon-"), |mut id, c| {
        if c.is_ascii_alphanumeric() || c == '-' {
            id.push(c);
        } else {
            let _ = write!(id, "_{:x}_", c as u32);
        }
        id
    })
}

/// `svg` with each id inside it prefixed by `prefix` and a `.`, and each
/// `href="#…"` and `url(#…)` to one of them rewritten to match (da-ui2st):
/// inlined, an icon's ids are page-global, so two fields' `id="g"` would
/// paint each other's gradient. An [`icon_id`] never holds a `.`, so no
/// scoped id can meet another icon's, or an icon's own.
fn scope_ids(svg: &str, prefix: &str) -> String {
    let Ok(doc) = roxmltree::Document::parse(svg) else {
        return svg.to_string();
    };
    let attrs: Vec<_> = doc.descendants().flat_map(|n| n.attributes()).collect();
    let ids: HashSet<&str> = attrs
        .iter()
        .filter(|a| a.name() == "id")
        .map(|a| a.value())
        .collect();
    let mut at = Vec::new();
    for a in &attrs {
        let range = a.range_value();
        if a.name() == "id" {
            at.push(range.start);
            continue;
        }
        let raw = &svg[range.clone()];
        for (i, _) in raw.match_indices('#') {
            let head =
                raw[..i].trim_end_matches(|c: char| c.is_whitespace() || c == '\'' || c == '"');
            let refers =
                (i == 0 && a.name() == "href") || head.to_ascii_lowercase().ends_with("url(");
            let name = raw[i + 1..]
                .split(|c: char| c.is_whitespace() || "'\")".contains(c))
                .next();
            if refers && name.is_some_and(|n| ids.contains(n)) {
                at.push(range.start + i + 1);
            }
        }
    }
    at.sort_unstable();
    let mut out = String::with_capacity(svg.len() + at.len() * (prefix.len() + 1));
    let mut last = 0;
    for i in at {
        let _ = write!(out, "{}{prefix}.", &svg[last..i]);
        last = i;
    }
    out.push_str(&svg[last..]);
    out
}

/// Index of the byte just past the first `>` in `s` — the insertion point
/// for `<defs>`, immediately after the opening `<svg …>` tag.
fn after_first_tag(s: &str) -> usize {
    s.find('>').map(|i| i + 1).unwrap_or(0)
}

/// Emit one filled shape for a [`BlockFill`], solid or 45°-striped: a
/// `tag` element with the geometry attributes `shape`. `class` names the
/// element (`block` for a lane's own blocks, `now-chunk` for the strip
/// past the now-line); a striped fill appends its `<pattern>` to `defs` on
/// first use, keyed so identical fills share one def. `close` ends the tag
/// — `/>`, or a mark's [`mark_close`].
#[allow(clippy::too_many_arguments)]
fn push_fill(
    out: &mut String,
    defs: &mut String,
    patterns: &mut HashMap<String, String>,
    class: &str,
    fill: &BlockFill,
    tag: &str,
    shape: &str,
    cross_extent: f64,
    close: &str,
) {
    match fill {
        BlockFill::Single(color) => {
            let _ = write!(
                out,
                r#"<{tag} class="{cl}" {shape} fill="{c}"{close}"#,
                cl = class,
                c = color,
            );
        }
        BlockFill::MultiState(colors) => {
            if colors.is_empty() {
                return;
            }
            let pattern_id = stripe_pattern_id(defs, patterns, colors, cross_extent);
            let _ = write!(
                out,
                r#"<{tag} class="{cl} multi-state" {shape} fill="url(#{pid})"{close}"#,
                cl = class,
                pid = pattern_id,
            );
        }
    }
}

/// The rest of a mark's tag after its geometry (da-s5da): what it is as
/// data — its field, its state as the CLI prints it (`∅` for none), the
/// whole hid of the entry that generated it (da-6hg6v) and its instants —
/// and a `<title>` child, which any SVG viewer shows on hover with no
/// script. Closes the `tag` element.
///
/// `data-id` is omitted rather than emitted empty when a mark has no
/// single entry behind it, so a reader tests for the attribute and never
/// for a sentinel. It is the WHOLE hid, not the short prefix the CLI
/// prints: `web/src/lib/selection.ts`'s `same()` matches either against
/// the other by prefix, and on a long record only the whole one cannot
/// be matched by a different entry.
fn mark_close(
    tag: &str,
    field: &str,
    state: Option<&str>,
    id: Option<&str>,
    times: &[(&str, i64)],
    title: Option<&str>,
) -> String {
    let mut out = format!(
        r#" data-field="{}" data-state="{}""#,
        escape_xml(field),
        escape_xml(state.unwrap_or("∅")),
    );
    if let Some(id) = id {
        let _ = write!(out, r#" data-id="{}""#, escape_xml(id));
    }
    for (name, ms) in times {
        let _ = write!(out, r#" data-{name}="{}""#, iso(*ms));
    }
    match title {
        Some(t) => {
            let _ = write!(out, "><title>{}</title></{tag}>", escape_xml(t));
        }
        None => out.push_str("/>"),
    }
    out
}

/// How the now-chunk's rect closes (da-6hg6v). It is the OPEN BLOCK of
/// the state the field is in, so it names the entry that opened that
/// state — vividn: «if you double tap a color, that color is an open
/// block, so it should select the datum entry that generated that open
/// block». It carries no `data-field` / `data-state` / times of its own:
/// it never has, `lanes.ts` reads it as the `.now-chunk` inside its
/// field's group, and this bead adds identity, not description.
fn now_chunk_close(id: Option<&str>) -> String {
    match id {
        Some(id) => format!(r#" data-id="{}"/>"#, escape_xml(id)),
        None => "/>".to_string(),
    }
}

/// An instant as a mark's data carries it: ISO, UTC, to the millisecond —
/// the way a slug writes it.
fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}

/// A `<rect>`'s geometry attributes for a block given its cross-axis
/// position+extent (in plot-coord cross axis) and time-axis position+extent.
fn block_rect(
    layout: &DayviewLayout,
    geom: &PlotGeom,
    cross_off: f64,
    cross_ext: f64,
    time_off: f64,
    time_ext: f64,
) -> String {
    let (x, y, w, h) = match layout.orientation {
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
    };
    format!(r#"x="{x}" y="{y}" width="{w}" height="{h}""#)
}

/// The `<pattern>` a dead zone is hatched with: one id, so the bands a
/// page stacks share it.
const DEAD_ZONE_ID: &str = "dead-zone";

/// `from`, the lane's knots strictly between, `to`: where an edge drawn
/// across that stretch of a forked band bends.
fn bends(span: &RenderedSpan, from: f64, to: f64) -> Vec<f64> {
    let inner = span.lane.iter().map(|k| k[0]);
    let mut xs = vec![from];
    xs.extend(inner.filter(|x| from < *x && *x < to));
    xs.push(to);
    xs
}

/// A path through `(time, cross-fraction of the day's band)` points.
fn path_d(
    layout: &DayviewLayout,
    geom: &PlotGeom,
    day: &RenderedDay,
    points: impl Iterator<Item = (f64, f64)>,
) -> String {
    let mut d = String::new();
    for (time, frac) in points {
        let cross = day.day_offset + frac * layout.day_height;
        let (x, y) = to_xy(layout, geom, time, cross);
        let _ = write!(d, "{}{x},{y}", if d.is_empty() { "M" } else { "L" });
    }
    d
}

/// A block's element and geometry attributes: the `<rect>` it has always
/// been, or in a forked band (da-1v9gp) a `<path>` that narrows and widens
/// with its lane.
fn block_shape(
    layout: &DayviewLayout,
    geom: &PlotGeom,
    day: &RenderedDay,
    field: &RenderedField,
    span: &RenderedSpan,
    b: &RenderedBlock,
) -> (&'static str, String) {
    if span.lane.is_empty() {
        let cross = day.day_offset + field.cross_offset;
        let rect = block_rect(
            layout,
            geom,
            cross,
            field.cross_extent,
            b.time_offset,
            b.time_extent,
        );
        return ("rect", rect);
    }
    // The field's own edges, as fractions of the lane's.
    let edge = |x: f64, within: f64| {
        let (near, far) = span.lane_at(x);
        (x, near + (far - near) * within / layout.day_height)
    };
    let xs = bends(span, b.time_offset, b.time_offset + b.time_extent);
    let near = xs.iter().map(|x| edge(*x, field.cross_offset));
    let far = (xs.iter().rev()).map(|x| edge(*x, field.cross_offset + field.cross_extent));
    let d = path_d(layout, geom, day, near.chain(far));
    ("path", format!(r#"d="{d}Z""#))
}

/// The outline of a stretch's lane where the band is forked (da-1v9gp):
/// its two edges from where the band narrows to where it widens again, and
/// a cap across each end the stretch has there.
fn lane_outline(
    layout: &DayviewLayout,
    geom: &PlotGeom,
    day: &RenderedDay,
    span: &RenderedSpan,
) -> String {
    let (first, last) = (span.lane[0], span.lane[span.lane.len() - 1]);
    let (cap_lo, cap_hi) = (first[1..] != [0.0, 1.0], last[1..] != [0.0, 1.0]);
    let from = if cap_lo { span.time_offset } else { first[0] };
    let to = if cap_hi {
        span.time_offset + span.time_extent
    } else {
        last[0]
    };
    let mut xs = bends(span, from, to);
    // One stroke: out along an edge, across the cap, back along the other.
    if cap_lo && !cap_hi {
        xs.reverse();
    }
    let near = xs.iter().map(|x| (*x, span.lane_at(*x).0));
    let far = xs.iter().rev().map(|x| (*x, span.lane_at(*x).1));
    let d = path_d(layout, geom, day, near.chain(far));
    if cap_lo && cap_hi {
        d + "Z"
    } else {
        d
    }
}

/// A label candidate along the axis: where its tick sits, how much room
/// the text needs, and whether the tick is a major one.
struct LabelSlot {
    center: f64,
    extent: f64,
    is_major: bool,
}

/// Decide which ticks get a label and where the text sits, given the
/// `[lo, hi]` range labels may occupy along the label axis.
///
/// The tick count is fixed while the render size is not, so a narrow render
/// packs labels until they collide (da-sv8c). Labels that would cross an
/// end of the range are inset to sit flush against it. The last label,
/// squeezed by that inset (the closing `00`, da-i37p2), gives way alone
/// rather than thin the rest; if more still doesn't fit, the series is
/// thinned to its major ticks and then greedily to whatever survives.
fn fit_axis_labels(slots: &[LabelSlot], lo: f64, hi: f64) -> Vec<(usize, f64)> {
    let all: Vec<usize> = (0..slots.len()).collect();
    let placed = greedy_place(&all, slots, lo, hi);
    if placed.len() + 1 >= all.len() && placed.iter().zip(&all).all(|(p, i)| p.0 == *i) {
        return placed;
    }
    let majors: Vec<usize> = all.iter().copied().filter(|&i| slots[i].is_major).collect();
    if majors.is_empty() {
        return placed;
    }
    greedy_place(&majors, slots, lo, hi)
}

/// Place `candidates` (ascending) in order, insetting each into `[lo, hi]`
/// and dropping any that still overlaps the one before it.
fn greedy_place(candidates: &[usize], slots: &[LabelSlot], lo: f64, hi: f64) -> Vec<(usize, f64)> {
    let mut out = Vec::with_capacity(candidates.len());
    let mut prev_end = f64::NEG_INFINITY;
    for &i in candidates {
        let half = slots[i].extent / 2.0;
        if slots[i].extent <= 0.0 || hi - lo < slots[i].extent {
            continue;
        }
        let center = slots[i].center.clamp(lo + half, hi - half);
        if center - half < prev_end + AXIS_LABEL_GAP {
            continue;
        }
        prev_end = center + half;
        out.push((i, center));
    }
    out
}

fn emit_time_axis(
    out: &mut String,
    layout: &DayviewLayout,
    geom: &PlotGeom,
    opts: DayviewSvgOpts<'_>,
) {
    let label_size = axis_label_size(layout.orientation, layout.time_axis_height);
    // Every labelled tick is drawn, the closing midnight's `00` too: a full
    // day's axis ends on an hour it reaches (da-i37p2). A tick with no label
    // is an instant an overlay owns — nowview's now-line, under its clock —
    // and gets no mark. A strip too thin for text carries no ticks at all.
    let labelled: Vec<&DayviewAxisTick> = if label_size > 0.0 {
        layout
            .axis_ticks
            .iter()
            .filter(|t| !t.label.is_empty())
            .collect()
    } else {
        Vec::new()
    };
    let horizontal = matches!(layout.orientation, Orientation::Horizontal);
    let axis_origin = if horizontal { geom.data_x } else { geom.data_y };
    // Along the label axis a horizontal axis is limited by the text's own
    // width; a vertical one stacks baselines, so a line's height is the
    // extent that matters.
    let slots: Vec<LabelSlot> = labelled
        .iter()
        .map(|t| LabelSlot {
            center: axis_origin + t.time_pos,
            extent: if horizontal {
                text_width(&t.label, label_size)
            } else {
                label_size
            },
            is_major: t.is_major,
        })
        .collect();
    // Labels stay inside the plot group and clear of any end overlay, so
    // the closing `00` is inset flush to the axis end, as the leading one
    // is to the plot's edge when no date column holds its overhang.
    let placed = fit_axis_labels(
        &slots,
        0.0,
        axis_origin + drawn_extent(layout) - opts.axis_end_reserved.max(0.0),
    );

    match layout.orientation {
        Orientation::Horizontal => {
            // Baseline beneath the day-stack data area.
            let axis_y = geom.data_y + geom.cross_total;
            let _ = write!(
                out,
                r#"<path class="axis" transform="translate({x},{y})" d="M0.5,{t}V0.5H{end}V{t}" fill="none" stroke="{c}"/>"#,
                x = geom.data_x,
                y = axis_y,
                t = TICK_LENGTH,
                end = drawn_extent(layout) + 0.5,
                c = AXIS_COLOR,
            );
            let mut placed = placed.iter().peekable();
            for (i, tick) in labelled.iter().enumerate() {
                let x = geom.data_x + tick.time_pos;
                let _ = write!(
                    out,
                    r#"<line class="tick" x1="{x}" x2="{x}" y1="{y0}" y2="{y1}" stroke="{c}"/>"#,
                    x = x,
                    y0 = axis_y,
                    y1 = axis_y + TICK_LENGTH,
                    c = AXIS_COLOR,
                );
                let Some((_, label_x)) = placed.next_if(|(j, _)| *j == i) else {
                    continue;
                };
                let _ = write!(
                    out,
                    r#"<text class="tick-label" x="{x}" y="{y}" text-anchor="middle" fill="{c}" stroke="{c}" font-size="{fs}" font-family="{AXIS_LABEL_FONT}">{label}</text>"#,
                    x = label_x,
                    y = axis_y + layout.time_axis_height,
                    c = TEXT_COLOR,
                    fs = label_size,
                    label = tick.label,
                );
            }
        }
        Orientation::Vertical => {
            // Baseline at the right edge of the time-axis strip.
            let axis_x = geom.data_x;
            let _ = write!(
                out,
                r#"<path class="axis" transform="translate({x},{y})" d="M-{t},0.5H0.5V{end}H-{t}" fill="none" stroke="{c}"/>"#,
                x = axis_x,
                y = geom.data_y,
                t = TICK_LENGTH,
                end = drawn_extent(layout) + 0.5,
                c = AXIS_COLOR,
            );
            let mut placed = placed.iter().peekable();
            for (i, tick) in labelled.iter().enumerate() {
                let y = geom.data_y + tick.time_pos;
                let _ = write!(
                    out,
                    r#"<line class="tick" x1="{x0}" x2="{x1}" y1="{y}" y2="{y}" stroke="{c}"/>"#,
                    x0 = axis_x - TICK_LENGTH,
                    x1 = axis_x,
                    y = y,
                    c = AXIS_COLOR,
                );
                let Some((_, label_y)) = placed.next_if(|(j, _)| *j == i) else {
                    continue;
                };
                let _ = write!(
                    out,
                    r#"<text class="tick-label" x="{x}" y="{y}" text-anchor="end" fill="{c}" stroke="{c}" font-size="{fs}" font-family="{AXIS_LABEL_FONT}" dy="0.35em">{label}</text>"#,
                    x = axis_x - VERTICAL_AXIS_LABEL_INSET,
                    y = label_y,
                    c = TEXT_COLOR,
                    fs = label_size,
                    label = tick.label,
                );
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

/// Return the id of a `<pattern>` in `defs` drawing `colors` as 45°
/// diagonal stripes, appending the def on first use. Stripes are
/// `STRIPE_WIDTH_PX` wide, one per state; tile width = `n * stripe`, tile
/// height = the field's row height. Mirrors the original
/// `fieldSvgBlocks.ts` lines 105-177.
///
/// A parallel state set (`vertical,sit`) MUST NOT render as side-by-side
/// segments: a split along the time axis is dayview's visual language for a
/// real state transition, so segments assert the state changed partway
/// through the block. Stripes interleave the members, which is what
/// concurrency looks like (da-trrs).
fn stripe_pattern_id(
    defs: &mut String,
    patterns: &mut HashMap<String, String>,
    colors: &[String],
    row_height: f64,
) -> String {
    // Patterns are `userSpaceOnUse`, so identical (colors, row_height)
    // blocks can share one def and stay in phase with each other.
    let key = format!("{}@{row_height}", colors.join(","));
    if let Some(id) = patterns.get(&key) {
        return id.clone();
    }
    let id = format!("ds-{}", patterns.len());
    let tile_w = STRIPE_WIDTH_PX * colors.len() as f64;
    let _ = write!(
        defs,
        r#"<pattern id="{id}" patternUnits="userSpaceOnUse" width="{tw}" height="{th}" patternTransform="rotate(45)">"#,
        id = id,
        tw = tile_w,
        th = row_height,
    );
    for (i, color) in colors.iter().enumerate() {
        let _ = write!(
            defs,
            r#"<rect x="{x}" y="0" width="{w}" height="{h}" fill="{c}"/>"#,
            x = i as f64 * STRIPE_WIDTH_PX,
            w = STRIPE_WIDTH_PX,
            h = row_height,
            c = color,
        );
    }
    defs.push_str("</pattern>");
    patterns.insert(key, id.clone());
    id
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
        compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput, DayviewError,
        DayviewFieldInput, DayviewPoint, DayviewSpan, Orientation,
    };

    const DAY_START: i64 = 1_745_712_000_000;
    const HOUR: i64 = 60 * 60 * 1000;

    fn one_day_with_block(error: bool, orientation: Orientation) -> DayviewConfig {
        DayviewConfig {
            days: vec![DayviewDayInput {
                date: None,
                label_lines: vec!["Mon".into(), "Apr 27".into(), "2026".into()],
                start_ms: DAY_START,
                end_ms: DAY_START + 24 * HOUR,
                // A double start on mood at 06:30, inside its block.
                errors: (error.then(|| DayviewError {
                    time_ms: DAY_START + 6 * HOUR + HOUR / 2,
                    field: "mood".into(),
                    what: "opened while already open".into(),
                }))
                .into_iter()
                .collect(),
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
                        id: None,
                        title: None,
                    }],
                    points: vec![DayviewPoint {
                        time_ms: DAY_START + 8 * HOUR,
                        fg_color: "#ff0000".into(),
                        state_label: None,
                        id: None,
                        title: None,
                        glyph: None,
                        icon: false,
                    }],
                    icon: None,
                    glyph_size: Default::default(),
                }],
                spans: Vec::new(),
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
        // A point's symbol is drawn, and nothing in the picture selects.
        assert!(svg.contains("point-fg"));
        assert!(svg[..svg.find('>').unwrap()].contains(&format!(r#"style="{NO_SELECT}""#)));
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
        assert!(!svg.contains("integrity-error"));
    }

    /// da-0lful: in either orientation the line crosses the day's whole
    /// cross axis at the finding's instant, labelled with its field and
    /// titled with what is wrong, and v2's icon sits on the field's track.
    #[test]
    fn an_error_is_a_line_across_the_day_and_an_icon_on_its_track() {
        for orientation in [Orientation::Horizontal, Orientation::Vertical] {
            let layout = compute_dayview(&one_day_with_block(true, orientation));
            let svg = render_dayview_svg(&layout);
            let doc = roxmltree::Document::parse(&svg).unwrap();
            let class = |c: &str| {
                doc.descendants()
                    .find(|n| n.attribute("class") == Some(c))
                    .unwrap_or_else(|| panic!("{orientation:?}: no .{c} in {svg}"))
            };
            let num = |n: roxmltree::Node, a: &str| n.attribute(a).unwrap().parse::<f64>().unwrap();
            let group = class("integrity-error");
            assert_eq!(group.attribute("data-field"), Some("mood"));
            let title = group.children().find(|n| n.has_tag_name("title")).unwrap();
            assert_eq!(title.text(), Some("mood: opened while already open"));
            assert_eq!(class("error-label").text(), Some("mood"));

            let line = class("error-line");
            let geom = PlotGeom::new(&layout);
            let at = layout.time_axis_extent * 6.5 / 24.0;
            let (time, cross) = match orientation {
                Orientation::Horizontal => (("x1", "x2", geom.data_x), ("y1", "y2", geom.data_y)),
                Orientation::Vertical => (("y1", "y2", geom.data_y), ("x1", "x2", geom.data_x)),
            };
            for a in [time.0, time.1] {
                assert!((num(line, a) - time.2 - at).abs() < 1e-6, "{orientation:?}");
            }
            assert_eq!(
                (num(line, cross.0) - cross.2, num(line, cross.1) - cross.2),
                (0.0, layout.day_height),
                "{orientation:?}: the line spans the whole day"
            );

            // The icon is centred on mood's track: 10% across, 15% wide.
            let icon = class("error-track");
            let t = icon.attribute("transform").unwrap();
            let xy: Vec<f64> = t["translate(".len()..t.len() - 1]
                .split(',')
                .map(|v| v.parse().unwrap())
                .collect();
            let size = 15.0 * 1.5;
            let (centre_time, centre_cross) = match orientation {
                Orientation::Horizontal => (xy[0] - geom.data_x, xy[1] - geom.data_y),
                Orientation::Vertical => (xy[1] - geom.data_y, xy[0] - geom.data_x),
            };
            assert!(
                (centre_time + size / 2.0 - at).abs() < 1e-6,
                "{orientation:?}"
            );
            assert!(
                (centre_cross + size / 2.0 - 17.5).abs() < 1e-6,
                "{orientation:?}"
            );
        }
    }

    #[test]
    fn multi_state_block_emits_diagonal_stripe_pattern() {
        // da-trrs: a parallel state set renders as 45° stripes, one per
        // member — never as segments split along the time axis, which is
        // dayview's language for a real state transition.
        let mut cfg = one_day_with_block(false, Orientation::Horizontal);
        cfg.days[0].fields[0].blocks[0].fill =
            BlockFill::MultiState(vec!["#ff0000".into(), "#00ff00".into(), "#0000ff".into()]);
        let layout = compute_dayview(&cfg);
        let svg = render_dayview_svg(&layout);
        assert!(svg.contains("<defs>"), "pattern defs emitted: {svg}");
        assert!(svg.contains("<pattern"));
        assert!(svg.contains(r#"patternTransform="rotate(45)""#));
        assert!(svg.contains("#ff0000"));
        assert!(svg.contains("#00ff00"));
        assert!(svg.contains("#0000ff"));
        assert!(svg.contains("multi-state"));
        assert!(svg.contains("url(#"));
        // Exactly one rect for the block — not one segment per state.
        assert_eq!(svg.matches(r#"class="block multi-state""#).count(), 1);
        // Stripes span the whole block; no sub-block split along time.
        let stripes = svg.matches(r#"<rect x="#).count();
        assert_eq!(stripes, 3, "one stripe rect per state: {svg}");
    }

    #[test]
    fn identical_multi_state_blocks_share_one_pattern_def() {
        // Patterns are userSpaceOnUse, so equal (colors, row height) blocks
        // share a def and stay in phase instead of bloating <defs>.
        let mut cfg = one_day_with_block(false, Orientation::Horizontal);
        let fill = BlockFill::MultiState(vec!["#aa3333".into(), "#33aa33".into()]);
        cfg.days[0].fields[0].blocks[0].fill = fill.clone();
        let mut second = cfg.days[0].fields[0].blocks[0].clone();
        second.start_ms += 3 * 3_600_000;
        second.end_ms += 3 * 3_600_000;
        second.fill = fill;
        cfg.days[0].fields[0].blocks.push(second);
        let layout = compute_dayview(&cfg);
        let svg = render_dayview_svg(&layout);
        assert_eq!(svg.matches("<pattern ").count(), 1, "one shared def: {svg}");
        assert_eq!(svg.matches(r#"class="block multi-state""#).count(), 2);
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
                date: None,
                label_lines: vec!["Mon".into()],
                start_ms: DAY_START,
                end_ms: DAY_START + 24 * HOUR,
                errors: Vec::new(),
                fields: vec![],
                spans: Vec::new(),
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

    /// da-6hg6v: a mark that HAS an entry names it; one that has none
    /// carries no `data-id` at all, so a reader tests for the attribute
    /// and never has to know a sentinel.
    #[test]
    fn a_mark_names_its_entry_and_omits_the_attribute_when_it_has_none() {
        let mut cfg = one_day_with_block(false, Orientation::Horizontal);
        let field = &mut cfg.days[0].fields[0];
        field.blocks[0].id = Some("b17ew04s0c85".into());
        let svg = render_dayview_svg(&compute_dayview(&cfg));
        assert!(svg.contains(r#"data-id="b17ew04s0c85""#), "{svg}");
        // The dot was left without one.
        let dot = svg
            .split(r#"<circle class="point-bg""#)
            .nth(1)
            .expect("a dot");
        let dot = &dot[..dot.find('>').expect("tag close")];
        assert!(!dot.contains("data-id"), "{dot}");
    }

    /// da-qietq: a dot is its symbol alone. The disc stays the mark but is
    /// never painted, and a plain dot's circle is the disc's full size.
    #[test]
    fn a_dot_paints_no_disc_and_its_circle_is_full_size() {
        let svg = render_dayview_svg(&compute_dayview(&one_day_with_block(
            false,
            Orientation::Horizontal,
        )));
        let dot = svg
            .split(r#"<circle class="point-bg""#)
            .nth(1)
            .expect("a dot");
        let (disc, fg) = dot.split_once('>').expect("tag close");
        assert!(disc.contains(r#"fill-opacity="0""#), "{disc}");
        let r = |tag: &str| {
            tag.split(r#" r=""#)
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_string()
        };
        assert!(fg.starts_with(r##"<circle class="point-fg""##), "{fg}");
        assert!(fg.contains(r##"fill="#ff0000""##), "{fg}");
        assert_eq!(r(fg), r(disc));
    }

    /// da-6hg6v: the now-chunk is the open block of the state a field is
    /// in, so it names the entry that opened that state — and, having
    /// never carried any other attribute, it carries only that one.
    #[test]
    fn the_now_chunk_names_its_entry_and_nothing_else() {
        let mut cfg = one_day_with_block(false, Orientation::Horizontal);
        cfg.right_panel_width = 40.0;
        let layout = compute_dayview(&cfg);
        let named = render_dayview_svg_with(
            &layout,
            DayviewSvgOpts {
                axis_end_reserved: 0.0,
                right_panel_fills: &[RightPanelFill {
                    field: "mood".into(),
                    fill: BlockFill::Single("#abcdef".into()),
                    id: Some("d9plbzg641as".into()),
                }],
            },
        );
        let chunk = named
            .split(r#"<rect class="now-chunk"#)
            .nth(1)
            .unwrap_or_else(|| panic!("no now-chunk in {named}"));
        let chunk = &chunk[..chunk.find('>').expect("tag close")];
        assert!(chunk.contains(r#"data-id="d9plbzg641as""#), "{chunk}");
        assert!(!chunk.contains("data-field"), "{chunk}");

        let unnamed = render_dayview_svg_with(
            &layout,
            DayviewSvgOpts {
                axis_end_reserved: 0.0,
                right_panel_fills: &[RightPanelFill {
                    field: "mood".into(),
                    fill: BlockFill::Single("#abcdef".into()),
                    id: None,
                }],
            },
        );
        assert!(unnamed.contains(r#"class="now-chunk""#), "{unnamed}");
        assert!(!unnamed.contains("data-id"), "{unnamed}");
    }

    /// `one_day_with_block` at 100px an hour, its day cut where the clock
    /// jumped: a block over 06:00–07:00 of real time and a dot at 08:00.
    fn jumped(spans: &[(i64, i64, i64, &str)]) -> String {
        render_dayview_svg(&compute_dayview(&jumped_config(spans)))
    }

    fn jumped_config(spans: &[(i64, i64, i64, &str)]) -> DayviewConfig {
        let mut cfg = DayviewConfig {
            width: 2400.0,
            margin: 0.0,
            label_width: Some(0.0),
            ..one_day_with_block(false, Orientation::Horizontal)
        };
        cfg.days[0].spans = (spans.iter())
            .map(|(start, lo, hi, label)| DayviewSpan {
                start_ms: DAY_START + start * HOUR,
                end_ms: DAY_START + (start + hi - lo) * HOUR,
                lo: *lo as f64 / 24.0,
                hi: *hi as f64 / 24.0,
                label: label.to_string(),
                // The clock its wall hours are of its real ones.
                offset_s: Some(((lo - start) * 3600) as i32),
            })
            .collect();
        cfg
    }

    /// The hatch is written whatever else `<defs>` holds: an icon whose
    /// own markup says `dead-zone` once stood in for it.
    #[test]
    fn a_dead_zone_is_hatched_beside_an_icon_that_names_one() {
        let mut cfg = jumped_config(&[(0, 0, 7, "UTC+1"), (7, 8, 24, "UTC+2")]);
        let field = &mut cfg.days[0].fields[0];
        field.icon =
            Some(r#"<svg viewBox="0 0 2 2"><rect class="dead-zone" width="2"/></svg>"#.into());
        field.points[0].icon = true;
        let svg = render_dayview_svg(&compute_dayview(&cfg));
        assert_eq!(svg.matches(r#"<pattern id="dead-zone""#).count(), 1);
    }

    /// The clock skipped 07:00–08:00: each stretch is a day group with its
    /// own straight axis, the skipped hour is hatched, and its ends say
    /// which offset each side ran at.
    #[test]
    fn a_forward_jump_draws_a_hatched_zone_between_two_axes() {
        let svg = jumped(&[(0, 0, 7, "UTC+1"), (7, 8, 24, "UTC+2")]);
        let groups: Vec<&str> = svg.split(r#"<g class="day-data""#).skip(1).collect();
        assert_eq!(groups.len(), 2);
        assert!(groups[0].starts_with(
            r#" data-day="0" data-from="2025-04-27T00:00:00.000Z" data-to="2025-04-27T07:00:00.000Z" data-axis="x" data-lo="0" data-hi="700" data-offset="0">"#
        ));
        assert!(groups[1].starts_with(
            r#" data-day="0" data-from="2025-04-27T07:00:00.000Z" data-to="2025-04-27T23:00:00.000Z" data-axis="x" data-lo="800" data-hi="2400" data-offset="3600">"#
        ));
        assert!(svg.contains(r##"<rect class="dead-zone" x="700" y="0" width="100" height="100" fill="url(#dead-zone)" pointer-events="none"/>"##));
        assert_eq!(svg.matches(r#"<pattern id="dead-zone""#).count(), 1);
        assert!(svg.contains(r#"text-anchor="start" font-size="11" fill="white" stroke="black" stroke-width="2" paint-order="stroke" pointer-events="none">UTC+1</text>"#));
        assert!(svg.contains(r#"text-anchor="end" font-size="11" fill="white" stroke="black" stroke-width="2" paint-order="stroke" pointer-events="none">UTC+2</text>"#));
        // The dot an hour after the jump is at 09:00, and nothing forks.
        assert!(groups[1].contains(r#"<circle class="point-bg" cx="900" cy="17.5" r="7.5""#));
        assert!(!svg.contains(r#"<path class="block""#));
    }

    /// The clock went from 07:00 back to 06:00: each stretch has a lane,
    /// outlined, and a mark is drawn at its lane's size.
    #[test]
    fn a_backward_jump_draws_each_stretch_in_its_own_lane() {
        let svg = jumped(&[(0, 0, 7, "UTC+2"), (7, 6, 24, "UTC+1")]);
        let groups: Vec<&str> = svg.split(r#"<g class="day-data""#).skip(1).collect();
        assert_eq!(groups.len(), 2);
        // The first 06:00–07:00 is the near half of the band; mood's own
        // 10%–25% of it is 5–12.5px there.
        assert!(groups[0].contains(
            r##"<path class="block" d="M600,5L700,5L700,12.5L600,12.5Z" fill="#abcdef""##
        ));
        // The outlines: narrowing from 05:11 to a cap at 07:00, and from a
        // cap at 06:00 to whole again by 07:53.
        assert!(groups[0].starts_with(concat!(
            r#" data-day="0" data-from="2025-04-27T00:00:00.000Z" data-to="2025-04-27T07:00:00.000Z" data-axis="x" data-lo="0" data-hi="700" data-offset="0">"#,
            r#"<path class="lane" d="M511,0L561,0L700,0L700,50L561,50L511,100" fill="none""#
        )));
        assert!(groups[1].contains(
            r#"<path class="lane" d="M789,0L739,50L600,50L600,100L739,100L789,100" fill="none""#
        ));
        // An hour after the jump is 07:00 again, in the far half.
        assert!(groups[1].contains(r#"<circle class="point-bg" cx="700" cy="58.75" r="3.75""#));
        assert!(!svg.contains("dead-zone"));
    }
}
