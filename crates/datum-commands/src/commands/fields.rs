//! `datum fields` — every field the database knows: the ones logged and the
//! ones declared, each with its spec and the states it has been logged with.
//!
//! What a front end needs to offer a field without the user typing it
//! (datum.day's widget bar, da-g8o8): the spec says what a press means
//! where one is declared, and the logged states are the evidence where one
//! is not.

use std::collections::BTreeMap;

use anyhow::Result;
use datum_core::db::Db;
use serde_json::{json, Value as JsonValue};

use crate::args::FieldsArgs;

pub fn fields(args: FieldsArgs, db: &Db) -> Result<()> {
    let listing = compute_fields(db)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&listing)?);
        return Ok(());
    }
    for row in listing.as_array().into_iter().flatten() {
        let states: Vec<String> = row["states"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| s.as_str().map_or_else(|| s.to_string(), str::to_string))
            .collect();
        println!(
            "{}\t{}",
            row["field"].as_str().unwrap_or(""),
            states.join(" ")
        );
    }
    Ok(())
}

/// `[{"field", "spec", "states"}]`, alphabetical by field.
pub fn compute_fields(db: &Db) -> Result<JsonValue> {
    let mut by_field: BTreeMap<String, Vec<JsonValue>> = db
        .distinct_fields()?
        .into_iter()
        .map(|f| (f, Vec::new()))
        .collect();
    for (field, state) in db.distinct_field_states()? {
        by_field.entry(field).or_default().push(state);
    }
    let registry = db.load_registry()?;
    for field in registry.fields.keys() {
        by_field.entry(field.clone()).or_default();
    }
    Ok(JsonValue::Array(
        by_field
            .into_iter()
            .map(|(field, states)| {
                let spec = registry.fields.get(&field);
                json!({ "field": field, "spec": spec, "states": states })
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
}
