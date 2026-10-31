//! `datum track <field>` / `datum ignore <field>` — `setup F … --track` /
//! `--ignore` (da-ijf3p), and the TRACK row that says tracking began, for a
//! field that is IGNORED or has no entries (da-6fvys).

use anyhow::Result;
use datum_core::{
    action::Action,
    db::{Db, IGNORED_STATE_VALUE},
    entry::{Entry, Verb},
    field_spec::{verb_choice, VERB_CHOICES_TEXT, VERB_NAMES_TEXT},
    shape::is_ignored_state,
    time::DatumTime,
    timezone_field::TIMEZONE_FIELD,
    track::track_entry,
};

use crate::args::{SpecCommand, SpecSetArgs};
use crate::config::client_id;
use crate::render::{
    self,
    action::{RenderContext, Surface},
    show_action_with_context, Outcome, OutputArgs,
};
use datum_parse::input::Show;

/// `--verb V` read, or refused naming the four and where they are explained.
pub(crate) fn parse_verb(raw: &str) -> Result<Action> {
    verb_choice(raw).ok_or_else(|| {
        anyhow::anyhow!(
            "invalid --verb '{raw}' — must be one of: {VERB_NAMES_TEXT} \
             (`datum help logging` says what each is)"
        )
    })
}

/// The verb `field` should be declared with, given an explicit `--verb`
/// and whatever the database already knows.
///
/// The order is: what the user said, then what the spec already says,
/// then what the log implies — read ONCE and written down, so the
/// decision stops depending on the log. A field with none of the three
/// has nothing to infer from, and the user is asked (da-c8mh).
pub fn resolve_verb(field: &str, requested: Option<&str>, db: &Db) -> Result<Action> {
    // da-yw6o2: reserved, and a switch whatever is asked.
    if field == TIMEZONE_FIELD {
        return match requested.map(parse_verb).transpose()? {
            None | Some(Action::Switch) => Ok(Action::Switch),
            Some(_) => anyhow::bail!("'{field}' is a switch"),
        };
    }
    if let Some(raw) = requested {
        return parse_verb(raw);
    }
    if let Some(spec) = db.load_spec(field)? {
        return Ok(spec.verb);
    }
    db.verb_from_field_log(field)?.ok_or_else(|| {
        anyhow::anyhow!(
            "'{field}' is new — say what a press of it means: --verb {VERB_CHOICES_TEXT}"
        )
    })
}

/// Whether declaring `field` at `now` begins tracking it, and so writes its
/// TRACK row (da-6fvys, vividn, 2026-10-09): «track should only change the
/// state of a field if it is IGNORED or nothing (because no data exists)».
/// A field that is running, ended or already TRACKED gets its spec and no row.
pub(crate) fn begins_tracking(db: &Db, field: &str, now: &DatumTime) -> Result<bool> {
    // da-yw6o2: `timezone` holds what is logged to it, and no TRACKED.
    if field == TIMEZONE_FIELD {
        return Ok(false);
    }
    Ok(match db.state_at(field, now.utc)? {
        Some(state) => is_ignored_state(&state),
        None => db.entries_for_field(field, Some(1))?.is_empty(),
    })
}

/// `track F [CHANGES]` / `ignore F [CHANGES]` (da-ijf3p): `setup F [CHANGES]
/// --track` / `--ignore`, but for `F` alone on a field with a spec, which
/// writes the status only and leaves the spec as it is. `ignore F` alone
/// on a field with none writes its row too, and declares nothing: hiding a
/// field asks for no verb (vremya, 10-16; vividn to rule).
///
/// `F` may be a list, `a,b,c`: one write each, in that order, each echoed
/// as alone (da-klzka). Every name is checked first as a name it declares
/// is, so a list with one refused writes nothing.
pub fn status(args: SpecSetArgs, db: &Db) -> Result<()> {
    let fields: Vec<String> = args.field.split(',').map(String::from).collect();
    for field in &fields {
        check_new(field, &args, db)?;
    }
    for field in fields {
        status_one(
            SpecSetArgs {
                field,
                ..args.clone()
            },
            db,
        )?;
    }
    Ok(())
}

/// What refuses `field` alone, asked before anything of a list is written:
/// a name no entry or spec holds yet is a name being chosen, and `track`
/// declaring a spec asks what `setup` would.
fn check_new(field: &str, args: &SpecSetArgs, db: &Db) -> Result<()> {
    if db.load_spec(field)?.is_some() {
        return Ok(());
    }
    if db.entries_for_field(field, Some(1))?.is_empty() {
        datum_core::slug::validate_field_name(field)?;
        datum_core::slug::validate_new_field_name(field)?;
        if args.track {
            datum_parse::field_name::check_declared(field)?;
        }
    }
    if args.track {
        super::helpers::refuse_hid_named_field(db, field)?;
        if args.from_json.is_none() && super::spec::states_named(args).is_empty() {
            resolve_verb(field, args.verb.as_deref(), db)?;
        }
    }
    Ok(())
}

fn status_one(args: SpecSetArgs, db: &Db) -> Result<()> {
    // Bare: nothing set but the field and the status. A flag `SpecSetArgs`
    // gains is a change by this comparison with no edit here.
    let bare = args.track != args.ignore
        && SpecSetArgs {
            field: args.field.clone(),
            show: args.show,
            track: args.track,
            ignore: args.ignore,
            ..Default::default()
        } == args;
    let spec = db.load_spec(&args.field)?;
    let field = &args.field;
    let now = DatumTime::now();
    let entry = match spec {
        Some(spec) if bare && args.track => begins_tracking(db, field, &now)?
            .then(|| client_id().map(|c| track_entry(field, spec.verb, now, &c)))
            .transpose()?,
        _ if bare && args.ignore => {
            // da-t3quu: a typo's IGNORED row is hidden from `fields`.
            super::helpers::note_unseen_field(db, field, "ignoring")?;
            let ignored = db
                .state_at(field, now.utc)?
                .is_some_and(|s| is_ignored_state(&s));
            (!ignored)
                .then(|| client_id().map(|c| ignored_entry(field, c)))
                .transpose()?
        }
        _ => return super::spec::spec(SpecCommand::Set(Box::new(args)), db),
    };
    // Nothing if it's already in that status, as `setup --track` (da-2pbvd).
    let Some(entry) = entry else {
        let held = match db.entries_for_field(field, Some(1))?.pop() {
            Some(row) => Some(row),
            None => db.spec_entry(field)?,
        };
        match held.filter(|_| args.show == Show::Json) {
            Some(row) => echo(db, &row, Outcome::NoDiff, args.show)?,
            None => println!("no change"),
        }
        return Ok(());
    };
    db.insert_entry(&entry)?;
    let _ = datum_core::last_docs::set_last_docs(db, std::slice::from_ref(&entry.hid));
    echo_status(db, &entry, args.show)
}

/// A status row's echo, as any log's (da-te4id): `TRACK` and the verb's glyph,
/// or `IGNORE`, over the state change into TRACKED or IGNORED.
pub(crate) fn echo_status(db: &Db, row: &Entry, show: Show) -> Result<()> {
    let ignoring = row.state.as_ref().is_some_and(is_ignored_state);
    let action = if ignoring {
        Outcome::Ignore
    } else {
        Outcome::Create
    };
    echo(db, row, action, show)
}

fn echo(db: &Db, row: &Entry, action: Outcome, show: Show) -> Result<()> {
    let ctx = RenderContext::for_entry(db, row, Surface::Action, None)?;
    show_action_with_context(
        row,
        action,
        super::helpers::registry_for(db, &[&row.field]).as_ref(),
        Some(&ctx),
        &OutputArgs {
            show: match show {
                Show::Json => render::Show::Json,
                _ => render::Show::Default,
            },
            format_string: None,
        },
    );
    Ok(())
}

/// The row that ignores `field` from now on.
pub(crate) fn ignored_entry(field: &str, client: String) -> Entry {
    Entry::new_legacy(
        field,
        Verb::Switch,
        Some(serde_json::Value::String(IGNORED_STATE_VALUE.to_string())),
        None,
        Some(DatumTime::now()),
        None,
        client,
    )
}
