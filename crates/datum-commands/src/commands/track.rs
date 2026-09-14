//! `datum track <field>` — enable per-field tracking (opt-in registry entry).

use anyhow::Result;
use datum_core::{
    action::Action,
    db::Db,
    entry::{Entry, Verb},
    field_spec::FieldSpec,
    shape::{is_inactive_state, TRACKED_STATE_VALUE},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::args::TrackArgs;
use crate::config::device_id;

pub fn track(args: TrackArgs, db: &Db) -> Result<()> {
    // da-2sr: tracking lives in the entry log, as a switch entry. da-5ifp:
    // a field that holds a state is tracked `false`; an occur field has no
    // off state to hold, so it gets the TRACKED sentinel. Either one is not
    // IGNORED, which is what un-ignores a field. Color and per-state-color
    // overrides still live on FieldSpec, so we update the spec only when the
    // user actually passed those arguments.
    let spec = db.load_spec(&args.field)?;
    let holds_state = match &spec {
        Some(s) => matches!(s.default_verb, Action::Start | Action::Switch),
        None => db
            .distinct_field_states()?
            .iter()
            .any(|(f, s)| *f == args.field && !is_inactive_state(Some(s))),
    };
    let state = if holds_state {
        JsonValue::Bool(false)
    } else {
        JsonValue::String(TRACKED_STATE_VALUE.to_string())
    };
    let device = device_id()?;
    let entry = Entry::new_legacy(
        &args.field,
        Verb::Switch,
        Some(state),
        None,
        Some(DatumTime::now()),
        None,
        device,
    );
    db.insert_entry(&entry)?;

    if args.color.is_some() || !args.state_colors.is_empty() || !args.tags.is_empty() {
        let mut spec = match spec {
            Some(s) => s,
            None => FieldSpec::new(&args.field, Action::Occur),
        };
        if let Some(c) = &args.color {
            spec.color = Some(c.clone());
        }
        for sc in &args.state_colors {
            match sc.split_once('=') {
                Some((state, color)) => {
                    spec.state_colors
                        .insert(state.to_string(), color.to_string());
                }
                None => anyhow::bail!("Invalid --state format: expected STATE=COLOR, got '{sc}'"),
            }
        }
        // da-a7g: `--tag <name>` is repeatable and idempotent — each flag adds
        // the tag if absent. To replace the full set, use `datum spec set`.
        for tag in &args.tags {
            spec.add_tag(tag);
        }
        db.save_field_spec(&spec)?;
    }
    println!("✓ Tracking '{}'", args.field);
    Ok(())
}
