//! `datum reparse <hid>...` — replay named entries' stored `raw_input`
//! through the current parser, diffing the result against the
//! persisted columns.
//!
//! Closes the persisted-intent loop (da-g5uk): the insert path stamps
//! `raw_input`, `alias_chain`, and `parser_version` on every row; this
//! command consumes them so a parser-bug fix can retroactively repair
//! already-stored rows. Replay is hermetic to the alias state at write
//! time — `alias_chain` is rehydrated as the resolver, never the live
//! aliases — and to the spec: the keys a field's spec bound ride in the
//! chain as a `spec_keys` step (da-zv2jr), never the live spec. A row
//! logged before da-zv2jr carries no such step, so a line whose
//! positionals bound to spec keys cannot replay: reparse reports drift
//! there rather than reconstruct the binding.
//!
//! Per da-bkak: reparse operates ONLY on entries the user explicitly
//! names by hid (or quickId / slug — anything `resolve_quickid_single`
//! accepts). Walking all entries is out of scope: long-standing data
//! validated by the user is sacrosanct and is never bulk-rewritten.
//!
//! Replay covers the parser-derivable shape (`field`, `state`, `dur`,
//! `following_state`, `data`) for the log verbs (Occur/Start/Stop/
//! Switch/Add). Other verbs don't write rows, so they have nothing to
//! diff against.

use anyhow::Result;
use datum_core::{
    action::Action,
    db::Db,
    dur::{Dur, DurSpan},
    entry::{Entry, CURRENT_PARSER_VERSION},
    ids::resolve_quickid_single,
    time::DatumTime,
};
use datum_parse::alias::expand_argv;
use datum_parse::input::TimeArgs;
use datum_parse::parse_argv;
use serde_json::Value as JsonValue;

use crate::args::{Command, ReparseArgs};
use crate::commands::helpers::build_resolver_from_chain;
use crate::commands::log_core::{
    bind_spec_words, build_time_from_flags, change_command, compound_field,
    datum_value_to_slug_string, expand_array_states, extract_reserved_columns, lift_time_keys,
    parse_log_args, positional_id, prepare_log_args, take_positional_id,
    validate_switch_state_token, LiftedTimes,
};

/// Parser-derivable Entry shape — what the parser alone determines from
/// `raw_input` + `alias_chain`. Fields that depend on time / device /
/// DB lookups (`occur_time`, `client_id`, slug derivation against the
/// spec table, etc.) are deliberately out of scope: they aren't
/// parser-state and shouldn't drift from a parser-version bump.
#[derive(Debug, Clone)]
struct ReparsedShape {
    field: String,
    state: Option<JsonValue>,
    dur: Option<Dur>,
    following_state: Option<JsonValue>,
    data: Option<JsonValue>,
}

/// Replay one entry's `raw_input` through the current parser and project
/// the result onto the parser-derivable shape. Returns `Ok(None)` for
/// rows whose verb doesn't write entries (so there's nothing to compare
/// against) or whose `raw_input` is empty (legacy rows / migration
/// imports without provenance).
fn replay_shape(entry: &Entry) -> Result<Option<ReparsedShape>> {
    use datum_parse::alias::shell_split;

    let Some(raw) = entry.raw_input.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    let mut argv: Vec<String> = vec!["datum".to_string()];
    argv.extend(shell_split(raw));
    // A global `--db` would shadow the alias name at argv[1], as on the
    // insert (da-hfi3); replay has no use for it (da-s1hot).
    let (argv, _db_flag) = crate::config::split_off_global_db_flag(argv);

    let resolver = build_resolver_from_chain(entry.alias_chain.as_ref());
    let (expanded, _chain) =
        expand_argv(argv, &resolver).map_err(|e| anyhow::anyhow!("expand_argv failed: {e}"))?;
    let cli = parse_argv(&expanded).map_err(|e| anyhow::anyhow!("parse_argv failed: {e}"))?;

    let (mut action, mut log_args) = match cli.command {
        Command::Occur(a) => (Action::Occur, a),
        Command::Start(a) => (Action::Start, a),
        Command::Stop(a) => (Action::Stop, a),
        Command::Switch(a) => (Action::Switch, a),
        Command::Add(a) => (Action::Add, a),
        Command::Note(a) => (Action::Note, a),
        // Non-log verbs don't produce entries — their replay has nothing
        // to compare against.
        _ => return Ok(None),
    };

    // The keys the field's spec bound at log time, as the entry recorded
    // them (da-zv2jr) — never the spec as it stands now.
    let bound = entry.alias_chain.as_ref().and_then(JsonValue::as_array);
    if let Some(words) = bound.and_then(|steps| steps.iter().find_map(|s| s.get("spec_keys"))) {
        bind_spec_words(&mut log_args, serde_json::from_value(words.clone())?);
    }
    if let Some(types) = bound.and_then(|steps| steps.iter().find_map(|s| s.get("key_types"))) {
        log_args.key_types = serde_json::from_value(types.clone())?;
    }

    // An add's id word was its id when that is the id it stored (da-znksm).
    if positional_id(action, &log_args).map(|i| &log_args.rest[i]) == entry.id_structure.as_ref() {
        take_positional_id(action, &mut log_args)?;
    }

    // Mirror the per-action pre-pass from `log_entry` so the parsed
    // bundle matches what the insert path would have produced (da-ooe2:
    // declares the trailing `dur=` slot and peels the switch `,`-chain;
    // a note's bare words are its comment, da-fohr0).
    prepare_log_args(action, &mut log_args)?;
    if matches!(action, Action::Switch) {
        for tok in &log_args.rest {
            if !tok.contains('=') {
                validate_switch_state_token(tok)?;
            }
        }
    }

    // `start_time=` / `end_time=` place the block rather than land in
    // `data` (da-mjwq), lifted as `log_entry` lifts them (da-0rx53).
    let lifted = if matches!(
        action,
        Action::Add | Action::Note | Action::Start | Action::Switch | Action::Occur
    ) {
        lift_time_keys(&mut log_args.rest)
    } else {
        LiftedTimes::default()
    };
    // The lifted anchor is the line's `-t`, as on the insert, so a `-y`
    // beside it is a clock time and not a date-only one (da-98u27).
    if let Some((_, raw)) = lifted.anchor() {
        log_args.time.time = Some(raw.to_string());
    }

    // Relative `*_time` / `*_date` keys (`next_date=+7`) anchor where the
    // insert anchored them (da-jbzmb): the row's own local time, or for a
    // timeless row its create time in the local zone (da-rp2rq). Never the
    // replay's "now", which would move the value every time it is asked.
    // `start_time=S` plus a duration stored S + dur and anchored at S.
    let base = match &entry.occur_time {
        Some(t) => match (
            &lifted.start,
            &lifted.end,
            entry.dur.as_ref().map(|d| &d.span),
        ) {
            (Some(_), None, Some(DurSpan::Of(d))) => {
                let mut s = DatumTime {
                    utc: t.utc - *d,
                    ..t.clone()
                };
                s.refresh_offset()?;
                s
            }
            _ => t.clone(),
        },
        None => {
            let mut t = DatumTime::now();
            t.utc = entry.create_time;
            t.refresh_offset()?;
            t
        }
    };
    let parsed = parse_log_args(&log_args, action, Some(base.clone()))?;
    let mut extra_data = parsed.extra_data;
    let mut reserved = extract_reserved_columns(&mut extra_data)?;
    action = change_command(
        action,
        parsed.command,
        &mut reserved.state,
        &mut log_args.last_state,
    );

    // `end_time=` alone on `start` / `switch`, with no duration, ran as
    // `end` — a switch's state became the `-l` assertion (da-mjwq).
    if lifted.end.is_some()
        && lifted.start.is_none()
        && reserved.dur.is_none()
        && log_args.duration.duration.is_none()
        && matches!(action, Action::Start | Action::Switch)
    {
        if matches!(action, Action::Switch) {
            log_args.last_state = Some(String::new());
        }
        reserved.state = None;
        action = Action::Stop;
    }

    let value = if matches!(action, Action::Switch) {
        expand_array_states(parsed.value)?
    } else {
        parsed.value
    };

    // State precedence (from log_entry):
    //   1. reserved `state=` wins.
    //   2. positional value.
    //   3. action-driven default: Stop → false, Start/Switch → true,
    //      others → None.
    let mut state = reserved.state.or(value);
    if state.is_none() {
        state = match action {
            Action::Stop => Some(JsonValue::Bool(false)),
            Action::Start | Action::Switch => Some(JsonValue::Bool(true)),
            _ => None,
        };
    }

    // Dur: reserved `dur=` wins; Action::Occur with no explicit dur
    // gets `Dur::moment()` (da-ju71.2 lock).
    let mut dur = reserved.dur;
    // The pair is the block between them: its span is the dur, both ends
    // resolved as the insert resolved them, against its own clock.
    if let (Some(s), Some(e)) = (&lifted.start, &lifted.end) {
        let at = |raw: &String| {
            let time = TimeArgs {
                time: Some(raw.clone()),
                ..log_args.time.clone()
            };
            build_time_from_flags(&time, entry.create_time)
        };
        let secs = (at(e)?.utc - at(s)?.utc).num_seconds().max(0);
        dur = Some(Dur::of(chrono::Duration::seconds(secs)));
    }
    if matches!(action, Action::Occur) && dur.is_none() {
        dur = Some(Dur::moment());
    }

    // `-l` on `end` records the closed block's state as `data.last_state`,
    // which `log_entry` looks up rather than the line spelling it. Keep
    // the stored one, as the declaration below (da-pdk6q).
    if matches!(action, Action::Stop) && log_args.last_state.is_some() {
        if let Some(prior) = entry.data.as_ref().and_then(|d| d.get("last_state")) {
            let mut obj = match extra_data.take() {
                Some(JsonValue::Object(m)) => m,
                _ => serde_json::Map::new(),
            };
            obj.insert("last_state".to_string(), prior.clone());
            extra_data = Some(JsonValue::Object(obj));
        }
    }

    // Before da-uf6it a bare `id=<v>` was the entry's identity, its id
    // template, and left `data` (#1100 made it data). A row it named so —
    // no stored `id`, `<v>` its template or, interpolated, its slug's body —
    // replays it as it was meant.
    let slug_body = entry.slug.strip_prefix(&format!("{}:", entry.field));
    if let Some(JsonValue::Object(m)) = &mut extra_data {
        let id = m.get("id").map(|v| datum_value_to_slug_string(Some(v)));
        if entry.data.as_ref().is_none_or(|d| d.get("id").is_none())
            && id.is_some()
            && [entry.id_structure.as_deref(), slug_body].contains(&id.as_deref())
        {
            m.remove("id");
        }
    }
    if extra_data
        .as_ref()
        .and_then(JsonValue::as_object)
        .is_some_and(|m| m.is_empty())
    {
        extra_data = None;
    }

    Ok(Some(ReparsedShape {
        // The flex-field verbs land the field in `reserved.field`, as
        // `log_entry` reads it (da-5t94), and an `-F` value builds on it.
        field: compound_field(
            reserved.field.unwrap_or(parsed.field),
            &log_args.field_keys,
            &parsed.field_values,
        )?,
        state,
        dur,
        // The first row of a field carries its declaration as a trailing
        // switch (da-gvcen), put there by a spec lookup in `log_entry`
        // rather than by the parser. A line that spelled no
        // `following_state=` keeps a stored one that IS that declaration:
        // dropping it would un-ignore nothing and report every field's
        // first row as drifted (da-9p48y).
        following_state: reserved.following_state.or_else(|| {
            let declared =
                datum_core::track::declared_state(datum_core::track::verb_for_action(action));
            entry.following_state.clone().filter(|fs| *fs == declared)
        }),
        data: extra_data,
    }))
}

/// One field-level drift between the replayed shape and the stored row.
struct Delta {
    field_name: &'static str,
    old: String,
    new: String,
}

fn fmt_json(v: &Option<JsonValue>) -> String {
    match v {
        Some(j) => j.to_string(),
        None => "<unset>".to_string(),
    }
}

fn fmt_dur(d: &Option<Dur>) -> String {
    match d {
        Some(d) => d.to_storage_string(),
        None => "<unset>".to_string(),
    }
}

fn diff_shape(shape: &ReparsedShape, entry: &Entry) -> Vec<Delta> {
    let mut out: Vec<Delta> = Vec::new();
    if shape.field != entry.field {
        out.push(Delta {
            field_name: "field",
            old: entry.field.clone(),
            new: shape.field.clone(),
        });
    }
    if shape.state != entry.state {
        out.push(Delta {
            field_name: "state",
            old: fmt_json(&entry.state),
            new: fmt_json(&shape.state),
        });
    }
    if shape.dur != entry.dur {
        out.push(Delta {
            field_name: "dur",
            old: fmt_dur(&entry.dur),
            new: fmt_dur(&shape.dur),
        });
    }
    if shape.following_state != entry.following_state {
        out.push(Delta {
            field_name: "following_state",
            old: fmt_json(&entry.following_state),
            new: fmt_json(&shape.following_state),
        });
    }
    if shape.data != entry.data {
        out.push(Delta {
            field_name: "data",
            old: fmt_json(&entry.data),
            new: fmt_json(&shape.data),
        });
    }
    out
}

/// The `data` `reparse --fix` would write to `entry`: `None` when its line
/// is missing or no longer parses, which is when the command cannot help.
pub(crate) fn replayed_data(entry: &Entry) -> Option<JsonValue> {
    replay_shape(entry).ok().flatten()?.data
}

fn drift(entry: &Entry, deltas: &[Delta]) -> String {
    let mut out = format!(
        "{}  ({} delta{})",
        entry.slug,
        deltas.len(),
        if deltas.len() == 1 { "" } else { "s" }
    );
    for d in deltas {
        out += &format!("\n  {}: {} → {}", d.field_name, d.old, d.new);
    }
    out
}

fn apply_fix(entry: &Entry, shape: &ReparsedShape, db: &Db) -> Result<()> {
    let mut updated = entry.clone();
    updated.field = shape.field.clone();
    updated.state = shape.state.clone();
    updated.dur = shape.dur.clone();
    updated.following_state = shape.following_state.clone();
    updated.data = shape.data.clone();
    updated.parser_version = CURRENT_PARSER_VERSION;
    updated.modify_time = chrono::Utc::now();
    updated.content_hash = updated.expected_content_hash();
    db.save_existing_entry(&updated)?;
    Ok(())
}

pub fn reparse(args: ReparseArgs, db: &Db) -> Result<()> {
    if args.hids.is_empty() {
        anyhow::bail!(
            "reparse requires at least one <hid> (or slug / quickId). \
             Walking all entries is intentionally not supported — reparse \
             is opt-in per row. See `datum reparse --help`."
        );
    }

    let mut scanned = 0usize;
    let mut drifted = 0usize;
    let mut fixed = 0usize;
    let mut refused = 0usize;

    // Comma-aware tokenisation (da-obes): `reparse a,b` resolves the
    // same as `reparse a b`, matching get/del/update/retime. Unlike
    // those, reparse stays on `resolve_quickid_single` per token rather
    // than the batch resolver — its da-bkak contract forbids walking all
    // entries, and the batch path would let a wildcard (`*`) expand to
    // the whole store. Single resolution keeps a multi-match wildcard an
    // AmbiguousPrefix error instead. Empty tokens (trailing comma) drop,
    // and a slug holding a comma is one token ([`id_list`], da-7nv8r).
    //
    // [`id_list`]: crate::commands::helpers::id_list
    let tokens = args
        .hids
        .iter()
        .flat_map(|raw| crate::commands::helpers::id_list(db, raw));

    // Every fix, `_LAST` included, lands before anything prints: a reader
    // closing the pipe ends the process at the print (da-ki5j3).
    let mut lines: Vec<String> = Vec::new();
    let walk = || -> Result<()> {
        for token in tokens {
            let entry = resolve_quickid_single(db, token)?;

            scanned += 1;

            let shape = match replay_shape(&entry) {
                Ok(Some(s)) => s,
                // Non-log verb or no provenance — nothing to compare.
                Ok(None) => {
                    lines.push(format!("{}: no replayable raw_input — skipped", entry.slug));
                    continue;
                }
                // A line today's parser refuses (`wake_time=soonish`, da-b2f8o)
                // is reported and passed over, so the rows after it are read.
                Err(e) if e.downcast_ref::<datum_parse::ParseError>().is_some() => {
                    lines.push(format!("{}: no longer parses: {e}", entry.slug));
                    refused += 1;
                    continue;
                }
                Err(e) => return Err(e),
            };

            let deltas = diff_shape(&shape, &entry);
            if deltas.is_empty() {
                lines.push(format!("{}: clean", entry.slug));
                continue;
            }

            drifted += 1;
            lines.push(drift(&entry, &deltas));

            if args.fix {
                apply_fix(&entry, &shape, db)?;
                fixed += 1;
            }
        }
        Ok(())
    };
    let walked = walk();
    crate::cmd::stamp_batch(db);
    for line in &lines {
        println!("{line}");
    }
    walked?;

    let refused = match refused {
        0 => String::new(),
        n => format!(", {n} no longer parse{}", if n == 1 { "s" } else { "" }),
    };
    if args.fix {
        println!("\nreparse: {scanned} scanned, {drifted} drifted, {fixed} fixed{refused}.");
    } else {
        println!("\nreparse: {scanned} scanned, {drifted} would change{refused}.");
    }

    Ok(())
}
