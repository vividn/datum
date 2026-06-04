//! Multiline / inline block rendering for the **state second line** and the
//! **user-data block** (da-gvj1 rules 3-11).
//!
//! Tail keeps the compact `{id}`-chip main line and nothing else; doc-style
//! surfaces (`get`, write-action output) follow up with a second line that
//! exposes the structure of `Structured` / `Parallel` states. The user-data
//! block (everything in `extra_data` except `id`) goes through the same
//! alignment logic.
//!
//! Layout rules:
//! * Inline when the rendered width fits the threshold (≈40 chars for data,
//!   ≈65 for state). Multiline otherwise.
//! * Multiline: keys right-padded so each key's last char touches `:`. All
//!   `:` columns share an alignment column within the block; the `{` (or
//!   `[ {`) opens on the first content line, the `}` trails the last value.
//! * In the state second line, **keys are bare**, **values are chipped**.
//!   Chip background is hashed by `key=value` (rule 5) so `.format=audio`
//!   and `.format=video` get different colors.
//! * Value rendering (rule 11): strings quoted, numbers/bools bare, null
//!   left as `null`. The unit-annotated `{v, u}` shape compacts to
//!   `"<v><u>"` matching the existing data-block treatment.
//!
//! Widths are measured with [`super::colors::visible_width`] so ANSI
//! escapes don't throw off alignment. Since da-dhtp that measure is
//! Unicode display width (emoji/CJK = 2 columns, zero-width = 0), so
//! multi-byte keys and values align exactly rather than approximately.

use datum_core::{entry::Entry, field_spec::FieldSpec};
use serde_json::Value as JsonValue;

use super::colors::{paint_bg, state_rgb, visible_width};

/// Inline-vs-multiline threshold for the state second line (rule 7).
const STATE_INLINE_WIDTH: usize = 65;
/// Inline-vs-multiline threshold for the user-data block (rule 8).
const DATA_INLINE_WIDTH: usize = 40;

// ─── State second line ──────────────────────────────────────────────────────

/// Render the state second line for a doc-style surface. Returns `None`
/// when the state has no extra structure beyond what the main-line chip
/// already shows: simple strings, bools, `null`, `IGNORED`, and parallels
/// of only-simple states all collapse to no second line.
pub(crate) fn format_state_doc_line(
    value: &JsonValue,
    _field: &str,
    spec: Option<&FieldSpec>,
) -> Option<String> {
    match value {
        JsonValue::Object(obj) => Some(render_struct_block(obj, spec, 0)),
        JsonValue::Array(items) if has_structured_member(items) => {
            Some(render_parallel_block(items, spec))
        }
        _ => None,
    }
}

fn has_structured_member(items: &[JsonValue]) -> bool {
    items.iter().any(|v| matches!(v, JsonValue::Object(_)))
}

/// Render a single struct (`{ .k: v, … }`). Inline if it fits the state
/// threshold; otherwise multiline with right-aligned keys. `indent` is the
/// leading column where the opening `{` lands — used by the parallel
/// renderer to align nested struct lines under the outer `[ {`.
fn render_struct_block(
    obj: &serde_json::Map<String, JsonValue>,
    spec: Option<&FieldSpec>,
    indent: usize,
) -> String {
    let pairs: Vec<(String, String)> = obj
        .iter()
        .map(|(k, v)| (format!(".{k}"), render_value_chip(k, v, spec)))
        .collect();

    let inline = render_struct_inline(&pairs);
    if visible_width(&inline) <= STATE_INLINE_WIDTH {
        return inline;
    }
    render_struct_multiline(&pairs, indent)
}

fn render_struct_inline(pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return "{}".to_string();
    }
    let body = pairs
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ {body} }}")
}

/// Multiline form (rule 9/10):
///
/// ```text
/// {       .id: <chip>
///      .title: <chip>
///   .recorded: <chip> }
/// ```
fn render_struct_multiline(pairs: &[(String, String)], indent: usize) -> String {
    if pairs.is_empty() {
        return "{}".to_string();
    }
    let key_w = pairs
        .iter()
        .map(|(k, _)| visible_width(k))
        .max()
        .unwrap_or(0);
    // Opening line starts with "{ " then key padded to key_w. Width of
    // the leading marker `"{ "` is 2; subsequent lines pad with spaces so
    // the keys align under the first key (i.e. shifted right by 2 from
    // the `{`).
    let mut lines: Vec<String> = Vec::with_capacity(pairs.len());
    let pad_after_brace = " ".repeat(2);
    for (i, (k, v)) in pairs.iter().enumerate() {
        let kw = visible_width(k);
        let lpad = " ".repeat(key_w - kw);
        let leader = if i == 0 {
            "{ ".to_string()
        } else {
            " ".repeat(indent) + &pad_after_brace
        };
        let trailer = if i == pairs.len() - 1 { " }" } else { "" };
        lines.push(format!("{leader}{lpad}{k}: {v}{trailer}"));
    }
    lines.join("\n")
}

/// Render a parallel state block (`[ { … }, { … } ]`) with one inner
/// state per line (rule 4). Each inner block decides inline-vs-multiline
/// independently (rule 7).
fn render_parallel_block(items: &[JsonValue], spec: Option<&FieldSpec>) -> String {
    // The opening `[ ` indents nested structs by 2 columns; the closing
    // `]` trails the last item. Each item lives on its own line; doc
    // inline lists in parallel get `, ` (with space, rule 6) but with
    // "one inner state per line" we just put a `,` at end-of-line
    // followed by a newline. There's no inline-vs-multiline decision at
    // the parallel level itself — always one per line.
    let mut lines: Vec<String> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        // Each inner item: if it's a struct, render inline-or-multiline
        // with indent=2 (positioned under the `[ `). Simple states get
        // rendered as a chip so the `.format=audio` style applies.
        let body = match item {
            JsonValue::Object(obj) => render_struct_block(obj, spec, 2),
            other => render_value_chip("", other, spec),
        };
        let leader = if i == 0 {
            "[ ".to_string()
        } else {
            "  ".to_string()
        };
        let trailer = if i == items.len() - 1 { " ]" } else { "," };
        lines.push(format!("{leader}{body}{trailer}"));
    }
    lines.join("\n")
}

/// Render a single value as a chip. Background color is hashed by the
/// caller-supplied key (e.g. `"format=audio"`); when `key` is empty we
/// hash by the rendered value alone, which is the simple-state case
/// inside a parallel block.
///
/// Special case: the `.id` field hashes by the bare string value (no
/// `key=` prefix, no surrounding quotes) so the second-line `.id` chip
/// matches the main-line `{id}` chip — both routes feed `state_rgb`
/// the same input. See `colors::structured_state_rgb` for the main path.
fn render_value_chip(key: &str, value: &JsonValue, spec: Option<&FieldSpec>) -> String {
    let body = render_value_body(value);
    let hash_key = if key == "id" {
        match value.as_str() {
            Some(id) => id.to_string(),
            None => body.clone(),
        }
    } else if key.is_empty() {
        body.clone()
    } else {
        format!("{key}={body}")
    };
    paint_bg(state_rgb(&hash_key, spec), &body)
}

/// Render a JSON value as a plain string per rule 11. Strings get quoted,
/// numbers and booleans render bare. The unit-annotated `{v, u}` shape
/// (e.g. `{v: 75.2, u: "kg"}`) is treated as a string and emerges as
/// `"75.2kg"`, matching the existing parity treatment of unit-annotated
/// leaves.
fn render_value_body(value: &JsonValue) -> String {
    match value {
        JsonValue::String(s) => format!("\"{s}\""),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => "null".to_string(),
        JsonValue::Object(m) => {
            if let (Some(v), Some(u)) = (m.get("v"), m.get("u").and_then(JsonValue::as_str)) {
                let inner = match v {
                    JsonValue::String(s) => s.clone(),
                    other => other.to_string(),
                };
                return format!("\"{inner}{u}\"");
            }
            serde_json::to_string(value).unwrap_or_default()
        }
        JsonValue::Array(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

// ─── User-data block ────────────────────────────────────────────────────────

/// Render the user-data block (entry's `extra_data`) per rules 8–11.
/// Returns `None` when there's nothing to render (no data, empty object,
/// or only-`id` data).
///
/// The block is written with a 2-space leading indent so it visually
/// nests under the info line. Inline form (`key: value, key: value`) is
/// used when the rendered width ≤ 40 chars; otherwise the block is
/// multiline with right-aligned keys.
pub(crate) fn format_data_block(entry: &Entry) -> Option<String> {
    let extra = entry.data.as_ref()?;
    let map = extra.as_object()?;
    let pairs: Vec<(String, JsonValue)> = map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    if pairs.is_empty() {
        return None;
    }

    // Render value bodies once; no chipping in the data block (rule 5
    // applies to the state second line only).
    let rendered: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| (k.clone(), render_value_body(v)))
        .collect();

    let inline = render_data_inline(&rendered);
    let block = if visible_width(&inline) <= DATA_INLINE_WIDTH {
        format!("  {inline}")
    } else {
        render_data_multiline(&rendered)
    };
    Some(block)
}

fn render_data_inline(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_data_multiline(pairs: &[(String, String)]) -> String {
    let key_w = pairs
        .iter()
        .map(|(k, _)| visible_width(k))
        .max()
        .unwrap_or(0);
    let mut lines: Vec<String> = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        let pad = " ".repeat(key_w - visible_width(k));
        lines.push(format!("  {pad}{k}: {v}"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    // da-46nj: tests that toggle the process-global `DATUM_COLOR`/`NO_COLOR`
    // env vars must run serialized — otherwise one test's `set_var`/
    // `remove_var` races with another's read inside `render_value_chip` /
    // `format_state_doc_line`. The shared scopes lock the process-wide
    // poison-tolerant mutex and restore the prior env on drop (da-3ywd).
    use datum_test_util::{color_scope, no_color_scope};

    #[test]
    fn simple_string_state_renders_no_second_line() {
        let out = format_state_doc_line(&json!("happy"), "mood", None);
        assert!(out.is_none());
    }

    #[test]
    fn structured_state_renders_inline_when_short() {
        let _scope = no_color_scope();
        let v = json!({"id": "joe"});
        let line = format_state_doc_line(&v, "call", None).unwrap();
        assert_eq!(line, "{ .id: \"joe\" }");
    }

    #[test]
    fn structured_state_renders_multiline_when_long() {
        let _scope = no_color_scope();
        let v = json!({
            "id": "bob",
            "title": "Q2 retro 2024 — all-hands strategy session running long",
            "format": "audio",
            "recorded": true,
        });
        let line = format_state_doc_line(&v, "call", None).unwrap();
        // Multi-line, all `:` align at the same column inside the block.
        for ln in line.lines() {
            assert!(ln.contains(':'), "every line carries a key: {ln:?}");
        }
        assert!(
            line.lines().count() >= 2,
            "long state must wrap to multiline: {line}"
        );
    }

    #[test]
    fn parallel_with_only_simple_members_renders_no_second_line() {
        let v = json!(["vertical", "sit"]);
        let out = format_state_doc_line(&v, "body", None);
        assert!(
            out.is_none(),
            "all-simple parallel needs no second line: {out:?}"
        );
    }

    #[test]
    fn parallel_with_structured_member_renders_brackets() {
        let _scope = no_color_scope();
        let v = json!([
            {"id": "joe"},
            {"id": "bob", "format": "audio"},
        ]);
        let line = format_state_doc_line(&v, "call", None).unwrap();
        assert!(line.starts_with("[ {"), "opens with `[ {{`: {line:?}");
        assert!(line.ends_with(" ]"), "ends with `]`: {line:?}");
        // One inner state per line — rule 4.
        let comma_lines: Vec<&str> = line
            .lines()
            .filter(|l| l.trim_end().ends_with(','))
            .collect();
        assert!(
            !comma_lines.is_empty(),
            "must have a trailing-comma separator line: {line:?}"
        );
    }

    #[test]
    fn data_block_under_threshold_inline() {
        let mut entry = sample_entry();
        entry.data = Some(json!({"amount": "200mg"}));
        let block = format_data_block(&entry).unwrap();
        assert_eq!(block, "  amount: \"200mg\"");
    }

    #[test]
    fn data_block_over_threshold_multiline_right_aligned() {
        let mut entry = sample_entry();
        entry.data = Some(json!({
            "duration_seconds": 7200,
            "place": "the long forgotten place where things linger",
            "n": 1,
        }));
        let block = format_data_block(&entry).unwrap();
        let lines: Vec<&str> = block.lines().collect();
        assert!(lines.len() >= 3, "multi-line: {block}");
        // All colons land at the same column.
        let colon_cols: Vec<usize> = lines.iter().map(|l| l.find(':').unwrap()).collect();
        assert!(
            colon_cols.windows(2).all(|w| w[0] == w[1]),
            "colons not aligned: {colon_cols:?}"
        );
        // No space immediately before the colon.
        for ln in &lines {
            let idx = ln.find(':').unwrap();
            assert_ne!(ln.as_bytes()[idx - 1], b' ', "no space before `:`: {ln:?}");
        }
    }

    #[test]
    fn data_block_empty_data_returns_none() {
        let mut entry = sample_entry();
        entry.data = Some(json!({}));
        assert!(format_data_block(&entry).is_none());
    }

    #[test]
    fn data_block_strings_quoted_numbers_bare() {
        let mut entry = sample_entry();
        entry.data = Some(json!({"k": 5, "label": "hi"}));
        let block = format_data_block(&entry).unwrap();
        assert!(block.contains("k: 5"), "{block}");
        assert!(block.contains("label: \"hi\""), "{block}");
    }

    #[test]
    fn data_block_unit_annotated_renders_quoted() {
        let mut entry = sample_entry();
        entry.data = Some(json!({"weight": {"v": 75.2, "u": "kg"}}));
        let block = format_data_block(&entry).unwrap();
        assert!(block.contains("weight: \"75.2kg\""), "{block}");
    }

    fn sample_entry() -> Entry {
        use datum_core::entry::Verb;
        use datum_core::time::DatumTime;
        Entry::new(
            "f",
            Verb::Point,
            None,
            None,
            Some(DatumTime::now()),
            None,
            "d",
        )
    }

    // ─── da-gvj1 corpus coverage ─────────────────────────────────────────────
    //
    // The bead's locked-rule corpus (sections A–N). Tests target the
    // observable invariants of each rule rather than character-by-character
    // strings, so chip-color hashing changes don't ripple through every
    // assertion.

    // E ── key=value chip color hash (state second line)
    #[test]
    fn state_second_line_chip_color_hashed_by_key_eq_value() {
        // `.format=audio` and `.format=video` must paint with different
        // colours because their `key=value` hashes differ — rule 5.
        let _scope = color_scope();
        let audio_chip = render_value_chip("format", &json!("audio"), None);
        let video_chip = render_value_chip("format", &json!("video"), None);
        // Strip ANSI to compare; the rendered text body is identical, but
        // the bg codes differ.
        let audio_ansi: String = audio_chip.chars().take_while(|c| *c != '"').collect();
        let video_ansi: String = video_chip.chars().take_while(|c| *c != '"').collect();
        assert_ne!(
            audio_ansi, video_ansi,
            "format=audio vs format=video must hash differently"
        );
    }

    // da-bjuv ── `.id` chip on the state second line must use the same
    // hash input as the main-line `{id}` chip; the previous `id="bob"`
    // form pulled the surrounding quotes into the hash and produced a
    // different colour from the main chip's `state_rgb("bob")`.
    #[test]
    fn id_chip_hash_matches_main_render_id_hash() {
        use super::super::colors::{state_rgb, structured_state_rgb};
        let _scope = color_scope();
        let chip = render_value_chip("id", &json!("bob"), None);
        let expected_bg = state_rgb("bob", None).bg();
        let main_bg = structured_state_rgb(
            json!({"id": "bob", "format": "audio"}).as_object().unwrap(),
            None,
        )
        .bg();
        assert!(
            chip.starts_with(&expected_bg),
            "second-line .id chip must hash by bare id: bg={expected_bg:?} chip={chip:?}"
        );
        assert_eq!(
            expected_bg, main_bg,
            "second-line .id chip and main {{id}} chip must share bg"
        );
    }

    // K ── value type rendering
    #[test]
    fn value_strings_quoted_numbers_bare_bools_bare_null_text() {
        assert_eq!(render_value_body(&json!("happy")), "\"happy\"");
        assert_eq!(render_value_body(&json!(75.2)), "75.2");
        assert_eq!(render_value_body(&json!(7)), "7");
        assert_eq!(render_value_body(&json!(true)), "true");
        assert_eq!(render_value_body(&json!(false)), "false");
        assert_eq!(render_value_body(&json!(null)), "null");
    }

    // K ── unit-annotated leaf — surfaces as quoted concatenation
    #[test]
    fn value_unit_annotated_renders_quoted_concatenated() {
        let v = json!({"v": 75.2, "u": "kg"});
        assert_eq!(render_value_body(&v), "\"75.2kg\"");
    }

    // I ── multiline alignment: keys right-aligned, `:` aligned
    #[test]
    fn structured_multiline_keys_right_aligned() {
        let _scope = no_color_scope();
        // Long values force multiline.
        let v = json!({
            "id": "bob",
            "title": "long enough to push the block past 65 characters easily for sure",
            "format": "audio",
            "recorded": true,
        });
        let line = format_state_doc_line(&v, "call", None).unwrap();
        // Each line must contain a `:` and no space before it.
        let colon_cols: Vec<usize> = line
            .lines()
            .map(|l| l.find(':').expect("every line has a colon"))
            .collect();
        assert!(
            colon_cols.windows(2).all(|w| w[0] == w[1]),
            "colons must share a column: {colon_cols:?} for {line:?}"
        );
        for ln in line.lines() {
            let idx = ln.find(':').unwrap();
            assert_ne!(ln.as_bytes()[idx - 1], b' ', "no space before `:`: {ln:?}");
        }
    }

    // J ── braces: opener on first content line, closer trails last value
    #[test]
    fn structured_multiline_opens_on_first_content_line_closes_after_last_value() {
        let _scope = no_color_scope();
        let v = json!({
            "id": "bob",
            "title": "long enough to push the block past 65 characters easily for sure",
        });
        let line = format_state_doc_line(&v, "call", None).unwrap();
        let first = line.lines().next().unwrap();
        let last = line.lines().last().unwrap();
        assert!(
            first.starts_with("{ "),
            "opener not on first line: {first:?}"
        );
        assert!(
            last.trim_end().ends_with(" }"),
            "closer not trailing last value: {last:?}"
        );
        // No dedicated line for `{` or `}`.
        assert!(
            !line.lines().any(|l| l.trim() == "{" || l.trim() == "}"),
            "no bare brace lines: {line}"
        );
    }

    // N ── empty/id-only structured: no second line needed
    //
    // Rule N edge case: `{id: "x"}` with nothing else SHOULD still show a
    // second line carrying `.id` only (rule 4 keeps id). We treat "id only
    // and no other fields" as still printing the second line; the chip on
    // the main line already shows the id, but having the second line
    // confirms the structure shape consistently. This test pins the chosen
    // behaviour: id-only structured states **do** print the second line.
    #[test]
    fn id_only_structured_state_still_renders_second_line() {
        let _scope = no_color_scope();
        let v = json!({"id": "x"});
        let line = format_state_doc_line(&v, "f", None);
        assert!(line.is_some(), "id-only structured: second line present");
    }

    // G ── parallel inline-multiline decision is per-state
    #[test]
    fn parallel_states_inline_vs_multiline_decided_independently() {
        let _scope = no_color_scope();
        let v = json!([
            {"id": "joe"},
            {
                "id": "bob",
                "title": "long enough to push the block past 65 characters easily for sure",
                "format": "audio",
                "recorded": true,
            },
        ]);
        let line = format_state_doc_line(&v, "call", None).unwrap();
        // The first inner block ("joe") is short → inline `{ .id: "joe" }`.
        assert!(
            line.contains("{ .id: \"joe\" }"),
            "short block stays inline: {line}"
        );
        // The second is long → multi-line presence: the rendered output
        // has more than 2 lines (1 for joe + N for bob).
        assert!(
            line.lines().count() > 2,
            "long block goes multiline: {line}"
        );
    }

    // H ── data block: single-key always one line
    #[test]
    fn data_block_single_key_always_one_line() {
        let mut entry = sample_entry();
        entry.data = Some(json!({"k": "v"}));
        let block = format_data_block(&entry).unwrap();
        assert_eq!(block.lines().count(), 1);
    }

    // F (doc inline) — inside the inline form, separator is `, ` (with space).
    #[test]
    fn doc_inline_list_uses_comma_with_space() {
        let _scope = no_color_scope();
        let v = json!({"id": "joe", "format": "audio"});
        let line = format_state_doc_line(&v, "call", None).unwrap();
        // Inline form: `{ .id: "joe", .format: "audio" }` — note `, ` between pairs.
        assert!(
            line.contains(", "),
            "inline list uses `, ` separator: {line:?}"
        );
    }
}
