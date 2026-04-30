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
use datum_core::entry::{Entry, EntryShape, Verb};
use datum_core::field_spec::FieldSpec;
use datum_layout::dayview::{
    compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput,
    DayviewFieldInput, DayviewPoint, Orientation,
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
        // Datum's TS used a `db.changes` event stream + 5min timeout race;
        // we approximate with a simple poll. DB-change events are a
        // follow-up.
        loop {
            render_once(&args, db)?;
            std::thread::sleep(std::time::Duration::from_secs(300));
        }
    } else {
        render_once(&args, db)
    }
}

pub fn dayview_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("dayview_from_argv: stub — R2 extracts bespoke parser")
}

pub fn dayview_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("dayview_from_string: stub — R2 extracts bespoke parser")
}

fn render_once(args: &DayviewArgs, db: &Db) -> Result<()> {
    // 1. Resolve the visible day window.
    let today = Local::now().date_naive();
    let end_date = match args.end_date.as_deref() {
        Some(s) => resolve_date_arg(s, today)
            .with_context(|| format!("invalid --end-date: '{s}'"))?,
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
                anyhow::bail!(
                    "--start-date {} is after --end-date {}",
                    start, end_date
                );
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

        let fields = build_day_fields(db, day_start_ms, day_end_ms)?;

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
    //    Orientation flips which dim `--height` derives `day_height` from
    //    (vertical: total height; horizontal: total width).
    let orientation = match args.orientation {
        DayviewOrientation::Vertical => Orientation::Vertical,
        DayviewOrientation::Horizontal => Orientation::Horizontal,
    };
    let width = args.width.unwrap_or(DEFAULT_WIDTH);
    let day_height_default = args.day_height.unwrap_or(DEFAULT_DAY_HEIGHT);
    // If the user gave --height (vertical) or --width (horizontal) without
    // an explicit --day-height, derive day_height to fit the day-stack
    // axis. Otherwise use the configured day_height.
    let day_height = match orientation {
        Orientation::Vertical => match (args.height, args.day_height) {
            (Some(total), None) => {
                let usable = total - 2.0 * args.margin - args.time_axis_height
                    - (n_days - 1) as f64 * DEFAULT_INTERDAY_MARGIN;
                if usable <= 0.0 || n_days == 0 {
                    day_height_default
                } else {
                    (usable / n_days as f64).max(10.0)
                }
            }
            _ => day_height_default,
        },
        Orientation::Horizontal => match (args.width, args.day_height) {
            (Some(total), None) => {
                let usable = total - 2.0 * args.margin - args.time_axis_height
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
            let mut svg_xml = String::with_capacity(body.len() + 64);
            svg_xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
            svg_xml.push_str(body);
            let png = rasterize_to_png(&svg_xml)
                .with_context(|| format!("rasterize SVG to PNG for {path}"))?;
            std::fs::write(path, png).with_context(|| format!("write {path}"))?;
        }
        _ => anyhow::bail!(
            "output file must end in .svg, .png, .html, or .json: {path}"
        ),
    }
    Ok(())
}

/// Pure-Rust SVG → PNG via resvg + tiny-skia. No system libs needed.
fn rasterize_to_png(svg_xml: &str) -> Result<Vec<u8>> {
    let opts = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg_xml, &opts)
        .map_err(|e| anyhow::anyhow!("parse SVG: {e}"))?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow::anyhow!("allocate pixmap {}x{}", size.width(), size.height()))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
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
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", _ => "Dec",
    };
    vec![
        weekday.to_string(),
        format!("{} {:02}", month, date.day()),
        format!("{}", date.year()),
    ]
}

/// Build the per-field block/point list for a day window.
fn build_day_fields(
    db: &Db,
    day_start_ms: i64,
    day_end_ms: i64,
) -> Result<Vec<DayviewFieldInput>> {
    let day_start_utc = Utc.timestamp_millis_opt(day_start_ms).single()
        .context("day_start_ms out of range")?;
    let day_end_utc = Utc.timestamp_millis_opt(day_end_ms).single()
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
            if !s.renders_on("dayview") {
                continue;
            }
        }
        let mut blocks: Vec<DayviewBlock> = Vec::new();
        let mut points: Vec<DayviewPoint> = Vec::new();

        // Sort field entries by start_time so Switch entries can derive their
        // region end from the next Switch on the same field.
        entries_for_field.sort_by_key(|e| {
            e.start_time
                .as_ref()
                .map(|t| t.utc.timestamp_millis())
                .unwrap_or(i64::MIN)
        });
        let switch_region_ends =
            switch_region_end_ms(&entries_for_field, day_end_ms);

        for (idx, entry) in entries_for_field.iter().enumerate() {
            let Some(start_dt) = entry.start_time.as_ref() else {
                continue;
            };
            let start_ms = start_dt.utc.timestamp_millis();

            match entry.entry_shape {
                EntryShape::Point if entry.verb == Verb::Switch => {
                    // Switch at time T starts a state region that runs until
                    // the next Switch on the same field, or end of the day
                    // window if none. Switches with null/false/IGNORED state
                    // suppress rendering — they mark "state ended", not a new
                    // colored region.
                    let value = entry.value.as_ref();
                    if is_unrendered_switch_value(value) {
                        continue;
                    }
                    let end_ms = switch_region_ends[idx];
                    if end_ms <= day_start_ms || start_ms >= day_end_ms {
                        continue;
                    }
                    let fill = block_fill_for_value(value, &field, spec.as_ref());
                    blocks.push(DayviewBlock {
                        start_ms,
                        end_ms,
                        fill,
                        state_label: state_label(value),
                    });
                }
                EntryShape::Point => {
                    // Skip if outside the window — also handled by layout
                    // clip, but cheaper to short-circuit here.
                    if start_ms < day_start_ms || start_ms > day_end_ms {
                        continue;
                    }
                    // Occur/note dots belong to a field, not a state — the
                    // value (when present) is a quantity or note text, not
                    // a state name. Color comes from the field spec
                    // (spec.color → md5(field)). Treating null as
                    // `Rgb::IGNORED` here would render every occur dot
                    // grey (da-8mu).
                    let fg = field_rgb(&field, spec.as_ref());
                    let bg = match db.state_at(&field, start_dt.utc).unwrap_or(None) {
                        Some(v) => state_value_rgb(&v, &field, spec.as_ref())
                            .unwrap_or(Rgb::BLACK),
                        None => Rgb::BLACK,
                    };
                    points.push(DayviewPoint {
                        time_ms: start_ms,
                        fg_color: rgb_to_hex(fg),
                        bg_color: rgb_to_hex(bg),
                        state_label: state_label(entry.value.as_ref()),
                    });
                }
                EntryShape::Block => {
                    let Some(end_dt) = entry.end_time.as_ref() else {
                        continue;
                    };
                    let end_ms = end_dt.utc.timestamp_millis();
                    if end_ms <= day_start_ms || start_ms >= day_end_ms {
                        continue;
                    }
                    let fill = block_fill_for_value(
                        entry.value.as_ref(),
                        &field,
                        spec.as_ref(),
                    );
                    blocks.push(DayviewBlock {
                        start_ms,
                        end_ms,
                        fill,
                        state_label: state_label(entry.value.as_ref()),
                    });
                }
                EntryShape::Ongoing => {
                    // Open block. Cap at min(now, day_end) so it doesn't
                    // bleed past the visible domain when the ongoing
                    // entry started yesterday.
                    let cap_ms = day_end_ms.min(Utc::now().timestamp_millis());
                    if cap_ms <= day_start_ms || start_ms >= day_end_ms {
                        continue;
                    }
                    let fill = block_fill_for_value(
                        entry.value.as_ref(),
                        &field,
                        spec.as_ref(),
                    );
                    blocks.push(DayviewBlock {
                        start_ms,
                        end_ms: cap_ms,
                        fill,
                        state_label: state_label(entry.value.as_ref()),
                    });
                }
            }
        }

        // Per the bead spec: "Empty fields (zero entries in window) are
        // SKIPPED. Don't pollute the SVG with empty bands." The layout
        // also drops empties as a safety net.
        if blocks.is_empty() && points.is_empty() {
            continue;
        }

        // Resolve (y, height, z_index). Without a spec we still want the
        // deterministic-hash y fallback — `FieldSpec::resolve_position`
        // handles it via a transient default-spec instance.
        let resolved = match &spec {
            Some(s) => s.resolve_position(&field),
            None => FieldSpec::new(field.clone(), datum_core::entry::Verb::Occur)
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

/// Switch values that should not produce a colored region. Mirrors original
/// datum's `simplifyState === null || false` short-circuit in
/// `fieldSvgBlocks.ts`: a switch to null / false / IGNORED ends a state but
/// does not start a new one.
fn is_unrendered_switch_value(v: Option<&JsonValue>) -> bool {
    match v {
        None | Some(JsonValue::Null) => true,
        Some(v) => is_ignored_or_false(v),
    }
}

/// For each entry, the UTC-ms end-time of its switch region (next Switch on
/// the same field, or `fallback_end_ms` if it's the last). Non-switch entries
/// get `fallback_end_ms` (the value is ignored — we only read it for Switch
/// rows). Input must be sorted by start_time ascending.
fn switch_region_end_ms(entries: &[Entry], fallback_end_ms: i64) -> Vec<i64> {
    let mut out = vec![fallback_end_ms; entries.len()];
    let mut next_start_ms: Option<i64> = None;
    for (i, entry) in entries.iter().enumerate().rev() {
        if entry.verb == Verb::Switch && entry.entry_shape == EntryShape::Point {
            out[i] = next_start_ms.unwrap_or(fallback_end_ms);
            if let Some(t) = entry.start_time.as_ref() {
                next_start_ms = Some(t.utc.timestamp_millis());
            }
        }
    }
    out
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

    #[test]
    fn rgb_to_hex_lowercase_six_digit() {
        assert_eq!(rgb_to_hex(Rgb { r: 0xab, g: 0x12, b: 0xcd }), "#ab12cd");
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
    fn unrendered_switch_value_skips_null_false_ignored() {
        assert!(is_unrendered_switch_value(None));
        assert!(is_unrendered_switch_value(Some(&JsonValue::Null)));
        assert!(is_unrendered_switch_value(Some(&JsonValue::Bool(false))));
        assert!(is_unrendered_switch_value(Some(&JsonValue::String(
            IGNORED_STATE_VALUE.into()
        ))));
        assert!(!is_unrendered_switch_value(Some(&JsonValue::String(
            "happy".into()
        ))));
        assert!(!is_unrendered_switch_value(Some(&JsonValue::Bool(true))));
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

        let fields = build_day_fields(&db, day_start, day_end).unwrap();
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
        let t8 = Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap().timestamp_millis();
        let t12 = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap().timestamp_millis();
        let t16 = Utc.with_ymd_and_hms(2026, 4, 27, 16, 0, 0).unwrap().timestamp_millis();
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
        let day_start = Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0).unwrap().timestamp_millis();
        let day_end = Utc.with_ymd_and_hms(2026, 4, 28, 0, 0, 0).unwrap().timestamp_millis();

        let fields = build_day_fields(&db, day_start, day_end).unwrap();
        assert_eq!(fields.len(), 1);
        let mood = &fields[0];
        // Two regions: 8-12 (happy) and 16-day_end (focus). The false-switch
        // at 12 produces no region, just terminates the previous one.
        assert_eq!(mood.blocks.len(), 2);
        let t8 = Utc.with_ymd_and_hms(2026, 4, 27, 8, 0, 0).unwrap().timestamp_millis();
        let t12 = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap().timestamp_millis();
        let t16 = Utc.with_ymd_and_hms(2026, 4, 27, 16, 0, 0).unwrap().timestamp_millis();
        assert_eq!(mood.blocks[0].start_ms, t8);
        assert_eq!(mood.blocks[0].end_ms, t12);
        assert_eq!(mood.blocks[1].start_ms, t16);
        assert_eq!(mood.blocks[1].end_ms, day_end);
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
        let mut hidden = FieldSpec::new("mood", Verb::Switch);
        hidden.render = Some(Render::All(false));
        db.save_field_spec(&hidden).unwrap();

        // coffee has no render set → default visible.
        db.save_field_spec(&FieldSpec::new("coffee", Verb::Occur))
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
            Verb::Occur,
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

        let fields = build_day_fields(&db, day_start, day_end).unwrap();
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
        let mut spec = FieldSpec::new("stretch_routine", Verb::Occur);
        let mut map = std::collections::HashMap::new();
        map.insert("dayview".to_string(), false);
        spec.render = Some(Render::PerSurface(map));
        db.save_field_spec(&spec).unwrap();

        let utc = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "stretch_routine",
            Verb::Occur,
            None,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let day_start = Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0).unwrap().timestamp_millis();
        let day_end = Utc.with_ymd_and_hms(2026, 4, 28, 0, 0, 0).unwrap().timestamp_millis();
        let fields = build_day_fields(&db, day_start, day_end).unwrap();
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
        let mut spec = FieldSpec::new("caffeine", Verb::Occur);
        spec.color = Some("#643200".into());
        db.save_field_spec(&spec).unwrap();

        let utc = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "caffeine",
            Verb::Occur,
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
        let fields = build_day_fields(&db, day_start, day_end).unwrap();
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
            Verb::Occur,
            None,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();

        let day_start = Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0).unwrap().timestamp_millis();
        let day_end = Utc.with_ymd_and_hms(2026, 4, 28, 0, 0, 0).unwrap().timestamp_millis();
        let fields = build_day_fields(&db, day_start, day_end).unwrap();
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
