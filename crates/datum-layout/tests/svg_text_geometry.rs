//! Geometry assertions on the rendered SVG's text (per da-sv8c).
//!
//! The rest of the suite asserts on what the dayview / nowview SVG *says*.
//! Nothing asserted on where the words land, which is how a tick label
//! drawn half-outside the canvas at every width survived. An SVG is a
//! value: every `<text>` carries its x, y, font-size and text-anchor, so
//! its horizontal extent is computable without a renderer. This walks a
//! width sweep and asserts no two labels sharing a visual row overlap and
//! none falls outside the viewport.
//!
//! Text extents use the renderer's own `text_width` estimate — the same
//! ratio the axis placement uses, so the check and the fix agree on what
//! "fits" means.

use datum_layout::dayview::{
    compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput, DayviewFieldInput,
    DayviewPoint, Orientation,
};
use datum_layout::dayview_svg::{render_dayview_svg, text_width};
use datum_layout::nowview::{nowview_config, NowviewCurrentState, NowviewGeometry};
use datum_layout::nowview_svg::render_nowview_svg;

/// The widths the bead surveyed: 300 and 430 are the panes the PWA renders
/// into, the rest bracket the CLI's own defaults.
const WIDTHS: [f64; 6] = [300.0, 430.0, 600.0, 800.0, 1200.0, 1600.0];

const NOW_MS: i64 = 1_700_000_000_000;
const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;

/// One `<text>` element reduced to the box it paints.
#[derive(Debug)]
struct TextBox {
    label: String,
    x0: f64,
    x1: f64,
    y: f64,
    font_size: f64,
}

impl TextBox {
    /// Two labels share a visual row when their baselines are closer than a
    /// line of the smaller of the two.
    fn same_row(&self, other: &TextBox) -> bool {
        (self.y - other.y).abs() < self.font_size.min(other.font_size) * 0.8
    }

    fn overlaps(&self, other: &TextBox) -> bool {
        self.x0 < other.x1 && other.x0 < self.x1
    }
}

/// Pull every `<text>` out of `svg`, resolving each into absolute viewport
/// coordinates by accumulating the `translate(...)` of every `<g>` it sits
/// inside (the plot group, and the per-day label groups within it).
fn text_boxes(svg: &str) -> Vec<TextBox> {
    let mut out = Vec::new();
    let mut stack: Vec<(f64, f64)> = vec![(0.0, 0.0)];
    let mut rest = svg;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let close = rest.find('>').expect("unterminated tag");
        let tag = &rest[..close];
        let self_closing = tag.ends_with('/');
        let name = tag.split([' ', '/']).next().unwrap_or("");
        match name {
            "" if tag.starts_with("/g") => {
                stack.pop();
            }
            "g" if !self_closing => {
                let (dx, dy) = translate_of(tag);
                let top = *stack.last().unwrap();
                stack.push((top.0 + dx, top.1 + dy));
            }
            "text" => {
                let body_end = rest.find("</text>").expect("unclosed <text>");
                let label = rest[close + 1..body_end].to_string();
                let (ox, oy) = *stack.last().unwrap();
                let x = attr(tag, "x").expect("<text> without x") + ox;
                let y = attr(tag, "y").expect("<text> without y") + oy;
                let font_size = attr(tag, "font-size").expect("<text> without font-size");
                let w = text_width(&label, font_size);
                let x0 = match str_attr(tag, "text-anchor").unwrap_or("start") {
                    "middle" => x - w / 2.0,
                    "end" => x - w,
                    _ => x,
                };
                out.push(TextBox {
                    label,
                    x0,
                    x1: x0 + w,
                    y,
                    font_size,
                });
            }
            _ => {}
        }
        rest = &rest[close..];
    }
    out
}

/// `(dx, dy)` of a `transform="translate(x,y)"` attribute, `(0, 0)` when
/// the tag carries no transform.
fn translate_of(tag: &str) -> (f64, f64) {
    let Some(t) = str_attr(tag, "transform") else {
        return (0.0, 0.0);
    };
    let inner = t
        .trim_start_matches("translate(")
        .trim_end_matches(')')
        .trim();
    let mut parts = inner.split([',', ' ']).filter(|p| !p.is_empty());
    let dx = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    let dy = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0.0);
    (dx, dy)
}

fn str_attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let i = attrs.find(&needle)? + needle.len();
    let rest = &attrs[i..];
    Some(&rest[..rest.find('"')?])
}

fn attr(attrs: &str, name: &str) -> Option<f64> {
    str_attr(attrs, name)?.parse().ok()
}

fn svg_dim(svg: &str, name: &str) -> f64 {
    let head = &svg[..svg.find('>').unwrap()];
    attr(head, name).unwrap_or_else(|| panic!("no {name} on <svg>"))
}

/// Assert the whole text layer of `svg` is legible: inside the canvas, and
/// no two labels sharing a row written over each other.
fn assert_text_layer_clean(svg: &str, what: &str) {
    let width = svg_dim(svg, "width");
    let height = svg_dim(svg, "height");
    let boxes = text_boxes(svg);
    assert!(!boxes.is_empty(), "{what}: no text rendered at all");

    for b in &boxes {
        assert!(
            b.x0 >= -0.01 && b.x1 <= width + 0.01,
            "{what}: '{}' spans [{:.1}..{:.1}] outside viewport width {width}",
            b.label,
            b.x0,
            b.x1,
        );
        assert!(
            b.y <= height + 0.01,
            "{what}: '{}' baseline {:.1} below viewport height {height}",
            b.label,
            b.y,
        );
    }
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(
                !(a.same_row(b) && a.overlaps(b)),
                "{what}: '{}' [{:.1}..{:.1}] overlaps '{}' [{:.1}..{:.1}] on row y≈{:.1}",
                a.label,
                a.x0,
                a.x1,
                b.label,
                b.x0,
                b.x1,
                a.y,
            );
        }
    }
}

fn block(start_ms: i64, end_ms: i64, color: &str, state: &str) -> DayviewBlock {
    DayviewBlock {
        start_ms,
        end_ms,
        fill: BlockFill::Single(color.into()),
        state_label: Some(state.into()),
        id: None,
        title: None,
        created_ms: None,
    }
}

/// A plausible logged day: an ongoing state, a finished block, a dot.
fn day_fields(start_ms: i64, end_ms: i64) -> Vec<DayviewFieldInput> {
    let span = end_ms - start_ms;
    vec![
        DayviewFieldInput {
            field: "mood".into(),
            y_frac: 0.05,
            height_frac: 0.25,
            z_index: 0.05,
            blocks: vec![
                block(start_ms, start_ms + span / 2, "#4477aa", "ok"),
                block(start_ms + span / 2, end_ms, "#44aa77", "good"),
            ],
            points: Vec::new(),
            icon: None,
            glyph_size: Default::default(),
        },
        DayviewFieldInput {
            field: "coffee".into(),
            y_frac: 0.4,
            height_frac: 0.2,
            z_index: 0.4,
            blocks: Vec::new(),
            points: vec![DayviewPoint {
                time_ms: start_ms + span / 3,
                fg_color: "#cc7722".into(),
                state_label: None,
                id: None,
                title: None,
                created_ms: None,
                glyph: None,
                icon: false,
                moment: false,
            }],
            icon: None,
            glyph_size: Default::default(),
        },
        DayviewFieldInput {
            field: "project".into(),
            y_frac: 0.7,
            height_frac: 0.25,
            z_index: 0.7,
            blocks: vec![block(
                start_ms + span / 4,
                start_ms + 3 * span / 4,
                "#aa4477",
                "datum",
            )],
            points: Vec::new(),
            icon: None,
            glyph_size: Default::default(),
        },
    ]
}

fn dayview_at(width: f64, orientation: Orientation) -> String {
    let day_start = NOW_MS - 30 * HOUR;
    let days: Vec<DayviewDayInput> = (0..3)
        .map(|i| {
            let s = day_start + i * 24 * HOUR;
            let e = s + 24 * HOUR;
            DayviewDayInput {
                date: None,
                label_lines: vec!["Mon".into(), "04-27".into()],
                start_ms: s,
                end_ms: e,
                errors: Vec::new(),
                fields: day_fields(s, e),
                spans: Vec::new(),
            }
        })
        .collect();
    let config = DayviewConfig {
        days,
        now_ms: NOW_MS,
        width,
        height: None,
        day_height: 100.0,
        interday_margin: 15.0,
        time_axis_height: 15.0,
        margin: 2.0,
        label_width: None,
        orientation,
        axis_ticks_override: None,
        right_panel_width: 0.0,
        clock_axis_height: 0.0,
    };
    render_dayview_svg(&compute_dayview(&config))
}

fn nowview_at(width: f64, history_minutes: f64) -> String {
    let start_ms = NOW_MS - (history_minutes * 60_000.0) as i64;
    let config = nowview_config(
        day_fields(start_ms, NOW_MS),
        NOW_MS,
        NOW_MS,
        None,
        &NowviewGeometry {
            history_minutes,
            width: Some(width),
            ..Default::default()
        },
    );
    let layout = compute_dayview(&config);
    let panel = vec![
        NowviewCurrentState {
            field: "mood".into(),
            state_label: Some("good".into()),
            colors: vec!["#44aa77".into()],
            id: None,
        },
        NowviewCurrentState {
            field: "coffee".into(),
            state_label: None,
            colors: vec!["#cc7722".into()],
            id: None,
        },
    ];
    render_nowview_svg(&layout, "23:05", &panel, false, false)
}

#[test]
fn dayview_text_stays_legible_across_widths() {
    for width in WIDTHS {
        let svg = dayview_at(width, Orientation::Horizontal);
        assert_text_layer_clean(&svg, &format!("dayview @ {width}"));
    }
}

#[test]
fn vertical_dayview_text_stays_legible_across_widths() {
    for width in WIDTHS {
        let svg = dayview_at(width, Orientation::Vertical);
        assert_text_layer_clean(&svg, &format!("vertical dayview @ {width}"));
    }
}

#[test]
fn nowview_text_stays_legible_across_widths() {
    for width in WIDTHS {
        for history in [15.0, 30.0, 120.0, 480.0] {
            let svg = nowview_at(width, history);
            assert_text_layer_clean(&svg, &format!("nowview @ {width} / {history}m"));
        }
    }
}

/// The defect that was width-independent: the oldest tick sits at the very
/// left edge of the data area, so a centred label lost half of itself off
/// the canvas at every width — and read as a plausible wrong number rather
/// than as breakage.
#[test]
fn oldest_nowview_tick_label_is_never_clipped() {
    for width in WIDTHS {
        let svg = nowview_at(width, 30.0);
        let boxes = text_boxes(&svg);
        let oldest = boxes
            .iter()
            .filter(|b| b.label.starts_with('-'))
            .min_by(|a, b| a.x0.total_cmp(&b.x0));
        let Some(oldest) = oldest else { continue };
        assert!(
            oldest.x0 >= -0.01,
            "nowview @ {width}: '{}' starts at {:.1}, off the left edge",
            oldest.label,
            oldest.x0,
        );
    }
}

/// The clock is drawn at 1.5× the tick font and composites on top, so a
/// tick label under it is an unreadable glyph mash rather than a missing
/// one. It must own its room outright.
#[test]
fn clock_never_shares_room_with_a_tick_label() {
    for width in WIDTHS {
        let svg = nowview_at(width, 30.0);
        let boxes = text_boxes(&svg);
        let clock = boxes
            .iter()
            .find(|b| b.label == "23:05")
            .expect("clock label rendered");
        for b in &boxes {
            if !std::ptr::eq(b, clock) && b.label.starts_with('-') {
                assert!(
                    !(clock.same_row(b) && clock.overlaps(b)),
                    "nowview @ {width}: clock [{:.1}..{:.1}] over tick '{}' [{:.1}..{:.1}]",
                    clock.x0,
                    clock.x1,
                    b.label,
                    b.x0,
                    b.x1,
                );
            }
        }
    }
}

/// Where the time axis ends, in the plot group's coordinates: the hooked
/// path's origin plus its run (`H`, d3's half-pixel past the end).
fn axis_end(svg: &str) -> f64 {
    let tag = &svg[svg.find(r#"<path class="axis""#).expect("axis path")..];
    let tag = &tag[..tag.find('>').unwrap()];
    let d = str_attr(tag, "d").unwrap();
    let run: f64 = d[d.find('H').unwrap() + 1..d.rfind('V').unwrap()]
        .parse()
        .unwrap();
    translate_of(tag).0 + run - 0.5
}

/// The `x1` of every tick mark, in the plot group's coordinates.
fn tick_xs(svg: &str) -> Vec<f64> {
    svg.split(r#"<line class="tick" "#)
        .skip(1)
        .map(|t| attr(&t[..t.find('>').unwrap()], "x1").unwrap())
        .collect()
}

/// A full day's axis ends on midnight, an hour it reaches, so it closes on
/// a 25th tick and its `00`, inset to end where the axis does (da-i37p2).
/// Squeezed by that inset, the `00` gives way alone rather than thin the
/// other hours to every third: at 600px all 24 keep their labels. The
/// count per width is pinned, so a thinning that moved would fail here.
/// Nowview's axis ends on the now-line, an instant its clock owns: no mark.
/// That half also passes on #727's parent, so it guards a regression; the
/// dayview half is what #727 changed.
#[test]
fn a_full_day_closes_on_midnight_and_the_now_line_on_no_tick() {
    for (width, labels) in WIDTHS.into_iter().zip([9, 9, 24, 25, 25, 25]) {
        for orientation in [Orientation::Horizontal, Orientation::Vertical] {
            let svg = dayview_at(width, orientation);
            assert_eq!(tick_xs(&svg).len(), 25, "{orientation:?} dayview @ {width}");
        }
        let svg = dayview_at(width, Orientation::Horizontal);
        let end = 2.0 + axis_end(&svg); // the plot's margin
        assert!((tick_xs(&svg)[24] + 2.0 - end).abs() < 1e-6);
        let hours: Vec<TextBox> = text_boxes(&svg)
            .into_iter()
            .filter(|b| b.label.len() == 2 && b.label.chars().all(|c| c.is_ascii_digit()))
            .collect();
        assert_eq!(hours.len(), labels, "dayview @ {width}: hour labels");
        let last = hours.last().unwrap();
        if labels == 24 {
            assert_eq!(last.label, "23", "dayview @ {width}: 00 gives way alone");
        } else {
            assert_eq!(last.label, "00", "dayview @ {width}");
            assert!(
                (last.x1 - end).abs() < 1e-6,
                "dayview @ {width}: the closing 00 ends at {:.1}, the axis at {end:.1}",
                last.x1,
            );
        }

        let svg = nowview_at(width, 30.0);
        let end = axis_end(&svg);
        assert!(
            tick_xs(&svg).iter().all(|x| (x - end).abs() > 1.0),
            "nowview @ {width}: a tick under the now-line at {end:.1}",
        );
    }
}
