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
//! - Warning icon next to date labels for days with conflicts.
//! - Time-axis strip on the side opposite the date labels (bottom for
//!   horizontal, left for vertical).

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::dayview::{BlockFill, DayviewAxisTick, DayviewLayout, Orientation};

const NOW_LINE_COLOR: &str = "#ffcc00";
const BG_COLOR: &str = "black";
const TEXT_COLOR: &str = "white";
const AXIS_COLOR: &str = "white";
/// Per-stripe width (px) in multi-state diagonal patterns. Matches the
/// original TS `stripeWidth = 8`.
const STRIPE_WIDTH_PX: f64 = 8.0;

/// Advance width of one character as a fraction of the font size. The
/// renderer has no font engine, so axis-label placement (and the geometry
/// tests that check it) estimate text extents with this ratio.
pub const CHAR_ADVANCE_RATIO: f64 = 0.6;

/// Estimated rendered width (px) of `label` at `font_size`.
pub fn text_width(label: &str, font_size: f64) -> f64 {
    label.chars().count() as f64 * font_size * CHAR_ADVANCE_RATIO
}

/// The largest a vertical time axis's labels grow (da-78evn). A vertical
/// strip's `time_axis_height` is its WIDTH, which has to hold the text
/// rather than set its size, so past this the strip only gains room. It is
/// the size nowview's default 22px horizontal strip draws at.
pub const VERTICAL_AXIS_LABEL_MAX: f64 = 16.0;

/// The time axis's label font size: the strip less a little air, and in a
/// vertical strip no larger than [`VERTICAL_AXIS_LABEL_MAX`]. `0` — a
/// strip too thin for text — draws no labels.
pub fn axis_label_size(orientation: Orientation, time_axis_height: f64) -> f64 {
    if time_axis_height <= 6.0 {
        return 0.0;
    }
    let size = (time_axis_height - 6.0).max(6.0);
    match orientation {
        Orientation::Horizontal => size,
        Orientation::Vertical => size.min(VERTICAL_AXIS_LABEL_MAX),
    }
}

/// How far a vertical axis's labels sit left of its baseline: their
/// right-hand end, which is what a strip wide enough for them must clear.
pub const VERTICAL_AXIS_LABEL_INSET: f64 = 6.0;

/// Minimum blank space (px) between two adjacent axis labels, and between
/// a label and whatever an overlay reserves at the end of the axis.
pub(crate) const AXIS_LABEL_GAP: f64 = 4.0;

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
fn to_xy(layout: &DayviewLayout, geom: &PlotGeom, time: f64, cross: f64) -> (f64, f64) {
    match layout.orientation {
        Orientation::Horizontal => (geom.data_x + time, geom.data_y + cross),
        Orientation::Vertical => (geom.data_x + cross, geom.data_y + time),
    }
}

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
        }

        // Per-field bands — emitted in plot coords (no per-day transform);
        // helpers map (time, cross) → (x, y) per orientation.
        // The day's time axis in the root's user units: `data-lo` is
        // `data-from`, `data-hi` is `data-to`, along `data-axis`.
        let (axis, lo) = match layout.orientation {
            Orientation::Horizontal => ("x", layout.margin + geom.data_x),
            Orientation::Vertical => ("y", layout.margin + geom.data_y),
        };
        let _ = write!(
            &mut out,
            r#"<g class="day-data" data-day="{i}" data-from="{from}" data-to="{to}" data-axis="{axis}" data-lo="{lo}" data-hi="{hi}">"#,
            from = iso(day.start_ms),
            to = iso(day.end_ms),
            hi = lo + layout.time_axis_extent,
        );
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
                push_fill_rect(
                    &mut out,
                    &mut defs,
                    &mut patterns,
                    "block",
                    &b.fill,
                    rect_x,
                    rect_y,
                    rect_w,
                    rect_h,
                    field.cross_extent,
                    &mark_close(
                        "rect",
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
            if layout.right_panel_width > 0.0 && day.now_time_pos.is_some() {
                if let Some(pf) = opts
                    .right_panel_fills
                    .iter()
                    .find(|r| r.field == field.field)
                {
                    let (rect_x, rect_y, rect_w, rect_h) = block_rect(
                        layout,
                        &geom,
                        day.day_offset + field.cross_offset,
                        field.cross_extent,
                        layout.time_axis_extent,
                        layout.right_panel_width,
                    );
                    push_fill_rect(
                        &mut out,
                        &mut defs,
                        &mut patterns,
                        "now-chunk",
                        &pf.fill,
                        rect_x,
                        rect_y,
                        rect_w,
                        rect_h,
                        field.cross_extent,
                        &now_chunk_close(pf.id.as_deref()),
                    );
                }
            }
            for p in &field.points {
                let (cx, cy) = to_xy(layout, &geom, p.time_pos, day.day_offset + p.cross_pos);
                let outer_r = p.r;
                // The disc is the mark — the pointer target, the title, the
                // data attributes — but it is never painted: a dot is its
                // symbol alone, in its own colour (da-qietq). The symbol
                // lets the pointer through to it.
                let icon = (field.icon.as_deref()).filter(|svg| p.icon && svg.starts_with("<svg"));
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
                        r##"<use class="point-fg" href="#{id}" x="{x}" y="{y}" width="{d}" height="{d}" color="{c}" pointer-events="none"/>"##,
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
                        r#"<circle class="point-fg" cx="{cx}" cy="{cy}" r="{r}" fill="{c}" pointer-events="none"/>"#,
                        cx = cx,
                        cy = cy,
                        r = outer_r,
                        c = p.fg_color,
                    ),
                    Some(g) => write!(
                        &mut out,
                        r#"<text class="point-fg" x="{cx}" y="{cy}" dy="0.35em" text-anchor="middle" font-size="{fs}" fill="{c}" pointer-events="none">{g}</text>"#,
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
        if let Some(nt) = day.now_time_pos.filter(|_| layout.time_axis_extent > 0.0) {
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
                    // The axis strip is left of the first day, so that
                    // day's line crosses it (da-78evn).
                    let y = geom.data_y + nt;
                    let near = if day.day_offset <= 1e-9 {
                        layout.time_axis_height
                    } else {
                        0.0
                    };
                    (
                        geom.data_x + day.day_offset - near,
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
            if layout.right_panel_width > 0.0 && is_axis_day && layout.time_axis_height > 0.0 {
                let far = layout.time_axis_extent + layout.right_panel_width;
                let (hx1, hy1, hx2, hy2) = match layout.orientation {
                    Orientation::Horizontal => {
                        let y = geom.data_y + geom.cross_total;
                        (geom.data_x + nt, y, geom.data_x + far, y)
                    }
                    Orientation::Vertical => {
                        let x = geom.data_x;
                        (x, geom.data_y + nt, x, geom.data_y + far)
                    }
                };
                let _ = write!(
                    &mut out,
                    r#"<line class="current-time-line-far" x1="{hx1}" x2="{hx2}" y1="{hy1}" y2="{hy2}" stroke="{c}" stroke-width="2" stroke-dasharray="4,2"/>"#,
                    c = NOW_LINE_COLOR,
                );
            }
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

/// Emit one filled rect for a [`BlockFill`], solid or 45°-striped. `class`
/// names the element (`block` for a lane's own blocks, `now-chunk` for the
/// strip past the now-line); a striped fill appends its `<pattern>` to
/// `defs` on first use, keyed so identical fills share one def. `close`
/// ends the tag — `/>`, or a mark's [`mark_close`].
#[allow(clippy::too_many_arguments)]
fn push_fill_rect(
    out: &mut String,
    defs: &mut String,
    patterns: &mut HashMap<String, String>,
    class: &str,
    fill: &BlockFill,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    cross_extent: f64,
    close: &str,
) {
    match fill {
        BlockFill::Single(color) => {
            let _ = write!(
                out,
                r#"<rect class="{cl}" x="{x}" y="{y}" width="{w}" height="{h}" fill="{c}"{close}"#,
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
                r#"<rect class="{cl} multi-state" x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{pid})"{close}"#,
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
/// end of the range are inset to sit flush against it; if everything still
/// doesn't fit, the series is thinned to its major ticks and then greedily
/// to whatever survives.
fn fit_axis_labels(slots: &[LabelSlot], lo: f64, hi: f64) -> Vec<(usize, f64)> {
    let all: Vec<usize> = (0..slots.len()).collect();
    let placed = greedy_place(&all, slots, lo, hi);
    if placed.len() == all.len() {
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
    let n = layout.axis_ticks.len();
    // The trailing tick is the day boundary / the now-line's own position;
    // its label is never drawn. A strip too thin for text carries no ticks
    // at all, as before.
    let labelled: &[DayviewAxisTick] = if label_size > 0.0 {
        layout.axis_ticks.get(..n.saturating_sub(1)).unwrap_or(&[])
    } else {
        &[]
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
            } else if t.label.is_empty() {
                0.0
            } else {
                label_size
            },
            is_major: t.is_major,
        })
        .collect();
    // Labels stay inside the plot group and clear of any end overlay.
    let placed = fit_axis_labels(
        &slots,
        0.0,
        axis_origin + layout.time_axis_extent - opts.axis_end_reserved.max(0.0),
    );

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
            let mut placed = placed.iter().peekable();
            for (i, tick) in labelled.iter().enumerate() {
                let x = geom.data_x + tick.time_pos;
                let _ = write!(
                    out,
                    r#"<line class="tick" x1="{x}" x2="{x}" y1="{y0}" y2="{y1}" stroke="{c}"/>"#,
                    x = x,
                    y0 = axis_y,
                    y1 = axis_y + 4.0,
                    c = AXIS_COLOR,
                );
                let Some((_, label_x)) = placed.next_if(|(j, _)| *j == i) else {
                    continue;
                };
                let _ = write!(
                    out,
                    r#"<text class="tick-label" x="{x}" y="{y}" text-anchor="middle" fill="{c}" font-size="{fs}">{label}</text>"#,
                    x = label_x,
                    y = axis_y + label_size + 2.0,
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
                r#"<line class="axis" x1="{x}" x2="{x}" y1="{y0}" y2="{y1}" stroke="{c}"/>"#,
                x = axis_x,
                y0 = geom.data_y,
                y1 = geom.data_y + layout.time_axis_extent,
                c = AXIS_COLOR,
            );
            let mut placed = placed.iter().peekable();
            for (i, tick) in labelled.iter().enumerate() {
                let y = geom.data_y + tick.time_pos;
                let _ = write!(
                    out,
                    r#"<line class="tick" x1="{x0}" x2="{x1}" y1="{y}" y2="{y}" stroke="{c}"/>"#,
                    x0 = axis_x - 4.0,
                    x1 = axis_x,
                    y = y,
                    c = AXIS_COLOR,
                );
                let Some((_, label_y)) = placed.next_if(|(j, _)| *j == i) else {
                    continue;
                };
                let _ = write!(
                    out,
                    r#"<text class="tick-label" x="{x}" y="{y}" text-anchor="end" fill="{c}" font-size="{fs}" dy="0.35em">{label}</text>"#,
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
        compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput,
        DayviewFieldInput, DayviewPoint, Orientation,
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
}
