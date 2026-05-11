//! `datum spec` — field-spec management (set, show, list, aliases).

use anyhow::{Context, Result};
use datum_core::{
    action::Action,
    db::{Db, IGNORED_STATE_VALUE},
    entry::{Entry, Verb},
    field_spec::FieldSpec,
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::args::{SpecCommand, SpecSetArgs};
use crate::config::device_id;

pub fn spec(sub: SpecCommand, db: &Db) -> Result<()> {
    match sub {
        SpecCommand::Show { field } => match db.load_spec(&field)? {
            Some(spec) => println!("{}", serde_json::to_string_pretty(&spec)?),
            None => println!("No spec defined for field '{field}'"),
        },
        SpecCommand::Set(args) => spec_set(*args, db)?,
        SpecCommand::List { tag } => {
            if let Some(tag) = tag {
                let specs = db.list_specs_by_tag(&tag)?;
                if specs.is_empty() {
                    println!("No field specs tagged '{tag}'.");
                } else {
                    for spec in &specs {
                        println!("  {}", spec.field);
                    }
                }
            } else {
                let reg = db.load_registry()?;
                if reg.fields.is_empty() {
                    println!("No field specs defined.");
                } else {
                    let mut names: Vec<&String> = reg.fields.keys().collect();
                    names.sort();
                    for name in names {
                        println!("  {name}");
                    }
                }
            }
        }
        SpecCommand::Remove { field } => {
            db.delete_field_spec(&field)?;
            println!("✓ Spec removed for '{field}' (derived aliases dropped)");
        }
    }
    Ok(())
}

fn spec_set(args: SpecSetArgs, db: &Db) -> Result<()> {
    // --json replaces the entire spec; individual flags merge into existing.
    let mut spec = if let Some(json) = &args.json {
        let mut s: FieldSpec =
            serde_json::from_str(json).with_context(|| "Invalid field spec JSON")?;
        s.field = args.field.clone();
        s
    } else {
        db.load_spec(&args.field)?
            .unwrap_or_else(|| FieldSpec::new(&args.field, Action::Occur))
    };

    if let Some(dv) = &args.default_verb {
        spec.default_verb = dv
            .parse::<Action>()
            .map_err(|e| anyhow::anyhow!("Invalid --default-verb: {e}"))?;
    }
    if !args.states.is_empty() {
        spec.allowed_states = Some(args.states.clone());
    }
    if let Some(c) = &args.color {
        spec.color = Some(c.clone());
    }
    if let Some(t) = &args.id_template {
        spec.id_template = Some(t.clone());
    }
    if let Some(d) = &args.description {
        spec.description = Some(d.clone());
    }
    if let Some(u) = &args.unit {
        spec.unit = Some(u.clone());
    }
    for sc in &args.state_colors {
        match sc.split_once('=') {
            Some((state, color)) => {
                spec.state_colors
                    .insert(state.to_string(), color.to_string());
            }
            None => anyhow::bail!("Invalid --state-color format: expected STATE=COLOR, got '{sc}'"),
        }
    }
    // da-2sr: --track / --ignore on `spec set` write switch entries rather
    // than mutating the spec. The flags are kept on the parser surface for
    // backwards compatibility; they no longer touch the spec.
    if args.track {
        let device = device_id()?;
        let entry = Entry::new_legacy(
            &args.field,
            Verb::Switch,
            None,
            None,
            None,
            Some(DatumTime::now()),
            None,
            device,
        );
        db.insert_entry(&entry)?;
    }
    if args.ignore {
        let device = device_id()?;
        let entry = Entry::new_legacy(
            &args.field,
            Verb::Switch,
            Some(JsonValue::String(IGNORED_STATE_VALUE.to_string())),
            None,
            None,
            Some(DatumTime::now()),
            None,
            device,
        );
        db.insert_entry(&entry)?;
    }
    if !args.id_keys.is_empty() {
        spec.id_keys = args.id_keys.clone();
    }
    if !args.data_keys.is_empty() {
        spec.data_keys = args.data_keys.clone();
    }
    if args.no_autoalias {
        spec.no_autoalias = true;
    }
    if args.y.is_some() {
        spec.presentation.y = args.y;
    }
    if args.height.is_some() {
        spec.presentation.height = args.height;
    }
    if args.z_index.is_some() {
        spec.presentation.z_index = args.z_index;
    }
    // da-a7g: `--tag <name>` repeats replace the spec's full tag set
    // (mirrors how `--state <name>` replaces `allowed_states`). To clear
    // tags, re-set the spec from JSON with `tags: []`.
    if !args.tags.is_empty() {
        spec.tags = args.tags.clone();
    }

    spec.validate_default_verb()
        .map_err(|e| anyhow::anyhow!(e))?;

    // Always regenerate derived aliases from the current shape.
    spec.regenerate_derived_aliases();

    let report = db.save_field_spec_with_report(&spec)?;
    println!("✓ Spec saved for '{}'", spec.field);
    for conflict in &report.skipped {
        match &conflict.reason {
            datum_core::db::AliasConflictReason::UserAlias => eprintln!(
                "  ⚠ auto-alias '{}' skipped — user alias with the same name already exists",
                conflict.name
            ),
            datum_core::db::AliasConflictReason::OwnedByField(owner) => eprintln!(
                "  ⚠ auto-alias '{}' skipped — owned by field '{}'",
                conflict.name, owner
            ),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::db::Db;

    #[test]
    fn cmd_spec_set_flag_flow_creates_derived_aliases() {
        let db = Db::open_in_memory().unwrap();

        // spec set mood --default-verb switch --state happy --state sad \
        //               --color green --id-template "%field%:%state%"
        let args = SpecSetArgs {
            field: "mood".to_string(),
            json: None,
            default_verb: Some("switch".to_string()),
            states: vec!["happy".to_string(), "sad".to_string()],
            color: Some("green".to_string()),
            state_colors: vec!["happy=00ff00".to_string()],
            id_template: Some("%field%:%state%".to_string()),
            description: None,
            unit: None,
            track: true,
            ignore: false,
            id_keys: vec![],
            data_keys: vec![],
            no_autoalias: false,
            y: None,
            height: None,
            z_index: None,
            tags: vec![],
        };
        spec_set(args, &db).unwrap();

        let spec = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(spec.id_template.as_deref(), Some("%field%:%state%"));
        assert_eq!(spec.color.as_deref(), Some("green"));
        let states = spec.allowed_states.as_ref().unwrap();
        assert!(states.contains(&"happy".to_string()) && states.contains(&"sad".to_string()));
        // da-2sr: --track on `spec set` writes a switch entry; the spec
        // itself no longer carries tracked/ignored bits.
        let mood_tracked = db.get_tracked_fields().unwrap();
        let mood_row = mood_tracked.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood_row.ignored);

        // Derived aliases landed in the aliases table (hot-path lookup).
        let happy = db.lookup_alias("happy").unwrap().unwrap();
        assert_eq!(happy.expands_to, "switch mood happy");
        let sad = db.lookup_alias("sad").unwrap().unwrap();
        assert_eq!(sad.expands_to, "switch mood sad");

        // Dropping "sad" from the states list drops its alias.
        let args2 = SpecSetArgs {
            field: "mood".to_string(),
            json: None,
            default_verb: None,
            states: vec!["happy".to_string()],
            color: None,
            state_colors: vec![],
            id_template: None,
            description: None,
            unit: None,
            track: false,
            ignore: false,
            id_keys: vec![],
            data_keys: vec![],
            no_autoalias: false,
            y: None,
            height: None,
            z_index: None,
            tags: vec![],
        };
        spec_set(args2, &db).unwrap();
        assert!(db.lookup_alias("sad").unwrap().is_none());
        assert!(db.lookup_alias("happy").unwrap().is_some());
    }
}
