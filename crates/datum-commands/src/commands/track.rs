//! `datum track <field>` — declare a field: what a press of it means,
//! written down (da-c8mh), plus the TRACK row that says tracking began,
//! for a field that is IGNORED or has no entries (da-6fvys).

use anyhow::{Context, Result};
use datum_core::{
    action::Action,
    db::Db,
    field_spec::{
        parse_key_decl, FieldSpec, KeyKind, SubfieldSpec, SubfieldType, VERB_CHOICES_TEXT,
    },
    kit_color::resolve,
    shape::is_ignored_state,
    time::DatumTime,
    track::track_entry,
};

use crate::args::TrackArgs;
use crate::config::client_id;

/// The verb `field` should be declared with, given an explicit `--verb`
/// and whatever the database already knows.
///
/// The order is: what the user said, then what the spec already says,
/// then what the log implies — read ONCE and written down, so the
/// decision stops depending on the log. A field with none of the three
/// has nothing to infer from, and the user is asked (da-c8mh).
pub fn resolve_verb(field: &str, requested: Option<&str>, db: &Db) -> Result<Action> {
    if let Some(raw) = requested {
        let verb: Action = raw.parse().map_err(|_| {
            anyhow::anyhow!("invalid --verb '{raw}' — must be one of: {VERB_CHOICES_TEXT}")
        })?;
        let probe = FieldSpec::new(field, verb);
        probe.validate_verb().map_err(|e| anyhow::anyhow!(e))?;
        return Ok(verb);
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
    Ok(match db.state_at(field, now.utc)? {
        Some(state) => is_ignored_state(&state),
        None => db.entries_for_field(field, Some(1))?.is_empty(),
    })
}

pub fn track(args: TrackArgs, db: &Db) -> Result<()> {
    // da-2sr: tracking lives in the entry log, as a switch entry. da-c8mh:
    // the spec is written in the same breath, so the field's verb is a
    // fact about the field rather than a re-reading of its log.
    let verb = resolve_verb(&args.field, args.verb.as_deref(), db)?;
    let now = DatumTime::now();
    let begins_tracking = begins_tracking(db, &args.field, &now)?;
    let mut spec = db
        .load_spec(&args.field)?
        .unwrap_or_else(|| FieldSpec::new(&args.field, verb));
    spec.verb = verb;
    if let Some(c) = &args.color {
        spec.color = Some(c)
            .filter(|c| !c.is_empty())
            .map(|c| resolve(c, &args.field))
            .transpose()
            .context("--color")?;
    }
    // `--state` carries both halves of a switch field's declaration
    // (da-94q1): a bare NAME is a state it may be switched to, `NAME=COLOR`
    // paints one. So `track mood --verb switch --state good --state bad`
    // declares the field whole, in the one command the web's field form
    // runs.
    let mut states: Vec<String> = Vec::new();
    for sc in &args.states {
        match sc.split_once('=') {
            Some((state, color)) => {
                let color = resolve(color, state).context("--state")?;
                spec.state_colors.insert(state.to_string(), color);
            }
            None => {
                super::spec::check_state_name(&args.field, sc)?;
                states.push(sc.clone());
            }
        }
    }
    if !states.is_empty() {
        spec.allowed_states = Some(states);
    }
    // da-a7g: `--tag <name>` is repeatable and idempotent — each flag adds
    // the tag if absent. `datum spec F --tags` sets the whole set.
    for tag in &args.tags {
        spec.add_tag(tag);
    }
    // da-h119q: `--key NAME[=DEFAULT]` / `--required-key`, so a field is
    // declared with the keys a person cares about in the one command. A
    // default is JSON when it parses and text when it does not, as the
    // web's key editor stores an `any` key's. Naming a key the spec already
    // has sets its kind and keeps the rest — its type, and whether it is in
    // the slug, which is what an entry IS.
    let keys = args.keys.iter().map(|k| (k, false));
    for (key, required) in keys.chain(args.required_keys.iter().map(|k| (k, true))) {
        let (name, kind) = parse_key_decl(key);
        let json = |d: &str| serde_json::from_str(d).unwrap_or_else(|_| d.into());
        let default = match &kind {
            KeyKind::Default(d) => Some(json(d)),
            _ => None,
        };
        anyhow::ensure!(!name.trim().is_empty(), "--key needs a name: '{key}'");
        spec.forget_key(name);
        let sub = spec
            .subfields
            .entry(name.to_string())
            .or_insert(SubfieldSpec {
                required,
                value_type: SubfieldType::Any,
                in_slug: false,
                default: None,
            });
        sub.required = required;
        sub.default = default;
    }
    // da-ulc0v: the alias-style tail. `-k` / `-K` add a slot to the ordered
    // lists (a name already in that list takes the new kind in its place),
    // and a bare `KEY=VALUE` assigns (`KEY=` removes one, as `spec
    // --assign`). A key lives in one place: each declaration takes its name
    // out of the others.
    super::spec::add_slots(&mut spec, &args.data_keys, false);
    super::spec::add_slots(&mut spec, &args.id_keys, true);
    for (k, v) in args.assign.iter().filter_map(|kv| kv.split_once('=')) {
        if v.is_empty() {
            spec.assign.remove(k);
        } else {
            spec.forget_key(k);
            spec.assign.insert(k.to_string(), v.to_string());
        }
    }
    // da-fu29p: `-R KEY`, as a log line's own, so a field whose entries are
    // «whatever words follow» is declared in the one command too; `-S`
    // joins them, and `-C` makes them one comment (da-zqnt3; the parser
    // refuses `-S` alone and `-C` with `-R`). Either names the whole route,
    // replacing the spec's.
    if args.remainder.is_some() || args.comment_remainder {
        spec.remainder = args.remainder.clone().filter(|r| !r.is_empty());
        spec.string_remainder = args.string_remainder && spec.remainder.is_some();
        spec.comment_remainder = args.comment_remainder;
    }
    spec.regenerate_derived_aliases();
    let client = client_id()?;
    db.set_client_id(&client);
    let entry = begins_tracking.then(|| track_entry(&args.field, verb, now, &client));
    db.save_field_spec_with(&spec, entry.as_ref())?;
    if let Some(entry) = entry {
        let _ = datum_core::last_docs::set_last_docs(db, std::slice::from_ref(&entry.hid));
    }
    println!("✓ Tracking '{}' ({verb})", args.field);
    Ok(())
}
