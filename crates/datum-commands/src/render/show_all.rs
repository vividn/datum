//! Grouped engineering view for `-A` / `--show-all` (da-l41v).
//!
//! Reworks the prior `serde_json::to_string_pretty(entry)` dump into a
//! schema-mirroring block: each storage column appears as a `key value`
//! pair under a section heading that matches the comment dividers in
//! `crates/datum-core/src/schema.sql`. Reading the output top-to-bottom is
//! equivalent to reading the entries table definition column-by-column.
//!
//! Column ordering inside a group follows schema.sql exactly. Missing
//! values render as `NULL` so the section width acts as engineering
//! completeness reference, not a sample of what this entry happens to
//! carry. JSON values (`state`, `data`, `following_state`, …) pretty-print
//! across multiple lines when they're structured; scalars render inline.
//!
//! Color is restrained per the bead: section headings + column names get
//! a dim/gray cast, values stay default-fg, `NULL` markers dim. All
//! escapes route through [`super::paint`] so `--no-color`, `NO_COLOR`, and
//! non-TTY stdout drop to plain text automatically.
//!
//! Used from [`super::show_action_with_context`] when `Show::All`.

use datum_core::entry::Entry;
use serde_json::Value as JsonValue;

use super::{paint, DIM, GRAY_FG};

const KEY_WIDTH: usize = 25;

const ID_COLS: &[&str] = &["hid", "slug", "content_hash"];
const WHAT_COLS: &[&str] = &["field", "verb", "state", "dur", "following_state", "data"];
const WHEN_COLS: &[&str] = &[
    "occur_time",
    "occur_time_offset",
    "occur_time_tz",
    "occur_time_precision",
];
const WHERE_COLS: &[&str] = &["lat", "lon", "location"];
const BOOK_COLS: &[&str] = &["create_time", "modify_time", "device_id", "deleted_at"];
const INTENT_COLS: &[&str] = &[
    "raw_input",
    "alias_chain",
    "parser_version",
    "user_asserted_last_state",
    "id_structure",
    "field_structure",
    "extra_metadata",
];

/// Render the grouped engineering view. Returns the rendered block with
/// a trailing newline-free body; the caller adds its own surrounding
/// newlines if needed.
pub fn format_show_all(entry: &Entry) -> String {
    let mut out = String::new();
    push_group(&mut out, "Identity", ID_COLS, entry);
    push_group(&mut out, "What", WHAT_COLS, entry);
    push_group(&mut out, "When", WHEN_COLS, entry);
    push_group(&mut out, "Where", WHERE_COLS, entry);
    push_group(&mut out, "Bookkeeping", BOOK_COLS, entry);
    push_group(&mut out, "Intent", INTENT_COLS, entry);
    // Trim trailing newline so println!() in the caller doesn't double-space.
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn push_group(out: &mut String, name: &str, cols: &[&str], entry: &Entry) {
    out.push_str(&heading(name));
    out.push('\n');
    for col in cols {
        let rendered = render_column(col, entry);
        push_row(out, col, &rendered);
    }
    out.push('\n');
}

fn heading(name: &str) -> String {
    // `── <Name> ` followed by a long em-dash run, in dim gray so it
    // reads as a comment divider rather than data.
    let dashes = "─".repeat(40_usize.saturating_sub(name.chars().count() + 4));
    paint(GRAY_FG, &format!("── {name} {dashes}"))
}

fn push_row(out: &mut String, key: &str, value: &str) {
    let padded_key = format!("{key:<KEY_WIDTH$}");
    let painted_key = paint(GRAY_FG, &padded_key);
    if value.contains('\n') {
        // Multi-line value (typically pretty-printed JSON): print the key on
        // its own line, then indent each value line under it so the schema
        // column is still scannable at the left margin.
        out.push_str("  ");
        out.push_str(&painted_key);
        out.push('\n');
        for line in value.lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    } else {
        out.push_str("  ");
        out.push_str(&painted_key);
        out.push(' ');
        out.push_str(value);
        out.push('\n');
    }
}

fn render_column(col: &str, e: &Entry) -> String {
    match col {
        "hid" => e.hid.to_string(),
        "slug" => e.slug.to_string(),
        "content_hash" => e.content_hash.clone(),
        "field" => e.field.clone(),
        "verb" => e.verb.to_string(),
        "state" => render_json_opt(e.state.as_ref()),
        "dur" => match &e.dur {
            Some(d) => d.to_storage_string(),
            None => null_marker(),
        },
        "following_state" => render_json_opt(e.following_state.as_ref()),
        "data" => render_json_opt(e.data.as_ref()),
        "occur_time" => match &e.occur_time {
            Some(t) => t.utc.to_rfc3339(),
            None => null_marker(),
        },
        "occur_time_offset" => match &e.occur_time {
            Some(t) => format!("{} min", t.offset_minutes),
            None => null_marker(),
        },
        "occur_time_tz" => match &e.occur_time {
            Some(t) => t.timezone.clone(),
            None => null_marker(),
        },
        "occur_time_precision" => render_str_opt(e.occur_time_precision.as_deref()),
        "lat" => match e.lat {
            Some(v) => v.to_string(),
            None => null_marker(),
        },
        "lon" => match e.lon {
            Some(v) => v.to_string(),
            None => null_marker(),
        },
        "location" => render_str_opt(e.location.as_deref()),
        "create_time" => e.create_time.to_rfc3339(),
        "modify_time" => e.modify_time.to_rfc3339(),
        "device_id" => e.device_id.clone(),
        "deleted_at" => match e.deleted_at {
            Some(t) => t.to_rfc3339(),
            None => null_marker(),
        },
        "raw_input" => {
            if e.raw_input.is_empty() {
                null_marker()
            } else {
                e.raw_input.clone()
            }
        }
        "alias_chain" => render_json_opt(e.alias_chain.as_ref()),
        "parser_version" => e.parser_version.to_string(),
        "user_asserted_last_state" => render_json_opt(e.user_asserted_last_state.as_ref()),
        "id_structure" => render_str_opt(e.id_structure.as_deref()),
        "field_structure" => render_str_opt(e.field_structure.as_deref()),
        "extra_metadata" => render_json_opt(e.extra_metadata.as_ref()),
        _ => null_marker(),
    }
}

fn null_marker() -> String {
    paint(DIM, "NULL")
}

fn render_str_opt(s: Option<&str>) -> String {
    match s {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => null_marker(),
    }
}

fn render_json_opt(v: Option<&JsonValue>) -> String {
    match v {
        None => null_marker(),
        Some(JsonValue::Null) => null_marker(),
        Some(value) => match value {
            // Structured values pretty-print; scalars stay inline so a
            // simple `"happy"` doesn't fan out to three lines.
            JsonValue::Object(_) | JsonValue::Array(_) => {
                serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
            }
            JsonValue::String(s) => format!("\"{s}\""),
            other => other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::colors::strip_ansi;
    use super::*;
    use datum_core::dur::Dur;
    use datum_core::entry::Verb;
    use datum_core::ids::{Hid, Slug};
    use serde_json::json;

    fn empty_entry() -> Entry {
        Entry {
            hid: Hid::new("abc123def456"),
            slug: Slug::new("coffee:2026-05-13T09:15Z"),
            content_hash: "deadbeef".to_string(),
            field: "coffee".to_string(),
            verb: Verb::Point,
            state: None,
            dur: None,
            following_state: None,
            data: None,
            occur_time: None,
            occur_time_precision: None,
            lat: None,
            lon: None,
            location: None,
            create_time: chrono::DateTime::parse_from_rfc3339("2026-05-13T09:15:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            modify_time: chrono::DateTime::parse_from_rfc3339("2026-05-13T09:16:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            device_id: "dev-1".to_string(),
            id_structure: None,
            field_structure: None,
            deleted_at: None,
            raw_input: String::new(),
            alias_chain: None,
            parser_version: 2,
            user_asserted_last_state: None,
            extra_metadata: None,
            start_time: None,
            end_time: None,
            start_time_precision: 0,
            start_time_precision_unit: datum_core::entry::PrecisionUnit::Exact,
            end_time_precision: 0,
            end_time_precision_unit: datum_core::entry::PrecisionUnit::Exact,
            entry_shape: datum_core::entry::EntryShape::Point,
            intent: None,
            value: None,
        }
    }

    #[test]
    fn every_schema_column_appears() {
        let e = empty_entry();
        let plain = strip_ansi(&format_show_all(&e));
        for col in ID_COLS
            .iter()
            .chain(WHAT_COLS)
            .chain(WHEN_COLS)
            .chain(WHERE_COLS)
            .chain(BOOK_COLS)
            .chain(INTENT_COLS)
        {
            assert!(
                plain.contains(col),
                "expected schema column {col} in output:\n{plain}"
            );
        }
    }

    #[test]
    fn group_headings_in_schema_order() {
        let plain = strip_ansi(&format_show_all(&empty_entry()));
        let order = ["Identity", "What", "When", "Where", "Bookkeeping", "Intent"];
        let mut last = 0;
        for name in order {
            let at = plain
                .find(name)
                .unwrap_or_else(|| panic!("missing heading {name} in:\n{plain}"));
            assert!(at >= last, "heading {name} out of order in:\n{plain}");
            last = at;
        }
    }

    #[test]
    fn null_marker_used_for_missing_values() {
        let plain = strip_ansi(&format_show_all(&empty_entry()));
        // dur, state, lat, etc. on a bare entry all collapse to NULL.
        assert!(plain.contains("NULL"), "NULL marker missing:\n{plain}");
    }

    #[test]
    fn structured_data_pretty_prints() {
        let mut e = empty_entry();
        e.data = Some(json!({"weight": {"v": 75.2, "u": "kg"}, "reps": 10}));
        let plain = strip_ansi(&format_show_all(&e));
        // Multi-line JSON: the value indents under the `data` key.
        assert!(plain.contains("\"weight\""), "data not rendered: {plain}");
        assert!(plain.contains("\"reps\""), "data not rendered: {plain}");
    }

    #[test]
    fn dur_uses_storage_string() {
        let mut e = empty_entry();
        e.dur = Some(Dur::Moment);
        let plain = strip_ansi(&format_show_all(&e));
        assert!(
            plain.contains("MOMENT"),
            "dur Moment must serialize to MOMENT:\n{plain}"
        );
    }
}
