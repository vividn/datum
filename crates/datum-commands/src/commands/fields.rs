//! `datum fields` — every field the database knows: the ones logged and the
//! ones declared, each with its spec and the states it has been logged with.
//!
//! What a front end needs to offer a field without the user typing it
//! (datum.day's widget bar, da-g8o8): the spec says what a press means
//! where one is declared, and the logged states are the evidence where one
//! is not.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use datum_core::db::Db;
use datum_core::shape::{is_ignored_state, TRACKED_STATE_VALUE};
use serde_json::{json, Value as JsonValue};

use crate::args::{FieldsArgs, ShowFlag};
use crate::render::colors::{field_hex, state_rgb};
use crate::render::format_grid;

pub fn fields(args: FieldsArgs, db: &Db) -> Result<()> {
    let listing = compute_fields(db)?;
    if args.show == ShowFlag::None {
        return Ok(());
    }
    if args.show == ShowFlag::Json {
        println!("{}", serde_json::to_string_pretty(&listing)?);
        return Ok(());
    }
    print!("{}", format_listing(&listing, args.include_ignored));
    Ok(())
}

/// The plain listing (da-1lnf): a `field` / `states` header over one row
/// per field, so the second column reads as what it is — the set of states
/// the field has been logged with, not its current one.
///
/// Ignored fields are left out unless `include_ignored`; when one is shown
/// a third `ignored` column says which. `--json` never comes through here:
/// it carries every field with its `ignored` flag, which is what the web
/// widget bar reads.
pub fn format_listing(listing: &JsonValue, include_ignored: bool) -> String {
    let ignored = |row: &JsonValue| row["ignored"] == json!(true);
    let shown: Vec<&JsonValue> = listing
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| include_ignored || !ignored(row))
        .collect();
    let mark = shown.iter().any(|row| ignored(row));
    let mut headers = vec!["field", "states"];
    if mark {
        headers.push("ignored");
    }
    let rows: Vec<Vec<String>> = shown
        .iter()
        .map(|row| {
            let states: Vec<String> = row["states"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|s| s.as_str().map_or_else(|| s.to_string(), str::to_string))
                .collect();
            let mut cells = vec![
                row["field"].as_str().unwrap_or("").to_string(),
                states.join(" "),
            ];
            if mark {
                cells.push(if ignored(row) { "true" } else { "" }.to_string());
            }
            cells
        })
        .collect();
    format_grid(&headers, &rows)
}

/// `[{"field", "spec", "states", "color", "state_colors"}]`, alphabetical by
/// field. `color` and `state_colors` are the RESOLVED colours (spec override
/// → md5 fallback, ANSI-256 legacy numbers included) that
/// `crates/datum-commands/src/render/colors.rs` paints the CLI with — so a
/// caller like the web widget bar never has to re-derive them (da-yiyj).
/// `state_colors` covers every state the field has been logged with plus
/// every `allowed_states` entry, since a not-yet-logged allowed state still
/// gets a button. `ignored` is the field's latest track/ignore status
/// ([`is_ignored_state`], da-d08c) — whether a spec exists has no say in it.
pub fn compute_fields(db: &Db) -> Result<JsonValue> {
    let mut by_field: BTreeMap<String, Vec<JsonValue>> = db
        .distinct_fields()?
        .into_iter()
        .map(|f| (f, Vec::new()))
        .collect();
    for (field, state) in db.distinct_field_states()? {
        let entry = by_field.entry(field).or_default();
        // da-5ifp: TRACKED and IGNORED are statuses, not states a field
        // can be switched to.
        if state != TRACKED_STATE_VALUE && !is_ignored_state(&state) {
            entry.push(state);
        }
    }
    let registry = db.load_registry()?;
    for field in registry.fields.keys() {
        by_field.entry(field.clone()).or_default();
    }
    let names: Vec<&str> = by_field.keys().map(String::as_str).collect();
    let latest = db.batch_state_at(&names, chrono::Utc::now())?;
    Ok(JsonValue::Array(
        by_field
            .iter()
            .map(|(field, states)| {
                let ignored = latest.get(field).is_some_and(is_ignored_state);
                let spec = registry.fields.get(field);
                let color = field_hex(field, spec);
                let mut state_names: BTreeSet<&str> =
                    states.iter().filter_map(JsonValue::as_str).collect();
                if let Some(allowed) = spec.and_then(|s| s.allowed_states.as_deref()) {
                    state_names.extend(allowed.iter().map(String::as_str));
                }
                let state_colors: JsonValue = state_names
                    .into_iter()
                    .map(|s| (s.to_string(), json!(state_rgb(s, spec).hex())))
                    .collect::<serde_json::Map<_, _>>()
                    .into();
                json!({
                    "field": field,
                    "spec": spec,
                    "states": states,
                    "color": color,
                    "state_colors": state_colors,
                    "ignored": ignored,
                })
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::action::Action;
    use datum_core::entry::Verb;
    use datum_core::field_spec::FieldSpec;
    use datum_core::test_util::{entry_builder, fresh_db};

    /// da-1lnf: the plain listing leads with a header, so the second
    /// column reads as the SET of states the field has been logged with.
    #[test]
    fn listing_leads_with_a_header() {
        let listing = json!([
            {"field": "work", "states": [false, true], "ignored": false},
            {"field": "x", "states": [], "ignored": false},
        ]);
        let out = format_listing(&listing, false);
        let mut lines = out.lines();
        assert_eq!(lines.next(), Some("field  states"));
        assert_eq!(lines.next(), Some("work   false true"));
        assert_eq!(lines.next(), Some("x"));
        assert_eq!(lines.next(), None);
    }

    /// da-1lnf: an ignored field is left out of the plain listing, and
    /// comes back — marked by an `ignored` column — when asked for.
    #[test]
    fn ignored_fields_are_left_out_unless_asked_for() {
        let listing = json!([
            {"field": "old", "states": [], "ignored": true},
            {"field": "work", "states": [false, true], "ignored": false},
        ]);
        let plain = format_listing(&listing, false);
        assert!(!plain.contains("old"), "ignored field leaked:\n{plain}");
        assert!(!plain.contains("ignored"), "stray column:\n{plain}");

        let all = format_listing(&listing, true);
        let mut lines = all.lines();
        assert_eq!(lines.next(), Some("field  states      ignored"));
        assert_eq!(lines.next(), Some("old                true"));
        assert_eq!(lines.next(), Some("work   false true"));
    }

    /// da-dhtp: the columns are laid out with the table's own width
    /// function, so a wide-glyph field name does not push the column.
    #[test]
    fn wide_glyphs_do_not_push_the_column() {
        let listing = json!([
            {"field": "日本語", "states": ["a"], "ignored": false},
            {"field": "abcdef", "states": ["b"], "ignored": false},
        ]);
        let out = format_listing(&listing, false);
        let lines: Vec<&str> = out.lines().collect();
        // Both names are six columns wide, so both rows end at the same
        // visible column — unless the layout counted 日本語's nine BYTES.
        assert_eq!(
            crate::render::visible_width(lines[1]),
            crate::render::visible_width(lines[2]),
            "state column moved:\n{out}"
        );
    }

    #[test]
    fn lists_logged_and_declared_fields_with_their_states() {
        let db = fresh_db();
        for state in ["sad", "happy", "sad"] {
            entry_builder("mood")
                .verb(Verb::Switch)
                .state(json!(state))
                .insert(&db);
        }
        entry_builder("coffee").insert(&db);
        db.save_field_spec(&FieldSpec::new("sleep", Action::Start))
            .unwrap();

        let listing = compute_fields(&db).unwrap();
        let fields: Vec<&str> = listing
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["field"].as_str().unwrap())
            .collect();
        assert_eq!(fields, ["coffee", "mood", "sleep"]);
        assert_eq!(listing[1]["spec"], JsonValue::Null);
        assert_eq!(listing[1]["states"], json!(["happy", "sad"]));
        assert_eq!(listing[2]["spec"]["default_verb"], "start");
        assert_eq!(listing[2]["states"], json!([]));
    }

    #[test]
    fn colourless_field_resolves_to_md5() {
        let db = fresh_db();
        entry_builder("coffee").insert(&db);

        let listing = compute_fields(&db).unwrap();
        assert_eq!(listing[0]["field"], "coffee");
        assert_eq!(listing[0]["color"], field_hex("coffee", None));
    }

    #[test]
    fn spec_colour_wins_over_md5() {
        let db = fresh_db();
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.color = Some("#ff6600".to_string());
        db.save_field_spec(&spec).unwrap();

        let listing = compute_fields(&db).unwrap();
        assert_eq!(listing[0]["color"], "#ff6600");
    }

    #[test]
    fn colourless_state_resolves_to_md5_of_state_not_field() {
        let db = fresh_db();
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.color = Some("#ff6600".to_string());
        db.save_field_spec(&spec).unwrap();
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("happy"))
            .insert(&db);

        let listing = compute_fields(&db).unwrap();
        assert_eq!(
            listing[0]["state_colors"]["happy"],
            state_rgb("happy", None).hex(),
            "a colourless state hashes its own name, not the field's colour"
        );
        assert_ne!(listing[0]["state_colors"]["happy"], listing[0]["color"]);
    }

    #[test]
    fn legacy_ansi256_number_resolves() {
        let db = fresh_db();
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.color = Some("196".to_string());
        db.save_field_spec(&spec).unwrap();

        let listing = compute_fields(&db).unwrap();
        assert_eq!(listing[0]["color"], field_hex("mood", Some(&spec)));
    }

    #[test]
    fn allowed_states_get_resolved_colours_even_unlogged() {
        let db = fresh_db();
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.allowed_states = Some(vec!["happy".to_string(), "sad".to_string()]);
        db.save_field_spec(&spec).unwrap();

        let listing = compute_fields(&db).unwrap();
        assert_eq!(
            listing[0]["state_colors"]["sad"],
            state_rgb("sad", None).hex()
        );
    }
}
