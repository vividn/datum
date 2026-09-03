//! The request the browser sends and the SVG that comes back.
//!
//! One JSON object in, one JSON object out — pure, and testable on the
//! host without a browser. The caller supplies *data* (which entries fall
//! in the window, which value belongs on a block, which spec overrides a
//! colour); every rule about geometry and colour lives here or in
//! `datum-layout`, so the browser never re-derives a number the CLI owns.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use datum_layout::dayview::{
    compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput, DayviewFieldInput,
    DayviewPoint, Orientation,
};
use datum_layout::dayview_svg::render_dayview_svg;
use datum_layout::nowview::{nowview_config, NowviewGeometry, NowviewPanelEntry};
use datum_layout::nowview_svg::render_nowview_svg;

use crate::colors::{
    field_hex, is_unrendered_switch_value, state_label, state_value_colors, state_value_hex,
    FieldStyle, BLACK_HEX, DEFAULT_FIELD_HEIGHT,
};

/// Which colour to use when a value resolves to nothing. The two dot
/// fallbacks in `field_visuals::render_field_visuals` differ, and only the
/// caller — which knows the verb — can pick between them.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fallback {
    /// The field's own colour (a dot whose fg is the field, or whose bg
    /// comes from `following_state`).
    #[default]
    Field,
    /// Black — a dot with no prevailing state underneath it.
    Black,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockReq {
    pub start_ms: i64,
    pub end_ms: i64,
    /// The state value this block is filled from. Absent → the field colour.
    #[serde(default)]
    pub state: Option<JsonValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PointReq {
    pub time_ms: i64,
    /// Foreground value; absent → `fg_fallback`.
    #[serde(default)]
    pub fg: Option<JsonValue>,
    #[serde(default)]
    pub fg_fallback: Fallback,
    /// Background value (the state underneath the dot); absent → `bg_fallback`.
    #[serde(default)]
    pub bg: Option<JsonValue>,
    #[serde(default)]
    pub bg_fallback: Fallback,
    /// The label shown on hover. Absent → derived from `fg`.
    #[serde(default)]
    pub state_label: Option<JsonValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldReq {
    pub field: String,
    #[serde(default)]
    pub blocks: Vec<BlockReq>,
    #[serde(default)]
    pub points: Vec<PointReq>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DayReq {
    #[serde(default)]
    pub label_lines: Vec<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    #[serde(default)]
    pub has_error: bool,
    #[serde(default)]
    pub fields: Vec<FieldReq>,
}

/// Geometry knobs for the dayview. Every one is optional and falls back to
/// the same default `datum dayview` uses.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DayviewOptions {
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub day_height: Option<f64>,
    #[serde(default)]
    pub interday_margin: Option<f64>,
    #[serde(default)]
    pub time_axis_height: Option<f64>,
    #[serde(default)]
    pub margin: Option<f64>,
    #[serde(default)]
    pub orientation: Option<Orientation>,
}

/// Geometry knobs for the nowview, mirroring the CLI's flags.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NowviewOptions {
    #[serde(default)]
    pub history_minutes: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub time_axis_height: Option<f64>,
    #[serde(default)]
    pub margin: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "view", rename_all = "lowercase")]
pub enum Request {
    Dayview {
        now_ms: i64,
        #[serde(default)]
        days: Vec<DayReq>,
        #[serde(default)]
        styles: BTreeMap<String, FieldStyle>,
        #[serde(default)]
        options: DayviewOptions,
    },
    Nowview {
        now_ms: i64,
        #[serde(default)]
        fields: Vec<FieldReq>,
        #[serde(default)]
        styles: BTreeMap<String, FieldStyle>,
        /// The prevailing state per field at `now_ms`, for the panel to the
        /// right of the now-line. A field absent from the map, or mapped to
        /// null, renders as a name with no state.
        #[serde(default)]
        panel_states: BTreeMap<String, JsonValue>,
        /// The `HH:MM` clock painted at the now-line. The browser owns this
        /// because it owns the viewer's timezone.
        #[serde(default)]
        clock: String,
        #[serde(default)]
        options: NowviewOptions,
    },
}

/// What comes back: exactly one of `svg` or `error` is set.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub svg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    fn ok(svg: String) -> Self {
        Response {
            svg: Some(svg),
            error: None,
        }
    }
    fn err(message: impl Into<String>) -> Self {
        Response {
            svg: None,
            error: Some(message.into()),
        }
    }
}

/// Render one request. Never panics on bad input — a parse failure comes
/// back as `{"error": …}`, which is what the pane shows.
pub fn render_json(body: &str) -> String {
    let response = match serde_json::from_str::<Request>(body) {
        Ok(request) => render(request),
        Err(e) => Response::err(format!("bad layout request: {e}")),
    };
    serde_json::to_string(&response)
        .unwrap_or_else(|e| format!("{{\"error\":\"unserializable response: {e}\"}}"))
}

fn render(request: Request) -> Response {
    match request {
        Request::Dayview {
            now_ms,
            days,
            styles,
            options,
        } => {
            let defaults = DayviewConfig::default();
            let config = DayviewConfig {
                days: days
                    .into_iter()
                    .map(|d| DayviewDayInput {
                        label_lines: d.label_lines,
                        start_ms: d.start_ms,
                        end_ms: d.end_ms,
                        has_error: d.has_error,
                        fields: build_fields(d.fields, &styles),
                    })
                    .collect(),
                now_ms,
                width: options.width.unwrap_or(defaults.width),
                height: options.height,
                day_height: options.day_height.unwrap_or(defaults.day_height),
                interday_margin: options.interday_margin.unwrap_or(defaults.interday_margin),
                time_axis_height: options
                    .time_axis_height
                    .unwrap_or(defaults.time_axis_height),
                margin: options.margin.unwrap_or(defaults.margin),
                label_width: None,
                orientation: options.orientation.unwrap_or(defaults.orientation),
                axis_ticks_override: None,
                right_panel_width: 0.0,
            };
            Response::ok(render_dayview_svg(&compute_dayview(&config)))
        }
        Request::Nowview {
            now_ms,
            fields,
            styles,
            panel_states,
            clock,
            options,
        } => {
            let defaults = NowviewGeometry::default();
            let geometry = NowviewGeometry {
                history_minutes: options
                    .history_minutes
                    .filter(|m| *m > 0.0)
                    .unwrap_or(defaults.history_minutes),
                width: options.width,
                height: options.height.unwrap_or(defaults.height),
                margin: options.margin.unwrap_or(defaults.margin),
                time_axis_height: options
                    .time_axis_height
                    .unwrap_or(defaults.time_axis_height),
            };
            let config = nowview_config(build_fields(fields, &styles), now_ms, &geometry);
            let layout = compute_dayview(&config);
            let panel = build_panel(&layout, &panel_states, &styles);
            Response::ok(render_nowview_svg(&layout, &clock, &panel))
        }
    }
}

/// Resolve each field's colours and cross-axis position. Mirrors
/// `dayview::build_day_fields` minus the database: fields whose blocks and
/// points both come out empty are dropped rather than drawn as empty bands.
fn build_fields(
    fields: Vec<FieldReq>,
    styles: &BTreeMap<String, FieldStyle>,
) -> Vec<DayviewFieldInput> {
    let mut out = Vec::with_capacity(fields.len());
    for f in fields {
        let style = styles.get(&f.field);
        let blocks: Vec<DayviewBlock> = f
            .blocks
            .into_iter()
            .map(|b| DayviewBlock {
                start_ms: b.start_ms,
                end_ms: b.end_ms,
                fill: block_fill(b.state.as_ref(), &f.field, style),
                state_label: state_label(b.state.as_ref()),
            })
            .collect();
        let points: Vec<DayviewPoint> = f
            .points
            .into_iter()
            .map(|p| DayviewPoint {
                time_ms: p.time_ms,
                fg_color: dot_color(p.fg.as_ref(), p.fg_fallback, &f.field, style),
                bg_color: dot_color(p.bg.as_ref(), p.bg_fallback, &f.field, style),
                state_label: state_label(p.state_label.as_ref().or(p.fg.as_ref())),
            })
            .collect();
        if blocks.is_empty() && points.is_empty() {
            continue;
        }
        let y_frac = style
            .and_then(|s| s.y)
            .unwrap_or_else(|| crate::colors::hash_y(&f.field));
        out.push(DayviewFieldInput {
            field: f.field,
            y_frac,
            height_frac: style.and_then(|s| s.height).unwrap_or(DEFAULT_FIELD_HEIGHT),
            z_index: style.and_then(|s| s.z_index).unwrap_or(y_frac),
            blocks,
            points,
        });
    }
    out
}

/// A block's fill: an array of states stripes, anything else is solid.
/// Mirrors `field_visuals::block_fill_for_value`, including its rule that
/// `false` / `IGNORED` stay visible but muted in the field's own colour.
fn block_fill(value: Option<&JsonValue>, field: &str, style: Option<&FieldStyle>) -> BlockFill {
    let field_color = || crate::colors::md5_hex(field);
    match value {
        Some(JsonValue::Array(items)) if !items.is_empty() => BlockFill::MultiState(
            items
                .iter()
                .map(|v| state_value_hex(v, field, style).unwrap_or_else(field_color))
                .collect(),
        ),
        Some(v) => {
            let muted = matches!(v, JsonValue::Bool(false))
                || matches!(v, JsonValue::String(s) if s == crate::colors::IGNORED_STATE);
            BlockFill::Single(if muted {
                field_color()
            } else {
                state_value_hex(v, field, style).unwrap_or_else(field_color)
            })
        }
        None => BlockFill::Single(field_color()),
    }
}

fn dot_color(
    value: Option<&JsonValue>,
    fallback: Fallback,
    field: &str,
    style: Option<&FieldStyle>,
) -> String {
    let fallback = || match fallback {
        Fallback::Field => field_hex(field, style),
        Fallback::Black => BLACK_HEX.to_string(),
    };
    match value {
        Some(v) => state_value_hex(v, field, style).unwrap_or_else(fallback),
        None => fallback(),
    }
}

/// One panel row per rendered field, in layout order — the same walk
/// `nowview::build_panel_entries` does over `layout.days[0].fields`.
fn build_panel(
    layout: &datum_layout::dayview::DayviewLayout,
    panel_states: &BTreeMap<String, JsonValue>,
    styles: &BTreeMap<String, FieldStyle>,
) -> Vec<NowviewPanelEntry> {
    let Some(day) = layout.days.first() else {
        return Vec::new();
    };
    day.fields
        .iter()
        .map(|f| {
            let style = styles.get(&f.field);
            let state = panel_states.get(&f.field);
            let (state_label, colors) = match state {
                Some(v) if !is_unrendered_switch_value(Some(v)) => {
                    let colors = state_value_colors(v, &f.field, style);
                    let colors = if colors.is_empty() {
                        vec![field_hex(&f.field, style)]
                    } else {
                        colors
                    };
                    (crate::colors::state_label(Some(v)), colors)
                }
                _ => (None, vec![field_hex(&f.field, style)]),
            };
            NowviewPanelEntry {
                field: f.field.clone(),
                state_label,
                colors,
            }
        })
        .collect()
}
