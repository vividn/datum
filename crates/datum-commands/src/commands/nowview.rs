//! `datum nowview` — render an SVG/PNG/HTML showing each tracked field's
//! current state plus an optional history timeline.
//!
//! Reference: `/tmp/datum-survey/src/dayview/nowview.ts` (the TypeScript
//! original). Layout math + SVG emit live in `datum_layout::nowview` so the
//! SvelteKit PWA can render the same view client-side via WASM. This file
//! is the thin CLI wrapper: it pulls entries from SQLite, resolves colors
//! through `crate::render::colors`, and writes the chosen output format.
//!
//! Terminal rendering used to live here too, gated by `--terminal`. Per
//! da-raz that role belongs to `datum now`, which renders a terminal
//! nowview band by default. `nowview` is now SVG-only, paralleling
//! `dayview`.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Timelike, Utc};
use datum_core::db::{Db, ListParams, TimeMetric, IGNORED_STATE_VALUE};
use datum_core::entry::{Entry, EntryShape, Verb};
use datum_layout::nowview::{
    compute_nowview, default_width_for_history, NowviewBlock, NowviewConfig, NowviewField,
    NowviewPoint,
};
use datum_layout::nowview_svg::{render_nowview_svg, with_xml_declaration, wrap_svg_in_html};
use datum_parse::time::parse_duration_str::parse_duration;
use serde_json::Value as JsonValue;

use crate::args::NowviewArgs;
use crate::render::colors::{md5_rgb, state_value_rgb, Rgb};

/// Typed entry point — the dispatcher in `cmd.rs` calls this.
pub fn nowview(args: NowviewArgs, db: &Db) -> Result<()> {
    if args.watch {
        // Simple poll loop: redraw, sleep 60s, redraw. rusqlite update
        // hooks are a follow-up (per the bead).
        loop {
            render_once(&args, db)?;
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    } else {
        render_once(&args, db)
    }
}

pub fn nowview_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("nowview_from_argv: stub — R2 extracts bespoke parser")
}

pub fn nowview_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("nowview_from_string: stub — R2 extracts bespoke parser")
}

fn render_once(args: &NowviewArgs, db: &Db) -> Result<()> {
    // Resolve history / now-width durations in minutes. Invalid input is
    // a hard error in our CLI (datum's TS warned and continued; we prefer
    // a clear failure).
    let history_minutes = match &args.history {
        Some(s) => duration_minutes(s)
            .with_context(|| format!("invalid --history: '{s}'"))?,
        None => 0.0,
    };
    let now_width_minutes = match &args.now_width {
        Some(s) => duration_minutes(s)
            .with_context(|| format!("invalid --now-width: '{s}'"))?,
        None => 5.0,
    };

    let width = args.width.unwrap_or_else(|| default_width_for_history(history_minutes));
    let end = Utc::now();
    let end_ms = end.timestamp_millis();

    let fields = build_fields(db, end, history_minutes)?;

    let config = NowviewConfig {
        end_ms,
        history_minutes,
        now_width_minutes,
        width,
        height: args.height,
        margin: args.margin,
        time_axis_height: args.time_axis_height,
    };
    let mut layout = compute_nowview(&fields, &config);
    layout.current_time_label = format_local_hh_mm(end);

    let svg = render_nowview_svg(&layout);

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
            let body = with_xml_declaration(svg);
            let png = rasterize_to_png(&body)
                .with_context(|| format!("rasterize SVG to PNG for {path}"))?;
            std::fs::write(path, png).with_context(|| format!("write {path}"))?;
        }
        _ => anyhow::bail!(
            "output file must end in .svg, .png, or .html: {path}"
        ),
    }
    Ok(())
}

/// Pure-Rust SVG → PNG via resvg + tiny-skia. Both are pure-Rust crates;
/// no system libs required (per the bead's local-first / pure-Rust rule).
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

/// Convert the user-supplied duration string to absolute minutes. Negative
/// inputs are normalized to positive (the panel always extends backward
/// from "now"). 0 minutes is allowed.
fn duration_minutes(s: &str) -> Result<f64> {
    let dur = parse_duration(s).map_err(|e| anyhow::anyhow!("{e}"))?;
    let secs = dur.num_seconds().abs() as f64;
    Ok(secs / 60.0)
}

/// Build the per-field input list for the layout: blocks (closed/ongoing)
/// and points, with state colors pre-resolved.
///
/// Exposed crate-wide so `datum now` can reuse the exact same data shape
/// for its terminal nowview band (per da-raz: `now` renders the band by
/// default; `nowview` is SVG-only).
pub(crate) fn build_fields(
    db: &Db,
    end: DateTime<Utc>,
    history_minutes: f64,
) -> Result<Vec<NowviewField>> {
    // We pull a generous window so a still-open block that started before
    // the window can still be picked up, then the layout clips. We use
    // `Hybrid` time metric (matches dayview).
    let window_start = end
        - chrono::Duration::seconds((history_minutes.max(1.0) * 60.0 * 4.0) as i64);

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

    // Group by field.
    let mut by_field: std::collections::BTreeMap<String, Vec<Entry>> =
        std::collections::BTreeMap::new();
    for e in entries {
        by_field.entry(e.field.clone()).or_default().push(e);
    }

    // For each field, also pick up the latest pre-window entry so an
    // ongoing block that started long ago still renders. We do this by
    // also peeking at `state_at(field, window_start)` and synthesizing
    // a leading block when a pre-existing ongoing-state covers the
    // window.
    let mut all_fields: Vec<String> = by_field.keys().cloned().collect();
    // Include any field that's tracked but had no entries in window.
    let tracked = db.get_tracked_fields()?;
    for tf in &tracked {
        if !all_fields.iter().any(|f| f == &tf.field) {
            all_fields.push(tf.field.clone());
        }
    }

    let mut out: Vec<NowviewField> = Vec::with_capacity(all_fields.len());
    for field in all_fields {
        let spec = db.load_spec(&field).ok().flatten();
        // da-2zd: per-surface visibility. Hidden-on-nowview specs drop
        // out before any per-entry work — data still flows through other
        // surfaces (tail/get/grep/view).
        if let Some(s) = spec.as_ref() {
            if !s.renders_on("nowview") {
                continue;
            }
        }
        let mut entries_for_field = by_field.remove(&field).unwrap_or_default();

        let mut blocks: Vec<NowviewBlock> = Vec::new();
        let mut points: Vec<NowviewPoint> = Vec::new();

        // Sort by start_time so Switch entries can derive their region end
        // from the next Switch on the same field.
        entries_for_field.sort_by_key(|e| {
            e.start_time
                .as_ref()
                .map(|t| t.utc.timestamp_millis())
                .unwrap_or(i64::MIN)
        });
        let now_ms = end.timestamp_millis();
        let switch_region_ends = switch_region_end_ms(&entries_for_field, now_ms);

        for (idx, entry) in entries_for_field.iter().enumerate() {
            // start_time present is required for visualization. Skip any
            // record (e.g. pure `add` notes) without one.
            let Some(start_dt) = entry.start_time.as_ref() else {
                continue;
            };
            let start_ms = start_dt.utc.timestamp_millis();

            match entry.entry_shape {
                EntryShape::Point if entry.verb == Verb::Switch => {
                    // Switch starts a state region that runs until the next
                    // Switch on the same field, or until "now" if it's the
                    // most recent.
                    let value = entry.value.as_ref();
                    if is_unrendered_switch_value(value) {
                        continue;
                    }
                    let region_end_ms = switch_region_ends[idx];
                    let color = match value {
                        Some(v) => state_value_rgb(v, &field, spec.as_ref())
                            .unwrap_or(md5_rgb(&field)),
                        None => md5_rgb(&field),
                    };
                    blocks.push(NowviewBlock {
                        start_ms,
                        end_ms: region_end_ms,
                        color: rgb_to_hex(color),
                        state_label: state_label(value),
                    });
                }
                EntryShape::Point => {
                    let fg = state_value_rgb(
                        entry.value.as_ref().unwrap_or(&JsonValue::Null),
                        &field,
                        spec.as_ref(),
                    )
                    .unwrap_or(md5_rgb(&field));
                    // Background ring color: state at this instant
                    // (per da-k5z fg/bg rule). Falls back to black when
                    // no underlying ongoing state is tracked.
                    let bg = match db.state_at(&field, start_dt.utc).unwrap_or(None) {
                        Some(v) => state_value_rgb(&v, &field, spec.as_ref())
                            .unwrap_or(Rgb::BLACK),
                        None => Rgb::BLACK,
                    };
                    points.push(NowviewPoint {
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
                    let color = match entry.value.as_ref() {
                        Some(v) => {
                            state_value_rgb(v, &field, spec.as_ref()).unwrap_or(md5_rgb(&field))
                        }
                        None => md5_rgb(&field),
                    };
                    blocks.push(NowviewBlock {
                        start_ms,
                        end_ms,
                        color: rgb_to_hex(color),
                        state_label: state_label(entry.value.as_ref()),
                    });
                }
                EntryShape::Ongoing => {
                    let color = match entry.value.as_ref() {
                        Some(v) => {
                            state_value_rgb(v, &field, spec.as_ref()).unwrap_or(md5_rgb(&field))
                        }
                        None => md5_rgb(&field),
                    };
                    blocks.push(NowviewBlock {
                        start_ms,
                        end_ms: end.timestamp_millis(),
                        color: rgb_to_hex(color),
                        state_label: state_label(entry.value.as_ref()),
                    });
                }
            }
        }

        // Skip fields that have no visualizable data at all in the window.
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

fn rgb_to_hex(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)
}

/// Switch values that should not produce a colored region. Mirrors original
/// datum's `simplifyState === null || false` short-circuit: a switch to
/// null/false/IGNORED ends a state but does not start a new colored region.
fn is_unrendered_switch_value(v: Option<&JsonValue>) -> bool {
    match v {
        None | Some(JsonValue::Null) => true,
        Some(JsonValue::Bool(false)) => true,
        Some(JsonValue::String(s)) if s == IGNORED_STATE_VALUE => true,
        _ => false,
    }
}

/// For each entry, the UTC-ms end-time of its switch region (next Switch on
/// the same field, or `fallback_end_ms` if none). Non-Switch entries get
/// `fallback_end_ms` (caller only reads the Switch positions). Input must be
/// sorted by start_time ascending.
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
        assert_eq!(rgb_to_hex(Rgb { r: 0xab, g: 0x12, b: 0xcd }), "#ab12cd");
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
        // We can't assert wall-clock values, but we can verify shape.
        let t = chrono::Utc::now();
        let s = format_local_hh_mm(t);
        let parts: Vec<&str> = s.split(':').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].len(), 2);
    }

    /// da-2zd: render visibility for nowview. `render.nowview = false`
    /// drops the field from nowview; the same spec rendering on dayview
    /// stays unaffected (verified by the dayview test in dayview.rs).
    #[test]
    fn render_nowview_false_drops_field_from_nowview() {
        use chrono::Utc;
        use datum_core::entry::{Entry, Verb};
        use datum_core::field_spec::{FieldSpec, Render};
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();

        // muted has render.nowview = false → drops out of nowview.
        let mut muted = FieldSpec::new("muted", Verb::Occur);
        let mut map = std::collections::HashMap::new();
        map.insert("nowview".to_string(), false);
        muted.render = Some(Render::PerSurface(map));
        db.save_field_spec(&muted).unwrap();

        // visible has the default → stays visible.
        db.save_field_spec(&FieldSpec::new("visible", Verb::Occur))
            .unwrap();

        let now = Utc::now();
        let dt = DatumTime::from_utc_and_tz(now, "UTC").unwrap();
        for f in ["muted", "visible"] {
            db.insert_entry(&Entry::new(
                f,
                Verb::Occur,
                None,
                None,
                None,
                Some(dt.clone()),
                None,
                "test",
            ))
            .unwrap();
        }

        let fields = build_fields(&db, now, 60.0).unwrap();
        let names: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
        assert!(names.contains(&"visible"), "names = {names:?}");
        assert!(
            !names.contains(&"muted"),
            "render.nowview=false must drop muted, names = {names:?}",
        );
    }

    /// da-2zd: a bool-form `render = false` hides on every surface,
    /// including nowview.
    #[test]
    fn render_all_false_drops_field_from_nowview() {
        use chrono::Utc;
        use datum_core::entry::{Entry, Verb};
        use datum_core::field_spec::{FieldSpec, Render};
        use datum_core::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("background_telemetry", Verb::Occur);
        spec.render = Some(Render::All(false));
        db.save_field_spec(&spec).unwrap();

        let now = Utc::now();
        let dt = DatumTime::from_utc_and_tz(now, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "background_telemetry",
            Verb::Occur,
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
}
