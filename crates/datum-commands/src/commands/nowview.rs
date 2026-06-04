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
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::field_spec::FieldSpec;
use datum_core::shape::{
    dot_bg_uses_following_state, dot_fg_uses_state, is_block_verb, is_switching_verb,
    is_unrendered_switch_value, opens_secondary_switch_region, render_shape, RenderShape,
};
use datum_layout::dayview::{
    compute_dayview, DayviewBlock, DayviewConfig, DayviewDayInput, DayviewFieldInput, Orientation,
};
use datum_layout::nowview::{
    default_width_for_history, nowview_axis_ticks, NowviewBlock, NowviewField, NowviewPanelEntry,
    NowviewPoint,
};
use datum_layout::nowview_svg::{render_nowview_svg, with_xml_declaration, wrap_svg_in_html};
use datum_parse::time::parse_duration_str::parse_duration;
use serde_json::Value as JsonValue;

use crate::args::NowviewArgs;
use crate::commands::field_visuals::{
    canonical_verb, dur_seconds, occur_time_ms, occur_time_ms_opt, push_switch_region,
    render_field_visuals, rgb_to_hex, state_label, switch_region_end_ms,
};
use crate::render::colors::{field_rgb, md5_rgb, state_value_colors, state_value_rgb, Rgb};
use crate::render::term_layout;

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
    // Per da-57dj: no `--output-file` → render a pure visual band stack to
    // the terminal. With `--window` we emit the horizontal Flow layout,
    // otherwise the vertical Snapshot of each tracked field's current
    // state. SVG/PNG/HTML remain the file-output formats.
    if args.output_file.is_none() {
        if args.watch {
            let periodic = nowview_terminal_periodic_refresh(stdout_is_tty());
            return crate::commands::watch::watch_loop(db, periodic, || {
                render_terminal_once(&args, db)
            });
        }
        return render_terminal_once(&args, db);
    }

    if args.watch {
        // Re-render whenever an external `datum` write bumps the SQLite
        // data_version (≈500ms latency). nowview always shows "now", so the
        // clock label stays fresh via a 60s periodic refresh — except when
        // we'd be dumping SVG to a terminal, where time-tick refreshes just
        // scroll noise. There, only DB-change wakes the renderer.
        let periodic = nowview_periodic_refresh(args.output_file.is_some(), stdout_is_tty());
        crate::commands::watch::watch_loop(db, periodic, || render_once(&args, db))
    } else {
        render_once(&args, db)
    }
}

/// Periodic-refresh cadence for the terminal nowview: piped → 60s so the
/// clock stays fresh; interactive TTY → DB-change only so we don't flood
/// the screen with redraws.
fn nowview_terminal_periodic_refresh(stdout_is_tty: bool) -> Option<std::time::Duration> {
    if stdout_is_tty {
        None
    } else {
        Some(std::time::Duration::from_secs(60))
    }
}

/// Render the pure-visual terminal nowview once: vertical Snapshot by
/// default, horizontal Flow when `--window <duration>` was passed.
fn render_terminal_once(args: &NowviewArgs, db: &Db) -> Result<()> {
    use datum_layout::nowview_terminal::{render_nowview_terminal, RenderMode, TerminalConfig};

    let end = Utc::now();

    let (mode, history_minutes) = match args.window.as_deref() {
        Some(s) => {
            let m = duration_minutes(s).with_context(|| format!("invalid --window: '{s}'"))?;
            if m <= 0.0 {
                anyhow::bail!("--window must be positive (got {m} minutes)");
            }
            (RenderMode::Flow, m)
        }
        None => (RenderMode::Snapshot, 0.0),
    };

    // Snapshot doesn't sample history, but build_fields still wants a
    // positive window for its lookback buffer — use 1m for Snapshot so the
    // active block (if any) lands inside the lookback.
    let lookback = if history_minutes > 0.0 {
        history_minutes
    } else {
        1.0
    };

    let fields = build_fields(db, end, lookback, /* include_empty = */ true)?;

    // da-tpny: canonical ioctl-first probes and the canonical color gate
    // (DATUM_COLOR → NO_COLOR → TTY). Fallbacks stay nowview's own: 40
    // cols (matches Snapshot's reference width ~80px / 2 cells per
    // pixel) and 24 rows.
    let cols = args
        .cols
        .unwrap_or_else(|| term_layout::detect_cols_or(4, 40).min(u16::MAX as usize) as u16);
    let rows = args
        .rows
        .unwrap_or_else(|| term_layout::detect_rows_or(3, 24).min(u16::MAX as usize) as u16);
    let use_color = !args.no_color && crate::render::colors::color_enabled();

    let cfg = TerminalConfig {
        end_ms: end.timestamp_millis(),
        history_minutes,
        now_width_minutes: 0.0,
        cols,
        rows,
        use_color,
        current_time_label: format_local_hh_mm(end),
        mode,
    };

    let out = render_nowview_terminal(&fields, &cfg);
    println!("{out}");
    Ok(())
}

fn stdout_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// Decide nowview's periodic-refresh: 60s when we're writing somewhere a
/// viewer can poll the file (any `--output-file`, or any non-terminal
/// stdout sink like a pipe). None when we'd be re-emitting SVG to a TTY,
/// where the user only wants updates on real changes.
fn nowview_periodic_refresh(
    has_output_file: bool,
    stdout_is_tty: bool,
) -> Option<std::time::Duration> {
    if has_output_file || !stdout_is_tty {
        Some(std::time::Duration::from_secs(60))
    } else {
        None
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
    // 2-day buffer scaled down for the shorter nowview scope. The shared
    // constructor clamps the upper bound to the caller's exact `end`
    // instant (sub-ms precision) — round-tripping `end` through
    // `day_end_ms` would truncate nanoseconds and exclude entries
    // timestamped at "now".
    let buffer = chrono::Duration::seconds((history_minutes.max(1.0) * 60.0 * 4.0) as i64);
    let fetch_start = day_start_utc - buffer;
    let entries = db.list_entries(&super::render_helpers::live_window_params(fetch_start, end))?;

    let mut by_field: std::collections::BTreeMap<String, Vec<Entry>> =
        std::collections::BTreeMap::new();
    for e in entries {
        by_field.entry(e.field.clone()).or_default().push(e);
    }

    // Include any tracked field with no entries in window so the leading-
    // state synthesis below can still give it a band: a field whose open
    // switch state predates the fetch window has no in-window entries at
    // all, but `state_at` still knows its active state (da-ncoa).
    let tracked = db
        .get_tracked_fields(chrono::Utc::now())
        .unwrap_or_default();
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
            if !s.presentation.renders_on("nowview") {
                continue;
            }
        }
        entries_for_field.sort_by_key(occur_time_ms);
        let switch_region_ends = switch_region_end_ms(&entries_for_field, day_end_ms);

        // da-ncoa: synthesize the leading region for a state set before
        // the fetch window — no in-window entry opens it, so without this
        // the band falls back to the field-default color.
        let mut blocks: Vec<DayviewBlock> = Vec::new();
        if let Some((start_ms, end_ms, state)) =
            leading_state_region(db, &field, &entries_for_field, fetch_start, day_end_ms)?
        {
            push_switch_region(
                &mut blocks,
                Some(&state),
                start_ms,
                end_ms,
                &field,
                spec.as_ref(),
                day_start_ms,
                day_end_ms,
            );
        }

        let (entry_blocks, points) = render_field_visuals(
            &entries_for_field,
            &field,
            spec.as_ref(),
            day_start_ms,
            day_end_ms,
            &switch_region_ends,
            db,
        );
        blocks.extend(entry_blocks);

        if blocks.is_empty() && points.is_empty() {
            continue;
        }

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

/// Build the per-field input list for the *terminal* renderer (used by
/// `datum now`'s nowview band). Returns the simpler `NowviewField` shape
/// since the terminal renderer divides rows evenly and sorts alphabetically.
///
/// Exposed crate-wide so `cmd_now` can reuse the exact same data shape.
///
/// `include_empty` keeps tracked fields with no blocks or points in the
/// result — used by `dm nowview`'s visual band stack so every tracked
/// field gets a band painted in its spec default color (per da-57dj).
/// `datum now` passes `false` to preserve its long-standing behavior of
/// hiding fields with no current data.
pub(crate) fn build_fields(
    db: &Db,
    end: DateTime<Utc>,
    history_minutes: f64,
    include_empty: bool,
) -> Result<Vec<NowviewField>> {
    let day_end_ms = end.timestamp_millis();

    // da-v0uy: same query window as `build_dayview_fields` — in particular
    // the right edge clamps to `end`, so a future-dated entry can't
    // truncate the current region at a timestamp past the now-line (the
    // SVG path and this terminal band must agree on where "now" ends).
    let window_start =
        end - chrono::Duration::seconds((history_minutes.max(1.0) * 60.0 * 4.0) as i64);
    let entries = db.list_entries(&super::render_helpers::live_window_params(
        window_start,
        end,
    ))?;

    let mut by_field: std::collections::BTreeMap<String, Vec<Entry>> =
        std::collections::BTreeMap::new();
    for e in entries {
        by_field.entry(e.field.clone()).or_default().push(e);
    }

    let mut all_fields: Vec<String> = by_field.keys().cloned().collect();
    let tracked = db.get_tracked_fields(chrono::Utc::now())?;
    for tf in &tracked {
        if !all_fields.iter().any(|f| f == &tf.field) {
            all_fields.push(tf.field.clone());
        }
    }

    let mut out: Vec<NowviewField> = Vec::with_capacity(all_fields.len());
    for field in all_fields {
        let spec = db.load_spec(&field).ok().flatten();
        if let Some(s) = spec.as_ref() {
            if !s.presentation.renders_on("nowview") {
                continue;
            }
        }
        let mut entries_for_field = by_field.remove(&field).unwrap_or_default();
        entries_for_field.sort_by_key(occur_time_ms);
        let switch_region_ends = switch_region_end_ms(&entries_for_field, day_end_ms);

        // da-ncoa: synthesize the leading region for a state set before
        // the fetch window. Without it the renderer paints the field-
        // default color across the band, or — for `datum now`'s
        // include_empty=false path — omits the field entirely while the
        // state list above still shows it.
        let mut blocks: Vec<NowviewBlock> = Vec::new();
        if let Some((start_ms, end_ms, state)) =
            leading_state_region(db, &field, &entries_for_field, window_start, day_end_ms)?
        {
            push_switch_region_terminal(
                &mut blocks,
                Some(&state),
                start_ms,
                end_ms,
                &field,
                spec.as_ref(),
            );
        }

        let (entry_blocks, points) = render_field_visuals_terminal(
            &entries_for_field,
            &field,
            spec.as_ref(),
            &switch_region_ends,
            db,
        );
        blocks.extend(entry_blocks);

        if !include_empty && blocks.is_empty() && points.is_empty() {
            continue;
        }

        let resolved = match &spec {
            Some(s) => s.presentation.resolve_position(&field),
            None => FieldSpec::new(field.clone(), datum_core::action::Action::Occur)
                .presentation
                .resolve_position(&field),
        };

        // Per-field fallback color — single hex unless the field has a
        // saved parallel-state default (rare).
        let default_colors = vec![rgb_to_hex(field_rgb(&field, spec.as_ref()))];

        out.push(NowviewField {
            field,
            blocks,
            points,
            y_frac: resolved.y,
            height_frac: resolved.height,
            z_index: resolved.z_index,
            default_colors,
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

/// Leading-state synthesis (da-ncoa): the entry fetch only reaches back to
/// `window_start`, so a state set BEFORE that instant has no in-window
/// switching entry to open its region. Ask [`Db::state_at`] — which knows
/// the true answer and already drives the SVG state panel — for the state
/// active at `window_start`, and return the region the missing entry would
/// have opened: `[window_start, first in-window switching entry or
/// fallback_end_ms]`. `entries` must be sorted by occur_time ascending.
///
/// Returns `Ok(None)` when an in-window switching entry already covers
/// `window_start` or the field has no prior state-bearing entry. A state
/// that doesn't render (null / false / IGNORED) is still returned — the
/// push helpers' existing [`is_unrendered_switch_value`] gate drops it.
fn leading_state_region(
    db: &Db,
    field: &str,
    entries: &[Entry],
    window_start: DateTime<Utc>,
    fallback_end_ms: i64,
) -> Result<Option<(i64, i64, JsonValue)>> {
    let window_start_ms = window_start.timestamp_millis();
    let lead_end_ms = entries
        .iter()
        .filter(|e| is_switching_verb(canonical_verb(e)))
        .find_map(occur_time_ms_opt)
        .unwrap_or(fallback_end_ms);
    if lead_end_ms <= window_start_ms {
        return Ok(None);
    }
    match db.state_at(field, window_start)? {
        Some(state) => Ok(Some((window_start_ms, lead_end_ms, state))),
        None => Ok(None),
    }
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
        // Same dispatch shape as the SVG path. The terminal `Verb::Point` fg
        // differs (md5_rgb fallback when state lookup yields nothing, rather
        // than the field rgb) — preserved via the `else` branch below.
        let shape = render_shape(verb, entry.shape(), &entry.state);
        match shape {
            RenderShape::Hidden | RenderShape::Marker => {}
            RenderShape::Region if is_block_verb(verb) => {
                push_block_terminal(&mut blocks, entry, occur_ms, field, spec);
            }
            RenderShape::Region => {
                push_switch_region_terminal(
                    &mut blocks,
                    entry.state.as_ref(),
                    occur_ms,
                    switch_region_ends[idx],
                    field,
                    spec,
                );
            }
            RenderShape::Dot => {
                let fg = if dot_fg_uses_state(verb) {
                    entry
                        .state
                        .as_ref()
                        .and_then(|v| state_value_rgb(v, field, spec))
                        .unwrap_or_else(|| field_rgb(field, spec))
                } else {
                    state_value_rgb(
                        entry.state.as_ref().unwrap_or(&JsonValue::Null),
                        field,
                        spec,
                    )
                    .unwrap_or_else(|| md5_rgb(field))
                };
                let bg = if dot_bg_uses_following_state(verb) {
                    entry
                        .following_state
                        .as_ref()
                        .and_then(|v| state_value_rgb(v, field, spec))
                        .unwrap_or_else(|| field_rgb(field, spec))
                } else {
                    match db.state_at(field, occur_dt.utc).unwrap_or(None) {
                        Some(v) => state_value_rgb(&v, field, spec).unwrap_or(Rgb::BLACK),
                        None => Rgb::BLACK,
                    }
                };
                points.push(NowviewPoint {
                    time_ms: occur_ms,
                    fg_color: rgb_to_hex(fg),
                    bg_color: rgb_to_hex(bg),
                    state_label: state_label(entry.state.as_ref()),
                });
            }
        }
        if opens_secondary_switch_region(verb) {
            push_switch_region_terminal(
                &mut blocks,
                entry.following_state.as_ref(),
                occur_ms,
                switch_region_ends[idx],
                field,
                spec,
            );
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

pub(crate) fn format_local_hh_mm(t: DateTime<Utc>) -> String {
    let local = t.with_timezone(&Local);
    format!("{}:{:02}", local.hour(), local.minute())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::field_visuals::stub_entry;
    use datum_core::action::Action;
    use datum_core::entry::Verb;

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
    fn periodic_refresh_ticks_when_output_file_set_even_on_tty() {
        // A file viewer is the consumer; the host TTY is incidental.
        assert!(nowview_periodic_refresh(true, true).is_some());
        assert!(nowview_periodic_refresh(true, false).is_some());
    }

    #[test]
    fn periodic_refresh_ticks_when_stdout_is_piped() {
        // Non-TTY stdout = piped to another tool that wants updates.
        assert!(nowview_periodic_refresh(false, false).is_some());
    }

    #[test]
    fn periodic_refresh_disabled_when_dumping_to_terminal() {
        // SVG-to-terminal: time-tick refresh just floods the screen.
        assert!(nowview_periodic_refresh(false, true).is_none());
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
        muted.presentation.render = Some(Render::PerSurface(map));
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
        spec.presentation.y = Some(0.4);
        spec.presentation.height = Some(0.25);
        spec.presentation.z_index = Some(0.7);
        db.save_field_spec(&spec).unwrap();

        let now = Utc::now();
        let dt = DatumTime::from_utc_and_tz(now - chrono::Duration::minutes(15), "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "focus",
            Verb::Switch,
            Some(JsonValue::String("deep".into())),
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
        spec.presentation.render = Some(Render::All(false));
        db.save_field_spec(&spec).unwrap();

        let now = Utc::now();
        let dt = DatumTime::from_utc_and_tz(now, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "background_telemetry",
            Verb::Point,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let fields = build_fields(&db, now, 60.0, false).unwrap();
        assert!(fields.iter().all(|f| f.field != "background_telemetry"));
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

    /// da-ncoa: a switch state set long before the fetch window still
    /// paints the band in its state color — `build_fields` synthesizes a
    /// leading block from `state_at` instead of falling back to the
    /// field-default color (or dropping the field when include_empty=false).
    #[test]
    fn terminal_fields_synthesize_leading_block_for_pre_window_state() {
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        db.set_state_color("mood", "happy", "#112233").unwrap();

        let now = Utc::now();
        let old = DatumTime::from_utc_and_tz(now - chrono::Duration::hours(10), "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "mood",
            Verb::Switch,
            Some(JsonValue::String("happy".into())),
            None,
            Some(old),
            None,
            "test",
        ))
        .unwrap();

        // history 1.0 min → 4-minute fetch window; the entry is far outside.
        let fields = build_fields(&db, now, 1.0, false).unwrap();
        let mood = fields
            .iter()
            .find(|f| f.field == "mood")
            .expect("field with pre-window state must not be omitted");
        assert_eq!(mood.blocks.len(), 1, "blocks = {:?}", mood.blocks);
        let block = &mood.blocks[0];
        let window_start_ms = (now - chrono::Duration::seconds(240)).timestamp_millis();
        assert_eq!(block.start_ms, window_start_ms);
        assert_eq!(block.end_ms, now.timestamp_millis());
        assert_eq!(block.state_label.as_deref(), Some("happy"));
        assert_eq!(block.colors, vec!["#112233".to_string()]);
        assert_ne!(block.colors, mood.default_colors);
    }

    /// da-ncoa: the synthesized leading block ends where the first
    /// in-window switching entry takes over — no overlap, no gap.
    #[test]
    fn leading_block_ends_at_first_in_window_switch() {
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let now = Utc::now();
        let old = DatumTime::from_utc_and_tz(now - chrono::Duration::hours(10), "UTC").unwrap();
        let recent_utc = now - chrono::Duration::minutes(2);
        let recent = DatumTime::from_utc_and_tz(recent_utc, "UTC").unwrap();
        for (t, state) in [(old, "away"), (recent, "back")] {
            db.insert_entry(&Entry::new(
                "presence",
                Verb::Switch,
                Some(JsonValue::String(state.into())),
                None,
                Some(t),
                None,
                "test",
            ))
            .unwrap();
        }

        let fields = build_fields(&db, now, 1.0, false).unwrap();
        let f = fields.iter().find(|f| f.field == "presence").unwrap();
        assert_eq!(f.blocks.len(), 2, "blocks = {:?}", f.blocks);
        assert_eq!(f.blocks[0].state_label.as_deref(), Some("away"));
        assert_eq!(f.blocks[0].end_ms, recent_utc.timestamp_millis());
        assert_eq!(f.blocks[1].state_label.as_deref(), Some("back"));
        assert_eq!(f.blocks[1].start_ms, recent_utc.timestamp_millis());
    }

    /// da-ncoa: same synthesis on the dayview-shape path used by the SVG
    /// render — a pre-window state covers the band from the window's
    /// left edge.
    #[test]
    fn dayview_fields_synthesize_leading_block_for_pre_window_state() {
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let now = Utc::now();
        let old = DatumTime::from_utc_and_tz(now - chrono::Duration::hours(10), "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "mood",
            Verb::Switch,
            Some(JsonValue::String("happy".into())),
            None,
            Some(old),
            None,
            "test",
        ))
        .unwrap();

        // history 60 min → fetch buffer 4h; the entry is at -10h.
        let fields = build_dayview_fields(&db, now, 60.0).unwrap();
        let mood = fields
            .iter()
            .find(|f| f.field == "mood")
            .expect("field with pre-window state must not be omitted");
        assert_eq!(mood.blocks.len(), 1, "blocks = {:?}", mood.blocks);
        let block = &mood.blocks[0];
        let day_start_ms = now.timestamp_millis() - 60 * 60 * 1000;
        assert!(
            block.start_ms <= day_start_ms,
            "leading block must reach the window's left edge: {} > {}",
            block.start_ms,
            day_start_ms,
        );
        assert_eq!(block.end_ms, now.timestamp_millis());
        assert_eq!(block.state_label.as_deref(), Some("happy"));
    }

    /// da-ncoa: an unrendered pre-window state (switch back to `false`)
    /// must NOT synthesize a block — with include_empty=false the field
    /// stays hidden, same as before.
    #[test]
    fn unrendered_pre_window_state_synthesizes_nothing() {
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let now = Utc::now();
        let old = DatumTime::from_utc_and_tz(now - chrono::Duration::hours(10), "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "screen",
            Verb::Switch,
            Some(JsonValue::Bool(false)),
            None,
            Some(old),
            None,
            "test",
        ))
        .unwrap();

        let fields = build_fields(&db, now, 1.0, false).unwrap();
        assert!(
            fields.iter().all(|f| f.field != "screen"),
            "off-state field must stay hidden",
        );
    }
}
