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
        SpecCommand::Show { field, text } => match db.load_spec(&field)? {
            Some(spec) if text => render_spec_text(&spec),
            Some(spec) => println!("{}", serde_json::to_string_pretty(&spec)?),
            None => println!("No spec defined for field '{field}'"),
        },
        SpecCommand::AddState {
            field,
            name,
            color,
            description,
        } => spec_add_state(&field, &name, color.as_deref(), description.as_deref(), db)?,
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
    // Clear convention for STATE=VALUE flags (--state-color, --state-description,
    // and any future sibling): an empty VALUE removes the entry for that state.
    for sc in &args.state_colors {
        match sc.split_once('=') {
            Some((state, "")) => {
                spec.state_colors.remove(state);
            }
            Some((state, color)) => {
                spec.state_colors
                    .insert(state.to_string(), color.to_string());
            }
            None => anyhow::bail!("Invalid --state-color format: expected STATE=COLOR, got '{sc}'"),
        }
    }
    // da-zwqk: per-state descriptions, same STATE=TEXT shape as colors.
    // Empty TEXT clears the description for that state.
    for sd in &args.state_descriptions {
        match sd.split_once('=') {
            Some((state, "")) => {
                spec.state_descriptions.remove(state);
            }
            Some((state, text)) => {
                spec.state_descriptions
                    .insert(state.to_string(), text.to_string());
            }
            None => {
                anyhow::bail!("Invalid --state-description format: expected STATE=TEXT, got '{sd}'")
            }
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
    if let Some(s) = args.strict {
        spec.strict = s;
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
    warn_alias_conflicts(&report);
    Ok(())
}

/// Print the `⚠ auto-alias … skipped` lines from a save report to stderr.
/// Shared by every spec-mutating command that regenerates aliases.
fn warn_alias_conflicts(report: &datum_core::db::SaveSpecReport) {
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
}

/// `datum spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION]` — merge one
/// state into a field spec without disturbing the others (da-t9ps).
///
/// The incremental, footgun-free counterpart to `spec set --state`, which
/// replaces the whole `allowed_states` list. Appends NAME to the states
/// (idempotent — re-adding an existing state just refreshes its color and
/// description) and, when supplied, records the per-state color and
/// description alongside it. Creates the spec (Occur default) if absent.
fn spec_add_state(
    field: &str,
    name: &str,
    color: Option<&str>,
    description: Option<&str>,
    db: &Db,
) -> Result<()> {
    let mut spec = db
        .load_spec(field)?
        .unwrap_or_else(|| FieldSpec::new(field, Action::Occur));

    let states = spec.allowed_states.get_or_insert_with(Vec::new);
    let existed = states.iter().any(|s| s == name);
    if !existed {
        states.push(name.to_string());
    }
    if let Some(c) = color {
        spec.state_colors.insert(name.to_string(), c.to_string());
    }
    if let Some(d) = description {
        spec.state_descriptions
            .insert(name.to_string(), d.to_string());
    }

    spec.validate_default_verb()
        .map_err(|e| anyhow::anyhow!(e))?;
    spec.regenerate_derived_aliases();

    let report = db.save_field_spec_with_report(&spec)?;
    let verb = if existed { "updated" } else { "added" };
    println!("✓ State '{name}' {verb} on '{field}'");
    warn_alias_conflicts(&report);
    Ok(())
}

/// Human-readable summary of a FieldSpec — `datum spec show <FIELD> --text`.
///
/// JSON output (`spec show <FIELD>` without `--text`) is precise and good
/// for programmatic use, but a typed-out reading of the spec's intent reads
/// faster for humans. This format prints field-level metadata first, then
/// per-state lines listing color + description side by side (da-nhzo).
fn render_spec_text(spec: &FieldSpec) {
    println!("{}", spec.field);
    println!("  default_verb: {:?}", spec.default_verb);
    if let Some(d) = &spec.description {
        println!("  description: {d}");
    }
    if let Some(u) = &spec.unit {
        println!("  unit: {u}");
    }
    if let Some(c) = &spec.color {
        println!("  color: {c}");
    }
    if let Some(states) = &spec.allowed_states {
        println!("  states:");
        for state in states {
            let color = spec
                .state_colors
                .get(state)
                .map(String::as_str)
                .unwrap_or("(auto)");
            let desc = spec
                .state_descriptions
                .get(state)
                .map(String::as_str)
                .unwrap_or("");
            if desc.is_empty() {
                println!("    {state:<16} {color}");
            } else {
                println!("    {state:<16} {color}  — {desc}");
            }
        }
    } else if !spec.state_descriptions.is_empty() {
        // Descriptions without an allowed_states list — list them anyway.
        println!("  state_descriptions:");
        let mut names: Vec<&String> = spec.state_descriptions.keys().collect();
        names.sort();
        for name in names {
            let desc = &spec.state_descriptions[name];
            println!("    {name:<16} {desc}");
        }
    }
    if !spec.id_keys.is_empty() {
        println!("  id_keys: {:?}", spec.id_keys);
    }
    if !spec.data_keys.is_empty() {
        println!("  data_keys: {:?}", spec.data_keys);
    }
    if !spec.tags.is_empty() {
        println!("  tags: {:?}", spec.tags);
    }
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
            state_descriptions: vec![],
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
            strict: None,
        };
        spec_set(args, &db).unwrap();

        let spec = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(spec.id_template.as_deref(), Some("%field%:%state%"));
        assert_eq!(spec.color.as_deref(), Some("green"));
        let states = spec.allowed_states.as_ref().unwrap();
        assert!(states.contains(&"happy".to_string()) && states.contains(&"sad".to_string()));
        // da-2sr: --track on `spec set` writes a switch entry; the spec
        // itself no longer carries tracked/ignored bits.
        let mood_tracked = db.get_tracked_fields(chrono::Utc::now()).unwrap();
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
            state_descriptions: vec![],
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
            strict: None,
        };
        spec_set(args2, &db).unwrap();
        assert!(db.lookup_alias("sad").unwrap().is_none());
        assert!(db.lookup_alias("happy").unwrap().is_some());
    }

    #[test]
    fn cmd_spec_add_state_merges_without_dropping_others() {
        let db = Db::open_in_memory().unwrap();

        // A live multi-state field.
        spec_set(
            SpecSetArgs {
                field: "mood".to_string(),
                default_verb: Some("switch".to_string()),
                states: vec!["happy".to_string(), "sad".to_string()],
                state_descriptions: vec!["happy=joyful".to_string()],
                ..Default::default()
            },
            &db,
        )
        .unwrap();

        // Add one state with color + description — the others must survive.
        spec_add_state("mood", "meh", Some("yellow"), Some("so-so"), &db).unwrap();

        let spec = db.load_spec("mood").unwrap().unwrap();
        let states = spec.allowed_states.as_ref().unwrap();
        assert_eq!(states, &["happy", "sad", "meh"]);
        assert_eq!(
            spec.state_colors.get("meh").map(String::as_str),
            Some("yellow")
        );
        assert_eq!(
            spec.state_descriptions.get("meh").map(String::as_str),
            Some("so-so")
        );
        // Pre-existing description is untouched.
        assert_eq!(
            spec.state_descriptions.get("happy").map(String::as_str),
            Some("joyful")
        );
        // The new state's derived alias landed.
        let meh = db.lookup_alias("meh").unwrap().unwrap();
        assert_eq!(meh.expands_to, "switch mood meh");
    }

    #[test]
    fn cmd_spec_add_state_creates_spec_and_reupdates_idempotently() {
        let db = Db::open_in_memory().unwrap();

        // No spec yet — add-state creates an Occur-default spec.
        spec_add_state("energy", "high", Some("green"), Some("wired"), &db).unwrap();
        let spec = db.load_spec("energy").unwrap().unwrap();
        assert_eq!(
            spec.allowed_states.as_deref(),
            Some(&["high".to_string()][..])
        );

        // Re-adding refreshes the color but does not duplicate the state or
        // clobber a description that wasn't re-supplied.
        spec_add_state("energy", "high", Some("lime"), None, &db).unwrap();
        let spec = db.load_spec("energy").unwrap().unwrap();
        assert_eq!(
            spec.allowed_states.as_deref(),
            Some(&["high".to_string()][..])
        );
        assert_eq!(
            spec.state_colors.get("high").map(String::as_str),
            Some("lime")
        );
        assert_eq!(
            spec.state_descriptions.get("high").map(String::as_str),
            Some("wired")
        );
    }

    #[test]
    fn cmd_spec_set_empty_state_value_clears_entry() {
        let db = Db::open_in_memory().unwrap();

        // Set a color and a description for the same state...
        spec_set(
            SpecSetArgs {
                field: "mood".to_string(),
                state_colors: vec!["happy=00ff00".to_string()],
                state_descriptions: vec!["happy=joyful".to_string()],
                ..Default::default()
            },
            &db,
        )
        .unwrap();
        let spec = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(
            spec.state_colors.get("happy").map(String::as_str),
            Some("00ff00")
        );
        assert_eq!(
            spec.state_descriptions.get("happy").map(String::as_str),
            Some("joyful")
        );

        // ...then clear both via the empty-VALUE convention.
        spec_set(
            SpecSetArgs {
                field: "mood".to_string(),
                state_colors: vec!["happy=".to_string()],
                state_descriptions: vec!["happy=".to_string()],
                ..Default::default()
            },
            &db,
        )
        .unwrap();
        let spec = db.load_spec("mood").unwrap().unwrap();
        assert!(!spec.state_colors.contains_key("happy"));
        assert!(!spec.state_descriptions.contains_key("happy"));
    }
}
