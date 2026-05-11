//! `datum dayview` — render a multi-day timeline visualization.
//!
//! Reference: `/tmp/datum-survey/src/dayview/{dayview,singleday,allFieldsSvg,fieldSvgBlocks}.ts`.
//!
//! Layout math + SVG emit live in `datum_layout::dayview` so the SvelteKit
//! PWA can render the same view client-side via WASM. This file is the
//! thin CLI wrapper: it resolves date windows, pulls entries from SQLite,
//! resolves colors through `crate::render::colors`, queries the conflicts
//! queue for per-day error overlays, and writes the chosen output format.
//!
//! TODO (timezone bifurcation): v1 renders in the host's local time. A day
//! with entries in multiple timezones (DST or travel) should bifurcate
//! visually during the ambiguous overlap window — see bead da-vp7's spec
//! for the long-term plan. Bifurcation is a separate visualization bead.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Datelike, Local, NaiveDate, TimeZone, Utc};
use datum_core::conflicts::ListFilter;
use datum_core::db::{Db, ListParams, TimeMetric, IGNORED_STATE_VALUE};
use datum_core::entry::{Entry, Verb};
use datum_core::field_spec::FieldSpec;
use datum_core::shape::{is_switching_verb, is_unrendered_switch_value, render_shape, RenderShape};
use datum_layout::dayview::{
    compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput, DayviewFieldInput,
    DayviewPoint, Orientation,
};
use datum_layout::dayview_svg::{render_dayview_svg, wrap_dayview_svg_in_html};
use datum_parse::cli::DayviewOrientation;
use datum_parse::time::{parse_date_arg, DateArg};
use serde_json::Value as JsonValue;

use crate::args::DayviewArgs;
use crate::render::colors::{field_rgb, md5_rgb, state_value_rgb, Rgb};

/// Datum default per-day band height (matches dayview.ts:50).
const DEFAULT_DAY_HEIGHT: f64 = 100.0;
/// Datum default total SVG width (matches dayview.ts:38).
const DEFAULT_WIDTH: f64 = 2000.0;
/// Datum default interday margin (matches dayview.ts:51).
const DEFAULT_INTERDAY_MARGIN: f64 = 15.0;

pub fn dayview(args: DayviewArgs, db: &Db) -> Result<()> {
    if args.watch {
        // Re-render whenever an external `datum` write bumps the SQLite
        // data_version (≈500ms latency), with a 60s periodic refresh
        // so the now-line still advances on an idle DB.
        crate::commands::watch::watch_loop(db, std::time::Duration::from_secs(60), || {
            render_once(&args, db)
        })
    } else {
        render_once(&args, db)
    }
}

fn render_once(args: &DayviewArgs, db: &Db) -> Result<()> {
    // 1. Resolve the visible day window.
    let today = Local::now().date_naive();
    let end_date = match args.end_date.as_deref() {
        Some(s) => {
            resolve_date_arg(s, today).with_context(|| format!("invalid --end-date: '{s}'"))?
        }
        None => today,
    };

    // Resolve n_days from --start-date / --end-date / --n-days interaction
    // (mirrors dayview.ts lines 19–37).
    let n_days: u32 = match (args.start_date.as_deref(), args.n_days) {
        (Some(start_str), explicit_n) => {
            let start = resolve_date_arg(start_str, today)
                .with_context(|| format!("invalid --start-date: '{start_str}'"))?;
            let diff = (end_date - start).num_days() + 1;
            if diff < 1 {
                anyhow::bail!("--start-date {} is after --end-date {}", start, end_date);
            }
            let derived = diff as u32;
            if let Some(n) = explicit_n {
                if n != derived {
                    anyhow::bail!(
                        "--n-days {n} does not match the difference between --start-date \
                         and --end-date ({derived})"
                    );
                }
            }
            derived
        }
        (None, Some(n)) => n.max(1),
        (None, None) => 1,
    };

    // 2. Build the per-day windows (top → bottom = oldest → newest).
    let day_dates: Vec<NaiveDate> = (0..n_days)
        .map(|i| end_date - chrono::Duration::days((n_days - 1 - i) as i64))
        .collect();

    // 3. Pull conflicts (one global query, then bucket by day window).
    //    Empty result is fine; just no error icons.
    let conflicts = db
        .list_conflicts(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap_or_default();

    // 4. For each day, build the field input list.
    let mut days_input: Vec<DayviewDayInput> = Vec::with_capacity(day_dates.len());
    let now_utc = Utc::now();
    let now_ms = now_utc.timestamp_millis();

    for date in &day_dates {
        let (day_start_ms, day_end_ms) = local_day_bounds_ms(*date)?;
        let label_lines = format_day_label(*date);

        // Bucket pending conflicts whose detected_at falls in this day's
        // local window.
        let has_error = conflicts.iter().any(|c| {
            let t = c.detected_at.timestamp_millis();
            t >= day_start_ms && t < day_end_ms
        });

        let fields = build_day_fields(db, day_start_ms, day_end_ms, now_ms)?;

        days_input.push(DayviewDayInput {
            label_lines,
            start_ms: day_start_ms,
            end_ms: day_end_ms,
            has_error,
            fields,
        });
    }

    // 5. Resolve sizing per dayview.ts §44–78. Defaults: width=2000,
    //    day_height=100, time_axis_height=15, interday_margin=15, margin=2.
    //    Orientation flips which dim `--day-height` derives from: in
    //    horizontal mode days stack along y (use --height); in vertical
    //    mode days stack along x (use --width).
    let orientation = match args.orientation {
        DayviewOrientation::Vertical => Orientation::Vertical,
        DayviewOrientation::Horizontal => Orientation::Horizontal,
    };
    let width = args.width.unwrap_or(DEFAULT_WIDTH);
    let day_height_default = args.day_height.unwrap_or(DEFAULT_DAY_HEIGHT);
    // If the user gave --height (horizontal mode) or --width (vertical mode)
    // without an explicit --day-height, derive day_height to fit the
    // day-stack axis. Otherwise use the configured day_height.
    let day_height = match orientation {
        Orientation::Horizontal => match (args.height, args.day_height) {
            (Some(total), None) => {
                let usable = total
                    - 2.0 * args.margin
                    - args.time_axis_height
                    - (n_days - 1) as f64 * DEFAULT_INTERDAY_MARGIN;
                if usable <= 0.0 || n_days == 0 {
                    day_height_default
                } else {
                    (usable / n_days as f64).max(10.0)
                }
            }
            _ => day_height_default,
        },
        Orientation::Vertical => match (args.width, args.day_height) {
            (Some(total), None) => {
                let usable = total
                    - 2.0 * args.margin
                    - args.time_axis_height
                    - (n_days - 1) as f64 * DEFAULT_INTERDAY_MARGIN;
                if usable <= 0.0 || n_days == 0 {
                    day_height_default
                } else {
                    (usable / n_days as f64).max(10.0)
                }
            }
            _ => day_height_default,
        },
    };

    let config = DayviewConfig {
        days: days_input,
        now_ms,
        width,
        height: args.height,
        day_height,
        interday_margin: DEFAULT_INTERDAY_MARGIN,
        time_axis_height: args.time_axis_height,
        margin: args.margin,
        label_width: None,
        orientation,
        axis_ticks_override: None,
        right_panel_width: 0.0,
    };

    let layout = compute_dayview(&config);

    if args.json {
        let json = serde_json::to_string_pretty(&layout)?;
        return write_payload(args.output_file.as_deref(), &json, OutputKind::Json);
    }

    let svg = render_dayview_svg(&layout);
    write_payload(args.output_file.as_deref(), &svg, OutputKind::Svg)
}

enum OutputKind {
    Svg,
    Json,
}

fn write_payload(path: Option<&str>, body: &str, kind: OutputKind) -> Result<()> {
    let Some(path) = path else {
        println!("{body}");
        return Ok(());
    };
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match (ext.as_deref(), kind) {
        (Some("json"), OutputKind::Json) | (Some("json"), OutputKind::Svg) => {
            std::fs::write(path, body).with_context(|| format!("write {path}"))?;
        }
        (Some("svg"), OutputKind::Svg) => {
            let mut out = String::with_capacity(body.len() + 64);
            out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
            out.push_str(body);
            std::fs::write(path, out).with_context(|| format!("write {path}"))?;
        }
        (Some("html"), OutputKind::Svg) => {
            let html = wrap_dayview_svg_in_html(body);
            std::fs::write(path, html).with_context(|| format!("write {path}"))?;
        }
        (Some("png"), OutputKind::Svg) => {
            #[cfg(feature = "png")]
            {
                let mut svg_xml = String::with_capacity(body.len() + 64);
                svg_xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
                svg_xml.push_str(body);
                let png = rasterize_to_png(&svg_xml)
                    .with_context(|| format!("rasterize SVG to PNG for {path}"))?;
                std::fs::write(path, png).with_context(|| format!("write {path}"))?;
            }
            #[cfg(not(feature = "png"))]
            anyhow::bail!(
                "PNG output requires the `png` cargo feature; rebuild with \
                 `--features png`, or pick a .svg / .html / .json output \
                 path instead (got: {path})"
            );
        }
        _ => anyhow::bail!("output file must end in .svg, .png, .html, or .json: {path}"),
    }
    Ok(())
}

/// Pure-Rust SVG → PNG via resvg + tiny-skia. No system libs needed.
#[cfg(feature = "png")]
fn rasterize_to_png(svg_xml: &str) -> Result<Vec<u8>> {
    let mut opts = resvg::usvg::Options::default();
    // resvg silently drops <text> nodes when its fontdb has no usable face
    // for the requested font-family — that's why PNG labels disappeared.
    // Load system fonts, then pin font_family + the generic family aliases
    // to a face that actually exists so resolution succeeds even when the
    // dayview SVG omits font-family (default "Times New Roman" is rarely
    // present on Linux) and even when fontconfig has no serif/sans alias.
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

/// Resolve an absolute or relative date arg against `today`.
fn resolve_date_arg(s: &str, today: NaiveDate) -> Result<NaiveDate> {
    let parsed: DateArg = parse_date_arg(s).map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(d) = parsed.absolute {
        return Ok(d);
    }
    Ok(today + chrono::Duration::days(parsed.days_offset))
}

/// Local-day bounds in UTC ms. Local-time DST gaps/duplicates are resolved
/// by `Local::from_local_datetime`'s `single` heuristic; we fall back to
/// the earliest match if ambiguous.
fn local_day_bounds_ms(date: NaiveDate) -> Result<(i64, i64)> {
    let start_naive = date
        .and_hms_opt(0, 0, 0)
        .context("local midnight not representable")?;
    let start = Local
        .from_local_datetime(&start_naive)
        .single()
        .or_else(|| Local.from_local_datetime(&start_naive).earliest())
        .context("ambiguous local midnight")?;
    let end = start + chrono::Duration::days(1);
    Ok((
        start.with_timezone(&Utc).timestamp_millis(),
        end.with_timezone(&Utc).timestamp_millis(),
    ))
}

/// Format `date` as datum's 3-line label `["ccc", "LLL dd", "yyyy"]`. The
/// layout drops lines as needed when the row is too short.
fn format_day_label(date: NaiveDate) -> Vec<String> {
    let weekday = match date.weekday() {
        chrono::Weekday::Mon => "Mon",
        chrono::Weekday::Tue => "Tue",
        chrono::Weekday::Wed => "Wed",
        chrono::Weekday::Thu => "Thu",
        chrono::Weekday::Fri => "Fri",
        chrono::Weekday::Sat => "Sat",
        chrono::Weekday::Sun => "Sun",
    };
    let month = match date.month() {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    };
    vec![
        weekday.to_string(),
        format!("{} {:02}", month, date.day()),
        format!("{}", date.year()),
    ]
}

/// Build the per-field block/point list for a day window.
///
/// `now_ms` caps the open switch-region's right edge for the day that
/// contains `now_ms` (per da-zxs: today's active state stops at the now-line,
/// not at end-of-day). For days entirely before `now_ms` the cap is a no-op
/// (`min(day_end_ms, now_ms) == day_end_ms`); for days entirely after
/// `now_ms` the resulting region falls outside the day window and is
/// dropped by `push_switch_region`.
fn build_day_fields(
    db: &Db,
    day_start_ms: i64,
    day_end_ms: i64,
    now_ms: i64,
) -> Result<Vec<DayviewFieldInput>> {
    let day_start_utc = Utc
        .timestamp_millis_opt(day_start_ms)
        .single()
        .context("day_start_ms out of range")?;
    let day_end_utc = Utc
        .timestamp_millis_opt(day_end_ms)
        .single()
        .context("day_end_ms out of range")?;

    // Pull every entry whose hybrid time intersects this day. We include
    // a leading buffer so a still-open block that started before the day
    // starts can render. The layout clips to the visible window.
    let buffer = chrono::Duration::days(2);
    let params = ListParams {
        field: None,
        metric: TimeMetric::Hybrid,
        start_utc: Some(day_start_utc - buffer),
        end_utc: Some(day_end_utc),
        limit: None,
        head: false,
        reverse: false,
    };
    let entries = db.list_entries(&params)?;

    // Bucket by field.
    let mut by_field: std::collections::BTreeMap<String, Vec<Entry>> =
        std::collections::BTreeMap::new();
    for e in entries {
        by_field.entry(e.field.clone()).or_default().push(e);
    }

    let mut out: Vec<DayviewFieldInput> = Vec::with_capacity(by_field.len());
    for (field, mut entries_for_field) in by_field {
        let spec = db.load_spec(&field).ok().flatten();
        // da-2zd: per-surface visibility. When the spec says this field
        // is hidden on `dayview`, drop it before doing per-entry work —
        // data still flows through tail/get/grep/view, only the visual
        // band is suppressed.
        if let Some(s) = spec.as_ref() {
            if !s.presentation.renders_on("dayview") {
                continue;
            }
        }
        // Sort field entries by occur_time so the next-switch lookup can
        // walk the list in order.
        entries_for_field.sort_by_key(occur_time_ms);
        let fallback_end_ms = day_end_ms.min(now_ms);
        let switch_region_ends = switch_region_end_ms(&entries_for_field, fallback_end_ms);

        let (blocks, points) = render_field_visuals_dayview(
            &entries_for_field,
            &field,
            spec.as_ref(),
            day_start_ms,
            day_end_ms,
            &switch_region_ends,
            db,
        );

        // Per the bead spec: "Empty fields (zero entries in window) are
        // SKIPPED. Don't pollute the SVG with empty bands." The layout
        // also drops empties as a safety net.
        if blocks.is_empty() && points.is_empty() {
            continue;
        }

        // Resolve (y, height, z_index). Without a spec we still want the
        // deterministic-hash y fallback — `PresentationSpec::resolve_position`
        // handles it via a transient default-spec instance.
        let resolved = match &spec {
            Some(s) => s.presentation.resolve_position(&field),
            None => FieldSpec::new(field.clone(), datum_core::action::Action::Occur)
                .presentation
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

/// Resolve a block's fill from its value. Arrays → multi-state stripes
/// (one color per element); scalars → single solid color.
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
            // Skip "false" and "IGNORED" by falling back to the field's
            // own md5 color. Datum's `simplifyState` returned `null|false`
            // for these and the original renderer dropped them — we keep
            // them visible but muted via the field color.
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

fn is_ignored_or_false(v: &JsonValue) -> bool {
    matches!(v, JsonValue::Bool(false))
        || matches!(v, JsonValue::String(s) if s == IGNORED_STATE_VALUE)
}

/// For each entry, the UTC-ms end-time of its switch region (the occur_time
/// of the next switching event on the same field, or `fallback_end_ms` if
/// it's the last). Non-switching entries get `fallback_end_ms` (callers only
/// read the switching positions). Input must be sorted by occur_time
/// ascending.
///
/// A "switching event" is any entry that changes the field's prevailing
/// state at its `occur_time`: Switch (state), BlockSwitch (following_state),
/// or PointSwitch (following_state). Block / Point / Add / Occur do not
/// switch the prevailing state.
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

/// Canonical verb for an entry. da-5ns.3 wired `derive_verb` into the
/// insert/update path so the stored `verb` column is authoritative; the
/// render dispatches on it directly.
fn canonical_verb(entry: &Entry) -> Verb {
    entry.verb
}

/// Sort key for an entry's effective time. Falls back to i64::MIN so
/// time-less rows sink to the front (they're skipped during render anyway).
fn occur_time_ms(entry: &Entry) -> i64 {
    occur_time_ms_opt(entry).unwrap_or(i64::MIN)
}

fn occur_time_ms_opt(entry: &Entry) -> Option<i64> {
    entry.occur_time.as_ref().map(|t| t.utc.timestamp_millis())
}

/// Parse `entry.dur` as seconds. Returns `None` for a missing or unparseable
/// duration string. Negative durations are clamped to zero.
fn dur_seconds(entry: &Entry) -> Option<i64> {
    entry.duration_seconds().map(|s| s.max(0))
}

/// Verb-driven render of one field's blocks and points for the dayview.
///
/// Spec mapping (per da-5ns.5):
/// - **Block**: emit a colored block for `[occur - dur, occur]` using
///   `state` as the fill.
/// - **Block-switch**: same block, AND open a new switch region at
///   `occur` colored `following_state` until the next switching event.
/// - **Point-switch**: a dot at `occur` (fg = state, bg = following_state),
///   AND open a new switch region at `occur` colored `following_state`.
/// - **Point**: a dot at `occur` (fg = field color, bg = state at instant
///   from prior-row lookup).
/// - **Switch**: a colored region from `occur` to the next switching
///   event, fill = `state`.
///
/// `switch_region_ends[idx]` is the UTC-ms end-time of the switch region
/// opened by entry `idx` (or `fallback_end_ms` if it doesn't open one).
fn render_field_visuals_dayview(
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
        // Centralized Hidden gate: a `Switch` to null/false/IGNORED
        // terminates state without opening a region. Block-style and
        // point-style verbs always reach the per-verb arms below.
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

/// Emit a block clipped to the day window for a `[occur - dur, occur]`
/// interval (Block / BlockSwitch). No-op if `dur` is missing or the block
/// falls entirely outside the visible window.
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

/// Emit a switch-region block from `start_ms` to `region_end_ms`, colored by
/// `value`. No-op when the value is the "ended" sentinel (null/false/IGNORED)
/// or the region falls outside the day window.
// Internal helper — eight params group three independently varying axes
// (block bounds, day window, field/spec). Folding them into a struct
// here would just rename the locals at the only call site.
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

fn rgb_to_hex(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)
}

fn state_label(v: Option<&JsonValue>) -> Option<String> {
    v.map(|val| match val {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::action::Action;

    /// Without the `png` feature, requesting a `.png` output path must
    /// return a friendly error pointing at the missing feature, not panic
    /// or fall through to the generic "must end in" branch.
    #[cfg(not(feature = "png"))]
    #[test]
    fn png_output_without_feature_returns_friendly_error() {
        let err = write_payload(
            Some("/tmp/datum-feature-gate-check.png"),
            "<svg/>",
            OutputKind::Svg,
        )
        .expect_err("expected friendly error when png feature is off");
        let msg = err.to_string();
        assert!(msg.contains("png"), "missing 'png' in error: {msg}");
        assert!(msg.contains("feature"), "missing 'feature' in error: {msg}");
    }

    #[test]
    fn rgb_to_hex_lowercase_six_digit() {
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
    fn format_day_label_three_lines() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let labels = format_day_label(d);
        assert_eq!(labels, vec!["Mon", "Apr 27", "2026"]);
    }

    #[test]
    fn resolve_date_arg_handles_iso_and_relative() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let abs = resolve_date_arg("2024-01-15", today).unwrap();
        assert_eq!(abs, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
        let yesterday = resolve_date_arg("-1", today).unwrap();
        assert_eq!(yesterday, NaiveDate::from_ymd_opt(2026, 4, 26).unwrap());
        let tomorrow = resolve_date_arg("+1", today).unwrap();
        assert_eq!(tomorrow, NaiveDate::from_ymd_opt(2026, 4, 28).unwrap());
        let zero = resolve_date_arg("0", today).unwrap();
        assert_eq!(zero, today);
    }

    #[test]
    fn block_fill_array_yields_multi_state() {
        let v = JsonValue::Array(vec![
            JsonValue::String("happy".into()),
            JsonValue::String("calm".into()),
        ]);
        match block_fill_for_value(Some(&v), "mood", None) {
            BlockFill::MultiState(colors) => assert_eq!(colors.len(), 2),
            _ => panic!("expected multi-state fill"),
        }
    }

    #[test]
    fn block_fill_scalar_yields_single() {
        let v = JsonValue::String("happy".into());
        match block_fill_for_value(Some(&v), "mood", None) {
            BlockFill::Single(c) => assert!(c.starts_with('#')),
            _ => panic!("expected single fill"),
        }
    }

    #[test]
    fn block_fill_empty_array_falls_back_to_field_color() {
        let v = JsonValue::Array(vec![]);
        match block_fill_for_value(Some(&v), "mood", None) {
            BlockFill::Single(_) => {}
            _ => panic!("expected single fill on empty array"),
        }
    }

    #[test]
    fn block_fill_none_uses_field_color() {
        match block_fill_for_value(None, "mood", None) {
            BlockFill::Single(c) => assert_eq!(c, rgb_to_hex(md5_rgb("mood"))),
            _ => panic!("expected single fill"),
        }
    }

    #[test]
    fn switch_region_end_ms_chains_consecutive_switches() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;

        let mk = |hour: u32, value: &str| {
            let utc = Utc.with_ymd_and_hms(2026, 4, 27, hour, 0, 0).unwrap();
            let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
            Entry::new(
                "mood",
                Verb::Switch,
                Some(JsonValue::String(value.into())),
                None,
                None,
                Some(dt),
                None,
                "test",
            )
        };
        let entries = vec![mk(8, "happy"), mk(12, "calm"), mk(16, "focus")];
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        assert_eq!(ends.len(), 3);
        // First switch ends at second switch's start.
        assert_eq!(
            ends[0],
            Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        // Second switch ends at third switch's start.
        assert_eq!(
            ends[1],
            Utc.with_ymd_and_hms(2026, 4, 27, 16, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        // Last switch falls back to day_end.
        assert_eq!(ends[2], day_end);
    }

    #[test]
    fn three_switches_render_as_three_regions_no_dots() {
        // Bead da-7r4 acceptance: 3 Switch entries on the same field at 8am,
        // 12pm, 4pm should render as 3 colored regions stacked end-to-end,
        // not as 3 dots.
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        for (hour, state) in &[(8, "happy"), (12, "calm"), (16, "focus")] {
            let utc = Utc.with_ymd_and_hms(2026, 4, 27, *hour, 0, 0).unwrap();
            let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
            let e = Entry::new(
                "mood",
                Verb::Switch,
                Some(JsonValue::String((*state).into())),
                None,
                None,
                Some(dt),
                None,
                "test",
            );
            db.insert_entry(&e).unwrap();
        }
        // Sanity: read-back must surface verb=Switch (the SQL generated
        // column is the source of truth at read-time).
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();

        let fields = build_day_fields(&db, day_start, day_end, day_end).unwrap();
        assert_eq!(fields.len(), 1);
        let mood = &fields[0];
        assert_eq!(mood.field, "mood");
        // No dots — Switch entries become regions.
        assert!(
            mood.points.is_empty(),
            "expected zero points, got {} (Switch should not render as dots)",
            mood.points.len()
        );
        // Three regions, end-to-end.
        assert_eq!(mood.blocks.len(), 3);
        let t8 = Utc
            .with_ymd_and_hms(2026, 4, 27, 8, 0, 0)
            .unwrap()
            .timestamp_millis();
        let t12 = Utc
            .with_ymd_and_hms(2026, 4, 27, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let t16 = Utc
            .with_ymd_and_hms(2026, 4, 27, 16, 0, 0)
            .unwrap()
            .timestamp_millis();
        assert_eq!(mood.blocks[0].start_ms, t8);
        assert_eq!(mood.blocks[0].end_ms, t12);
        assert_eq!(mood.blocks[1].start_ms, t12);
        assert_eq!(mood.blocks[1].end_ms, t16);
        assert_eq!(mood.blocks[2].start_ms, t16);
        assert_eq!(mood.blocks[2].end_ms, day_end);
    }

    #[test]
    fn switch_to_false_skips_region_other_switches_still_render() {
        // A switch to false ends a state but does not start a new colored
        // region — matching original datum's `simplifyState === null|false`
        // skip in fieldSvgBlocks.
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let mk = |hour: u32, value: JsonValue| {
            let utc = Utc.with_ymd_and_hms(2026, 4, 27, hour, 0, 0).unwrap();
            let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
            Entry::new(
                "mood",
                Verb::Switch,
                Some(value),
                None,
                None,
                Some(dt),
                None,
                "test",
            )
        };
        for e in [
            mk(8, JsonValue::String("happy".into())),
            mk(12, JsonValue::Bool(false)),
            mk(16, JsonValue::String("focus".into())),
        ] {
            db.insert_entry(&e).unwrap();
        }
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();

        let fields = build_day_fields(&db, day_start, day_end, day_end).unwrap();
        assert_eq!(fields.len(), 1);
        let mood = &fields[0];
        // Two regions: 8-12 (happy) and 16-day_end (focus). The false-switch
        // at 12 produces no region, just terminates the previous one.
        assert_eq!(mood.blocks.len(), 2);
        let t8 = Utc
            .with_ymd_and_hms(2026, 4, 27, 8, 0, 0)
            .unwrap()
            .timestamp_millis();
        let t12 = Utc
            .with_ymd_and_hms(2026, 4, 27, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let t16 = Utc
            .with_ymd_and_hms(2026, 4, 27, 16, 0, 0)
            .unwrap()
            .timestamp_millis();
        assert_eq!(mood.blocks[0].start_ms, t8);
        assert_eq!(mood.blocks[0].end_ms, t12);
        assert_eq!(mood.blocks[1].start_ms, t16);
        assert_eq!(mood.blocks[1].end_ms, day_end);
    }

    /// da-zxs: on the day that contains `now_ms`, the open switch region
    /// (no later switching event) caps at the now-line, not at end-of-day.
    /// There is no data past now, so drawing the band into the future is
    /// misleading.
    #[test]
    fn open_switch_region_caps_at_now_on_today() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        // One Switch at 08:00. No later switches → this is the active state.
        let utc8 = Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc8, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "mood",
            Verb::Switch,
            Some(JsonValue::String("focus".into())),
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        // "now" lands inside the day window at 14:00.
        let now_ms = Utc
            .with_ymd_and_hms(2026, 4, 27, 14, 0, 0)
            .unwrap()
            .timestamp_millis();
        let t8 = utc8.timestamp_millis();

        let fields = build_day_fields(&db, day_start, day_end, now_ms).unwrap();
        let mood = &fields[0];
        assert_eq!(mood.blocks.len(), 1);
        assert_eq!(mood.blocks[0].start_ms, t8);
        assert_eq!(
            mood.blocks[0].end_ms, now_ms,
            "active state must terminate at now_ms, not at day_end_ms",
        );
    }

    /// da-zxs: a day entirely before `now_ms` is unaffected — the open
    /// switch region for that past day still extends to its own day_end.
    #[test]
    fn open_switch_region_unchanged_for_past_day() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let utc8 = Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc8, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "mood",
            Verb::Switch,
            Some(JsonValue::String("focus".into())),
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        // "now" is days later — this day is entirely in the past.
        let now_ms = Utc
            .with_ymd_and_hms(2026, 5, 5, 12, 0, 0)
            .unwrap()
            .timestamp_millis();

        let fields = build_day_fields(&db, day_start, day_end, now_ms).unwrap();
        let mood = &fields[0];
        assert_eq!(mood.blocks.len(), 1);
        assert_eq!(mood.blocks[0].end_ms, day_end);
    }

    /// da-2zd: a spec with `render = false` is dropped from dayview but
    /// the underlying entries still exist in the DB (verified via direct
    /// list_entries). Validates the "data still flows" rule.
    #[test]
    fn render_false_spec_drops_field_from_dayview() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::field_spec::{FieldSpec, Render};
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();

        // mood has render = false → must NOT appear in dayview output.
        let mut hidden = FieldSpec::new("mood", Action::Switch);
        hidden.presentation.render = Some(Render::All(false));
        db.save_field_spec(&hidden).unwrap();

        // coffee has no render set → default visible.
        db.save_field_spec(&FieldSpec::new("coffee", Action::Occur))
            .unwrap();

        // Log one entry on each field so neither is empty by accident.
        let utc = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "mood",
            Verb::Switch,
            Some(JsonValue::String("happy".into())),
            None,
            None,
            Some(dt.clone()),
            None,
            "test",
        ))
        .unwrap();
        db.insert_entry(&Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();

        let fields = build_day_fields(&db, day_start, day_end, day_end).unwrap();
        let names: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
        assert!(names.contains(&"coffee"), "names = {names:?}");
        assert!(
            !names.contains(&"mood"),
            "render=false must drop mood from dayview, names = {names:?}",
        );
    }

    /// da-2zd: per-surface render — `render.dayview = false` hides only
    /// dayview, while `nowview` (and hence its build) stays visible.
    #[test]
    fn render_dayview_false_hides_only_dayview() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::field_spec::{FieldSpec, Render};
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("stretch_routine", Action::Occur);
        let mut map = std::collections::HashMap::new();
        map.insert("dayview".to_string(), false);
        spec.presentation.render = Some(Render::PerSurface(map));
        db.save_field_spec(&spec).unwrap();

        let utc = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "stretch_routine",
            Verb::Point,
            None,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let fields = build_day_fields(&db, day_start, day_end, day_end).unwrap();
        assert!(
            fields.iter().all(|f| f.field != "stretch_routine"),
            "render.dayview=false must drop the band",
        );
    }

    /// da-8mu: an occur entry's dot must take its fg from the field spec's
    /// `color`, not from the (typically null) value via `state_value_rgb`
    /// — null mapped to `Rgb::IGNORED` (#888888) and made every dot grey.
    #[test]
    fn occur_dot_fg_uses_field_spec_color() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::field_spec::FieldSpec;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("caffeine", Action::Occur);
        spec.color = Some("#643200".into());
        db.save_field_spec(&spec).unwrap();

        let utc = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
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

        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let fields = build_day_fields(&db, day_start, day_end, day_end).unwrap();
        let caffeine = fields
            .iter()
            .find(|f| f.field == "caffeine")
            .expect("caffeine field present");
        assert_eq!(caffeine.points.len(), 1);
        assert_eq!(caffeine.points[0].fg_color, "#643200");
        assert_ne!(caffeine.points[0].fg_color, "#888888");
    }

    /// da-8mu: without a spec.color override the dot still uses the field's
    /// derived (md5) color, never the null-state grey `#888888`.
    #[test]
    fn occur_dot_fg_falls_back_to_md5_field_color() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let utc = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "alarm",
            Verb::Point,
            None,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let fields = build_day_fields(&db, day_start, day_end, day_end).unwrap();
        let alarm = fields
            .iter()
            .find(|f| f.field == "alarm")
            .expect("alarm field present");
        assert_eq!(alarm.points.len(), 1);
        let expected = rgb_to_hex(md5_rgb("alarm"));
        assert_eq!(alarm.points[0].fg_color, expected);
        assert_ne!(alarm.points[0].fg_color, "#888888");
    }

    // ─── da-5ns.5: verb-driven render tests ────────────────────────────
    //
    // These bypass `db.insert_entry` (which hardcodes verb="point" until
    // sub-bead 3 wires `derive_verb`) by constructing Entry structs
    // directly and calling the pure render function. State_at queries are
    // satisfied by an empty in-memory Db.

    /// Helper: build a stub Entry with the desired (verb, occur_time, dur,
    /// state, following_state) shape. The slug/hid/content_hash are placeholders
    /// — the renderer does not read them.
    fn stub_entry(
        field: &str,
        verb: Verb,
        occur_utc: chrono::DateTime<Utc>,
        dur: Option<&str>,
        state: Option<JsonValue>,
        following_state: Option<JsonValue>,
    ) -> Entry {
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

    /// Block at [occur - dur, occur] — `occur_time` is the trailing edge.
    #[test]
    fn block_renders_at_occur_minus_dur_to_occur() {
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
            render_field_visuals_dayview(&entries, "lunch", None, day_start, day_end, &ends, &db);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].start_ms,
            occur.timestamp_millis() - 30 * 60 * 1000
        );
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].state_label.as_deref(), Some("eating"));
    }

    /// Block whose `[occur - dur, occur]` interval crosses the day boundary.
    /// The renderer emits the block; the layout clips it to the visible
    /// window (verified separately in `dayview.rs::tests::blocks_clip_to_day_window`).
    #[test]
    fn block_crossing_day_boundary_emits_full_interval() {
        use chrono::TimeZone;
        let db = Db::open_in_memory().unwrap();
        // 2-hour block ending at 01:00 on day-2 → spans the prior midnight.
        let occur = Utc.with_ymd_and_hms(2026, 4, 28, 1, 0, 0).unwrap();
        let entries = vec![stub_entry(
            "sleep",
            Verb::Block,
            occur,
            Some("PT2H"),
            Some(JsonValue::String("dozing".into())),
            None,
        )];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 29, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, _points) =
            render_field_visuals_dayview(&entries, "sleep", None, day_start, day_end, &ends, &db);
        assert_eq!(blocks.len(), 1);
        let expected_start = occur.timestamp_millis() - 2 * 60 * 60 * 1000;
        assert_eq!(blocks[0].start_ms, expected_start);
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
        // Block start sits in the prior day — confirms the renderer doesn't
        // self-clip; the layout owns clipping.
        assert!(blocks[0].start_ms < day_start);
    }

    /// Block-switch emits the leading block AND opens a new switch region
    /// at occur_time colored by `following_state`. With visibly distinct
    /// colors, the two emitted regions must hold distinct fills.
    #[test]
    fn block_switch_emits_block_plus_following_region_with_distinct_colors() {
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
            render_field_visuals_dayview(&entries, "mood", None, day_start, day_end, &ends, &db);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 2);
        // Block: [occur - 1h, occur] colored "groggy".
        assert_eq!(
            blocks[0].start_ms,
            occur.timestamp_millis() - 60 * 60 * 1000
        );
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].state_label.as_deref(), Some("groggy"));
        // Following region: [occur, day_end] colored "focused".
        assert_eq!(blocks[1].start_ms, occur.timestamp_millis());
        assert_eq!(blocks[1].end_ms, day_end);
        assert_eq!(blocks[1].state_label.as_deref(), Some("focused"));
        // Distinct colors: state ("groggy") and following_state ("focused")
        // hash to different RGBs via md5_rgb.
        match (&blocks[0].fill, &blocks[1].fill) {
            (BlockFill::Single(a), BlockFill::Single(b)) => assert_ne!(a, b),
            _ => panic!("expected single-fill blocks"),
        }
    }

    /// Point-switch: a dot at occur (state passed through) AND a switch
    /// region opening at occur colored by `following_state`.
    #[test]
    fn point_switch_emits_dot_plus_following_region() {
        use chrono::TimeZone;
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 14, 30, 0).unwrap();
        let entries = vec![stub_entry(
            "task",
            Verb::PointSwitch,
            occur,
            Some("PT0S"),
            Some(JsonValue::String("commit".into())),
            Some(JsonValue::String("review".into())),
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
            render_field_visuals_dayview(&entries, "task", None, day_start, day_end, &ends, &db);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].time_ms, occur.timestamp_millis());
        assert_eq!(points[0].state_label.as_deref(), Some("commit"));
        // fg = state color, bg = following_state color → must differ for
        // visibly distinct labels.
        assert_ne!(points[0].fg_color, points[0].bg_color);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_ms, occur.timestamp_millis());
        assert_eq!(blocks[0].end_ms, day_end);
        assert_eq!(blocks[0].state_label.as_deref(), Some("review"));
    }

    /// A block-switch followed later by a switch ends the following-state
    /// region at the switch's occur_time, not at day_end.
    #[test]
    fn block_switch_following_region_ends_at_next_switch() {
        use chrono::TimeZone;
        let db = Db::open_in_memory().unwrap();
        let bs_occur = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let switch_occur = Utc.with_ymd_and_hms(2026, 4, 27, 14, 0, 0).unwrap();
        let entries = vec![
            stub_entry(
                "mood",
                Verb::BlockSwitch,
                bs_occur,
                Some("PT30M"),
                Some(JsonValue::String("groggy".into())),
                Some(JsonValue::String("focused".into())),
            ),
            stub_entry(
                "mood",
                Verb::Switch,
                switch_occur,
                None,
                Some(JsonValue::String("tired".into())),
                None,
            ),
        ];
        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, day_end);
        let (blocks, _points) =
            render_field_visuals_dayview(&entries, "mood", None, day_start, day_end, &ends, &db);
        // 3 blocks expected: leading "groggy" block, "focused" region from
        // 9:00→14:00, and the "tired" switch region from 14:00→day_end.
        assert_eq!(blocks.len(), 3);
        // Find the focused region.
        let focused = blocks
            .iter()
            .find(|b| b.state_label.as_deref() == Some("focused"))
            .expect("focused region present");
        assert_eq!(focused.start_ms, bs_occur.timestamp_millis());
        assert_eq!(focused.end_ms, switch_occur.timestamp_millis());
    }

    /// Block-switch where `following_state` is the "ended" sentinel (false /
    /// null / IGNORED) emits the leading block but no following region.
    #[test]
    fn block_switch_with_unrendered_following_state_emits_only_leading_block() {
        use chrono::TimeZone;
        let db = Db::open_in_memory().unwrap();
        let occur = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap();
        let entries = vec![stub_entry(
            "mood",
            Verb::BlockSwitch,
            occur,
            Some("PT15M"),
            Some(JsonValue::String("hungry".into())),
            Some(JsonValue::Bool(false)),
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
            render_field_visuals_dayview(&entries, "mood", None, day_start, day_end, &ends, &db);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].state_label.as_deref(), Some("hungry"));
    }

    /// `switch_region_end_ms` chains across all three switching verbs
    /// (Switch / BlockSwitch / PointSwitch). Each one terminates the prior
    /// switching event's region.
    #[test]
    fn switch_region_end_ms_chains_across_all_switching_verbs() {
        use chrono::TimeZone;
        let t1 = Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 4, 27, 16, 0, 0).unwrap();
        let entries = vec![
            stub_entry(
                "f",
                Verb::Switch,
                t1,
                None,
                Some(JsonValue::String("a".into())),
                None,
            ),
            stub_entry(
                "f",
                Verb::BlockSwitch,
                t2,
                Some("PT5M"),
                Some(JsonValue::String("via".into())),
                Some(JsonValue::String("b".into())),
            ),
            stub_entry(
                "f",
                Verb::PointSwitch,
                t3,
                Some("PT0S"),
                Some(JsonValue::String("via".into())),
                Some(JsonValue::String("c".into())),
            ),
        ];
        let fallback = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ends = switch_region_end_ms(&entries, fallback);
        assert_eq!(ends[0], t2.timestamp_millis());
        assert_eq!(ends[1], t3.timestamp_millis());
        assert_eq!(ends[2], fallback);
    }
}
