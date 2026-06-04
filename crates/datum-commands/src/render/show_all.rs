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
    "dur_precision",
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
        // da-5uph: P1D rows render as a bare local date — engineers
        // scanning the show-all dump shouldn't see a synthetic 00:00:00
        // UTC stamp for what the user logged as a date-only entry.
        "occur_time" => match &e.occur_time {
            Some(t) => {
                if e.occur_time_precision.as_deref() == Some("P1D") {
                    t.local_time().format("%Y-%m-%d").to_string()
                } else {
                    t.utc.to_rfc3339()
                }
            }
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
        "dur_precision" => render_str_opt(e.dur_precision.as_deref()),
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
        "raw_input" => render_str_opt(e.raw_input.as_deref()),
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
            dur_precision: None,
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
            raw_input: None,
            alias_chain: None,
            parser_version: 2,
            user_asserted_last_state: None,
            extra_metadata: None,
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

    /// da-5uph: a P1D row's `occur_time` renders as `YYYY-MM-DD` in the
    /// entry's local zone — never as the synthetic `00:00:00 UTC` stamp
    /// the column would emit if it ignored precision.
    #[test]
    fn occur_time_p1d_renders_as_bare_date() {
        let mut e = empty_entry();
        e.occur_time = Some(datum_core::time::DatumTime {
            utc: chrono::DateTime::parse_from_rfc3339("2026-05-12T19:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            offset_minutes: 5 * 60,
            timezone: "Asia/Karachi".to_string(),
        });
        e.occur_time_precision = Some("P1D".to_string());
        let plain = strip_ansi(&format_show_all(&e));
        // Local in +5h is 2026-05-13 — the date the user logged.
        assert!(
            plain.contains("2026-05-13"),
            "P1D occur_time should show local date:\n{plain}"
        );
        assert!(
            !plain.contains("19:00") && !plain.contains("T19:"),
            "P1D occur_time must omit clock:\n{plain}"
        );
    }

    /// da-i5vs: `dur_precision` is part of the grouped "When" view so
    /// `-A` surfaces it alongside `occur_time_precision`. NULL when unset.
    #[test]
    fn dur_precision_renders_in_when_group() {
        let mut e = empty_entry();
        e.dur_precision = Some("PT1M".to_string());
        let plain = strip_ansi(&format_show_all(&e));
        assert!(
            plain.contains("dur_precision"),
            "dur_precision column missing from -A:\n{plain}"
        );
        assert!(
            plain.contains("PT1M"),
            "dur_precision value missing from -A:\n{plain}"
        );
    }

    /// da-ol43: anti-drift guard for the six column lists above. They
    /// hand-mirror the entries-table DDL in
    /// `crates/datum-core/src/schema.sql`, and drift has already happened
    /// once (da-i5vs: `dur_precision` landed in the schema but was missing
    /// from the When group). Parse the CREATE TABLE block out of the
    /// embedded schema and assert every stored column appears in exactly
    /// one group — and that the groups list nothing the schema lacks.
    ///
    /// Extracts column names from the `CREATE TABLE ... entries (...)`
    /// body: one column definition per line, skipping comments,
    /// continuation lines, table-level constraints, and generated columns
    /// (`modify_time_ms` is a VIRTUAL derivation of `modify_time` for sync
    /// filtering — derived data, not part of the engineering dump).
    fn entries_table_columns() -> Vec<String> {
        let sql = include_str!("../../../datum-core/src/schema.sql");
        const HEADER: &str = "CREATE TABLE IF NOT EXISTS entries (";
        let body_start = sql
            .find(HEADER)
            .expect("schema.sql lost its `CREATE TABLE ... entries` block")
            + HEADER.len();
        // Walk to the matching close-paren so later tables/indexes in the
        // file are excluded from the scan.
        let mut depth = 1usize;
        let mut body_end = None;
        for (i, c) in sql[body_start..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        body_end = Some(body_start + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &sql[body_start..body_end.expect("unbalanced parens in entries DDL")];

        let mut cols = Vec::new();
        for line in body.lines() {
            let line = line.trim_start();
            if line.is_empty() || line.starts_with("--") {
                continue;
            }
            let name: String = line
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                // Continuation line (e.g. the `(CAST(...)` half of the
                // modify_time_ms definition).
                continue;
            }
            if matches!(
                name.to_ascii_uppercase().as_str(),
                "CHECK" | "PRIMARY" | "FOREIGN" | "UNIQUE" | "CONSTRAINT"
            ) {
                continue;
            }
            if line.contains("GENERATED ALWAYS") {
                continue;
            }
            cols.push(name);
        }
        cols
    }

    #[test]
    fn show_all_groups_mirror_entries_schema() {
        let schema_cols = entries_table_columns();
        // Sanity-check the extractor itself: a parser bug that returns an
        // empty/truncated list would otherwise vacuously pass.
        assert!(
            schema_cols.len() >= 20 && schema_cols.first().map(String::as_str) == Some("hid"),
            "entries-DDL extractor looks broken: {schema_cols:?}"
        );

        let mut listed: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for col in ID_COLS
            .iter()
            .chain(WHAT_COLS)
            .chain(WHEN_COLS)
            .chain(WHERE_COLS)
            .chain(BOOK_COLS)
            .chain(INTENT_COLS)
        {
            *listed.entry(col).or_insert(0) += 1;
        }

        for col in &schema_cols {
            match listed.get(col.as_str()) {
                None => panic!(
                    "schema column `{col}` is missing from the `-A` view — \
                     add `{col}` to a show_all group (ID/WHAT/WHEN/WHERE/BOOK/INTENT_COLS) \
                     and give it a `render_column` arm"
                ),
                Some(1) => {}
                Some(n) => panic!(
                    "schema column `{col}` appears in {n} show_all groups — must be exactly one"
                ),
            }
        }

        for col in listed.keys() {
            assert!(
                schema_cols.iter().any(|c| c == col),
                "show_all lists `{col}` but the entries table has no such column — \
                 remove it from its group or fix the schema"
            );
        }
    }

    /// da-5uph: exact-second rows (precision `None` or `PT1S`) keep
    /// rendering as RFC3339 — the change is scoped to `P1D`.
    #[test]
    fn occur_time_non_p1d_keeps_rfc3339() {
        let mut e = empty_entry();
        e.occur_time = Some(datum_core::time::DatumTime {
            utc: chrono::DateTime::parse_from_rfc3339("2026-05-13T09:15:30Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        });
        let plain = strip_ansi(&format_show_all(&e));
        assert!(
            plain.contains("2026-05-13T09:15:30"),
            "non-P1D occur_time should show full datetime:\n{plain}"
        );
    }
}
