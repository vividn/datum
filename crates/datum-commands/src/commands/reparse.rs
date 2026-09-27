//! `datum reparse <hid>...` — replay named entries' stored `raw_input`
//! through the current parser, diffing the result against the
//! persisted columns.
//!
//! Closes the persisted-intent loop (da-g5uk): the insert path stamps
//! `raw_input`, `alias_chain`, and `parser_version` on every row; this
//! command consumes them so a parser-bug fix can retroactively repair
//! already-stored rows. Replay is hermetic to the alias state at write
//! time — `alias_chain` is rehydrated as the resolver, never the live
//! aliases.
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
    dur::Dur,
    entry::{Entry, CURRENT_PARSER_VERSION},
    ids::resolve_quickid_single,
    time::DatumTime,
};
use datum_parse::alias::expand_argv;
use datum_parse::parse_argv;
use serde_json::Value as JsonValue;

use crate::args::{Command, ReparseArgs};
use crate::commands::helpers::build_resolver_from_chain;
use crate::commands::log_core::{
    expand_array_states, extract_reserved_columns, parse_log_args, prepare_log_args,
    validate_switch_state_token,
};

/// Parser-derivable Entry shape — what the parser alone determines from
/// `raw_input` + `alias_chain`. Fields that depend on time / device /
/// DB lookups (`occur_time`, `device_id`, slug derivation against the
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

    let resolver = build_resolver_from_chain(entry.alias_chain.as_ref());
    let (expanded, _chain) =
        expand_argv(argv, &resolver).map_err(|e| anyhow::anyhow!("expand_argv failed: {e}"))?;
    let cli = parse_argv(&expanded).map_err(|e| anyhow::anyhow!("parse_argv failed: {e}"))?;

    let (action, mut log_args) = match cli.command {
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

    // Mirror the per-action pre-pass from `log_entry` so the parsed
    // bundle matches what the insert path would have produced (da-ooe2:
    // declares the trailing `dur=` slot and peels the switch `,`-chain).
    prepare_log_args(action, &mut log_args)?;
    if matches!(action, Action::Switch) {
        for tok in &log_args.rest {
            if !tok.contains('=') {
                validate_switch_state_token(tok)?;
            }
        }
    }

    // Relative `*_time` / `*_date` keys (`next_date=+7`) anchor where the
    // insert anchored them (da-jbzmb): the row's own local time, or for a
    // timeless row its create time in the local zone (da-rp2rq). Never the
    // replay's "now", which would move the value every time it is asked.
    let base = match &entry.occur_time {
        Some(t) => t.clone(),
        None => {
            let mut t = DatumTime::now();
            t.utc = entry.create_time;
            t.refresh_offset()?;
            t
        }
    };
    let parsed = parse_log_args(&log_args, action, Some(base.local_time().naive_local()))?;
    let mut extra_data = parsed.extra_data;
    let reserved = extract_reserved_columns(&mut extra_data)?;

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
    if matches!(action, Action::Occur) && dur.is_none() {
        dur = Some(Dur::moment());
    }

    Ok(Some(ReparsedShape {
        // The flex-field verbs land the field in `reserved.field`, as
        // `log_entry` reads it (da-5t94).
        field: reserved.field.unwrap_or(parsed.field),
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

fn print_drift(entry: &Entry, deltas: &[Delta]) {
    println!(
        "{}  ({} delta{})",
        entry.slug,
        deltas.len(),
        if deltas.len() == 1 { "" } else { "s" }
    );
    for d in deltas {
        println!("  {}: {} → {}", d.field_name, d.old, d.new);
    }
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

    for token in tokens {
        let entry = resolve_quickid_single(db, token)?;

        scanned += 1;

        let shape = match replay_shape(&entry)? {
            Some(s) => s,
            // Non-log verb or no provenance — nothing to compare.
            None => {
                println!("{}: no replayable raw_input — skipped", entry.slug);
                continue;
            }
        };

        let deltas = diff_shape(&shape, &entry);
        if deltas.is_empty() {
            println!("{}: clean", entry.slug);
            continue;
        }

        drifted += 1;
        print_drift(&entry, &deltas);

        if args.fix {
            apply_fix(&entry, &shape, db)?;
            fixed += 1;
        }
    }

    if args.fix {
        println!("\nreparse: {scanned} scanned, {drifted} drifted, {fixed} fixed.");
    } else {
        println!("\nreparse: {scanned} scanned, {drifted} would change.");
    }

    Ok(())
}
