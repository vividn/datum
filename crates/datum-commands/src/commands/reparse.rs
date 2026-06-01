//! `datum reparse <hid>...` — replay named entries' stored `raw_input`
//! through the current parser, diffing the result against the
//! persisted columns.
//!
//! Closes the persisted-intent loop (da-g5uk): the insert path stamps
//! `raw_input`, `alias_chain`, and `parser_version` on every row; this
//! command consumes them so a parser-bug fix can retroactively repair
//! already-stored rows. Replay is hermetic to the alias state at write
//! time — `alias_chain` is rehydrated as the resolver, never the live
//! `aliases` table.
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

use std::collections::HashMap;

use anyhow::Result;
use datum_core::{
    action::Action,
    db::Db,
    dur::Dur,
    entry::{Entry, CURRENT_PARSER_VERSION},
    ids::resolve_quickid_single,
};
use datum_parse::alias::{expand_argv, ResolvedAlias, Resolver};
use datum_parse::parse_argv;
use serde_json::Value as JsonValue;

use crate::args::{Command, ReparseArgs};
use crate::commands::log_core::{
    expand_array_states, extract_reserved_columns, extract_switch_trailing_reserved,
    parse_log_args, validate_switch_state_token,
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

/// Owned alias resolver built from a stored `alias_chain` JSON column.
/// Replay must be hermetic to the alias state at write time, so we
/// reconstruct the resolver from the row, never read the live aliases
/// table.
struct ChainResolver {
    map: HashMap<String, ResolvedAlias>,
}

impl Resolver for ChainResolver {
    fn lookup(&self, name: &str) -> Option<ResolvedAlias> {
        self.map.get(name).cloned()
    }
}

fn build_resolver_from_chain(chain: Option<&JsonValue>) -> ChainResolver {
    use datum_parse::alias::shell_split;

    let mut map: HashMap<String, ResolvedAlias> = HashMap::new();
    let Some(JsonValue::Array(steps)) = chain else {
        return ChainResolver { map };
    };
    for step in steps {
        let Some(name) = step.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(expansion) = step.get("expansion").and_then(|v| v.as_str()) else {
            continue;
        };
        // First-write-wins: a chain may record the same name twice if
        // the user re-ran a command, but the shape is identical (same
        // expansion text), so collisions are benign.
        map.entry(name.to_string())
            .or_insert_with(|| ResolvedAlias {
                expands_to: expansion.to_string(),
                tokens: shell_split(expansion),
            });
    }
    ChainResolver { map }
}

/// Replay one entry's `raw_input` through the current parser and project
/// the result onto the parser-derivable shape. Returns `Ok(None)` for
/// rows whose verb doesn't write entries (so there's nothing to compare
/// against) or whose `raw_input` is empty (legacy rows / migration
/// imports without provenance).
fn replay_shape(entry: &Entry) -> Result<Option<ReparsedShape>> {
    use datum_parse::alias::shell_split;

    if entry.raw_input.is_empty() {
        return Ok(None);
    }

    let mut argv: Vec<String> = vec!["datum".to_string()];
    argv.extend(shell_split(&entry.raw_input));

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
        // Non-log verbs don't produce entries — their replay has nothing
        // to compare against.
        _ => return Ok(None),
    };

    // Mirror the per-action pre-pass from `log_entry` so the parsed
    // bundle matches what the insert path would have produced.
    if matches!(action, Action::Switch) {
        extract_switch_trailing_reserved(&mut log_args)?;
        for tok in &log_args.rest {
            if !tok.contains('=') {
                validate_switch_state_token(tok)?;
            }
        }
    }

    // No clean occur-time anchor for reparse — relative-offset values on
    // `*_time` / `*_date` keys fall through as raw strings rather than
    // being re-resolved against whatever "now" happens to be.
    let parsed = parse_log_args(&log_args, action, None)?;
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
    // gets `Dur::Moment` (da-ju71.2 lock).
    let mut dur = reserved.dur;
    if matches!(action, Action::Occur) && dur.is_none() {
        dur = Some(Dur::Moment);
    }

    Ok(Some(ReparsedShape {
        field: parsed.field,
        state,
        dur,
        following_state: reserved.following_state,
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
    updated.value = shape.state.clone();
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
    // AmbiguousPrefix error instead. Empty tokens (trailing comma) drop.
    let tokens = args
        .hids
        .iter()
        .flat_map(|raw| raw.split(','))
        .map(str::trim)
        .filter(|t| !t.is_empty());

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
