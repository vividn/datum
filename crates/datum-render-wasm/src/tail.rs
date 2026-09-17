//! `datum tail`, in the browser: the CLI's own table, and the rows behind it.
//!
//! The tail pane and the REPL's `tail` are two surfaces of one row set
//! (da-6ec2), so this module builds the row set once and offers it twice:
//!
//! * [`tail_json`] — the text `datum tail` would print, header and all, for
//!   a caller that shows CLI output as CLI output (the REPL).
//! * [`tail_rows_json`] — the same rows as data, plus the columns already
//!   painted, for a caller with a grid of its own (the pane's `<table>`).
//!
//! Neither re-derives anything. The table is
//! `datum_commands::render::format_table_with`; the columns are
//! `table_columns_with`, which is the same cells before the terminal
//! padding; the `_n` ordinals are `render::quick_ordinal_map`, the pure
//! half of the query `datum tail` runs against SQLite; and the colours are
//! `render::colors`. What the browser supplies is only what it alone knows:
//! the rows, each field's history, the clock and the viewer's zone.

use std::collections::HashMap;

use datum_commands::render::colors::{field_hex, state_hex, ColorOverride};
use datum_commands::render::tail_rows::tail_rows;
use datum_commands::render::{
    attach_quick_ordinals, format_table_with, quick_ordinal_map, table_columns_with, RenderContext,
    TableArgs, TimeMetricCol,
};
use datum_core::entry::Entry;
use datum_core::field_spec::SpecRegistry;
use datum_core::state::FieldStateEvents;
use datum_parse::DatumTime;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::request::{entry_from_web, now_from, registry_from_web};

/// What the module needs from the browser to list rows.
#[derive(Debug, Deserialize)]
pub struct TailRequest {
    /// The rows to list, as the browser stores them, in the order they are
    /// to be shown — oldest first, the way `datum tail` prints. The `_n`
    /// ordinals do not depend on this order; they count from the newest row
    /// whichever end of the listing it is at.
    pub entries: Vec<JsonValue>,
    /// Each listed field's live entries, keyed by field name. What the CLI
    /// compiles a row's `∅` slot and transition colours from, loaded from
    /// SQLite; here it arrives already loaded. A field absent from the map
    /// renders as a field with no history.
    #[serde(default)]
    pub histories: HashMap<String, Vec<JsonValue>>,
    pub now_ms: i64,
    /// The viewer's IANA zone, when the browser can name one.
    #[serde(default)]
    pub timezone: Option<String>,
    /// The viewer's UTC offset in minutes, east-positive.
    pub offset_minutes: i32,
    /// Paint the cells. See `RenderRequest::color`.
    #[serde(default)]
    pub color: bool,
    /// The field specs the browser holds. See `RenderRequest::specs`.
    #[serde(default)]
    pub specs: Vec<JsonValue>,
}

/// The listing, resolved: datum's own rows, one context each.
struct Listing {
    entries: Vec<Entry>,
    contexts: Vec<RenderContext>,
    now: DatumTime,
    registry: SpecRegistry,
}

impl Listing {
    fn of(req: &TailRequest) -> Result<Self, String> {
        let entries = req
            .entries
            .iter()
            .map(entry_from_web)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("entries: {e}"))?;
        // One `FieldStateEvents` per field, built once and shared by every
        // row of that field — the shape `build_render_contexts` gives the
        // CLI, for the same reason: a probe per row would recompile the
        // same history ten times over.
        let events: HashMap<&str, FieldStateEvents> = req
            .histories
            .iter()
            .map(|(field, rows)| {
                let rows: Vec<Entry> = rows.iter().filter_map(|r| entry_from_web(r).ok()).collect();
                (field.as_str(), FieldStateEvents::from_entries(rows.iter()))
            })
            .collect();
        let mut contexts: Vec<RenderContext> = entries
            .iter()
            .map(|e| RenderContext::from_events(events.get(e.field.as_str()), e))
            .collect();
        // The `_n` shorthands. The CLI's universe is its whole database and
        // the browser's is the rows it was handed; for an unfiltered tail
        // those are the same set, and the rule for turning it into ordinals
        // is one function either way.
        attach_quick_ordinals(&mut contexts, &entries, &quick_ordinal_map(&entries));
        Ok(Self {
            entries,
            contexts,
            now: now_from(req.now_ms, req.timezone.clone(), req.offset_minutes)?,
            registry: registry_from_web(&req.specs),
        })
    }
}

/// The table `datum tail` prints, for these rows at this instant.
///
/// `{"text": "..."}` on success, `{"error": "..."}` otherwise. The text is
/// [`format_table_with`]'s own output — header, alignment, `_n` column,
/// colours — down to the trailing newline.
pub fn tail_json(body: &str) -> String {
    match table_text(body) {
        Ok(text) => json!({ "text": text }).to_string(),
        Err(e) => json!({ "error": e }).to_string(),
    }
}

fn table_text(body: &str) -> Result<String, String> {
    let req: TailRequest = serde_json::from_str(body).map_err(|e| format!("bad request: {e}"))?;
    let listing = Listing::of(&req)?;
    let _color = ColorOverride::set(req.color);
    // `TableArgs::default()` is `datum tail` with no flags and no terminal
    // to fit: the hybrid time metric, no extra columns, a header row, and
    // no width budget — which is exactly what the CLI uses when its stdout
    // is not a tty (`commands/tail.rs`: `max_total_width` stays `None`).
    format_table_with(
        &listing.entries,
        Some(&listing.contexts),
        Some(&listing.now),
        Some(&listing.registry),
        &TableArgs::default(),
    )
    .ok_or_else(|| "the table was silenced".to_string())
}

/// The same listing as data: `{"rows": [...], "columns": [...]}`.
///
/// `rows` is `datum tail --json`'s row shape
/// (`datum_commands::render::tail_rows`) with the colours the CLI would
/// paint added — `field_color`, `state_color`, and the `∅` / LEAD / TRAIL
/// marks as flags — so a caller that is not a terminal can style a row
/// without deciding any of it. `columns` is the table's own columns in the
/// table's own order, each cell painted and unpadded: a caller with real
/// columns lays those out itself, and never re-derives what is in them.
pub fn tail_rows_json(body: &str) -> String {
    match rows_and_columns(body) {
        Ok(v) => v.to_string(),
        Err(e) => json!({ "error": e }).to_string(),
    }
}

fn rows_and_columns(body: &str) -> Result<JsonValue, String> {
    let req: TailRequest = serde_json::from_str(body).map_err(|e| format!("bad request: {e}"))?;
    let listing = Listing::of(&req)?;
    let _color = ColorOverride::set(req.color);
    let columns = table_columns_with(
        &listing.entries,
        Some(&listing.contexts),
        Some(&listing.now),
        Some(&listing.registry),
        &TableArgs::default(),
    );
    let rows = tail_rows(
        &listing.entries,
        Some(&listing.contexts),
        TimeMetricCol::Hybrid,
    );
    let rows: Vec<JsonValue> = rows
        .iter()
        .zip(listing.entries.iter().zip(&listing.contexts))
        .map(|(row, (entry, ctx))| {
            let mut v = serde_json::to_value(row).map_err(|e| e.to_string())?;
            let spec = listing.registry.fields.get(&entry.field);
            let object = v
                .as_object_mut()
                .ok_or("a row serialized to a non-object")?;
            object.insert("field_color".into(), json!(field_hex(&entry.field, spec)));
            object.insert(
                "state_color".into(),
                json!(entry
                    .state
                    .as_ref()
                    .and_then(|s| state_hex(s, &entry.field, spec))),
            );
            // The marks the state cell carries, named. `∅` is the CLI's
            // "this field had no state here, or was being ignored"; LEAD
            // and TRAIL are the two edges of a transition.
            object.insert("prior_state_ignored".into(), json!(ctx.prior_state_ignored));
            object.insert("lead".into(), json!(ctx.lead_state.is_some()));
            object.insert("trail".into(), json!(ctx.trail_state.is_some()));
            Ok(v)
        })
        .collect::<Result<_, String>>()?;
    Ok(json!({ "rows": rows, "columns": columns }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_commands::render::strip_ansi;

    /// The same instant the crate docs and `request.rs`'s tests use:
    /// 2026-09-08T14:17:25Z, in a zone two hours east.
    const NOW_MS: i64 = 1_788_877_045_000;
    const OFFSET: i32 = 120;

    /// Exactly what the browser does: parse the argv with datum's parser,
    /// list the rows it built. `n` seeds the hid so rows are distinct.
    fn parsed(argv: &[&str], n: u8) -> JsonValue {
        let req = json!({
            "argv": argv,
            "now_ms": NOW_MS,
            "timezone": "Europe/Berlin",
            "offset_minutes": OFFSET,
            "device_id": "test",
            "hid_bytes": [n, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        });
        let res: JsonValue =
            serde_json::from_str(&datum_parse_wasm::parse_json(&req.to_string())).unwrap();
        res.get("entry")
            .cloned()
            .expect("the parser built an entry")
    }

    fn request(entries: &[JsonValue], color: bool) -> String {
        json!({
            "entries": entries,
            "histories": {},
            "now_ms": NOW_MS,
            "timezone": "Europe/Berlin",
            "offset_minutes": OFFSET,
            "color": color,
        })
        .to_string()
    }

    fn text(entries: &[JsonValue], color: bool) -> String {
        let res: JsonValue = serde_json::from_str(&tail_json(&request(entries, color))).unwrap();
        assert!(res.get("error").is_none(), "tail error: {res}");
        res["text"].as_str().unwrap().to_string()
    }

    fn listed(entries: &[JsonValue], color: bool) -> JsonValue {
        let res: JsonValue =
            serde_json::from_str(&tail_rows_json(&request(entries, color))).unwrap();
        assert!(res.get("error").is_none(), "tail_rows error: {res}");
        res
    }

    /// The table is the CLI's table: a header naming the columns, then one
    /// line per row, oldest at the top the way `tail` prints.
    #[test]
    fn the_text_is_the_cli_s_table() {
        let rows = [
            parsed(&["occur", "coffee", "-t", "1200"], 1),
            parsed(&["switch", "mood", "happy", "-t", "1300"], 2),
        ];
        let plain = strip_ansi(&text(&rows, false));
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(lines.len(), 3, "header plus two rows: {plain}");
        // `_n` first (da-431c), then `time (+2)` — the offset rides in the header (da-lkji), and the
        // rest of the words are the CLI's own column names.
        assert!(lines[0].starts_with("_n  time (+2)"), "{}", lines[0]);
        for word in ["field", "state", "hid"] {
            assert!(lines[0].contains(word), "no `{word}` column: {}", lines[0]);
        }
        assert!(lines[1].contains("coffee"), "{}", lines[1]);
        assert!(lines[2].contains("mood"), "{}", lines[2]);
        assert!(lines[2].contains("happy"), "{}", lines[2]);
    }

    /// `_1` is the newest row, which in a tail is the bottom one.
    #[test]
    fn the_ordinals_count_up_from_the_newest_row() {
        let rows = [
            parsed(&["occur", "coffee", "-t", "1200"], 1),
            parsed(&["occur", "coffee", "-t", "1300"], 2),
        ];
        let listing = listed(&rows, false);
        let quick: Vec<&str> = listing["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["quick_id"].as_str().unwrap())
            .collect();
        assert_eq!(quick, vec!["_2", "_1"]);
    }

    /// Colour is a request bit: on, the field badge arrives as a truecolor
    /// SGR sequence; off, the same table carries no escapes at all.
    #[test]
    fn colour_is_on_the_request_and_off_by_default() {
        let rows = [parsed(&["occur", "coffee", "-t", "1200"], 1)];
        let painted = text(&rows, true);
        assert!(
            painted.contains("\x1b[38;2;"),
            "no truecolor fg: {painted:?}"
        );
        let plain = text(&rows, false);
        assert!(
            !plain.contains('\x1b'),
            "escapes with colour off: {plain:?}"
        );
        // And the two say the same thing — colour adds no words.
        assert_eq!(strip_ansi(&painted), plain);
    }

    /// The gate does not leak: a painted call leaves the next unpainted
    /// one unpainted. The module is reused across calls, so this is the
    /// trap the guard exists for.
    #[test]
    fn a_painted_call_does_not_paint_the_next_one() {
        let rows = [parsed(&["occur", "coffee", "-t", "1200"], 1)];
        let _ = text(&rows, true);
        assert!(!text(&rows, false).contains('\x1b'));
    }

    /// The columns are the table's columns: same order, same header words,
    /// and the same cell text once the paint is stripped.
    #[test]
    fn the_columns_are_the_table_s_own() {
        let rows = [
            parsed(&["occur", "coffee", "-t", "1200"], 1),
            parsed(&["switch", "mood", "happy", "-t", "1300"], 2),
        ];
        let listing = listed(&rows, true);
        let columns = listing["columns"].as_array().unwrap();
        let keys: Vec<&str> = columns.iter().map(|c| c["key"].as_str().unwrap()).collect();
        assert_eq!(keys, vec!["_n", "time", "field", "state", "hid"]);

        let table = strip_ansi(&text(&rows, false));
        let header = table.lines().next().unwrap();
        for column in columns {
            let word = strip_ansi(column["header"].as_str().unwrap());
            assert!(header.contains(&word), "header lost `{word}`: {header}");
        }
        // Every cell is somewhere in its row, in the row's own order.
        for (i, line) in table.lines().skip(1).enumerate() {
            let mut at = 0;
            for column in columns {
                let cell = strip_ansi(column["cells"][i].as_str().unwrap());
                if cell.is_empty() {
                    continue;
                }
                let found = line[at..]
                    .find(&cell)
                    .unwrap_or_else(|| panic!("`{cell}` is not next in `{line}`"));
                at += found + cell.len();
            }
        }
    }

    /// The colours the rows report are the colours the cells are painted
    /// with — one derivation, reported two ways, so a caller styling a
    /// `<td>` from `field_color` gets what a terminal would show.
    #[test]
    fn the_row_colours_are_the_cell_colours() {
        let rows = [parsed(&["switch", "mood", "happy", "-t", "1300"], 1)];
        let listing = listed(&rows, true);
        let row = &listing["rows"][0];
        let field_color = row["field_color"].as_str().unwrap();
        let state_color = row["state_color"].as_str().unwrap();
        assert_ne!(field_color, state_color, "a field is not its state");

        let sgr = |hex: &str| {
            let n = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
            format!("38;2;{};{};{}", n(1), n(3), n(5))
        };
        let bg = |hex: &str| {
            let n = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
            format!("48;2;{};{};{}", n(1), n(3), n(5))
        };
        let column = |key: &str| {
            listing["columns"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["key"] == key)
                .unwrap()["cells"][0]
                .as_str()
                .unwrap()
                .to_string()
        };
        // The field badge is a bg chip; the state chip too.
        assert!(column("field").contains(&bg(field_color)), "{field_color}");
        assert!(column("state").contains(&bg(state_color)), "{state_color}");
        // The fg spelling of the same colour is what the info line uses,
        // so the helper has to be able to produce it as well.
        assert!(!sgr(field_color).is_empty());
    }

    /// A row with no state reports no state colour — `null`, not a colour
    /// invented for an absence.
    #[test]
    fn a_stateless_row_reports_no_state_colour() {
        let rows = [parsed(&["occur", "coffee", "-t", "1200"], 1)];
        let listing = listed(&rows, false);
        assert!(listing["rows"][0]["state_color"].is_null());
        assert_eq!(listing["rows"][0]["field"], "coffee");
    }

    /// A field's declared colour is its colour here too. The widget bar
    /// writes these specs (`spec set --color`), and a field that paints
    /// one way in the terminal must not paint another in the browser.
    #[test]
    fn a_declared_field_colour_wins_over_the_hash() {
        let rows = [parsed(&["occur", "coffee", "-t", "1200"], 1)];
        let body = json!({
            "entries": rows,
            "now_ms": NOW_MS,
            "offset_minutes": OFFSET,
            "color": true,
            "specs": [{ "field": "coffee", "verb": "occur", "color": "#ff6600" }],
        })
        .to_string();
        let res: JsonValue = serde_json::from_str(&tail_rows_json(&body)).unwrap();
        assert_eq!(res["rows"][0]["field_color"], "#ff6600");
    }

    /// The `∅` the CLI prints for a field with no prior state is reported
    /// as a flag as well as painted into the cell.
    #[test]
    fn a_first_entry_reports_the_empty_prior_state() {
        let rows = [parsed(&["occur", "coffee", "-t", "1200"], 1)];
        let listing = listed(&rows, false);
        assert_eq!(listing["rows"][0]["prior_state_ignored"], json!(true));
        let cell = listing["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["key"] == "state")
            .unwrap()["cells"][0]
            .as_str()
            .unwrap()
            .to_string();
        assert!(cell.contains('∅'), "the cell lost its mark: {cell}");
    }

    /// An empty listing is the CLI's own empty listing, not a panic.
    #[test]
    fn no_rows_is_the_cli_s_no_data() {
        assert_eq!(text(&[], false), "[No data]\n");
        let listing = listed(&[], false);
        assert_eq!(listing["rows"].as_array().unwrap().len(), 0);
        assert_eq!(listing["columns"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn garbage_in_is_an_error_not_a_panic() {
        for res in [tail_json("not json"), tail_rows_json("not json")] {
            let res: JsonValue = serde_json::from_str(&res).unwrap();
            assert!(res.get("error").is_some(), "{res}");
        }
    }
}
