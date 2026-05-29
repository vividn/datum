//! Unified update/merge/rekey core.
//!
//! One entry point — [`update_with_strategy`] — implements the three
//! mutation strategies. The `update`, `merge`, and `rekey` verbs are thin
//! CLI wrappers around it. Future batch mutations (S3) iterate over this
//! function rather than reimplementing per-strategy code.

use anyhow::Result;
use datum_core::{db::Db, entry::Entry, time::DatumTime};
use serde_json::Value as JsonValue;

use crate::{
    args::{ConflictStrategyArg, OutputFlagsExt, UpdateArgs},
    // da-5t94 rider: the remainder-bucket sentinel is shared with the log
    // path rather than re-declared here.
    commands::log_core::PRIMARY_VALUE_KEY,
    render::{show_create, show_rename, show_update, OutputArgs},
};

/// Command-level mutation strategy for `update` / `merge` / `rekey`.
///
/// A narrower sibling of [`ConflictStrategy`]: no `Ignore` (it isn't a
/// meaningful choice when the caller has explicitly asked to mutate a row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateStrategy {
    /// Shallow: payload keys replace existing top-level keys; omitted keys
    /// preserved. Empty value (`key=`) drops the key.
    Replace,
    /// Deep: recursive object merge; scalar conflicts → payload wins.
    Merge,
    /// Rename keys inside `data`. Payload is read as a `{oldKey: "newKey"}`
    /// mapping (values must be strings); each existing `data.oldKey` is
    /// renamed to `data.newKey` with its value preserved. After the rename,
    /// the row goes through the same auto-slug-reinterpolation as a normal
    /// update — so renaming a key referenced by `id_structure` /
    /// `field_structure` cascades into the slug/field columns automatically.
    /// Mirrors original datum's `combineData` strategy 'rekey' (utils/rekey.ts).
    Rekey,
}

impl UpdateStrategy {
    pub(crate) fn verb(self) -> &'static str {
        match self {
            UpdateStrategy::Replace => "update",
            UpdateStrategy::Merge => "merge",
            UpdateStrategy::Rekey => "rekey",
        }
    }
}

/// Result of applying [`update_with_strategy`].
///
/// `old_slug` is `Some(_)` iff auto-slug-reinterpolation produced a different
/// slug than the row started with — the row was rekeyed in place (hid
/// preserved). Otherwise `entry.slug` is the unchanged in-place-updated row.
#[derive(Clone, Debug)]
pub struct UpdateOutcome {
    pub entry: Entry,
    pub old_slug: Option<datum_core::ids::Slug>,
}

/// Inputs to [`update_with_strategy`]. Constructed by the verb wrappers
/// (from parsed `UpdateArgs`) and eventually by batch mutation callers
/// that generate one per id.
#[derive(Debug)]
pub struct UpdateRequest<'a> {
    pub id_or_slug: &'a str,
    pub payload_value: Option<JsonValue>,
    pub payload_extra: Option<JsonValue>,
    pub strategy: UpdateStrategy,
    /// Delimiter collapsing empty segments when re-interpolating the slug
    /// (or `field`) from a stored template. Used by every strategy: any
    /// mutation that touches templated data triggers auto-reinterpolation.
    pub slug_delimiter: &'a str,
}

/// Apply one of the three mutation strategies to the row identified by
/// `req.id_or_slug` and return the resulting entry.
///
/// Does not print — callers that need the CLI confirmation line should
/// use [`update_from_args`] or call [`render_outcome`] themselves.
pub fn update_with_strategy(req: UpdateRequest<'_>, db: &Db) -> Result<UpdateOutcome> {
    let existing = crate::commands::helpers::resolve_prefix_or_tombstone(db, req.id_or_slug)?;
    if existing.deleted_at.is_some() {
        anyhow::bail!(
            "Cannot {} tombstoned entry {} — restore it first with `datum undelete`",
            req.strategy.verb(),
            req.id_or_slug,
        );
    }

    match req.strategy {
        UpdateStrategy::Replace => apply_mutation(
            existing,
            req.payload_value,
            req.payload_extra,
            ExtraMergeMode::Shallow,
            req.slug_delimiter,
            db,
        ),
        UpdateStrategy::Merge => apply_mutation(
            existing,
            req.payload_value,
            req.payload_extra,
            ExtraMergeMode::Deep,
            req.slug_delimiter,
            db,
        ),
        UpdateStrategy::Rekey => {
            apply_data_key_rename(existing, req.payload_extra, req.slug_delimiter, db)
        }
    }
}

/// CLI adapter: parse an `UpdateArgs` (with optional `--strategy` override),
/// dispatch [`update_with_strategy`], render the confirmation line.
///
/// Returns the outcome so tests can inspect the resulting entry without
/// a second DB round-trip.
pub fn update_from_args(
    args: UpdateArgs,
    default_strategy: UpdateStrategy,
    db: &Db,
) -> Result<UpdateOutcome> {
    let strategy = args
        .strategy
        .map(strategy_from_arg)
        .transpose()?
        .unwrap_or(default_strategy);

    let UpdatePayload {
        pattern,
        default_last: _,
        value: payload_value,
        extra: payload_extra,
    } = parse_update_payload(&args)?;
    let output = args.output()?;
    let delim = crate::config::resolve_id_delimiter(args.id_delimiter.as_deref());

    let outcome = update_with_strategy(
        UpdateRequest {
            id_or_slug: &pattern,
            payload_value,
            payload_extra,
            strategy,
            slug_delimiter: &delim,
        },
        db,
    )?;

    render_outcome(&outcome, &output, db);
    Ok(outcome)
}

/// Batch variant of [`update_from_args`]: splits the id argument on commas,
/// resolves each token through the S2 quickId resolver (so wildcards expand
/// and hid prefixes work), and applies the same payload to every resolved
/// row.
///
/// Default semantics: non-strict — per-row failures are reported but don't
/// abort the batch; the overall invocation exits non-zero if anything
/// failed. With `--strict`, the first resolution or tombstone error aborts
/// before any write (tombstone pre-scan keeps a later tombstone from
/// partially mutating earlier live siblings).
///
/// `--dry-run` prints what would be mutated and returns without writing.
pub fn update_batch_from_args(
    args: UpdateArgs,
    default_strategy: UpdateStrategy,
    db: &Db,
) -> Result<()> {
    let strategy = args
        .strategy
        .map(strategy_from_arg)
        .transpose()?
        .unwrap_or(default_strategy);

    let payload = parse_update_payload(&args)?;
    let UpdatePayload {
        pattern,
        default_last,
        value: payload_value,
        extra: payload_extra,
    } = payload;
    let (entries, token_errors) = crate::commands::helpers::resolve_batch(
        db,
        &pattern,
        crate::commands::helpers::BatchMode::LiveOrTombstone,
    );

    if entries.is_empty() {
        if let Some((_, err)) = token_errors.into_iter().next() {
            return Err(err);
        }
        anyhow::bail!("no entries specified");
    }

    if args.strict {
        if let Some((_, err)) = token_errors.first() {
            return Err(anyhow::anyhow!("{err}"));
        }
        // Pre-scan tombstones so strict never partially mutates a batch:
        // the tombstone refusal is knowable before any write. Runtime
        // errors (rekey collisions, etc.) still abort mid-loop — those
        // aren't pre-detectable.
        if let Some(tomb) = entries.iter().find(|e| e.deleted_at.is_some()) {
            anyhow::bail!(
                "Cannot {} tombstoned entry {} — restore it first with `datum undelete`",
                strategy.verb(),
                tomb.slug,
            );
        }
    }

    let output = args.output()?;
    let delim = crate::config::resolve_id_delimiter(args.id_delimiter.as_deref());

    if args.dry_run {
        let registry = db.load_registry().ok();
        let now = DatumTime::now();
        let ctxs = crate::commands::render_helpers::build_render_contexts(&entries, db);
        for (i, entry) in entries.iter().enumerate() {
            if entry.deleted_at.is_some() {
                eprintln!(
                    "[dry-run] skip tombstoned: {} (restore first with `datum undelete`)",
                    entry.slug,
                );
                continue;
            }
            crate::render::print_entry(entry, ctxs.get(i), Some(&now), registry.as_ref());
        }
        for (tok, err) in &token_errors {
            eprintln!("[dry-run] resolve '{tok}': {err}");
        }
        let live = entries.iter().filter(|e| e.deleted_at.is_none()).count();
        eprintln!(
            "[dry-run] would {} {} {}",
            strategy.verb(),
            live,
            if live == 1 { "entry" } else { "entries" },
        );
        return Ok(());
    }

    // da-uzxj: prompt before mutating when the user defaulted to
    // `_LAST` AND the pointer is older than the protection window.
    // UpdateArgs has no `--force` yet (follow-up); passing `false`
    // means the prompt always fires under the default+stale branch,
    // and the user confirms with Enter (yes-default).
    if default_last {
        // Capitalize the strategy verb ("Update" / "Merge" / "Rekey")
        // so the prompt reads naturally.
        let verb = strategy.verb();
        let label = {
            let mut s = String::with_capacity(verb.len());
            let mut chars = verb.chars();
            if let Some(c) = chars.next() {
                s.extend(c.to_uppercase());
            }
            s.extend(chars);
            s
        };
        if !crate::commands::helpers::confirm_default_last_if_stale(
            db, None, false, &label, &entries,
        )? {
            println!("Aborted.");
            return Ok(());
        }
    }

    let mut first_err: Option<anyhow::Error> = None;
    let mut touched_hids: Vec<datum_core::ids::Hid> = Vec::new();
    for existing in entries {
        if existing.deleted_at.is_some() {
            let msg = format!(
                "Cannot {} tombstoned entry {} — restore it first with `datum undelete`",
                strategy.verb(),
                existing.slug,
            );
            if args.strict {
                return Err(anyhow::anyhow!(msg));
            }
            eprintln!("{msg}");
            first_err.get_or_insert_with(|| anyhow::anyhow!(msg));
            continue;
        }

        let result = update_with_strategy(
            UpdateRequest {
                id_or_slug: &existing.slug,
                payload_value: payload_value.clone(),
                payload_extra: payload_extra.clone(),
                strategy,
                slug_delimiter: &delim,
            },
            db,
        );

        match result {
            Ok(outcome) => {
                touched_hids.push(outcome.entry.hid.clone());
                render_outcome(&outcome, &output, db);
            }
            Err(e) => {
                if args.strict {
                    return Err(e);
                }
                eprintln!("{e}");
                first_err.get_or_insert(e);
            }
        }
    }

    for (tok, err) in token_errors {
        let msg = format!("{tok}: {err}");
        eprintln!("{msg}");
        first_err.get_or_insert_with(|| anyhow::anyhow!(msg));
    }

    if !touched_hids.is_empty() {
        let _ = datum_core::last_docs::set_last_docs(db, &touched_hids);
    }

    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Print the post-mutation confirmation line matching the old behavior:
/// update/merge → `UPDATE`; rekey → `RENAME` + `CREATE`.
pub fn render_outcome(outcome: &UpdateOutcome, output: &OutputArgs, db: &Db) {
    let registry = db.load_registry().ok();
    match &outcome.old_slug {
        Some(old) => {
            show_rename(old, &outcome.entry.slug, output);
            show_create(&outcome.entry, registry.as_ref(), output);
        }
        None => {
            show_update(&outcome.entry, registry.as_ref(), output);
        }
    }
}

fn strategy_from_arg(a: ConflictStrategyArg) -> Result<UpdateStrategy> {
    match a {
        ConflictStrategyArg::Update => Ok(UpdateStrategy::Replace),
        ConflictStrategyArg::Merge => Ok(UpdateStrategy::Merge),
        ConflictStrategyArg::Rekey => Ok(UpdateStrategy::Rekey),
        ConflictStrategyArg::Ignore => {
            anyhow::bail!("`ignore` is not a meaningful strategy for update/merge/rekey")
        }
    }
}

#[derive(Clone, Copy)]
enum ExtraMergeMode {
    /// Top-level keys from payload replace existing; other keys preserved.
    Shallow,
    /// Recursive object merge; scalar conflicts → payload wins.
    Deep,
}

fn apply_mutation(
    existing: Entry,
    payload_value: Option<JsonValue>,
    payload_extra: Option<JsonValue>,
    mode: ExtraMergeMode,
    slug_delimiter: &str,
    db: &Db,
) -> Result<UpdateOutcome> {
    // da-5ns.3: peel reserved column-bound keys (`state=`, `dur=`,
    // `following_state=`, `occur_time=`) out of the payload and route
    // them to the entry's columns instead of `data`. Unknown keys
    // (including `value=`) stay in `data` (da-53rz). The storage layer's
    // `derive_verb` call recomputes the verb on the resulting (state,
    // dur, fs) triple.
    let mut payload_extra = payload_extra;
    let reserved = crate::commands::log_core::extract_reserved_columns(&mut payload_extra)?;
    // da-5t94: `id=` re-slugs on update. The shared peeler leaves `id`
    // alone (on insert it competes with `-K` slug slots); update has no
    // slug slots, so a bare `id=` here always rewrites the slug body. Peel
    // it locally so it lands on `id_structure` instead of leaking to `data`.
    let id_literal = take_reserved_id(&mut payload_extra);

    let mut updated = existing.clone();
    updated.data = combine_extra(updated.data, payload_extra, mode);
    // da-5t94: `field=` / `id=` re-field and re-slug in place. Setting
    // `updated.field` (or the literal `id_structure`) makes
    // `persist_with_auto_reinterp` re-derive the slug as `<field>:<body>`
    // while preserving hid + timestamps. Previously these keys leaked
    // silently into `data` because update only peeled the other reserved
    // columns. The id literal mirrors insert's `id=` promotion (stored as
    // a literal id_structure so later edits don't re-key the slug).
    if let Some(f) = reserved.field {
        updated.field = f;
    }
    if let Some(id) = id_literal {
        updated.id_structure = Some(id);
    }
    if let Some(v) = payload_value {
        updated.value = Some(v.clone());
        updated.state = Some(v);
    }
    if let Some(s) = reserved.state {
        updated.state = Some(s.clone());
        updated.value = Some(s);
    }
    if let Some(d) = reserved.dur {
        updated.dur = Some(d);
    }
    if let Some(dp) = reserved.dur_precision {
        updated.dur_precision = Some(dp);
    }
    if let Some(fs) = reserved.following_state {
        updated.following_state = Some(fs);
    }
    if let Some(t) = reserved.occur_time {
        // Base any relative offset (`-5m`, `now`) against the existing
        // occur_time when present, falling back to wall-clock now.
        let base = updated.occur_time.clone().unwrap_or_else(DatumTime::now);
        updated.occur_time = Some(crate::commands::log_core::parse_reserved_occur_time(
            &t, &base,
        )?);
    }
    persist_with_auto_reinterp(existing, updated, slug_delimiter, db)
}

/// Rename keys inside `data` per a `{oldKey: "newKey"}` mapping. After the
/// rename, the row goes through the same persistence path as a normal update
/// — including auto-slug-reinterpolation, so a renamed key referenced by
/// `id_structure` cascades into the slug.
fn apply_data_key_rename(
    existing: Entry,
    payload_extra: Option<JsonValue>,
    slug_delimiter: &str,
    db: &Db,
) -> Result<UpdateOutcome> {
    let mapping = match payload_extra {
        Some(JsonValue::Object(m)) if !m.is_empty() => m,
        _ => anyhow::bail!("rekey requires a key-rename mapping like 'oldKey=newKey'"),
    };
    let mut updated = existing.clone();
    if let Some(JsonValue::Object(mut data_map)) = updated.data.take() {
        for (old_key, new_key_v) in mapping {
            let new_key = match new_key_v {
                JsonValue::String(s) if !s.is_empty() => s,
                _ => {
                    anyhow::bail!("rekey mapping value for '{old_key}' must be a non-empty string")
                }
            };
            if let Some(value) = data_map.remove(&old_key) {
                data_map.insert(new_key, value);
            }
        }
        updated.data = if data_map.is_empty() {
            None
        } else {
            Some(JsonValue::Object(data_map))
        };
    }
    persist_with_auto_reinterp(existing, updated, slug_delimiter, db)
}

/// Shared post-mutation path: re-interpolate `field_structure` and
/// `id_structure` against the mutated entry, then persist. If the slug
/// changed, the row is rewritten in place under a new slug (hid preserved)
/// via [`Db::save_existing_entry_with_rekey`]; otherwise the regular
/// in-place update path runs.
///
/// Mirrors original datum's `updateDoc.ts` flow: after combineData, call
/// compileField then assembleId, and rekey the doc if `_id` changed.
///
/// da-5ns.4: implementation moved to
/// [`Db::save_existing_entry_with_auto_reinterp`] so non-update mutators
/// (e.g. `retime`) can reuse the same auto-rekey trigger when they mutate
/// templated columns (`occur_time`, `state`, `dur`, …).
fn persist_with_auto_reinterp(
    existing: Entry,
    updated: Entry,
    slug_delimiter: &str,
    db: &Db,
) -> Result<UpdateOutcome> {
    // da-rcoi: strict-spec enforcement also runs on the update / merge /
    // rekey paths — a key renamed into an undeclared slot or merged in
    // from a payload would otherwise slip past the log-time check.
    crate::commands::helpers::enforce_strict_extra_data(&updated, db)?;
    let (entry, old_slug) =
        db.save_existing_entry_with_auto_reinterp(&existing.slug, updated, slug_delimiter)?;
    Ok(UpdateOutcome { entry, old_slug })
}

fn combine_extra(
    existing: Option<JsonValue>,
    payload: Option<JsonValue>,
    mode: ExtraMergeMode,
) -> Option<JsonValue> {
    match (existing, payload, mode) {
        (None, payload, _) => payload,
        (existing, None, _) => existing,
        (
            Some(JsonValue::Object(mut e_map)),
            Some(JsonValue::Object(p_map)),
            ExtraMergeMode::Shallow,
        ) => {
            for (k, v) in p_map {
                // `key=` with an empty value is treated as "drop this key".
                if is_empty_string(&v) {
                    e_map.remove(&k);
                } else {
                    e_map.insert(k, v);
                }
            }
            if e_map.is_empty() {
                None
            } else {
                Some(JsonValue::Object(e_map))
            }
        }
        (Some(existing), Some(payload), ExtraMergeMode::Deep) => {
            Some(deep_merge(existing, payload))
        }
        // Non-object payload on an existing: payload wins wholesale.
        (_, Some(payload), _) => Some(payload),
    }
}

fn is_empty_string(v: &JsonValue) -> bool {
    matches!(v, JsonValue::String(s) if s.is_empty())
}

/// da-5t94: pull a `id=` token out of the update payload, returning it as
/// the slug-body literal to stamp onto `id_structure`. Resets the payload
/// to `None` once empty so an `id`-only update doesn't carry a stray empty
/// object into `combine_extra`.
fn take_reserved_id(payload_extra: &mut Option<JsonValue>) -> Option<String> {
    let JsonValue::Object(obj) = payload_extra.as_mut()? else {
        return None;
    };
    let v = obj.remove("id")?;
    let lit = match v {
        JsonValue::String(s) => s,
        other => other.to_string(),
    };
    if obj.is_empty() {
        *payload_extra = None;
    }
    Some(lit)
}

fn deep_merge(existing: JsonValue, incoming: JsonValue) -> JsonValue {
    match (existing, incoming) {
        (JsonValue::Object(mut e_map), JsonValue::Object(i_map)) => {
            for (k, v) in i_map {
                match e_map.remove(&k) {
                    Some(prev) => {
                        e_map.insert(k, deep_merge(prev, v));
                    }
                    None => {
                        e_map.insert(k, v);
                    }
                }
            }
            JsonValue::Object(e_map)
        }
        (_, incoming) => incoming,
    }
}

/// Outcome of [`parse_update_payload`].
///
/// `default_last` is true when the caller did NOT supply any
/// non-`KEY=VALUE` positional — i.e. we fell through to the `_LAST`
/// fallback. da-uzxj: `update_batch_from_args` reads this to decide
/// whether the 15-min stale-default prompt should fire.
pub(crate) struct UpdatePayload {
    pub pattern: String,
    pub default_last: bool,
    pub value: Option<JsonValue>,
    pub extra: Option<JsonValue>,
}

pub(crate) fn parse_update_payload(args: &UpdateArgs) -> Result<UpdatePayload> {
    use datum_parse::input::data_args::{handle_data_args, DataArgs};

    // da-sk5c: rewire the leading positional through the same slot-fill
    // discipline datum's updateCmd.ts gets from `flexiblePositional`.
    // Whatever argparse caught as the first positional joins the rest of
    // the stream and the FIRST bare (non-`KEY=VALUE`) token wins the id
    // slot — so `dm update <quickId> some=data` and `dm update some=data
    // <quickId>` are interchangeable. KEY=VALUE tokens flow through to
    // the payload regardless of position; if no bare positional is
    // supplied, `_LAST_WITH_PROTECTION` is the default.
    //
    // The id is split off *before* the rest enters `handle_data_args` so
    // its raw string survives intact (the data pipeline's `infer_type`
    // would otherwise rewrite e.g. `gym:a,gym:b,` into a JSON array,
    // collapsing daydumb's compound-id comma syntax — datum's TS form
    // never had a compound-id path, so the TS rewiring never had to care).
    let mut resolved_id: Option<String> = None;
    let mut positionals: Vec<String> = Vec::with_capacity(1 + args.rest.len());
    let mut consider = |tok: String| {
        if resolved_id.is_none() && !tok.contains('=') {
            resolved_id = Some(tok);
        } else {
            positionals.push(tok);
        }
    };
    if let Some(id) = &args.id_or_slug {
        consider(id.clone());
    }
    for r in &args.rest {
        consider(r.clone());
    }
    // da-uzxj: when the user supplied no bare positional we record
    // `default_last = true` and fall back to `_LAST` (NOT
    // `_LAST_WITH_PROTECTION`). The 15-min protection is now applied
    // as a prompt in `update_batch_from_args` rather than as a hard
    // refusal at the resolver layer — keeps the behaviour symmetric
    // with `delete` and `undo`.
    let default_last = resolved_id.is_none();
    let resolved_id =
        resolved_id.unwrap_or_else(|| datum_core::ids::resolve::LAST_TOKEN.to_string());

    // Same composition shape as `parse_log_args` — the only difference is
    // there are no `-K` slug-keys in update (slug stays put) and no `-K`
    // slot fill, so every `-k KEY=VALUE` is an immediate that bypasses
    // the data queue. `-k KEY` (no `=`) and `-k KEY=` stay as declared
    // slots so positionals can fill them.
    // Same daydumb compat as the log path — see `parse_log_args`. The
    // `flat_paths` compat shim is gone (da-ju71.4); leading-dot keys
    // now fold into `state.*`.
    let mut data_args = DataArgs {
        preserve_null_strings: true,
        ..DataArgs::default()
    };
    let mut injected_immediates: Vec<String> = Vec::new();
    for spec in &args.keys {
        let (name, rhs) = spec
            .split_once('=')
            .map(|(n, v)| (n, Some(v)))
            .unwrap_or((spec.as_str(), None));
        if name.is_empty() {
            anyhow::bail!("empty key name in spec '{spec}'");
        }
        match rhs {
            Some(v) if !v.is_empty() => injected_immediates.push(format!("{name}={v}")),
            _ => data_args.keys.push(spec.clone()),
        }
    }

    for c in &args.comment {
        data_args.comment.push(c.clone());
    }

    data_args
        .data
        .reserve(injected_immediates.len() + positionals.len());
    data_args.data.extend(injected_immediates);
    data_args.data.extend(positionals);
    data_args.remainder = Some(PRIMARY_VALUE_KEY.to_string());

    let mut datum_data = handle_data_args(&data_args).map_err(anyhow::Error::from)?;

    // da-rori: state normalization (the trivial `{id: v}` → scalar collapse)
    // now lives at the shared `extract_reserved_columns` boundary that
    // `apply_mutation` runs on this payload, so `update <hid> state=v` and
    // `switch f v` produce the same `entry.state` shape by construction.
    let value = datum_data.remove(PRIMARY_VALUE_KEY);

    // Preserve update's empty-string drop sentinel: `KEY=` (or `-k KEY=`)
    // drops the key from the existing entry in `combine_extra`. handle_data_args
    // turns an empty value into "remove from payload"; on the update path that
    // erases the signal, so we re-introduce a String("") for each such token
    // so `is_empty_string` downstream still fires. The id_or_slug positional
    // joins the chain here because the flex rewiring above means argparse's
    // "first positional" can itself be a `key=` drop sentinel.
    for token in args
        .id_or_slug
        .iter()
        .chain(args.rest.iter())
        .chain(args.keys.iter())
    {
        if let Some((k, v)) = token.split_once('=') {
            if v.is_empty() && !k.is_empty() {
                datum_data.insert(k.to_string(), JsonValue::String(String::new()));
            }
        }
    }

    let extra_data = if datum_data.is_empty() {
        None
    } else {
        Some(JsonValue::Object(datum_data))
    };
    Ok(UpdatePayload {
        pattern: resolved_id,
        default_last,
        value,
        extra: extra_data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::UpdateArgs;

    use datum_core::entry::Verb;

    fn update_args(id: &str, rest: Vec<&str>) -> UpdateArgs {
        UpdateArgs {
            id_or_slug: Some(id.to_string()),
            rest: rest.into_iter().map(String::from).collect(),
            strategy: None,
            id: None,
            id_delimiter: None,
            keys: vec![],
            comment: Vec::new(),
            dry_run: false,
            strict: false,
            output: Default::default(),
        }
    }

    fn seed_entry(
        db: &Db,
        field: &str,
        verb: Verb,
        value: Option<JsonValue>,
        extra: Option<JsonValue>,
    ) -> Entry {
        use datum_core::time::DatumTime;
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime {
            utc: t,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new(field, verb, value, extra, Some(dt), None, "d1");
        db.insert_entry(&e).unwrap();
        e
    }

    #[test]
    fn replace_strategy_returns_fresh_entry() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            None,
            Some(serde_json::json!({"reps": 10})),
        );

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"reps": 20})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert!(outcome.old_slug.is_none());
        assert_eq!(outcome.entry.slug, e.slug);
        assert_eq!(
            outcome.entry.data.as_ref().unwrap()["reps"],
            serde_json::json!(20),
        );
    }

    #[test]
    fn merge_strategy_deep_merges() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "notes",
            Verb::Point,
            None,
            Some(serde_json::json!({"nested": {"a": 1, "c": 3}})),
        );

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"nested": {"b": 2}})),
                strategy: UpdateStrategy::Merge,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert!(outcome.old_slug.is_none());
        let nested = &outcome.entry.data.as_ref().unwrap()["nested"];
        assert_eq!(nested["a"], serde_json::json!(1));
        assert_eq!(nested["b"], serde_json::json!(2));
        assert_eq!(nested["c"], serde_json::json!(3));
    }

    #[test]
    fn rekey_strategy_renames_data_keys() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            None,
            Some(serde_json::json!({"reps": 10, "set": 3})),
        );

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                // {reps: "repetitions"} → rename data.reps to data.repetitions
                payload_extra: Some(serde_json::json!({"reps": "repetitions"})),
                strategy: UpdateStrategy::Rekey,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        let data = outcome.entry.data.as_ref().unwrap();
        assert_eq!(
            data["repetitions"],
            serde_json::json!(10),
            "value preserved under new key"
        );
        assert!(data.get("reps").is_none(), "old key removed");
        assert_eq!(data["set"], serde_json::json!(3), "untouched key preserved");
        // No id_structure → slug shouldn't move.
        assert!(outcome.old_slug.is_none());
    }

    #[test]
    fn rekey_without_mapping_errors() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("x")),
            None,
        );

        let err = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: None,
                strategy: UpdateStrategy::Rekey,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("rekey"),
            "expected rekey-mapping error, got: {err}"
        );
    }

    #[test]
    fn rekey_mapping_value_must_be_string() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            None,
            Some(serde_json::json!({"reps": 10})),
        );

        let err = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"reps": 42})),
                strategy: UpdateStrategy::Rekey,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("non-empty string"),
            "expected string-mapping error, got: {err}"
        );
    }

    #[test]
    fn rekey_cascades_to_slug_when_renamed_key_is_in_template() {
        // Set up an entry with id_structure referencing data.kind. When we
        // rename data.kind → data.category, no template var "category" is
        // resolvable, but the test focuses on a rename that DOES still
        // resolve a template variable: rename data.oldKind → data.kind so
        // template '%field%:%kind%' starts producing the value.
        let db = Db::open_in_memory().unwrap();

        let mut e = seed_entry(
            &db,
            "task",
            Verb::Point,
            None,
            Some(serde_json::json!({"oldKind": "chore"})),
        );
        // Stamp id_structure post-insert via an UPDATE; in production this
        // comes from `--id` at create time.
        e.id_structure = Some("%kind%".to_string());
        db.save_existing_entry(&e).unwrap();

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"oldKind": "kind"})),
                strategy: UpdateStrategy::Rekey,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.old_slug.as_deref(), Some(e.slug.as_str()));
        assert_eq!(outcome.entry.slug, "task:chore");
    }

    #[test]
    fn strategy_override_ignore_rejected() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "gym", Verb::Point, Some(serde_json::json!(1)), None);

        let mut args = update_args(&e.slug, vec!["note=x"]);
        args.strategy = Some(ConflictStrategyArg::Ignore);
        let err = update_from_args(args, UpdateStrategy::Replace, &db).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("ignore"),
            "expected ignore rejection, got: {err}"
        );
    }

    #[test]
    fn tombstoned_row_rejected_across_all_strategies() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            Some(serde_json::json!(1)),
            Some(serde_json::json!({"reps": 10})),
        );
        db.soft_delete_entry(&e.slug).unwrap();

        for strategy in [
            UpdateStrategy::Replace,
            UpdateStrategy::Merge,
            UpdateStrategy::Rekey,
        ] {
            // Tombstone check fires before strategy dispatch, so any payload
            // is fine here — the call never reaches apply_*.
            let err = update_with_strategy(
                UpdateRequest {
                    id_or_slug: &e.slug,
                    payload_value: None,
                    payload_extra: None,
                    strategy,
                    slug_delimiter: ":",
                },
                &db,
            )
            .unwrap_err();
            assert!(
                err.to_string().to_lowercase().contains("tombstoned"),
                "{strategy:?}: expected tombstone error, got: {err}"
            );
        }
    }

    /// da-rori: `update <hid> state=happy` must yield the SAME scalar
    /// `entry.state` as `switch mood happy` — the trivial `{id: "happy"}`
    /// collapses at the shared `extract_reserved_columns` boundary, and
    /// state never leaks into `data`.
    #[test]
    fn update_state_key_collapses_trivial_id_to_scalar() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("sad")),
            None,
        );

        let payload = parse_update_payload(&update_args(&e.slug, vec!["state=happy"])).unwrap();
        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: payload.value,
                payload_extra: payload.extra,
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.entry.state, Some(serde_json::json!("happy")));
        assert_eq!(outcome.entry.value, Some(serde_json::json!("happy")));
        assert!(
            outcome
                .entry
                .data
                .as_ref()
                .is_none_or(|d| d.get("state").is_none()),
            "state must not leak into data: {:?}",
            outcome.entry.data,
        );
    }

    /// da-rori: structured state (`state.a`, `state.b`) normalizes
    /// identically on the update path — left intact, not collapsed.
    #[test]
    fn update_structured_state_survives_intact() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("x")),
            None,
        );

        let payload = parse_update_payload(&update_args(&e.slug, vec![".a=1", ".b=2"])).unwrap();
        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: payload.value,
                payload_extra: payload.extra,
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(
            outcome.entry.state,
            Some(serde_json::json!({"a": 1, "b": 2}))
        );
    }

    #[test]
    fn update_reinterpolates_slug_when_template_var_changes() {
        // da-5ns.3: `state=` is a column-bound reserved key. The
        // `%state%` lookup falls back to `entry.state` for Verb::Switch,
        // so updating `state=sad` re-derives the slug.
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );
        e.id_structure = Some("%state%".to_string());
        db.save_existing_entry(&e).unwrap();
        let original_hid = e.hid.clone();

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"state": "sad"})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.old_slug.as_deref(), Some(e.slug.as_str()));
        assert_eq!(outcome.entry.slug, "mood:sad");
        // Hid is preserved — the row didn't move, just got rekeyed.
        assert_eq!(outcome.entry.hid, original_hid);
        // Only one live row exists for the field (no new row inserted).
        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
    }

    #[test]
    fn update_reinterpolates_field_from_field_structure() {
        // field_structure makes `field` a template too. Renaming the
        // referenced data key cascades into the field column.
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(
            &db,
            "word_pt",
            Verb::Point,
            None,
            Some(serde_json::json!({"srcLang": "pt"})),
        );
        e.field_structure = Some("word_%srcLang%".to_string());
        db.save_existing_entry(&e).unwrap();

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"srcLang": "es"})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.entry.field, "word_es");
    }

    #[test]
    fn update_no_op_when_template_var_unchanged() {
        // Touching un-templated data should NOT move the slug.
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            None,
            Some(serde_json::json!({"reps": 10, "kind": "deadlift"})),
        );
        e.id_structure = Some("%kind%".to_string());
        // Stamp the slug to match the template so we're starting from the
        // canonical form (otherwise a stale slug would 'jump' on first edit).
        e.slug = datum_core::ids::Slug("gym:deadlift".to_string());
        db.save_existing_entry_with_rekey(&seed_slug_for("gym"), &e)
            .unwrap();

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                // Bumping reps doesn't touch %kind% → slug stays put.
                payload_extra: Some(serde_json::json!({"reps": 20})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert!(outcome.old_slug.is_none(), "slug should not have moved");
        assert_eq!(outcome.entry.slug, "gym:deadlift");
    }

    /// Helper for `update_no_op_when_template_var_unchanged` — the seeded
    /// slug for a `field=gym` entry created at 2024-01-01T00:00:00.000Z by
    /// `seed_entry` is `gym:2024-01-01T00:00:00.000Z`.
    fn seed_slug_for(field: &str) -> datum_core::ids::Slug {
        datum_core::ids::Slug(format!("{field}:2024-01-01T00:00:00.000Z"))
    }

    #[test]
    fn update_records_history_on_auto_rekey() {
        // The auto-rekey path must record an entry_history row capturing the
        // pre-mutation state. `datum history` would otherwise lose the slug
        // change.
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"state": "happy"})),
        );
        e.id_structure = Some("%state%".to_string());
        db.save_existing_entry(&e).unwrap();

        update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"state": "sad"})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        // Two save_existing_entry calls + the auto-rekey itself produce
        // multiple history rows; we only need to assert the final pre-rekey
        // state was captured (slug 'mood:happy' or the seed slug appears).
        let conn = db.conn();
        let mut stmt = conn
            .prepare("SELECT change_kind, snapshot FROM entry_history WHERE entry_hid = ?1 ORDER BY version")
            .unwrap();
        let rows: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![e.hid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            rows.iter().any(|(kind, _)| kind == "create"),
            "expected an initial create row, got: {rows:?}"
        );
        assert!(
            rows.iter().filter(|(kind, _)| kind == "user_edit").count() >= 1,
            "expected user_edit row capturing pre-rekey state, got: {rows:?}"
        );
    }

    #[test]
    fn update_dur_rekeys_when_template_references_dur() {
        // da-5ns.4: %dur% resolves from the entry.dur column. Mutating
        // dur= must cascade into a templated slug.
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(&db, "nap", Verb::Point, None, None);
        e.dur = Some(datum_core::dur::Dur::parse("PT1H").unwrap());
        e.id_structure = Some("%dur%".to_string());
        e.slug = datum_core::ids::Slug("nap:PT1H".to_string());
        db.save_existing_entry_with_rekey(&seed_slug_for("nap"), &e)
            .unwrap();

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"dur": "PT2H"})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.old_slug.as_deref(), Some(e.slug.as_str()));
        assert_eq!(outcome.entry.slug, "nap:PT2H");
        assert_eq!(
            outcome
                .entry
                .dur
                .as_ref()
                .map(datum_core::dur::Dur::to_storage_string)
                .as_deref(),
            Some("PT2H")
        );
    }

    #[test]
    fn update_following_state_rekeys_when_template_references_it() {
        // da-5ns.4: %following_state% resolves from
        // entry.following_state. Mutating it cascades into a templated
        // slug. Pair it with a state+dur so derive_verb accepts the
        // (state, dur, following_state) triple as a Block shape.
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(
            &db,
            "block",
            Verb::Switch,
            Some(serde_json::json!("work")),
            None,
        );
        e.dur = Some(datum_core::dur::Dur::parse("PT1H").unwrap());
        e.following_state = Some(serde_json::json!("idle"));
        e.id_structure = Some("%following_state%".to_string());
        e.slug = datum_core::ids::Slug("block:idle".to_string());
        db.save_existing_entry_with_rekey(&seed_slug_for("block"), &e)
            .unwrap();

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"following_state": "focused"})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.old_slug.as_deref(), Some(e.slug.as_str()));
        assert_eq!(outcome.entry.slug, "block:focused");
    }

    #[test]
    fn update_occur_time_rekeys_when_template_references_occur_time() {
        // da-5ns.4: occur_time= on `dm update` mutates the column AND
        // cascades into a templated slug.
        use datum_core::time::DatumTime;
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(&db, "coffee", Verb::Point, None, None);
        e.id_structure = Some("%occur_time%".to_string());
        e.slug = datum_core::ids::Slug("coffee:2024-01-01T00:00:00.000Z".to_string());
        db.save_existing_entry_with_rekey(&seed_slug_for("coffee"), &e)
            .unwrap();

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({
                    "occur_time": "2026-05-01T10:00:00Z"
                })),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.old_slug.as_deref(), Some(e.slug.as_str()));
        assert_eq!(outcome.entry.slug, "coffee:2026-05-01T10:00:00.000Z");
        let want_utc: chrono::DateTime<chrono::Utc> = "2026-05-01T10:00:00Z".parse().unwrap();
        assert_eq!(outcome.entry.occur_time.as_ref().unwrap().utc, want_utc,);
        let _ = DatumTime::now;
    }

    #[test]
    fn update_state_rekeys_for_non_switch_verb_via_column() {
        // da-5ns.4: %state% now resolves from the entry.state column for
        // any verb (not just Switch). Updating state= on a Verb::Point
        // entry must rekey.
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(&db, "task", Verb::Point, None, None);
        e.state = Some(serde_json::json!("todo"));
        e.id_structure = Some("%state%".to_string());
        e.slug = datum_core::ids::Slug("task:todo".to_string());
        db.save_existing_entry_with_rekey(&seed_slug_for("task"), &e)
            .unwrap();

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"state": "doing"})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.old_slug.as_deref(), Some(e.slug.as_str()));
        assert_eq!(outcome.entry.slug, "task:doing");
    }

    #[test]
    fn flexible_positional_id_after_keyvalue() {
        // da-sk5c: the acceptance scenario — `alias abc "update some=data"`
        // expands to `update some=data <quickId>`, where argparse catches
        // `some=data` as the leading positional. The flex rewiring must
        // route `<quickId>` to the id slot and `some=data` to the payload,
        // not the other way around.
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "task",
            Verb::Point,
            None,
            Some(serde_json::json!({"reps": 10})),
        );

        // id_or_slug is "some=data" (argparse's first positional), rest is
        // [slug] — i.e. the user typed `update some=data <slug>`.
        let mut args = update_args("some=data", vec![&e.slug]);
        // Sanity: the test helper put the key=value token in id_or_slug
        // exactly as argparse would after seeing `update some=data <slug>`.
        assert_eq!(args.id_or_slug.as_deref(), Some("some=data"));

        args.strategy = Some(ConflictStrategyArg::Update);
        update_from_args(args, UpdateStrategy::Replace, &db).unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let data = after.data.as_ref().unwrap();
        assert_eq!(
            data["some"],
            serde_json::json!("data"),
            "key=value payload landed under data.some"
        );
        assert_eq!(
            data["reps"],
            serde_json::json!(10),
            "untouched keys preserved (Replace is shallow per-top-level-key)"
        );
    }

    #[test]
    fn flexible_positional_id_before_keyvalue() {
        // The natural-order companion: `update <slug> some=data`. With the
        // flex rewiring both orderings collapse to the same payload.
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "task",
            Verb::Point,
            None,
            Some(serde_json::json!({"reps": 10})),
        );

        update_from_args(
            update_args(&e.slug, vec!["some=data"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let data = after.data.as_ref().unwrap();
        assert_eq!(data["some"], serde_json::json!("data"));
        assert_eq!(data["reps"], serde_json::json!(10));
    }

    #[test]
    fn deep_merge_recurses_into_objects() {
        let existing = serde_json::json!({"a": {"b": 1, "c": 2}, "x": "keep"});
        let incoming = serde_json::json!({"a": {"b": 10, "d": 4}, "y": "new"});
        let merged = deep_merge(existing, incoming);
        assert_eq!(merged["a"]["b"], serde_json::json!(10));
        assert_eq!(merged["a"]["c"], serde_json::json!(2));
        assert_eq!(merged["a"]["d"], serde_json::json!(4));
        assert_eq!(merged["x"], serde_json::json!("keep"));
        assert_eq!(merged["y"], serde_json::json!("new"));
    }
}
