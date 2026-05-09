//! Command dispatch and argument → Entry conversion.

use anyhow::{Context, Result};
use chrono::{Local, TimeZone, Utc};
use datum_core::{
    action::Action,
    db::{
        ConflictStrategy, Db, InsertOutcome, ListParams, RetimeFilter, TimeMetric,
        IGNORED_STATE_VALUE,
    },
    derive_verb,
    entry::{Entry, Verb},
    field_spec::{self, Alias, FieldSpec, InterpolationContext},
    ids::{contains_wildcard, resolve_quickid, resolve_quickid_single},
    time::{parse_date_arg, parse_duration, parse_time_arg, resolve_timezone, DatumTime, TimeBuilder},
};
use regex_lite::Regex;
use serde_json::Value as JsonValue;

use crate::{
    args::{
        AliasCommand, Cli, Command, ConfigCommand, ConfigShowArgs,
        ConflictsAutoResolveArgs, ConflictsCommand, ConflictsListArgs, ConflictsResolveArgs,
        ConflictsShowArgs, DbImportArgs, DbImportArgsExt, GetArgs,
        GrepArgs, HistoryArgs, IdsArgs, IgnoreArgs, LogArgs, LogArgsExt, MigrateCommand,
        NowArgs, OutputFlagsExt, RetimeArgs, SetupArgs, SpecCommand, SpecSetArgs, TailArgs,
        TrackArgs, UndoArgs, UpdateArgs, WhyArgs,
    },
    config::{self, device_id},
    render::{
        print_entry_pretty, print_now_view, print_table_with,
        show_create, show_create_with_context, show_delete, show_exists, show_rename,
        show_update_with_context, RenderContext, TableArgs, TimeMetricCol,
    },
};

/// Parse an argv vector through the full pre-pass → parser → dispatch
/// path against `db`. Used by integration tests to exercise the CLI
/// without going through the binary. Hides the bespoke parser behind
/// `datum-commands`'s module boundary so consumers don't pull it in
/// directly.
pub fn dispatch_argv(argv: Vec<String>, db: &Db) -> Result<()> {
    let expanded = crate::args::expand_argv_with_db(argv, db)?;
    let cli = datum_parse::parse_argv(&expanded)?;
    dispatch(cli, db)
}

pub fn dispatch(cli: Cli, db: &Db) -> Result<()> {
    use crate::commands;
    match cli.command {
        Command::Occur(args) => run_log_with_alias(args, db, commands::occur::occur),
        Command::Start(args) => run_log_with_alias(args, db, commands::start::start),
        Command::Stop(args) => run_log_with_alias(args, db, commands::end::stop),
        Command::Switch(args) => run_log_with_alias(args, db, commands::switch::switch),
        Command::Add(args) => run_log_with_alias(args, db, commands::add::add),
        Command::Tail(args) => commands::tail::tail(args, db),
        Command::Head(args) => commands::head::head(args, db),
        Command::Now(args) => commands::now::now(args, db),
        Command::Get(args) => commands::get::get(args, db),
        Command::Delete(args) => commands::del::del(args, db),
        Command::Edit(args) => commands::edit::edit(args, db),
        Command::Update(args) => commands::update::update(args, db),
        Command::Merge(args) => commands::merge::merge(args, db),
        Command::Rekey(args) => commands::rekey::rekey(args, db),
        Command::Retime(args) => commands::retime::retime(args, db),
        Command::Undo(args) => commands::undo::undo(args, db),
        Command::History(args) => commands::history::history(args, db),
        Command::Why(args) => commands::why::why(args, db),
        Command::Spec(sub) => commands::spec::spec(sub, db),
        Command::Alias(sub) => commands::alias::alias(sub, db),
        Command::Kit(sub) => commands::kit::kit(sub, db),
        Command::Track(args) => commands::track::track(args, db),
        Command::Ignore(args) => commands::ignore::ignore(args, db),
        Command::Grep(args) => commands::grep::grep(args, db),
        Command::Check(args) => commands::check::check(args, db),
        Command::Count(args) => commands::count::count(args, db),
        Command::Ids(args) => commands::ids::ids(args, db),
        Command::Dayview(args) => commands::dayview::dayview(args, db),
        Command::Nowview(args) => commands::nowview::nowview(args, db),
        Command::View(args) => commands::view::view(args, db),
        Command::Info(args) => commands::info::info(args, cli.db.as_deref(), db),
        Command::Db(sub) => commands::db::dispatch(sub, db),
        Command::Config(sub) => commands::config::config(sub, cli.db.as_deref()),
        Command::Sync(args) => commands::sync::dispatch(args, db),
        Command::Conflicts(sub) => commands::conflicts::conflicts(sub, db),
    }
}

/// Wrap a logging-verb dispatcher so that `--alias <NAME>` (and
/// `--alias-overwrite`) on the same invocation is honoured: the inner
/// command runs first, then on success the canonical argv form is
/// persisted to the alias table via
/// [`crate::commands::alias::record_inline_alias`].
///
/// The raw pre-expansion argv must be captured *before* `inner` runs,
/// because `log_entry` consumes the [`crate::args::IntentContext`] off
/// the thread-local slot when stamping provenance onto the entry.
fn run_log_with_alias(
    args: LogArgs,
    db: &Db,
    inner: fn(LogArgs, &Db) -> Result<()>,
) -> Result<()> {
    let alias_name = args.alias_name.clone();
    let alias_overwrite = args.alias_overwrite;
    let raw_argv = crate::args::peek_intent_context()
        .map(|ctx| ctx.raw_argv)
        .unwrap_or_default();
    inner(args, db)?;
    crate::commands::alias::record_inline_alias(alias_name, alias_overwrite, &raw_argv, db)
}

fn load_registry(db: &Db) -> Result<datum_core::field_spec::SpecRegistry> {
    Ok(db.load_registry()?)
}

/// Convenience wrapper that swallows registry-load errors (the renderer
/// treats `None` as "no registry available"). Exposed for sibling modules
/// in `commands/` that don't want to import the private `load_registry`.
pub(crate) fn load_registry_opt(db: &Db) -> Option<datum_core::field_spec::SpecRegistry> {
    load_registry(db).ok()
}

// ─── Ids ─────────────────────────────────────────────────────────────────────

/// Parse `--since` into a UTC lower bound.
///
/// Grammar (tried in order):
///   1. RFC3339 datetime, e.g. `2024-03-16T00:00:00Z`
///   2. `YYYY-MM-DD` — interpreted as local midnight
///   3. Duration offset (`1d`, `30m`, `2h`, `PT5M`) — treated as "N ago"
pub(crate) fn parse_since(s: &str) -> Result<chrono::DateTime<Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let local_midnight = Local
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .context("ambiguous local midnight")?;
        return Ok(local_midnight.with_timezone(&Utc));
    }
    let dur = parse_duration(s)
        .map_err(|e| anyhow::anyhow!("invalid --since '{s}': {e}"))?;
    Ok(chrono::Utc::now() - dur)
}

pub fn compute_ids(args: &IdsArgs, db: &Db) -> Result<Vec<String>> {
    let since_utc = args.since.as_deref().map(parse_since).transpose()?;
    Ok(db.list_entry_ids(
        args.field.as_deref(),
        since_utc,
        args.limit,
        args.oldest_first,
        args.slug,
    )?)
}

pub(crate) fn cmd_ids(args: IdsArgs, db: &Db) -> Result<()> {
    let ids = compute_ids(&args, db)?;
    if args.json {
        let arr: Vec<serde_json::Value> =
            ids.iter().map(|s| serde_json::Value::String(s.clone())).collect();
        println!("{}", serde_json::Value::Array(arr));
    } else {
        for id in ids {
            println!("{id}");
        }
    }
    Ok(())
}

// ─── setup ───────────────────────────────────────────────────────────────────

pub(crate) fn cmd_setup(_args: SetupArgs, db: &Db) -> Result<()> {
    let target = datum_core::migrations::CURRENT_SCHEMA_VERSION;
    let current = datum_core::migrations::read_schema_version(db.conn())?
        .unwrap_or(target);
    if current > target {
        anyhow::bail!(
            "setup: database schema v{current} is newer than this binary (v{target}). \
             Upgrade datum to a version that supports v{current}."
        );
    }
    let applied = datum_core::migrations::apply_migrations(
        db.conn(),
        datum_core::migrations::MIGRATIONS,
        current,
        target,
    )?;
    let entries = db.count_entries().unwrap_or(0);
    println!(
        "setup: schema v{current}{}",
        if applied.is_empty() {
            " (up to date)".to_string()
        } else {
            format!(" → v{target}")
        }
    );
    if !applied.is_empty() {
        println!("  applied: {}", applied.join(", "));
    }
    println!("  entries: {entries} (unchanged)");
    println!("  OK");
    Ok(())
}

// ─── Config ──────────────────────────────────────────────────────────────────

pub(crate) fn cmd_config(sub: ConfigCommand, cli_db: Option<&str>) -> Result<()> {
    match sub {
        ConfigCommand::Show(args) => cmd_config_show(args, cli_db),
    }
}

pub(crate) fn cmd_config_show(args: ConfigShowArgs, cli_db: Option<&str>) -> Result<()> {
    let cfg = config::Config::load(cli_db)?;

    if args.json {
        let rows = cfg
            .rows()
            .into_iter()
            .map(|(k, v, s)| {
                (
                    k.to_string(),
                    serde_json::json!({
                        "value": v,
                        "source": s.as_str(),
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let out = serde_json::json!({
            "config": rows,
            "toml_path": cfg.toml_path.to_string_lossy(),
            "toml_loaded": cfg.toml_loaded,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let rows = cfg.rows();
    let key_w = rows.iter().map(|(k, _, _)| k.len()).max().unwrap_or(0).max(3);
    let val_w = rows
        .iter()
        .map(|(_, v, _)| v.len())
        .max()
        .unwrap_or(0)
        .max(5);
    println!(
        "{:<key_w$}  {:<val_w$}  {}",
        "KEY",
        "VALUE",
        "SOURCE",
        key_w = key_w,
        val_w = val_w,
    );
    for (k, v, s) in rows {
        println!(
            "{:<key_w$}  {:<val_w$}  {}",
            k,
            v,
            s.as_str(),
            key_w = key_w,
            val_w = val_w,
        );
    }
    println!();
    println!(
        "toml: {} ({})",
        cfg.toml_path.display(),
        if cfg.toml_loaded { "loaded" } else { "not found" }
    );
    Ok(())
}

// ─── Logging ─────────────────────────────────────────────────────────────────

pub(crate) fn log_entry(action: Action, mut args: LogArgs, db: &Db) -> Result<()> {
    // -T/--no-timestamp: skip time-flag computation but keep the action's
    // own state-canonicalization rules (so `switch ... -T` still preserves
    // its positional state, `start -T` still records state=true, etc.).
    // Time-specifying flags are rejected so silent discard can't mask a
    // user misconception. The schema's NOT NULL `occur_time` is filled in
    // at insert time with `create_time` as a stub, and the slug carries
    // `:unset` so timeless entries remain identifiable downstream.
    if args.time.omit_timestamp {
        check_no_time_flags(&args)?;
    }

    // da-ffp6: For switch, only the first bare positional is the state;
    // subsequent bare positionals are reserved tokens (currently: duration).
    // Multi-state must be opt-in via comma-separated tokens (`a,b`), never
    // whitespace. Pull any trailing duration positional into args.duration
    // before handle_data_args sees it.
    if matches!(action, Action::Switch) {
        extract_switch_trailing_reserved(&mut args)?;
    }

    // For switch, walk the bare positionals and reject interior `,,`
    // empty segments before handle_data_args runs. Datum's `infer_type`
    // turns a leading-comma string into an array (`,a,b` → `["a","b"]`)
    // — that coercion is desired (da-lvvp / da-5z01) — but an interior
    // `a,,b` inside an edge-comma token would silently become a `null`
    // array element without this pre-check.
    if matches!(action, Action::Switch) {
        for token in &args.rest {
            if token.contains('=') {
                continue;
            }
            validate_switch_state_token(token)?;
        }
    }

    let parsed = parse_log_args(&args, action)?;
    let ParsedLogArgs {
        field,
        value,
        mut extra_data,
        slug_values,
    } = parsed;

    // da-5ns.3: route reserved column-bound keys (`state=`, `dur=`,
    // `following_state=`, `occur_time=`) out of the data blob.
    // Unknown keys (including `value=`) stay in the data payload (da-53rz).
    let reserved = extract_reserved_columns(&mut extra_data)?;

    // da-5ns.3 strict positional gate (Action::Add only): the v=20 model
    // expects `state=<v>` for the primary state. A bare unkeyed
    // positional with no `-K` slot binding has nowhere to land — the
    // legacy primary-value column is gone. Reject so a stale `dm add
    // field 80` doesn't silently lose data.
    if matches!(action, Action::Add) && value.is_some() && args.slug_keys.is_empty() {
        anyhow::bail!(
            "unbound positional value: `dm add` requires `state=<v>` (or a `-K` slot \
             binding) — the v=20 schema dropped the implicit `value` column"
        );
    }

    // Occur is a point-in-time event with no state. parse_log_args returns
    // value=None for occur (its remainder bucket is disabled), so any
    // positional that didn't fill a `-k`/`-K` slot already errored out of
    // handle_data_args via ExtraData. This guard catches the residual
    // case where a `state=<v>` reserved-column key was set on occur — a
    // command-level mistake we want to surface, not silently insert.
    if matches!(action, Action::Occur) && reserved.state.is_some() {
        anyhow::bail!(
            "`dm occur` doesn't carry state — drop the `state=<v>` key and \
             use `dm switch` (or `start`/`stop`) for state-changing entries"
        );
    }

    // S12 (da-u4k.12): for `switch`, a comma-separated value string is split
    // into a JSON array of concurrent states (e.g. `switch f a,b` →
    // `["a","b"]`). Single states stay scalar. Applied post-parse so the
    // slot/value handling in `parse_log_args` stays verb-agnostic.
    let value = if matches!(action, Action::Switch) {
        expand_array_states(value)?
    } else {
        value
    };

    // `-u`/`--undo`: instead of inserting, locate the most recent live
    // entry whose (field, state) matches what this command would have
    // produced and soft-delete it. Restores original datum's `uu`-alias
    // muscle memory (re-run the last command with `-u` appended → undo).
    // `state=<v>` from reserved columns wins over the bare positional
    // value, mirroring the precedence used at insert below.
    if args.undo {
        let undo_state = reserved.state.as_ref().or(value.as_ref());
        let target = db.find_latest_for_undo(&field, undo_state)?.ok_or_else(|| {
            anyhow::anyhow!("no matching entry to undo for field '{field}'")
        })?;
        let registry = load_registry(db).ok();
        let output = args.output();
        db.soft_delete_entry(&target.slug)?;
        show_delete(&target, registry.as_ref(), &output);
        let _ = datum_core::last_docs::set_last_docs(db, &[target.hid.clone()]);
        return Ok(());
    }

    // `entry.unit` is no longer settable from the log path — `-u` is now
    // the undo flag (handled above). The schema column survives for
    // FieldSpec-derived defaults and import paths; pass `None` here.
    let unit: Option<String> = None;
    let device = device_id()?;

    // Compute time_start, time_end, precision, and (for Action::Add) a shape
    // override. Skipped entirely for -T — a timeless entry carries no times
    // and no precision metadata.
    let (time_start, time_end, base_precision, add_override_shape) = if args.time.omit_timestamp {
        (None, None, None, None)
    } else {
        let (base_time, base_precision) = build_time_with_precision(&args)?;

        // Action::Add is the canonical shape-inferring verb. It peeks for
        // `start_time=` / `end_time=` / `dur=` tokens in extra_data, strips
        // them out, and derives the entry's shape + start/end from their
        // combination.
        let add_override: Option<AddTimestampResult> = if matches!(action, Action::Add) {
            let hints = extract_add_temporal_keys(&mut extra_data, &base_time)?;
            Some(hints.resolve(&base_time))
        } else {
            None
        };

        // --last-state on Stop: close the most recent open block, recording
        // its prior state as `last_state` metadata. Errors if there's nothing
        // to close. S6: the flag is value-accepting (`-L=<v>`); any `Some(_)`
        // form triggers the close-most-recent behavior. A provided value is
        // carried onto the entry's `user_asserted_last_state` for the
        // post-write mismatch detector.
        if args.last_state.is_some() && matches!(action, Action::Stop) {
            match db.find_open_block(&field, base_time.utc)? {
                None => anyhow::bail!(
                    "No open block for '{field}' to close with --last-state"
                ),
                Some(block) => {
                    let last_state = block.value.clone().unwrap_or(JsonValue::Bool(true));
                    let mut obj = match extra_data.take() {
                        Some(JsonValue::Object(m)) => m,
                        _ => serde_json::Map::new(),
                    };
                    obj.insert("last_state".to_string(), last_state);
                    extra_data = Some(JsonValue::Object(obj));
                }
            }
        }

        let (time_start, time_end) = if let Some(result) = &add_override {
            (result.start_time.clone(), result.end_time.clone())
        } else if args.duration.moment && matches!(action, Action::Switch) {
            // --moment on switch: point-in-time change (time_start == time_end)
            (Some(base_time.clone()), Some(base_time))
        } else if let Some(dur_str) = &args.duration.duration {
            let dur = parse_duration(dur_str)?;
            match action {
                Action::Start => {
                    // start --dur: time_start=base, time_end=base+dur
                    let end_utc = base_time.utc + dur;
                    let time_end = DatumTime { utc: end_utc, ..base_time.clone() };
                    (Some(base_time), Some(time_end))
                }
                Action::Stop => {
                    // stop --dur: time_end=base, time_start=base-dur
                    let start_utc = base_time.utc - dur;
                    let time_start = DatumTime { utc: start_utc, ..base_time.clone() };
                    (Some(time_start), Some(base_time))
                }
                _ => {
                    // occur/switch/add/block with --dur: set time_end=time_start+dur
                    let end_utc = base_time.utc + dur;
                    let time_end = DatumTime { utc: end_utc, ..base_time.clone() };
                    (Some(base_time), Some(time_end))
                }
            }
        } else {
            (Some(base_time), None)
        };

        let override_shape = add_override.map(|r| r.shape);
        (time_start, time_end, Some(base_precision), override_shape)
    };

    // da-8y2q.2: Entry::new_legacy takes a storage Verb but the value is
    // immediately overwritten by `derive_verb_for_storage` at insert time.
    // Pass `Verb::Point` as a neutral placeholder; the action-driven state
    // canonicalization below seeds the (state, dur, fs) triple and the
    // storage layer derives the canonical verb.
    let mut entry = Entry::new_legacy(field, Verb::Point, value, extra_data, unit, time_start, time_end, device);

    // For Action::Add, the shape was inferred from temporal hints; override the
    // shape-inference performed by `Entry::new` so `entry_shape` reflects the
    // add-specific classification (e.g. `ongoing` when only start_time is set).
    if let Some(shape) = add_override_shape {
        entry.entry_shape = shape;
    }

    // da-5ns.3: stamp the reserved columns extracted upstream onto the
    // entry. `state=` overrides the value-derived state assignment from
    // `Entry::new_legacy`; `dur=` and `following_state=` overwrite the
    // legacy time-shape derivation. The storage layer's `derive_verb`
    // call then classifies the resulting (state, dur, fs) triple.
    if let Some(s) = reserved.state {
        entry.state = Some(s.clone());
        entry.value = Some(s);
    }
    if let Some(d) = reserved.dur {
        entry.dur = Some(d);
    }
    if let Some(fs) = reserved.following_state {
        entry.following_state = Some(fs);
    }
    let _ = reserved.occur_time;

    // da-8y2q.2: Action-driven state canonicalization (moved out of
    // `derive_verb_for_storage`). With no explicit `state=`: `Action::Stop`
    // means "the field has ended" → false; `Action::Start` and
    // `Action::Switch` mean "transition to active" → true. The storage
    // layer (`derive_verb_for_storage`) keeps the Verb::Switch promotion as
    // a backstop so that direct `EntryBuilder` writes hit the same rule
    // when their verb is already set to Switch/BlockSwitch/PointSwitch.
    if entry.state.is_none() {
        match action {
            Action::Stop => {
                let v = JsonValue::Bool(false);
                entry.state = Some(v.clone());
                entry.value = Some(v);
            }
            Action::Start | Action::Switch => {
                let v = JsonValue::Bool(true);
                entry.state = Some(v.clone());
                entry.value = Some(v);
            }
            _ => {}
        }
    }

    // S6: carry an explicit `-L/--last-state=<v>` assertion onto the entry.
    // Bare `-L` (no value) is a no-op assertion-wise; only a supplied value
    // enters the column. Parsed through the same token parser that handles
    // positional switch values so `null` / `true` / numbers normalize
    // consistently with state compilation downstream.
    if let Some(raw) = args.last_state.as_deref() {
        if !raw.is_empty() {
            // S12: `-L=a,b` asserts the prior state was a concurrent array.
            entry.user_asserted_last_state = Some(parse_switch_state_token(raw)?);
        }
    }

    // Persist the effective precision derived from the -q stack and any
    // explicit `±N` uncertainty. The end_time precision mirrors start's
    // since both times flow from the same parse.
    if let Some(precision) = base_precision {
        entry.start_time_precision = precision.seconds;
        entry.start_time_precision_unit = precision.unit;
        if entry.end_time.is_some() {
            entry.end_time_precision = precision.seconds;
            entry.end_time_precision_unit = precision.unit;
        }
    }

    if args.no_metadata {
        entry.create_time = chrono::DateTime::UNIX_EPOCH;
        entry.modify_time = chrono::DateTime::UNIX_EPOCH;
        entry.device_id.clear();
    }

    // Stamp provenance from the CLI pre-pass. `take_` clears the slot so the
    // same invocation's context isn't reused for a subsequent entry in the
    // same process (matters for tests that run multiple dispatches).
    if let Some(ctx) = crate::args::take_intent_context() {
        entry.raw_input = ctx.raw_input.clone();
        entry.alias_chain = ctx.alias_chain_json();
    }

    // Slug precedence:
    //   1. `-K` slug-keys (explicit per-invocation id chain)
    //   2. `--id` template
    //   3. spec's `id_template`
    //   4. default `<field>:<timestamp>` from Entry::new
    if !slug_values.is_empty() {
        entry.slug = datum_core::slug::derive(
            &entry.field,
            &slug_values,
            entry.start_time.as_ref().or(entry.end_time.as_ref()),
        );
    } else {
        let spec_template = db
            .load_spec(&entry.field)?
            .and_then(|s| s.id_template);
        let template = args.id.clone().or(spec_template);
        if let Some(tmpl) = template {
            let uid = entry.short_uid();
            let ctx = InterpolationContext::from_entry(&entry, &uid);
            entry.slug = datum_core::ids::Slug(field_spec::interpolate(&tmpl, &ctx, &args.id_delimiter));
            entry.id_structure = Some(tmpl);
        }
    }

    // da-8y2q.2: derive the storage verb up-front so the lead/trail-state
    // capture (which dispatches on `entry.verb`) sees the same shape the
    // insert path will write. Without this, `capture_lead_state` would key
    // off the `Verb::Point` placeholder set by `Entry::new_legacy` and
    // skip state-bearing rows.
    if let Ok((derived_verb, _)) = datum_core::derive_verb(
        entry.state.clone(),
        entry.dur.as_deref(),
        entry.following_state.clone(),
    ) {
        entry.verb = derived_verb;
    }

    // Capture render hints *before* the insert:
    //   1. first-time: no live entries exist for this field yet. Drives both
    //      the `∅` left-char and the field-badge sparkle in the renderer.
    //   2. prior_state: for state-changing verbs, fetch the active state at
    //      `time_start` minus one microsecond. That's the "before" half of
    //      the transition indicator. A no-op prior (Null/False) means there
    //      was nothing active; the renderer skips the indicator in that case.
    let is_first_occurrence = db.count_entries_filtered(Some(&entry.field), None)? == 0;
    let lead_state = capture_lead_state(db, &entry);

    let strategy = args.conflict_strategy();
    let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
    let registry = load_registry(db).ok();
    let output = args.output();
    let ongoing_state = capture_ongoing_state(db, &entry);
    let trail_state = capture_trail_state(db, &entry);
    let ctx = RenderContext {
        lead_state,
        trail_state,
        is_first_occurrence,
        ongoing_state,
    };
    match &outcome {
        InsertOutcome::Inserted { slug, .. } => {
            let fresh = db.get_entry(slug).unwrap_or_else(|_| entry.clone());
            show_create_with_context(&fresh, registry.as_ref(), &ctx, &output);
        }
        InsertOutcome::Updated { slug } => {
            let fresh = db.get_entry(slug).unwrap_or_else(|_| entry.clone());
            show_update_with_context(&fresh, registry.as_ref(), &ctx, &output);
        }
        InsertOutcome::Merged { slug } => {
            let fresh = db.get_entry(slug).unwrap_or_else(|_| entry.clone());
            show_update_with_context(&fresh, registry.as_ref(), &ctx, &output);
        }
        InsertOutcome::Rekeyed { new_slug, old_slug, .. } => {
            show_rename(old_slug, new_slug, &output);
            if let Ok(fresh) = db.get_entry(new_slug) {
                show_create_with_context(&fresh, registry.as_ref(), &ctx, &output);
            }
        }
        InsertOutcome::Ignored { slug } => {
            if let Ok(existing) = db.get_entry(slug) {
                show_exists(&existing, registry.as_ref(), &output);
            } else if !output.silent() {
                eprintln!("⚠ Ignored 1 — duplicate of {slug}");
            }
        }
    }

    // Stamp the last-accessed pointer at the affected entry's hid so a
    // follow-up bare `get` / `delete` / `update` can target it. Failure
    // to write the pointer must not fail the user-visible insert.
    if let Some(slug) = match &outcome {
        InsertOutcome::Inserted { slug, .. }
        | InsertOutcome::Updated { slug }
        | InsertOutcome::Merged { slug }
        | InsertOutcome::Ignored { slug } => Some(slug.as_str()),
        InsertOutcome::Rekeyed { new_slug, .. } => Some(new_slug.as_str()),
    } {
        if let Ok(fresh) = db.get_entry(slug) {
            let _ = datum_core::last_docs::set_last_docs(db, &[fresh.hid]);
        }
    }

    Ok(())
}

/// Per da-fck: capture the field's compiled state at `entry.start_time -
/// ε` for the LEAD slot of the state-transition group. Returns the raw
/// `last_state_at` output — including `Bool(false)` and the IGNORED
/// sentinels — so the renderer can decide between painting a state colour
/// and falling back to the empty-slot grey.
///
/// Restricted to state-changing verbs (switch / start / stop / block).
/// Other verbs ignore the LEAD slot and this returns `None`.
fn capture_lead_state(db: &Db, entry: &Entry) -> Option<JsonValue> {
    if !matches!(
        entry.verb,
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::PointSwitch
    ) {
        return None;
    }
    let at = entry.start_time.as_ref()?.utc;
    let probe = at - chrono::Duration::microseconds(1);
    datum_core::state::last_state_at(db.conn(), &entry.field, probe, Some(&entry.hid))
        .ok()
        .flatten()
}

/// Per da-fck: capture the trail-state for the TRAIL slot.
///
/// * `Block` shapes → `last_state_at(end_time, exclude=row)` — the state
///   that resumes after the block closes.
/// * `Point`/`Ongoing` → derive from the entry alone (no DB hit) via
///   [`render::action::entry_self_trail_state`]; the row's own emitted
///   state continues forward.
fn capture_trail_state(db: &Db, entry: &Entry) -> Option<JsonValue> {
    if !matches!(
        entry.verb,
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::PointSwitch
    ) {
        return None;
    }
    if matches!(
        entry.entry_shape,
        datum_core::entry::EntryShape::Block
    ) {
        let end = entry.end_time.as_ref()?.utc;
        return datum_core::state::last_state_at(db.conn(), &entry.field, end, Some(&entry.hid))
            .ok()
            .flatten();
    }
    crate::render::action::entry_self_trail_state(entry)
}

/// Parsed LogArgs — distinguishes the slug-contributing id-key values from
/// the rest of the extra_data payload so callers can build the slug from
/// `-K` slots without re-walking the argv.
struct ParsedLogArgs {
    field: String,
    value: Option<JsonValue>,
    extra_data: Option<JsonValue>,
    /// Values captured from `-K` slots, in declaration order. Empty when
    /// no slug-contributing slot was filled.
    slug_values: Vec<String>,
}

/// Sentinel remainder-bucket key. Holds positionals that don't fill a
/// declared `-K`/`-k` slot — extracted post-call as the entry's primary
/// `value`. The `__datum_*` prefix avoids collision with any user key.
const PRIMARY_VALUE_KEY: &str = "__datum_primary_value";

/// Parse LogArgs into a structured bundle — field, primary value, extra_data,
/// and the list of `-K` slug-key values for slug derivation.
///
/// Delegates the data-assembly work (slot fill, `KEY=VALUE` positionals,
/// suffix coercion for `_time`/`_date`/`_dur` keys, comment-list shaping)
/// to [`datum_parse::input::data_args::handle_data_args`]. The wrapper
/// here owns the daydumb-specific concerns: peeling the state positional
/// for state-bearing verbs (Switch/Start/Stop) so it doesn't compete with
/// `-k` slots, and pulling slug values out of the resolved [`DatumData`].
fn parse_log_args(args: &LogArgs, action: Action) -> Result<ParsedLogArgs> {
    use datum_parse::input::data_args::{handle_data_args, DataArgs};

    let field = args.field_or_verb.clone();

    let mut data_args = DataArgs::default();
    // Daydumb compat: flat paths (no `state.id` synthesis from `state` /
    // `.x` keys) and literal `"null"` / `"undefined"` switch values. See
    // the `DataArgs` field docs for why we diverge from datum's TS port.
    data_args.flat_paths = true;
    data_args.preserve_null_strings = true;
    let mut slug_paths: Vec<String> = Vec::new();
    for spec in &args.slug_keys {
        let (name, _) = spec.split_once('=').map(|(n, v)| (n, Some(v))).unwrap_or((spec.as_str(), None));
        if name.is_empty() {
            anyhow::bail!("empty id-key name in spec '{spec}'");
        }
        slug_paths.push(canonical_data_path(name));
        data_args.push_id_key(spec.clone());
    }

    // `-k <SPEC>` becomes a slot-with-default declaration to handle_data_args.
    // `-k KEY` is a required slot, `-k KEY=` an omit-if-absent slot, and
    // `-k KEY=VALUE` a slot whose default is `VALUE` — overridable by the
    // next positional. This mirrors original datum's `parseDataArgs`; the
    // earlier "immediate" hack (route `-k KEY=VALUE` straight into the
    // data queue, never consume a positional) silently dropped positionals
    // into the primary-value remainder, which then leaked into `state`
    // for non-state-bearing verbs like `occur`.
    for spec in &args.keys {
        let (name, _) = spec.split_once('=').map(|(n, v)| (n, Some(v))).unwrap_or((spec.as_str(), None));
        if name.is_empty() {
            anyhow::bail!("empty id-key name in spec '{spec}'");
        }
        data_args.keys.push(spec.clone());
    }

    for c in &args.comment {
        data_args.comment.push(c.clone());
    }

    // For state-bearing verbs (Switch/Start/Stop), peel the FIRST bare
    // positional out of the data queue and remember it as the state value.
    // This keeps the state from competing with `-k` slot-fill — a switch
    // user writing `switch project household -k optional=50` wants the
    // state to be `household` and `optional` to default to `50`, not for
    // `household` to consume the optional slot.
    //
    // `KEY=VALUE` positionals are not state candidates; they go through
    // handle_data_args as explicit assignments regardless of action.
    let state_bearing = matches!(
        action,
        Action::Switch | Action::Start | Action::Stop
    );
    let mut state_positional: Option<String> = None;
    for pos in &args.rest {
        if state_bearing && state_positional.is_none() && !pos.contains('=') {
            state_positional = Some(pos.clone());
        } else {
            data_args.data.push(pos.clone());
        }
    }

    // Remainder bucket policy:
    //   * `-R`/`--remainder` → user-named bucket.
    //   * `-C`/`--comment-remainder` → "comment" bucket.
    //   * Action::Occur → no remainder; an unbound positional is an error
    //     (occur doesn't carry state, has no `value` column to absorb it,
    //     and silently routing to extra_data would mask user mistakes).
    //   * Otherwise → PRIMARY_VALUE_KEY sentinel; for state-bearing verbs
    //     this only catches *extra* positionals after the state was peeled.
    let user_remainder = args.remainder.is_some() || args.comment_remainder;
    data_args.remainder = if let Some(r) = &args.remainder {
        Some(r.clone())
    } else if args.comment_remainder {
        Some("comment".to_string())
    } else if matches!(action, Action::Occur) {
        None
    } else {
        Some(PRIMARY_VALUE_KEY.to_string())
    };
    data_args.string_remainder = args.string_remainder;
    data_args.comment_remainder = args.comment_remainder;

    let mut datum_data = handle_data_args(&data_args).map_err(anyhow::Error::from)?;

    // Value extraction: state positional wins for state-bearing verbs;
    // otherwise pull from the PRIMARY_VALUE_KEY remainder. When the user
    // redirected the remainder via -R/-C, value is intentionally empty.
    let value = if let Some(sp) = state_positional {
        // Discard any extra unbound positionals that landed in the
        // primary-value bucket; the state already came from `sp`.
        // Use the same token-coercion path that powers switch state
        // assertions (-L) so "null" stays a string per daydumb's
        // preserve_null_strings policy and unit-annotated numbers like
        // "75.2kg" round-trip the same way as keyed values.
        datum_data.remove(PRIMARY_VALUE_KEY);
        Some(parse_value_token(&sp))
    } else if user_remainder {
        None
    } else {
        datum_data.remove(PRIMARY_VALUE_KEY)
    };

    // Slug values: the resolved value at each `-K` slot path, in
    // declaration order. Slots that resolved to nothing (e.g. `-K key=`
    // with no positional) contribute the empty string — matching legacy
    // `slug_values.push(... .unwrap_or_default())`.
    let mut slug_values: Vec<String> = Vec::with_capacity(slug_paths.len());
    for path in &slug_paths {
        slug_values.push(datum_value_to_slug_string(get_path_value(&datum_data, path)));
    }

    // `id=<name>` shorthand: when no `-K` slug-keys were declared, a
    // positional `id=...` token is promoted to the slug identity. The
    // value stays in extra_data only as a side effect — pre-existing
    // behaviour stripped it from the payload, so we do the same here.
    if slug_values.is_empty() {
        if let Some(id_value) = datum_data.remove("id") {
            slug_values.push(datum_value_to_slug_string(Some(&id_value)));
        }
    }

    let extra_data = if datum_data.is_empty() {
        None
    } else {
        Some(JsonValue::Object(datum_data))
    };

    Ok(ParsedLogArgs { field, value, extra_data, slug_values })
}

/// Translate a `-K`/`-k` raw key name to the path `handle_data_args`
/// writes to. With daydumb's `flat_paths` compat (set in `parse_log_args`),
/// the path is taken verbatim — no `state` / leading-dot synthesis.
fn canonical_data_path(raw: &str) -> String {
    raw.to_string()
}

/// Resolve a slot path within `datum_data` for slug derivation. Daydumb's
/// flat-paths compat stores `-K my.nested` as the literal key `my.nested`,
/// so try the literal lookup first before walking dotted segments.
fn get_path_value<'a>(
    obj: &'a serde_json::Map<String, JsonValue>,
    path: &str,
) -> Option<&'a JsonValue> {
    if let Some(v) = obj.get(path) {
        return Some(v);
    }
    let mut segs = path.split('.');
    let first = segs.next()?;
    let mut cur = obj.get(first)?;
    for seg in segs {
        cur = cur.as_object().and_then(|m| m.get(seg))?;
    }
    Some(cur)
}

/// Stringify a resolved `DatumData` value for slug derivation. Missing
/// or `null` slots collapse to `""`, matching the legacy
/// `slot_value.unwrap_or_default()` behaviour. String values pass through
/// without their JSON quoting; everything else uses serde's default
/// representation.
fn datum_value_to_slug_string(v: Option<&JsonValue>) -> String {
    match v {
        None | Some(JsonValue::Null) => String::new(),
        Some(JsonValue::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// da-ffp6: For `Verb::Switch`, partition `args.rest` so only the first
/// bare (non-`KEY=VALUE`) positional contributes to the state value.
/// Subsequent bare positionals are reserved tokens — currently any
/// `parse_duration`-compatible string — and feed into `args.duration`
/// rather than appending to the state list.
///
/// Multi-state remains opt-in via comma-separated tokens (`switch f a,b`);
/// whitespace-separated trailing positionals never silently become array
/// elements. A trailing bare positional that fails to parse as a duration
/// is rejected with a hint pointing at the comma form.
fn extract_switch_trailing_reserved(args: &mut LogArgs) -> Result<()> {
    let mut state_consumed = false;
    let mut new_rest: Vec<String> = Vec::with_capacity(args.rest.len());
    let mut trailing_duration: Option<String> = None;
    for token in args.rest.drain(..) {
        if token.contains('=') {
            new_rest.push(token);
            continue;
        }
        if !state_consumed {
            state_consumed = true;
            new_rest.push(token);
            continue;
        }
        if parse_duration(&token).is_err() {
            anyhow::bail!(
                "switch: trailing positional '{token}' is not a valid duration. \
                 Use comma-separated states for multi-state switch (e.g. `a,b`)."
            );
        }
        if trailing_duration.is_some() {
            anyhow::bail!(
                "switch: multiple trailing duration positionals; expected at most one"
            );
        }
        trailing_duration = Some(token);
    }
    args.rest = new_rest;
    if let Some(dur) = trailing_duration {
        if args.duration.duration.is_some() {
            anyhow::bail!(
                "switch: trailing duration positional conflicts with --dur"
            );
        }
        args.duration.duration = Some(dur);
    }
    Ok(())
}

/// S12: parse a switch value token, splitting on `,` into an array of
/// concurrent states. A single state (no comma) stays scalar. Edge
/// commas are stripped (da-lvvp / da-5z01: tolerate `,a,b`, `a,b,`,
/// `,a,b,`); interior empty segments are still rejected so users
/// don't silently lose a state they meant to type.
fn parse_switch_state_token(s: &str) -> Result<JsonValue> {
    validate_switch_state_token(s)?;
    if !s.contains(',') {
        return Ok(parse_value_token(s));
    }
    let trimmed = s.trim_matches(',');
    if trimmed.is_empty() {
        return Ok(JsonValue::Array(vec![]));
    }
    if !trimmed.contains(',') {
        return Ok(parse_value_token(trimmed));
    }
    let parts: Vec<JsonValue> = trimmed.split(',').map(parse_value_token).collect();
    Ok(JsonValue::Array(parts))
}

/// Reject interior empty segments (`a,,b`) in a switch state-list token.
/// Leading/trailing edge commas are tolerated and stripped — matching
/// `infer_type`'s edge-empty handling (da-lvvp). Surfaced separately
/// from [`parse_switch_state_token`] because `log_entry` runs the same
/// validation against raw `args.rest` tokens before [`handle_data_args`]
/// has a chance to coerce them into arrays via `infer_type`; without
/// this pre-check, an interior `,,` inside a leading-comma token would
/// silently become a `null` array element.
fn validate_switch_state_token(s: &str) -> Result<()> {
    if !s.contains(',') {
        return Ok(());
    }
    if s.trim_matches(',').contains(",,") {
        anyhow::bail!(
            "invalid state list '{s}': empty segment between commas"
        );
    }
    Ok(())
}

/// S12: for `Verb::Switch`, expand any comma-containing string in the
/// primary value into an array of states. Handles three inputs:
///   * scalar string → array if comma-separated, otherwise unchanged
///   * array (from legacy multi-positional switch) → flatten any
///     comma-containing string elements into their components
///   * all other value shapes → passthrough
fn expand_array_states(value: Option<JsonValue>) -> Result<Option<JsonValue>> {
    let Some(v) = value else { return Ok(None); };
    match v {
        JsonValue::String(s) if s.contains(',') => Ok(Some(parse_switch_state_token(&s)?)),
        JsonValue::Array(items) => {
            let mut out: Vec<JsonValue> = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    JsonValue::String(s) if s.contains(',') => {
                        match parse_switch_state_token(&s)? {
                            JsonValue::Array(inner) => out.extend(inner),
                            other => out.push(other),
                        }
                    }
                    other => out.push(other),
                }
            }
            Ok(Some(match out.len() {
                0 => JsonValue::Array(out),
                1 => out.into_iter().next().unwrap(),
                _ => JsonValue::Array(out),
            }))
        }
        other => Ok(Some(other)),
    }
}

/// Parse a single value token. Tries number, bool, then falls back to string.
fn parse_value_token(s: &str) -> JsonValue {
    // Unit-annotated number: "75.2kg", "500ml"
    if let Some((num, unit)) = split_numeric_unit(s) {
        if let Ok(n) = num.parse::<f64>() {
            return serde_json::json!({"v": n, "u": unit});
        }
    }
    if let Ok(n) = s.parse::<i64>() {
        return JsonValue::Number(n.into());
    }
    if let Ok(n) = s.parse::<f64>() {
        return serde_json::json!(n);
    }
    if s == "true" { return JsonValue::Bool(true); }
    if s == "false" { return JsonValue::Bool(false); }
    JsonValue::String(s.to_string())
}

/// Split "75.2kg" → Some(("75.2", "kg")), "hello" → None.
fn split_numeric_unit(s: &str) -> Option<(&str, &str)> {
    let idx = s
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == '-')
        .last()
        .map(|(i, _)| i + 1)?;
    if idx == 0 || idx == s.len() {
        return None;
    }
    let (num, unit) = s.split_at(idx);
    if unit.chars().all(|c| c.is_alphabetic()) {
        Some((num, unit))
    } else {
        None
    }
}

/// Temporal hints parsed out of extra_data during a `datum add` invocation.
/// Present fields drive shape selection; `dur_is_null` is a sentinel for the
/// explicit `dur=null` case (force-point).
#[derive(Debug, Default)]
struct AddTemporalHints {
    start_time: Option<DatumTime>,
    end_time: Option<DatumTime>,
    dur: Option<chrono::Duration>,
    dur_is_null: bool,
}

/// Shape + timestamp pair produced by resolving [`AddTemporalHints`].
#[derive(Debug, Clone)]
struct AddTimestampResult {
    start_time: Option<DatumTime>,
    end_time: Option<DatumTime>,
    shape: datum_core::entry::EntryShape,
}

impl AddTemporalHints {
    /// Apply the D3 decision table to the hints.
    ///
    /// Cases (shape | start | end):
    /// - `dur=null`                         → point | `base`          | None
    /// - no hints                           → point | `base`          | None
    /// - start only                         → ongoing | start          | None
    /// - end only                           → block | `base`           | end
    /// - start + end                        → block | start            | end
    /// - dur only                           → block | `base-dur`       | base
    /// - start + dur                        → block | start            | start+dur
    /// - end + dur                          → block | end-dur          | end
    fn resolve(&self, base_time: &DatumTime) -> AddTimestampResult {
        use datum_core::entry::EntryShape;
        let mk = |utc: chrono::DateTime<chrono::Utc>| DatumTime { utc, ..base_time.clone() };

        if self.dur_is_null {
            return AddTimestampResult {
                start_time: Some(base_time.clone()),
                end_time: None,
                shape: EntryShape::Point,
            };
        }

        match (self.start_time.clone(), self.end_time.clone(), self.dur) {
            (Some(s), Some(e), _) => AddTimestampResult {
                start_time: Some(s),
                end_time: Some(e),
                shape: EntryShape::Block,
            },
            (Some(s), None, Some(d)) => {
                let end = mk(s.utc + d);
                AddTimestampResult {
                    start_time: Some(s),
                    end_time: Some(end),
                    shape: EntryShape::Block,
                }
            }
            (Some(s), None, None) => AddTimestampResult {
                start_time: Some(s),
                end_time: None,
                shape: EntryShape::Ongoing,
            },
            (None, Some(e), Some(d)) => {
                let start = mk(e.utc - d);
                AddTimestampResult {
                    start_time: Some(start),
                    end_time: Some(e),
                    shape: EntryShape::Block,
                }
            }
            (None, Some(e), None) => AddTimestampResult {
                start_time: Some(base_time.clone()),
                end_time: Some(e),
                shape: EntryShape::Block,
            },
            (None, None, Some(d)) => {
                let start = mk(base_time.utc - d);
                AddTimestampResult {
                    start_time: Some(start),
                    end_time: Some(base_time.clone()),
                    shape: EntryShape::Block,
                }
            }
            (None, None, None) => AddTimestampResult {
                start_time: Some(base_time.clone()),
                end_time: None,
                shape: EntryShape::Point,
            },
        }
    }
}

/// Reserved column values pulled out of the data blob by
/// [`extract_reserved_columns`]. Each field is `None` when the user
/// did not pass the corresponding key.
#[derive(Debug, Default)]
pub(crate) struct ReservedColumns {
    pub(crate) state: Option<JsonValue>,
    pub(crate) dur: Option<String>,
    pub(crate) following_state: Option<JsonValue>,
    pub(crate) occur_time: Option<JsonValue>,
}

/// da-5ns.3: route the reserved column-bound keys (`state=`, `dur=`,
/// `following_state=`, `occur_time=`) out of the user's `data` payload
/// onto entry columns. Unknown keys (including `value=`) fall through
/// to the user-data payload — `value` is no longer a column post-da-5ns,
/// so `value=X` lands as `data.value = X` like any other user key (da-53rz).
///
/// `occur_time=` is captured but its value-shape grammar (RFC3339,
/// signed offsets, `now`) is left to callers to interpret — the
/// existing time-flag pipeline already handles common cases via
/// `-t/--time`; sub-bead 6 (import) drives the column-keyed form.
pub(crate) fn extract_reserved_columns(
    extra_data: &mut Option<JsonValue>,
) -> Result<ReservedColumns> {
    let mut out = ReservedColumns::default();
    let Some(JsonValue::Object(obj)) = extra_data.as_mut() else {
        return Ok(out);
    };
    if let Some(v) = obj.remove("state") {
        out.state = Some(v);
    }
    if let Some(v) = obj.remove("dur") {
        // `dur=null` (or string `"null"`) is a sentinel for "no
        // duration" — leave entry.dur as None rather than writing the
        // string "null" which derive_verb would reject.
        if v.is_null() || matches!(v.as_str(), Some("null")) {
            // intentionally drop — None signals point shape.
        } else {
            out.dur = Some(json_to_dur_string(&v)?);
        }
    }
    if let Some(v) = obj.remove("following_state") {
        out.following_state = Some(v);
    }
    if let Some(v) = obj.remove("occur_time") {
        out.occur_time = Some(v);
    }
    if obj.is_empty() {
        *extra_data = None;
    }
    Ok(out)
}

/// Coerce a parsed JSON value back to an ISO-8601 duration string. The
/// CLI parses `dur=PT1H` into `JsonValue::String("PT1H")`; the unit-
/// annotated forms (`dur=5m` → `{"v": 5, "u": "m"}`) get reconstructed
/// via [`json_to_raw_token`].
fn json_to_dur_string(v: &JsonValue) -> Result<String> {
    if let Some(s) = v.as_str() {
        return Ok(s.to_string());
    }
    Ok(json_to_raw_token(v))
}

/// Extract and remove `start_time=`, `end_time=`, `dur=` keys from the
/// extra_data payload for a `datum add` invocation. Returns the parsed
/// values; leaves extra_data with only the remaining kv pairs. If extra_data
/// becomes empty, it is reset to None.
fn extract_add_temporal_keys(
    extra_data: &mut Option<JsonValue>,
    base_time: &DatumTime,
) -> Result<AddTemporalHints> {
    let mut hints = AddTemporalHints::default();

    let obj = match extra_data.as_mut() {
        Some(JsonValue::Object(m)) => m,
        _ => return Ok(hints),
    };

    if let Some(v) = obj.remove("start_time") {
        hints.start_time = Some(parse_add_time_value(&v, base_time)?);
    }
    if let Some(v) = obj.remove("end_time") {
        hints.end_time = Some(parse_add_time_value(&v, base_time)?);
    }
    if let Some(v) = obj.remove("dur") {
        if v.is_null() || matches!(v.as_str(), Some("null")) {
            hints.dur_is_null = true;
        } else {
            hints.dur = Some(parse_duration(&json_to_raw_token(&v))?);
        }
    }

    if obj.is_empty() {
        *extra_data = None;
    }
    Ok(hints)
}

/// Undo `parse_value_token` for temporal keys: reconstruct the user's raw
/// token string from its parsed JSON form. Handles unit-annotated objects
/// (`{"v": 5, "u": "m"}` ← `5m`), numbers, and bare strings.
fn json_to_raw_token(v: &JsonValue) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(obj) = v.as_object() {
        if let (Some(num), Some(u)) = (obj.get("v"), obj.get("u")) {
            let num_s = if let Some(n) = num.as_i64() {
                n.to_string()
            } else if let Some(n) = num.as_f64() {
                n.to_string()
            } else {
                num.to_string()
            };
            let unit_s = u.as_str().unwrap_or("");
            return format!("{num_s}{unit_s}");
        }
    }
    if let Some(n) = v.as_i64() {
        return n.to_string();
    }
    if let Some(n) = v.as_f64() {
        return n.to_string();
    }
    v.to_string()
}

/// da-5ns.4: parse the reserved `occur_time=` payload from `dm update`. Same
/// grammar as `start_time=` / `end_time=`: "now", `±N{m,h,s}` offsets, or
/// RFC3339 timestamps. Exposed crate-private so [`update_core::apply_mutation`]
/// can reuse the parser.
pub(crate) fn parse_reserved_occur_time(v: &JsonValue, base: &DatumTime) -> Result<DatumTime> {
    parse_add_time_value(v, base)
}

/// Parse a `start_time=…` or `end_time=…` value token against a base time.
/// Accepts:
///  * "now" → `base_time`
///  * signed duration offsets like `-5m`, `+1h`, `-30s` → base ± offset
///  * RFC3339 timestamps → absolute
fn parse_add_time_value(v: &JsonValue, base: &DatumTime) -> Result<DatumTime> {
    let raw = json_to_raw_token(v);
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("now") {
        return Ok(base.clone());
    }
    // Try RFC3339 absolute.
    if let Ok(abs) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(DatumTime {
            utc: abs.with_timezone(&chrono::Utc),
            ..base.clone()
        });
    }
    // Otherwise treat as a signed duration offset.
    let (sign, body) = if let Some(rest) = s.strip_prefix('-') {
        (-1i64, rest)
    } else if let Some(rest) = s.strip_prefix('+') {
        (1i64, rest)
    } else {
        (1i64, s)
    };
    let dur = parse_duration(body)?;
    let delta = if sign < 0 { -dur } else { dur };
    let utc = base.utc + delta;
    Ok(DatumTime { utc, ..base.clone() })
}

/// Build the log entry's base time together with its effective precision
/// (implicit from `-q*` stacks or explicit from a `±N` uncertainty suffix).
/// Reject time-specifying flags when the caller passed `-T/--no-timestamp`.
/// A timeless entry has no place for time offsets or duration — silently
/// discarding them would mask a user misconception, so we bail with a list
/// of the offending flags.
fn check_no_time_flags(args: &LogArgs) -> Result<()> {
    let mut bad: Vec<&str> = Vec::new();
    if args.time.time.is_some() { bad.push("-t/--time"); }
    if args.time.date.is_some() { bad.push("-d/--date"); }
    if args.duration.duration.is_some() { bad.push("--dur"); }
    if args.time.timezone.is_some() { bad.push("-z/--timezone"); }
    if args.time.yesterday > 0 { bad.push("-y"); }
    if args.time.quick > 0 { bad.push("-q"); }
    if !bad.is_empty() {
        anyhow::bail!(
            "-T/--no-timestamp cannot be combined with time flags: {}",
            bad.join(", ")
        );
    }
    Ok(())
}

fn build_time_with_precision(args: &LogArgs) -> Result<(DatumTime, datum_core::time::Uncertainty)> {
    let tz_name = match args.time.timezone.as_deref() {
        Some(spec) => Some(
            resolve_timezone(spec)
                .map_err(|e| anyhow::anyhow!("Invalid -z/--timezone value '{spec}': {e}"))?,
        ),
        None => None,
    };

    let mut builder = TimeBuilder::now()
        .with_yesterday_count(args.time.yesterday as i64)
        .with_quick_count(args.time.quick as i64)
        .with_quick_precision(args.time.quick)
        .with_timezone(tz_name);

    if let Some(d) = &args.time.date {
        let da = parse_date_arg(d)?;
        builder = builder.with_date_arg(da);
    }
    if let Some(t) = &args.time.time {
        let ta = parse_time_arg(t)?;
        builder = builder.with_time_arg(ta);
    }

    Ok(builder.build_with_precision())
}

// ─── Tail / Now ──────────────────────────────────────────────────────────────

pub(crate) fn cmd_tail(args: TailArgs, db: &Db) -> Result<()> {
    let columns = split_columns(&args.columns);
    let metric: TimeMetricCol = args
        .metric
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    let entries = tail_entries(&args, db)?;
    let registry = load_registry(db).ok();
    let output = args.output();

    // Per da-6t0: tail/head always render through the tabular path so the
    // header row, column alignment, and `hid` column are on by default. The
    // legacy info-line branch is gone; `--table` is now a no-op flag kept
    // only for backward CLI compatibility.
    let tbl = TableArgs {
        output: output.clone(),
        metric,
        columns,
        suppress_header: false,
    };
    let now = DatumTime::now();
    let ctxs = build_render_contexts(&entries, db);
    print_table_with(&entries, Some(&ctxs), Some(&now), registry.as_ref(), &tbl);

    if let Some(secs) = args.watch {
        let interval = watch_interval(secs);
        let mut since = entries
            .iter()
            .map(|e| e.create_time)
            .max()
            .unwrap_or_else(chrono::Utc::now);
        let field = args.field.as_deref();
        let tick_tbl = TableArgs {
            suppress_header: true,
            ..tbl.clone()
        };
        loop {
            std::thread::sleep(interval);
            let new_rows = db.list_entries_created_after(field, since)?;
            if let Some(latest) = new_rows.iter().map(|e| e.create_time).max() {
                since = latest;
            }
            if !new_rows.is_empty() {
                let now = DatumTime::now();
                let tick_ctxs = build_render_contexts(&new_rows, db);
                print_table_with(
                    &new_rows,
                    Some(&tick_ctxs),
                    Some(&now),
                    registry.as_ref(),
                    &tick_tbl,
                );
            }
        }
    }

    Ok(())
}

/// Vector form of `build_render_context_cached` — one context per entry, in
/// the same order. Used by the table path so the canonical state-cell
/// renderer can emit transition arrows for tail rows.
///
/// Per da-uuu: builds a per-field [`FieldStateEvents`] cache up front so the
/// lead / trail / ongoing state probes run in memory instead of issuing one
/// (multi-row, all-columns) `last_state_at` query per entry. The previous
/// implementation re-fetched the same field's state history once per output
/// row, which dominated `dm tail` wall-clock — especially with `-yt`/`-d`
/// filters where the probe time is far enough from `now` to defeat the OS
/// page cache. The cache is scoped to this batch.
pub(crate) fn build_render_contexts(entries: &[Entry], db: &Db) -> Vec<RenderContext> {
    use std::collections::HashMap;
    // Bound the per-field load by the latest probe time we'll issue. Block
    // trail probes go to `end_time`; everything else to `start_time` (or
    // `create_time` for the rare state-affecting row that lacks a start).
    // Events strictly later than this max never affect any probe, so passing
    // it to the loader prunes the WHERE clause and skips work for histories
    // that extend far past this batch (`tail -yt 15` on a DB written to all
    // morning will skip every today-event).
    let max_probe: Option<chrono::DateTime<Utc>> = entries
        .iter()
        .filter(|e| needs_state_probe(e))
        .filter_map(probe_upper_bound)
        .max();
    let mut cache: HashMap<String, datum_core::state::FieldStateEvents> = HashMap::new();
    for e in entries {
        if !cache.contains_key(&e.field) && needs_state_probe(e) {
            let events = datum_core::state::FieldStateEvents::load_for_field_up_to(
                db.conn(),
                &e.field,
                max_probe,
            )
            .unwrap_or_default();
            cache.insert(e.field.clone(), events);
        }
    }
    entries
        .iter()
        .map(|e| build_render_context_cached(e, db, &cache))
        .collect()
}

/// Whether this entry's `RenderContext` needs any state-history probe at
/// all. `Add`/`Note` rows skip the cache load entirely.
fn needs_state_probe(entry: &Entry) -> bool {
    matches!(
        entry.verb,
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::PointSwitch | Verb::Point
    )
}

/// The latest probe time `build_render_context_cached` will issue for this
/// entry. Block rows probe at `end_time`; everything else at `start_time`,
/// falling back to `create_time` for state-affecting rows that lack a start.
fn probe_upper_bound(entry: &Entry) -> Option<chrono::DateTime<Utc>> {
    if matches!(entry.entry_shape, datum_core::entry::EntryShape::Block) {
        if let Some(end) = entry.end_time.as_ref() {
            return Some(end.utc);
        }
    }
    entry
        .start_time
        .as_ref()
        .map(|t| t.utc)
        .or(Some(entry.create_time))
}

/// Cached variant of the per-row render context build. Lead/trail/ongoing
/// probes hit the per-field event vector instead of re-querying the DB;
/// `is_first_occurrence` still goes to the DB but does so via a cheap count
/// against the field index — small relative to the state probes we replaced.
fn build_render_context_cached(
    entry: &Entry,
    db: &Db,
    cache: &std::collections::HashMap<String, datum_core::state::FieldStateEvents>,
) -> RenderContext {
    let events = cache.get(&entry.field);
    let lead_state = capture_lead_state_cached(events, entry);
    let trail_state = capture_trail_state_cached(events, entry);
    let probe = entry
        .start_time
        .as_ref()
        .map(|t| t.utc)
        .unwrap_or(entry.create_time);
    let is_first_occurrence = db
        .count_entries_before(&entry.field, probe)
        .unwrap_or(1)
        == 0;
    let ongoing_state = capture_ongoing_state_cached(events, entry);
    RenderContext {
        lead_state,
        trail_state,
        is_first_occurrence,
        ongoing_state,
    }
}

fn capture_lead_state_cached(
    events: Option<&datum_core::state::FieldStateEvents>,
    entry: &Entry,
) -> Option<JsonValue> {
    if !matches!(
        entry.verb,
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::PointSwitch
    ) {
        return None;
    }
    let at = entry.start_time.as_ref()?.utc;
    let probe = at - chrono::Duration::microseconds(1);
    events?.at(probe, Some(&entry.hid))
}

fn capture_trail_state_cached(
    events: Option<&datum_core::state::FieldStateEvents>,
    entry: &Entry,
) -> Option<JsonValue> {
    if !matches!(
        entry.verb,
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::PointSwitch
    ) {
        return None;
    }
    if matches!(entry.entry_shape, datum_core::entry::EntryShape::Block) {
        let end = entry.end_time.as_ref()?.utc;
        return events?.at(end, Some(&entry.hid));
    }
    crate::render::action::entry_self_trail_state(entry)
}

fn capture_ongoing_state_cached(
    events: Option<&datum_core::state::FieldStateEvents>,
    entry: &Entry,
) -> Option<JsonValue> {
    if !matches!(entry.verb, Verb::Point) {
        return None;
    }
    let at = entry.start_time.as_ref()?.utc;
    events?.at(at, Some(&entry.hid))
}

/// Per da-k5z: for occur rows, look up the field's compiled state at the
/// row's start_time excluding the row itself, so the dot's bg can encode
/// "is this point happening inside an ongoing block?". Returns `None` for
/// non-occur verbs and on any DB error (degrade to default-bg dot).
fn capture_ongoing_state(db: &Db, entry: &Entry) -> Option<JsonValue> {
    if !matches!(entry.verb, Verb::Point) {
        return None;
    }
    let at = entry.start_time.as_ref()?.utc;
    datum_core::state::last_state_at(db.conn(), &entry.field, at, Some(&entry.hid))
        .ok()
        .flatten()
}

/// Clamp a user-supplied `--watch` interval into a sane poll cadence.
/// Non-finite, zero, or negative values collapse to 1 s; anything positive
/// is honoured at millisecond granularity (so `--watch 0.25` → 250 ms).
fn watch_interval(secs: f64) -> std::time::Duration {
    if !secs.is_finite() || secs <= 0.0 {
        return std::time::Duration::from_secs(1);
    }
    let millis = (secs * 1000.0).round() as u64;
    std::time::Duration::from_millis(millis.max(1))
}

/// Run the tail query as `cmd_tail` does, but return the entries instead of
/// printing. Used by integration tests and by `head` when it routes through
/// the shared tail path.
pub fn tail_entries(args: &TailArgs, db: &Db) -> Result<Vec<Entry>> {
    let params = tail_params(args)?;
    Ok(db.list_entries(&params)?)
}

/// Split `--column` args (comma-splittable, repeatable) into a flat list.
pub fn split_columns(raw: &[String]) -> Vec<String> {
    raw.iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Translate [`TailArgs`] into [`ListParams`] for [`Db::list_entries`].
///
/// Window semantics (matching datum):
/// * No time args → unbounded (future entries included).
/// * `-d`/`-y` with no `-t`/`-q` → full local-day window centred on the
///   computed date, with a ±1-day pad to absorb timezone offsets; no default
///   limit (unless `-n` is set).
/// * `-t` (alone or with `-d`/`-y`) → upper-bound only (classic tail).
pub fn tail_params(args: &TailArgs) -> Result<ListParams> {
    let metric: TimeMetric = args
        .metric
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let has_date = args.time.date.is_some() || args.time.yesterday > 0;
    let has_time = args.time.time.is_some() || args.time.quick > 0;
    let is_date_only = has_date && !has_time;

    let (start_utc, end_utc) = if !has_date && !has_time {
        (None, None)
    } else {
        let window = build_window(args)?;
        if is_date_only {
            // Pad by ±1 day so timezone skew doesn't drop border rows; caller
            // can further refine if needed, but for tail the pad is harmless
            // since `-n` still bounds the result.
            (Some(window.day_start_utc), Some(window.day_end_utc))
        } else {
            (None, Some(window.point_utc))
        }
    };

    // Default limit: 10 for "tail", unlimited when date-only (matches datum).
    let limit = args.count.or(if is_date_only { None } else { Some(10) });

    Ok(ListParams {
        field: args.field.clone(),
        metric,
        start_utc,
        end_utc,
        limit,
        head: args.oldest_first,
        reverse: args.reverse,
    })
}

struct TailWindow {
    point_utc: chrono::DateTime<Utc>,
    day_start_utc: chrono::DateTime<Utc>,
    day_end_utc: chrono::DateTime<Utc>,
}

fn build_window(args: &TailArgs) -> Result<TailWindow> {
    let mut builder = TimeBuilder::now()
        .with_yesterday_count(args.time.yesterday as i64)
        .with_quick_count(args.time.quick as i64);
    if let Some(d) = &args.time.date {
        builder = builder.with_date_arg(parse_date_arg(d)?);
    }
    if let Some(t) = &args.time.time {
        builder = builder.with_time_arg(parse_time_arg(t)?);
    }
    let point = builder.build();
    let local = point.utc.with_timezone(&Local);
    let day = local.date_naive();
    let day_start = Local
        .from_local_datetime(&(day - chrono::Duration::days(1)).and_hms_opt(0, 0, 0).unwrap())
        .single()
        .unwrap();
    let day_end = Local
        .from_local_datetime(&(day + chrono::Duration::days(1)).and_hms_opt(23, 59, 59).unwrap())
        .single()
        .unwrap();
    Ok(TailWindow {
        point_utc: point.utc,
        day_start_utc: day_start.with_timezone(&Utc),
        day_end_utc: day_end.with_timezone(&Utc),
    })
}

pub(crate) fn cmd_grep(args: GrepArgs, db: &Db) -> Result<()> {
    let regexes: Vec<Regex> = args
        .patterns
        .iter()
        .map(|p| Regex::new(p).with_context(|| format!("Invalid regex pattern: {p}")))
        .collect::<Result<_>>()?;

    let candidates = if let Some(field) = &args.field {
        db.entries_for_field(field, None)?
    } else {
        let far_future = chrono::Utc::now() + chrono::Duration::days(365);
        let far_past = chrono::DateTime::UNIX_EPOCH;
        db.entries_in_range(far_past, far_future)?
    };

    let matches: Vec<Entry> = candidates
        .into_iter()
        .filter(|entry| {
            let json = match serde_json::to_string(entry) {
                Ok(s) => s,
                Err(_) => return false,
            };
            regexes.iter().all(|r| r.is_match(&json))
        })
        .collect();

    let registry = load_registry(db).ok();
    let tbl = TableArgs {
        output: args.output(),
        metric: TimeMetricCol::Hybrid,
        columns: vec![],
        suppress_header: false,
    };
    let now = DatumTime::now();
    let ctxs = build_render_contexts(&matches, db);
    print_table_with(&matches, Some(&ctxs), Some(&now), registry.as_ref(), &tbl);
    Ok(())
}

pub(crate) fn cmd_now(args: NowArgs, db: &Db) -> Result<()> {
    emit_stale_sync_warning(db);

    // C4e: surface a top-line warning when there are unresolved conflicts.
    // We swallow read errors silently — the now view is still useful even
    // if the conflict-count query trips (e.g., a future schema migration
    // hasn't run yet on this DB).
    if let Ok(n) = db.pending_conflict_count() {
        if n > 0 {
            print!("{}", crate::render::render_pending_conflict_warning(n));
        }
    }

    let tracked = db.get_tracked_fields()?;
    if tracked.is_empty() {
        println!("No tracked fields. Use `datum track <field>` or log with `occur`/`start`.");
        return Ok(());
    }

    let state_colors = db.get_all_state_colors()?;

    let mut latest_per_field = std::collections::HashMap::new();
    let mut overlap_counts = std::collections::HashMap::new();
    for tf in &tracked {
        if tf.ignored && !args.show_all {
            continue;
        }
        if let Some(entry) = db.latest_entry_for_now(&tf.field)? {
            latest_per_field.insert(tf.field.clone(), entry);
        }
        let open = db.open_count_for_field(&tf.field)?;
        if open >= 2 {
            overlap_counts.insert(tf.field.clone(), open);
        }
    }

    print_now_view(
        &tracked,
        &latest_per_field,
        &state_colors,
        &overlap_counts,
        args.show_all,
    );

    // da-raz: `datum now` also renders a terminal nowview band beneath the
    // active-states list. `--bare` suppresses it (e.g. for piping). The
    // band defaults to a 1-hour window, paralleling the SVG `nowview`
    // surface but in cells instead of pixels.
    if !args.bare {
        print_now_band(&args, db);
    }
    Ok(())
}

/// Render the terminal nowview band beneath `datum now`. Mirrors the old
/// `datum nowview --terminal` path that lived in commands/nowview.rs.
fn print_now_band(args: &NowArgs, db: &Db) {
    use std::io::IsTerminal;

    use chrono::Utc;
    use datum_layout::nowview_terminal::{render_nowview_terminal, TerminalConfig};

    use crate::commands::nowview::{build_fields, format_local_hh_mm};

    const HISTORY_MINUTES: f64 = 60.0;
    const NOW_WIDTH_MINUTES: f64 = 5.0;

    let end = Utc::now();
    let fields = match build_fields(db, end, HISTORY_MINUTES) {
        Ok(f) => f,
        Err(_) => return, // never block the state list on a band-render error
    };
    let cols = args.cols.unwrap_or_else(detect_terminal_cols);
    let use_color = !args.no_color
        && std::env::var_os("NO_COLOR").is_none()
        && std::io::stdout().is_terminal();
    let cfg = TerminalConfig {
        end_ms: end.timestamp_millis(),
        history_minutes: HISTORY_MINUTES,
        now_width_minutes: NOW_WIDTH_MINUTES,
        cols,
        use_color,
        current_time_label: format_local_hh_mm(end),
    };
    let band = render_nowview_terminal(&fields, &cfg);
    println!("{band}");
}

/// Detect terminal width via `$COLUMNS`, falling back to 80. Pure-Rust
/// (no termios crate) so the code path stays wasm-safe in case a host
/// embedder ever calls into `cmd_now` from a non-tty context.
fn detect_terminal_cols() -> u16 {
    if let Ok(s) = std::env::var("COLUMNS") {
        if let Ok(n) = s.parse::<u16>() {
            if n >= 20 {
                return n;
            }
        }
    }
    80
}

/// Prepend a stale-sync warning to `datum now` when sync coverage looks
/// broken for the active DB. C4d upgrade: check daemon liveness too.
///
/// Rules:
/// - No sync configured → no warning
/// - Daemon running AND recent pull (< 1h) → no warning
/// - Daemon down OR stale pull (≥ 1h) → warn, explaining why
fn emit_stale_sync_warning(db: &Db) {
    let threshold = chrono::Duration::hours(1);

    // Active DB name for per-db config lookup. Bare paths have no name.
    let db_name = std::env::var("DATUM_DB").ok().filter(|v| {
        !v.is_empty() && !config::looks_like_path(v)
    });
    let cfg = match config::load_raw_config_or_default() {
        Ok(c) => c,
        Err(_) => return,
    };
    if config::resolve_sync_server(&cfg, db_name.as_deref()).is_none() {
        return;
    }

    let daemon_running =
        crate::commands::sync::daemon_status::read().is_running();
    let last_pull =
        datum_sync_client::sync_state::get_last_pull_at(db).ok().flatten();
    let label = db_name.as_deref().unwrap_or("main");

    match (daemon_running, last_pull) {
        // Best case: daemon up, recent pull — say nothing.
        (true, Some(t)) if Utc::now().signed_duration_since(t) <= threshold => {}

        // Daemon running but pull still stale: the daemon's unhealthy
        // (backing off, wrong server, expired token). Tell the user.
        (true, Some(t)) => {
            let age = Utc::now().signed_duration_since(t);
            println!(
                "⚠ sync stale: {label} last pulled {} (daemon running)",
                human_age(age),
            );
        }
        (true, None) => {
            println!(
                "⚠ sync stale: {label} has never pulled (daemon running)"
            );
        }

        // Daemon down: user needs to install or start it.
        (false, _) => {
            let age_part = match last_pull {
                Some(t) => format!(
                    ", last pulled {}",
                    human_age(Utc::now().signed_duration_since(t)),
                ),
                None => "".to_string(),
            };
            println!(
                "⚠ sync daemon not running{age_part} — \
                 run `datum sync install-daemon` to enable auto-sync",
            );
        }
    }
}

fn human_age(age: chrono::Duration) -> String {
    if age.num_hours() < 24 {
        format!("{}h ago", age.num_hours().max(1))
    } else {
        format!("{}d ago", age.num_days().max(1))
    }
}

pub(crate) fn cmd_track(args: TrackArgs, db: &Db) -> Result<()> {
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
                    spec.state_colors.insert(state.to_string(), color.to_string());
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

pub(crate) fn cmd_ignore(args: IgnoreArgs, db: &Db) -> Result<()> {
    // da-2sr: writing a switch entry with the IGNORED sentinel is the only
    // way to mark a field ignored. No spec writes — the spec keeps its
    // schema-only role.
    let device = device_id()?;
    let entry = Entry::new_legacy(
        &args.field,
        Verb::Switch,
        Some(JsonValue::String(IGNORED_STATE_VALUE.to_string())),
        None,
        None,
        Some(DatumTime::now()),
        None,
        device,
    );
    db.insert_entry(&entry)?;
    println!("✓ Ignoring '{}'", args.field);
    Ok(())
}

// ─── Get / Delete ─────────────────────────────────────────────────────────────

/// Resolve `input` to an entry via quickId matching, preserving the
/// "Tombstoned" signal so update/edit can produce their tailored error.
///
/// The live-entry path goes through [`resolve_quickid_single`], which adds
/// `_` / `<field>:_` shorthands and slug wildcards on top of the existing
/// C3 prefix behaviour. It excludes soft-deleted rows, so a prefix of a
/// tombstone comes back as `NotFound`. When that happens we retry with an
/// exact id/slug lookup that includes tombstones, then one more attempt
/// that scans tombstones by hid/slug prefix — so the undo / history / why
/// commands still work on a soft-deleted row's short hid (e.g. `5arfa`).
pub(crate) fn resolve_prefix_or_tombstone(db: &Db, input: &str) -> Result<Entry> {
    match resolve_quickid_single(db, input) {
        Ok(e) => Ok(e),
        Err(datum_core::error::DatumError::NotFound(_)) => {
            // Exact match first, then fall through to a tombstone-prefix scan.
            if let Ok(exact) = db.get_entry_including_deleted(input) {
                return Ok(exact);
            }
            let conn = db.conn();
            let pattern = {
                let mut s = String::with_capacity(input.len() + 1);
                for ch in input.chars() {
                    if ch == '\\' || ch == '%' || ch == '_' {
                        s.push('\\');
                    }
                    s.push(ch);
                }
                s.push('%');
                s
            };
            let mut stmt = conn.prepare(
                "SELECT slug FROM entries
                 WHERE (slug LIKE ?1 ESCAPE '\\' OR hid LIKE ?1 ESCAPE '\\')
                   AND deleted_at IS NOT NULL
                 ORDER BY slug",
            )?;
            let matches: Vec<String> = stmt
                .query_map([pattern.as_str()], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<_, _>>()?;
            match matches.len() {
                0 => Err(datum_core::error::DatumError::NotFound(input.to_string()).into()),
                1 => Ok(db.get_entry_including_deleted(&matches[0])?),
                _ => Err(anyhow::anyhow!(
                    "ambiguous tombstone prefix '{input}' matches {} entries",
                    matches.len()
                )),
            }
        }
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn cmd_get(args: GetArgs, db: &Db) -> Result<()> {
    // Bare `datum get` (no positional) defaults to `_LAST` — resolve
    // against the last-accessed pointer with no time guard. The pointer
    // may name multiple hids when the previous command was a batch
    // insert/update; expand to all of them.
    let pattern = args
        .id_or_slug
        .as_deref()
        .unwrap_or(datum_core::ids::resolve::LAST_TOKEN);

    let entries = resolve_quickid(db, pattern).map_err(anyhow::Error::from)?;
    if entries.is_empty() {
        return Err(anyhow::Error::from(
            datum_core::error::DatumError::NotFound(pattern.to_string()),
        ));
    }

    let registry = load_registry(db).ok();
    let output = args.output();
    let ctxs = build_render_contexts(&entries, db);
    for (i, entry) in entries.iter().enumerate() {
        if args.pretty {
            print_entry_pretty(entry)?;
        } else {
            crate::render::show_action_with_context(
                entry,
                crate::render::Action::Exists,
                registry.as_ref(),
                ctxs.get(i),
                &output,
            );
        }
    }

    // Stamp the pointer at the resolved hids so a follow-up bare command
    // sees the same set. Read-only browsing commands (tail, head, grep)
    // do NOT touch the pointer.
    let hids: Vec<datum_core::ids::Hid> = entries.iter().map(|e| e.hid.clone()).collect();
    let _ = datum_core::last_docs::set_last_docs(db, &hids);
    Ok(())
}

/// Batch-resolution surface for S3: how "only a tombstone matches" should
/// be reported.
#[derive(Clone, Copy)]
pub(crate) enum BatchMode {
    /// Live only. A literal token hitting a soft-deleted row surfaces as
    /// `NotFound` — delete won't re-delete a tombstone.
    Live,
    /// Live preferred; a literal token matching only a tombstone surfaces
    /// the tombstoned entry so update/merge/rekey can emit their tailored
    /// "Cannot X tombstoned entry" error. Wildcards still only match live.
    LiveOrTombstone,
}

/// Split a comma-separated id argument, resolve each token, and return a
/// deduplicated entry list plus the tokens that failed to resolve. Wildcards
/// (`*` / `?`) expand to every live match; literals go through the quickId
/// resolver (underscore shorthands, exact slug/hid, hid prefix). Empty
/// tokens (`","` / trailing comma) are silently dropped so datum-style
/// `"abc,def,ghi,"` works without special casing. Duplicates (two tokens
/// reaching the same slug) collapse in first-seen order.
pub(crate) fn resolve_batch(
    db: &Db,
    input: &str,
    mode: BatchMode,
) -> (Vec<Entry>, Vec<(String, anyhow::Error)>) {
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen: std::collections::HashSet<datum_core::ids::Slug> = std::collections::HashSet::new();
    let mut errors: Vec<(String, anyhow::Error)> = Vec::new();
    for raw in input.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        match resolve_quickid(db, token) {
            Ok(matches) if matches.is_empty() => {
                // Literal (non-wildcard) tokens may refer to a tombstone —
                // surface it in LiveOrTombstone mode so update/rekey can
                // emit their tailored error. Wildcards stay strictly live.
                if matches!(mode, BatchMode::LiveOrTombstone) && !contains_wildcard(token) {
                    if let Ok(tomb) = db.get_entry_including_deleted(token) {
                        if seen.insert(tomb.slug.clone()) {
                            entries.push(tomb);
                        }
                        continue;
                    }
                }
                errors.push((
                    token.to_string(),
                    anyhow::Error::from(datum_core::error::DatumError::NotFound(
                        token.to_string(),
                    )),
                ));
            }
            Ok(matches) => {
                for entry in matches {
                    if seen.insert(entry.slug.clone()) {
                        entries.push(entry);
                    }
                }
            }
            Err(e) => errors.push((token.to_string(), anyhow::Error::from(e))),
        }
    }
    (entries, errors)
}

// ─── Edit ────────────────────────────────────────────────────────────────────

/// Open an entry in `$EDITOR` as pretty JSON; on save, re-parse and persist.
///
/// Testing hook: `DATUM_EDITOR` takes precedence over `EDITOR`. Both are
/// invoked as `sh -c "$editor \"$1\"" -- <tmpfile>`, so tests can set
/// `DATUM_EDITOR="cp /path/to/prepared.json"` to simulate a scripted edit.
// `cmd_edit` was moved to `crate::commands::edit::edit` in da-4lov to
// route the editor flow through `UserEditableEntry`, which makes
// system-only fields (extra_metadata, hid, slug, content_hash, etc.)
// physically unreachable from the editor tempfile.

// ─── Update / Merge / Rekey ──────────────────────────────────────────────────
// Underlying logic lives in `commands::update_core::update_with_strategy`.
// See that module for the unified strategy function the wrappers (and S3
// batch mutations) call into.

pub(crate) fn parse_update_payload(
    args: &UpdateArgs,
) -> Result<(Option<JsonValue>, Option<JsonValue>)> {
    use datum_parse::input::data_args::{handle_data_args, DataArgs};

    // Same composition shape as `parse_log_args` — the only difference is
    // there are no `-K` slug-keys in update (slug stays put) and no `-K`
    // slot fill, so every `-k KEY=VALUE` is an immediate that bypasses
    // the data queue. `-k KEY` (no `=`) and `-k KEY=` stay as declared
    // slots so positionals can fill them.
    let mut data_args = DataArgs::default();
    // Same daydumb compat flags as the log path — see `parse_log_args`.
    data_args.flat_paths = true;
    data_args.preserve_null_strings = true;
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

    data_args.data.reserve(injected_immediates.len() + args.rest.len());
    data_args.data.extend(injected_immediates);
    data_args.data.extend(args.rest.iter().cloned());
    data_args.remainder = Some(PRIMARY_VALUE_KEY.to_string());

    let mut datum_data = handle_data_args(&data_args).map_err(anyhow::Error::from)?;
    let value = datum_data.remove(PRIMARY_VALUE_KEY);

    // Preserve update's empty-string drop sentinel: `KEY=` (or `-k KEY=`)
    // drops the key from the existing entry in `combine_extra`. handle_data_args
    // turns an empty value into "remove from payload"; on the update path that
    // erases the signal, so we re-introduce a String("") for each such token
    // so `is_empty_string` downstream still fires.
    for token in args.rest.iter().chain(args.keys.iter()) {
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
    Ok((value, extra_data))
}

// ─── Retime ──────────────────────────────────────────────────────────────────

pub(crate) fn cmd_retime(args: RetimeArgs, db: &Db) -> Result<()> {
    if args.slug.is_none() && args.field.is_none() && args.after.is_none() && args.before.is_none()
    {
        anyhow::bail!(
            "retime requires at least one selector: <slug>, --field, --after, or --before"
        );
    }

    // Accept the full quickId surface (`_`, `<field>:_`, hid/slug prefix,
    // slug wildcard) where retime previously required an exact slug/hid.
    // The underlying filter still matches on exact slug/hid, so we resolve
    // the user-supplied pattern to a concrete slug first.
    let resolved_slug: Option<datum_core::ids::Slug> = args
        .slug
        .as_deref()
        .map(|raw| resolve_quickid_single(db, raw).map(|e| e.slug))
        .transpose()
        .map_err(anyhow::Error::from)?;

    let after_utc = args
        .after
        .as_deref()
        .map(parse_time_bound)
        .transpose()
        .context("Invalid --after value")?;
    let before_utc = args
        .before
        .as_deref()
        .map(parse_time_bound)
        .transpose()
        .context("Invalid --before value")?;

    let filter = RetimeFilter {
        slug_or_hid: resolved_slug.as_deref(),
        field: args.field.as_deref(),
        after_utc,
        before_utc,
    };

    // --time: re-parse a fresh absolute time and overwrite both the start
    // timestamp and its precision fields for each match. end_time (if any)
    // shifts by the same delta so the duration is preserved.
    if let Some(time_str) = args.time.as_deref() {
        let ta = parse_time_arg(time_str)
            .with_context(|| format!("Invalid --time value '{time_str}'"))?;
        let builder = TimeBuilder::now().with_time_arg(ta);
        let (new_time, precision) = builder.build_with_precision();
        let n = db.set_start_time_with_precision(
            &filter,
            &new_time,
            precision.seconds,
            &precision.unit.to_string(),
        )?;
        println!(
            "✓ Retimed {n} → {} (±{}{})",
            new_time.utc.to_rfc3339(),
            precision.seconds,
            if precision.seconds == 0 { "" } else { "s" },
        );
        return Ok(());
    }

    // --offset: classic duration-shift path.
    let offset_str = args
        .offset
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("retime requires either --offset or --time"))?;
    let offset = parse_duration(offset_str)
        .with_context(|| format!("Invalid --offset value '{offset_str}'"))?;

    let n = db.update_times(&filter, offset)?;
    let secs = offset.num_seconds();
    let sign = if secs >= 0 { "+" } else { "-" };
    println!("✓ Retimed {n} ({sign}{}s)", secs.abs());
    Ok(())
}

/// Parse a `--after` / `--before` argument to a UTC timestamp.
/// Accepts `YYYY-MM-DD` (midnight local) or RFC3339 timestamps.
fn parse_time_bound(s: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive = d
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("bad date: {s}"))?;
        let local = naive
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| anyhow::anyhow!("ambiguous local time for {s}"))?;
        return Ok(local.with_timezone(&Utc));
    }
    anyhow::bail!("expected YYYY-MM-DD or RFC3339, got '{s}'")
}

/// Bare `undo` path: revert each hid named by the last-docs pointer.
/// Tombstoned hids are restored (history-revert preferred, undelete as
/// fallback); live hids fall back to history (undoing the most recent
/// non-delete change). The pointer itself is rewritten to the restored
/// hids so `_LAST` keeps tracking the user's "current" set.
fn undo_via_last_docs(
    db: &Db,
    hids: &[datum_core::ids::Hid],
    registry: &Option<datum_core::field_spec::SpecRegistry>,
    output: &crate::render::OutputArgs,
) -> Result<()> {
    if hids.is_empty() {
        anyhow::bail!("Last-docs pointer is empty — nothing to undo");
    }
    let mut restored_hids: Vec<datum_core::ids::Hid> = Vec::new();
    let mut first_err: Option<anyhow::Error> = None;
    for hid in hids {
        let current = match resolve_prefix_or_tombstone(db, hid) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("undo {hid}: {e}");
                first_err.get_or_insert(e);
                continue;
            }
        };
        let latest = datum_core::history::latest_version(db.conn(), &current.hid)
            .map_err(anyhow::Error::from)?;
        let restored: Result<datum_core::entry::Entry> = match latest {
            Some(n) if n >= 2 => db
                .revert_entry_to(&current.hid, n - 1)
                .map_err(anyhow::Error::from),
            _ => {
                if current.deleted_at.is_some() {
                    db.undelete(&current.slug).map_err(anyhow::Error::from)?;
                    db.get_entry(&current.slug).map_err(anyhow::Error::from)
                } else {
                    Err(anyhow::anyhow!(
                        "Nothing to undo for {} — no prior version recorded",
                        current.slug,
                    ))
                }
            }
        };
        match restored {
            Ok(entry) => {
                restored_hids.push(entry.hid.clone());
                show_create(&entry, registry.as_ref(), output);
            }
            Err(e) => {
                eprintln!("undo {hid}: {e}");
                first_err.get_or_insert(e);
            }
        }
    }
    if !restored_hids.is_empty() {
        let _ = datum_core::last_docs::set_last_docs(db, &restored_hids);
    }
    match first_err {
        Some(e) if restored_hids.is_empty() => Err(e),
        _ => Ok(()),
    }
}

pub(crate) fn cmd_undo(args: UndoArgs, db: &Db) -> Result<()> {
    let registry = load_registry(db).ok();

    // Slug-targeted undo. Prefer the history-based revert (D4): back off to
    // the prior recorded version. Fall back to the legacy undelete path when
    // no history exists for the entry (e.g. a pre-D4 tombstone imported from
    // a backup) so the command remains useful for those rows.
    if let Some(id_or_slug) = args.id_or_slug.as_deref() {
        let current = resolve_prefix_or_tombstone(db, id_or_slug)?;
        let latest = datum_core::history::latest_version(db.conn(), &current.hid)
            .map_err(anyhow::Error::from)?;

        match (args.to_version, latest) {
            (Some(v), _) => {
                let restored = db
                    .revert_entry_to(&current.hid, v)
                    .map_err(anyhow::Error::from)?;
                show_create(&restored, registry.as_ref(), &args.output());
                Ok(())
            }
            (None, Some(n)) if n >= 2 => {
                let restored = db
                    .revert_entry_to(&current.hid, n - 1)
                    .map_err(anyhow::Error::from)?;
                show_create(&restored, registry.as_ref(), &args.output());
                Ok(())
            }
            _ => {
                // No prior version to revert to — fall back to legacy undelete
                // so tombstoned entries without history still come back. Use
                // the resolved full slug/hid so prefixes keep working here too.
                db.undelete(&current.slug).map_err(anyhow::Error::from)?;
                if let Ok(entry) = db.get_entry(&current.slug) {
                    show_create(&entry, registry.as_ref(), &args.output());
                }
                Ok(())
            }
        }
    } else {
        if args.to_version.is_some() {
            anyhow::bail!("--to-version requires an <id_or_slug> to target");
        }

        // Bare `undo` defaults to `_LAST_WITH_PROTECTION` (matches datum):
        // revert whatever the last command targeted, refusing if the
        // pointer is older than the protection window. `--force` skips
        // the pointer entirely and falls through to the legacy
        // most-recent-tombstone path so existing scripts keep working.
        if !args.force {
            match datum_core::last_docs::get_last_docs(db) {
                Ok(Some(reference))
                    if datum_core::last_docs::within_protection_window(
                        reference.time,
                        Utc::now(),
                    ) =>
                {
                    return undo_via_last_docs(db, &reference.hids, &registry, &args.output());
                }
                Ok(Some(_)) => {
                    // Pointer exists but is stale — fall through to the
                    // window-based fallback so legacy `undo --window` and
                    // bare-undo against ancient tombstones still works.
                }
                Ok(None) | Err(_) => {
                    // No pointer (fresh DB or pre-da-e65 schema) — fall through.
                }
            }
        }

        let window = parse_duration(&args.window)
            .with_context(|| format!("Invalid --window value '{}'", args.window))?;

        let target = if args.force {
            db.most_recent_tombstone()?
        } else {
            db.list_recent_tombstones(window)?.into_iter().next()
        };

        match target {
            Some(entry) => {
                // Prefer the history-based revert so the undo shows up in the
                // entry's trail; fall back to undelete for legacy tombstones.
                let latest = datum_core::history::latest_version(db.conn(), &entry.hid)
                    .map_err(anyhow::Error::from)?;
                match latest {
                    Some(n) if n >= 2 => {
                        let restored = db
                            .revert_entry_to(&entry.hid, n - 1)
                            .map_err(anyhow::Error::from)?;
                        show_create(&restored, registry.as_ref(), &args.output());
                    }
                    _ => {
                        db.undelete(&entry.slug).map_err(anyhow::Error::from)?;
                        let restored = db.get_entry(&entry.slug).unwrap_or(entry);
                        show_create(&restored, registry.as_ref(), &args.output());
                    }
                }
                Ok(())
            }
            None => {
                // Nothing within the window. If --force was off and there's
                // still an older tombstone, nudge the user toward --force;
                // otherwise the store has no tombstones at all.
                if !args.force {
                    if let Some(older) = db.most_recent_tombstone()? {
                        anyhow::bail!(
                            "No deletes within {}; most recent is '{}' (use --force to restore it)",
                            args.window,
                            older.slug
                        );
                    }
                }
                anyhow::bail!("Nothing to undo — no soft-deleted entries found")
            }
        }
    }
}

// ─── History / Why ───────────────────────────────────────────────────────────

pub(crate) fn cmd_history(args: HistoryArgs, db: &Db) -> Result<()> {
    let current = resolve_prefix_or_tombstone(db, &args.id_or_slug)
        .with_context(|| format!("entry not found: {}", args.id_or_slug))?;
    let hid = current.hid.clone();

    // `--version N` wins over `--diff` when both are set. The snapshot is
    // rendered as pretty JSON either way — `--json` is a no-op here but
    // accepted for symmetry with the list/diff forms.
    if let Some(v) = args.version {
        let snap = resolve_version_snapshot(db, &current, v)?;
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(());
    }

    if let Some(ab) = args.diff.as_ref() {
        let (a, b) = (ab[0], ab[1]);
        let snap_a = resolve_version_snapshot(db, &current, a)?;
        let snap_b = resolve_version_snapshot(db, &current, b)?;
        if args.json {
            let payload = serde_json::json!({
                "a": { "version": a, "snapshot": snap_a },
                "b": { "version": b, "snapshot": snap_b },
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            print_history_diff(a, &snap_a, b, &snap_b);
        }
        return Ok(());
    }

    // Default: list every version, newest first.
    let versions = datum_core::history::list(db.conn(), &hid, true)
        .map_err(anyhow::Error::from)?;

    if args.json {
        let payload: Vec<_> = versions
            .iter()
            .map(|v| {
                serde_json::json!({
                    "version": v.version,
                    "modify_time": v.modify_time.to_rfc3339(),
                    "change_kind": v.change_kind.as_str(),
                    "raw_input": v.raw_input,
                    "alias_chain": v.alias_chain,
                    "device_id": v.device_id,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!("hid: {}", hid);
    if versions.is_empty() {
        println!("  (no history recorded)");
        return Ok(());
    }
    for v in versions {
        let raw = if v.raw_input.is_empty() {
            String::new()
        } else {
            format!("'{}'", v.raw_input)
        };
        println!(
            "  v{}  {}  {:<22} {}",
            v.version,
            v.modify_time.format("%Y-%m-%dT%H:%M"),
            v.change_kind.as_str(),
            raw
        );
    }
    Ok(())
}

/// Resolve "state at version N" for display or diff. Uses
/// [`datum_core::history::state_at_version`] so the call site doesn't need
/// to re-encode the v1-is-special rule.
fn resolve_version_snapshot(
    db: &Db,
    current: &Entry,
    version: u32,
) -> Result<Entry> {
    datum_core::history::state_at_version(db.conn(), &current.hid, version, Some(current))
        .map_err(anyhow::Error::from)?
        .ok_or_else(|| anyhow::anyhow!("version {version} not found for entry {}", current.hid))
}

fn print_history_diff(a: u32, snap_a: &Entry, b: u32, snap_b: &Entry) {
    let va = serde_json::to_value(snap_a).unwrap_or(serde_json::Value::Null);
    let vb = serde_json::to_value(snap_b).unwrap_or(serde_json::Value::Null);
    println!("--- v{a}\n+++ v{b}");
    diff_json("", &va, &vb);
}

fn diff_json(path: &str, a: &JsonValue, b: &JsonValue) {
    if a == b {
        return;
    }
    match (a, b) {
        (JsonValue::Object(ma), JsonValue::Object(mb)) => {
            let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match (ma.get(k), mb.get(k)) {
                    (Some(va), Some(vb)) => diff_json(&child_path, va, vb),
                    (Some(va), None) => println!("- {child_path}: {}", render_scalar(va)),
                    (None, Some(vb)) => println!("+ {child_path}: {}", render_scalar(vb)),
                    (None, None) => {}
                }
            }
        }
        _ => {
            println!("- {}: {}", path, render_scalar(a));
            println!("+ {}: {}", path, render_scalar(b));
        }
    }
}

fn render_scalar(v: &JsonValue) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| v.to_string())
}

pub(crate) fn cmd_why(args: WhyArgs, db: &Db) -> Result<()> {
    let current = resolve_prefix_or_tombstone(db, &args.id_or_slug)
        .with_context(|| format!("entry not found: {}", args.id_or_slug))?;

    println!("hid: {}  slug: {}", current.hid, current.slug);
    if !current.raw_input.is_empty() {
        println!("  raw_input:   {}", current.raw_input);
    }
    if let Some(chain) = &current.alias_chain {
        println!("  alias_chain: {}", chain);
    }

    if !args.full {
        return Ok(());
    }

    let versions = datum_core::history::list(db.conn(), &current.hid, true)
        .map_err(anyhow::Error::from)?;
    println!();
    println!("history (newest first):");
    if versions.is_empty() {
        println!("  (no history recorded)");
        return Ok(());
    }
    for v in versions {
        let raw = if v.raw_input.is_empty() {
            "<no raw_input captured>".to_string()
        } else {
            v.raw_input.clone()
        };
        println!(
            "  v{}  {}  {}  {}",
            v.version,
            v.modify_time.format("%Y-%m-%dT%H:%M"),
            v.change_kind,
            raw
        );
        if let Some(chain) = &v.alias_chain {
            println!("       alias_chain: {chain}");
        }
    }
    Ok(())
}

// ─── Spec management ─────────────────────────────────────────────────────────

pub(crate) fn cmd_spec(sub: SpecCommand, db: &Db) -> Result<()> {
    match sub {
        SpecCommand::Show { field } => match db.load_spec(&field)? {
            Some(spec) => println!("{}", serde_json::to_string_pretty(&spec)?),
            None => println!("No spec defined for field '{field}'"),
        },
        SpecCommand::Set(args) => cmd_spec_set(args, db)?,
        SpecCommand::List { tag } => {
            if let Some(tag) = tag {
                let specs = db.list_specs_by_tag(&tag)?;
                if specs.is_empty() {
                    println!("No field specs tagged '{tag}'.");
                } else {
                    for spec in &specs {
                        println!("  {}", spec.field);
                    }
                }
            } else {
                let reg = db.load_registry()?;
                if reg.fields.is_empty() {
                    println!("No field specs defined.");
                } else {
                    let mut names: Vec<&String> = reg.fields.keys().collect();
                    names.sort();
                    for name in names {
                        println!("  {name}");
                    }
                }
            }
        }
        SpecCommand::Remove { field } => {
            db.delete_field_spec(&field)?;
            println!("✓ Spec removed for '{field}' (derived aliases dropped)");
        }
    }
    Ok(())
}

pub(crate) fn cmd_spec_set(args: SpecSetArgs, db: &Db) -> Result<()> {
    // --json replaces the entire spec; individual flags merge into existing.
    let mut spec = if let Some(json) = &args.json {
        let mut s: FieldSpec = serde_json::from_str(json)
            .with_context(|| "Invalid field spec JSON")?;
        s.field = args.field.clone();
        s
    } else {
        db.load_spec(&args.field)?
            .unwrap_or_else(|| FieldSpec::new(&args.field, Action::Occur))
    };

    if let Some(dv) = &args.default_verb {
        spec.default_verb = dv
            .parse::<Action>()
            .map_err(|e| anyhow::anyhow!("Invalid --default-verb: {e}"))?;
    }
    if !args.states.is_empty() {
        spec.allowed_states = Some(args.states.clone());
    }
    if let Some(c) = &args.color {
        spec.color = Some(c.clone());
    }
    if let Some(t) = &args.id_template {
        spec.id_template = Some(t.clone());
    }
    if let Some(d) = &args.description {
        spec.description = Some(d.clone());
    }
    if let Some(u) = &args.unit {
        spec.unit = Some(u.clone());
    }
    for sc in &args.state_colors {
        match sc.split_once('=') {
            Some((state, color)) => {
                spec.state_colors.insert(state.to_string(), color.to_string());
            }
            None => anyhow::bail!(
                "Invalid --state-color format: expected STATE=COLOR, got '{sc}'"
            ),
        }
    }
    // da-2sr: --track / --ignore on `spec set` write switch entries rather
    // than mutating the spec. The flags are kept on the parser surface for
    // backwards compatibility; they no longer touch the spec.
    if args.track {
        let device = device_id()?;
        let entry = Entry::new_legacy(
            &args.field,
            Verb::Switch,
            None,
            None,
            None,
            Some(DatumTime::now()),
            None,
            device,
        );
        db.insert_entry(&entry)?;
    }
    if args.ignore {
        let device = device_id()?;
        let entry = Entry::new_legacy(
            &args.field,
            Verb::Switch,
            Some(JsonValue::String(IGNORED_STATE_VALUE.to_string())),
            None,
            None,
            Some(DatumTime::now()),
            None,
            device,
        );
        db.insert_entry(&entry)?;
    }
    if !args.id_keys.is_empty() {
        spec.id_keys = args.id_keys.clone();
    }
    if !args.data_keys.is_empty() {
        spec.data_keys = args.data_keys.clone();
    }
    if args.no_autoalias {
        spec.no_autoalias = true;
    }
    if args.y.is_some() {
        spec.y = args.y;
    }
    if args.height.is_some() {
        spec.height = args.height;
    }
    if args.z_index.is_some() {
        spec.z_index = args.z_index;
    }
    // da-a7g: `--tag <name>` repeats replace the spec's full tag set
    // (mirrors how `--state <name>` replaces `allowed_states`). To clear
    // tags, re-set the spec from JSON with `tags: []`.
    if !args.tags.is_empty() {
        spec.tags = args.tags.clone();
    }

    spec.validate_default_verb()
        .map_err(|e| anyhow::anyhow!(e))?;

    // Always regenerate derived aliases from the current shape.
    spec.regenerate_derived_aliases();

    let report = db.save_field_spec_with_report(&spec)?;
    println!("✓ Spec saved for '{}'", spec.field);
    for conflict in &report.skipped {
        match &conflict.reason {
            datum_core::db::AliasConflictReason::UserAlias => eprintln!(
                "  ⚠ auto-alias '{}' skipped — user alias with the same name already exists",
                conflict.name
            ),
            datum_core::db::AliasConflictReason::OwnedByField(owner) => eprintln!(
                "  ⚠ auto-alias '{}' skipped — owned by field '{}'",
                conflict.name, owner
            ),
        }
    }
    Ok(())
}

// ─── Conflicts (D5) ──────────────────────────────────────────────────────────

pub(crate) fn cmd_conflicts(sub: ConflictsCommand, db: &Db) -> Result<()> {
    match sub {
        ConflictsCommand::List(args) => cmd_conflicts_list(args, db),
        ConflictsCommand::Show(args) => cmd_conflicts_show(args, db),
        ConflictsCommand::Resolve(args) => cmd_conflicts_resolve(args, db),
        ConflictsCommand::AutoResolve(args) => cmd_conflicts_auto_resolve(args, db),
    }
}

pub(crate) fn cmd_conflicts_list(args: ConflictsListArgs, db: &Db) -> Result<()> {
    use datum_core::conflicts::{ConflictKind, ListFilter};
    let kind = match args.kind.as_deref() {
        None => None,
        Some(s) => Some(
            s.parse::<ConflictKind>()
                .map_err(|e| anyhow::anyhow!(e))?,
        ),
    };
    let filter = ListFilter {
        kind,
        pending: if args.all { None } else { Some(true) },
    };
    let rows = db.list_conflicts(&filter)?;
    if rows.is_empty() {
        println!("No conflicts.");
        return Ok(());
    }
    for c in &rows {
        let status = match &c.resolution {
            None => "pending".to_string(),
            Some(r) => format!("resolved:{r}"),
        };
        println!(
            "{hid}  {kind:<12}  {slug}  [{status}]  detected={detected}",
            hid = c.hid,
            kind = c.conflict_kind.as_str(),
            slug = c.entry_slug,
            status = status,
            detected = c.detected_at.to_rfc3339(),
        );
    }
    Ok(())
}

pub(crate) fn cmd_conflicts_show(args: ConflictsShowArgs, db: &Db) -> Result<()> {
    let conflict = db
        .get_conflict(&args.hid)?
        .ok_or_else(|| anyhow::anyhow!("conflict '{}' not found", args.hid))?;
    print!("{}", crate::render::render_conflict_show(&conflict));
    Ok(())
}

pub(crate) fn cmd_conflicts_resolve(args: ConflictsResolveArgs, db: &Db) -> Result<()> {
    use datum_core::conflicts::Resolution;

    if args.skip {
        // `--skip` is an explicit no-op: leave the conflict pending.
        // We still acknowledge so scripts get a stable signal.
        let conflict = db
            .get_conflict(&args.hid)?
            .ok_or_else(|| anyhow::anyhow!("conflict '{}' not found", args.hid))?;
        println!(
            "↷ skipped {hid}  {kind}  (still pending)",
            hid = conflict.hid,
            kind = conflict.conflict_kind.as_str(),
        );
        return Ok(());
    }

    if args.edit {
        let resolved = run_conflict_edit(&args.hid, db)?;
        println!(
            "✓ resolved {hid}  {kind}  {resolution}",
            hid = resolved.hid,
            kind = resolved.conflict_kind.as_str(),
            resolution = resolved.resolution.as_deref().unwrap_or("?"),
        );
        return Ok(());
    }

    let resolution = match args.take.as_deref() {
        Some("local") => Resolution::TakeLocal,
        Some("remote") => Resolution::TakeRemote,
        Some(other) => anyhow::bail!(
            "--take only accepts 'local' or 'remote' (got '{other}')"
        ),
        None => {
            if args.close_older {
                Resolution::CloseOlder
            } else {
                // `--keep both` records the user's explicit keep-both decision;
                // any other string is a usage error (we only support "both" today).
                match args.keep.as_deref() {
                    Some("both") => Resolution::KeepBoth,
                    Some(other) => anyhow::bail!(
                        "--keep only supports 'both' (got '{other}'); use --close-older for auto-close"
                    ),
                    None => anyhow::bail!(
                        "specify a resolution: --keep both | --close-older | --take local|remote | --edit | --skip"
                    ),
                }
            }
        }
    };
    let resolved = db.resolve_conflict(&args.hid, resolution)?;
    println!(
        "✓ resolved {hid}  {kind}  {resolution}",
        hid = resolved.hid,
        kind = resolved.conflict_kind.as_str(),
        resolution = resolved.resolution.as_deref().unwrap_or("?"),
    );
    Ok(())
}

/// `--edit` flow: dump the local snapshot to a temp file as JSON, open
/// `$EDITOR` (or `$VISUAL`, falling back to `vi`), parse the edited
/// result back into an `Entry`, and apply it as the merged version.
///
/// The temp file is prefixed with the conflict hid so editors that show
/// the buffer name surface it. We only support JSON for now; the user
/// is expected to keep the structure intact. A blank save aborts the
/// resolution (the conflict stays pending).
fn run_conflict_edit(token: &str, db: &Db) -> Result<datum_core::conflicts::Conflict> {
    use std::io::Write;

    let conflict = db
        .get_conflict(token)?
        .ok_or_else(|| anyhow::anyhow!("conflict '{}' not found", token))?;
    let initial = serde_json::to_string_pretty(&conflict.local_entry)?;

    let mut tmp = std::env::temp_dir();
    tmp.push(format!("datum-conflict-{}.json", conflict.hid));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(initial.as_bytes())?;
    }

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor).arg(&tmp).status()?;
    if !status.success() {
        anyhow::bail!("editor '{editor}' exited with {status}; conflict left pending");
    }

    let edited_raw = std::fs::read_to_string(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    if edited_raw.trim().is_empty() {
        anyhow::bail!("editor saved an empty file; conflict left pending");
    }
    let edited: datum_core::entry::Entry = serde_json::from_str(&edited_raw)
        .map_err(|e| anyhow::anyhow!("could not parse edited entry as JSON Entry: {e}"))?;
    db.resolve_conflict_with_edited(&conflict.id, &edited)
        .map_err(Into::into)
}

pub(crate) fn cmd_conflicts_auto_resolve(args: ConflictsAutoResolveArgs, db: &Db) -> Result<()> {
    let applied = db.auto_resolve_conflicts(Utc::now(), args.min_age)?;
    if applied.is_empty() {
        println!("No conflicts eligible for auto-resolution (min_age={}s).", args.min_age);
        return Ok(());
    }
    for c in &applied {
        println!(
            "✓ auto-resolved {hid}  {kind}  ({slug})",
            hid = c.hid,
            kind = c.conflict_kind.as_str(),
            slug = c.entry_slug,
        );
    }
    Ok(())
}

// ─── Alias management ────────────────────────────────────────────────────────

pub(crate) fn cmd_alias(sub: AliasCommand, db: &Db) -> Result<()> {
    match sub {
        AliasCommand::Show { name } => {
            let reg = db.load_registry()?;
            match reg.aliases.get(&name) {
                Some(alias) => println!("{name} → datum {}", alias.expands_to),
                None => println!("No alias '{name}'"),
            }
        }
        AliasCommand::Set { name, expansion } => {
            let alias = Alias { name: name.clone(), expands_to: expansion, description: None };
            db.save_alias(&alias)?;
            println!("✓ Alias '{name}' saved.");
        }
        AliasCommand::List => {
            let reg = db.load_registry()?;
            if reg.aliases.is_empty() {
                println!("No aliases defined.");
            } else {
                for (name, alias) in &reg.aliases {
                    println!("  {name} → datum {}", alias.expands_to);
                }
            }
        }
        AliasCommand::Remove { name } => {
            db.delete_alias(&name)?;
            println!("✓ Alias '{name}' removed (if it was user-created)");
        }
    }
    Ok(())
}

// ─── Migration ───────────────────────────────────────────────────────────────

pub(crate) fn cmd_migrate(cmd: MigrateCommand, db: &Db) -> Result<()> {
    match cmd {
        MigrateCommand::Run => cmd_migrate_run(db),
        MigrateCommand::Status => cmd_migrate_status(db),
        MigrateCommand::Edit => cmd_migrate_edit(),
    }
}

pub(crate) fn cmd_migrate_run(db: &Db) -> Result<()> {
    // Db::open already auto-advanced on session start. Re-check and report.
    let applied = datum_core::migrations::migrate_to_current(db.conn())?;
    let current = db.schema_version()?;
    if applied.is_empty() {
        println!("Schema already at v{current}. No pending migrations.");
    } else {
        println!("Advanced schema to v{current}:");
        for line in applied {
            println!("  ✓ {line}");
        }
    }
    Ok(())
}

pub(crate) fn cmd_migrate_status(db: &Db) -> Result<()> {
    let (current, target, rows) = datum_core::migrations::migration_status(
        db.conn(),
        datum_core::migrations::MIGRATIONS,
    )?;
    println!("Current schema version: v{current}");
    println!("Binary target version:  v{target}");
    println!();
    println!("Registered migrations:");
    if rows.is_empty() {
        println!("  (none — schema v{target} is the initial shape)");
    } else {
        for r in rows {
            let state = if r.applied { "applied" } else { "pending" };
            println!("  v{}→v{}  [{state}]  {}", r.from, r.to, r.summary);
        }
    }
    Ok(())
}

pub(crate) fn cmd_migrate_edit() -> Result<()> {
    anyhow::bail!(
        "`datum migrate edit` (per-entry data migration) is not yet implemented; \
         tracked in bead da-glnr"
    )
}

pub(crate) fn cmd_db_import(args: DbImportArgs, db: &Db) -> Result<()> {
    let backup_path = args.resolved_path()?.to_string();
    let dry_run = args.dry_run;

    let content = std::fs::read_to_string(&backup_path)
        .with_context(|| format!("Reading backup file: {}", backup_path))?;
    let backup: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| "Parsing backup JSON")?;

    let docs = backup["docs"].as_array()
        .context("Backup JSON missing 'docs' array")?;

    let device = device_id()?;
    let mut skipped = 0usize;
    let mut points = 0usize;
    let mut blocks = 0usize;
    let mut dur_parse_warnings = 0usize;
    let mut entries: Vec<Entry> = Vec::with_capacity(docs.len());
    // Stable provenance timestamp pulled from the backup file itself, not
    // the wall clock at import time. Two imports of the same backup file
    // produce the same _import marker → identical content_hash → dedup.
    // Falls back to "" if the source omits backupTime; this still keeps
    // idempotency (every re-import of that source uses "" too).
    let backup_time = backup["backupTime"].as_str().unwrap_or("").to_string();

    for doc in docs {
        let data = &doc["data"];
        let meta = &doc["meta"];

        let field = match data["field"].as_str() {
            Some(f) => f.to_string(),
            None => { skipped += 1; continue; }
        };

        // ─── state ──────────────────────────────────────────────────
        // `data.state` undefined → state column NULL.
        // `data.state == null` → state column NULL (legacy datum can't
        //   distinguish missing from explicit null on read; collapse them
        //   here so the canonical state goes through derive_verb's
        //   null→true promotion below).
        // anything else → Some(value), preserved verbatim.
        let raw_state = data.get("state");
        let state_in: Option<JsonValue> = match raw_state {
            None => None,
            Some(v) if v.is_null() => None,
            Some(v) => Some(v.clone()),
        };

        let occur_time = parse_datum_time(&data["occurTime"]);
        let occur_time_precision = parse_datum_precision(&data["occurTime"]);

        // ─── dur ────────────────────────────────────────────────────
        // Per da-5ns.6:
        //   legacy `dur` undefined (key missing) → dur=NULL
        //   legacy `dur` null (explicit null)    → dur="PT0S"  (point-for-state)
        //   legacy `dur` "<ISO>"                 → dur="<ISO>" verbatim
        //   legacy `dur` malformed               → dur=NULL, warn, preserve
        //                                          literal in `data` blob
        //   legacy `dur` numeric (rare)          → dur="PT<n>S" (seconds)
        let raw_dur = data.get("dur");
        let dur_in: Option<String> = match raw_dur {
            None => None,
            Some(v) if v.is_null() => Some("PT0S".to_string()),
            Some(JsonValue::String(s)) => {
                if parse_duration(s).is_ok() {
                    Some(s.clone())
                } else {
                    eprintln!(
                        "warning: could not parse dur \"{}\" for {}; keeping literal in extra_data",
                        s, field,
                    );
                    dur_parse_warnings += 1;
                    None
                }
            }
            Some(v) => {
                if let Some(secs) = v.as_i64() {
                    Some(format!("PT{}S", secs))
                } else if let Some(secs) = v.as_f64() {
                    Some(format!("PT{}S", secs as i64))
                } else {
                    eprintln!(
                        "warning: could not parse dur {} for {}; keeping literal in extra_data",
                        v, field,
                    );
                    dur_parse_warnings += 1;
                    None
                }
            }
        };

        // ─── verb + state ───────────────────────────────────────────
        // following_state is always None for legacy entries (block-switch /
        // point-switch are daydumb-new shapes — they never appear in a
        // CouchDB dump).
        //
        // We do NOT canonicalize state (the null→true promotion) here.
        // Per the da-5ns.3 contract, `derive_verb_for_storage` owns verb
        // derivation at insert time and runs the canonicalization itself.
        // If we pre-canonicalized — turning a no-state, no-dur legacy doc
        // into `state=true, dur=None` — storage would then re-derive the
        // verb from `(state_set=true, DurKind::None, fs=None)` and write
        // `verb='switch'` instead of the correct `verb='point'` (da-7sr).
        //
        // Run derive_verb purely for input validation (to skip malformed
        // dur / illegal shape combinations mid-bulk rather than letting
        // them fail at insert) and discard its outputs. `verb = Point` is
        // a placeholder that won't trip the switch-style state→true
        // promotion in `derive_verb_for_storage`; storage will compute
        // the real verb from raw `(state_in, dur_in, None)`.
        if let Err(e) = derive_verb(state_in.clone(), dur_in.as_deref(), None) {
            eprintln!("warning: skipping {}: {}", field, e);
            skipped += 1;
            continue;
        }
        let state = state_in.clone();
        let verb = Verb::Point;

        // ─── data blob ──────────────────────────────────────────────
        // Everything in `data` except first-class columns goes into the
        // `data` blob. Note: `dur` is preserved here as user-visible intent
        // (datum-compatible read-back). System provenance lives in
        // `extra_metadata._import` (da-tsw) — strictly disjoint.
        // Doc-root `_rev` and `_attachments` are CouchDB plumbing and never
        // make it into `data` (they live on the doc, not on `data.*`).
        let mut blob = serde_json::Map::new();
        if let Some(obj) = data.as_object() {
            for (k, v) in obj {
                if !matches!(k.as_str(), "field" | "state" | "occurTime") {
                    blob.insert(k.clone(), v.clone());
                }
            }
        }
        let data_blob = if blob.is_empty() {
            None
        } else {
            Some(JsonValue::Object(blob))
        };

        // The "_import" marker distinguishes this row's source. A future
        // datum-v1 importer (bash + text documents) will set
        // source: "datum-v1-shell". Keep this marker shape stable —
        // downstream queries may filter on it (e.g. dedup by `original_id`
        // on re-import).
        let mut import_marker = serde_json::Map::new();
        import_marker.insert(
            "source".to_string(),
            JsonValue::String("datum-v2-couchdb".to_string()),
        );
        import_marker.insert(
            "backup_time".to_string(),
            JsonValue::String(backup_time.clone()),
        );
        if let Some(orig_id) = doc["_id"].as_str() {
            import_marker.insert(
                "original_id".to_string(),
                JsonValue::String(orig_id.to_string()),
            );
        }
        let mut metadata_obj = serde_json::Map::new();
        metadata_obj.insert("_import".to_string(), JsonValue::Object(import_marker));
        let extra_metadata = Some(JsonValue::Object(metadata_obj));

        let created_at = parse_datum_time(&meta["createTime"])
            .map(|t| t.utc)
            .unwrap_or_else(chrono::Utc::now);
        // Prefer source modifyTime; fall back to createTime so imported rows
        // never carry a fresh "now" stamp when the source had a time at all.
        let modified_at = parse_datum_time(&meta["modifyTime"])
            .map(|t| t.utc)
            .unwrap_or(created_at);

        let mut entry = Entry::new(
            field,
            verb,
            state,
            data_blob,
            None, // unit — legacy datum stored unit annotations inside data values
            occur_time,
            dur_in.clone(),
            device.clone(),
        );
        entry.create_time = created_at;
        entry.modify_time = modified_at;
        entry.occur_time_precision = occur_time_precision;
        entry.id_structure = meta
            .get("idStructure")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        entry.field_structure = meta
            .get("fieldStructure")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // Imported rows weren't produced by a real parse pass; sentinel 0
        // distinguishes them from `CURRENT_PARSER_VERSION` rows so a future
        // re-derive sweep can find and reprocess them.
        entry.parser_version = 0;
        entry.raw_input = String::new();
        entry.alias_chain = None;
        entry.extra_metadata = extra_metadata;
        // Recompute hash after overriding columns that participate in it.
        entry.content_hash = entry.expected_content_hash();

        // Tally based on dur shape (matches user-facing summary semantics:
        // dur=None or dur=zero → point, otherwise → block).
        let is_block = match entry.dur.as_deref() {
            None => false,
            Some(d) if matches!(d, "PT0S" | "P0D" | "PT0M" | "PT0H") => false,
            Some(_) => true,
        };
        if is_block {
            blocks += 1;
        } else {
            points += 1;
        }

        if dry_run {
            println!("[dry-run] would import: {}", entry.slug);
        }
        entries.push(entry);
    }

    let imported = if dry_run {
        entries.len()
    } else {
        // Bulk-load fast path: single tx, no per-row history rows, no
        // per-row conflict detection. Source backups are self-consistent;
        // run `datum conflicts list` post-import for belt-and-braces.
        // Dedup (`Ignored`) maps to `skipped` to mirror the previous per-row
        // behavior; skipped rows also retract their point/block tally.
        let outcomes = db.insert_entries_bulk(&entries, ConflictStrategy::Ignore)?;
        let mut inserted = 0usize;
        for (entry, outcome) in entries.iter().zip(outcomes.iter()) {
            match outcome {
                InsertOutcome::Inserted { .. } => inserted += 1,
                InsertOutcome::Ignored { .. } => {
                    skipped += 1;
                    match entry.entry_shape {
                        datum_core::entry::EntryShape::Block => {
                            blocks = blocks.saturating_sub(1);
                        }
                        _ => points = points.saturating_sub(1),
                    }
                }
                _ => {}
            }
        }
        inserted
    };

    if dry_run {
        println!(
            "[dry-run] Migration summary: {imported} would import ({points} point, {blocks} block), {skipped} skipped."
        );
    } else {
        println!(
            "Migration complete: {imported} imported ({points} point, {blocks} block), {skipped} skipped."
        );
        if imported > 0 {
            // Bulk-load fast path skipped per-row conflict detection.
            // Source backups are self-consistent so this is normally fine,
            // but give the user a one-liner manual fallback.
            println!("(run `datum conflicts list` to verify nothing slipped through.)");
        }
    }
    if dur_parse_warnings > 0 {
        println!("({dur_parse_warnings} dur values could not be parsed; literals preserved in extra_data.)");
    }
    Ok(())
}

fn parse_datum_time(v: &JsonValue) -> Option<DatumTime> {
    let utc_str = v["utc"].as_str()?;
    let utc: chrono::DateTime<chrono::Utc> = utc_str.parse().ok()?;
    let tz = v["tz"].as_str().unwrap_or("UTC").to_string();
    let offset_minutes = v["o"].as_f64().map(|o| (o * 60.0) as i32).unwrap_or(0);
    Some(DatumTime { utc, offset_minutes, timezone: tz })
}

/// Map a legacy datum `occurTime` block to a single ISO-8601 precision
/// string for the new schema's `occur_time_precision` column.
///
/// Legacy datum stored precision as a `(precision, precisionUnit)` pair on
/// the time block. The v=20 schema collapses that into one column. Per
/// the da-5ns.6 canon table:
///   year → P1Y, month → P1M, day → P1D, week → P1W,
///   hour → PT1H, minute → PT1M, second → PT1S
/// Anything else (including absent, "exact", or unrecognized units) →
/// `None` (treated as exact-second).
fn parse_datum_precision(occur_block: &JsonValue) -> Option<String> {
    let unit = occur_block.get("precisionUnit")?.as_str()?;
    match unit {
        "year" => Some("P1Y".to_string()),
        "month" => Some("P1M".to_string()),
        "day" | "date" => Some("P1D".to_string()),
        "week" => Some("P1W".to_string()),
        "hour" => Some("PT1H".to_string()),
        "minute" => Some("PT1M".to_string()),
        "second" => Some("PT1S".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{DeleteArgs, EditArgs};
    use crate::commands::update_core::{update_from_args, UpdateStrategy};

    #[test]
    fn test_parse_value_token_number() {
        assert_eq!(parse_value_token("42"), JsonValue::Number(42.into()));
        assert_eq!(parse_value_token("true"), JsonValue::Bool(true));
    }

    #[test]
    fn test_parse_value_token_unit_annotated() {
        let v = parse_value_token("75.2kg");
        assert_eq!(v["v"], serde_json::json!(75.2));
        assert_eq!(v["u"], serde_json::json!("kg"));
    }

    #[test]
    fn test_parse_value_token_string() {
        assert_eq!(parse_value_token("happy"), JsonValue::String("happy".to_string()));
    }

    #[test]
    fn test_split_numeric_unit() {
        assert_eq!(split_numeric_unit("75.2kg"), Some(("75.2", "kg")));
        assert_eq!(split_numeric_unit("500ml"), Some(("500", "ml")));
        assert_eq!(split_numeric_unit("hello"), None);
        assert_eq!(split_numeric_unit("42"), None);
    }

    #[test]
    fn test_dur_start_sets_time_end() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::fresh("sleep");
        args.duration.duration = Some("8h".to_string());
        // Just verify the dur parsing logic: start --dur 8h → end = start + 8h
        let (base, _precision) = build_time_with_precision(&args).unwrap();
        let dur = parse_duration("8h").unwrap();
        let end_utc = base.utc + dur;
        let diff_secs = (end_utc - base.utc).num_seconds();
        assert_eq!(diff_secs, 8 * 3600);
        // Ensure dispatch doesn't error
        log_entry(Action::Start, args, &db).unwrap();
    }

    #[test]
    fn test_dur_stop_sets_time_start() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::fresh("sleep");
        args.duration.duration = Some("8h".to_string());
        log_entry(Action::Stop, args, &db).unwrap();
        let entries = db.entries_for_field("sleep", None).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        let diff_secs = e.duration_seconds().unwrap();
        assert_eq!(diff_secs, 8 * 3600);
    }

    #[test]
    fn test_get_and_delete() {
        use datum_core::{db::Db, entry::Entry, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        let t = DatumTime::now();
        let entry = Entry::new_legacy("test", Verb::Point, None, None, None, Some(t), None, "dev");
        let slug = entry.slug.clone();
        db.insert_entry(&entry).unwrap();

        // get
        let fetched = db.get_entry(&slug).unwrap();
        assert_eq!(fetched.slug, slug);

        // delete
        db.delete_entry(&slug).unwrap();
        let err = db.get_entry(&slug);
        assert!(matches!(err, Err(datum_core::error::DatumError::NotFound(_))));
    }

    #[test]
    fn test_auto_track_on_occur() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Bare `occur coffee` — occur is stateless under da-k5o6 and rejects
        // bare positionals; the per-occurrence quantity, if any, would land
        // via `-k amount=2`. Auto-track depends on the verb, not the data.
        let args = LogArgs::fresh("coffee");
        log_entry(Action::Occur, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(tracked.iter().any(|f| f.field == "coffee" && !f.ignored));
    }

    #[test]
    fn test_auto_track_on_start() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs::fresh("work");
        log_entry(Action::Start, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(tracked.iter().any(|f| f.field == "work" && !f.ignored));
    }

    #[test]
    fn test_undo_flag_deletes_last_matching_entry() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // First insert: `datum switch coffee 2` (switch is the state-bearing
        // verb; `occur` carries no state under da-k5o6).
        let mut args = LogArgs::fresh("coffee");
        args.rest = vec!["2".to_string()];
        log_entry(Action::Switch, args, &db).unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 1);

        // Undo via `-u`: same command form with `args.undo = true`. The
        // matching entry is soft-deleted, so the live count drops to 0.
        let mut undo_args = LogArgs::fresh("coffee");
        undo_args.rest = vec!["2".to_string()];
        undo_args.undo = true;
        log_entry(Action::Switch, undo_args, &db).unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 0);
    }

    #[test]
    fn test_undo_flag_state_specific() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Two entries with different states under the same field.
        let mut a = LogArgs::fresh("coffee");
        a.rest = vec!["1".to_string()];
        log_entry(Action::Switch, a, &db).unwrap();
        let mut b = LogArgs::fresh("coffee");
        b.rest = vec!["2".to_string()];
        log_entry(Action::Switch, b, &db).unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 2);

        // Undo only `switch coffee 2` — the `switch coffee 1` entry stays put.
        let mut undo = LogArgs::fresh("coffee");
        undo.rest = vec!["2".to_string()];
        undo.undo = true;
        log_entry(Action::Switch, undo, &db).unwrap();
        let live = db.entries_for_field("coffee", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].state, Some(serde_json::json!(1)));
    }

    #[test]
    fn test_undo_flag_errors_when_no_match() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::fresh("coffee");
        args.rest = vec!["3".to_string()];
        args.undo = true;
        let err = log_entry(Action::Switch, args, &db).unwrap_err();
        assert!(
            err.to_string().contains("no matching entry to undo"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_no_auto_track_on_note() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::fresh("journal");
        // v3 (da-5ns): bare positional under Action::Add is rejected
        // (the v=20 schema dropped the implicit `value` column). Stage
        // the note as a kv pair, and use `-T` so the entry is timeless
        // — without an `occur_time` event, the state compiler emits
        // nothing for a Point row, which is what the original
        // "add-only ⇒ ignored" semantic relied on.
        args.rest = vec!["text=some text".to_string()];
        args.time.omit_timestamp = true;
        log_entry(Action::Add, args, &db).unwrap();
        // da-2sr: an add-only field has no state-bearing entry, so state_at
        // returns None — the projection labels it ignored (the "no entries"
        // default). It still appears in `get_tracked_fields` so callers
        // know it exists, but `now` filters it out by default.
        let tracked = db.get_tracked_fields().unwrap();
        let journal = tracked.iter().find(|f| f.field == "journal");
        assert!(journal.is_some(), "field should be present after any write");
        assert!(journal.unwrap().ignored, "add-only field defaults to ignored");
    }

    #[test]
    fn test_cmd_track_and_ignore() {
        use crate::args::{IgnoreArgs, TrackArgs};
        use datum_core::db::Db;
        use datum_core::test_util::{advance_time, set_time};
        let db = Db::open_in_memory().unwrap();

        // da-afk: cmd_track/cmd_ignore stamp entries with `DatumTime::now()`,
        // and the slug is `field:<ms-iso>`. Three back-to-back calls on a fast
        // machine can land in the same millisecond and hard-error on the slug
        // collision. Freeze the clock and step it between calls so the slugs
        // are distinct by construction.
        let _g = set_time("2026-01-15T12:00:00Z");

        let track_args = TrackArgs {
            field: "mood".to_string(),
            color: Some("ff6600".to_string()),
            state_colors: vec!["happy=00ff00".to_string(), "sad=ff0000".to_string()],
            tags: vec![],
        };
        cmd_track(track_args, &db).unwrap();

        // da-2sr: track wrote a switch entry (value=NULL) and a spec for the
        // colors. The derived projection sees mood as tracked.
        let tracked = db.get_tracked_fields().unwrap();
        let mood = tracked.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood.ignored);
        assert_eq!(mood.color.as_deref(), Some("ff6600"));

        let state_colors = db.get_all_state_colors().unwrap();
        assert_eq!(
            state_colors.get(&("mood".to_string(), "happy".to_string())).map(|s| s.as_str()),
            Some("00ff00")
        );

        advance_time("1m");
        // Ignore writes a switch IGNORED entry — supersedes the prior
        // track switch — and leaves the spec (and its color) alone.
        cmd_ignore(IgnoreArgs { field: "mood".to_string() }, &db).unwrap();
        let tracked2 = db.get_tracked_fields().unwrap();
        let mood2 = tracked2.iter().find(|f| f.field == "mood").unwrap();
        assert!(mood2.ignored);
        // Color survives because it lives on FieldSpec, not in the entry log.
        assert_eq!(mood2.color.as_deref(), Some("ff6600"));

        advance_time("1m");
        // Re-track flips it back to tracked (most recent switch wins).
        cmd_track(TrackArgs {
            field: "mood".to_string(),
            color: None,
            state_colors: vec![],
            tags: vec![],
        }, &db).unwrap();
        let tracked3 = db.get_tracked_fields().unwrap();
        assert!(!tracked3.iter().find(|f| f.field == "mood").unwrap().ignored);
    }

    #[test]
    fn cmd_spec_set_flag_flow_creates_derived_aliases() {
        use crate::args::SpecSetArgs;
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // spec set mood --default-verb switch --state happy --state sad \
        //               --color green --id-template "%field%:%state%"
        let args = SpecSetArgs {
            field: "mood".to_string(),
            json: None,
            default_verb: Some("switch".to_string()),
            states: vec!["happy".to_string(), "sad".to_string()],
            color: Some("green".to_string()),
            state_colors: vec!["happy=00ff00".to_string()],
            id_template: Some("%field%:%state%".to_string()),
            description: None,
            unit: None,
            track: true,
            ignore: false,
            id_keys: vec![],
            data_keys: vec![],
            no_autoalias: false,
            y: None,
            height: None,
            z_index: None,
            tags: vec![],
        };
        cmd_spec_set(args, &db).unwrap();

        let spec = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(spec.id_template.as_deref(), Some("%field%:%state%"));
        assert_eq!(spec.color.as_deref(), Some("green"));
        let states = spec.allowed_states.as_ref().unwrap();
        assert!(states.contains(&"happy".to_string()) && states.contains(&"sad".to_string()));
        // da-2sr: --track on `spec set` writes a switch entry; the spec
        // itself no longer carries tracked/ignored bits.
        let mood_tracked = db.get_tracked_fields().unwrap();
        let mood_row = mood_tracked.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood_row.ignored);

        // Derived aliases landed in the aliases table (hot-path lookup).
        let happy = db.lookup_alias("happy").unwrap().unwrap();
        assert_eq!(happy.expands_to, "switch mood happy");
        let sad = db.lookup_alias("sad").unwrap().unwrap();
        assert_eq!(sad.expands_to, "switch mood sad");

        // Dropping "sad" from the states list drops its alias.
        let args2 = SpecSetArgs {
            field: "mood".to_string(),
            json: None,
            default_verb: None,
            states: vec!["happy".to_string()],
            color: None,
            state_colors: vec![],
            id_template: None,
            description: None,
            unit: None,
            track: false,
            ignore: false,
            id_keys: vec![],
            data_keys: vec![],
            no_autoalias: false,
            y: None,
            height: None,
            z_index: None,
            tags: vec![],
        };
        cmd_spec_set(args2, &db).unwrap();
        assert!(db.lookup_alias("sad").unwrap().is_none());
        assert!(db.lookup_alias("happy").unwrap().is_some());
    }

    #[test]
    fn test_cmd_now_no_crash() {
        use crate::args::NowArgs;
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Empty db
        cmd_now(NowArgs { show_all: false, bare: true, ..NowArgs::default() }, &db).unwrap();

        // With some tracked fields and entries
        let mut args = LogArgs::fresh("mood");
        args.rest = vec!["happy".to_string()];
        log_entry(Action::Switch, args, &db).unwrap();
        // da-2sr: writing the switch entry alone makes mood tracked; no
        // separate set_tracked / spec write is needed.

        cmd_now(NowArgs { show_all: false, bare: true, ..NowArgs::default() }, &db).unwrap();
        cmd_now(NowArgs { show_all: true, bare: true, ..NowArgs::default() }, &db).unwrap();
    }

    // ─── id-template tests (A1 acceptance) ──────────────────────────────────

    fn log_args(field: &str, rest: Vec<String>, id: Option<String>) -> LogArgs {
        let mut args = LogArgs::fresh(field);
        args.rest = rest;
        args.id = id;
        args
    }

    #[test]
    fn spec_id_template_is_default() {
        use datum_core::db::Db;
        use datum_core::field_spec::FieldSpec;
        let db = Db::open_in_memory().unwrap();

        // Spec defines id_template; caller does NOT pass --id.
        let mut spec = FieldSpec::new("coffee", Action::Occur);
        spec.id_template = Some("%field%:%uid%".to_string());
        db.save_field_spec(&spec).unwrap();

        log_entry(Action::Occur, log_args("coffee", vec![], None), &db).unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(
            e.id_structure.as_deref(),
            Some("%field%:%uid%"),
            "spec template must be persisted as id_structure"
        );
        // slug = field ":" short_uid → must start with "coffee:" and end with 5-char uid
        let expected_uid = e.short_uid();
        assert_eq!(e.slug, format!("coffee:{expected_uid}"));
    }

    #[test]
    fn explicit_id_flag_overrides_spec_default() {
        use datum_core::db::Db;
        use datum_core::field_spec::FieldSpec;
        let db = Db::open_in_memory().unwrap();

        // Spec has one template; --id passes a different one → --id wins.
        let mut spec = FieldSpec::new("coffee", Action::Occur);
        spec.id_template = Some("%field%:%uid%".to_string());
        db.save_field_spec(&spec).unwrap();

        log_entry(
            Action::Occur,
            log_args("coffee", vec![], Some("%field%-override-%uid%".to_string())),
            &db,
        )
        .unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        assert_eq!(e.id_structure.as_deref(), Some("%field%-override-%uid%"));
        let uid = e.short_uid();
        assert_eq!(e.slug, format!("coffee-override-{uid}"));
    }

    #[test]
    fn id_template_round_trip_through_db() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        log_entry(
            Action::Occur,
            log_args("coffee", vec![], Some("%field%:%uid%".to_string())),
            &db,
        )
        .unwrap();

        // Re-open registry path and fetch by slug — must still have id_structure.
        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        let fetched = db.get_entry(&e.slug).unwrap();
        assert_eq!(fetched.id_structure.as_deref(), Some("%field%:%uid%"));
        assert_eq!(fetched.slug, e.slug);
    }

    #[test]
    fn no_template_keeps_default_slug() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        log_entry(Action::Occur, log_args("coffee", vec![], None), &db).unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        // No spec, no --id → id_structure remains None; slug follows field:time rule
        assert!(e.id_structure.is_none());
        assert!(e.slug.starts_with("coffee:"));
    }

    #[test]
    fn missing_key_collapses_delimiter_via_cli() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // occur verb has no state → %state% empty → "::" collapses to ":"
        log_entry(
            Action::Occur,
            log_args(
                "coffee",
                vec![],
                Some("%field%:%state%:%uid%".to_string()),
            ),
            &db,
        )
        .unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        let uid = e.short_uid();
        assert_eq!(e.slug, format!("coffee:{uid}"));
    }

    // ─── head command tests ─────────────────────────────────────────────────

    fn make_entry_at(field: &str, secs_offset: i64, device: &str) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        // Fixed base time + offset keeps ids/slugs distinct and orderable.
        let base: chrono::DateTime<chrono::Utc> =
            "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs_offset);
        let t = DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() };
        Entry::new_legacy(
            field,
            Verb::Point,
            Some(serde_json::json!(secs_offset)),
            None,
            None,
            Some(t),
            None,
            device,
        )
    }

    /// Build a `TailArgs` in head-mode (oldest-first) with the given field and count.
    /// Used by head regression tests to exercise the shared tail codepath.
    fn head_args(field: Option<&str>, count: Option<usize>) -> TailArgs {
        TailArgs {
            field: field.map(String::from),
            count,
            oldest_first: true,
            ..TailArgs::fresh()
        }
    }

    #[test]
    fn head_returns_oldest_first() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Insert out-of-order: newest first, then oldest, then middle.
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        cmd_tail(head_args(None, Some(10)), &db).unwrap();

        // Verify via tail_entries that ordering is ASC by time_start (head mode).
        let entries = tail_entries(&head_args(None, Some(10)), &db).unwrap();
        let values: Vec<i64> = entries
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 200, 300]);
    }

    #[test]
    fn head_respects_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        for i in 0..20 {
            db.insert_entry(&make_entry_at("mood", i * 60, "d1")).unwrap();
        }

        cmd_tail(head_args(Some("mood"), Some(5)), &db).unwrap();

        let got = tail_entries(&head_args(Some("mood"), Some(5)), &db).unwrap();
        assert_eq!(got.len(), 5);
        let values: Vec<i64> = got
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![0, 60, 120, 180, 240]);
    }

    #[test]
    fn head_filters_by_field() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("coffee", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();
        db.insert_entry(&make_entry_at("coffee", 300, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 400, "d1")).unwrap();

        cmd_tail(head_args(Some("coffee"), Some(10)), &db).unwrap();

        let got = tail_entries(&head_args(Some("coffee"), Some(10)), &db).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|e| e.field == "coffee"));
        let values: Vec<i64> = got
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 300]);
    }

    #[test]
    fn head_default_limit_is_10() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        for i in 0..15 {
            db.insert_entry(&make_entry_at("mood", i * 60, "d1")).unwrap();
        }
        // tail_params defaults count to 10 when no date window is set — same
        // default the old head command hardcoded.
        let got = tail_entries(&head_args(Some("mood"), None), &db).unwrap();
        assert_eq!(got.len(), 10);
        // First entry should be the oldest (offset=0).
        assert_eq!(got[0].value.as_ref().unwrap().as_i64().unwrap(), 0);
    }

    // ─── grep command tests ─────────────────────────────────────────────────

    fn make_grep_entry(
        field: &str,
        secs_offset: i64,
        value: serde_json::Value,
        extra: Option<serde_json::Value>,
    ) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        let base: chrono::DateTime<chrono::Utc> =
            "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs_offset);
        let t = DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() };
        Entry::new_legacy(field, Verb::Point, Some(value), extra, None, Some(t), None, "d1")
    }

    fn run_grep(db: &datum_core::db::Db, patterns: &[&str], field: Option<&str>) -> Vec<Entry> {
        let regexes: Vec<Regex> = patterns.iter().map(|p| Regex::new(p).unwrap()).collect();
        let candidates = if let Some(f) = field {
            db.entries_for_field(f, None).unwrap()
        } else {
            let far_future = chrono::Utc::now() + chrono::Duration::days(365);
            let far_past = chrono::DateTime::UNIX_EPOCH;
            db.entries_in_range(far_past, far_future).unwrap()
        };
        candidates
            .into_iter()
            .filter(|e| {
                let j = serde_json::to_string(e).unwrap();
                regexes.iter().all(|r| r.is_match(&j))
            })
            .collect()
    }

    #[test]
    fn grep_single_pattern_regex() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_grep_entry("mood", 100, serde_json::json!("happy"), None)).unwrap();
        db.insert_entry(&make_grep_entry("mood", 200, serde_json::json!("tired"), None)).unwrap();
        db.insert_entry(&make_grep_entry("coffee", 300, serde_json::json!(2), None)).unwrap();

        // Literal string match
        let got = run_grep(&db, &["happy"], None);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].value.as_ref().unwrap(), &serde_json::json!("happy"));

        // Regex alternation
        let got = run_grep(&db, &["happ|tire"], None);
        assert_eq!(got.len(), 2);

        // Regex anchoring — "^" applies to the serialized JSON start
        let got = run_grep(&db, &["nothing_here"], None);
        assert!(got.is_empty());

        // Exercise the command dispatch path for completeness
        cmd_grep(
            GrepArgs { patterns: vec!["happy".to_string()], field: None, output: Default::default() },
            &db,
        )
        .unwrap();
    }

    #[test]
    fn grep_multi_pattern_and() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Entry with both "foo" and "bar" in extra_data
        db.insert_entry(&make_grep_entry(
            "notes",
            100,
            serde_json::json!("hello"),
            Some(serde_json::json!({"tag": "foo", "project": "bar"})),
        )).unwrap();
        // Entry with only "foo"
        db.insert_entry(&make_grep_entry(
            "notes",
            200,
            serde_json::json!("world"),
            Some(serde_json::json!({"tag": "foo"})),
        )).unwrap();
        // Entry with only "bar"
        db.insert_entry(&make_grep_entry(
            "notes",
            300,
            serde_json::json!("world"),
            Some(serde_json::json!({"project": "bar"})),
        )).unwrap();

        let got = run_grep(&db, &["foo", "bar"], None);
        assert_eq!(got.len(), 1);
        let ex = got[0].data.as_ref().unwrap();
        assert_eq!(ex["tag"], serde_json::json!("foo"));
        assert_eq!(ex["project"], serde_json::json!("bar"));

        // Three patterns, only first entry has all
        let got = run_grep(&db, &["foo", "bar", "hello"], None);
        assert_eq!(got.len(), 1);

        // Unmatched combination returns none
        let got = run_grep(&db, &["foo", "bar", "missing"], None);
        assert!(got.is_empty());
    }

    #[test]
    fn grep_respects_field_filter() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // "rust" appears in both a mood entry and a project entry
        db.insert_entry(&make_grep_entry("mood", 100, serde_json::json!("excited about rust"), None)).unwrap();
        db.insert_entry(&make_grep_entry("project", 200, serde_json::json!("learn rust"), None)).unwrap();
        db.insert_entry(&make_grep_entry("mood", 300, serde_json::json!("calm"), None)).unwrap();

        // No field filter → both "rust" entries match
        let got = run_grep(&db, &["rust"], None);
        assert_eq!(got.len(), 2);

        // Filter to mood → only the mood "rust" entry matches
        let got = run_grep(&db, &["rust"], Some("mood"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].field, "mood");

        // Filter to project → only the project entry
        let got = run_grep(&db, &["rust"], Some("project"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].field, "project");

        // Field filter with no matching pattern
        let got = run_grep(&db, &["missing"], Some("mood"));
        assert!(got.is_empty());

        // Dispatch path
        cmd_grep(
            GrepArgs {
                patterns: vec!["rust".to_string()],
                field: Some("mood".to_string()),
                output: Default::default(),
            },
            &db,
        )
        .unwrap();
    }

    #[test]
    fn head_returns_all_when_fewer_than_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        cmd_tail(head_args(None, Some(10)), &db).unwrap();

        let got = tail_entries(&head_args(None, Some(10)), &db).unwrap();
        assert_eq!(got.len(), 2);
    }

    // ─── update / merge / rekey command tests ───────────────────────────────

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
        db: &datum_core::db::Db,
        field: &str,
        verb: Verb,
        value: Option<JsonValue>,
        extra: Option<JsonValue>,
    ) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime { utc: t, offset_minutes: 0, timezone: "UTC".to_string() };
        let e = Entry::new_legacy(field, verb, value, extra, None, Some(dt), None, "d1");
        db.insert_entry(&e).unwrap();
        e
    }

    #[test]
    fn update_replaces_extra_keys() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            Some(serde_json::json!(1)),
            Some(serde_json::json!({
                "weight": {"v": 70.0, "u": "kg"},
                "pose": "standing",
                "reps": 10,
            })),
        );
        // Let a moment pass so modified_at is observably greater than created_at.
        std::thread::sleep(std::time::Duration::from_millis(5));

        update_from_args(
            update_args(&e.slug, vec!["weight=80kg"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        // weight replaced
        assert_eq!(ex["weight"]["v"], serde_json::json!(80.0));
        assert_eq!(ex["weight"]["u"], serde_json::json!("kg"));
        // unmentioned keys preserved
        assert_eq!(ex["pose"], serde_json::json!("standing"));
        assert_eq!(ex["reps"], serde_json::json!(10));
        // modified_at bumped; content_hash refreshed
        assert!(after.modify_time >= e.modify_time);
        assert_ne!(after.content_hash, e.content_hash);
        assert_eq!(after.content_hash, after.expected_content_hash());
    }

    #[test]
    fn merge_preserves_untouched_keys() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "notes",
            Verb::Point,
            None,
            Some(serde_json::json!({
                "nested": {"a": 1, "b": 2},
                "other": "keep",
            })),
        );

        update_from_args(
            update_args(&e.slug, vec!["tag=red"]),
            UpdateStrategy::Merge,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        assert_eq!(ex["tag"], serde_json::json!("red"));
        // Deep-merge does NOT disturb existing keys anywhere in the tree.
        assert_eq!(ex["nested"]["a"], serde_json::json!(1));
        assert_eq!(ex["nested"]["b"], serde_json::json!(2));
        assert_eq!(ex["other"], serde_json::json!("keep"));
    }

    #[test]
    fn merge_deep_merges_nested_objects() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "notes",
            Verb::Point,
            None,
            Some(serde_json::json!({"nested": {"a": 1, "c": 3}})),
        );

        let mut args = update_args(&e.slug, vec![]);
        args.keys = vec!["nested.b=2".to_string()];
        // We use a single positional key=value with dot-notation-style to test
        // the deep-merge path. But our parser splits at first `=` so the key
        // becomes literal "nested.b" — that's fine for this test: we're
        // verifying the deep-merge code path runs without clobbering existing
        // keys. For true nested merge we'd need nested JSON — test below.
        update_from_args(args, UpdateStrategy::Merge, &db).unwrap();

        // Now overlay nested JSON via a second merge with -k syntax replaced
        // by direct extra_data manipulation through another merge call.
        // We'll exercise nested merging by passing a nested object through
        // parse_value_token's fallback (string): instead rely on the unit
        // test of `deep_merge` in update_core for that path.
        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        // Original scalar keys intact
        assert_eq!(ex["nested"]["a"], serde_json::json!(1));
        assert_eq!(ex["nested"]["c"], serde_json::json!(3));
        // Flat "nested.b" added
        assert_eq!(ex["nested.b"], serde_json::json!(2));
    }

    // `deep_merge_recurses_into_objects` lives in update_core.rs where the
    // helper now resides; this crate-level test would be redundant.

    #[test]
    fn update_empty_value_drops_key() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            None,
            Some(serde_json::json!({"foo": "bar", "baz": "qux"})),
        );

        update_from_args(
            update_args(&e.slug, vec!["foo="]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        assert!(ex.get("foo").is_none(), "foo= should drop the key");
        assert_eq!(ex["baz"], serde_json::json!("qux"));
    }

    #[test]
    fn rekey_renames_data_key_via_cli() {
        // CLI: `update <slug> --strategy rekey reps=repetitions` should
        // rename `data.reps` → `data.repetitions` in place. No new row.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            None,
            Some(serde_json::json!({"reps": 10, "set": 3})),
        );

        update_from_args(
            update_args(&e.slug, vec!["reps=repetitions"]),
            UpdateStrategy::Rekey,
            &db,
        )
        .unwrap();

        let live = db.entries_for_field("gym", None).unwrap();
        assert_eq!(live.len(), 1, "rekey is a key-rename, not a copy");
        let data = live[0].data.as_ref().unwrap();
        assert_eq!(data["repetitions"], serde_json::json!(10));
        assert!(data.get("reps").is_none());
        assert_eq!(data["set"], serde_json::json!(3));
    }

    #[test]
    fn rekey_without_mapping_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "mood", Verb::Switch, Some(serde_json::json!("x")), None);

        let err = update_from_args(update_args(&e.slug, vec![]), UpdateStrategy::Rekey, &db)
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("rekey"),
            "expected rekey-mapping error, got: {err}"
        );
    }

    #[test]
    fn update_tombstoned_row_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Point,
            Some(serde_json::json!(1)),
            Some(serde_json::json!({"reps": 10})),
        );
        db.soft_delete_entry(&e.slug).unwrap();

        // Use id (slug lookup would also find the tombstoned row via
        // get_entry_including_deleted).
        let err = update_from_args(
            update_args(&e.slug, vec!["reps=20"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("tombstoned"),
            "expected tombstone error, got: {err}"
        );

        // Merge and rekey must also reject tombstoned rows.
        let err = update_from_args(
            update_args(&e.slug, vec!["tag=red"]),
            UpdateStrategy::Merge,
            &db,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("tombstoned"));

        // Rekey: pass a valid mapping so the tombstone check (which fires
        // before strategy dispatch) is what trips, not the empty-mapping check.
        let rk = update_args(&e.slug, vec!["reps=repetitions"]);
        let err = update_from_args(rk, UpdateStrategy::Rekey, &db).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("tombstoned"));
    }

    #[test]
    fn update_resolves_unique_prefix() {
        // C3 integration smoke: a unique prefix routes through update just
        // like the full id. Use the first 5 chars of the id — prefix matching
        // is anchored at the start by `LIKE '...%'`, and the ULID timestamp
        // leader makes the head of a single-row table unique.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "gym", Verb::Point, Some(serde_json::json!(1)), None);
        let prefix: String = e.slug.chars().take(5).collect();
        update_from_args(
            update_args(&prefix, vec!["note=fixed"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();
        let fresh = db.get_entry(&e.slug).unwrap();
        assert_eq!(
            fresh.data.unwrap()["note"],
            serde_json::json!("fixed"),
        );
    }

    #[test]
    fn update_ambiguous_prefix_errors() {
        use datum_core::{db::Db, entry::Entry, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        // Two entries on the same field at distinct instants — both slugs
        // start with `gym:`, so that prefix matches multiple rows.
        let t0: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00.000Z".parse().unwrap();
        let t1: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00.001Z".parse().unwrap();
        for (t, v) in [(t0, 1), (t1, 2)] {
            let dt = DatumTime { utc: t, offset_minutes: 0, timezone: "UTC".to_string() };
            let e = Entry::new_legacy(
                "gym",
                Verb::Point,
                Some(serde_json::json!(v)),
                None,
                None,
                Some(dt),
                None,
                "d1",
            );
            db.insert_entry(&e).unwrap();
        }
        let err = update_from_args(
            update_args("gym:", vec!["note=x"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("ambiguous"), "expected ambiguous, got: {err}");
    }

    #[test]
    fn update_nonexistent_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let err = update_from_args(
            update_args("no-such-id", vec!["foo=bar"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("not found"));
    }

    #[test]
    fn update_strategy_flag_overrides_subcommand_default() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "notes",
            Verb::Point,
            None,
            Some(serde_json::json!({"tag": {"color": "red", "size": "m"}})),
        );

        // Invoke via Update default but override strategy to Merge — the nested
        // {color, size} should survive because deep-merge preserves siblings.
        let mut args = update_args(&e.slug, vec![]);
        args.strategy = Some(crate::args::ConflictStrategyArg::Merge);
        args.keys = vec!["tag.priority=1".to_string()];
        update_from_args(args, UpdateStrategy::Replace, &db).unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        assert_eq!(ex["tag"]["color"], serde_json::json!("red"));
        assert_eq!(ex["tag"]["size"], serde_json::json!("m"));
        // Our split-on-first-'=' parser treats "tag.priority" as a literal key.
        assert_eq!(ex["tag.priority"], serde_json::json!(1));
    }

    #[test]
    fn update_value_replaces_primary() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );

        update_from_args(
            update_args(&e.slug, vec!["sad"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();
        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.value, Some(serde_json::json!("sad")));
    }

    #[test]
    fn update_auto_reinterps_slug_when_id_structure_present() {
        // Headline behavior: a regular `update` on a row whose id_structure
        // references a state-bearing template var should auto-rekey the
        // slug when that var changes — no manual --strategy rekey, no
        // --id flag. da-5ns.3: `state=` writes the column; `%state%`
        // resolves to `entry.state` for Verb::Switch.
        use datum_core::db::Db;
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

        update_from_args(
            update_args(&e.slug, vec!["state=sad"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();

        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1, "auto-rekey is in-place; no new row");
        assert_eq!(live[0].slug, "mood:sad");
    }

    #[test]
    fn rekey_renames_key_referenced_by_id_structure_cascades_to_slug() {
        // Renaming a data key that the id_structure template references
        // should cascade: the rename triggers auto-reinterp, which moves
        // the slug.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut e = seed_entry(
            &db,
            "task",
            Verb::Point,
            None,
            Some(serde_json::json!({"oldKind": "chore"})),
        );
        e.id_structure = Some("%field%:%kind%".to_string());
        db.save_existing_entry(&e).unwrap();

        update_from_args(
            update_args(&e.slug, vec!["oldKind=kind"]),
            UpdateStrategy::Rekey,
            &db,
        )
        .unwrap();

        let live = db.entries_for_field("task", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].slug, "task:chore");
        let data = live[0].data.as_ref().unwrap();
        assert_eq!(data["kind"], serde_json::json!("chore"));
        assert!(data.get("oldKind").is_none());
    }

    // ─── B7: undo via soft-delete tombstone ─────────────────────────────────

    fn undo_args(slug: Option<&str>, force: bool, window: &str) -> UndoArgs {
        UndoArgs {
            id_or_slug: slug.map(str::to_string),
            to_version: None,
            force,
            window: window.to_string(),
            output: Default::default(),
        }
    }

    #[test]
    fn undo_restores_most_recent_delete_within_window() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Three entries; delete the middle one. `undo` with default window
        // must pick that one (the most recent tombstone) and bring it back.
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        let doomed = make_entry_at("mood", 200, "d1");
        let doomed_id = doomed.slug.clone();
        db.insert_entry(&doomed).unwrap();
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();

        db.soft_delete_entry(&doomed_id).unwrap();
        assert_eq!(db.entries_for_field("mood", None).unwrap().len(), 2);

        cmd_undo(undo_args(None, false, "15m"), &db).unwrap();

        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 3);
        assert!(live.iter().any(|e| e.slug == doomed_id));
    }

    #[test]
    fn undo_outside_window_errors_without_force() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("mood", 100, "d1");
        let id = e.slug.clone();
        db.insert_entry(&e).unwrap();
        db.soft_delete_entry(&id).unwrap();

        // Zero-length window: the tombstone we just created is already "too old"
        // (its deleted_at < now - 0). Undo without --force must bail out.
        let err = cmd_undo(undo_args(None, false, "0s"), &db).unwrap_err();
        assert!(
            err.to_string().contains("use --force"),
            "error should suggest --force, got: {err}"
        );

        // Row is still tombstoned.
        assert!(db.entries_for_field("mood", None).unwrap().is_empty());

        // With --force, the same call restores it regardless of window.
        cmd_undo(undo_args(None, true, "0s"), &db).unwrap();
        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].slug, id);
    }

    #[test]
    fn undo_specific_slug_targets_that_row() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Two deletes; the most recent is B, but we want to restore A specifically.
        let a = make_entry_at("mood", 100, "d1");
        let a_id = a.slug.clone();
        let a_slug = a.slug.clone();
        let b = make_entry_at("mood", 200, "d1");
        let b_id = b.slug.clone();
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        db.soft_delete_entry(&a_id).unwrap();
        db.soft_delete_entry(&b_id).unwrap();

        cmd_undo(undo_args(Some(&a_slug), false, "15m"), &db).unwrap();

        // A is back, B is still tombstoned.
        let live: Vec<_> = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].slug, a_id);
        assert!(db.get_entry_including_deleted(&b_id).unwrap().deleted_at.is_some());
    }

    // ─── B3: retime ─────────────────────────────────────────────────────────

    fn retime_args(slug: Option<&str>, offset: &str) -> RetimeArgs {
        RetimeArgs {
            slug: slug.map(str::to_string),
            offset: Some(offset.to_string()),
            time: None,
            field: None,
            after: None,
            before: None,
        }
    }

    fn make_block(
        field: &str,
        start_secs: i64,
        end_secs: i64,
        device: &str,
    ) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let start = DatumTime {
            utc: base + chrono::Duration::seconds(start_secs),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let end = DatumTime {
            utc: base + chrono::Duration::seconds(end_secs),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new_legacy(
            field,
            Verb::Switch,
            None,
            None,
            None,
            Some(start),
            Some(end),
            device,
        )
    }

    #[test]
    fn retime_single_slug_offsets_start_and_end() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_block("sleep", 0, 3600, "d1");
        let id = e.slug.clone();
        let t0_start = e.start_time.as_ref().unwrap().utc;
        let t0_end = e.end_time.as_ref().unwrap().utc;
        let hash_before = e.content_hash.clone();
        db.insert_entry(&e).unwrap();

        cmd_retime(retime_args(Some(&id), "-5m"), &db).unwrap();

        let after = db.get_entry(&id).unwrap();
        let shift = chrono::Duration::minutes(-5);
        assert_eq!(after.start_time.as_ref().unwrap().utc, t0_start + shift);
        assert_eq!(after.end_time.as_ref().unwrap().utc, t0_end + shift);
        // content_hash follows the new timestamps; modified_at moves too.
        assert_ne!(after.content_hash, hash_before);
        assert_eq!(after.content_hash, after.expected_content_hash());
        assert!(after.modify_time >= e.modify_time);
    }

    #[test]
    fn retime_field_wide_offset_filtered_by_after() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Two coffee rows straddling an --after cutoff, plus an unrelated
        // field. Only the coffee row past the cutoff should shift. Fixture
        // times sit days either side of the cutoff so local-midnight
        // interpretation doesn't muddle the test across timezones.
        let cutoff: chrono::DateTime<chrono::Utc> = "2026-04-01T00:00:00Z".parse().unwrap();
        let early = make_entry_at_utc("coffee", cutoff - chrono::Duration::days(2), "d1");
        let early_start = early.start_time.as_ref().unwrap().utc;
        let late = make_entry_at_utc("coffee", cutoff + chrono::Duration::days(2), "d1");
        let late_start = late.start_time.as_ref().unwrap().utc;
        let other = make_entry_at_utc("water", cutoff + chrono::Duration::days(3), "d1");
        let other_start = other.start_time.as_ref().unwrap().utc;
        db.insert_entry(&early).unwrap();
        db.insert_entry(&late).unwrap();
        db.insert_entry(&other).unwrap();

        let args = RetimeArgs {
            slug: None,
            offset: Some("+1h".to_string()),
            time: None,
            field: Some("coffee".to_string()),
            after: Some("2026-04-01".to_string()),
            before: None,
        };
        cmd_retime(args, &db).unwrap();

        let early_after = db.get_entry(&early.slug).unwrap();
        let late_after = db.get_entry(&late.slug).unwrap();
        let other_after = db.get_entry(&other.slug).unwrap();
        assert_eq!(early_after.start_time.as_ref().unwrap().utc, early_start,
            "pre-cutoff row must not shift");
        assert_eq!(
            late_after.start_time.as_ref().unwrap().utc,
            late_start + chrono::Duration::hours(1),
            "post-cutoff row shifts +1h"
        );
        assert_eq!(other_after.start_time.as_ref().unwrap().utc, other_start,
            "other field must not shift");
    }

    #[test]
    fn retime_negative_offset() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("focus", 10_000, "d1");
        let id = e.slug.clone();
        let t0 = e.start_time.as_ref().unwrap().utc;
        db.insert_entry(&e).unwrap();

        cmd_retime(retime_args(Some(&id), "-30m"), &db).unwrap();

        let after = db.get_entry(&id).unwrap();
        assert_eq!(
            after.start_time.as_ref().unwrap().utc,
            t0 - chrono::Duration::minutes(30)
        );
    }

    fn make_entry_at_utc(
        field: &str,
        utc: chrono::DateTime<chrono::Utc>,
        device: &str,
    ) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        let t = DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() };
        Entry::new_legacy(field, Verb::Point, None, None, None, Some(t), None, device)
    }

    #[test]
    fn delete_then_undo_is_noop() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("mood", 100, "d1");
        let id = e.slug.clone();
        let original_hash = e.content_hash.clone();
        db.insert_entry(&e).unwrap();

        crate::commands::del::del(
            DeleteArgs {
                id_or_slug: Some(id.to_string()),
                force: true,
                dry_run: false,
                strict: false,
                output: Default::default(),
            },
            &db,
        )
        .unwrap();
        cmd_undo(undo_args(None, false, "15m"), &db).unwrap();

        // Row is back, identical to the original insertion (including content_hash).
        let restored = db.get_entry(&id).unwrap();
        assert_eq!(restored.slug, id);
        assert_eq!(restored.content_hash, original_hash);
        assert!(restored.deleted_at.is_none());
        assert_eq!(db.count_entries().unwrap(), 1);
    }

    // ─── edit command tests ─────────────────────────────────────────────────
    //
    // The edit command spawns a subprocess under `sh -c` driven by the
    // DATUM_EDITOR env var. Env vars are process-global, so these tests
    // serialize against a shared mutex to avoid stepping on each other.

    use std::sync::Mutex;
    static EDIT_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Run a closure with DATUM_EDITOR set to `cmd`, holding the env lock.
    fn with_datum_editor<F: FnOnce()>(cmd: &str, f: F) {
        let _guard = EDIT_ENV_LOCK.lock().unwrap();
        let prev = std::env::var("DATUM_EDITOR").ok();
        std::env::set_var("DATUM_EDITOR", cmd);
        f();
        match prev {
            Some(v) => std::env::set_var("DATUM_EDITOR", v),
            None => std::env::remove_var("DATUM_EDITOR"),
        }
    }

    /// Build a minimal JSON payload for the editor tempfile that targets the
    /// `UserEditableEntry` shape. `occur_time` is taken verbatim from `e` so
    /// the round-trip preserves it (the deserializer lifts it back unchanged).
    fn editor_payload(e: &Entry, mutate: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut doc = serde_json::json!({
            "field": e.field,
            "verb": e.verb.to_string(),
            "state": e.state,
            "dur": e.dur,
            "following_state": e.following_state,
            "data": e.data,
            "unit": e.unit,
            "occur_time": serde_json::to_value(&e.occur_time).unwrap(),
            "occur_time_precision": e.occur_time_precision,
            "lat": e.lat,
            "lon": e.lon,
            "location": e.location,
            "id_structure": e.id_structure,
            "field_structure": e.field_structure,
        });
        mutate(&mut doc);
        serde_json::to_string_pretty(&doc).unwrap()
    }

    fn write_editor_payload(payload: &str) -> tempfile::NamedTempFile {
        let prep = tempfile::Builder::new()
            .prefix("datum-edit-prep-")
            .suffix(".json")
            .tempfile()
            .unwrap();
        std::fs::write(prep.path(), payload).unwrap();
        prep
    }

    /// Stamp `extra_metadata` directly on the row via SQL. The public
    /// `save_existing_entry` deliberately doesn't write the `extra_metadata`
    /// column (only the import / migration path does), so tests that need
    /// to seed system-managed provenance write the column directly.
    fn stamp_extra_metadata(db: &datum_core::db::Db, slug: &str, value: &JsonValue) {
        db.conn()
            .execute(
                "UPDATE entries SET extra_metadata = ?1 WHERE slug = ?2",
                rusqlite::params![value.to_string(), slug],
            )
            .unwrap();
    }

    #[test]
    fn edit_modifies_entry_via_scripted_editor() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"note": "sunny"})),
        );

        let payload = editor_payload(&e, |doc| {
            doc["state"] = serde_json::json!("focused");
            doc["data"] = serde_json::json!({"note": "clearer"});
        });
        let prep = write_editor_payload(&payload);

        let editor_cmd = format!("cp {}", prep.path().display());
        with_datum_editor(&editor_cmd, || {
            crate::commands::edit::edit(
                EditArgs { id_or_slug: e.slug.to_string(), output: Default::default() },
                &db,
            ).unwrap();
        });

        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.state, Some(serde_json::json!("focused")));
        assert_eq!(after.data, Some(serde_json::json!({"note": "clearer"})));
        // content_hash refreshed to match the new state.
        assert_eq!(after.content_hash, after.expected_content_hash());
        assert_ne!(after.content_hash, e.content_hash);
        assert!(after.modify_time >= e.modify_time);
    }

    #[test]
    fn edit_no_change_is_noop() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "coffee",
            Verb::Point,
            Some(serde_json::json!(1)),
            None,
        );
        let before = db.get_entry(&e.slug).unwrap();

        // `true` ignores its args → tempfile is left untouched.
        with_datum_editor("true", || {
            crate::commands::edit::edit(
                EditArgs { id_or_slug: e.slug.to_string(), output: Default::default() },
                &db,
            ).unwrap();
        });

        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
        assert_eq!(after.state, before.state);
    }

    #[test]
    fn edit_rejects_invalid_json() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );
        let before = db.get_entry(&e.slug).unwrap();

        let prep = tempfile::Builder::new()
            .prefix("datum-edit-prep-")
            .suffix(".txt")
            .tempfile()
            .unwrap();
        std::fs::write(prep.path(), b"{ not valid json").unwrap();

        let editor_cmd = format!("cp {}", prep.path().display());
        let result = with_datum_editor_result(&editor_cmd, || {
            crate::commands::edit::edit(
                EditArgs { id_or_slug: e.slug.to_string(), output: Default::default() },
                &db,
            )
        });
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not valid entry JSON"),
            "unexpected error: {err}"
        );

        // Row untouched.
        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
    }

    /// da-5mnz: writing a system-only key into the editor tempfile is a
    /// hard error — the legitimate edits in the same payload do NOT apply,
    /// and the row is fully unchanged. Replaces the da-4lov silent-drop
    /// behavior with fail-loud feedback so the user sees the boundary.
    #[test]
    fn edit_rejects_extra_metadata_injection() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );

        // Stamp system-managed provenance directly on the row, mimicking what
        // `datum db import --from <legacy>` would deposit.
        let original_meta = serde_json::json!({"_import": {"source": "legacy", "v": 1}});
        stamp_extra_metadata(&db, e.slug.as_str(), &original_meta);
        let before = db.get_entry(&e.slug).unwrap();
        assert_eq!(before.extra_metadata, Some(original_meta.clone()));

        let payload = editor_payload(&before, |doc| {
            // A legitimate change…
            doc["state"] = serde_json::json!("focused");
            // …paired with a system-key injection. The whole save must fail.
            doc["extra_metadata"] = serde_json::json!({"_evil": "hacked"});
        });
        let prep = write_editor_payload(&payload);

        let editor_cmd = format!("cp {}", prep.path().display());
        let result = with_datum_editor_result(&editor_cmd, || {
            crate::commands::edit::edit(
                EditArgs { id_or_slug: e.slug.to_string(), output: Default::default() },
                &db,
            )
        });
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("extra_metadata") && err.contains("system-owned"),
            "expected system-owned rejection naming extra_metadata, got: {err}"
        );

        // Row fully unchanged: the legit `state` edit did NOT apply because
        // the save was rejected wholesale.
        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.state, before.state);
        assert_eq!(after.extra_metadata, Some(original_meta));
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
    }

    /// da-5mnz: every documented system-owned top-level key is rejected
    /// individually with a message that names it. Guards against a future
    /// refactor adding a new system column without updating the boundary.
    #[test]
    fn edit_rejects_each_system_owned_key() {
        use datum_core::db::Db;
        let cases: &[(&str, JsonValue)] = &[
            ("extra_metadata", serde_json::json!({"_evil": "hacked"})),
            ("hid", serde_json::json!("ffffffffffff")),
            ("slug", serde_json::json!("attacker:owned")),
            ("content_hash", serde_json::json!("deadbeef")),
            ("create_time", serde_json::json!("1970-01-01T00:00:00Z")),
            ("modify_time", serde_json::json!("1970-01-01T00:00:00Z")),
            ("device_id", serde_json::json!("attacker-device")),
            ("parser_version", serde_json::json!(999)),
            ("raw_input", serde_json::json!("rm -rf /")),
            ("alias_chain", serde_json::json!(["evil"])),
            ("user_asserted_last_state", serde_json::json!("hacked")),
            ("deleted_at", serde_json::json!("2099-01-01T00:00:00Z")),
        ];

        for (key, attack) in cases {
            let db = Db::open_in_memory().unwrap();
            let e = seed_entry(
                &db,
                "mood",
                Verb::Switch,
                Some(serde_json::json!("happy")),
                None,
            );
            let before = db.get_entry(&e.slug).unwrap();

            let payload = editor_payload(&before, |doc| {
                doc[*key] = attack.clone();
            });
            let prep = write_editor_payload(&payload);

            let editor_cmd = format!("cp {}", prep.path().display());
            let result = with_datum_editor_result(&editor_cmd, || {
                crate::commands::edit::edit(
                    EditArgs { id_or_slug: e.slug.to_string(), output: Default::default() },
                    &db,
                )
            });
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains(*key) && err.contains("system-owned"),
                "key {key}: expected system-owned rejection, got: {err}"
            );

            // Row untouched.
            let after = db.get_entry(&e.slug).unwrap();
            assert_eq!(after.content_hash, before.content_hash, "key {key}: row mutated");
            assert_eq!(after.modify_time, before.modify_time, "key {key}: modify_time bumped");
        }
    }

    /// da-4lov complement: even when the user makes no semantic change,
    /// `extra_metadata` survives the round-trip. The editor view never carries
    /// it, so a save-without-changes still hits the "no-op" path or, if the
    /// editor whitespace-reformats, the apply path that copies user-only
    /// fields into a clone of the existing entry — extra_metadata stays put
    /// either way.
    #[test]
    fn edit_no_change_preserves_extra_metadata() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
        );
        let original_meta = serde_json::json!({"_import": {"source": "legacy"}});
        stamp_extra_metadata(&db, e.slug.as_str(), &original_meta);
        let before = db.get_entry(&e.slug).unwrap();

        with_datum_editor("true", || {
            crate::commands::edit::edit(
                EditArgs { id_or_slug: e.slug.to_string(), output: Default::default() },
                &db,
            ).unwrap();
        });

        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.extra_metadata, Some(original_meta));
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
    }

    /// Like `with_datum_editor` but returns the closure's result.
    fn with_datum_editor_result<T, F: FnOnce() -> T>(cmd: &str, f: F) -> T {
        let _guard = EDIT_ENV_LOCK.lock().unwrap();
        let prev = std::env::var("DATUM_EDITOR").ok();
        std::env::set_var("DATUM_EDITOR", cmd);
        let out = f();
        match prev {
            Some(v) => std::env::set_var("DATUM_EDITOR", v),
            None => std::env::remove_var("DATUM_EDITOR"),
        }
        out
    }


    // ─── da-e65: _LAST / _LAST_WITH_PROTECTION default targeting ─────────────
    //
    // Acceptance from the bead: bare get/delete/update/undo target the last
    // entry the user touched, with a 15-minute protection window for the
    // destructive variants. Read-only browsing (tail/head/grep) does NOT
    // bump the pointer.

    /// Bare `get` after `coffee=2` returns the just-inserted coffee entry.
    /// (Acceptance #1.)
    #[test]
    fn last_docs_bare_get_returns_most_recent_insert() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(Action::Switch, log_args("coffee", vec!["2".to_string()], None), &db).unwrap();
        let inserted = db.entries_for_field("coffee", None).unwrap()[0].slug.clone();

        cmd_get(GetArgs::fresh_default(), &db).unwrap();

        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids.len(), 1);
        // Pointer hid resolves to the same entry the get rendered.
        let resolved = datum_core::ids::resolve_quickid_single(
            &db,
            &pointer.hids[0],
        )
        .unwrap();
        assert_eq!(resolved.slug, inserted);
    }

    /// Bare `delete --force` deletes the just-inserted coffee entry without
    /// prompting. (Acceptance #3 — the prompt path is interactive and not
    /// covered here.)
    #[test]
    fn last_docs_bare_delete_force_removes_most_recent_insert() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(Action::Switch, log_args("coffee", vec!["2".to_string()], None), &db).unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 1);

        crate::commands::del::del(
            DeleteArgs {
                id_or_slug: None,
                force: true,
                dry_run: false,
                strict: false,
                output: Default::default(),
            },
            &db,
        )
        .unwrap();
        assert_eq!(db.entries_for_field("coffee", None).unwrap().len(), 0);
    }

    /// `_LAST_WITH_PROTECTION` refuses to resolve when the pointer is older
    /// than the 15-minute window. The error mentions the bypass token so
    /// the user knows what to do.
    #[test]
    fn last_docs_protection_window_rejects_stale_pointer() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Stamp the pointer with a timestamp from yesterday by hand-rolling
        // the row — `set_last_docs` always writes "now".
        let stale = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        db.conn()
            .execute(
                "INSERT INTO last_docs (id, hids, time) VALUES (1, ?1, ?2)",
                rusqlite::params!["[\"abcdef012345\"]", stale],
            )
            .unwrap();

        let err = crate::commands::del::del(
            DeleteArgs {
                id_or_slug: None,
                force: true,
                dry_run: false,
                strict: false,
                output: Default::default(),
            },
            &db,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("more than") && msg.contains("_LAST"),
            "stale-pointer error should explain the bypass: {msg}"
        );
    }

    /// `_LAST` (no protection) resolves regardless of staleness — and bare
    /// `get` defaults to it.
    #[test]
    fn last_docs_plain_last_ignores_window() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let entry = make_entry_at("mood", 100, "d1");
        let hid = entry.hid.clone();
        db.insert_entry(&entry).unwrap();

        // Stamp pointer at the entry but rewrite the time to be ancient.
        let stale = (chrono::Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        db.conn()
            .execute(
                "INSERT INTO last_docs (id, hids, time) VALUES (1, ?1, ?2)",
                rusqlite::params![format!("[\"{hid}\"]"), stale],
            )
            .unwrap();

        // get is non-destructive → defaults to _LAST → no protection check.
        cmd_get(GetArgs::fresh_default(), &db).unwrap();
    }

    /// Bare `get` against a fresh DB (no pointer ever set) errors with a
    /// "no previous documents" message rather than panicking.
    #[test]
    fn last_docs_bare_get_on_fresh_db_errors_clearly() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let err = cmd_get(GetArgs::fresh_default(), &db).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("No previous documents") || msg.contains("touch a document"),
            "fresh-DB get should surface NoLastDocsRef: {msg}"
        );
    }

    /// Read-only browsing commands (tail in particular) do NOT bump the
    /// pointer — only commands that target a specific entry do.
    #[test]
    fn last_docs_tail_does_not_update_pointer() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        cmd_tail(head_args(Some("mood"), Some(10)), &db).unwrap();

        // Pointer must still be untouched after a pure browse.
        assert!(datum_core::last_docs::get_last_docs(&db).unwrap().is_none());
    }

    /// When the pointer holds multiple hids (a future multi-insert batch
    /// or a comma-separated update), bare `get` resolves to all of them.
    /// Acceptance #4: after `dm coffee=2,coffee=3` the pointer holds two
    /// hids and a bare `dm get` returns both.
    #[test]
    fn last_docs_bare_get_returns_full_batch() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let a = make_entry_at("coffee", 100, "d1");
        let b = make_entry_at("coffee", 200, "d1");
        let (a_hid, b_hid) = (a.hid.clone(), b.hid.clone());
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();
        // Simulate a batch insert by stamping both hids in the pointer.
        datum_core::last_docs::set_last_docs(&db, &[a_hid.clone(), b_hid.clone()]).unwrap();

        // Bare `get` resolves _LAST → both entries are returned and
        // re-stamped onto the pointer (idempotent).
        cmd_get(GetArgs::fresh_default(), &db).unwrap();
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        let got: std::collections::HashSet<datum_core::ids::Hid> = pointer.hids.into_iter().collect();
        assert_eq!(
            got,
            std::collections::HashSet::from([a_hid, b_hid]),
            "bare get must surface every hid in the pointer"
        );
    }

    /// Update through the batch path stamps the pointer so a follow-up
    /// bare `get` reflects the just-mutated row.
    #[test]
    fn last_docs_update_stamps_pointer_for_followup_get() {
        use crate::commands::update_core::{update_batch_from_args, UpdateStrategy};
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(Action::Switch, log_args("coffee", vec!["2".to_string()], None), &db).unwrap();
        let slug = db.entries_for_field("coffee", None).unwrap()[0].slug.clone();

        // Mutate via the explicit-id path; the batch should write the pointer.
        update_batch_from_args(
            update_args(&slug, vec!["3"]),
            UpdateStrategy::Replace,
            &db,
        )
        .unwrap();

        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids.len(), 1);
        cmd_get(GetArgs::fresh_default(), &db).unwrap();
    }

    // ─── da-5ns.3: insert/update path wires the new schema ───────────────
    //
    // CLI scenarios from the bead description: reserved keys land on
    // entry columns, the storage layer's `derive_verb` classifies the
    // verb, and an unbound positional on `add` errors instead of silently
    // dropping the value. Unknown keys (including `value=`) fall through
    // to the user-data payload (da-53rz).

    fn make_add_args(field: &str, rest: Vec<&str>) -> LogArgs {
        let mut a = LogArgs::fresh(field);
        a.rest = rest.into_iter().map(String::from).collect();
        a
    }

    #[test]
    fn add_with_state_and_dur_lands_block_verb() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // `dm add` is the shape-inferring verb that lets reserved keys
        // drive the truth-table classification. `Action::Occur` rejects
        // explicit `state=` under da-k5o6, so the prior use of Occur here
        // was incidental — the truth-table contract belongs to Add.
        log_entry(
            Action::Add,
            make_add_args("focus", vec!["state=true", "dur=PT1H"]),
            &db,
        )
        .unwrap();
        let live = db.entries_for_field("focus", None).unwrap();
        assert_eq!(live.len(), 1);
        let e = &live[0];
        assert_eq!(e.verb, Verb::Block);
        assert_eq!(e.state, Some(serde_json::json!(true)));
        assert_eq!(e.dur.as_deref(), Some("PT1H"));
    }

    #[test]
    fn add_with_state_zero_dur_following_state_lands_point_switch() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(
            Action::Add,
            make_add_args(
                "phase",
                vec!["state=true", "dur=PT0S", "following_state=false"],
            ),
            &db,
        )
        .unwrap();
        let live = db.entries_for_field("phase", None).unwrap();
        assert_eq!(live.len(), 1);
        let e = &live[0];
        assert_eq!(e.verb, Verb::PointSwitch);
        assert_eq!(e.state, Some(serde_json::json!(true)));
        assert_eq!(e.dur.as_deref(), Some("PT0S"));
        assert_eq!(e.following_state, Some(serde_json::json!(false)));
    }

    /// da-53rz: post-da-5ns the `value` column is gone, so `value=X`
    /// must fall through to the user-data payload like any other key
    /// (lands as `data.value = X`) instead of erroring as a reserved
    /// column-bound key. Hits real SQL via `log_entry` to catch any
    /// column-routing regression.
    #[test]
    fn datum_occur_with_user_value_key_lands_as_data_key() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(
            Action::Occur,
            make_add_args("mood", vec!["value=1"]),
            &db,
        )
        .unwrap();
        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
        let e = &live[0];
        assert_eq!(
            e.data.as_ref().and_then(|d| d.get("value")),
            Some(&serde_json::json!(1)),
            "expected data.value=1, got data={:?}",
            e.data,
        );
    }

    #[test]
    fn add_rejects_unbound_positional_value() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let err = log_entry(Action::Add, make_add_args("focus", vec!["80"]), &db)
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unbound") || msg.contains("positional"),
            "expected unbound-positional rejection, got: {err}"
        );
    }

    #[test]
    fn update_state_recomputes_verb() {
        use datum_core::{db::Db, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime { utc: t, offset_minutes: 0, timezone: "UTC".to_string() };
        let e = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            None,
            Some(dt),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
        let after_seed = db.get_entry(&e.slug).unwrap();
        assert_eq!(after_seed.verb, Verb::Switch);

        // Mutate via update with a dur=PT30M — the row becomes a Block.
        crate::commands::update_core::update_from_args(
            update_args(&e.slug, vec!["dur=PT30M"]),
            crate::commands::update_core::UpdateStrategy::Replace,
            &db,
        )
        .unwrap();
        let after_update = db.get_entry(&e.slug).unwrap();
        assert_eq!(after_update.verb, Verb::Block);
        assert_eq!(after_update.dur.as_deref(), Some("PT30M"));
        assert_eq!(after_update.state, Some(serde_json::json!("happy")));
    }

    #[test]
    fn update_negative_dur_rejected() {
        use datum_core::{db::Db, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime { utc: t, offset_minutes: 0, timezone: "UTC".to_string() };
        let e = Entry::new(
            "focus",
            Verb::Switch,
            Some(serde_json::json!("on")),
            None,
            None,
            Some(dt),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
        // Negative-dur rejection lives in the storage layer's
        // `derive_verb` call; it bubbles up from the update path.
        let err = crate::commands::update_core::update_from_args(
            update_args(&e.slug, vec!["dur=-PT5M"]),
            crate::commands::update_core::UpdateStrategy::Replace,
            &db,
        )
        .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("negative") || msg.contains("dur"),
            "expected negative-dur rejection, got: {err}"
        );
    }
}
