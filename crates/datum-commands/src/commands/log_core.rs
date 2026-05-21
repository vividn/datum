//! Write-cluster core: the shared `log_entry` dispatcher plus all of its
//! supporting helpers (argument parsing, reserved-column extraction,
//! temporal-hint expansion, etc.).
//!
//! This module backs the `add`, `occur`, `start`, `end`, `stop`, and
//! `switch` verbs — each verb-named file under `commands/` is a thin
//! wrapper that calls [`log_entry`] with the appropriate `Action`.

use anyhow::Result;
use datum_core::{
    action::Action,
    db::{Db, InsertOutcome},
    entry::{Entry, Verb},
    field_spec::{self, InterpolationContext},
    time::{
        parse_date_arg, parse_duration, parse_time_arg, resolve_timezone, DatumTime, TimeBuilder,
    },
};
use serde_json::Value as JsonValue;

use crate::{
    args::{LogArgs, LogArgsExt},
    config::device_id,
    render::{
        show_create_with_context, show_delete, show_exists, show_rename, show_update_with_context,
        RenderContext, Surface,
    },
};

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
        let registry = crate::cmd::load_registry_opt(db);
        let output = args.output()?;
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
        let registry = crate::cmd::load_registry_opt(db);
        let ctx = RenderContext::for_entry(db, &entry, Surface::Action, None)?;
        let output = args.output()?;
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
    if crate::args::push_to_multi_collector(entry.clone(), args.output()?) {
        return Ok(());
    }

    let strategy = args.conflict_strategy();
    let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
    let registry = crate::cmd::load_registry_opt(db);
    let output = args.output()?;
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
pub(crate) fn parse_value_token(s: &str) -> JsonValue {
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
pub(crate) fn split_numeric_unit(s: &str) -> Option<(&str, &str)> {
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

pub(crate) fn build_time_with_precision(
    args: &LogArgs,
) -> Result<(DatumTime, datum_core::time::Uncertainty)> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
