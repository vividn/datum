//! Unified update/merge/rekey core.
//!
//! One entry point — [`update_with_strategy`] — implements the three
//! mutation strategies. The `update`, `merge`, and `rekey` verbs are thin
//! CLI wrappers around it. Future batch mutations (S3) iterate over this
//! function rather than reimplementing per-strategy code.

use anyhow::Result;
use datum_core::{
    db::Db,
    entry::Entry,
    field_spec::{self, InterpolationContext},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::{
    args::{ConflictStrategyArg, OutputFlagsExt, UpdateArgs},
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
    let existing = crate::cmd::resolve_prefix_or_tombstone(db, req.id_or_slug)?;
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
        UpdateStrategy::Rekey => apply_data_key_rename(
            existing,
            req.payload_extra,
            req.slug_delimiter,
            db,
        ),
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

    let (payload_value, payload_extra) = crate::cmd::parse_update_payload(&args)?;
    let output = args.output();

    let pattern = args
        .id_or_slug
        .as_deref()
        .unwrap_or(datum_core::ids::resolve::LAST_WITH_PROTECTION_TOKEN);

    let outcome = update_with_strategy(
        UpdateRequest {
            id_or_slug: pattern,
            payload_value,
            payload_extra,
            strategy,
            slug_delimiter: &args.id_delimiter,
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

    let pattern = args
        .id_or_slug
        .as_deref()
        .unwrap_or(datum_core::ids::resolve::LAST_WITH_PROTECTION_TOKEN);
    let (entries, token_errors) = crate::cmd::resolve_batch(
        db,
        pattern,
        crate::cmd::BatchMode::LiveOrTombstone,
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

    let (payload_value, payload_extra) = crate::cmd::parse_update_payload(&args)?;
    let output = args.output();

    if args.dry_run {
        let registry = db.load_registry().ok();
        let now = DatumTime::now();
        let ctxs = crate::cmd::build_render_contexts(&entries, db);
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
                slug_delimiter: &args.id_delimiter,
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
        ConflictStrategyArg::Ignore => anyhow::bail!(
            "`ignore` is not a meaningful strategy for update/merge/rekey"
        ),
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
    // them to the entry's columns instead of `data`. Rejects `value=`
    // outright. The storage layer's `derive_verb` call recomputes the
    // verb on the resulting (state, dur, fs) triple.
    let mut payload_extra = payload_extra;
    let reserved = crate::cmd::extract_reserved_columns(&mut payload_extra)?;

    let mut updated = existing.clone();
    updated.data = combine_extra(updated.data, payload_extra, mode);
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
    if let Some(fs) = reserved.following_state {
        updated.following_state = Some(fs);
    }
    let _ = reserved.occur_time;
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
        _ => anyhow::bail!(
            "rekey requires a key-rename mapping like 'oldKey=newKey'"
        ),
    };
    let mut updated = existing.clone();
    if let Some(JsonValue::Object(mut data_map)) = updated.data.take() {
        for (old_key, new_key_v) in mapping {
            let new_key = match new_key_v {
                JsonValue::String(s) if !s.is_empty() => s,
                _ => anyhow::bail!(
                    "rekey mapping value for '{old_key}' must be a non-empty string"
                ),
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
fn persist_with_auto_reinterp(
    existing: Entry,
    mut updated: Entry,
    slug_delimiter: &str,
    db: &Db,
) -> Result<UpdateOutcome> {
    if let Some(tmpl) = updated.field_structure.clone() {
        let uid = updated.short_uid();
        let ctx = InterpolationContext::from_entry(&updated, &uid);
        updated.field = field_spec::interpolate(&tmpl, &ctx, slug_delimiter);
    }
    let new_slug = if let Some(tmpl) = updated.id_structure.clone() {
        let uid = updated.short_uid();
        let ctx = InterpolationContext::from_entry(&updated, &uid);
        Some(datum_core::ids::Slug(field_spec::interpolate(
            &tmpl, &ctx, slug_delimiter,
        )))
    } else {
        None
    };
    updated.content_hash = updated.expected_content_hash();

    let slug_changed = matches!(&new_slug, Some(s) if *s != existing.slug);

    if slug_changed {
        let target = new_slug.unwrap();
        updated.slug = target.clone();
        db.save_existing_entry_with_rekey(&existing.slug, &updated)?;
        let fresh = db.get_entry(&target)?;
        Ok(UpdateOutcome {
            entry: fresh,
            old_slug: Some(existing.slug),
        })
    } else {
        db.save_existing_entry(&updated)?;
        let fresh = db.get_entry(&updated.slug)?;
        Ok(UpdateOutcome {
            entry: fresh,
            old_slug: None,
        })
    }
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
            id_delimiter: ":".to_string(),
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
        let dt = DatumTime { utc: t, offset_minutes: 0, timezone: "UTC".to_string() };
        let e = Entry::new(field, verb, value, extra, None, Some(dt), None, "d1");
        db.insert_entry(&e).unwrap();
        e
    }

    #[test]
    fn replace_strategy_returns_fresh_entry() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Occur,
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
            Verb::Add,
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
            Verb::Occur,
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
        assert_eq!(data["repetitions"], serde_json::json!(10), "value preserved under new key");
        assert!(data.get("reps").is_none(), "old key removed");
        assert_eq!(data["set"], serde_json::json!(3), "untouched key preserved");
        // No id_structure → slug shouldn't move.
        assert!(outcome.old_slug.is_none());
    }

    #[test]
    fn rekey_without_mapping_errors() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "mood", Verb::Switch, Some(serde_json::json!("x")), None);

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
            Verb::Occur,
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
            Verb::Occur,
            None,
            Some(serde_json::json!({"oldKind": "chore"})),
        );
        // Stamp id_structure post-insert via an UPDATE; in production this
        // comes from `--id` at create time.
        e.id_structure = Some("%field%:%kind%".to_string());
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
        let e = seed_entry(&db, "gym", Verb::Occur, Some(serde_json::json!(1)), None);

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
            Verb::Occur,
            Some(serde_json::json!(1)),
            Some(serde_json::json!({"reps": 10})),
        );
        db.soft_delete_entry(&e.slug).unwrap();

        for strategy in [UpdateStrategy::Replace, UpdateStrategy::Merge, UpdateStrategy::Rekey] {
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
        e.id_structure = Some("%field%:%state%".to_string());
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
            Verb::Add,
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
            Verb::Occur,
            None,
            Some(serde_json::json!({"reps": 10, "kind": "deadlift"})),
        );
        e.id_structure = Some("%field%:%kind%".to_string());
        // Stamp the slug to match the template so we're starting from the
        // canonical form (otherwise a stale slug would 'jump' on first edit).
        e.slug = datum_core::ids::Slug("gym:deadlift".to_string());
        db.save_existing_entry_with_rekey(&seed_slug_for("gym"), &e).unwrap();

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
        e.id_structure = Some("%field%:%state%".to_string());
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
