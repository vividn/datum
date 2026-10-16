//! `datum states <field>` — the states a field declares, then every state
//! it has been logged with that the spec does not declare, each with how
//! many entries carry it and when it was last used (da-7905t).
//!
//! Tidying a field («did I type `mood happy` and `mood Happy`?») and
//! checking a field for drift are both this one read.

use anyhow::Result;
use datum_core::db::Db;
use datum_core::shape::{is_ignored_state, TRACKED_STATE_VALUE};
use datum_core::time::Zone;
use serde_json::{json, Value as JsonValue};

use crate::args::{ShowFlag, StatesArgs};
use crate::render::colors::state_rgb;
use crate::render::format_grid;

pub fn states(args: StatesArgs, db: &Db) -> Result<()> {
    let listing = compute_states(db, &args.field)?;
    match args.show {
        ShowFlag::None => {}
        ShowFlag::Json => println!("{}", serde_json::to_string_pretty(&listing)?),
        _ => print!("{}", format_states(&listing)),
    }
    Ok(())
}

/// `{"field", "states": [{"state", "declared", "count", "last_used",
/// "color", "description"}]}`: the declared states in the spec's order, then
/// the undeclared ones most used first. `last_used` is the latest
/// `occur_time` (RFC 3339, UTC), null when never used or used only timeless;
/// `color` is the resolved colour the CLI paints the state with.
pub fn compute_states(db: &Db, field: &str) -> Result<JsonValue> {
    let spec = db.load_spec(field)?;
    let mut used: Vec<_> = db
        .field_state_uses(field)?
        .into_iter()
        // TRACKED and IGNORED are statuses, not states (da-5ifp).
        .filter(|(s, ..)| *s != TRACKED_STATE_VALUE && !is_ignored_state(s))
        .collect();
    if spec.is_none() && used.is_empty() {
        anyhow::bail!("states: no field '{field}': nothing logged and no spec");
    }
    let name = |s: &JsonValue| s.as_str().map_or_else(|| s.to_string(), str::to_string);
    let row = |state: JsonValue, declared: bool, use_: Option<(usize, Option<_>)>| {
        let (count, last) = use_.unwrap_or_default();
        let text = name(&state);
        json!({
            "state": state,
            "declared": declared,
            "count": count,
            "last_used": last.map(|t: chrono::DateTime<chrono::Utc>| t.to_rfc3339()),
            "color": state_rgb(&text, spec.as_ref()).hex(),
            "description": spec.as_ref().and_then(|s| s.state_descriptions.get(&text)),
        })
    };
    let mut rows = Vec::new();
    for state in spec.iter().flat_map(|s| s.declared_states()) {
        let found = used.iter().position(|(s, ..)| name(s) == *state);
        let use_ = found.map(|i| used.remove(i)).map(|(_, n, last)| (n, last));
        rows.push(row(json!(state), true, use_));
    }
    for (state, n, last) in used {
        rows.push(row(state, false, Some((n, last))));
    }
    Ok(json!({ "field": field, "states": rows }))
}

/// The table: `state  entries  last used  colour  description`, an
/// undeclared state saying so where a declared one has its description.
pub fn format_states(listing: &JsonValue) -> String {
    let rows: Vec<Vec<String>> = listing["states"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            let last = match (r["count"].as_u64(), r["last_used"].as_str()) {
                (Some(0), _) => "never".to_string(),
                (_, None) => "timeless".to_string(),
                (_, Some(t)) => chrono::DateTime::parse_from_rfc3339(t).map_or_else(
                    |_| t.to_string(),
                    |t| {
                        let local = Zone::host().local_naive(t.to_utc());
                        local.format("%Y-%m-%d %H:%M").to_string()
                    },
                ),
            };
            let description = if r["declared"] == json!(true) {
                r["description"].as_str().unwrap_or("").to_string()
            } else {
                "undeclared".to_string()
            };
            let state = &r["state"];
            vec![
                state
                    .as_str()
                    .map_or_else(|| state.to_string(), str::to_string),
                r["count"].to_string(),
                last,
                r["color"].as_str().unwrap_or("").to_string(),
                description,
            ]
        })
        .collect();
    format_grid(
        &["state", "entries", "last used", "colour", "description"],
        &rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::action::Action;
    use datum_core::entry::Verb;
    use datum_core::field_spec::FieldSpec;
    use datum_core::test_util::{entry_builder, fresh_db};

    /// A declared switch field with an undeclared state typed into it:
    /// the declared ones first, in the spec's order, unused ones honestly
    /// at 0 and never; then the undeclared one, marked.
    #[test]
    fn declared_then_undeclared_with_counts_and_last_use() {
        let db = fresh_db();
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.allowed_states = Some(vec!["sad".into(), "happy".into(), "calm".into()]);
        spec.state_descriptions
            .insert("happy".into(), "joyful".into());
        spec.state_colors.insert("happy".into(), "#00ff00".into());
        db.save_field_spec(&spec).unwrap();
        let t = |h| chrono::DateTime::parse_from_rfc3339(h).unwrap().to_utc();
        for (state, at) in [
            ("happy", "2026-10-01T09:00:00Z"),
            ("Happy", "2026-10-02T09:00:00Z"),
            ("happy", "2026-10-03T09:00:00Z"),
            ("sad", "2026-10-04T09:00:00Z"),
        ] {
            entry_builder("mood")
                .verb(Verb::Switch)
                .state(json!(state))
                .time_start_utc(t(at))
                .insert(&db);
        }

        let listing = compute_states(&db, "mood").unwrap();
        let rows: Vec<(&str, bool, u64, Option<&str>)> = listing["states"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["state"].as_str().unwrap(),
                    r["declared"].as_bool().unwrap(),
                    r["count"].as_u64().unwrap(),
                    r["last_used"].as_str(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("sad", true, 1, Some("2026-10-04T09:00:00+00:00")),
                ("happy", true, 2, Some("2026-10-03T09:00:00+00:00")),
                ("calm", true, 0, None),
                ("Happy", false, 1, Some("2026-10-02T09:00:00+00:00")),
            ]
        );
        assert_eq!(listing["states"][1]["description"], "joyful");
        assert_eq!(listing["states"][1]["color"], "#00ff00");

        let table = format_states(&listing);
        let lines: Vec<&str> = table.lines().map(str::trim_end).collect();
        assert!(lines[0].starts_with("state  entries  last used"), "{table}");
        assert!(lines[2].ends_with("#00ff00  joyful"), "{table}");
        assert!(lines[3].contains("  0        never "), "{table}");
        assert!(lines[4].ends_with("undeclared"), "{table}");
        assert_eq!(lines.len(), 5, "{table}");
    }

    /// No declared states: only what was used, each marked undeclared; a
    /// start/stop field's on and off are `true` and `false`.
    #[test]
    fn a_field_without_declared_states_lists_what_was_used() {
        let db = fresh_db();
        for state in [true, false, true] {
            entry_builder("work").state(json!(state)).insert(&db);
        }
        let listing = compute_states(&db, "work").unwrap();
        let states = &listing["states"];
        assert_eq!(states[0]["state"], json!(true));
        assert_eq!(states[0]["count"], 2);
        assert_eq!(states[1]["state"], json!(false));
        assert_eq!(states.as_array().unwrap().len(), 2);
        assert!(states
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["declared"] == false));
    }

    #[test]
    fn an_unknown_field_is_an_error() {
        let db = fresh_db();
        let err = compute_states(&db, "nope").unwrap_err().to_string();
        assert!(err.contains("no field 'nope'"), "{err}");
    }
}
