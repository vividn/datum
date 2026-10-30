//! `datum dayview` — render a multi-day timeline visualization.
//!
//! Reference: `/tmp/datum-survey/src/dayview/{dayview,singleday,allFieldsSvg,fieldSvgBlocks}.ts`.
//!
//! Layout math + SVG emit live in `datum_layout::dayview` so the SvelteKit
//! PWA can render the same view client-side via WASM. This file is the
//! thin CLI wrapper: it resolves date windows, pulls entries from SQLite,
//! resolves colors through `crate::render::colors`, reads what `check integrity`
//! finds for per-day error overlays, and writes the chosen output format.
//!
//! A day is drawn in local time as it was lived (da-1v9gp): on the wall
//! clock of the zone the record was kept in then (`Db::zone_timeline`),
//! whatever zone the reader is in now. [`day_spans`] cuts each day where
//! that clock jumped, and the layout draws the jump.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use datum_core::action::Action;
use datum_core::db::Db;
use datum_core::dur::Dur;
use datum_core::entry::{Entry, Verb};
use datum_core::field_spec::FieldSpec;
use datum_core::integrity::Finding;
use datum_core::time::{DatumTime, Zone, ZoneTimeline};
use datum_layout::dayview::{
    compute_dayview, day_height_for_extent, DayviewConfig, DayviewDayInput, DayviewError,
    DayviewFieldInput, DayviewLayout, DayviewSpan, GlyphSize, Orientation,
};
use datum_layout::dayview_svg::{
    render_dayview_svg_with, wrap_dayview_svg_in_html, DayviewSvgOpts,
};
use datum_parse::cli::DayviewOrientation;
use datum_parse::time::{parse_date, DateArg, TimeCtx};

use crate::args::{DayviewArgs, ShowFlag};
use crate::commands::field_visuals::{
    leading_edge_ms, name_marks, occur_time_ms, render_field_visuals, switch_regions,
};
use datum_core::state::FieldStateRows;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

/// Datum default per-day band height (matches dayview.ts:50).
const DEFAULT_DAY_HEIGHT: f64 = 100.0;
/// Datum default total SVG width (matches dayview.ts:38).
const DEFAULT_WIDTH: f64 = 2000.0;
/// Datum default interday margin (matches dayview.ts:51).
const DEFAULT_INTERDAY_MARGIN: f64 = 15.0;

pub fn dayview(args: DayviewArgs, db: &Db) -> Result<()> {
    if args.watch {
        // Re-render whenever an external `datum` write bumps the SQLite
        // data_version (≈500ms latency). Time-tick refresh is conditional:
        // only when the visible window actually contains "now" (otherwise
        // the now-line isn't visible and there's nothing to advance), and
        // suppressed when we'd be dumping SVG into a terminal.
        let today = today(db, &DatumTime::now())?;
        let end_date = resolved_end_date(&args, today)?;
        let contains_now = window_contains_today(&args, today, end_date)?;
        let periodic =
            dayview_periodic_refresh(contains_now, args.output_file.is_some(), stdout_is_tty());
        crate::commands::watch::watch_loop(db, periodic, |db| render_once(&args, db))
    } else {
        render_once(&args, db)
    }
}

fn stdout_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// Resolve `--end-date` against `today`, defaulting to `today` when absent.
/// Shared with `render_once` so the watch-mode pre-pass mirrors the actual
/// render's window math.
fn resolved_end_date(args: &DayviewArgs, today: NaiveDate) -> Result<NaiveDate> {
    match args.end_date.as_deref() {
        Some(s) => resolve_date_arg(s, today).with_context(|| format!("invalid --end-date: '{s}'")),
        None => Ok(today),
    }
}

/// True when the visible date window includes `today`. For the default
/// `--days=1` case the window is just `end_date`; with `--start-date`
/// it spans `[start, end]`; with only `--days` it spans `[end - n + 1,
/// end]`. `--end-date` defaults to today, so the common case is `true`.
fn window_contains_today(
    args: &DayviewArgs,
    today: NaiveDate,
    end_date: NaiveDate,
) -> Result<bool> {
    if end_date < today {
        return Ok(false);
    }
    let start_date = if let Some(s) = args.start_date.as_deref() {
        resolve_date_arg(s, today).with_context(|| format!("invalid --start-date: '{s}'"))?
    } else if let Some(n) = args.n_days {
        let span = n.max(1) as i64;
        end_date - chrono::Duration::days(span - 1)
    } else {
        end_date
    };
    Ok(start_date <= today && today <= end_date)
}

/// Pick the periodic-refresh duration for dayview's watch loop. None means
/// "wake only on a real DB change":
/// - past-only window → nothing on the canvas advances with time.
/// - SVG to a TTY → time-tick refreshes would just scroll noise.
fn dayview_periodic_refresh(
    contains_now: bool,
    has_output_file: bool,
    stdout_is_tty: bool,
) -> Option<std::time::Duration> {
    if !contains_now {
        return None;
    }
    if !has_output_file && stdout_is_tty {
        return None;
    }
    Some(std::time::Duration::from_secs(60))
}

/// Today on the clock the record is kept by (da-1v9gp): the zone of its
/// newest entry, and the reader's own (`Zone::host`, never `chrono::Local`:
/// da-y6qj) while it has none. The zone the days below are cut in, so
/// today is the day the now-line is on.
fn today(db: &Db, now: &DatumTime) -> Result<NaiveDate> {
    let zones = db.zone_timeline(Zone::host(), Some(now.utc))?;
    Ok(zones.zone_at(now.utc).local_naive(now.utc).date())
}

fn render_once(args: &DayviewArgs, db: &Db) -> Result<()> {
    let (layout, end_date) = layout_of(args, db, None)?;
    if args.show == ShowFlag::None {
        return Ok(());
    }
    if args.show == ShowFlag::Json {
        let mut json = serde_json::to_value(&layout)?;
        if !args.digests.is_empty() {
            let hash = |svg: String| format!("{:x}", Sha256::digest(svg)).into();
            json["digests"] = by_day(args, db, end_date, &args.digests, hash)?;
        }
        if !args.pictures.is_empty() {
            json["pictures"] = by_day(args, db, end_date, &args.pictures, JsonValue::from)?;
        }
        let json = serde_json::to_string_pretty(&json)?;
        return write_payload(args.output_file.as_deref(), &json, OutputKind::Json);
    }

    write_payload(
        args.output_file.as_deref(),
        &svg_of(args, &layout),
        OutputKind::Svg,
    )
}

/// The picture `args` asks for of `layout`: all of it, or its data layer
/// alone (`--data-layer`, da-0e06p).
fn svg_of(args: &DayviewArgs, layout: &DayviewLayout) -> String {
    let opts = DayviewSvgOpts {
        data_layer: args.data_layer,
        ..Default::default()
    };
    render_dayview_svg_with(layout, opts)
}

/// `--digests` and `--pictures`: each day's picture as `--days 1
/// --end-date` that day would draw it (the stdout less its newline), by
/// days back from `end_date`, as `each` gives it: its SHA-256 hex, so a
/// viewer holding a band knows whether its picture still stands without
/// asking for it (da-efxk5), or the SVG itself, so the bands a viewer
/// lacks come in one run and not one process a day (da-p5lyt).
fn by_day(
    args: &DayviewArgs,
    db: &Db,
    end_date: NaiveDate,
    backs: &[u32],
    each: impl Fn(String) -> JsonValue,
) -> Result<JsonValue> {
    let midnight = |back: u32| {
        (end_date - Duration::days(back.into()))
            .and_time(Default::default())
            .and_utc()
    };
    // What `check integrity` finds, most of a day's cost, read once for
    // days a week apart or less (`layout_of`'s window, each side): a read
    // costs its window, so a day far from the others reads its own.
    let mut sorted = backs.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut read = std::collections::HashMap::new();
    for run in sorted.chunk_by(|a, b| b - a <= 7) {
        let since = midnight(run[run.len() - 1]) - Duration::days(3);
        let until = midnight(run[0]) + Duration::days(3);
        let findings = std::rc::Rc::new(
            datum_core::integrity::findings_between(db.conn(), since, until).unwrap_or_default(),
        );
        read.extend(run.iter().map(|&back| (back, findings.clone())));
    }
    let mut out = serde_json::Map::new();
    for &back in backs {
        let day = DayviewArgs {
            start_date: None,
            end_date: Some((end_date - Duration::days(back.into())).to_string()),
            n_days: Some(1),
            digests: Vec::new(),
            pictures: Vec::new(),
            ..args.clone()
        };
        let svg = svg_of(&day, &layout_of(&day, db, Some(&read[&back]))?.0);
        out.insert(back.to_string(), each(svg));
    }
    Ok(out.into())
}

/// The layout `args` draws, and the last day it draws. `findings`, when
/// given, are what `check integrity` finds around its days, read already.
fn layout_of(
    args: &DayviewArgs,
    db: &Db,
    findings: Option<&[Finding]>,
) -> Result<(DayviewLayout, NaiveDate)> {
    // 1. Resolve the visible day window. One reading of the clock for the
    // days and the now-line both, so they cannot disagree about which day
    // is today, not even across midnight (da-tyggo).
    let now = DatumTime::now();
    let today = today(db, &now)?;
    let end_date = match args.end_date.as_deref() {
        Some(s) => {
            resolve_date_arg(s, today).with_context(|| format!("invalid --end-date: '{s}'"))?
        }
        None => today,
    };

    // Resolve n_days from --start-date / --end-date / --days interaction
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
                        "--days {n} does not match the difference between --start-date \
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

    // 3. For each day, build the field input list.
    let mut days_input: Vec<DayviewDayInput> = Vec::with_capacity(day_dates.len());
    let now_ms = now.utc.timestamp_millis();
    // The zones the days were lived in, read from three days before the
    // first date: a day begins up to 14 hours before its date's midnight in
    // UTC, and the fetch reaches its two-day buffer before that. To three
    // days after the last,
    // the same margin the other way, so a past day does not read on to now
    // (da-e853a).
    let midnight = |d: &NaiveDate| d.and_time(Default::default()).and_utc();
    let since = midnight(&day_dates[0]) - Duration::days(3);
    let until = midnight(&end_date) + Duration::days(3);
    let zones = db.zone_timeline_between(Zone::host(), Some(since), Some(until))?;
    // What `check integrity` finds in the window, bucketed below by the day
    // each flagged entry happened on (da-0lful). Empty result is fine; just
    // no error marks, and none around a sample (da-jc8ic).
    let findings = match (args.context.as_ref(), findings) {
        (Some(_), _) => Vec::new(),
        (None, Some(read)) => read.to_vec(),
        (None, None) => {
            datum_core::integrity::findings_between(db.conn(), since, until).unwrap_or_default()
        }
    };
    let sample = match args.sample.as_deref() {
        Some(json) => {
            let field = serde_json::from_str::<serde_json::Value>(json)
                .ok()
                .and_then(|v| v["field"].as_str().map(str::to_string))
                .context("--sample takes a field's spec as JSON, with its `field`")?;
            Some(super::spec::spec_from_json(&field, json, db)?)
        }
        None => None,
    };
    // With no sample, the context alone: a field not yet named (da-y72v8).
    let context = args.context.as_deref().map(parse_context).transpose()?;
    let window_ms = context.map(Around::window_ms);
    let windowed = window_ms.is_some();
    // One band, the window before now: a day-window flag would be dropped unsaid.
    if windowed
        && (args.start_date.is_some()
            || args.end_date.is_some()
            || args.n_days.is_some_and(|n| n != 1)
            || !args.digests.is_empty()
            || !args.pictures.is_empty())
    {
        anyhow::bail!(
            "--context draws one window before now: \
             no --start-date, --end-date, --digests, --pictures, or --days but 1"
        );
    }
    // Its ticks are measured along a horizontal band.
    if windowed && args.orientation == DayviewOrientation::Vertical {
        anyhow::bail!("--context is drawn horizontally");
    }

    // A sample's context in time (da-jc8ic): one band, the window before
    // now, with no integrity marks.
    if let Some(window_ms) = window_ms {
        let start_ms = now_ms - window_ms;
        let mut fields = build_day_fields(
            db,
            start_ms,
            now_ms,
            now_ms,
            // In place of the field's own entries, but in a window of its own.
            sample
                .as_ref()
                .map(|s| (s, !matches!(context, Some(Around::Window(_))))),
        )?;
        // `none`: the sample's field and nothing else.
        if context == Some(Around::None) {
            fields.retain(|f| sample.as_ref().is_some_and(|s| s.field == f.field));
        }
        if context == Some(Around::Now) {
            // Each other field's state now, not what it did in the last minute.
            let own = sample.as_ref().map(|s| s.field.as_str());
            for f in fields.iter_mut().filter(|f| Some(f.field.as_str()) != own) {
                f.points.clear();
            }
            fields.retain(|f| !f.blocks.is_empty() || !f.points.is_empty());
        }
        days_input.push(DayviewDayInput {
            date: None,
            label_lines: Vec::new(),
            start_ms,
            end_ms: now_ms,
            errors: Vec::new(),
            fields,
            spans: Vec::new(),
        });
    }

    for date in if windowed { &[][..] } else { &day_dates[..] } {
        // The day's window is every instant its date was read on a clock.
        // A date no clock read (Samoa's 2011-12-30) is an empty band.
        let mut spans = day_spans(&zones, *date);
        let day_start_ms = spans.iter().map(|s| s.start_ms).min().unwrap_or_default();
        let day_end_ms = spans.iter().map(|s| s.end_ms).max().unwrap_or_default();
        // A data layer is the day by instant (da-m6i2a): one stretch, so a
        // view crops it across a clock change as if there were none. Its
        // clock is named only when one clock read the whole day, so a
        // viewer drawing a day as it was lived knows to ask for its picture.
        if args.data_layer && day_start_ms < day_end_ms {
            spans = vec![DayviewSpan {
                start_ms: day_start_ms,
                end_ms: day_end_ms,
                lo: 0.0,
                hi: 1.0,
                label: String::new(),
                offset_s: spans
                    .iter()
                    .map(|s| s.offset_s)
                    .reduce(|a, b| a.filter(|_| a == b))
                    .flatten(),
            }];
        }
        let label_lines = format_day_label(*date);

        let errors = (findings.iter())
            .map(|f| (f, f.at.timestamp_millis()))
            .filter(|(_, t)| (day_start_ms..day_end_ms).contains(t))
            .map(|(f, time_ms)| DayviewError {
                time_ms,
                field: f.field.clone(),
                what: f.what.to_string(),
            })
            .collect();

        let fields = build_day_fields(
            db,
            day_start_ms,
            day_end_ms,
            // A data layer's running state runs on to the day's end, for its
            // viewer to cut at its own now as the clock moves (da-0e06p).
            if args.data_layer { day_end_ms } else { now_ms },
            // No `--context` here: a sample only where the field has nothing (da-srrqd).
            sample.as_ref().map(|s| (s, false)),
        )?;

        days_input.push(DayviewDayInput {
            date: Some(date.to_string()),
            label_lines,
            start_ms: day_start_ms,
            end_ms: day_end_ms,
            errors,
            fields,
            spans,
        });
    }

    // 4. Resolve sizing per dayview.ts §44–78. Defaults: width=2000,
    //    day_height=100, time_axis_height=15, interday_margin=15, margin=2.
    //    Orientation flips which dim `--day-height` derives from: in
    //    horizontal mode days stack along y (use --height); in vertical
    //    mode days stack along x (use --width).
    let orientation = match args.orientation {
        DayviewOrientation::Vertical => Orientation::Vertical,
        DayviewOrientation::Horizontal => Orientation::Horizontal,
    };
    // A data layer is drawn at a second a unit (da-m6i2a): its day's
    // instants, and no margin, label or axis around them.
    let layer_day =
        (days_input.first()).map(|d| ((d.end_ms - d.start_ms) as f64 / 1000.0).max(1.0));
    let (width, height) = match (layer_day.filter(|_| args.data_layer), orientation) {
        (Some(seconds), Orientation::Horizontal) => (seconds, args.height),
        (Some(seconds), Orientation::Vertical) => (DEFAULT_WIDTH, Some(seconds)),
        (None, _) => (args.width.unwrap_or(DEFAULT_WIDTH), args.height),
    };
    let (margin, time_axis_height) = match args.data_layer {
        true => (0.0, 0.0),
        false => (args.margin, args.time_axis_height),
    };
    let day_height_default = args.day_height.unwrap_or(DEFAULT_DAY_HEIGHT);
    // If the user gave --height (horizontal mode) or --width (vertical mode)
    // without an explicit --day-height, derive day_height to fit the
    // day-stack axis. Otherwise use the configured day_height.
    let stack_extent = match orientation {
        Orientation::Horizontal => args.height,
        Orientation::Vertical => args.width,
    };
    let day_height = match (stack_extent, args.day_height) {
        (Some(total), None) => day_height_for_extent(
            total,
            margin,
            time_axis_height,
            DEFAULT_INTERDAY_MARGIN,
            n_days as usize,
        )
        .unwrap_or(day_height_default),
        _ => day_height_default,
    };

    // A window's ticks are nowview's, minutes before now, across the whole
    // width. `now`'s are none, and `Some` empty, not `None`: `None` is a
    // day's 24 hourly ticks, which a minute's band must not draw.
    let window_ticks = match context {
        Some(Around::Now) => Some(Vec::new()),
        Some(around) => Some(datum_layout::nowview::nowview_axis_ticks(
            width - 2.0 * margin,
            around.window_ms() as f64 / 60_000.0,
            time_axis_height,
        )),
        None => None,
    };
    let config = DayviewConfig {
        days: days_input,
        now_ms,
        width,
        height,
        day_height,
        interday_margin: DEFAULT_INTERDAY_MARGIN,
        time_axis_height,
        margin,
        // A window has no date to label, and a data layer no room for one.
        label_width: if windowed || args.data_layer {
            Some(0.0)
        } else {
            args.label_width
        },
        orientation,
        axis_ticks_override: window_ticks,
        right_panel_width: 0.0,
        clock_axis_height: 0.0,
    };

    Ok((compute_dayview(&config), end_date))
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
                let png = super::render_helpers::rasterize_to_png(&svg_xml)
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

/// Resolve an absolute or relative date arg against `today`.
fn resolve_date_arg(s: &str, today: NaiveDate) -> Result<NaiveDate> {
    let parsed: DateArg = parse_date(s, TimeCtx::now()).map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(d) = parsed.absolute {
        return Ok(d);
    }
    Ok(today + chrono::Duration::days(parsed.days_offset))
}

/// `date` as it was lived (da-1v9gp): one stretch per UTC offset its wall
/// clock ran at, each the part of that offset's run the clock read `date`
/// in. An ordinary day is one. A day the clock jumped forward in is two
/// with the skipped times between them, and one it jumped back in is two
/// that both hold the repeated times: a 23 or a 25 hour day, each hour
/// drawn at the hour it was.
fn day_spans(zones: &ZoneTimeline, date: NaiveDate) -> Vec<DayviewSpan> {
    let midnight = date.and_time(Default::default());
    let next = midnight + Duration::days(1);
    // No offset is a day from UTC, so every instant of the date is within
    // a day of its own midnights read as UTC.
    let to = next.and_utc() + Duration::days(1);
    let runs = zones.offsets(midnight.and_utc() - Duration::days(1), to);
    let mut spans = Vec::new();
    for (i, (start, offset)) in runs.iter().enumerate() {
        let end = runs.get(i + 1).map_or(to, |run| run.0);
        let lo = (start.naive_utc() + *offset).max(midnight);
        let hi = (end.naive_utc() + *offset).min(next);
        if lo < hi {
            let ms = |wall: NaiveDateTime| (wall - *offset).and_utc().timestamp_millis();
            let along = |wall: NaiveDateTime| (wall - midnight).num_milliseconds() as f64 / 864e5;
            spans.push(DayviewSpan {
                start_ms: ms(lo),
                end_ms: ms(hi),
                lo: along(lo),
                hi: along(hi),
                label: offset_label(*offset),
                offset_s: Some(offset.local_minus_utc()),
            });
        }
    }
    spans
}

/// An offset as a stretch's end is labelled: `UTC+2`, `UTC-3`, `UTC+5:30`.
fn offset_label(offset: FixedOffset) -> String {
    let minutes = offset.local_minus_utc() / 60;
    let sign = if minutes < 0 { '-' } else { '+' };
    match (minutes.abs() / 60, minutes.abs() % 60) {
        (h, 0) => format!("UTC{sign}{h}"),
        (h, m) => format!("UTC{sign}{h}:{m:02}"),
    }
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
///
/// `sample` is `--sample`'s spec, and whether it is drawn in place of what
/// the field has (`--context`, da-jc8ic) or only where it has nothing.
fn build_day_fields(
    db: &Db,
    day_start_ms: i64,
    day_end_ms: i64,
    now_ms: i64,
    sample: Option<(&FieldSpec, bool)>,
) -> Result<Vec<DayviewFieldInput>> {
    let (sample, sample_always) = sample.map_or((None, false), |(s, a)| (Some(s), a));
    let day_start_utc = Utc
        .timestamp_millis_opt(day_start_ms)
        .single()
        .context("day_start_ms out of range")?;
    let day_end_utc = Utc
        .timestamp_millis_opt(day_end_ms)
        .single()
        .context("day_end_ms out of range")?;

    // Pull every entry whose hybrid time intersects this day. We include
    // a leading buffer so a block that started before the day can render,
    // and a trailing one because a block's stored instant is its END
    // (da-zwct): one that began today and ends after midnight is kept by
    // its leading edge (da-tyggo). A state older than the buffer is
    // carried in below. The layout clips to the visible window.
    let buffer = chrono::Duration::days(2);
    let entries = db.list_entries(&super::render_helpers::live_window_params(
        day_start_utc - buffer,
        day_end_utc + buffer,
    ))?;

    // Bucket by field.
    let mut by_field: std::collections::BTreeMap<String, Vec<Entry>> =
        std::collections::BTreeMap::new();
    for e in entries.into_iter().filter(|e| {
        e.occur_time
            .as_ref()
            .is_none_or(|t| leading_edge_ms(e, t.utc.timestamp_millis()) <= day_end_ms)
    }) {
        by_field.entry(e.field.clone()).or_default().push(e);
    }
    // da-w17jo: the buffer bounds what is FETCHED, not which states are
    // drawn. A state switched on before it, however long ago, is still in
    // effect, so every field is asked for its state at the fetch start —
    // the same carry-in nowview makes (da-ncoa) — including a field with
    // no entry in the window at all.
    for field in db.distinct_fields()? {
        by_field.entry(field).or_default();
    }
    if let Some(s) = sample {
        by_field.entry(s.field.clone()).or_default();
    }

    // da-vmjh: one spec query for every field in the day, not one per
    // field.
    let field_names: Vec<&str> = by_field.keys().map(String::as_str).collect();
    let mut specs = db.batch_load_specs(&field_names)?;
    if let Some(s) = sample {
        specs.insert(s.field.clone(), s.clone());
    }
    let positions = super::render_helpers::field_positions(db, &specs, &field_names)?;
    let fetch_start = day_start_utc - buffer;
    drop(field_names);

    let mut out: Vec<DayviewFieldInput> = Vec::with_capacity(by_field.len());
    for (field, mut entries_for_field) in by_field {
        let mut spec = specs.get(&field).cloned();
        if let Some(s) = spec.as_mut() {
            super::spec::drop_unvetted_icon(s);
        }
        // da-2zd: per-surface visibility. When the spec says this field
        // is hidden on `dayview`, drop it before doing per-entry work —
        // data still flows through tail/get/grep/view, only the visual
        // band is suppressed.
        if let Some(s) = spec.as_ref() {
            if !s.presentation.renders_on("dayview") {
                continue;
            }
        }
        entries_for_field.sort_by_key(occur_time_ms);
        let fallback_end_ms = day_end_ms.min(now_ms);
        let window = (fetch_start, day_end_utc + buffer);
        let regions = switch_regions(
            &FieldStateRows::load_between(db.conn(), &field, window.0, window.1)?,
            window,
            fallback_end_ms,
        );
        let (mut blocks, mut points) = render_field_visuals(
            &entries_for_field,
            &regions,
            &field,
            spec.as_ref(),
            day_start_ms,
            day_end_ms,
        );
        name_marks(&field, &mut blocks, &mut points, &entries_for_field);
        // The setup form's preview (da-srrqd): nothing of the field on the
        // day, so entries in its shape that no database holds, drawn as
        // its own would be.
        if let Some(s) = sample.filter(|s| {
            s.field == field && (sample_always || blocks.is_empty() && points.is_empty())
        }) {
            let made = sample_entries(s, day_start_ms, day_end_ms);
            let regions = switch_regions(
                &FieldStateRows::from_entries(&made),
                window,
                fallback_end_ms,
            );
            (blocks, points) = render_field_visuals(
                &made,
                &regions,
                &field,
                spec.as_ref(),
                day_start_ms,
                day_end_ms,
            );
            // Named as its entries would be; but no entry is behind a
            // sample's mark (da-jc8ic), so it carries no `data-id` and no
            // log time: which is how a reader tells it from a real one.
            name_marks(&field, &mut blocks, &mut points, &made);
            blocks
                .iter_mut()
                .for_each(|b| (b.id, b.created_ms) = (None, None));
            points
                .iter_mut()
                .for_each(|p| (p.id, p.created_ms) = (None, None));
        }

        // Per the bead spec: "Empty fields (zero entries in window) are
        // SKIPPED. Don't pollute the SVG with empty bands." The layout
        // also drops empties as a safety net.
        if blocks.is_empty() && points.is_empty() {
            continue;
        }

        // Resolved (y, height, z_index): the hash fallback without a spec,
        // the anchor's place for an anchored one.
        let resolved = positions[&field];

        out.push(DayviewFieldInput {
            field,
            y_frac: resolved.y,
            height_frac: resolved.height,
            z_index: resolved.z_index,
            blocks,
            points,
            glyph_size: glyph_size(spec.as_ref()),
            icon: spec.and_then(|s| s.presentation.icon),
        });
    }
    Ok(out)
}

/// What `--context` draws around a `--sample` (da-jc8ic).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Around {
    /// The sample's field alone, sampled, on the day before now: now at
    /// the band's end, as in the others (da-y72v8).
    None,
    /// Every field's state now across the band, the sample over it.
    Now,
    /// The band as the window of this many ms before now; the field's own
    /// entries in it are drawn, and a sample only when it has none.
    Window(i64),
}

impl Around {
    /// The band's length, ending now. `now` is a minute, nowview's
    /// snapshot's: enough for every field in a state to read it.
    fn window_ms(self) -> i64 {
        match self {
            Around::None => 86_400_000,
            Around::Now => 60_000,
            Around::Window(ms) => ms,
        }
    }
}

fn parse_context(s: &str) -> Result<Around> {
    Ok(match s {
        "none" => Around::None,
        "now" => Around::Now,
        _ => {
            let ms = datum_parse::time::dur::parse_duration(s)
                .ok()
                .map(|d| d.num_milliseconds())
                .filter(|ms| *ms > 0)
                .with_context(|| format!("--context takes none, now or a duration: '{s}'"))?;
            Around::Window(ms)
        }
    })
}

/// What `--sample` draws of a field with nothing on the day: an occurrence's
/// dots spread through it, on/off turning on and off, a switch's states in
/// turn and off among them unless it is never off (with none named, on and
/// off). The changes fall at fixed fractions of the day, so a sample stays
/// put while the form changes how it looks.
fn sample_entries(spec: &FieldSpec, start_ms: i64, end_ms: i64) -> Vec<Entry> {
    // Hours of a 24-hour day, so a sample's changes fall on whole minutes;
    // on a day of another length (DST) each is the same share of it, so a
    // sample is always inside its day.
    const DOTS: [f64; 8] = [1.5, 4.5, 6.0, 10.5, 14.5, 16.5, 20.5, 22.5];
    const TURNS: [f64; 8] = [1.0, 4.0, 6.5, 11.0, 14.0, 17.5, 20.5, 23.0];
    let at = |f: f64| DatumTime {
        utc: Utc
            .timestamp_millis_opt(start_ms + ((end_ms - start_ms) as f64 * f / 24.0) as i64)
            .unwrap(),
        offset_minutes: 0,
        timezone: None,
        uncertainty: None,
    };
    let state = |s: &str| match s {
        "true" => JsonValue::Bool(true),
        "false" => JsonValue::Bool(false),
        _ => JsonValue::String(s.to_string()),
    };
    let named = spec.allowed_states.iter().flatten();
    let mut turns: Vec<JsonValue> = match spec.verb {
        Action::Occur => {
            let dot = |f| {
                Entry::new(
                    &spec.field,
                    Verb::Point,
                    None,
                    None,
                    Some(at(f)),
                    Some(Dur::moment()),
                    "",
                )
            };
            return DOTS.into_iter().map(dot).collect();
        }
        Action::Start => vec![true.into(), false.into()],
        Action::Switch => named.map(|s| state(s)).collect(),
        _ => return Vec::new(),
    };
    if turns.is_empty() {
        turns.push(true.into());
    }
    if !spec.never_false && !turns.contains(&false.into()) {
        turns.push(false.into());
    }
    let change = |(i, f): (usize, f64)| {
        let s = turns[i % turns.len()].clone();
        Entry::new(
            &spec.field,
            Verb::Switch,
            Some(s),
            None,
            Some(at(f)),
            None,
            "",
        )
    };
    // One state alone is one change: a second to it is no change.
    let changes = if turns.len() == 1 { 1 } else { TURNS.len() };
    TURNS
        .into_iter()
        .take(changes)
        .enumerate()
        .map(change)
        .collect()
}

/// A field's dot size as its spec stores it. A stored spec may not have
/// passed `setup` (sync, a hand edit), so a factor that is not above 0
/// draws as unset.
pub(crate) fn glyph_size(spec: Option<&datum_core::field_spec::FieldSpec>) -> GlyphSize {
    let factor = |n: Option<f64>| n.filter(|n| *n > 0.0 && n.is_finite());
    spec.map_or_else(Default::default, |s| GlyphSize {
        scale: factor(s.presentation.symbol_scale).unwrap_or(1.0),
        aspect: factor(s.presentation.symbol_aspect).unwrap_or(1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::field_visuals::rgb_to_hex;
    use crate::render::colors::md5_rgb;
    use datum_core::action::Action;
    use datum_parse::cli::DayviewOrientation;
    use serde_json::Value as JsonValue;

    /// What [`day_spans`] cuts `date` into: each stretch's wall clock in
    /// minutes of the day, its length in real minutes, and its label.
    fn cut(zones: &ZoneTimeline, date: &str) -> Vec<(i64, i64, i64, String)> {
        (day_spans(zones, date.parse().unwrap()).into_iter())
            .map(|s| {
                let minute = |f: f64| (f * 1440.0).round() as i64;
                let real = (s.end_ms - s.start_ms) / 60_000;
                (minute(s.lo), minute(s.hi), real, s.label)
            })
            .collect()
    }

    #[test]
    fn a_day_is_cut_where_its_wall_clock_jumped() {
        let zone = |name: &str| Zone::named(name).unwrap();
        let berlin = ZoneTimeline::single(zone("Europe/Berlin"));
        let h = 60;
        assert_eq!(
            cut(&berlin, "2026-06-01"),
            [(0, 24 * h, 24 * h, "UTC+2".to_string())]
        );
        // 02:00 → 03:00: 23 hours, none of them between the two.
        assert_eq!(
            cut(&berlin, "2026-03-29"),
            [
                (0, 2 * h, 2 * h, "UTC+1".to_string()),
                (3 * h, 24 * h, 21 * h, "UTC+2".to_string())
            ]
        );
        // 03:00 → 02:00: 25 hours, 02:00–03:00 in both.
        assert_eq!(
            cut(&berlin, "2026-10-25"),
            [
                (0, 3 * h, 3 * h, "UTC+2".to_string()),
                (2 * h, 24 * h, 22 * h, "UTC+1".to_string())
            ]
        );
        // A traveller, Moscow to Berlin: the clock goes back two hours at
        // the first entry logged in Berlin, 08:40 there and 10:40 in Moscow.
        let at = |t: &str| t.parse::<chrono::DateTime<Utc>>().unwrap();
        let moved = ZoneTimeline::new(
            [
                (at("2026-01-10T05:00:00Z"), zone("Europe/Moscow")),
                (at("2026-01-11T07:40:00Z"), zone("Europe/Berlin")),
            ],
            zone("Europe/Moscow"),
        );
        assert_eq!(
            cut(&moved, "2026-01-11"),
            [
                (0, 10 * h + 40, 10 * h + 40, "UTC+3".to_string()),
                (8 * h + 40, 24 * h, 15 * h + 20, "UTC+1".to_string())
            ]
        );
        // A date no clock read: Samoa went from 12-29 straight to 12-31.
        let apia = ZoneTimeline::single(zone("Pacific/Apia"));
        assert_eq!(cut(&apia, "2011-12-30"), []);
    }

    #[test]
    fn an_offset_is_labelled_as_hours_from_utc() {
        let label = |minutes: i32| offset_label(FixedOffset::east_opt(minutes * 60).unwrap());
        assert_eq!(
            [label(120), label(0), label(-180), label(330), label(-570)],
            ["UTC+2", "UTC+0", "UTC-3", "UTC+5:30", "UTC-9:30"]
        );
    }

    fn args_with(
        end_date: Option<&str>,
        start_date: Option<&str>,
        n_days: Option<u32>,
    ) -> DayviewArgs {
        DayviewArgs {
            start_date: start_date.map(str::to_string),
            end_date: end_date.map(str::to_string),
            n_days,
            width: None,
            height: None,
            day_height: None,
            time_axis_height: 15.0,
            label_width: None,
            margin: 2.0,
            output_file: None,
            show: ShowFlag::Default,
            watch: true,
            orientation: DayviewOrientation::default(),
            sample: None,
            digests: Vec::new(),
            pictures: Vec::new(),
            context: None,
            data_layer: false,
        }
    }

    #[test]
    fn window_contains_today_default_is_today() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 13).unwrap();
        let args = args_with(None, None, None);
        let end = resolved_end_date(&args, today).unwrap();
        assert_eq!(end, today);
        assert!(window_contains_today(&args, today, end).unwrap());
    }

    #[test]
    fn window_contains_today_past_end_date_is_false() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 13).unwrap();
        let args = args_with(Some("-3"), None, None);
        let end = resolved_end_date(&args, today).unwrap();
        assert!(end < today);
        assert!(!window_contains_today(&args, today, end).unwrap());
    }

    #[test]
    fn window_contains_today_multi_day_window_straddling_today() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 13).unwrap();
        // 7-day window ending today.
        let args = args_with(None, None, Some(7));
        let end = resolved_end_date(&args, today).unwrap();
        assert!(window_contains_today(&args, today, end).unwrap());
    }

    #[test]
    fn window_contains_today_future_window_includes_today() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 13).unwrap();
        // start = today, end = today+5 → today is in [start, end].
        let args = args_with(Some("+5"), Some("0"), None);
        let end = resolved_end_date(&args, today).unwrap();
        assert!(window_contains_today(&args, today, end).unwrap());
    }

    #[test]
    fn periodic_refresh_off_for_past_only_window() {
        // No live now-line to advance → no time tick regardless of output.
        assert!(dayview_periodic_refresh(false, true, false).is_none());
        assert!(dayview_periodic_refresh(false, false, false).is_none());
        assert!(dayview_periodic_refresh(false, false, true).is_none());
    }

    #[test]
    fn periodic_refresh_off_when_dumping_to_terminal() {
        assert!(dayview_periodic_refresh(true, false, true).is_none());
    }

    #[test]
    fn periodic_refresh_on_when_window_live_and_output_is_pollable() {
        // File: viewer reloads on change.
        assert!(dayview_periodic_refresh(true, true, true).is_some());
        // Pipe: downstream tool wants the updates.
        assert!(dayview_periodic_refresh(true, false, false).is_some());
    }

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

        let fields = build_day_fields(&db, day_start, day_end, day_end, None).unwrap();
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

        let fields = build_day_fields(&db, day_start, day_end, day_end, None).unwrap();
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

        let fields = build_day_fields(&db, day_start, day_end, now_ms, None).unwrap();
        let mood = &fields[0];
        assert_eq!(mood.blocks.len(), 1);
        assert_eq!(mood.blocks[0].start_ms, t8);
        assert_eq!(
            mood.blocks[0].end_ms, now_ms,
            "active state must terminate at now_ms, not at day_end_ms",
        );
    }

    /// da-og8a2: a meeting recorded for 15:00–16:00 is a block there on a
    /// day whose now is 14:00, and a state still running stops at now.
    #[test]
    fn recorded_future_block_drawn_running_state_stops_at_now() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let at = |h| Utc.with_ymd_and_hms(2026, 4, 27, h, 0, 0).unwrap();
        for (field, h, state) in [
            ("work", 8, true),
            ("meeting", 15, true),
            ("meeting", 16, false),
        ] {
            let dt = DatumTime::from_utc_and_tz(at(h), "UTC").unwrap();
            let entry = Entry::new(
                field,
                Verb::Switch,
                Some(state.into()),
                None,
                Some(dt),
                None,
                "t",
            );
            db.insert_entry(&entry).unwrap();
        }
        let ms = |h| at(h).timestamp_millis();
        let day_end = Utc.with_ymd_and_hms(2026, 4, 28, 0, 0, 0).unwrap();

        let fields =
            build_day_fields(&db, ms(0), day_end.timestamp_millis(), ms(14), None).unwrap();
        let spans = |f: &str| {
            let field = fields.iter().find(|x| x.field == f).unwrap();
            (field.blocks.iter())
                .map(|b| (b.start_ms, b.end_ms))
                .collect::<Vec<_>>()
        };
        assert_eq!(spans("meeting"), [(ms(15), ms(16))]);
        assert_eq!(spans("work"), [(ms(8), ms(14))]);
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

        let fields = build_day_fields(&db, day_start, day_end, now_ms, None).unwrap();
        let mood = &fields[0];
        assert_eq!(mood.blocks.len(), 1);
        assert_eq!(mood.blocks[0].end_ms, day_end);
    }

    /// da-btyvj: an anchored field sits where its anchor resolves — and
    /// moves with it — even when the anchor has no marks in the day.
    #[test]
    fn anchored_field_follows_its_anchor() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::field_spec::FieldSpec;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let mut mood = FieldSpec::new("mood", Action::Switch);
        mood.presentation.y = Some(0.2);
        mood.presentation.height = Some(0.3);
        db.save_field_spec(&mood).unwrap();
        let mut coffee = FieldSpec::new("coffee", Action::Occur);
        coffee.presentation.relative_to = Some("mood".into());
        coffee.presentation.anchor_pos_ratio = Some(1.0);
        coffee.presentation.anchor_size_ratio = Some(0.5);
        db.save_field_spec(&coffee).unwrap();

        let utc = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();
        let day = |h| {
            Utc.with_ymd_and_hms(2026, 4, 27, h, 0, 0)
                .unwrap()
                .timestamp_millis()
        };
        let lane = |db: &Db| {
            let f = build_day_fields(db, day(0), day(23), day(23), None).unwrap();
            assert_eq!(f.len(), 1, "mood has no marks, only coffee draws");
            (f[0].y_frac, f[0].height_frac)
        };

        assert_eq!(lane(&db), (0.5, 0.15));
        mood.presentation.y = Some(0.4);
        db.save_field_spec(&mood).unwrap();
        let (y, h) = lane(&db);
        assert!((y - 0.7).abs() < 1e-9 && (h - 0.15).abs() < 1e-9, "{y} {h}");
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

        let fields = build_day_fields(&db, day_start, day_end, day_end, None).unwrap();
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
        let mut map = std::collections::BTreeMap::new();
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
        let fields = build_day_fields(&db, day_start, day_end, day_end, None).unwrap();
        assert!(
            fields.iter().all(|f| f.field != "stretch_routine"),
            "render.dayview=false must drop the band",
        );
    }

    /// da-795cy: a block already running at the fetch start was carried in
    /// as a switch region to the end of the day — `coding` painted over a
    /// day it ended two days before. The regions are the oracle's now, and
    /// the oracle hands the field back where the block ends.
    #[test]
    fn a_block_spanning_the_fetch_start_is_not_carried_in_as_a_region() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let at = |d: u32, h: u32, m: u32| {
            let utc = Utc.with_ymd_and_hms(2026, 4, d, h, m, 0).unwrap();
            Some(DatumTime::from_utc_and_tz(utc, "UTC").unwrap())
        };
        let state = |s: &str| Some(serde_json::json!(s));
        for (verb, value, when, dur) in [
            (Verb::Switch, state("idle"), at(24, 12, 0), None),
            // 23:30 on the 24th to 00:30 on the 25th: across the fetch
            // start, two days before the day drawn.
            (Verb::Block, state("coding"), at(25, 0, 30), Some("PT1H")),
        ] {
            let dur = dur.and_then(datum_core::dur::Dur::parse);
            db.insert_entry(&Entry::new("work", verb, value, None, when, dur, "test"))
                .unwrap();
        }
        let day = |d: u32| {
            Utc.with_ymd_and_hms(2026, 4, d, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        };
        let fields = build_day_fields(&db, day(27), day(28), day(28), None).unwrap();
        let work = fields.iter().find(|f| f.field == "work").unwrap();
        let states: Vec<_> = work
            .blocks
            .iter()
            .filter(|b| b.end_ms > day(27))
            .map(|b| b.state_label.as_deref())
            .collect();
        assert_eq!(states, [Some("idle")], "the day is idle, as `now` says");
    }

    /// da-ijjjn, da-sjhuj, da-a87dh: a dot's standard is two thirds of the
    /// lane a field with no height gets.
    #[test]
    fn the_standard_dot_is_two_thirds_of_the_default_lane() {
        let third = datum_core::field_spec::DEFAULT_FIELD_HEIGHT / 3.0;
        assert!((datum_layout::dayview::STANDARD_DOT - 2.0 * third).abs() < 1e-12);
    }

    /// da-c05v1 review: a spec saved without `setup`'s checks (sync, a
    /// hand edit) may carry a factor that is not above 0; it draws as unset
    /// rather than as `r="NaN"`, and a good one still reaches the layout.
    #[test]
    fn a_stored_glyph_factor_not_above_zero_draws_as_unset() {
        use chrono::TimeZone;
        use datum_core::entry::Verb;
        use datum_core::field_spec::FieldSpec;
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let utc = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        let day = |h: i64| {
            Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
                + h * 3_600_000
        };
        let sized = |scale: f64, aspect: f64| {
            let mut spec = FieldSpec::new("coffee", Action::Occur);
            spec.presentation.symbol_scale = Some(scale);
            spec.presentation.symbol_aspect = Some(aspect);
            db.save_field_spec(&spec).unwrap();
            let fields = build_day_fields(&db, day(0), day(24), day(24), None).unwrap();
            fields
                .into_iter()
                .find(|f| f.field == "coffee")
                .unwrap()
                .glyph_size
        };
        db.insert_entry(&Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        for (scale, aspect) in [(-1.0, 0.0), (f64::NAN, f64::INFINITY)] {
            assert_eq!(sized(scale, aspect), GlyphSize::default());
        }
        assert_eq!(
            sized(2.0, 0.5),
            GlyphSize {
                scale: 2.0,
                aspect: 0.5
            }
        );
    }

    /// da-8mu: an occur entry's dot must take its fg from the field spec's
    /// `color`, not from the (typically null) value via `state_value_rgb`
    /// — null mapped to `Rgb::GREY` (#888888) and made every dot grey.
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
        let fields = build_day_fields(&db, day_start, day_end, day_end, None).unwrap();
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
        let fields = build_day_fields(&db, day_start, day_end, day_end, None).unwrap();
        let alarm = fields
            .iter()
            .find(|f| f.field == "alarm")
            .expect("alarm field present");
        assert_eq!(alarm.points.len(), 1);
        let expected = rgb_to_hex(md5_rgb("alarm"));
        assert_eq!(alarm.points[0].fg_color, expected);
        assert_ne!(alarm.points[0].fg_color, "#888888");
    }
}
