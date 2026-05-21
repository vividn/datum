//! Command dispatch and argument → Entry conversion.

use anyhow::Result;
use chrono::Utc;
use datum_core::{
    action::Action,
    db::{Db, InsertOutcome},
    entry::{Entry, Verb},
    field_spec::{self, InterpolationContext},
    ids::{contains_wildcard, resolve_quickid, resolve_quickid_single},
    time::{
        parse_date_arg, parse_duration, parse_time_arg, resolve_timezone, DatumTime, TimeBuilder,
    },
};
use serde_json::Value as JsonValue;

use crate::{
    args::{Cli, Command, LogArgs, LogArgsExt, UpdateArgs},
    config::device_id,
    render::{
        show_create_with_context, show_delete, show_exists, show_rename, show_update_with_context,
        OutputArgs, RenderContext, Surface,
    },
};

/// Drive a raw argv through the unified comma-split-first pipeline
/// against `db`. Goes through the same
/// [`crate::args::parse_invocation_with_db`] entry point as the binary,
/// so test coverage at this layer exercises the same call graph that
/// `main.rs` runs at runtime — no parallel "tests-only" path.
pub fn dispatch_argv(argv: Vec<String>, db: &Db) -> Result<()> {
    let invocation = crate::args::parse_invocation_with_db(argv, db)?;
    dispatch_invocation(invocation, db)
}

/// Dispatch a parsed [`crate::args::Invocation`]. Single-row invocations
/// route through [`dispatch`]; multi-row invocations open the
/// multi-collector window, run each pre-parsed side, then atomically
/// commit the buffered entries with shared-occur-time normalization
/// and same-field merge.
///
/// Failure path: if any side fails during dispatch, the multi-collector
/// is drained and the error propagates — the atomic commit never runs,
/// so nothing is written.
pub fn dispatch_invocation(invocation: crate::args::Invocation, db: &Db) -> Result<()> {
    match invocation {
        crate::args::Invocation::Single(cli) => dispatch(cli, db),
        crate::args::Invocation::Multi { sides, .. } => dispatch_multi_sides(sides, db),
    }
}

fn dispatch_multi_sides(sides: Vec<Cli>, db: &Db) -> Result<()> {
    crate::args::begin_multi_collector();

    let run_sides = || -> Result<()> {
        for cli in sides {
            dispatch(cli, db)?;
        }
        Ok(())
    };

    let result = run_sides();
    let collected = crate::args::end_multi_collector().unwrap_or_default();
    result?;

    if collected.is_empty() {
        // No side produced a write-able entry (e.g. all sides were
        // non-logging verbs like `tail`). Nothing to do.
        return Ok(());
    }

    // Per spec: all sides share one occur_time. Time-flag side-scoping
    // (da-gh0.D) will revisit this — for now the leftmost side's time
    // wins and propagates to every other side. We rebuild slug and
    // content_hash for any side whose occur_time changed so the
    // identity columns reflect the canonical time.
    let (entries, outputs): (Vec<_>, Vec<_>) = collected.into_iter().unzip();
    let normalized = normalize_shared_occur_time(entries);

    // da-dayk: bucket per-side `OutputArgs` by `(field, occur_time)`
    // in first-appearance order — exactly how `merge_same_field_groups`
    // groups entries before insert. The leftmost contributing side's
    // flags drive the resulting merged row's render. The hid order
    // returned by `insert_entries_atomic` matches this group order
    // 1:1, so we can pair them up directly when rendering.
    let group_outputs = leftmost_output_per_group(&normalized, &outputs);

    let hids = db.insert_entries_atomic(normalized)?;
    if !hids.is_empty() {
        let _ = datum_core::last_docs::set_last_docs(db, &hids);
    }

    // da-dayk: render every persisted row through the same
    // `show_create_with_context` path single-row dispatch uses, with
    // the per-group `OutputArgs` so `--show none` (and friends) still
    // silence as expected. `build_render_contexts` batches the
    // state-history probes across rows.
    render_multi_inserted(db, &hids, &group_outputs);

    Ok(())
}

/// Walk `entries` in order, returning the [`OutputArgs`] of the first
/// entry seen for each unique `(field, occur_time)` key. The output
/// vector aligns with `merge_same_field_groups`' group-order output,
/// which matches the `hids` returned by
/// [`datum_core::db::Db::insert_entries_atomic`].
fn leftmost_output_per_group(
    entries: &[datum_core::entry::Entry],
    outputs: &[OutputArgs],
) -> Vec<OutputArgs> {
    use std::collections::HashSet;
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut group_outputs: Vec<OutputArgs> = Vec::new();
    for (entry, out) in entries.iter().zip(outputs.iter()) {
        let occur = entry
            .occur_time
            .as_ref()
            .map(|t| t.utc.to_rfc3339())
            .unwrap_or_default();
        let key = (entry.field.clone(), occur);
        if seen.insert(key) {
            group_outputs.push(out.clone());
        }
    }
    group_outputs
}

/// da-dayk: render every freshly-inserted multi row to stdout. Mirrors
/// the post-insert render block in [`log_entry`] but driven from the
/// hid list returned by [`datum_core::db::Db::insert_entries_atomic`].
/// `RenderContext`s are batch-built so we share one state-history probe
/// per field instead of N point probes.
fn render_multi_inserted(db: &Db, hids: &[datum_core::ids::Hid], group_outputs: &[OutputArgs]) {
    if hids.is_empty() {
        return;
    }
    let registry = load_registry(db).ok();
    let entries: Vec<Entry> = hids
        .iter()
        .filter_map(|hid| db.get_entry(hid.as_str()).ok())
        .collect();
    if entries.is_empty() {
        return;
    }
    let contexts = build_render_contexts(&entries, db);
    for (idx, entry) in entries.iter().enumerate() {
        let output = group_outputs.get(idx).cloned().unwrap_or_default();
        let ctx = contexts.get(idx).cloned().unwrap_or_default();
        show_create_with_context(entry, registry.as_ref(), &ctx, &output);
    }
}

/// Force every entry in `entries` to share the leftmost entry's
/// `occur_time` (Section B: "multi_all_sides_share_one_occur_time").
/// Slug and content_hash both depend on occur_time, so they are
/// rebuilt whenever we change the time. Entries with a non-templated
/// slug get the default `<field>:<timestamp>` rebuild; entries
/// carrying an `id_structure` template are re-interpolated against
/// the post-mutation state via [`datum_core::field_spec::interpolate`].
fn normalize_shared_occur_time(
    mut entries: Vec<datum_core::entry::Entry>,
) -> Vec<datum_core::entry::Entry> {
    let Some(shared) = entries.first().and_then(|e| e.occur_time.clone()) else {
        return entries;
    };
    for entry in entries.iter_mut().skip(1) {
        if entry.occur_time.as_ref().map(|t| &t.utc) == Some(&shared.utc) {
            continue;
        }
        entry.occur_time = Some(shared.clone());
        // da-ivld: every entry now carries an id_structure; rebuild the
        // slug as `<field>:<body>` where body = interpolate(id_structure).
        if let Some(tmpl) = entry.id_structure.clone() {
            let uid = entry.short_uid();
            let ctx = InterpolationContext::from_entry(entry, &uid);
            let body = field_spec::interpolate(&tmpl, &ctx, ":");
            entry.slug = datum_core::ids::Slug(format!("{}:{}", entry.field, body));
        } else {
            entry.slug = datum_core::entry::build_slug(&entry.field, Some(&shared));
        }
        entry.content_hash = entry.expected_content_hash();
    }
    entries
}

pub fn dispatch(cli: Cli, db: &Db) -> Result<()> {
    use crate::commands;
    match cli.command {
        // Repl is intercepted by the binary entry point before dispatch
        // (see `datum-cli::dispatch_single`). Reaching here means a
        // bare-flag invocation was routed into in-process dispatch — error
        // out instead of pretending to do work.
        Command::Repl => Err(anyhow::anyhow!(
            "no subcommand given — `datum` with only global flags is REPL-only"
        )),
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
        Command::Reparse(args) => commands::reparse::reparse(args, db),
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
fn run_log_with_alias(args: LogArgs, db: &Db, inner: fn(LogArgs, &Db) -> Result<()>) -> Result<()> {
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

    // da-ixrk: `start` never coerces a positional into state — state is
    // always `true` (or whatever `state=<v>` reserved-column sets). Any
    // bare positional must be a duration (or the `.` skip sentinel); a
    // non-duration positional is rejected so user typos like
    // `start work sit` don't silently produce a "sit" state.
    if matches!(action, Action::Start) {
        extract_start_strict_duration(&mut args)?;
    }
    // da-l3f3: For stop, state is optional (defaults to `false`), so a
    // lone bare positional that parses as a duration is the duration,
    // not the state. With two positionals, the trailing duration-parsable
    // one is dur and the earlier one is state.
    if matches!(action, Action::Stop) {
        extract_start_stop_trailing_duration(&mut args, action)?;
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

    // Resolve the entry's effective local time once up front so relative
    // offset values on `*_time` / `*_date` extra-data keys (e.g.
    // `wake_time=+30`, `nextDate=+3`) can anchor against it inside
    // `handle_data_args`. We deliberately swallow build errors here —
    // an invalid `-t`/`-d` flag surfaces from the downstream
    // `build_time_with_precision` call below (line ~410) with its full
    // error message intact; here we only want a clean base when the
    // flags parse, so `-T` and the undo early-return path keep their
    // current "no timestamp" semantics.
    let base_local = if args.time.omit_timestamp {
        None
    } else {
        build_time_with_precision(&args)
            .ok()
            .map(|(dt, _)| dt.local_time().naive_local())
    };
    let parsed = parse_log_args(&args, action, base_local)?;
    let ParsedLogArgs {
        field,
        value,
        mut extra_data,
        slug_values,
    } = parsed;

    // da-5ns.3: route reserved column-bound keys (`state=`, `dur=`,
    // `following_state=`, `occur_time=`) out of the data blob.
    // Unknown keys (including `value=`) stay in the data payload (da-53rz).
    let mut reserved = extract_reserved_columns(&mut extra_data)?;

    // da-5ns.3 strict positional gate (Action::Add only): the v=20 model
    // has no implicit `value` column, so a bare unkeyed positional with
    // no `-K` slot binding has nowhere to land. Reject with a generic
    // extra-args error naming the offending tokens — `state=<v>` is just
    // one of several ways to attach data and should not be presented as a
    // hard requirement (da-obmm).
    if matches!(action, Action::Add) && value.is_some() && args.slug_keys.is_empty() {
        let extras: Vec<&str> = args
            .rest
            .iter()
            .filter(|t| !t.contains('='))
            .map(String::as_str)
            .collect();
        let joined = extras.join(" ");
        anyhow::bail!(
            "unexpected positional argument(s) for `dm add`: {joined}. \
             Use `KEY=VALUE` to assign data (e.g. `state=...`, `foo=bar`), declare a \
             `-K key=` slug slot, or route them with `-R <bucket>` / `-C`."
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
    // slot/value handling in `parse_log_args` stays verb-agnostic. Under
    // da-r6ds the state for Switch flows through `reserved.state` (via
    // the `state=true` slot), so the expansion applies there as well.
    let value = if matches!(action, Action::Switch) {
        expand_array_states(value)?
    } else {
        value
    };
    if matches!(action, Action::Switch) {
        reserved.state = expand_array_states(reserved.state)?;
    }

    // `-u`/`--undo`: instead of inserting, locate the most recent live
    // entry whose (field, state) matches what this command would have
    // produced and soft-delete it. Restores original datum's `uu`-alias
    // muscle memory (re-run the last command with `-u` appended → undo).
    // `state=<v>` from reserved columns wins over the bare positional
    // value, mirroring the precedence used at insert below.
    if args.undo {
        let undo_state = reserved.state.as_ref().or(value.as_ref());
        let target = db
            .find_latest_for_undo(&field, undo_state)?
            .ok_or_else(|| anyhow::anyhow!("no matching entry to undo for field '{field}'"))?;
        let registry = load_registry(db).ok();
        let output = args.output();
        if args.dry_run {
            crate::render::print_entry(&target, None, None, registry.as_ref());
            eprintln!("[dry-run] would undo (soft-delete) {}", target.slug);
            return Ok(());
        }
        db.soft_delete_entry(&target.slug)?;
        show_delete(&target, registry.as_ref(), &output);
        let _ = datum_core::last_docs::set_last_docs(db, std::slice::from_ref(&target.hid));
        return Ok(());
    }

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
                None => anyhow::bail!("No open block for '{field}' to close with --last-state"),
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
                    let time_end = DatumTime {
                        utc: end_utc,
                        ..base_time.clone()
                    };
                    (Some(base_time), Some(time_end))
                }
                Action::Stop => {
                    // stop --dur: time_end=base, time_start=base-dur
                    let start_utc = base_time.utc - dur;
                    let time_start = DatumTime {
                        utc: start_utc,
                        ..base_time.clone()
                    };
                    (Some(time_start), Some(base_time))
                }
                _ => {
                    // occur/switch/add/block with --dur: set time_end=time_start+dur
                    let end_utc = base_time.utc + dur;
                    let time_end = DatumTime {
                        utc: end_utc,
                        ..base_time.clone()
                    };
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
    let mut entry = Entry::new_legacy(
        field,
        Verb::Point,
        value,
        extra_data,
        time_start,
        time_end,
        device,
    );

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
    if let Some(dp) = reserved.dur_precision {
        entry.dur_precision = Some(dp);
    }
    if let Some(fs) = reserved.following_state {
        entry.following_state = Some(fs);
    }
    let _ = reserved.occur_time;

    // da-ju71.2: emit `Dur::Moment` ("MOMENT") for Action::Occur when no
    // explicit dur was supplied. This locks the entry to Verb::Point in
    // `derive_verb` regardless of state shape, so an occur with
    // structured state (e.g. `occur drive .id=car .role=driver`) stays a
    // point-in-time event instead of being classified as Switch.
    if matches!(action, Action::Occur) && entry.dur.is_none() {
        entry.dur = Some(datum_core::dur::Dur::Moment);
    }

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
        // v=20 storage: the only precision column on the row is
        // `occur_time_precision` (ISO-8601). Stamp it from the builder
        // result so date-only events (`-D`, bare `-d`/`-y*`) and
        // explicit `±N` uncertainties survive the live insert path —
        // previously only retime + import wrote this column.
        entry.occur_time_precision = precision.to_iso_precision();
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

    // da-qjrc: field templates. If the field arg carries `%key%`
    // placeholders (e.g. `word__%srcLang%`), store the literal arg as
    // `field_structure` and interpolate it against the entry's current
    // state to produce the concrete `field`. The update path's
    // `save_existing_entry_with_auto_reinterp` already re-interpolates
    // `field_structure` on every mutation, so a later `dm update
    // <hid> srcLang=es` cascades into both `field` and `slug`.
    let delim = crate::config::resolve_id_delimiter(args.id_delimiter.as_deref());
    if field_spec::contains_template(&entry.field) {
        let template = entry.field.clone();
        let uid = entry.short_uid();
        let ctx = InterpolationContext::from_entry(&entry, &uid);
        entry.field = field_spec::interpolate(&template, &ctx, &delim);
        entry.field_structure = Some(template);
    }

    // da-ivld: id_structure is the single source of truth for slug.
    // Every entry now carries an id_structure on insert; slug is always
    // `<field>:<body>` where body = interpolate(id_structure, ctx, delim).
    // The `<field>:` prefix is implicit and never lives in the template.
    //
    // Precedence for the body template:
    //   1. `-K` slug-keys → `%key1%__%key2%` (one `%key%` per -K, joined
    //      by the canonical slug separator `__`)
    //   2. `id=<value>` positional shorthand (no `-K`, no `--id`) →
    //      literal `<value>` (joined by `__` if multiple promoted)
    //   3. `--id` template
    //   4. spec's `id_template`
    //   5. Default → `%occur_time%` if the entry has an occur_time, else
    //      the literal sentinel `unset` (preserves the
    //      `slug LIKE '%:unset'` filter used for add-only timeless entries).
    let id_structure: String = if !args.slug_keys.is_empty() {
        args.slug_keys
            .iter()
            .map(|spec| {
                let name = spec.split_once('=').map(|(n, _)| n).unwrap_or(spec);
                format!("%{name}%")
            })
            .collect::<Vec<_>>()
            .join("__")
    } else if !slug_values.is_empty() {
        // `id=<value>` positional promotion (parse_log_args populates
        // slug_values from data["id"] only when no `-K` was declared).
        // Stored as a literal — later updates don't re-key the slug.
        slug_values.join("__")
    } else if let Some(tmpl) = args.id.clone() {
        tmpl
    } else if let Some(tmpl) = db.load_spec(&entry.field)?.and_then(|s| s.id_template) {
        tmpl
    } else if entry.occur_time.is_some() {
        "%occur_time%".to_string()
    } else {
        "unset".to_string()
    };
    {
        let uid = entry.short_uid();
        let ctx = InterpolationContext::from_entry(&entry, &uid);
        let body = field_spec::interpolate(&id_structure, &ctx, &delim);
        entry.slug = datum_core::ids::Slug(format!("{}:{}", entry.field, body));
        entry.id_structure = Some(id_structure);
    }

    // da-8y2q.2: derive the storage verb up-front so the lead/trail-state
    // capture (which dispatches on `entry.verb`) sees the same shape the
    // insert path will write. Without this, `capture_lead_state` would key
    // off the `Verb::Point` placeholder set by `Entry::new_legacy` and
    // skip state-bearing rows.
    if let Ok((derived_verb, _)) = datum_core::derive_verb(
        entry.state.clone(),
        entry.dur.as_ref(),
        entry.following_state.clone(),
    ) {
        entry.verb = derived_verb;
    }

    // --dry-run: preview the would-be entry — including slug derivation,
    // reserved-column routing, and state canonicalization — without
    // touching the DB or the multi-collector. Skips `_LAST` pointer
    // updates so a dry-run never moves the last-accessed cursor. Exits 0
    // so `dm <verb> --dry-run` is safe to chain.
    if args.dry_run {
        let registry = load_registry(db).ok();
        let ctx = RenderContext::for_entry(db, &entry, Surface::Action, None)?;
        let output = args.output();
        crate::render::print_entry(&entry, Some(&ctx), None, registry.as_ref());
        if !output.silent() {
            eprintln!("[dry-run] would {action} {}", entry.slug);
        }
        return Ok(());
    }

    // da-gh0.C: when a multi-entry write window is active, hand the
    // fully prepared entry off to the dispatch-layer collector and
    // return. The multi driver normalizes occur_time across sides,
    // applies same-field merge, and writes the batch atomically; it
    // also owns the post-write display + `_LAST` stamp. None of the
    // per-side render/insert work below applies in that case. The
    // side's `OutputArgs` rides along so the multi driver can honour
    // per-side flags (`--show none`, format strings) when rendering
    // the resulting merged row (da-dayk).
    if crate::args::push_to_multi_collector(entry.clone(), args.output()) {
        return Ok(());
    }

    let strategy = args.conflict_strategy();
    let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
    let registry = load_registry(db).ok();
    let output = args.output();
    // Per da-9p2u: all four context fields (lead/trail/ongoing/first) are
    // derived from a single Action-surface probe so write-path and table-path
    // renders agree on transition history. The `last_state_at` helpers used
    // inside `for_entry` exclude this row by hid, so calling post-insert is
    // safe even though the row is now in the DB.
    let ctx = RenderContext::for_entry(db, &entry, Surface::Action, None)?;
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
        InsertOutcome::Rekeyed {
            new_slug, old_slug, ..
        } => {
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

/// Parsed LogArgs — distinguishes the slug-contributing id-key values from
/// the rest of the extra_data payload so callers can build the slug from
/// `-K` slots without re-walking the argv.
pub(crate) struct ParsedLogArgs {
    pub(crate) field: String,
    pub(crate) value: Option<JsonValue>,
    pub(crate) extra_data: Option<JsonValue>,
    /// Values captured from `-K` slots, in declaration order. Empty when
    /// no slug-contributing slot was filled.
    pub(crate) slug_values: Vec<String>,
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
pub(crate) fn parse_log_args(
    args: &LogArgs,
    action: Action,
    base_local: Option<chrono::NaiveDateTime>,
) -> Result<ParsedLogArgs> {
    use datum_parse::input::data_args::{handle_data_args, DataArgs};
    use datum_parse::input::flexible_positional::flexible_positional;

    // All data-taking actions route the field through the data-slot
    // machinery so positional `abc` and key=value `field=abc` are
    // interchangeable; the resolved field is pulled out of `datum_data`
    // after handle_data_args runs. da-r6ds extends this to
    // Switch/Start/Stop, matching the original datum's
    // flexiblePositional pattern across the verb family.
    let flex_field = matches!(
        action,
        Action::Add | Action::Occur | Action::Switch | Action::Start | Action::Stop
    );
    let mut field = if flex_field {
        String::new()
    } else {
        args.field_or_verb.clone()
    };

    // da-ju71.4: state-aware path translation is on (no `flat_paths`
    // compat shim) — `state=<v>` lands at `state.id` and leading-dot
    // `.x=<v>` keys fold into `state.x`. The bare positional state for
    // state-bearing verbs is handled below.
    let mut data_args = DataArgs {
        preserve_null_strings: true,
        base_local,
        ..DataArgs::default()
    };
    let mut slug_paths: Vec<String> = Vec::new();
    for spec in &args.slug_keys {
        let (name, _) = spec
            .split_once('=')
            .map(|(n, v)| (n, Some(v)))
            .unwrap_or((spec.as_str(), None));
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
        let (name, _) = spec
            .split_once('=')
            .map(|(n, v)| (n, Some(v)))
            .unwrap_or((spec.as_str(), None));
        if name.is_empty() {
            anyhow::bail!("empty id-key name in spec '{spec}'");
        }
        data_args.keys.push(spec.clone());
    }

    for c in &args.comment {
        data_args.comment.push(c.clone());
    }

    // da-r6ds: For `Switch`, peel the FIRST bare positional out of the
    // data queue and inject it as the value for the `state=true` slot
    // via `flexible_positional`. This keeps the state from competing with
    // `-k` slot-fill — a switch user writing `switch project household -k
    // optional=50` wants the state to be `household` and `optional` to
    // default to `50`, not for `household` to consume the optional slot.
    //
    // For `Start`/`Stop`, state is HARD-SET by the action canonicalization
    // downstream (see the `entry.state.is_none()` block in `log_entry`);
    // a bare positional must NEVER coerce into state. `extract_start_*_*`
    // helpers already peeled any duration positional, and the strict
    // helper rejects non-duration positionals on start. Stop is permissive:
    // residual non-duration positionals fall through to slot-fill / the
    // remainder bucket, but never to state.
    //
    // `KEY=VALUE` positionals are never state candidates; they go through
    // handle_data_args as explicit assignments regardless of action.
    let mut switch_state_positional: Option<String> = None;
    for pos in &args.rest {
        if matches!(action, Action::Switch)
            && switch_state_positional.is_none()
            && !pos.contains('=')
        {
            switch_state_positional = Some(pos.clone());
        } else {
            data_args.data.push(pos.clone());
        }
    }

    // da-r6ds: For Switch, the state slot is `state=true` (default `true`
    // when no positional supplies a value, matching TS #7 / da-5ns).
    // `flexible_positional` prepends the key+value at the front of both
    // lists; calls are made in reverse-order so field ends up first.
    //
    // `.` is the use-default-or-skip sentinel (da-1n61): drop the value
    // but keep the `state=true` slot so the default still fires.
    // `\.` escapes back to the literal `"."` string and is injected
    // normally.
    if matches!(action, Action::Switch) {
        let state_skipped = matches!(switch_state_positional.as_deref(), Some("."));
        let inject_value = switch_state_positional.filter(|_| !state_skipped);
        if inject_value.is_some() {
            flexible_positional(&mut data_args, inject_value, "state=true", false, false);
        } else {
            // Value None (no positional, or `.` sentinel): inject just the
            // slot declaration so the unfilled-keys phase applies the
            // `true` default.
            data_args.keys.insert(0, "state=true".to_string());
        }
    }

    // Flex-field actions: inject the captured `field_or_verb` positional
    // into the data stream as if the user had supplied it with `field=`
    // syntax, so `dm occur field=abc` and `dm occur abc` are
    // interchangeable. The bare positional fills the slot via slot-fill;
    // an explicit `field=<x>` later in argv overrides via the standard
    // last-write-wins in handle_data_args.
    if flex_field {
        flexible_positional(
            &mut data_args,
            Some(args.field_or_verb.clone()),
            "field",
            false,
            false,
        );
    }

    // Remainder bucket policy:
    //   * `-R`/`--remainder` → user-named bucket.
    //   * `-C`/`--comment-remainder` → "comment" bucket.
    //   * Action::Occur → no remainder; an unbound positional is an error
    //     (occur doesn't carry state, has no `value` column to absorb it,
    //     and silently routing to extra_data would mask user mistakes).
    //   * Action::Start | Action::Stop → no remainder (da-r6ds): state is
    //     hard-set by action canonicalization, so an unbound positional
    //     has nowhere to land. Erroring out matches Start's pre-existing
    //     strictness (extract_start_strict_duration) and prevents Stop
    //     from silently coercing typos into state.
    //   * Otherwise → PRIMARY_VALUE_KEY sentinel; Switch's state slot
    //     already consumed the leading bare positional above, so the
    //     remainder only catches *extra* positionals after state.
    let user_remainder = args.remainder.is_some() || args.comment_remainder;
    data_args.remainder = if let Some(r) = &args.remainder {
        Some(r.clone())
    } else if args.comment_remainder {
        Some("comment".to_string())
    } else if matches!(action, Action::Occur | Action::Start | Action::Stop) {
        None
    } else {
        Some(PRIMARY_VALUE_KEY.to_string())
    };
    data_args.string_remainder = args.string_remainder;
    data_args.comment_remainder = args.comment_remainder;

    let mut datum_data = handle_data_args(&data_args).map_err(anyhow::Error::from)?;

    // Flex-field actions: pull the resolved field out of `datum_data` so
    // it lands on entry.field rather than leaking into extra_data. The
    // slot was injected via flexible_positional above; slot-fill (or an
    // explicit `field=<x>` later in argv) puts the value at the "field"
    // path, and last-write-wins gives the latter precedence.
    if flex_field {
        match datum_data.remove("field") {
            Some(JsonValue::String(s)) => field = s,
            Some(other) => field = datum_value_to_slug_string(Some(&other)),
            None => {}
        }
    }

    // da-ju71.4: collapse the trivial `state: {id: x}` Object back to a
    // scalar when no other `state.*` keys are present, so a user's bare
    // `state=true` round-trips to `entry.state = true` instead of getting
    // gratuitously nested as `{id: true}`. The Structured state shape is
    // still produced when dot-prefixed siblings are present.
    if let Some(JsonValue::Object(state_obj)) = datum_data.get_mut("state") {
        if state_obj.len() == 1 {
            if let Some(id) = state_obj.remove("id") {
                datum_data.insert("state".to_string(), id);
            }
        }
    }

    // Value extraction: pull from the PRIMARY_VALUE_KEY remainder bucket.
    // When the user redirected the remainder via -R/-C, value is
    // intentionally empty. State for Switch now flows through
    // `datum_data["state"]` via the `state=true` slot above, and is
    // extracted onto `entry.state` by `extract_reserved_columns`
    // downstream; Start/Stop hard-set state in action canonicalization,
    // so they never reach this remainder either way.
    let _ = switch_state_positional; // consumed above; retained for clarity
    let value = if user_remainder {
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
        slug_values.push(datum_value_to_slug_string(get_path_value(
            &datum_data,
            path,
        )));
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

    Ok(ParsedLogArgs {
        field,
        value,
        extra_data,
        slug_values,
    })
}

/// Translate a `-K`/`-k` raw key name to the path `handle_data_args`
/// writes to. Post-da-ju71.4 the parser does state-aware translation, so
/// `state` → `state.id` and leading-dot `.x` → `state.x`; this helper
/// must mirror that for slot-fill lookup parity.
fn canonical_data_path(raw: &str) -> String {
    if raw == "state" {
        "state.id".to_string()
    } else if let Some(rest) = raw.strip_prefix('.') {
        format!("state.{rest}")
    } else {
        raw.to_string()
    }
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
pub(crate) fn extract_switch_trailing_reserved(args: &mut LogArgs) -> Result<()> {
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
        // da-1n61: `.` in the dur slot is the use-default-or-skip
        // sentinel — drop it without recording a duration.
        if token == "." {
            continue;
        }
        if parse_duration(&token).is_err() {
            anyhow::bail!(
                "switch: trailing positional '{token}' is not a valid duration. \
                 Use comma-separated states for multi-state switch (e.g. `a,b`)."
            );
        }
        if trailing_duration.is_some() {
            anyhow::bail!("switch: multiple trailing duration positionals; expected at most one");
        }
        trailing_duration = Some(token);
    }
    args.rest = new_rest;
    if let Some(dur) = trailing_duration {
        if args.duration.duration.is_some() {
            anyhow::bail!("switch: trailing duration positional conflicts with --dur");
        }
        args.duration.duration = Some(dur);
    }
    Ok(())
}

/// da-l3f3: For Start/Stop, peel a trailing bare positional that parses as
/// a duration into args.duration. Differs from
/// [`extract_switch_trailing_reserved`] in that state is optional for
/// start/stop (defaults to `true`), so a single bare-numeric positional
/// is interpreted as a duration rather than as state. The peel walks from
/// the right and takes the last bare positional iff it parses as a
/// duration; any earlier bare positional is left for parse_log_args to
/// consume as the state value.
pub(crate) fn extract_start_stop_trailing_duration(
    args: &mut LogArgs,
    action: Action,
) -> Result<()> {
    // da-r6ds: strip `.` / `\.` skip sentinels — after the bead, stop's
    // state is hard-set by action canonicalization, so there's no
    // positional state slot to "use default" for. Removing them here
    // keeps the data queue clean for handle_data_args downstream.
    args.rest.retain(|t| t != "." && t != r"\.");

    let last_bare_idx = args.rest.iter().rposition(|t| !t.contains('='));
    let Some(idx) = last_bare_idx else {
        return Ok(());
    };
    if parse_duration(&args.rest[idx]).is_err() {
        return Ok(());
    }
    let action_name = match action {
        Action::Start => "start",
        Action::Stop => "stop",
        _ => unreachable!("extract_start_stop_trailing_duration called for {action:?}"),
    };
    if args.duration.duration.is_some() {
        anyhow::bail!("{action_name}: trailing duration positional conflicts with --dur");
    }
    let dur = args.rest.remove(idx);
    args.duration.duration = Some(dur);
    Ok(())
}

/// da-ixrk: strict positional handling for `Action::Start`. State on start
/// is always `true` (or the explicit `state=<v>` reserved column); a bare
/// positional may never become state. The only bare positionals accepted
/// after `field` are a duration (peeled into `args.duration.duration`) or
/// the `.` / `\.` skip sentinel (removed in place, no-op). Anything else
/// is rejected so typos like `start work sit` don't silently land an
/// unintended state.
pub(crate) fn extract_start_strict_duration(args: &mut LogArgs) -> Result<()> {
    // da-r6ds: strip `.` / `\.` skip sentinels — state is hard-set to
    // `true` by action canonicalization, so there's no positional state
    // slot to "use default" for. Stripping here keeps the data queue
    // clean for handle_data_args downstream.
    args.rest.retain(|t| t != "." && t != r"\.");

    let mut duration_idx: Option<usize> = None;
    for (i, token) in args.rest.iter().enumerate() {
        if token.contains('=') {
            continue;
        }
        if parse_duration(token).is_ok() {
            if let Some(prev) = duration_idx {
                anyhow::bail!(
                    "start: only one duration positional allowed (got `{}` and `{}`)",
                    args.rest[prev],
                    token,
                );
            }
            duration_idx = Some(i);
            continue;
        }
        anyhow::bail!(
            "start: positional `{token}` is not a duration. `start` always sets \
             state=true; use `state=<v>` to set an explicit state, or pass a \
             duration like `30m` / `10m30s` / `--dur 30m`."
        );
    }
    if let Some(idx) = duration_idx {
        if args.duration.duration.is_some() {
            anyhow::bail!("start: trailing duration positional conflicts with --dur");
        }
        let dur = args.rest.remove(idx);
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
pub(crate) fn validate_switch_state_token(s: &str) -> Result<()> {
    if !s.contains(',') {
        return Ok(());
    }
    if s.trim_matches(',').contains(",,") {
        anyhow::bail!("invalid state list '{s}': empty segment between commas");
    }
    Ok(())
}

/// S12: for `Verb::Switch`, expand any comma-containing string in the
/// primary value into an array of states. Handles three inputs:
///   * scalar string → array if comma-separated, otherwise unchanged
///   * array (from legacy multi-positional switch) → flatten any
///     comma-containing string elements into their components
///   * all other value shapes → passthrough
pub(crate) fn expand_array_states(value: Option<JsonValue>) -> Result<Option<JsonValue>> {
    let Some(v) = value else {
        return Ok(None);
    };
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
    if s == "true" {
        return JsonValue::Bool(true);
    }
    if s == "false" {
        return JsonValue::Bool(false);
    }
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
        let mk = |utc: chrono::DateTime<chrono::Utc>| DatumTime {
            utc,
            ..base_time.clone()
        };

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
    pub(crate) dur: Option<datum_core::dur::Dur>,
    pub(crate) dur_precision: Option<String>,
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
            let s = json_to_dur_string(&v)?;
            out.dur = Some(
                datum_core::dur::Dur::parse(&s)
                    .ok_or_else(|| anyhow::anyhow!("invalid dur value: {s:?}"))?,
            );
        }
    }
    if let Some(v) = obj.remove("dur_precision") {
        // `dur_precision=null` is a sentinel for "clear precision".
        if v.is_null() || matches!(v.as_str(), Some("null")) {
            // intentionally drop — leave entry.dur_precision as None.
        } else {
            out.dur_precision = Some(json_to_raw_token(&v));
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
    Ok(DatumTime {
        utc,
        ..base.clone()
    })
}

/// Build the log entry's base time together with its effective precision
/// (implicit from `-q*` stacks or explicit from a `±N` uncertainty suffix).
/// Reject time-specifying flags when the caller passed `-T/--no-timestamp`.
/// A timeless entry has no place for time offsets or duration — silently
/// discarding them would mask a user misconception, so we bail with a list
/// of the offending flags.
fn check_no_time_flags(args: &LogArgs) -> Result<()> {
    let mut bad: Vec<&str> = Vec::new();
    if args.time.time.is_some() {
        bad.push("-t/--time");
    }
    if args.time.date.is_some() {
        bad.push("-d/--date");
    }
    if args.duration.duration.is_some() {
        bad.push("--dur");
    }
    if args.time.timezone.is_some() {
        bad.push("-z/--timezone");
    }
    if args.time.yesterday > 0 {
        bad.push("-y");
    }
    if args.time.quick > 0 {
        bad.push("-q");
    }
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
        .with_timezone(tz_name)
        .with_full_day(args.time.full_day);

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
        .map(|e| RenderContext::from_events(cache.get(&e.field), e, db))
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

/// The latest probe time the batch will issue for this entry. Block rows
/// probe at `end_time`; everything else at `start_time`, falling back to
/// `create_time` for state-affecting rows that lack a start.
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
    let mut seen: std::collections::HashSet<datum_core::ids::Slug> =
        std::collections::HashSet::new();
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
                    anyhow::Error::from(datum_core::error::DatumError::NotFound(token.to_string())),
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

// Open an entry in `$EDITOR` as pretty JSON; on save, re-parse and persist.
//
// Testing hook: `DATUM_EDITOR` takes precedence over `EDITOR`. Both are
// invoked as `sh -c "$editor \"$1\"" -- <tmpfile>`, so tests can set
// `DATUM_EDITOR="cp /path/to/prepared.json"` to simulate a scripted edit.
//
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
) -> Result<(String, Option<JsonValue>, Option<JsonValue>)> {
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
    let resolved_id = resolved_id
        .unwrap_or_else(|| datum_core::ids::resolve::LAST_WITH_PROTECTION_TOKEN.to_string());

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

    // da-ju71.4: same trivial-state collapse as `parse_log_args` — a bare
    // `state=v` should round-trip as a scalar, not as `{id: v}`.
    if let Some(JsonValue::Object(state_obj)) = datum_data.get_mut("state") {
        if state_obj.len() == 1 {
            if let Some(id) = state_obj.remove("id") {
                datum_data.insert("state".to_string(), id);
            }
        }
    }

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
    Ok((resolved_id, value, extra_data))
}

// ─── Why ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{DeleteArgs, GetArgs, RetimeArgs, TailArgs, UndoArgs};
    use crate::commands::tail::tail_entries;
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
        assert_eq!(
            parse_value_token("happy"),
            JsonValue::String("happy".to_string())
        );
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
        let entry = Entry::new_legacy("test", Verb::Point, None, None, Some(t), None, "dev");
        let slug = entry.slug.clone();
        db.insert_entry(&entry).unwrap();

        // get
        let fetched = db.get_entry(&slug).unwrap();
        assert_eq!(fetched.slug, slug);

        // delete
        db.delete_entry(&slug).unwrap();
        let err = db.get_entry(&slug);
        assert!(matches!(
            err,
            Err(datum_core::error::DatumError::NotFound(_))
        ));
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
        assert!(
            journal.unwrap().ignored,
            "add-only field defaults to ignored"
        );
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
        crate::commands::track::track(track_args, &db).unwrap();

        // da-2sr: track wrote a switch entry (value=NULL) and a spec for the
        // colors. The derived projection sees mood as tracked.
        let tracked = db.get_tracked_fields().unwrap();
        let mood = tracked.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood.ignored);
        assert_eq!(mood.color.as_deref(), Some("ff6600"));

        let state_colors = db.get_all_state_colors().unwrap();
        assert_eq!(
            state_colors
                .get(&("mood".to_string(), "happy".to_string()))
                .map(|s| s.as_str()),
            Some("00ff00")
        );

        advance_time("1m");
        // Ignore writes a switch IGNORED entry — supersedes the prior
        // track switch — and leaves the spec (and its color) alone.
        crate::commands::ignore::ignore(
            IgnoreArgs {
                field: "mood".to_string(),
            },
            &db,
        )
        .unwrap();
        let tracked2 = db.get_tracked_fields().unwrap();
        let mood2 = tracked2.iter().find(|f| f.field == "mood").unwrap();
        assert!(mood2.ignored);
        // Color survives because it lives on FieldSpec, not in the entry log.
        assert_eq!(mood2.color.as_deref(), Some("ff6600"));

        advance_time("1m");
        // Re-track flips it back to tracked (most recent switch wins).
        crate::commands::track::track(
            TrackArgs {
                field: "mood".to_string(),
                color: None,
                state_colors: vec![],
                tags: vec![],
            },
            &db,
        )
        .unwrap();
        let tracked3 = db.get_tracked_fields().unwrap();
        assert!(!tracked3.iter().find(|f| f.field == "mood").unwrap().ignored);
    }

    #[test]
    fn test_cmd_now_no_crash() {
        use crate::args::NowArgs;
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Empty db
        crate::commands::now::now(
            NowArgs {
                show_all: false,
                bare: true,
                ..NowArgs::default()
            },
            &db,
        )
        .unwrap();

        // With some tracked fields and entries
        let mut args = LogArgs::fresh("mood");
        args.rest = vec!["happy".to_string()];
        log_entry(Action::Switch, args, &db).unwrap();
        // da-2sr: writing the switch entry alone makes mood tracked; no
        // separate set_tracked / spec write is needed.

        crate::commands::now::now(
            NowArgs {
                show_all: false,
                bare: true,
                ..NowArgs::default()
            },
            &db,
        )
        .unwrap();
        crate::commands::now::now(
            NowArgs {
                show_all: true,
                bare: true,
                ..NowArgs::default()
            },
            &db,
        )
        .unwrap();
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

        // da-ivld: id_structure is body-only; the `<field>:` prefix is implicit.
        let mut spec = FieldSpec::new("coffee", Action::Occur);
        spec.id_template = Some("%uid%".to_string());
        db.save_field_spec(&spec).unwrap();

        log_entry(Action::Occur, log_args("coffee", vec![], None), &db).unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(
            e.id_structure.as_deref(),
            Some("%uid%"),
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
        spec.id_template = Some("%uid%".to_string());
        db.save_field_spec(&spec).unwrap();

        log_entry(
            Action::Occur,
            log_args("coffee", vec![], Some("override-%uid%".to_string())),
            &db,
        )
        .unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        assert_eq!(e.id_structure.as_deref(), Some("override-%uid%"));
        let uid = e.short_uid();
        assert_eq!(e.slug, format!("coffee:override-{uid}"));
    }

    #[test]
    fn id_template_round_trip_through_db() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        log_entry(
            Action::Occur,
            log_args("coffee", vec![], Some("%uid%".to_string())),
            &db,
        )
        .unwrap();

        // Re-open registry path and fetch by slug — must still have id_structure.
        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        let fetched = db.get_entry(&e.slug).unwrap();
        assert_eq!(fetched.id_structure.as_deref(), Some("%uid%"));
        assert_eq!(fetched.slug, e.slug);
    }

    #[test]
    fn no_template_keeps_default_slug() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        log_entry(Action::Occur, log_args("coffee", vec![], None), &db).unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        // da-ivld: no --id and no spec template ⇒ default-populate id_structure
        // from the entry's occur_time. Slug stays `<field>:<formatted occur_time>`.
        assert_eq!(e.id_structure.as_deref(), Some("%occur_time%"));
        assert!(e.slug.starts_with("coffee:"));
    }

    #[test]
    fn missing_key_collapses_delimiter_via_cli() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // occur verb has no state → %state% empty → "::" collapses to ":"
        log_entry(
            Action::Occur,
            log_args("coffee", vec![], Some("%state%:%uid%".to_string())),
            &db,
        )
        .unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        let uid = e.short_uid();
        assert_eq!(e.slug, format!("coffee:{uid}"));
    }

    // ─── field-template (da-qjrc) tests ─────────────────────────────────────

    #[test]
    fn field_template_writes_field_structure_and_interpolates() {
        // Bead da-qjrc worked example. The slot-filling order of
        // `parse_log_args` consumes `-K` slots before `-k` slots, so the
        // test pins `srcLang` via the `srcLang=pt` reserved-key form to
        // avoid coupling to positional order — the headline behaviour
        // under test is template detection + field interpolation, not
        // slot-fill mechanics.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        let mut args = LogArgs::fresh("word__%srcLang%");
        args.slug_keys = vec!["pos".to_string(), "source".to_string()];
        args.remainder = Some("en".to_string());
        args.rest = vec![
            "srcLang=pt".to_string(),
            "noun".to_string(),
            "fundo".to_string(),
            "fund".to_string(),
            "bottom".to_string(),
        ];

        log_entry(Action::Add, args, &db).unwrap();

        let entries = db.entries_for_field("word__pt", None).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.field, "word__pt");
        assert_eq!(e.field_structure.as_deref(), Some("word__%srcLang%"));
        assert_eq!(e.id_structure.as_deref(), Some("%pos%__%source%"));
        assert_eq!(e.slug, "word__pt:noun__fundo");
        let data = e.data.as_ref().expect("data populated");
        assert_eq!(data["srcLang"], serde_json::json!("pt"));
        assert_eq!(data["pos"], serde_json::json!("noun"));
        assert_eq!(data["source"], serde_json::json!("fundo"));
        assert_eq!(data["en"], serde_json::json!(["fund", "bottom"]));
    }

    #[test]
    fn field_template_update_cascades_to_field_and_slug() {
        // Re-interpolating field_structure on update must move the field
        // (and therefore the slug). Acceptance criterion from the bead.
        use crate::commands::update_core::{update_with_strategy, UpdateRequest, UpdateStrategy};
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        let mut args = LogArgs::fresh("word__%srcLang%");
        args.slug_keys = vec!["pos".to_string(), "source".to_string()];
        args.rest = vec![
            "srcLang=pt".to_string(),
            "noun".to_string(),
            "fundo".to_string(),
        ];
        log_entry(Action::Add, args, &db).unwrap();

        let original = db
            .entries_for_field("word__pt", None)
            .unwrap()
            .pop()
            .unwrap();

        update_with_strategy(
            UpdateRequest {
                id_or_slug: &original.slug,
                payload_value: None,
                payload_extra: Some(serde_json::json!({"srcLang": "es"})),
                strategy: UpdateStrategy::Replace,
                slug_delimiter: ":",
            },
            &db,
        )
        .unwrap();

        let live = db.entries_for_field("word__es", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].field, "word__es");
        assert_eq!(live[0].field_structure.as_deref(), Some("word__%srcLang%"));
        assert_eq!(live[0].slug, "word__es:noun__fundo");
    }

    #[test]
    fn no_field_template_leaves_field_structure_none() {
        // Bare field (no `%key%`) ⇒ field_structure stays None.
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        log_entry(Action::Occur, log_args("coffee", vec![], None), &db).unwrap();

        let entries = db.entries_for_field("coffee", None).unwrap();
        let e = &entries[0];
        assert_eq!(e.field, "coffee");
        assert_eq!(e.field_structure, None);
    }

    // ─── head command tests ─────────────────────────────────────────────────

    fn make_entry_at(field: &str, secs_offset: i64, device: &str) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        // Fixed base time + offset keeps ids/slugs distinct and orderable.
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs_offset);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new_legacy(
            field,
            Verb::Point,
            Some(serde_json::json!(secs_offset)),
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

        crate::commands::tail::tail(head_args(None, Some(10)), &db).unwrap();

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
            db.insert_entry(&make_entry_at("mood", i * 60, "d1"))
                .unwrap();
        }

        crate::commands::tail::tail(head_args(Some("mood"), Some(5)), &db).unwrap();

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
        db.insert_entry(&make_entry_at("coffee", 100, "d1"))
            .unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();
        db.insert_entry(&make_entry_at("coffee", 300, "d1"))
            .unwrap();
        db.insert_entry(&make_entry_at("mood", 400, "d1")).unwrap();

        crate::commands::tail::tail(head_args(Some("coffee"), Some(10)), &db).unwrap();

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
            db.insert_entry(&make_entry_at("mood", i * 60, "d1"))
                .unwrap();
        }
        // tail_params defaults count to 10 when no date window is set — same
        // default the old head command hardcoded.
        let got = tail_entries(&head_args(Some("mood"), None), &db).unwrap();
        assert_eq!(got.len(), 10);
        // First entry should be the oldest (offset=0).
        assert_eq!(got[0].value.as_ref().unwrap().as_i64().unwrap(), 0);
    }

    #[test]
    fn head_returns_all_when_fewer_than_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        crate::commands::tail::tail(head_args(None, Some(10)), &db).unwrap();

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
            id_delimiter: None,
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
        let dt = DatumTime {
            utc: t,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new_legacy(field, verb, value, extra, Some(dt), None, "d1");
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
        // da-ju71.4: dotted paths are real paths now (the old flat_paths
        // compat shim is gone), so `nested.b=2` deep-sets into the
        // existing `nested` object. Original keys must survive the merge.
        update_from_args(args, UpdateStrategy::Merge, &db).unwrap();

        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
        // Original scalar keys intact (deep-merge preserved them)
        assert_eq!(ex["nested"]["a"], serde_json::json!(1));
        assert_eq!(ex["nested"]["c"], serde_json::json!(3));
        // Newly-set dotted path lands inside the nested object
        assert_eq!(ex["nested"]["b"], serde_json::json!(2));
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
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("x")),
            None,
        );

        let err =
            update_from_args(update_args(&e.slug, vec![]), UpdateStrategy::Rekey, &db).unwrap_err();
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
        assert_eq!(fresh.data.unwrap()["note"], serde_json::json!("fixed"),);
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
            let dt = DatumTime {
                utc: t,
                offset_minutes: 0,
                timezone: "UTC".to_string(),
            };
            let e = Entry::new_legacy(
                "gym",
                Verb::Point,
                Some(serde_json::json!(v)),
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
        // da-ju71.4: dotted paths deep-set, so `tag.priority=1` lands as
        // `tag.priority` *inside* the existing nested `tag` object.
        assert_eq!(ex["tag"]["priority"], serde_json::json!(1));
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
        // da-ivld: id_structure is body-only; `<field>:` prefix is implicit.
        e.id_structure = Some("%state%".to_string());
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
        // da-ivld: id_structure is body-only; `<field>:` prefix is implicit.
        e.id_structure = Some("%kind%".to_string());
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

        crate::commands::undo::undo(undo_args(None, false, "15m"), &db).unwrap();

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
        let err = crate::commands::undo::undo(undo_args(None, false, "0s"), &db).unwrap_err();
        assert!(
            err.to_string().contains("use --force"),
            "error should suggest --force, got: {err}"
        );

        // Row is still tombstoned.
        assert!(db.entries_for_field("mood", None).unwrap().is_empty());

        // With --force, the same call restores it regardless of window.
        crate::commands::undo::undo(undo_args(None, true, "0s"), &db).unwrap();
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

        crate::commands::undo::undo(undo_args(Some(&a_slug), false, "15m"), &db).unwrap();

        // A is back, B is still tombstoned.
        let live: Vec<_> = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].slug, a_id);
        assert!(db
            .get_entry_including_deleted(&b_id)
            .unwrap()
            .deleted_at
            .is_some());
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

        crate::commands::retime::retime(retime_args(Some(&id), "-5m"), &db).unwrap();

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
        crate::commands::retime::retime(args, &db).unwrap();

        let early_after = db.get_entry(&early.slug).unwrap();
        let late_after = db.get_entry(&late.slug).unwrap();
        let other_after = db.get_entry(&other.slug).unwrap();
        assert_eq!(
            early_after.start_time.as_ref().unwrap().utc,
            early_start,
            "pre-cutoff row must not shift"
        );
        assert_eq!(
            late_after.start_time.as_ref().unwrap().utc,
            late_start + chrono::Duration::hours(1),
            "post-cutoff row shifts +1h"
        );
        assert_eq!(
            other_after.start_time.as_ref().unwrap().utc,
            other_start,
            "other field must not shift"
        );
    }

    #[test]
    fn retime_negative_offset() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("focus", 10_000, "d1");
        let id = e.slug.clone();
        let t0 = e.start_time.as_ref().unwrap().utc;
        db.insert_entry(&e).unwrap();

        crate::commands::retime::retime(retime_args(Some(&id), "-30m"), &db).unwrap();

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
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new_legacy(field, Verb::Point, None, None, Some(t), None, device)
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
        crate::commands::undo::undo(undo_args(None, false, "15m"), &db).unwrap();

        // Row is back, identical to the original insertion (including content_hash).
        let restored = db.get_entry(&id).unwrap();
        assert_eq!(restored.slug, id);
        assert_eq!(restored.content_hash, original_hash);
        assert!(restored.deleted_at.is_none());
        assert_eq!(db.count_entries().unwrap(), 1);
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
        log_entry(
            Action::Switch,
            log_args("coffee", vec!["2".to_string()], None),
            &db,
        )
        .unwrap();
        let inserted = db.entries_for_field("coffee", None).unwrap()[0]
            .slug
            .clone();

        crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap();

        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids.len(), 1);
        // Pointer hid resolves to the same entry the get rendered.
        let resolved = datum_core::ids::resolve_quickid_single(&db, &pointer.hids[0]).unwrap();
        assert_eq!(resolved.slug, inserted);
    }

    /// Bare `delete --force` deletes the just-inserted coffee entry without
    /// prompting. (Acceptance #3 — the prompt path is interactive and not
    /// covered here.)
    #[test]
    fn last_docs_bare_delete_force_removes_most_recent_insert() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        log_entry(
            Action::Switch,
            log_args("coffee", vec!["2".to_string()], None),
            &db,
        )
        .unwrap();
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
        crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap();
    }

    /// Bare `get` against a fresh DB (no pointer ever set) errors with a
    /// "no previous documents" message rather than panicking.
    #[test]
    fn last_docs_bare_get_on_fresh_db_errors_clearly() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let err = crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap_err();
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

        crate::commands::tail::tail(head_args(Some("mood"), Some(10)), &db).unwrap();

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
        crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap();
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        let got: std::collections::HashSet<datum_core::ids::Hid> =
            pointer.hids.into_iter().collect();
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
        log_entry(
            Action::Switch,
            log_args("coffee", vec!["2".to_string()], None),
            &db,
        )
        .unwrap();
        let slug = db.entries_for_field("coffee", None).unwrap()[0]
            .slug
            .clone();

        // Mutate via the explicit-id path; the batch should write the pointer.
        update_batch_from_args(update_args(&slug, vec!["3"]), UpdateStrategy::Replace, &db)
            .unwrap();

        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids.len(), 1);
        crate::commands::get::get(GetArgs::fresh_default(), &db).unwrap();
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
        assert_eq!(
            e.dur
                .as_ref()
                .map(datum_core::dur::Dur::to_storage_string)
                .as_deref(),
            Some("PT1H")
        );
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
        assert_eq!(
            e.dur
                .as_ref()
                .map(datum_core::dur::Dur::to_storage_string)
                .as_deref(),
            Some("PT0S")
        );
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
        log_entry(Action::Occur, make_add_args("mood", vec!["value=1"]), &db).unwrap();
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
        let err = log_entry(Action::Add, make_add_args("focus", vec!["80"]), &db).unwrap_err();
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
        let dt = DatumTime {
            utc: t,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
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
        assert_eq!(
            after_update
                .dur
                .as_ref()
                .map(datum_core::dur::Dur::to_storage_string)
                .as_deref(),
            Some("PT30M")
        );
        assert_eq!(after_update.state, Some(serde_json::json!("happy")));
    }

    #[test]
    fn update_negative_dur_rejected() {
        use datum_core::{db::Db, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        let t: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let dt = DatumTime {
            utc: t,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new(
            "focus",
            Verb::Switch,
            Some(serde_json::json!("on")),
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
