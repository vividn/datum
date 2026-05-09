//! `datum nowview` — render an SVG/PNG/HTML showing each tracked field's
//! recent state over the last 30 minutes (configurable via `--history`).
//!
//! Per bead da-6jy nowview's history strip is rendered by the dayview
//! pipeline ([`datum_layout::dayview::compute_dayview`] +
//! [`datum_layout::dayview_svg::render_dayview_svg`]) — a single-day
//! window scoped to the last `history_minutes`, with a relative-minute
//! axis. Nowview-specific overlay (the `HH:MM` clock label) layers on top
//! via [`datum_layout::nowview_svg::render_nowview_svg`]. This file is the
//! thin CLI wrapper: it pulls entries from SQLite, resolves field-spec
//! positions and colors the same way `datum dayview` does, and writes the
//! chosen output format.
//!
//! Terminal rendering used to live here too. Per da-raz that role belongs
//! to `datum now`, which renders a terminal nowview band by default.
//! `nowview` is now SVG-only, paralleling `dayview`.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, TimeZone, Timelike, Utc};
use datum_core::db::{Db, ListParams, TimeMetric, IGNORED_STATE_VALUE};
use datum_core::entry::{Entry, Verb};
use datum_core::field_spec::FieldSpec;
use datum_core::shape::{is_switching_verb, is_unrendered_switch_value, render_shape, RenderShape};
use datum_layout::dayview::{
    compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput, DayviewFieldInput,
    DayviewPoint, Orientation,
};
use datum_layout::nowview::{
    default_width_for_history, nowview_axis_ticks, NowviewBlock, NowviewField, NowviewPanelEntry,
    NowviewPoint,
};
use datum_layout::nowview_svg::{render_nowview_svg, with_xml_declaration, wrap_svg_in_html};
use datum_parse::time::parse_duration_str::parse_duration;
use serde_json::Value as JsonValue;

use crate::args::NowviewArgs;
use crate::render::colors::{field_rgb, md5_rgb, state_value_colors, state_value_rgb, Rgb};

/// Default history window when the user omits `--history` (per bead da-6jy
/// the user feedback was "scoped to last 30min").
const DEFAULT_HISTORY_MINUTES: f64 = 30.0;
/// Default per-field band height for the single-day nowview render. Larger
/// than dayview's 100 px default since nowview shows only one day and gets
/// the full vertical budget.
const DEFAULT_DAY_HEIGHT: f64 = 250.0;
/// Default reserved width (px) for the current-state panel rendered to the
/// right of the now-line (per bead da-34w).
const DEFAULT_PANEL_WIDTH: f64 = 120.0;
/// Width contribution of the panel toward the auto-sized SVG width when the
/// user omits `--width`. Added on top of [`default_width_for_history`] so
/// the data area keeps datum's history-based sizing.
const PANEL_DEFAULT_WIDTH_BUMP: f64 = 120.0;

/// Typed entry point — the dispatcher in `cmd.rs` calls this.
pub fn nowview(args: NowviewArgs, db: &Db) -> Result<()> {
    if args.watch {
        // Re-render whenever an external `datum` write bumps the SQLite
        // data_version (≈500ms latency), with a 60s periodic refresh so
        // the clock label stays current on an idle DB.
        crate::commands::watch::watch_loop(db, std::time::Duration::from_secs(60), || {
            render_once(&args, db)
        })
    } else {
        render_once(&args, db)
    }
}

fn render_once(args: &NowviewArgs, db: &Db) -> Result<()> {
    let history_minutes = match &args.history {
        Some(s) => duration_minutes(s).with_context(|| format!("invalid --history: '{s}'"))?,
        None => DEFAULT_HISTORY_MINUTES,
    };
    if history_minutes <= 0.0 {
        anyhow::bail!("--history must be positive (got {history_minutes} minutes)");
    }

    let width = args
        .width
        .unwrap_or_else(|| default_width_for_history(history_minutes) + PANEL_DEFAULT_WIDTH_BUMP);
    let end = Utc::now();
    let end_ms = end.timestamp_millis();
    let start_ms = end_ms - (history_minutes * 60_000.0) as i64;

    let fields = build_dayview_fields(db, end, history_minutes)?;

    // da-34w: nowview drops the date label — it's timestamp-focused, not
    // date-focused. The HH:MM clock overlay near the now-line takes its
    // place.
    let day = DayviewDayInput {
        label_lines: Vec::new(),
        start_ms,
        end_ms,
        has_error: false,
        fields,
    };

    let panel_width = DEFAULT_PANEL_WIDTH
        .min((width - 2.0 * args.margin) * 0.5)
        .max(0.0);

    // Pre-compute the time-axis extent so the relative-minute tick
    // override resolves to the same pixel space dayview will use. Mirrors
    // the math in `dayview::compute_dayview`. Date label is suppressed
    // (label_width = 0) so the time axis only loses the panel reservation.
    let plot_width = (width - 2.0 * args.margin).max(0.0);
    let time_axis_extent_for_ticks = (plot_width - panel_width).max(0.0);
    let axis_ticks_override = Some(nowview_axis_ticks(
        time_axis_extent_for_ticks,
        history_minutes,
    ));

    let day_height = if args.height > 0.0 {
        // Use the requested height as the per-day band, leaving room for
        // axis + margins. Floor at a sensible minimum so labels stay
        // readable.
        (args.height - 2.0 * args.margin - args.time_axis_height).max(50.0)
    } else {
        DEFAULT_DAY_HEIGHT
    };

    let config = DayviewConfig {
        days: vec![day],
        now_ms: end_ms,
        width,
        height: None,
        day_height,
        interday_margin: 0.0,
        time_axis_height: args.time_axis_height,
        margin: args.margin,
        // Date label suppressed — give all of the cross-perpendicular
        // budget to the data area (and the panel reservation).
        label_width: Some(0.0),
        orientation: Orientation::Horizontal,
        axis_ticks_override,
        right_panel_width: panel_width,
    };

    let layout = compute_dayview(&config);
    let panel_entries = build_panel_entries(db, end, &layout);
    let svg = render_nowview_svg(&layout, &format_local_hh_mm(end), &panel_entries);

    match args.output_file.as_deref() {
        None => {
            println!("{svg}");
            Ok(())
        }
        Some(path) => write_output(path, &svg),
    }
}

fn write_output(path: &str, svg: &str) -> Result<()> {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("svg") => {
            let body = with_xml_declaration(svg);
            std::fs::write(path, body).with_context(|| format!("write {path}"))?;
        }
        Some("html") => {
            let body = wrap_svg_in_html(svg);
            std::fs::write(path, body).with_context(|| format!("write {path}"))?;
        }
        Some("png") => {
            #[cfg(feature = "png")]
            {
                let body = with_xml_declaration(svg);
                let png = rasterize_to_png(&body)
                    .with_context(|| format!("rasterize SVG to PNG for {path}"))?;
                std::fs::write(path, png).with_context(|| format!("write {path}"))?;
            }
            #[cfg(not(feature = "png"))]
            anyhow::bail!(
                "PNG output requires the `png` cargo feature; rebuild with \
                 `--features png`, or pick a .svg / .html output path \
                 instead (got: {path})"
            );
        }
        _ => anyhow::bail!("output file must end in .svg, .png, or .html: {path}"),
    }
    Ok(())
}

/// Pure-Rust SVG → PNG via resvg + tiny-skia. Same font-fallback dance
/// `dayview` uses (resvg silently drops `<text>` nodes when its fontdb
/// has no usable face, which is exactly how labels disappeared from
/// dayview PNGs in da-6mk).
#[cfg(feature = "png")]
fn rasterize_to_png(svg_xml: &str) -> Result<Vec<u8>> {
    let mut opts = resvg::usvg::Options::default();
    {
        let fontdb = opts.fontdb_mut();
        fontdb.load_system_fonts();
        let fallback = fontdb.faces().next().and_then(|face| {
            face.families
                .iter()
                .find(|(_, lang)| *lang == resvg::usvg::fontdb::Language::English_UnitedStates)
                .or_else(|| face.families.first())
                .map(|(name, _)| name.clone())
        });
        if let Some(name) = fallback {
            fontdb.set_serif_family(name.clone());
            fontdb.set_sans_serif_family(name.clone());
            fontdb.set_monospace_family(name.clone());
            fontdb.set_cursive_family(name.clone());
            fontdb.set_fantasy_family(name.clone());
            opts.font_family = name;
        }
    }
    let tree = resvg::usvg::Tree::from_str(svg_xml, &opts)
        .map_err(|e| anyhow::anyhow!("parse SVG: {e}"))?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow::anyhow!("allocate pixmap {}x{}", size.width(), size.height()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| anyhow::anyhow!("encode PNG: {e}"))
}

/// Convert the user-supplied duration string to absolute minutes. Negative
/// inputs are normalized to positive (the panel always extends backward
/// from "now"). 0 minutes is allowed by the parser; the caller rejects.
fn duration_minutes(s: &str) -> Result<f64> {
    let dur = parse_duration(s).map_err(|e| anyhow::anyhow!("{e}"))?;
    let secs = dur.num_seconds().abs() as f64;
    Ok(secs / 60.0)
}

/// Build the per-field block/point list shaped for the dayview render path
/// (per da-6jy). Mirrors [`crate::commands::dayview::build_day_fields`] —
/// same spec loading, same per-surface visibility check, same y/height/
/// z_index resolution, same verb-driven render. The window is the nowview
/// scope `[end - history, end]` instead of a calendar day.
pub(crate) fn build_dayview_fields(
    db: &Db,
    end: DateTime<Utc>,
    history_minutes: f64,
) -> Result<Vec<DayviewFieldInput>> {
    let day_end_ms = end.timestamp_millis();
    let day_start_ms = day_end_ms - (history_minutes * 60_000.0) as i64;

    let day_start_utc = Utc
        .timestamp_millis_opt(day_start_ms)
        .single()
        .context("day_start_ms out of range")?;

    // Pull a generous leading window so an ongoing block that started
    // before the visible window can still render. Mirrors dayview's
    // 2-day buffer scaled down for the shorter nowview scope. Upper
    // bound is the caller's exact `end` instant (sub-ms precision) so
    // entries timestamped at "now" stay included — round-tripping `end`
    // through `day_end_ms` would truncate nanoseconds and exclude them.
    let buffer = chrono::Duration::seconds((history_minutes.max(1.0) * 60.0 * 4.0) as i64);
    let params = ListParams {
        field: None,
        metric: TimeMetric::Hybrid,
        start_utc: Some(day_start_utc - buffer),
        end_utc: Some(end),
        limit: None,
        head: false,
        reverse: false,
    };
    let entries = db.list_entries(&params)?;

    let mut by_field: std::collections::BTreeMap<String, Vec<Entry>> =
        std::collections::BTreeMap::new();
    for e in entries {
        by_field.entry(e.field.clone()).or_default().push(e);
    }

    // Include any tracked field with no entries in window so the layout
    // can still position it (dayview drops empty fields, but a tracked
    // field with an open switch state from before the window will have
    // a leading entry once `state_at` integration lands).
    let tracked = db.get_tracked_fields().unwrap_or_default();
    for tf in &tracked {
        by_field.entry(tf.field.clone()).or_default();
    }

    let mut out: Vec<DayviewFieldInput> = Vec::with_capacity(by_field.len());
    for (field, mut entries_for_field) in by_field {
        let spec = db.load_spec(&field).ok().flatten();
        // da-2zd: per-surface visibility. Hidden-on-nowview specs drop
        // out before per-entry work — data still flows through tail/get/
        // grep/view; only the visual band is suppressed.
        if let Some(s) = spec.as_ref() {
            if !s.renders_on("nowview") {
                continue;
            }
        }
        entries_for_field.sort_by_key(occur_time_ms);
        let switch_region_ends = switch_region_end_ms(&entries_for_field, day_end_ms);

        let (blocks, points) = render_field_visuals(
            &entries_for_field,
            &field,
            spec.as_ref(),
            day_start_ms,
            day_end_ms,
            &switch_region_ends,
            db,
        );

        if blocks.is_empty() && points.is_empty() {
            continue;
        }

        let resolved = match &spec {
            Some(s) => s.resolve_position(&field),
            None => FieldSpec::new(field.clone(), datum_core::action::Action::Occur)
                .resolve_position(&field),
        };

        out.push(DayviewFieldInput {
            field,
            y_frac: resolved.y,
            height_frac: resolved.height,
            z_index: resolved.z_index,
            blocks,
            points,
        });
    }
    Ok(out)
}

/// Build the per-field input list for the *terminal* renderer (used by
/// `datum now`'s nowview band). Returns the simpler `NowviewField` shape —
/// no positioning data — since the terminal renderer divides rows evenly
/// and sorts alphabetically.
///
/// Exposed crate-wide so `cmd_now` can reuse the exact same data shape.
pub(crate) fn build_fields(
    db: &Db,
    end: DateTime<Utc>,
    history_minutes: f64,
) -> Result<Vec<NowviewField>> {
    let day_end_ms = end.timestamp_millis();

    let window_start =
        end - chrono::Duration::seconds((history_minutes.max(1.0) * 60.0 * 4.0) as i64);
    let params = ListParams {
        field: None,
        metric: TimeMetric::Hybrid,
        start_utc: Some(window_start),
        end_utc: None,
        limit: None,
        head: false,
        reverse: false,
    };
    let entries = db.list_entries(&params)?;

    let mut by_field: std::collections::BTreeMap<String, Vec<Entry>> =
        std::collections::BTreeMap::new();
    for e in entries {
        by_field.entry(e.field.clone()).or_default().push(e);
    }

    let mut all_fields: Vec<String> = by_field.keys().cloned().collect();
    let tracked = db.get_tracked_fields()?;
    for tf in &tracked {
        if !all_fields.iter().any(|f| f == &tf.field) {
            all_fields.push(tf.field.clone());
        }
    }

    let mut out: Vec<NowviewField> = Vec::with_capacity(all_fields.len());
    for field in all_fields {
        let spec = db.load_spec(&field).ok().flatten();
        if let Some(s) = spec.as_ref() {
            if !s.renders_on("nowview") {
                continue;
            }
        }
        let mut entries_for_field = by_field.remove(&field).unwrap_or_default();
        entries_for_field.sort_by_key(occur_time_ms);
        let switch_region_ends = switch_region_end_ms(&entries_for_field, day_end_ms);

        let (blocks, points) = render_field_visuals_terminal(
            &entries_for_field,
            &field,
            spec.as_ref(),
            &switch_region_ends,
            db,
        );

        if blocks.is_empty() && points.is_empty() {
            continue;
        }

        out.push(NowviewField {
            field,
            blocks,
            points,
        });
    }
    Ok(out)
}

/// Build the right-of-now-line panel entries (per bead da-34w). For each
/// rendered field, looks up the active state at `end` and produces a
/// (field, label, color) triple. Fields with no resolvable state still
/// yield a row — the field name alone is a useful presence cue.
fn build_panel_entries(
    db: &Db,
    end: DateTime<Utc>,
    layout: &datum_layout::dayview::DayviewLayout,
) -> Vec<NowviewPanelEntry> {
    let day = match layout.days.first() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let mut out: Vec<NowviewPanelEntry> = Vec::with_capacity(day.fields.len());
    for f in &day.fields {
        let spec = db.load_spec(&f.field).ok().flatten();
        let state = db.state_at(&f.field, end).ok().flatten();
        let (state_label, colors) = match state.as_ref() {
            None => (None, vec![rgb_to_hex(field_rgb(&f.field, spec.as_ref()))]),
            Some(v) if is_unrendered_switch_value(Some(v)) => {
                (None, vec![rgb_to_hex(field_rgb(&f.field, spec.as_ref()))])
            }
            Some(v) => {
                let label = match v {
                    JsonValue::String(s) => s.clone(),
                    other => other.to_string(),
                };
                // Per da-9vop: parallel state sets keep every member's
                // colour so the swatch can render as a checkerboard.
                let rgbs = state_value_colors(v, &f.field, spec.as_ref());
                let hex = if rgbs.is_empty() {
                    vec![rgb_to_hex(field_rgb(&f.field, spec.as_ref()))]
                } else {
                    rgbs.into_iter().map(rgb_to_hex).collect()
                };
                (Some(label), hex)
            }
        };
        out.push(NowviewPanelEntry {
            field: f.field.clone(),
            state_label,
            colors,
        });
    }
    out
}

fn rgb_to_hex(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)
}

fn is_ignored_or_false(v: &JsonValue) -> bool {
    matches!(v, JsonValue::Bool(false))
        || matches!(v, JsonValue::String(s) if s == IGNORED_STATE_VALUE)
}

/// Resolve a block's fill from its value — array → multi-state stripes,
/// scalar → solid color. Mirrors dayview's `block_fill_for_value`.
fn block_fill_for_value(
    value: Option<&JsonValue>,
    field: &str,
    spec: Option<&FieldSpec>,
) -> BlockFill {
    match value {
        Some(JsonValue::Array(items)) if !items.is_empty() => {
            let colors: Vec<String> = items
                .iter()
                .map(|v| {
                    let rgb = state_value_rgb(v, field, spec).unwrap_or(md5_rgb(field));
                    rgb_to_hex(rgb)
                })
                .collect();
            BlockFill::MultiState(colors)
        }
        Some(v) => {
            let rgb = if is_ignored_or_false(v) {
                md5_rgb(field)
            } else {
                state_value_rgb(v, field, spec).unwrap_or(md5_rgb(field))
            };
            BlockFill::Single(rgb_to_hex(rgb))
        }
        None => BlockFill::Single(rgb_to_hex(md5_rgb(field))),
    }
}

/// For each entry, the UTC-ms end-time of its switch region (the occur_time
/// of the next switching event on the same field, or `fallback_end_ms` if
/// it's the last). Input must be sorted by occur_time ascending.
fn switch_region_end_ms(entries: &[Entry], fallback_end_ms: i64) -> Vec<i64> {
    let mut out = vec![fallback_end_ms; entries.len()];
    let mut next_occur_ms: Option<i64> = None;
    for (i, entry) in entries.iter().enumerate().rev() {
        if is_switching_verb(canonical_verb(entry)) {
            out[i] = next_occur_ms.unwrap_or(fallback_end_ms);
            if let Some(ms) = occur_time_ms_opt(entry) {
                next_occur_ms = Some(ms);
            }
        }
    }
    out
}

fn canonical_verb(entry: &Entry) -> Verb {
    entry.verb
}

fn occur_time_ms(entry: &Entry) -> i64 {
    occur_time_ms_opt(entry).unwrap_or(i64::MIN)
}

fn occur_time_ms_opt(entry: &Entry) -> Option<i64> {
    entry.occur_time.as_ref().map(|t| t.utc.timestamp_millis())
}

fn dur_seconds(entry: &Entry) -> Option<i64> {
    entry.duration_seconds().map(|s| s.max(0))
}

/// Verb-driven render of one field's blocks and points for the dayview
/// pipeline (per da-6jy nowview reuses this shape). Mirrors dayview's
/// `render_field_visuals_dayview`; differences: window-clipping is by the
/// nowview scope rather than a calendar day. The dayview layout will clip
/// blocks that overflow the window automatically.
fn render_field_visuals(
    entries: &[Entry],
    field: &str,
    spec: Option<&FieldSpec>,
    day_start_ms: i64,
    day_end_ms: i64,
    switch_region_ends: &[i64],
    db: &Db,
) -> (Vec<DayviewBlock>, Vec<DayviewPoint>) {
    let mut blocks: Vec<DayviewBlock> = Vec::new();
    let mut points: Vec<DayviewPoint> = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        let Some(occur_dt) = entry.occur_time.as_ref() else {
            continue;
        };
        let occur_ms = occur_dt.utc.timestamp_millis();

        let verb = canonical_verb(entry);
        // Centralized Hidden gate (datum_core::shape::render_shape).
        if render_shape(verb, entry.entry_shape, &entry.state) == RenderShape::Hidden {
            continue;
        }

        match verb {
            Verb::Block => {
                push_block_window(
                    &mut blocks,
                    entry,
                    occur_ms,
                    field,
                    spec,
                    day_start_ms,
                    day_end_ms,
                );
            }
            Verb::BlockSwitch => {
                push_block_window(
                    &mut blocks,
                    entry,
                    occur_ms,
                    field,
                    spec,
                    day_start_ms,
                    day_end_ms,
                );
                push_switch_region(
                    &mut blocks,
                    entry.following_state.as_ref(),
                    occur_ms,
                    switch_region_ends[idx],
                    field,
                    spec,
                    day_start_ms,
                    day_end_ms,
                );
            }
            Verb::PointSwitch => {
                if occur_ms >= day_start_ms && occur_ms <= day_end_ms {
                    let fg_rgb = entry
                        .state
                        .as_ref()
                        .and_then(|v| state_value_rgb(v, field, spec))
                        .unwrap_or_else(|| field_rgb(field, spec));
                    let bg_rgb = entry
                        .following_state
                        .as_ref()
                        .and_then(|v| state_value_rgb(v, field, spec))
                        .unwrap_or_else(|| field_rgb(field, spec));
                    points.push(DayviewPoint {
                        time_ms: occur_ms,
                        fg_color: rgb_to_hex(fg_rgb),
                        bg_color: rgb_to_hex(bg_rgb),
                        state_label: state_label(entry.state.as_ref()),
                    });
                }
                push_switch_region(
                    &mut blocks,
                    entry.following_state.as_ref(),
                    occur_ms,
                    switch_region_ends[idx],
                    field,
                    spec,
                    day_start_ms,
                    day_end_ms,
                );
            }
            Verb::Switch => {
                push_switch_region(
                    &mut blocks,
                    entry.state.as_ref(),
                    occur_ms,
                    switch_region_ends[idx],
                    field,
                    spec,
                    day_start_ms,
                    day_end_ms,
                );
            }
            Verb::Point => {
                if occur_ms < day_start_ms || occur_ms > day_end_ms {
                    continue;
                }
                let fg = field_rgb(field, spec);
                let bg = match db.state_at(field, occur_dt.utc).unwrap_or(None) {
                    Some(v) => state_value_rgb(&v, field, spec).unwrap_or(Rgb::BLACK),
                    None => Rgb::BLACK,
                };
                points.push(DayviewPoint {
                    time_ms: occur_ms,
                    fg_color: rgb_to_hex(fg),
                    bg_color: rgb_to_hex(bg),
                    state_label: state_label(entry.state.as_ref()),
                });
            }
        }
    }
    (blocks, points)
}

fn push_block_window(
    blocks: &mut Vec<DayviewBlock>,
    entry: &Entry,
    occur_ms: i64,
    field: &str,
    spec: Option<&FieldSpec>,
    day_start_ms: i64,
    day_end_ms: i64,
) {
    let Some(dur_secs) = dur_seconds(entry) else {
        return;
    };
    if dur_secs == 0 {
        return;
    }
    let start_ms = occur_ms - dur_secs * 1000;
    let end_ms = occur_ms;
    if end_ms <= day_start_ms || start_ms >= day_end_ms {
        return;
    }
    let fill = block_fill_for_value(entry.state.as_ref(), field, spec);
    blocks.push(DayviewBlock {
        start_ms,
        end_ms,
        fill,
        state_label: state_label(entry.state.as_ref()),
    });
}

// Mirrors dayview's helper of the same shape — same trade-off applies.
#[allow(clippy::too_many_arguments)]
fn push_switch_region(
    blocks: &mut Vec<DayviewBlock>,
    value: Option<&JsonValue>,
    start_ms: i64,
    region_end_ms: i64,
    field: &str,
    spec: Option<&FieldSpec>,
    day_start_ms: i64,
    day_end_ms: i64,
) {
    if is_unrendered_switch_value(value) {
        return;
    }
    if region_end_ms <= day_start_ms || start_ms >= day_end_ms {
        return;
    }
    let fill = block_fill_for_value(value, field, spec);
    blocks.push(DayviewBlock {
        start_ms,
        end_ms: region_end_ms,
        fill,
        state_label: state_label(value),
    });
}

/// Verb-driven render targeting the terminal-band [`NowviewField`] shape
/// (used by `datum now`). Color resolution mirrors the dayview path; the
/// shape is simpler because the terminal renderer doesn't carry block
/// fills as patterns.
fn render_field_visuals_terminal(
    entries: &[Entry],
    field: &str,
    spec: Option<&FieldSpec>,
    switch_region_ends: &[i64],
    db: &Db,
) -> (Vec<NowviewBlock>, Vec<NowviewPoint>) {
    let mut blocks: Vec<NowviewBlock> = Vec::new();
    let mut points: Vec<NowviewPoint> = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        let Some(occur_dt) = entry.occur_time.as_ref() else {
            continue;
        };
        let occur_ms = occur_dt.utc.timestamp_millis();

        let verb = canonical_verb(entry);
        // Centralized Hidden gate (datum_core::shape::render_shape).
        if render_shape(verb, entry.entry_shape, &entry.state) == RenderShape::Hidden {
            continue;
        }

        match verb {
            Verb::Block => {
                push_block_terminal(&mut blocks, entry, occur_ms, field, spec);
            }
            Verb::BlockSwitch => {
                push_block_terminal(&mut blocks, entry, occur_ms, field, spec);
                push_switch_region_terminal(
                    &mut blocks,
                    entry.following_state.as_ref(),
                    occur_ms,
                    switch_region_ends[idx],
                    field,
                    spec,
                );
            }
            Verb::PointSwitch => {
                let fg_rgb = entry
                    .state
                    .as_ref()
                    .and_then(|v| state_value_rgb(v, field, spec))
                    .unwrap_or_else(|| field_rgb(field, spec));
                let bg_rgb = entry
                    .following_state
                    .as_ref()
                    .and_then(|v| state_value_rgb(v, field, spec))
                    .unwrap_or_else(|| field_rgb(field, spec));
                points.push(NowviewPoint {
                    time_ms: occur_ms,
                    fg_color: rgb_to_hex(fg_rgb),
                    bg_color: rgb_to_hex(bg_rgb),
                    state_label: state_label(entry.state.as_ref()),
                });
                push_switch_region_terminal(
                    &mut blocks,
                    entry.following_state.as_ref(),
                    occur_ms,
                    switch_region_ends[idx],
                    field,
                    spec,
                );
            }
            Verb::Switch => {
                push_switch_region_terminal(
                    &mut blocks,
                    entry.state.as_ref(),
                    occur_ms,
                    switch_region_ends[idx],
                    field,
                    spec,
                );
            }
            Verb::Point => {
                let fg = state_value_rgb(
                    entry.state.as_ref().unwrap_or(&JsonValue::Null),
                    field,
                    spec,
                )
                .unwrap_or_else(|| md5_rgb(field));
                let bg = match db.state_at(field, occur_dt.utc).unwrap_or(None) {
                    Some(v) => state_value_rgb(&v, field, spec).unwrap_or(Rgb::BLACK),
                    None => Rgb::BLACK,
                };
                points.push(NowviewPoint {
                    time_ms: occur_ms,
                    fg_color: rgb_to_hex(fg),
                    bg_color: rgb_to_hex(bg),
                    state_label: state_label(entry.state.as_ref()),
                });
            }
        }
    }
    (blocks, points)
}

fn push_block_terminal(
    blocks: &mut Vec<NowviewBlock>,
    entry: &Entry,
    occur_ms: i64,
    field: &str,
    spec: Option<&FieldSpec>,
) {
    let Some(dur_secs) = dur_seconds(entry) else {
        return;
    };
    if dur_secs == 0 {
        return;
    }
    let start_ms = occur_ms - dur_secs * 1000;
    let colors = match entry.state.as_ref() {
        Some(v) => {
            let rgbs = state_value_colors(v, field, spec);
            if rgbs.is_empty() {
                vec![rgb_to_hex(md5_rgb(field))]
            } else {
                rgbs.into_iter().map(rgb_to_hex).collect()
            }
        }
        None => vec![rgb_to_hex(md5_rgb(field))],
    };
    blocks.push(NowviewBlock {
        start_ms,
        end_ms: occur_ms,
        colors,
        state_label: state_label(entry.state.as_ref()),
    });
}

fn push_switch_region_terminal(
    blocks: &mut Vec<NowviewBlock>,
    value: Option<&JsonValue>,
    start_ms: i64,
    region_end_ms: i64,
    field: &str,
    spec: Option<&FieldSpec>,
) {
    if is_unrendered_switch_value(value) {
        return;
    }
    if region_end_ms <= start_ms {
        return;
    }
    let colors = match value {
        Some(v) => {
            let rgbs = state_value_colors(v, field, spec);
            if rgbs.is_empty() {
                vec![rgb_to_hex(md5_rgb(field))]
            } else {
                rgbs.into_iter().map(rgb_to_hex).collect()
            }
        }
        None => vec![rgb_to_hex(md5_rgb(field))],
    };
    blocks.push(NowviewBlock {
        start_ms,
        end_ms: region_end_ms,
        colors,
        state_label: state_label(value),
    });
}

fn state_label(v: Option<&JsonValue>) -> Option<String> {
    v.map(|val| match val {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    })
}

pub(crate) fn format_local_hh_mm(t: DateTime<Utc>) -> String {
    let local = t.with_timezone(&Local);
    format!("{}:{:02}", local.hour(), local.minute())
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::action::Action;

    /// Without the `png` feature, requesting a `.png` output path must
    /// return a friendly error pointing at the missing feature, not panic.
    #[cfg(not(feature = "png"))]
    #[test]
    fn png_output_without_feature_returns_friendly_error() {
        let err = write_output("/tmp/datum-feature-gate-check.png", "<svg/>")
            .expect_err("expected friendly error when png feature is off");
        let msg = err.to_string();
        assert!(msg.contains("png"), "missing 'png' in error: {msg}");
        assert!(msg.contains("feature"), "missing 'feature' in error: {msg}");
    }

    #[test]
    fn duration_minutes_supports_common_forms() {
        assert!((duration_minutes("15m").unwrap() - 15.0).abs() < 1e-6);
        assert!((duration_minutes("1h").unwrap() - 60.0).abs() < 1e-6);
        assert!((duration_minutes("PT30M").unwrap() - 30.0).abs() < 1e-6);
        assert!((duration_minutes("1:30").unwrap() - 90.0).abs() < 1e-6);
    }

    #[test]
    fn duration_minutes_rejects_garbage() {
        assert!(duration_minutes("nope").is_err());
    }

    #[test]
    fn rgb_to_hex_emits_six_digit_lowercase() {
        assert_eq!(
            rgb_to_hex(Rgb {
                r: 0xab,
                g: 0x12,
                b: 0xcd
            }),
            "#ab12cd"
        );
        assert_eq!(rgb_to_hex(Rgb { r: 0, g: 0, b: 0 }), "#000000");
    }

    #[test]
    fn state_label_serializes_non_string_values() {
        assert_eq!(
            state_label(Some(&JsonValue::String("happy".into()))),
            Some("happy".into())
        );
        assert_eq!(
            state_label(Some(&JsonValue::Number(serde_json::Number::from(7)))),
            Some("7".into())
        );
        assert_eq!(state_label(None), None);
    }

    #[test]
    fn format_local_hh_mm_pads_minute() {
        let t = chrono::Utc::now();
        let s = format_local_hh_mm(t);
        let parts: Vec<&str> = s.split(':').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].len(), 2);
    }

    /// da-2zd: `render.nowview = false` drops the field from the dayview-
    /// shape build path used by the SVG render.
    #[test]
    fn render_nowview_false_drops_field_from_dayview_fields() {
        use chrono::Utc;
        use datum_core::entry::{Entry, Verb};
        use datum_core::field_spec::{FieldSpec, Render};
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();

        let mut muted = FieldSpec::new("muted", Action::Occur);
        let mut map = std::collections::HashMap::new();
        map.insert("nowview".to_string(), false);
        muted.render = Some(Render::PerSurface(map));
        db.save_field_spec(&muted).unwrap();

        db.save_field_spec(&FieldSpec::new("visible", Action::Occur))
            .unwrap();

        let now = Utc::now();
        let dt = DatumTime::from_utc_and_tz(now, "UTC").unwrap();
        for f in ["muted", "visible"] {
            db.insert_entry(&Entry::new(
                f,
                Verb::Point,
                None,
                None,
                None,
                Some(dt.clone()),
                None,
                "test",
            ))
            .unwrap();
        }

        let fields = build_dayview_fields(&db, now, 60.0).unwrap();
        let names: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
        assert!(names.contains(&"visible"), "names = {names:?}");
        assert!(
            !names.contains(&"muted"),
            "render.nowview=false must drop muted, names = {names:?}",
        );
    }

    /// da-6jy: per bead spec, the dayview-shape build path resolves field
    /// positions from the spec the same way `datum dayview` does. Verifies
    /// `y_frac`/`height_frac`/`z_index` come from the saved spec rather
    /// than the layout's old "even-divide" fallback.
    #[test]
    fn build_dayview_fields_resolves_spec_positions() {
        use chrono::Utc;
        use datum_core::entry::{Entry, Verb};
        use datum_core::field_spec::FieldSpec;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("focus", Action::Switch);
        spec.y = Some(0.4);
        spec.height = Some(0.25);
        spec.z_index = Some(0.7);
        db.save_field_spec(&spec).unwrap();

        let now = Utc::now();
        let dt = DatumTime::from_utc_and_tz(now - chrono::Duration::minutes(15), "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "focus",
            Verb::Switch,
            Some(JsonValue::String("deep".into())),
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let fields = build_dayview_fields(&db, now, 30.0).unwrap();
        let focus = fields.iter().find(|f| f.field == "focus").unwrap();
        assert!((focus.y_frac - 0.4).abs() < 1e-9);
        assert!((focus.height_frac - 0.25).abs() < 1e-9);
        assert!((focus.z_index - 0.7).abs() < 1e-9);
    }

    /// da-6jy: spec colors flow through to the dayview-shape blocks the
    /// same way they do in dayview. Confirms the shared color resolver is
    /// wired up.
    #[test]
    fn build_dayview_fields_applies_spec_colors() {
        use chrono::Utc;
        use datum_core::entry::{Entry, Verb};
        use datum_core::field_spec::FieldSpec;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("caffeine", Action::Occur);
        spec.color = Some("#643200".into());
        db.save_field_spec(&spec).unwrap();

        let now = Utc::now();
        let dt = DatumTime::from_utc_and_tz(now - chrono::Duration::minutes(10), "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "caffeine",
            Verb::Point,
            None,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let fields = build_dayview_fields(&db, now, 30.0).unwrap();
        let c = fields.iter().find(|f| f.field == "caffeine").unwrap();
        assert_eq!(c.points.len(), 1);
        assert_eq!(c.points[0].fg_color, "#643200");
    }

    /// da-2zd: bool-form `render = false` drops from the terminal-shape
    /// builder too (used by `datum now`'s band).
    #[test]
    fn render_all_false_drops_field_from_terminal_fields() {
        use chrono::Utc;
        use datum_core::entry::{Entry, Verb};
        use datum_core::field_spec::{FieldSpec, Render};
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("background_telemetry", Action::Occur);
        spec.render = Some(Render::All(false));
        db.save_field_spec(&spec).unwrap();

        let now = Utc::now();
        let dt = DatumTime::from_utc_and_tz(now, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "background_telemetry",
            Verb::Point,
            None,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let fields = build_fields(&db, now, 60.0).unwrap();
        assert!(fields.iter().all(|f| f.field != "background_telemetry"));
    }

    fn stub_entry(
        field: &str,
        verb: Verb,
        occur_utc: chrono::DateTime<Utc>,
        dur: Option<&str>,
        state: Option<JsonValue>,
        following_state: Option<JsonValue>,
    ) -> Entry {
        use datum_core::entry::Entry;
        use datum_core::time::DatumTime;
        let dt = DatumTime::from_utc_and_tz(occur_utc, "UTC").unwrap();
        let mut e = Entry::new(
            field,
            verb,
            state.clone(),
            None,
            None,
            Some(dt),
            dur.and_then(datum_core::dur::Dur::parse),
            "test",
        );
        e.verb = verb;
        e.following_state = following_state;
        e
    }

    /// Terminal-shape Block emits one closed segment at [occur - dur, occur].
    #[test]
    fn terminal_block_renders_at_occur_minus_dur_to_occur() {
        use chrono::TimeZone;
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 12, 30, 0).unwrap();
        let entries = vec![stub_entry(
            "lunch",
            Verb::Block,
            occur,
            Some("PT30M"),
            Some(JsonValue::String("eating".into())),
            None,
        )];
        let now_ms = Utc
            .with_ymd_and_hms(2026, 4, 27, 13, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, now_ms);
        let (blocks, points) = render_field_visuals_terminal(&entries, "lunch", None, &ends, &db);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].start_ms,
            occur.timestamp_millis() - 30 * 60 * 1000
        );
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
    }

    /// Terminal-shape PointSwitch emits a dot AND opens a switch region.
    #[test]
    fn terminal_point_switch_emits_dot_and_following_region() {
        use chrono::TimeZone;
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 10, 0, 0).unwrap();
        let entries = vec![stub_entry(
            "task",
            Verb::PointSwitch,
            occur,
            Some("PT0S"),
            Some(JsonValue::String("commit".into())),
            Some(JsonValue::String("review".into())),
        )];
        let now_ms = Utc
            .with_ymd_and_hms(2026, 4, 27, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, now_ms);
        let (blocks, points) = render_field_visuals_terminal(&entries, "task", None, &ends, &db);
        assert_eq!(points.len(), 1);
        assert_ne!(points[0].fg_color, points[0].bg_color);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].state_label.as_deref(), Some("review"));
    }

    /// Dayview-shape BlockSwitch produces both leading block and following
    /// region — same behavior as `datum dayview`.
    #[test]
    fn dayview_shape_block_switch_emits_block_and_following_region() {
        use chrono::TimeZone;
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let entries = vec![stub_entry(
            "mood",
            Verb::BlockSwitch,
            occur,
            Some("PT1H"),
            Some(JsonValue::String("groggy".into())),
            Some(JsonValue::String("focused".into())),
        )];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, points) =
            render_field_visuals(&entries, "mood", None, day_start, day_end, &ends, &db);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].start_ms, occur.timestamp_millis());
        assert_eq!(blocks[1].end_ms, day_end);
    }
}
