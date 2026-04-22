//! Unified update/merge/rekey core.
//!
//! One entry point — [`update_with_strategy`] — implements the three
//! mutation strategies. The `update`, `merge`, and `rekey` verbs are thin
//! CLI wrappers around it. Future batch mutations (S3) iterate over this
//! function rather than reimplementing per-strategy code.

use anyhow::Result;
use datum_core::{
    db::{ConflictStrategy, Db, InsertOutcome},
    entry::{new_hid, Entry},
    field_spec::{self, InterpolationContext},
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
    /// Re-interpolate the slug from a template, insert as a new row, leave
    /// the old row in place. Requires `rekey_template` or a spec-defined
    /// `id_template`.
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
/// `old_slug` is `Some(_)` iff the strategy produced a new row (rekey);
/// otherwise `entry.slug` is the in-place-updated row's slug.
#[derive(Clone, Debug)]
pub struct UpdateOutcome {
    pub entry: Entry,
    pub old_slug: Option<String>,
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
    /// `--id` override or spec template fallback; only consulted when
    /// `strategy == Rekey`.
    pub rekey_template: Option<String>,
    /// Delimiter collapsing empty segments in the interpolated slug;
    /// only consulted when `strategy == Rekey`.
    pub rekey_delimiter: &'a str,
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
            db,
        ),
        UpdateStrategy::Merge => apply_mutation(
            existing,
            req.payload_value,
            req.payload_extra,
            ExtraMergeMode::Deep,
            db,
        ),
        UpdateStrategy::Rekey => apply_rekey(
            existing,
            req.payload_value,
            req.payload_extra,
            req.rekey_template,
            req.rekey_delimiter,
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

    let outcome = update_with_strategy(
        UpdateRequest {
            id_or_slug: &args.id_or_slug,
            payload_value,
            payload_extra,
            strategy,
            rekey_template: args.id.clone(),
            rekey_delimiter: &args.id_delimiter,
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

    let (entries, token_errors) = crate::cmd::resolve_batch(
        db,
        &args.id_or_slug,
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
        for entry in &entries {
            if entry.deleted_at.is_some() {
                eprintln!(
                    "[dry-run] skip tombstoned: {} (restore first with `datum undelete`)",
                    entry.slug,
                );
                continue;
            }
            crate::render::print_entry(entry, registry.as_ref());
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
                rekey_template: args.id.clone(),
                rekey_delimiter: &args.id_delimiter,
            },
            db,
        );

        match result {
            Ok(outcome) => render_outcome(&outcome, &output, db),
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
    db: &Db,
) -> Result<UpdateOutcome> {
    let mut updated = existing;
    updated.data = combine_extra(updated.data, payload_extra, mode);
    if let Some(v) = payload_value {
        updated.value = Some(v);
    }
    updated.content_hash = updated.expected_content_hash();

    db.save_existing_entry(&updated)?;

    // Fetch the persisted row so callers see the db-stamped `modified_at`.
    let fresh = db.get_entry(&updated.slug)?;
    Ok(UpdateOutcome { entry: fresh, old_slug: None })
}

fn apply_rekey(
    existing: Entry,
    payload_value: Option<JsonValue>,
    payload_extra: Option<JsonValue>,
    rekey_template: Option<String>,
    rekey_delimiter: &str,
    db: &Db,
) -> Result<UpdateOutcome> {
    let old_slug = existing.slug.clone();

    let mut new_entry = existing;
    // Payloads are applied with shallow semantics on rekey — same as update.
    new_entry.data = combine_extra(new_entry.data, payload_extra, ExtraMergeMode::Shallow);
    if let Some(v) = payload_value {
        new_entry.value = Some(v);
    }

    // Fresh hid + timestamps. Slug is re-interpolated below.
    new_entry.hid = new_hid();
    let now = chrono::Utc::now();
    new_entry.create_time = now;
    new_entry.modify_time = now;
    new_entry.deleted_at = None;

    // Explicit --id beats the spec's id_template. Rekey without any
    // template is meaningless — the new slug would match the old one.
    let spec_template = db.load_spec(&new_entry.field)?.and_then(|s| s.id_template);
    let template = rekey_template.or(spec_template).ok_or_else(|| {
        anyhow::anyhow!(
            "rekey requires a template: pass --id or define id_template on the spec"
        )
    })?;

    let uid = new_entry.short_uid();
    let ctx = InterpolationContext::from_entry(&new_entry, &uid);
    new_entry.slug = field_spec::interpolate(&template, &ctx, rekey_delimiter);
    new_entry.id_structure = Some(template);

    new_entry.content_hash = new_entry.expected_content_hash();

    // Use the Rekey conflict strategy so duplicate content_hashes
    // (rekey with no payload change) don't collide with the existing row.
    let outcome = db.insert_entry_with_strategy(&new_entry, ConflictStrategy::Rekey)?;
    match outcome {
        InsertOutcome::Inserted { .. } | InsertOutcome::Rekeyed { .. } => Ok(UpdateOutcome {
            entry: new_entry,
            old_slug: Some(old_slug),
        }),
        other => anyhow::bail!("rekey produced unexpected outcome: {other:?}"),
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
            id_or_slug: id.to_string(),
            rest: rest.into_iter().map(String::from).collect(),
            strategy: None,
            id: None,
            id_delimiter: ":".to_string(),
            keys: vec![],
            comment: None,
            dry_run: false,
            strict: false,
            output_flags: Default::default(),
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
                rekey_template: None,
                rekey_delimiter: ":",
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
                rekey_template: None,
                rekey_delimiter: ":",
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
    fn rekey_strategy_returns_old_slug() {
        use datum_core::field_spec::FieldSpec;
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.id_template = Some("%field%:%state%".to_string());
        db.save_field_spec(&spec).unwrap();

        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("calm")),
            Some(serde_json::json!({"state": "calm"})),
        );

        let outcome = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: None,
                strategy: UpdateStrategy::Rekey,
                rekey_template: Some("%field%:%state%".to_string()),
                rekey_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        assert_eq!(outcome.old_slug.as_deref(), Some(e.slug.as_str()));
        assert_eq!(outcome.entry.slug, "mood:calm");

        // Old row preserved in place.
        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 2);
    }

    #[test]
    fn rekey_without_template_errors() {
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "mood", Verb::Switch, Some(serde_json::json!("x")), None);

        let err = update_with_strategy(
            UpdateRequest {
                id_or_slug: &e.slug,
                payload_value: None,
                payload_extra: None,
                strategy: UpdateStrategy::Rekey,
                rekey_template: None,
                rekey_delimiter: ":",
            },
            &db,
        )
        .unwrap_err();
        assert!(err.to_string().contains("template"), "{err}");
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
            let err = update_with_strategy(
                UpdateRequest {
                    id_or_slug: &e.slug,
                    payload_value: None,
                    payload_extra: None,
                    strategy,
                    rekey_template: Some("%field%:%uid%".to_string()),
                    rekey_delimiter: ":",
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
