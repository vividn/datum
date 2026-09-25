//! `datum nowview` — render an SVG/PNG/HTML showing each tracked field's
//! state now, and the context before it that `--context` asks for.
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
use chrono::{DateTime, TimeZone, Utc};
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::field_spec::FieldSpec;
use datum_core::shape::{
    dot_fg_uses_state, is_block_verb, is_switching_verb, is_unrendered_switch_value,
    opens_secondary_switch_region, render_shape, RenderShape,
};
use datum_core::time::Zone;
use datum_layout::dayview::{compute_dayview, DayviewBlock, DayviewFieldInput, Orientation};
use datum_layout::nowview::{
    nowview_config, NowviewBlock, NowviewCurrentState, NowviewField, NowviewGeometry, NowviewPoint,
};
use datum_layout::nowview_svg::{
    render_nowview_at_svg, render_nowview_svg, with_xml_declaration, wrap_svg_in_html,
};
use datum_parse::cli::DayviewOrientation;
use datum_parse::time::dur::parse_duration;
use datum_parse::time::DatumTime;
use serde_json::Value as JsonValue;

use crate::args::{NowviewArgs, ShowFlag};
use crate::commands::field_visuals::{
    block_span, canonical_verb, dot_glyph, dur_seconds, is_hole_state, leading_edge_ms,
    occur_time_ms, occur_time_ms_opt, punch_holes, push_switch_region, render_field_visuals,
    rgb_to_hex, state_label, switch_region_end_ms, title_marks,
};
use crate::render::colors::{field_rgb, md5_rgb, state_value_colors, state_value_rgb};
use crate::render::term_layout;

/// The SVG's height when `--height` is not given: the band's cross extent,
/// which is its width when time runs down the page (da-78evn).
const DEFAULT_HEIGHT: f64 = 300.0;

/// Typed entry point — the dispatcher in `cmd.rs` calls this.
pub fn nowview(args: NowviewArgs, db: &Db) -> Result<()> {
    if args.show == ShowFlag::None {
        return Ok(());
    }

    // `--show json` emits the resolved current-state panel as structured
    // data for external clients (e.g. the Pebble `nowview` watchface) and
    // short-circuits both the terminal and SVG dispatch below.
    if args.show == ShowFlag::Json {
        return render_json_once(&args, db);
    }

    // Per da-57dj: no `--output-file` → render a pure visual band stack to
    // the terminal. With a context window we emit the horizontal Flow
    // layout, otherwise the vertical Snapshot of each tracked field's
    // current state. SVG/PNG/HTML remain the file-output formats.
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
/// default, horizontal Flow when a context window was asked for.
///
/// da-7q8r: `--context`/`--history` names that window here too, so the one
/// flag means the same thing on both render paths — a zero (or no flag) is
/// the Snapshot, as it is in the SVG. `--window` is the older spelling and
/// still wins when both are given; unlike `--context 0` it refuses a
/// zero, since asking for a window of no time is a mistake, not a
/// snapshot.
fn render_terminal_once(args: &NowviewArgs, db: &Db) -> Result<()> {
    use datum_layout::nowview_terminal::{render_nowview_terminal, RenderMode, TerminalConfig};

    let (mode, history_minutes) = match args.window.as_deref() {
        Some(s) => {
            let m = duration_minutes(s).with_context(|| format!("invalid --window: '{s}'"))?;
            if m <= 0.0 {
                anyhow::bail!("--window must be positive (got {m} minutes)");
            }
            (RenderMode::Flow, m)
        }
        None => match args.history.as_deref() {
            Some(s) => {
                let m = duration_minutes(s).with_context(|| format!("invalid --context: '{s}'"))?;
                if m > 0.0 {
                    (RenderMode::Flow, m)
                } else {
                    (RenderMode::Snapshot, 0.0)
                }
            }
            None => (RenderMode::Snapshot, 0.0),
        },
    };

    // da-5flt: a time flag centres the Flow window on its instant (the
    // Snapshot reads the states at it); the now-line stays at now.
    let now = DatumTime::now();
    let built = args.time.build(&now)?;
    let now = now.utc;
    let end = match built.time.filter(|_| !built.unmodified) {
        Some(t) => t.utc + chrono::Duration::milliseconds((history_minutes * 30_000.0) as i64),
        None => now,
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
        now_ms: now.timestamp_millis(),
        history_minutes,
        now_width_minutes: 0.0,
        after_minutes: 0.0,
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

/// Resolve the history window, run the dayview layout, and build the
/// current-state panel — the shared compute behind both the SVG renderer
/// ([`render_once`]) and the JSON emitter ([`render_json_once`]).
struct NowComputed {
    now_ms: i64,
    /// The instant the picture is about: the time flags', or now.
    anchor: DateTime<Utc>,
    at: Option<DateTime<Utc>>,
    history_minutes: f64,
    /// The window's two ends, in ms.
    window: (i64, i64),
    layout: datum_layout::dayview::DayviewLayout,
    current_states: Vec<NowviewCurrentState>,
}

fn compute_now(args: &NowviewArgs, db: &Db) -> Result<NowComputed> {
    // da-7q8r: no `--context`/`--history` is the SNAPSHOT — each field's
    // state now, drawn as blocks, no time band. That was the default until
    // da-6jy flipped it to 30m in passing while moving the history portion
    // onto dayview's renderer; the terminal path never followed, so this
    // also puts the two render paths back in agreement.
    let history_minutes = match &args.history {
        Some(s) => duration_minutes(s).with_context(|| format!("invalid --context: '{s}'"))?,
        None => 0.0,
    };
    // da-5flt: the shared time flags name the instant, resolved against
    // the clock at every render so `--watch` and `-q` move with it.
    let now = DatumTime::now();
    let built = args.time.build(&now)?;
    let at = (!built.unmodified)
        .then(|| built.time.map(|t| t.utc))
        .flatten();
    let now = now.utc;
    let anchor = at.unwrap_or(now);
    // Geometry lives in `datum_layout::nowview` so the PWA reaches the same
    // config from its own store (da-6h6p) instead of re-deriving it.
    // da-78evn: `--width`/`--height` are the SVG's either way; the default
    // canvas turns on its side with the picture.
    let orientation = match args.orientation {
        DayviewOrientation::Horizontal => Orientation::Horizontal,
        DayviewOrientation::Vertical => Orientation::Vertical,
    };
    let (width, height) = match orientation {
        Orientation::Horizontal => (args.width, args.height.unwrap_or(DEFAULT_HEIGHT)),
        Orientation::Vertical => (
            Some(args.width.unwrap_or(DEFAULT_HEIGHT)),
            args.height.unwrap_or(0.0),
        ),
    };
    let geometry = NowviewGeometry {
        history_minutes,
        width,
        height,
        margin: args.margin,
        time_axis_height: args.time_axis_height,
        centred: at.is_some(),
        orientation,
    };
    // A centred window reaches half the history past the instant.
    let window_end = if geometry.centred {
        anchor + chrono::Duration::milliseconds((history_minutes * 60_000.0 / 2.0) as i64)
    } else {
        anchor
    };
    // The snapshot's window is an instant; a minute of lookback still
    // gives every field in a state a band for its state to be read from.
    let fields = build_dayview_fields(db, window_end, history_minutes.max(1.0))?;
    let mut config = nowview_config(fields, anchor.timestamp_millis(), &geometry);
    // The yellow line is now, and only now: absent when now is outside.
    config.now_ms = now.timestamp_millis();
    let window = (config.days[0].start_ms, config.days[0].end_ms);

    let layout = compute_dayview(&config);
    let current_states = build_current_states(db, anchor, &layout);

    Ok(NowComputed {
        now_ms: now.timestamp_millis(),
        anchor,
        at,
        history_minutes,
        window,
        layout,
        current_states,
    })
}

/// Render the nowview SVG, to stdout (`-o -`) or `--output-file`.
///
/// Bare `nowview` is the terminal band, and `nowview()` routes it to
/// `render_terminal_once` before this is ever reached — so `None` is not a
/// second spelling of `-o -` here. The SVG on stdout is an explicit ask
/// (datum.day's nowview pane, da-g8o8); a reader who saw `None` beside
/// `Some("-")` could take bare `nowview` for an SVG printer, which the help
/// text says it is not (review thread on #163).
fn render_once(args: &NowviewArgs, db: &Db) -> Result<()> {
    let Some(target) = args.output_file.as_deref() else {
        return render_terminal_once(args, db);
    };
    let c = compute_now(args, db)?;
    let clock = format_local_hh_mm(c.anchor);
    let svg = if c.at.is_some() && c.history_minutes > 0.0 {
        render_nowview_at_svg(&c.layout, &clock, &c.current_states, args.labels)
    } else {
        render_nowview_svg(&c.layout, &clock, &c.current_states, args.labels)
    };

    match target {
        "-" => {
            println!("{svg}");
            Ok(())
        }
        path => write_output(path, &svg),
    }
}

/// Emit the current-state panel as JSON (`--json`). The shape is
/// `{ now_ms, at_ms, start_ms, end_ms, history_minutes, panel: [{ field,
/// state_label, colors }] }` — the window's instant (`at_ms` null when it
/// is now) and ends, and the resolved per-field state and hex color(s) at
/// that instant, without any SVG geometry. Consumed by external clients
/// like the Pebble `nowview` watchface. With `--output-file` the JSON is written there; otherwise to
/// stdout.
fn render_json_once(args: &NowviewArgs, db: &Db) -> Result<()> {
    let c = compute_now(args, db)?;
    let payload = serde_json::json!({
        "now_ms": c.now_ms,
        "at_ms": c.at.map(|t| t.timestamp_millis()),
        "start_ms": c.window.0,
        "end_ms": c.window.1,
        "history_minutes": c.history_minutes,
        "panel": c.current_states,
    });
    let body = serde_json::to_string_pretty(&payload)?;

    match args.output_file.as_deref() {
        None => {
            println!("{body}");
            Ok(())
        }
        Some(path) => std::fs::write(path, body).with_context(|| format!("write {path}")),
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
/// from "now"). 0 minutes is allowed: `--history 0` is the snapshot, and
/// `--window` rejects it itself.
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
    // da-pcis: a window centred on an instant can reach past now, and the
    // record holds nothing there — a state still open runs up to now.
    let open_end_ms = day_end_ms.min(DatumTime::now().utc.timestamp_millis());

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

    // Include any field with no entries in window so the leading-state
    // synthesis below can still give it a band: a field whose open
    // switch state predates the fetch window has no in-window entries at
    // all, but `state_at` still knows its active state (da-ncoa).
    // da-vmjh: `distinct_fields`, not `get_tracked_fields` — only the
    // NAMES are read here, and the tracked read costs a state resolution
    // and a spec load per field to compute an `ignored` flag nobody asks
    // for.
    for field in db.distinct_fields().unwrap_or_default() {
        by_field.entry(field).or_default();
    }

    // da-vmjh: one spec read and one state read for every field, not one
    // per field. `leading_state_region` asks for the state at the SAME
    // instant for all of them, so the batch answers every call.
    let field_names: Vec<&str> = by_field.keys().map(String::as_str).collect();
    let specs = db.batch_load_specs(&field_names)?;
    let positions = super::render_helpers::field_positions(db, &specs, &field_names)?;
    // da-6hg6v: the governing ROW, not just its state — the synthesized
    // leading region is the open block of an entry BEFORE the window, and
    // it names that entry like every other mark.
    let leading_states = db.batch_governing_at(&field_names, fetch_start)?;
    drop(field_names);

    let mut out: Vec<DayviewFieldInput> = Vec::with_capacity(by_field.len());
    for (field, mut entries_for_field) in by_field {
        let spec = specs.get(&field).cloned();
        // da-2zd: per-surface visibility. Hidden-on-nowview specs drop
        // out before per-entry work — data still flows through tail/get/
        // grep/view; only the visual band is suppressed.
        if let Some(s) = spec.as_ref() {
            if !s.presentation.renders_on("nowview") {
                continue;
            }
        }
        entries_for_field.sort_by_key(occur_time_ms);
        let switch_region_ends = switch_region_end_ms(&entries_for_field, open_end_ms);

        // da-ncoa: synthesize the leading region for a state set before
        // the fetch window — no in-window entry opens it, so without this
        // the band falls back to the field-default color.
        let mut blocks: Vec<DayviewBlock> = Vec::new();
        if let Some((start_ms, end_ms, state, id)) = leading_state_region(
            leading_states.get(&field),
            &entries_for_field,
            fetch_start,
            open_end_ms,
        ) {
            push_switch_region(
                &mut blocks,
                Some(&state),
                id.as_deref(),
                start_ms,
                end_ms,
                &field,
                spec.as_ref(),
                day_start_ms,
                day_end_ms,
            );
        }

        let (entry_blocks, mut points) = render_field_visuals(
            &entries_for_field,
            &field,
            spec.as_ref(),
            day_start_ms,
            day_end_ms,
            &switch_region_ends,
        );
        blocks.extend(entry_blocks);
        title_marks(
            &field,
            &mut blocks,
            &mut points,
            (fetch_start.timestamp_millis(), open_end_ms),
        );

        if blocks.is_empty() && points.is_empty() {
            continue;
        }

        let resolved = positions[&field];

        out.push(DayviewFieldInput {
            field,
            y_frac: resolved.y,
            height_frac: resolved.height,
            z_index: resolved.z_index,
            blocks,
            points,
            // The now pane draws a dot's glyph, not its icon.
            icon: None,
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

    // da-vmjh: the names alone, and one spec + state read for all of
    // them — see `build_dayview_fields`.
    let mut all_fields: Vec<String> = by_field.keys().cloned().collect();
    for field in db.distinct_fields()? {
        if !all_fields.iter().any(|f| f == &field) {
            all_fields.push(field);
        }
    }
    let field_names: Vec<&str> = all_fields.iter().map(String::as_str).collect();
    let specs = db.batch_load_specs(&field_names)?;
    let positions = super::render_helpers::field_positions(db, &specs, &field_names)?;
    let leading_states = db.batch_governing_at(&field_names, window_start)?;

    let mut out: Vec<NowviewField> = Vec::with_capacity(all_fields.len());
    for field in &all_fields {
        let field = field.as_str();
        let spec = specs.get(field).cloned();
        if let Some(s) = spec.as_ref() {
            if !s.presentation.renders_on("nowview") {
                continue;
            }
        }
        let mut entries_for_field = by_field.remove(field).unwrap_or_default();
        entries_for_field.sort_by_key(occur_time_ms);
        // da-zwct: `day_end_ms` CLIPS an open region; it does not close
        // one. A block given as a duration, on the other hand, really
        // does end at its own instant — and for a just-logged
        // `start posture 4` that instant IS `day_end_ms`. The band's
        // "what is happening right now" read
        // ([`datum_layout::nowview_terminal`]) has to tell the two
        // apart, so an open region's end lands one millisecond past the
        // window: invisible in the drawing at any width, decisive there.
        let open_end_ms = day_end_ms + 1;
        let switch_region_ends = switch_region_end_ms(&entries_for_field, open_end_ms);

        // da-ncoa: synthesize the leading region for a state set before
        // the fetch window. Without it the renderer paints the field-
        // default color across the band, or — for `datum now`'s
        // include_empty=false path — omits the field entirely while the
        // state list above still shows it.
        let mut blocks: Vec<NowviewBlock> = Vec::new();
        // The terminal band draws no identity, so the governing entry
        // the oracle names goes unused here.
        if let Some((start_ms, end_ms, state, _)) = leading_state_region(
            leading_states.get(field),
            &entries_for_field,
            window_start,
            open_end_ms,
        ) {
            push_switch_region_terminal(
                &mut blocks,
                Some(&state),
                start_ms,
                end_ms,
                field,
                spec.as_ref(),
            );
        }

        let (entry_blocks, points) = render_field_visuals_terminal(
            &entries_for_field,
            field,
            spec.as_ref(),
            &switch_region_ends,
        );
        blocks.extend(entry_blocks);

        if !include_empty && blocks.is_empty() && points.is_empty() {
            continue;
        }

        let resolved = positions[field];

        // Per-field fallback color — single hex unless the field has a
        // saved parallel-state default (rare).
        let default_colors = vec![rgb_to_hex(field_rgb(field, spec.as_ref()))];

        out.push(NowviewField {
            field: field.to_string(),
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
fn build_current_states(
    db: &Db,
    end: DateTime<Utc>,
    layout: &datum_layout::dayview::DayviewLayout,
) -> Vec<NowviewCurrentState> {
    let day = match layout.days.first() {
        Some(d) => d,
        None => return Vec::new(),
    };
    // da-vmjh: one spec read and one state read for the whole panel.
    // Every row asks about the same instant, so the batch answers them
    // all — this used to be two queries per rendered field.
    let names: Vec<&str> = day.fields.iter().map(|f| f.field.as_str()).collect();
    let specs = db.batch_load_specs(&names).unwrap_or_default();
    // da-6hg6v: the governing ROW as well as its state — the chunk is
    // that entry's open block and carries its hid.
    let states = db.batch_governing_at(&names, end).unwrap_or_default();
    let mut out: Vec<NowviewCurrentState> = Vec::with_capacity(day.fields.len());
    for f in &day.fields {
        let spec = specs.get(&f.field).cloned();
        let (state, id) = match states.get(&f.field).cloned() {
            Some((v, id)) => (Some(v), id),
            None => (None, None),
        };
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
        // A field in no state gets no chunk, so it names no entry
        // either — the id follows the label.
        let id = state_label.as_ref().and(id);
        out.push(NowviewCurrentState {
            field: f.field.clone(),
            state_label,
            colors,
            id,
        });
    }
    out
}

/// Leading-state synthesis (da-ncoa): the entry fetch only reaches back to
/// `window_start`, so a state set BEFORE that instant has no in-window
/// switching entry to open its region. The caller passes the state and
/// the entry that set it, as [`Db::batch_governing_at`] resolved them at
/// `window_start` — the same oracle that drives the SVG state panel —
/// and this returns the region the missing entry would have opened:
/// `[window_start, first in-window switching entry or fallback_end_ms]`,
/// named with that entry's hid (da-6hg6v). `entries` must be sorted by
/// occur_time ascending.
///
/// Returns `None` when an in-window switching entry already covers
/// `window_start` or the field has no prior state-bearing entry. A state
/// that doesn't render (null / false / IGNORED) is still returned — the
/// push helpers' existing [`is_unrendered_switch_value`] gate drops it.
fn leading_state_region(
    state_at_window_start: Option<&(JsonValue, Option<String>)>,
    entries: &[Entry],
    window_start: DateTime<Utc>,
    fallback_end_ms: i64,
) -> Option<(i64, i64, JsonValue, Option<String>)> {
    let window_start_ms = window_start.timestamp_millis();
    let lead_end_ms = entries
        .iter()
        .filter(|e| is_switching_verb(canonical_verb(e)))
        .find_map(occur_time_ms_opt)
        .unwrap_or(fallback_end_ms);
    if lead_end_ms <= window_start_ms {
        return None;
    }
    state_at_window_start
        .map(|(state, id)| (window_start_ms, lead_end_ms, state.clone(), id.clone()))
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
) -> (Vec<NowviewBlock>, Vec<NowviewPoint>) {
    let mut blocks: Vec<NowviewBlock> = Vec::new();
    let mut points: Vec<NowviewPoint> = Vec::new();
    // da-6gvw: the spans the field was explicitly OFF for, subtracted at
    // the end — the same holes the dayview walk punches.
    let mut holes: Vec<(i64, i64)> = Vec::new();

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
                if is_hole_state(entry.state.as_ref()) {
                    if let Some(hole) = block_span(entry, occur_ms) {
                        holes.push(hole);
                    }
                } else {
                    push_block_terminal(&mut blocks, entry, occur_ms, field, spec);
                }
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
                points.push(NowviewPoint {
                    time_ms: occur_ms,
                    fg_color: rgb_to_hex(fg),
                    state_label: state_label(entry.state.as_ref()),
                    glyph: dot_glyph(entry, spec),
                });
            }
        }
        if opens_secondary_switch_region(verb) {
            // da-zwct: the row's trailing edge IS `occur_ms` — a block's
            // duration runs up to its stored instant, and a point-switch
            // spans nothing.
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
    (
        punch_holes(
            blocks,
            &holes,
            |b| (b.start_ms, b.end_ms),
            |b, start_ms, end_ms| {
                b.start_ms = start_ms;
                b.end_ms = end_ms;
            },
        ),
        points,
    )
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
    // da-zwct: the block LASTED `dur` up to `occur_ms`.
    let end_ms = occur_ms;
    let start_ms = leading_edge_ms(entry, occur_ms);
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
        end_ms,
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

/// `t` on the reader's clock — the zone `DatumTime::now()` stamps with, which
/// in the browser is the page's `TZ` (`chrono::Local` is UTC there, da-y6qj).
pub(crate) fn format_local_hh_mm(t: DateTime<Utc>) -> String {
    crate::render::action::clock_label(&Zone::host().local_naive(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::field_visuals::stub_entry;
    use datum_core::action::Action;
    use datum_core::entry::Verb;

    /// da-5flt: the time flags resolve at every render, so each `--watch`
    /// redraw (`compute_now` again) centres `-q` on the clock of THAT redraw.
    #[test]
    fn time_flags_resolve_again_at_each_redraw() {
        use datum_parse::time::clock::{install, uninstall, MockClock};
        let clock = std::sync::Arc::new(MockClock::new(DatumTime::now()));
        install(clock.clone());
        let db = Db::open_in_memory().unwrap();
        let mut args = NowviewArgs::default();
        args.time.quick = 1;
        let first = compute_now(&args, &db).unwrap();
        clock.advance(chrono::Duration::minutes(1));
        let second = compute_now(&args, &db).unwrap();
        uninstall();
        let (a, b) = (first.at.unwrap(), second.at.unwrap());
        assert_eq!(b - a, chrono::Duration::minutes(1));
        assert_eq!(second.now_ms - a.timestamp_millis(), 6 * 60_000);
    }

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

    /// da-jy9r: the clock is 24-hour `HH:MM` — two digits, a colon, two
    /// digits — at an hour below 10 as well as above. The readings are pinned,
    /// not `Utc::now()`, so a suite that runs at 02:57 sees what one that runs
    /// at 14:04 sees.
    #[test]
    fn format_local_hh_mm_pads_both_halves() {
        for (reading, want) in [
            ("2026-03-16T02:57:00", "02:57"),
            ("2026-03-16T14:04:00", "14:04"),
        ] {
            let naive = reading.parse::<chrono::NaiveDateTime>().unwrap();
            assert_eq!(crate::render::action::clock_label(&naive), want);
        }
        // And the host-zone wrapper keeps that shape, whatever `TZ` says.
        let s = format_local_hh_mm("2026-03-16T02:57:00Z".parse::<DateTime<Utc>>().unwrap());
        let (hh, mm) = s
            .split_once(':')
            .unwrap_or_else(|| panic!("one colon: {s}"));
        assert_eq!(hh.len(), 2, "hour not two digits in {s}");
        assert_eq!(mm.len(), 2, "minute not two digits in {s}");
        assert!(
            hh.chars().chain(mm.chars()).all(|c| c.is_ascii_digit()),
            "not all digits in {s}"
        );
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

    /// Terminal-shape Block emits one closed segment at [occur − dur,
    /// occur] — da-zwct: the duration precedes the stored instant.
    #[test]
    fn terminal_block_renders_the_duration_before_its_occur_time() {
        use chrono::TimeZone;
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
        let (blocks, points) = render_field_visuals_terminal(&entries, "lunch", None, &ends);
        assert!(points.is_empty());
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].start_ms,
            occur.timestamp_millis() - 30 * 60 * 1000
        );
        assert_eq!(blocks[0].end_ms, occur.timestamp_millis());
    }

    /// da-6gvw: the two surfaces must agree about a `false` block. The
    /// dayview punches a hole in the enclosing region; nowview's terminal
    /// walk is a second dispatch over the same rows and must punch the
    /// same one, or the two pictures of one day disagree.
    #[test]
    fn terminal_false_block_is_the_same_hole_the_dayview_punches() {
        use crate::commands::field_visuals::render_field_visuals;
        use chrono::TimeZone;

        let day_start = Utc
            .with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let day_end = Utc
            .with_ymd_and_hms(2026, 4, 28, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let entries = vec![
            stub_entry(
                "work",
                Verb::Switch,
                Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap(),
                None,
                Some(JsonValue::Bool(true)),
                None,
            ),
            stub_entry(
                "work",
                Verb::Block,
                Utc.with_ymd_and_hms(2026, 4, 27, 10, 0, 0).unwrap(),
                Some("PT5M"),
                Some(JsonValue::Bool(false)),
                None,
            ),
            stub_entry(
                "work",
                Verb::Switch,
                Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap(),
                None,
                Some(JsonValue::Bool(false)),
                None,
            ),
        ];
        let ends = switch_region_end_ms(&entries, day_end);
        let (terminal, points) = render_field_visuals_terminal(&entries, "work", None, &ends);
        assert!(points.is_empty());
        let (dayview, _) = render_field_visuals(&entries, "work", None, day_start, day_end, &ends);

        let terminal_spans: Vec<(i64, i64)> =
            terminal.iter().map(|b| (b.start_ms, b.end_ms)).collect();
        let dayview_spans: Vec<(i64, i64)> =
            dayview.iter().map(|b| (b.start_ms, b.end_ms)).collect();
        assert_eq!(terminal_spans, dayview_spans);

        let at = |h, m| {
            Utc.with_ymd_and_hms(2026, 4, 27, h, m, 0)
                .unwrap()
                .timestamp_millis()
        };
        assert_eq!(
            terminal_spans,
            vec![(at(9, 0), at(9, 55)), (at(10, 0), at(12, 0))]
        );
    }

    /// Terminal-shape PointSwitch emits a dot AND opens a switch region.
    #[test]
    fn terminal_point_switch_emits_dot_and_following_region() {
        use chrono::TimeZone;
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
        let (blocks, points) = render_field_visuals_terminal(&entries, "task", None, &ends);
        assert_eq!(points.len(), 1);
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
        // da-zwct: the switch never ended, so its region runs a
        // millisecond past the window rather than closing on the
        // now-line — that is how the band tells "still happening" from
        // "a block that ended exactly now".
        assert_eq!(block.end_ms, now.timestamp_millis() + 1);
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

    /// da-zwct, the way vividn reported it: `start posture 4` says posture
    /// was happening for the past four minutes, so `datum now`'s band
    /// must paint those four minutes BEHIND the now-line and nothing in
    /// the five-minute now panel to its right. The forward reading put
    /// the whole block there.
    #[test]
    fn a_just_logged_dur_block_paints_behind_the_now_line() {
        use datum_layout::nowview_terminal::{render_nowview_terminal, RenderMode, TerminalConfig};

        let db = Db::open_in_memory().unwrap();
        let now = chrono::Utc::now();
        let occur = datum_core::time::DatumTime::from_utc_and_tz(now, "UTC").unwrap();
        let mut e = datum_core::entry::Entry::new(
            "posture",
            Verb::Block,
            Some(serde_json::json!(true)),
            None,
            Some(occur),
            Some(datum_core::dur::Dur::of(chrono::Duration::minutes(4))),
            "d1",
        );
        e.state = Some(serde_json::json!(true));
        db.insert_entry(&e).unwrap();

        // The same knobs `cmd_now`'s band uses: an hour of history, a
        // five-minute now panel.
        let fields = build_fields(&db, now, 60.0, false).unwrap();
        let band = render_nowview_terminal(
            &fields,
            &TerminalConfig {
                end_ms: now.timestamp_millis(),
                now_ms: now.timestamp_millis(),
                history_minutes: 60.0,
                now_width_minutes: 5.0,
                after_minutes: 0.0,
                cols: 80,
                rows: 0,
                use_color: false,
                current_time_label: "12:00".into(),
                mode: RenderMode::InlineBand,
            },
        );

        let row = band
            .lines()
            .find(|l| l.starts_with("posture"))
            .unwrap_or_else(|| panic!("posture row in:\n{band}"));
        let (history, now_panel) = row
            .split_once('│')
            .unwrap_or_else(|| panic!("band divider in:\n{band}"));

        assert!(
            history.contains('█'),
            "the four minutes it lasted must be painted behind the now-line:\n{band}"
        );
        assert!(
            !now_panel.contains('█'),
            "nothing may be painted after the now-line — the block is over:\n{band}"
        );
        // And it is four minutes, not sixty: the history panel is an hour
        // wide, so all but its last handful of cells stay empty.
        let filled = history.chars().filter(|c| *c == '█').count();
        let cells = history.chars().count();
        assert!(
            filled * 10 < cells,
            "four minutes of a sixty-minute panel, got {filled} of {cells} cells:\n{band}"
        );
    }
}
