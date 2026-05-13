//! `datum track <field>` — enable per-field tracking (opt-in registry entry).

use anyhow::Result;
use datum_core::{
    action::Action,
    db::Db,
    entry::{Entry, Verb},
    field_spec::FieldSpec,
    time::DatumTime,
};

use crate::args::TrackArgs;
use crate::config::device_id;

pub fn track(args: TrackArgs, db: &Db) -> Result<()> {
    // da-2sr: tracking lives in the entry log. `track` writes a switch
    // entry with value=NULL — semantically "tracked, no current state".
    // Any later real switch (`switch mood happy`) supersedes this. Color
    // and per-state-color overrides still live on FieldSpec, so we update
    // the spec only when the user actually passed those arguments.
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

    if args.color.is_some() || !args.state_colors.is_empty() || !args.tags.is_empty() {
        let mut spec = match db.load_spec(&args.field)? {
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
