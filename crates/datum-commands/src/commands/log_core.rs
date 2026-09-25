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
    dur::DurSpan,
    entry::{Entry, Verb},
    field_spec::{self, InterpolationContext},
    time::{
        parse_date, parse_duration, parse_time, resolve_timezone, DatumTime, TimeBuilder, TimeCtx,
    },
};
use datum_parse::input::TimeArgs;
use serde_json::Value as JsonValue;

use crate::{
    args::{LogArgs, LogArgsExt},
    config::device_id,
    render::{
        show_create_with_context, show_delete, show_exists, show_islast, show_rename,
        show_update_with_context, RenderContext, Surface,
    },
};

// ─── Logging ─────────────────────────────────────────────────────────────────

pub(crate) fn log_entry(action: Action, mut args: LogArgs, db: &Db) -> Result<()> {
    // da-vo0y (review): `-I`/`--string-state` is Switch-only — on other
    // verbs it would silently suppress the injected `dur=` slot and change
    // duration parsing. And on switch it conflicts with the other
    // remainder-routing flags: the joined positionals would land in that
    // bucket instead of state, leaving a surprise `state=true`.
    if args.string_state {
        if !matches!(action, Action::Switch) {
            anyhow::bail!("`-I`/`--string-state` is only valid with `switch`");
        }
        if args.remainder.is_some() || args.string_remainder || args.comment_remainder {
            anyhow::bail!(
                "`-I`/`--string-state` conflicts with `-R`/`-S`/`-C` — the \
                 joined positionals already target the state"
            );
        }
    }

    if args.if_changed && !matches!(action, Action::Switch | Action::Start | Action::Stop) {
        anyhow::bail!("`--if-changed` is only valid with `switch`, `start` and `end`");
    }

    // da-py5w: `note` is the one verb whose bare words are prose, not a
    // state or a typo — it behaves as if `-C` were given, so
    // `note coffee too strong today` ≡ `note coffee -C too strong today`.
    // `-R <bucket>` names where the remainder goes, so an explicit one
    // still wins (and keeps its own array shape).
    if matches!(action, Action::Note) && args.remainder.is_none() {
        args.comment_remainder = true;
    }

    // da-ooe2: restore v2's flexiblePositional duration model. The verb-
    // specific trailing-duration peelers are gone; `prepare_log_args`
    // declares an optional `dur=` slot last for the duration-bearing verbs
    // and the normal slot machinery in `handle_data_args` fills it from the
    // trailing positional (or an explicit `dur=<v>` anywhere in argv). The
    // switch `,`-chain peel (da-a2o8) is preserved inside the helper.
    prepare_log_args(action, &mut args)?;

    // da-mjwq: on the verbs that can carry a duration, `start_time=` and
    // `end_time=` are not data — they place the block. Lifted here, before
    // the data parser's `*time` suffix rule would coerce them to clock
    // strings, and turned into the `-t` the same line would otherwise
    // carry: `end_time=` when given (a block's occur_time is its END,
    // da-zwct), else `start_time=`. The pair and the start+duration forms
    // move that anchor below, once the `dur=` slot is filled. One time
    // parser: whatever `-t` accepts (`-20`, `4pm`, `16:15`, an ISO
    // datetime, `yesterday`, a `±5m` suffix), the keys accept.
    let lifted = if matches!(
        action,
        Action::Add | Action::Note | Action::Start | Action::Switch | Action::Occur
    ) {
        lift_time_keys(&mut args.rest)
    } else {
        LiftedTimes::default()
    };
    if let Some((key, raw)) = lifted.anchor() {
        if args.time.time.is_some() {
            anyhow::bail!("-t and {key}= both set the time; use one");
        }
        if args.time.omit_timestamp {
            anyhow::bail!(
                "-T/--no-timestamp cannot be combined with {key}=: a timeless entry has no instant to place"
            );
        }
        args.time.time = Some(raw.to_string());
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
    // `build_time_with_uncertainty` call below (line ~410) with its full
    // error message intact; here we only want a clean base when the
    // flags parse, so `-T` and the undo early-return path keep their
    // current "no timestamp" semantics.
    let base_local = if args.time.omit_timestamp {
        None
    } else {
        build_time_with_uncertainty(&args)
            .ok()
            .map(|dt| dt.local_time().naive_local())
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

    // da-mjwq: `end_time=` alone on `start` closes the field's open block
    // at E — the action becomes `end` (≡ `end field -t E`), so one alias
    // `posture → start posture` serves both directions. On `switch` the
    // state it names is the block being closed: ≡ `end field -t E -L=STATE`,
    // and `datum check` reports a `last_state_mismatch` when history
    // disagrees. A duration alongside keeps the verb: that is `-t E <dur>`.
    let has_dur = reserved.dur.is_some() || args.duration.duration.is_some();
    let action = if lifted.end.is_some()
        && lifted.start.is_none()
        && !has_dur
        && matches!(action, Action::Start | Action::Switch)
    {
        if matches!(action, Action::Switch) {
            if args.last_state.is_some() {
                anyhow::bail!("end_time= on switch already asserts the prior state; drop -L");
            }
            // The slot's default `true` names nothing; a stated state is
            // the assertion. Bare `-L` still records the prior state.
            args.last_state = Some(match reserved.state.take() {
                Some(s) if s != JsonValue::Bool(true) => switch_state_raw(&s),
                _ => String::new(),
            });
        }
        Action::Stop
    } else {
        action
    };

    // -T/--no-timestamp: skip time-flag computation but keep the action's
    // own state-canonicalization rules (so `switch ... -T` still preserves
    // its positional state, `start -T` still records state=true, etc.).
    // Time-specifying flags and a duration are rejected so silent discard
    // can't mask a user misconception. A timeless entry stores
    // `occur_time = NULL` (the durable timeless marker) and slugs on
    // `create_time` → `field:c<create_time>` (da-r17h), so `occur f -T`
    // becomes note-data: the data lands, no occurrence is logged, and
    // many-per-field works. Checked here, once the `dur=` slot is filled.
    if args.time.omit_timestamp {
        check_no_time_flags(
            &args,
            reserved.dur.is_some() || args.duration.duration.is_some(),
        )?;
    }

    // da-5t94: the resolved `field` now flows through the shared peeler.
    // For flex-field verbs the injected `field=` slot lands in
    // `reserved.field`; `block` keeps `field_or_verb`. `id` is untouched
    // here — it stays on the insert-only slug-promotion path in
    // `parse_log_args` so it can defer to an explicit `-K` slug slot.
    let field = reserved.field.take().unwrap_or(field);

    // da-5ns.3 strict positional gate (the timeless Add/Note family): the
    // v=20 model has no implicit `value` column, so a bare unkeyed positional
    // with no `-K` slot binding has nowhere to land. Reject with a generic
    // extra-args error naming the offending tokens — `state=<v>` is just
    // one of several ways to attach data and should not be presented as a
    // hard requirement (da-obmm).
    if matches!(action, Action::Add | Action::Note) && value.is_some() && args.slug_keys.is_empty()
    {
        let extras: Vec<&str> = args
            .rest
            .iter()
            .filter(|t| !t.contains('='))
            .map(String::as_str)
            .collect();
        let joined = extras.join(" ");
        anyhow::bail!(
            "unexpected positional argument(s) for `dm {action}`: {joined}. \
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

    // da-state-back: when the user writes `_` / `__` / `_n` as a switch
    // state, resolve it to a previous DISTINCT state of the field —
    // mirroring the quick-id ordinal pattern (`_` = 1 back, `__` = 2 back).
    // Only for `Action::Switch`; other verbs treat the same literals as
    // plain strings. Resolution skips the field's current state and counts
    // only distinct historical alternatives, so `switch field _` toggles
    // A↔B and `__` only resolves when a genuine third state exists.
    let value = if matches!(action, Action::Switch) {
        resolve_underscore_state(value, db, &field)?
    } else {
        value
    };
    if matches!(action, Action::Switch) {
        reserved.state = resolve_underscore_state(reserved.state, db, &field)?;
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
        let registry = crate::commands::helpers::load_registry_opt(db);
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

    // `start foo -m` / `switch foo -m` is a momentary dot, not an open block:
    // stamp dur=MOMENT via the occur model — `(time_start, None)` + `Dur::moment()`
    // (NOT a `(base, base)` zero-span, which would derive `Dur::of(PT0S)` and
    // preempt the Moment, since a moment has no end). `derive_verb` then locks it
    // to `Verb::Point`. An explicit duration overrides it — `dur=` / trailing
    // positional (reserved.dur), or the switch-chain's peeled dur
    // (args.duration.duration) — per "set
    // dur to MOMENT unless explicitly set otherwise (dur=3)". (Occur is a point
    // by default.)
    // A switch `,`-chain (`switch a -m , b`) carries a `following_state` and
    // must stay a zero-span `Verb::PointSwitch` — `derive_verb` short-circuits
    // `Dur::moment()` to `Verb::Point` and drops the following_state, so the
    // Moment path is only for the non-chain switch (and start). That chain keeps
    // the `(base, base)` branch below.
    let moment_point = args.duration.moment
        && matches!(action, Action::Start | Action::Switch)
        && reserved.dur.is_none()
        && args.duration.duration.is_none()
        && reserved.following_state.is_none();

    // Compute time_start, time_end, and (for Action::Add) a shape override.
    // Skipped entirely for -T — a timeless entry carries no times, and so no
    // uncertainty either. The time's uncertainty rides on `base_time` and
    // flows to `entry.occur_time` with it (da-b8rs); every derived time
    // below is built with `..base_time`, so it carries too.
    let (time_start, time_end) = if args.time.omit_timestamp {
        (None, None)
    } else {
        let now = DatumTime::now().utc;
        let base_time = build_time_from_flags(&args.time, now)?;

        // da-mjwq: the two lifted forms that are not a plain `-t` twin.
        // `base_time` is the lifted anchor (`end_time=`, else `start_time=`):
        //   start_time=S end_time=E   → the block S..E: occur_time = E, dur = E − S
        //   start_time=S + a duration → the block S..S+dur: occur_time = S + dur
        // `end_time=E` + a duration is `-t E <dur>`, and either key alone
        // is `-t <key>` — both already what `base_time` says. Under the
        // reference semantics (da-zwct) the duration PRECEDES occur_time,
        // so `start posture start_time=4pm 15` is the block 16:00–16:15,
        // the row `start posture -t 16:15 15` writes.
        let lifted_span = match (&lifted.start, &lifted.end) {
            (Some(s), Some(e)) => {
                if has_dur {
                    anyhow::bail!(
                        "start_time= and end_time= already fix the duration; drop the dur"
                    );
                }
                let start = build_time_from_flags(
                    &TimeArgs {
                        time: Some(s.clone()),
                        ..args.time.clone()
                    },
                    now,
                )?;
                if base_time.utc < start.utc {
                    anyhow::bail!("end_time={e} is before start_time={s}");
                }
                Some((Some(start), Some(base_time.clone())))
            }
            (Some(_), None) if has_dur => {
                let dur = match reserved.dur.as_ref().map(|d| &d.span) {
                    Some(DurSpan::Of(d)) => *d,
                    Some(_) => anyhow::bail!(
                        "start_time= needs a duration with a length to place the block's end"
                    ),
                    None => parse_duration(args.duration.duration.as_deref().unwrap_or(""))?,
                };
                let end = DatumTime {
                    utc: base_time.utc + dur,
                    ..base_time.clone()
                };
                Some((Some(base_time.clone()), Some(end)))
            }
            _ => None,
        };

        // da-lxyw: `add`/`note` are the timeless verbs. With no clock- or
        // date-modifying flag (`-t`/`-d`/`-y*`/`-q*`/`-D` — a lifted key
        // became a `-t` above) the entry carries no `occur_time`; that
        // absence is what tells `add` from `occur`. `-z` alone only
        // re-zones, and `-T` is the branch above. (da-r17h: `note` is `add`
        // with an auto id, so it shares the rule.)
        let add_timeless = matches!(action, Action::Add | Action::Note)
            && args.time.time.is_none()
            && args.time.date.is_none()
            && args.time.yesterday == 0
            && args.time.quick == 0
            && !args.time.full_day;

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
                    let last_state = block.state.clone().unwrap_or(JsonValue::Bool(true));
                    let mut obj = match extra_data.take() {
                        Some(JsonValue::Object(m)) => m,
                        _ => serde_json::Map::new(),
                    };
                    obj.insert("last_state".to_string(), last_state);
                    extra_data = Some(JsonValue::Object(obj));
                }
            }
        }

        let (time_start, time_end) = if let Some(span) = lifted_span {
            span
        } else if add_timeless {
            (None, None)
        } else if args.duration.moment
            && matches!(action, Action::Switch)
            && reserved.following_state.is_some()
        {
            // `switch a -m , b`: a zero-span point-in-time switch with a
            // following_state — `derive_verb` reads (state, dur=PT0S,
            // following_state) as `Verb::PointSwitch`. The bare `switch foo -m`
            // (no chain) takes the dur=MOMENT path instead (see `moment_point`).
            (Some(base_time.clone()), Some(base_time))
        } else if let Some(dur_str) = &args.duration.duration {
            // The only remaining producer of `args.duration.duration` is the
            // switch `,`-chain peel in `extract_switch_then_chain` (da-azqq
            // retired `--dur`, the flag that used to fill it from argv). Like
            // every other spelling of a duration, the block ENDS at the
            // entry's time and the span it lasted precedes it — da-zwct,
            // and the same "dur only" row of [`AddTemporalHints::resolve`]'s
            // table. `switch activity end 14m , researching` says the
            // fourteen minutes just gone were `end`, and `researching`
            // starts now.
            let dur = parse_duration(dur_str)?;
            let time_start = DatumTime {
                utc: base_time.utc - dur,
                ..base_time.clone()
            };
            (Some(time_start), Some(base_time))
        } else {
            (Some(base_time), None)
        };

        (time_start, time_end)
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

    // da-5ns.3: stamp the reserved columns extracted upstream onto the
    // entry. `state=` overrides the value-derived state assignment from
    // `Entry::new_legacy`; `dur=` and `following_state=` overwrite the
    // legacy time-shape derivation. The storage layer's `derive_verb`
    // call then classifies the resulting (state, dur, fs) triple.
    if let Some(s) = reserved.state {
        entry.state = Some(s);
    }
    if let Some(d) = reserved.dur {
        // da-ooe2: an explicit duration (trailing positional or `dur=<v>`,
        // both resolved through the injected `dur=` slot) plus the
        // switch-chain's peeled duration is a double-specification — fail
        // loudly rather than silently letting one win. The `,`-chain peels
        // its dur out of `args.rest` and leaves the slot unfilled, so this
        // never trips on a well-formed chain.
        if args.duration.duration.is_some() {
            anyhow::bail!("duration specified twice: trailing positional / `dur=` conflicts with the `,`-chain duration");
        }
        entry.dur = Some(d);
    }
    if let Some(fs) = reserved.following_state {
        entry.following_state = Some(fs);
    }
    // da-r17h (task 8): an explicit `occur_time=` key always overwrites the
    // column — like `dur=` above — with type-aware parsing (RFC3339, clock
    // times, `now`, or signed offsets). This makes the time-anchored columns
    // uniformly settable across every verb: `add field occur_time=+5` shifts
    // it +5m and re-anchors what was a timeless entry. The relative-offset
    // base is the entry's current occur_time when present, else wall-clock
    // now (mirrors the `dm update` path in update_core::apply_mutation).
    if let Some(t) = reserved.occur_time {
        let base = entry.occur_time.clone().unwrap_or_else(DatumTime::now);
        entry.occur_time = Some(parse_reserved_occur_time(&t, &base)?);
    }

    // da-ju71.2: stamp `Dur::moment()` ("MOMENT") to lock the entry to
    // Verb::Point in `derive_verb` regardless of state shape. Three cases:
    // Action::Occur is a point by default (e.g. `occur drive .id=car
    // .role=driver` stays a point, not a switch); `-m`/`--moment` on
    // switch/start does the same (`moment_point`), so `start foo -m` is a
    // dot rather than an open block; and `-T` on a verb with a state is a
    // note about that state, not a transition (`timeless_state`, da-w149).
    // An explicit dur was honored above (and `-T` refused one).
    // A `,`-chain names a transition, which a note cannot carry: leaving it
    // unstamped keeps `derive_verb`'s refusal of a duration-less chain.
    let timeless_state = args.time.omit_timestamp
        && entry.following_state.is_none()
        && (entry.state.is_some()
            || matches!(action, Action::Start | Action::Switch | Action::Stop));
    if entry.dur.is_none() && (matches!(action, Action::Occur) || moment_point || timeless_state) {
        entry.dur = Some(datum_core::dur::Dur::moment());
    }

    // da-b8rs: an uncertainty qualifies the thing it is about, so it is
    // stamped after every path that can set `dur` — including the MOMENT
    // default just above. With no duration there is nothing to qualify;
    // say so rather than storing an orphan value nothing reads.
    if let Some(u) = reserved.dur_uncertainty {
        if entry.dur.is_none() {
            anyhow::bail!(
                "dur_uncertainty= needs a duration: pass dur=<D> (or a trailing duration) alongside it"
            );
        }
        entry.set_dur_uncertainty(Some(u));
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
                entry.state = Some(JsonValue::Bool(false));
            }
            Action::Start | Action::Switch => {
                entry.state = Some(JsonValue::Bool(true));
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

    // An explicit `occur_time_uncertainty=` key overrides whatever the -q
    // stack or a `±N` suffix put on the time. Same rule as `dur_uncertainty=`
    // above: a timeless entry has no time to qualify.
    if let Some(u) = reserved.occur_time_uncertainty {
        if entry.occur_time.is_none() {
            anyhow::bail!(
                "occur_time_uncertainty= needs a time: a timeless entry has no instant to qualify"
            );
        }
        entry.set_occur_uncertainty(Some(u));
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
        // §5 #11: empty raw_input normalises to None on the way in too;
        // it doesn't pretend the user passed an empty argv.
        entry.raw_input = if ctx.raw_input.is_empty() {
            None
        } else {
            Some(ctx.raw_input.clone())
        };
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
        let ctx = InterpolationContext::from_entry(&entry);
        entry.field = field_spec::interpolate_field(&template, &ctx, &delim)?;
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
    //      (timeless) `%create_time%` → `field:c<create_time>`. da-r17h:
    //      this replaced the old `:unset` sentinel, which made the slug
    //      UNIQUE-collide so only one timeless entry per field could exist.
    //      Distinct create_times keep many-per-field timeless notes apart.
    //      A bare `add` is the exception: it demands an explicit id (it is
    //      identified by *what*, not *when*), so it errors here instead.
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
    } else if matches!(action, Action::Add) && !args.time.omit_timestamp {
        // da-r17h: a bare timeless `add` with no id has no identity to slug
        // on. `note` (auto create_time id) or an explicit id is the way in.
        // `add … -T` is exempt: `-T` explicitly converts to note-data, so it
        // takes the create_time-id branch below like any other `-T` insert.
        anyhow::bail!(
            "add needs an id. Derive one from a key (-K name) or set it \
             directly (--id <value>). See --help for --id %templates% and id=."
        );
    } else {
        // Timeless `note` / `-T` / `--no-timestamp`: identify by create_time.
        // `%create_time%` auto-prefixes `c` → `field:c<create_time>`.
        "%create_time%".to_string()
    };
    {
        let ctx = InterpolationContext::from_entry(&entry);
        let body = field_spec::interpolate(&id_structure, &ctx, &delim);
        entry.slug = datum_core::ids::Slug(format!("{}:{}", entry.field, body));
        entry.id_structure = Some(id_structure);
    }

    // da-hxmtg: `--if-changed` writes nothing when the field already holds
    // this state at the entry's time — the same governing-state walk `now`
    // reads — and names the entry that set it as ISLAST. A block (a `dur`)
    // is a span, not a transition, so it writes as it always did. A dry run
    // still previews ISLAST but leaves `_` where it was.
    if args.if_changed && entry.occur_time.is_none() {
        anyhow::bail!("`--if-changed` needs a time: a timeless entry has no state to compare with");
    }
    if let (true, Some(t), None) = (args.if_changed, &entry.occur_time, &entry.dur) {
        if let Some((state, Some(hid))) = db
            .batch_governing_at(&[&entry.field], t.utc)?
            .remove(&entry.field)
        {
            if Some(&state) == entry.state.as_ref() {
                let last = db.get_entry(&hid)?;
                let registry = crate::commands::helpers::load_registry_opt(db);
                show_islast(&last, registry.as_ref(), &args.output()?);
                if !args.dry_run {
                    let _ = datum_core::last_docs::set_last_docs(db, &[last.hid]);
                }
                return Ok(());
            }
        }
    }

    // da-gvcen: the first log of a field with no spec brings the field
    // into being, and the ENTRY says so — the declaration rides in its
    // own trailing edge as a `following_state`. vividn, 2026-09-21, on the
    // second row da-c8mh wrote a millisecond earlier for this: "a
    // separate entry gets logged at a separate time point (slightly
    // earlier iirc). I don't like this. It's very clunky. […] we should
    // use following_state, so a POINT_SWITCH."
    //
    // The state it declares is the one the TRACK row carried —
    // `declared_state`, da-5ifp — so the declaration changed which column
    // it lives in and not what it says: TRACKED for a field with no off
    // state to sit in, `false` for one that has.
    //
    // That trailing switch is not decoration: it is what un-ignores a
    // field that was IGNORED before, which is the one job the old TRACK
    // row's state did (`batch_state_at` reads a compound verb's
    // `following_state` past its trailing edge, da-2i8g). What records
    // that the field EXISTS is the spec, written below.
    //
    // An entry with no trailing instant — an open `start`, a `switch`, a
    // timeless `note` — cannot carry one, and none is invented: the truth
    // table is asked, the row is asked for an instant, and where either
    // refuses, the entry's own state is the switch. A caller that spelled
    // `following_state=` itself keeps it.
    // A field is declared ONCE. The spec used to say so the moment it was
    // written; now that it is written only after the entry lands, an
    // earlier side of the same multi window is the other thing that can
    // have declared this field already.
    let declares_field = db.load_spec(&entry.field)?.is_none()
        && !crate::args::field_declared_in_window(&entry.field);
    if declares_field && entry.following_state.is_none() && entry.occur_time.is_some() {
        let declared =
            datum_core::track::declared_state(datum_core::track::verb_for_action(action));
        if datum_core::derive_verb(
            entry.state.clone(),
            entry.dur.as_ref(),
            Some(declared.clone()),
        )
        .is_ok()
        {
            entry.following_state = Some(declared);
        }
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

    // da-rcoi: when the field's spec is strict, reject undeclared
    // extra_data keys before the write lands. Runs ahead of the dry-run
    // and multi-collector branches so the failure surfaces uniformly —
    // strict-mode protection shouldn't depend on which insert path the
    // CLI chose.
    crate::commands::helpers::enforce_strict_extra_data(&entry, db)?;

    // --dry-run: preview the would-be entry — including slug derivation,
    // reserved-column routing, and state canonicalization — without
    // touching the DB or the multi-collector. Skips `_LAST` pointer
    // updates so a dry-run never moves the last-accessed cursor. Exits 0
    // so `dm <verb> --dry-run` is safe to chain.
    if args.dry_run {
        let registry = crate::commands::helpers::load_registry_opt(db);
        let ctx = RenderContext::for_entry(db, &entry, Surface::Action, None)?;
        let output = args.output()?;
        crate::render::print_entry(&entry, Some(&ctx), None, registry.as_ref());
        if !output.silent() {
            eprintln!("[dry-run] would {action} {}", entry.slug);
        }
        return Ok(());
    }

    // da-c8mh, da-gvcen: the spec is the field's record of existence, and
    // it is written ONLY ONCE THE ENTRY HAS LANDED — below on the single
    // path, and by the multi driver after its atomic insert. da-c8mh
    // bought both-or-neither with an atomic pair of rows; the pair is
    // gone, but the property is not optional. A spec left behind by a
    // failed insert makes every later log of that field see a spec, so
    // `declares_field` is false and the declaration is never written
    // again — an IGNORED field would stay ignored until someone thought
    // to run `datum track`. "The declaration precedes the thing declared"
    // is already true in the row's own columns and needs no help from
    // write order.

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
        // The window owns the spec write, once its atomic insert lands.
        if declares_field {
            crate::args::push_pending_declaration(&entry.field, action);
        }
        return Ok(());
    }

    let strategy = args.conflict_strategy();
    let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
    // The entry landed, so the field exists: now the spec may be written.
    if declares_field {
        crate::commands::helpers::save_track_spec(db, &entry.field, action)?;
    }
    let registry = crate::commands::helpers::load_registry_opt(db);
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
            // Signal the benign idempotent re-log so the binary exits 11
            // (datum v2 parity, da-9sfm). Not an error — the EXISTS render
            // above is the user-facing result.
            crate::args::mark_duplicate_noop();
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
pub(crate) const PRIMARY_VALUE_KEY: &str = "__datum_primary_value";

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
        Action::Add | Action::Note | Action::Occur | Action::Switch | Action::Start | Action::Stop
    );
    let field = if flex_field {
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
            && !args.string_state
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
    // `.` is the use-default-or-skip sentinel (da-1n61): da-ooe2 injects it
    // as the slot VALUE so `handle_data_args`' dot-default handling resolves
    // it to the slot's `true` default. Crucially this consumes the
    // `state=true` slot rather than leaving it hungry — otherwise the
    // trailing `dur=` positional would be swallowed as state. `\.` escapes
    // back to the literal `"."` string and is injected normally. With no
    // positional at all, insert just the slot so the unfilled-keys phase
    // applies the default.
    // `-I`/`--string-state` (da-vo0y): join ALL positionals into state rather
    // than peeling a single one, so skip the single-positional state slot.
    // With `data_args.remainder == None` for switch, `string_state` routes
    // the joined remainder to `state.id` in `handle_data_args`. The no-
    // positional case still defaults to `state=true` via the action
    // canonicalization downstream.
    if matches!(action, Action::Switch) && !args.string_state {
        if switch_state_positional.is_some() {
            flexible_positional(
                &mut data_args,
                switch_state_positional.take(),
                "state=true",
                false,
                false,
            );
        } else {
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
    //   * Action::Occur | Start | Stop | Switch → no remainder. State is
    //     hard-set (start/stop) or flows through the `state=true` slot
    //     (switch), and the trailing `dur=` slot absorbs the duration, so
    //     a leftover positional has nowhere meaningful to land. Erroring
    //     out (da-r6ds / da-ooe2) prevents silent coercion — e.g. a second
    //     trailing positional on switch (`switch f ai 3 5m`) is rejected
    //     rather than dropped into an unused primary value.
    //   * Otherwise (Add) → PRIMARY_VALUE_KEY sentinel.
    let user_remainder = args.remainder.is_some() || args.comment_remainder;
    data_args.remainder = if let Some(r) = &args.remainder {
        Some(r.clone())
    } else if args.comment_remainder {
        Some("comment".to_string())
    } else if matches!(
        action,
        Action::Occur | Action::Start | Action::Stop | Action::Switch
    ) {
        None
    } else {
        Some(PRIMARY_VALUE_KEY.to_string())
    };
    data_args.string_remainder = args.string_remainder;
    data_args.comment_remainder = args.comment_remainder;
    data_args.string_state = args.string_state;
    // da-vo0y F2: trailing-dur peek — only when a string-join remainder is
    // active on a duration-bearing verb (matches the dur=-slot suppression
    // in `prepare_log_args`; the peek replaces the slot there).
    data_args.peek_trailing_dur =
        (args.string_state || args.string_remainder || args.comment_remainder)
            && matches!(
                action,
                Action::Switch | Action::Start | Action::Stop | Action::Block
            );

    let mut datum_data = handle_data_args(&data_args).map_err(anyhow::Error::from)?;

    // da-5t94: `field` is peeled by the shared `extract_reserved_columns`
    // boundary (called after `parse_log_args` returns), so it lands on
    // `entry.field` for BOTH insert and update. The flex-field slot
    // injected above (or an explicit `field=<x>`) leaves the resolved
    // value at the "field" path in `datum_data`; it rides out in
    // `extra_data` and the shared peeler routes it onto the column. For
    // non-flex (`block`), `field` is seeded from `field_or_verb`.

    // da-ju71.4 / da-rori: normalize state *here* (before slug derivation
    // below reads `state.id`) via the shared `normalize_state`. The same
    // step runs again at the `extract_reserved_columns` boundary that both
    // insert and update share — it is idempotent, so the early collapse
    // for slug timing and the boundary collapse agree.
    if let Some(slot) = datum_data.get_mut("state") {
        *slot = normalize_state(std::mem::replace(slot, JsonValue::Null));
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

/// da-a2o8: peel the BlockSwitch / PointSwitch "then" chain out of a
/// `switch`'s positionals.
///
/// A standalone bare-comma token (`","` — produced when the user types
/// `switch body vertical 5 , sit` and the shell tokenises around
/// whitespace) marks the boundary between the leading
/// state-and-duration positionals and the trailing `following_state`
/// value. We rewrite the tail as `following_state=<value>` so the
/// reserved-column extractor downstream populates the column.
///
/// Concurrent-state form (`a,b` with no whitespace around the comma) is
/// a single positional containing a comma — never a standalone `","`
/// token — so this preprocessor leaves it untouched. The alias layer's
/// `maybe_collapse_switch_chain` is what decides whether a standalone
/// comma in this position should even reach `args.rest`: it bypasses
/// the multi-entry split (and swaps left/right when needed) only when
/// at least one side carries a dur signal. Otherwise the comma goes
/// through `split_argv_at_commas` and `args.rest` never sees a `,`.
///
/// Constraints:
/// * Exactly one bare-comma token is supported. Multi-step chains
///   (`a 5 , b 30 , c`) are out of scope for the MVP.
/// * Post-comma must be exactly one bare positional. KEY=VALUE pairs
///   after the comma are rejected since they don't belong to either
///   side of the chain.
/// * The pre-comma side keeps its existing semantics — a bare
///   positional is the state, a trailing bare positional is the
///   duration. That work is left to the slot machinery in
///   [`parse_log_args`] (the `state=true` slot and the injected `dur=`
///   slot from [`prepare_log_args`]).
pub(crate) fn extract_switch_then_chain(args: &mut LogArgs) -> Result<()> {
    let comma_idx = match args.rest.iter().position(|t| t == ",") {
        Some(i) => i,
        None => return Ok(()),
    };
    // Reject a second standalone comma — multi-step chains are a
    // follow-up. Without this guard the second comma would silently
    // get folded into following_state and look like an alias for `a,b`
    // concurrent state — wrong.
    if args.rest.iter().skip(comma_idx + 1).any(|t| t == ",") {
        anyhow::bail!("switch: multi-step `,` chains aren't supported yet — use one bare comma");
    }
    let post: Vec<String> = args.rest.split_off(comma_idx + 1);
    // Drop the comma token itself.
    args.rest.pop();
    let mut post_bare: Vec<String> = Vec::with_capacity(post.len());
    for token in post {
        if token.contains('=') {
            anyhow::bail!(
                "switch: only a bare state value may follow the `,` chain marker (got `{token}`)"
            );
        }
        post_bare.push(token);
    }
    if post_bare.is_empty() {
        anyhow::bail!(
            "switch: `,` chain marker requires a trailing state (e.g. `switch body vertical 5 , sit`)"
        );
    }
    if post_bare.len() > 1 {
        anyhow::bail!(
            "switch: `,` chain marker accepts exactly one trailing state (got {n})",
            n = post_bare.len()
        );
    }
    let following_state = post_bare.into_iter().next().unwrap();
    args.rest.push(format!("following_state={following_state}"));
    // da-b4k5: in chain context the `end`/`stop`/`start` keywords mean
    // the same thing they do as a standalone verb — `false`/`false`/
    // `true`. The non-chain path achieves this by rewriting `switch f
    // end` to `end f` so `Action::Stop` runs (which then sets
    // `state=false`). The chain path can't go through that rewrite
    // without losing the chain, so translate the keyword to the
    // reserved `state=` column AND eagerly peel the duration positional
    // into `args.duration` — the `state=true` slot would otherwise
    // reassign the next bare positional as state (it doesn't know `state=…` is
    // already in flight).
    // da-vh30: under `-I`/`--string-state` the bare positionals ARE the
    // literal string state — `switch song -I end of the road 5m , next`
    // must keep "end" as the first word of the state, not translate it
    // to `state=false` (and then mis-peel "of" as the duration). So the
    // keyword translation is suppressed in string-state mode; a user who
    // wants the keyword semantics simply omits `-I`. (Post-comma keywords
    // are literal in BOTH modes — there is no tail translation.)
    let leading_keyword_idx = args
        .rest
        .iter()
        .position(|t| !t.contains('='))
        .filter(|_| !args.string_state)
        .filter(|&i| matches!(args.rest[i].as_str(), "end" | "stop" | "start"));
    if let Some(idx) = leading_keyword_idx {
        let keyword = args.rest.remove(idx);
        let state_kv = match keyword.as_str() {
            "start" => "state=true",
            _ => "state=false",
        };
        args.rest.push(state_kv.to_string());
        // Peel the next bare positional as the duration. It's not
        // required (the bead's repro always supplies one, but a chain
        // with no dur is still a legal shape — derive_verb rejects
        // `(state=set, fs=set, dur=None)` further downstream with a
        // clear error rather than silently producing the wrong row).
        if let Some(dur_idx) = args.rest.iter().position(|t| !t.contains('=')) {
            let dur_token = args.rest.remove(dur_idx);
            if args.duration.duration.is_some() {
                anyhow::bail!(
                    "switch: `{keyword}`-chain duration positional `{dur_token}` conflicts with an already-peeled duration"
                );
            }
            args.duration.duration = Some(dur_token);
        }
    }
    Ok(())
}

/// da-ooe2: shared verb pre-pass run before `parse_log_args` on both the
/// insert (`log_entry`) and reparse paths.
///
/// Restores v2 datum's flexiblePositional duration model: rather than
/// peeling a trailing positional with verb-specific rules, declare an
/// optional `dur=` slot LAST for the duration-bearing verbs. The trailing
/// positional (or an explicit `dur=<v>` anywhere in argv) then fills it via
/// the normal slot machinery in `handle_data_args`, and
/// `extract_reserved_columns` routes the resolved value onto `entry.dur`.
///
/// Occur/Add are excluded: occur is Moment-locked (da-ju71.2) and add
/// infers its shape from temporal hints. Both still accept an explicit
/// `dur=<v>` through the reserved-column path.
///
/// For start/stop the leading `.` / `\.` skip sentinel has no state slot to
/// land on (state is hard-set by action canonicalization), so it is
/// stripped here; switch absorbs `.` into its `state=true` slot inside
/// `parse_log_args`.
///
/// String-join mode (da-vo0y F2): with `-I`/`-S`/`-C` active the `dur=`
/// slot is suppressed — it would front-eat the first joined token — and
/// the trailing-dur PEEK in `handle_data_args` claims the duration from
/// the join window's tail instead. The start/stop `.`-strip is likewise
/// bypassed there: the `.` is the peek's explicit skip-dur sentinel.
pub(crate) fn prepare_log_args(action: Action, args: &mut LogArgs) -> Result<()> {
    // da-a2o8: peel a standalone-comma "then" chain (`switch f vertical 5 ,
    // sit`) into `following_state=…` before the dur slot is declared. The
    // alias layer's `maybe_collapse_switch_chain` already ensured the
    // dur-bearing side is on the left of the comma.
    if matches!(action, Action::Switch) {
        extract_switch_then_chain(args)?;
    }

    if matches!(
        action,
        Action::Switch | Action::Start | Action::Stop | Action::Block
    ) {
        // da-vo0y F2: with a string-join remainder active (`-I`/`-S`/`-C`),
        // the front-eating `dur=` slot would swallow the first joined token
        // as a duration (`start movie -SR title terminator 90` must not try
        // `terminator` as a dur). Suppress the slot and let the trailing-dur
        // PEEK in `handle_data_args` claim the duration from the join
        // window's tail instead (set via `peek_trailing_dur` below). The
        // start/stop `.`-strip is likewise skipped in peek mode — the `.`
        // is the peek's explicit skip-dur sentinel there.
        let string_join = args.string_state || args.string_remainder || args.comment_remainder;
        if matches!(action, Action::Start | Action::Stop) && !string_join {
            args.rest.retain(|t| t != "." && t != r"\.");
        }
        if !string_join {
            args.keys.push("dur=".to_string());
        }
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

/// Resolve `_` / `__` / `_n` state tokens against the field's history.
///
/// Mirrors the entry-id quick-id ordinal pattern (`_` = 1 back, `__` = 2,
/// `_n` = n) but resolves against the field's distinct state history via
/// [`Db::resolve_state_back`]. Only the scalar-string case is rewritten;
/// arrays (multi-state) and structured states pass through untouched —
/// each element of a multi-state array could itself be `_`, but that's a
/// follow-up.
///
/// Errors when the user explicitly asked for a back-state but the field's
/// history doesn't run that deep — silently storing the literal string
/// `"_"` would be confusing.
fn resolve_underscore_state(
    value: Option<JsonValue>,
    db: &Db,
    field: &str,
) -> Result<Option<JsonValue>> {
    let Some(JsonValue::String(s)) = &value else {
        return Ok(value);
    };
    let Some(n) = datum_core::ids::parse_underscore_ordinal(s) else {
        return Ok(value);
    };
    // Anchored at wall-clock now: `_` means "the previous distinct state
    // as the timeline reads at this moment" — consistent with `datum now`
    // even when the entry being logged is backdated.
    match db
        .resolve_state_back(field, n, chrono::Utc::now())
        .map_err(anyhow::Error::from)?
    {
        Some(resolved) => Ok(Some(resolved)),
        None => Err(anyhow::anyhow!(
            "no state {n} back on field '{field}' — history is too short or \
             only contains the current state"
        )),
    }
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

/// da-mjwq: `start_time=` / `end_time=` lifted off a log line — the raw
/// tokens, taken out of `args.rest` BEFORE the data parser's `*time`
/// suffix rule would coerce them to clock strings, so they reach the one
/// time parser `-t` uses with their day, sign and `±` uncertainty intact.
#[derive(Debug, Default)]
struct LiftedTimes {
    start: Option<String>,
    end: Option<String>,
}

impl LiftedTimes {
    /// The key that names the entry's time and its raw value: `end_time=`
    /// when given (a block's occur_time is its END, da-zwct), else
    /// `start_time=`. `None` when neither key was on the line.
    fn anchor(&self) -> Option<(&'static str, &str)> {
        match (&self.start, &self.end) {
            (_, Some(e)) => Some(("end_time", e)),
            (Some(s), None) => Some(("start_time", s)),
            (None, None) => None,
        }
    }
}

/// Take every `start_time=<v>` / `end_time=<v>` token out of the
/// positionals (the last of a repeated key wins) and return their raw
/// values. Nothing of the keys stays behind for the data parser.
fn lift_time_keys(rest: &mut Vec<String>) -> LiftedTimes {
    let mut lifted = LiftedTimes::default();
    rest.retain(|tok| match tok.split_once('=') {
        Some(("start_time", v)) => {
            lifted.start = Some(v.to_string());
            false
        }
        Some(("end_time", v)) => {
            lifted.end = Some(v.to_string());
            false
        }
        _ => true,
    });
    lifted
}

/// The `-L=<v>` spelling of a switch state, so `switch f STATE end_time=E`
/// can hand its state to the `--last-state` path as the user would have
/// typed it: a string as itself, a concurrent array joined by `,`.
fn switch_state_raw(state: &JsonValue) -> String {
    match state {
        JsonValue::String(s) => s.clone(),
        JsonValue::Array(items) => items
            .iter()
            .map(switch_state_raw)
            .collect::<Vec<_>>()
            .join(","),
        other => other.to_string(),
    }
}

/// Reserved column values pulled out of the data blob by
/// [`extract_reserved_columns`]. Each field is `None` when the user
/// did not pass the corresponding key.
#[derive(Debug, Default)]
pub(crate) struct ReservedColumns {
    /// `field=` — the entry's identity. Peeled here (the shared boundary)
    /// so `update <hid> field=X` re-fields in place rather than leaking
    /// into `data` (da-5t94). On insert the positional field is injected
    /// as `field=` (see `parse_log_args`) and flows through here too.
    ///
    /// `id=` is deliberately NOT lifted here: on insert it competes with
    /// `-K` slug slots and may legitimately stay in `data` (the slot
    /// already claimed the slug), so the update path peels it locally
    /// instead — see `update_core::take_reserved_id`.
    pub(crate) field: Option<String>,
    pub(crate) state: Option<JsonValue>,
    pub(crate) dur: Option<datum_core::dur::Dur>,
    /// `dur_uncertainty=` — how precisely `dur` is known, as an ISO-8601
    /// ± radius. Lands inside `entry.dur` (da-b8rs), so it needs a `dur`
    /// to attach to.
    pub(crate) dur_uncertainty: Option<String>,
    /// `occur_time_uncertainty=` — the same, for `occur_time`.
    pub(crate) occur_time_uncertainty: Option<String>,
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
/// da-ju71.4 / da-rori: normalize an extracted `state` value. Collapse the
/// trivial single-key `{id: x}` Object back to the scalar `x` so a bare
/// `state=v` round-trips to `entry.state = v` instead of the gratuitously
/// nested `{id: v}`. Structured state (dot-prefixed `state.*` siblings) has
/// more than one key and is left intact. Idempotent on scalars and arrays,
/// so callers may pre-normalize (e.g. for slug derivation) without the
/// shared boundary double-collapsing.
pub(crate) fn normalize_state(state: JsonValue) -> JsonValue {
    if let JsonValue::Object(mut obj) = state {
        if obj.len() == 1 {
            if let Some(id) = obj.remove("id") {
                return id;
            }
        }
        return JsonValue::Object(obj);
    }
    state
}

pub(crate) fn extract_reserved_columns(
    extra_data: &mut Option<JsonValue>,
) -> Result<ReservedColumns> {
    let mut out = ReservedColumns::default();
    let Some(JsonValue::Object(obj)) = extra_data.as_mut() else {
        return Ok(out);
    };
    // da-5t94: `field=` routes onto the entry's identity column. Coerced to
    // a slug-string here so both callers (insert + update) consume a ready
    // `String` and the conversion lives in one place.
    if let Some(v) = obj.remove("field") {
        out.field = Some(datum_value_to_slug_string(Some(&v)));
    }
    if let Some(v) = obj.remove("state") {
        // da-rori: normalize at the shared boundary so `update <hid>
        // state=X` and `switch f X` yield the same `entry.state` shape.
        // Both the insert and update paths route state through here.
        out.state = Some(normalize_state(v));
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
    if let Some(v) = obj.remove("dur_uncertainty") {
        // `dur_uncertainty=null` is a sentinel for "not stated".
        if v.is_null() || matches!(v.as_str(), Some("null")) {
            // intentionally drop — leave the uncertainty unstated.
        } else {
            out.dur_uncertainty = Some(json_to_raw_token(&v));
        }
    }
    if let Some(v) = obj.remove("occur_time_uncertainty") {
        if v.is_null() || matches!(v.as_str(), Some("null")) {
            // intentionally drop — leave the uncertainty unstated.
        } else {
            out.occur_time_uncertainty = Some(json_to_raw_token(&v));
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
/// CLI parses `dur=PT1H` into `JsonValue::String("PT1H")`; a bare number
/// is reconstructed via [`json_to_raw_token`].
fn json_to_dur_string(v: &JsonValue) -> Result<String> {
    if let Some(s) = v.as_str() {
        return Ok(s.to_string());
    }
    Ok(json_to_raw_token(v))
}

/// Undo `parse_value_token` for temporal keys: reconstruct the user's raw
/// token string from its parsed JSON form: numbers and bare strings.
fn json_to_raw_token(v: &JsonValue) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(n) = v.as_i64() {
        return n.to_string();
    }
    if let Some(n) = v.as_f64() {
        return n.to_string();
    }
    v.to_string()
}

/// da-5ns.4: parse the reserved `occur_time=` payload from `dm update`:
/// "now", `±N{m,h,s}` offsets, clock times, or RFC3339 timestamps. Exposed crate-private so [`update_core::apply_mutation`]
/// can reuse the parser.
pub(crate) fn parse_reserved_occur_time(v: &JsonValue, base: &DatumTime) -> Result<DatumTime> {
    parse_add_time_value(v, base)
}

/// Parse an `occur_time=…` value token against a base time. (da-mjwq: the
/// `start_time=` / `end_time=` keys no longer come through here — they are
/// lifted raw and take the `-t` path, see [`lift_time_keys`].) Accepts:
///  * "now" → `base_time`
///  * RFC3339 timestamps → absolute
///  * clock-time forms (`11:00`, `1:32`, `8`, `5pm`, `9:15am`): parsed via
///    the same grammar as the global `-t` flag and anchored on `base`'s
///    date in its timezone. Per da-2jty, `infer_typed_value` already
///    pre-normalizes `*_time`/`*Time` keys to `HH:MM:SS`, so when the
///    user types `end_time=11:00` we land in this branch — not in the
///    duration fall-through.
///  * signed duration offsets like `-5m`, `+1h`, `-30s` → base ± offset
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
    // Clock-time path: reuse the `-t` grammar via `TimeArgs::build` so a
    // bare `11:00` / `11:00:00` / `8` / `5pm` interprets the same way the
    // global `-t` flag does. We only take this branch when the token
    // doesn't look like a signed duration (which TimeArgs would otherwise
    // greedily eat — e.g. `-5` is "5 minutes ago" to TimeArgs but `-5m`
    // is duration here). A leading `+`/`-` followed by a digit-then-unit
    // pattern stays on the duration path below.
    if !looks_like_signed_duration(s) {
        let ta = datum_parse::input::TimeArgs {
            time: Some(s.to_string()),
            ..Default::default()
        };
        if let Ok(out) = ta.build(base) {
            if let Some(t) = out.time {
                return Ok(t);
            }
        }
    }
    // Otherwise treat as a signed duration offset (`-5m`, `+1h`, `-30s`).
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

/// `±N<unit>` — a signed token with a trailing duration unit letter (`m`,
/// `s`, `h`, `d`, etc.). `+5m`, `-1h30m`, `-30s` qualify; `+30`, `11:00`,
/// `5pm` do not. Used to keep the clock-time path from greedily eating
/// signed-duration tokens that have always had duration semantics here.
fn looks_like_signed_duration(s: &str) -> bool {
    let body = match s.strip_prefix(['-', '+']) {
        Some(rest) => rest,
        None => return false,
    };
    let mut chars = body.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    body.chars()
        .any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E')
}

/// Build the log entry's base time with its effective uncertainty on it
/// (implicit from `-q*` stacks or explicit from a `±N` uncertainty suffix).
/// Reject time-specifying flags when the caller passed `-T/--no-timestamp`.
/// A timeless entry has no place for a clock offset — silently discarding
/// one would mask a user misconception, so we bail with a list of the
/// offending flags. Nor has it a duration (`has_dur`: a `dur=`, the trailing
/// positional, or a `,`-chain's): `-T` makes note-data, and a state it names
/// is a moment (da-w149).
fn check_no_time_flags(args: &LogArgs, has_dur: bool) -> Result<()> {
    let mut bad: Vec<&str> = Vec::new();
    if args.time.time.is_some() {
        bad.push("-t/--time");
    }
    if args.time.date.is_some() {
        bad.push("-d/--date");
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
    if has_dur {
        anyhow::bail!("-T makes note-data, which has no duration; drop the duration or drop -T");
    }
    Ok(())
}

pub(crate) fn build_time_with_uncertainty(args: &LogArgs) -> Result<DatumTime> {
    build_time_from_flags(&args.time, DatumTime::now().utc)
}

/// The `-t` path proper: the timing flag group resolved against `now`.
/// da-mjwq: a lifted `start_time=` / `end_time=` goes through here with its
/// raw value in the `-t` slot, so the keys and the flag cannot disagree —
/// and both keys of a pair against the same `now`, so `-20` and `-5` are
/// fifteen minutes apart exactly.
fn build_time_from_flags(time: &TimeArgs, now: chrono::DateTime<chrono::Utc>) -> Result<DatumTime> {
    let tz_name = match time.timezone.as_deref() {
        Some(spec) => Some(
            resolve_timezone(spec)
                .map_err(|e| anyhow::anyhow!("Invalid -z/--timezone value '{spec}': {e}"))?,
        ),
        None => None,
    };

    let mut builder = TimeBuilder::now()
        .with_yesterday_count(time.yesterday as i64)
        .with_quick_count(time.quick as i64)
        .with_quick_uncertainty(time.quick)
        .with_timezone(tz_name)
        .with_full_day(time.full_day);

    if let Some(d) = &time.date {
        let da = parse_date(d, TimeCtx::now())?;
        builder = builder.with_date_arg(da);
    }
    for t in time.time_values() {
        let ta = parse_time(t, TimeCtx::now())?;
        builder = builder.with_time_arg(ta);
    }

    Ok(builder.build_with_uncertainty_now(now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_value_token_number() {
        assert_eq!(parse_value_token("42"), JsonValue::Number(42.into()));
        assert_eq!(parse_value_token("true"), JsonValue::Bool(true));
    }

    /// A switch state typed as digits-then-letters is the string typed
    /// (da-jh7io): `5pm` is not `{v: 5, u: "pm"}`.
    #[test]
    fn test_parse_value_token_digits_then_letters_is_a_string() {
        for typed in ["75.2kg", "5pm", "4th"] {
            assert_eq!(parse_value_token(typed), JsonValue::String(typed.into()));
        }
    }

    #[test]
    fn test_parse_value_token_string() {
        assert_eq!(
            parse_value_token("happy"),
            JsonValue::String("happy".to_string())
        );
    }

    #[test]
    fn test_dur_start_sets_time_end() {
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("sleep");
        args.duration.duration = Some("8h".to_string());
        // Just verify the dur parsing logic: an 8h duration → end = start + 8h
        let base = build_time_with_uncertainty(&args).unwrap();
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
        let mut args = LogArgs::with_field("sleep");
        args.duration.duration = Some("8h".to_string());
        log_entry(Action::Stop, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "sleep");
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        let diff_secs = e.duration_seconds().unwrap();
        assert_eq!(diff_secs, 8 * 3600);
    }

    /// da-2jty: `end_time=HH:MM` (or the post-infer `HH:MM:SS` form) must
    /// be read as a clock time and anchored to base's date, not as 11
    /// hours of duration. Regression for the reported `add sleep
    /// end_time=11:00` producing dur=11h.
    #[test]
    fn parse_add_time_value_clock_time_anchors_to_base_date() {
        use chrono::{TimeZone, Utc};
        let base = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 5, 21, 13, 31, 12).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        // Post-infer normalized form.
        let got = parse_add_time_value(&JsonValue::String("11:00:00".into()), &base).unwrap();
        let want = Utc.with_ymd_and_hms(2026, 5, 21, 11, 0, 0).unwrap();
        assert_eq!(got.utc, want, "expected 11:00 today, got {got:?}");

        // Pre-infer raw form lands here too.
        let got = parse_add_time_value(&JsonValue::String("11:00".into()), &base).unwrap();
        assert_eq!(got.utc, want);

        // Plain integer → hour.
        let got = parse_add_time_value(&JsonValue::String("8".into()), &base).unwrap();
        assert_eq!(got.utc, Utc.with_ymd_and_hms(2026, 5, 21, 8, 0, 0).unwrap());
    }

    /// da-2jty: signed duration offsets keep their "shift base by this
    /// duration" semantics — the clock-time path must not regress them.
    #[test]
    fn parse_add_time_value_signed_duration_still_shifts_base() {
        use chrono::{TimeZone, Utc};
        let base = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 5, 21, 13, 31, 12).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        let got = parse_add_time_value(&JsonValue::String("-5m".into()), &base).unwrap();
        assert_eq!(
            got.utc,
            Utc.with_ymd_and_hms(2026, 5, 21, 13, 26, 12).unwrap()
        );

        let got = parse_add_time_value(&JsonValue::String("+1h".into()), &base).unwrap();
        assert_eq!(
            got.utc,
            Utc.with_ymd_and_hms(2026, 5, 21, 14, 31, 12).unwrap()
        );

        let got = parse_add_time_value(&JsonValue::String("-30s".into()), &base).unwrap();
        assert_eq!(
            got.utc,
            Utc.with_ymd_and_hms(2026, 5, 21, 13, 30, 42).unwrap()
        );
    }

    // ─── da-a2o8: extract_switch_then_chain ──────────────────────────────

    fn switch_args(rest: &[&str]) -> LogArgs {
        let mut a = LogArgs::with_field("body");
        a.rest = rest.iter().map(|s| s.to_string()).collect();
        a
    }

    /// `switch body vertical 5 , sit` reaches log_core with the chain
    /// already swapped-if-needed by `maybe_collapse_switch_chain`, so
    /// `args.rest` is `["vertical", "5", ",", "sit"]`. Rewrite the tail
    /// as `following_state=sit` and leave `5` for the injected `dur=` slot
    /// (da-ooe2) to consume downstream.
    #[test]
    fn then_chain_rewrites_tail_as_following_state() {
        let mut args = switch_args(&["vertical", "5", ",", "sit"]);
        extract_switch_then_chain(&mut args).unwrap();
        assert_eq!(
            args.rest,
            vec![
                "vertical".to_string(),
                "5".to_string(),
                "following_state=sit".to_string(),
            ]
        );
    }

    /// Bare-positional-only chain (`vertical , sit`) is unreachable in
    /// practice — the alias layer only routes through here when a dur
    /// signal is present — but the helper still rewrites the comma
    /// correctly so the trail isn't fragile if the caller changes.
    #[test]
    fn then_chain_works_without_duration_positional() {
        let mut args = switch_args(&["vertical", ",", "sit"]);
        extract_switch_then_chain(&mut args).unwrap();
        assert_eq!(
            args.rest,
            vec!["vertical".to_string(), "following_state=sit".to_string()]
        );
    }

    /// Concurrent-state form (`vertical,sit` glued) arrives as a single
    /// positional, never as a standalone `,` token. The helper must
    /// leave it untouched so the existing array-state path runs.
    #[test]
    fn concurrent_state_form_is_untouched() {
        let mut args = switch_args(&["vertical,sit"]);
        let before = args.rest.clone();
        extract_switch_then_chain(&mut args).unwrap();
        assert_eq!(args.rest, before);
    }

    /// Multi-step chains are deferred — second bare comma is a hard
    /// error so the helper doesn't silently consume only the last leg.
    #[test]
    fn multi_step_chain_is_rejected() {
        let mut args = switch_args(&["vertical", "5", ",", "sit", "30", ",", "sofa"]);
        let err = extract_switch_then_chain(&mut args).unwrap_err();
        assert!(err.to_string().contains("multi-step"), "got: {err}");
    }

    /// Trailing-state slot is required — `switch f X ,` (nothing after
    /// the comma) is malformed.
    #[test]
    fn missing_trailing_state_errors() {
        let mut args = switch_args(&["vertical", ","]);
        let err = extract_switch_then_chain(&mut args).unwrap_err();
        assert!(
            err.to_string().contains("requires a trailing state"),
            "got: {err}"
        );
    }

    /// Post-comma KEY=VALUE pairs are rejected — `vertical , sit
    /// hours=8` would be confusing (does `hours=8` belong to `sit` or
    /// to the row?). Force the user to clarify rather than silently
    /// dropping or misrouting the key.
    #[test]
    fn post_chain_key_value_is_rejected() {
        let mut args = switch_args(&["vertical", "5", ",", "sit", "hours=8"]);
        let err = extract_switch_then_chain(&mut args).unwrap_err();
        assert!(
            err.to_string().contains("only a bare state value"),
            "got: {err}"
        );
    }

    /// da-b4k5: the chain-context `end` keyword translates to the
    /// reserved `state=false` column AND eagerly peels the duration
    /// positional onto `args.duration`. Without the peel, the `state=true`
    /// slot would treat the next bare positional (`14m`) as the state and
    /// silently lose the duration.
    #[test]
    fn then_chain_translates_end_keyword_to_state_false() {
        let mut args = switch_args(&["end", "14m", ",", "researching"]);
        extract_switch_then_chain(&mut args).unwrap();
        assert!(
            args.rest.iter().any(|t| t == "state=false"),
            "state=false reserved key: {:?}",
            args.rest
        );
        assert!(
            args.rest.iter().any(|t| t == "following_state=researching"),
            "following_state preserved: {:?}",
            args.rest
        );
        assert!(
            !args.rest.iter().any(|t| t == "14m"),
            "dur positional peeled out of rest: {:?}",
            args.rest
        );
        assert_eq!(args.duration.duration.as_deref(), Some("14m"));
    }

    /// da-b4k5: `stop` mirrors `end` — both map to `state=false`.
    #[test]
    fn then_chain_translates_stop_keyword_to_state_false() {
        let mut args = switch_args(&["stop", "14m", ",", "Y"]);
        extract_switch_then_chain(&mut args).unwrap();
        assert!(args.rest.iter().any(|t| t == "state=false"));
        assert_eq!(args.duration.duration.as_deref(), Some("14m"));
    }

    /// da-b4k5: `start` maps to `state=true`, mirroring the non-chain
    /// `start <field>` shorthand.
    #[test]
    fn then_chain_translates_start_keyword_to_state_true() {
        let mut args = switch_args(&["start", "14m", ",", "Y"]);
        extract_switch_then_chain(&mut args).unwrap();
        assert!(args.rest.iter().any(|t| t == "state=true"));
        assert_eq!(args.duration.duration.as_deref(), Some("14m"));
    }

    /// da-vh30: under `-I`/`--string-state` the bare positionals are the
    /// LITERAL string state, so the da-b4k5 keyword translation must not
    /// fire — `end` stays a bare token (first word of the joined state)
    /// and no eager dur peel happens; the trailing-dur PEEK claims `14m`
    /// downstream instead.
    #[test]
    fn then_chain_keyword_untranslated_under_string_state() {
        let mut args = switch_args(&["end", "14m", ",", "researching"]);
        args.string_state = true;
        extract_switch_then_chain(&mut args).unwrap();
        assert_eq!(
            args.rest,
            vec![
                "end".to_string(),
                "14m".to_string(),
                "following_state=researching".to_string(),
            ]
        );
        assert!(
            args.duration.duration.is_none(),
            "no eager dur peel in string-state mode: {:?}",
            args.duration.duration
        );
    }

    /// da-b4k5: a non-keyword leading state is untouched — `vertical`
    /// stays `vertical` so the existing chain semantics for actual
    /// state values (the original da-a2o8 contract) keep working.
    #[test]
    fn then_chain_preserves_non_keyword_state() {
        let mut args = switch_args(&["vertical", "5", ",", "sit"]);
        extract_switch_then_chain(&mut args).unwrap();
        assert!(
            args.rest.iter().any(|t| t == "vertical"),
            "got: {:?}",
            args.rest
        );
    }

    /// End-to-end through `log_entry`: `switch body vertical 5 , sit`
    /// lands a single row with `state=vertical`, `dur=5m`,
    /// `following_state=sit`.
    #[test]
    fn block_switch_e2e_writes_one_row_with_following_state() {
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("body");
        args.rest = vec![
            "vertical".to_string(),
            "5".to_string(),
            ",".to_string(),
            "sit".to_string(),
        ];
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "body");
        assert_eq!(entries.len(), 1, "exactly one row");
        let e = &entries[0];
        assert_eq!(
            e.state,
            Some(serde_json::json!("vertical")),
            "state = vertical"
        );
        assert_eq!(
            e.following_state,
            Some(serde_json::json!("sit")),
            "following_state = sit"
        );
        assert_eq!(
            e.duration_seconds(),
            Some(5 * 60),
            "dur = 5 minutes (300 seconds)"
        );
    }

    /// End-to-end PointSwitch: `switch body vertical -m , sit` lands a
    /// zero-duration row with `state=vertical`,
    /// `following_state=sit`. The `-m` flag is consumed by
    /// `DurationArgs` before `extract_switch_then_chain` runs, so
    /// `args.duration.moment` carries the signal here.
    #[test]
    fn point_switch_e2e_writes_zero_duration_row() {
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("body");
        args.duration.moment = true;
        args.rest = vec!["vertical".to_string(), ",".to_string(), "sit".to_string()];
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "body");
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.state, Some(serde_json::json!("vertical")));
        assert_eq!(e.following_state, Some(serde_json::json!("sit")));
        // Moment → dur=PT0S (zero seconds).
        assert_eq!(e.duration_seconds(), Some(0));
    }

    /// da-rori: the shared `extract_reserved_columns` boundary normalizes
    /// state so insert and update can't drift. A trivial single-key
    /// `{id: x}` collapses to the scalar `x`.
    #[test]
    fn extract_reserved_columns_collapses_trivial_state_id() {
        let mut data = Some(serde_json::json!({"state": {"id": "happy"}}));
        let reserved = extract_reserved_columns(&mut data).unwrap();
        assert_eq!(reserved.state, Some(serde_json::json!("happy")));
        // state never leaks back into the data payload.
        assert!(data.is_none());
    }

    /// da-rori: structured state (multiple `state.*` siblings) survives the
    /// boundary intact — only the trivial `{id}` shape collapses.
    #[test]
    fn extract_reserved_columns_keeps_structured_state() {
        let mut data = Some(serde_json::json!({"state": {"a": 1, "b": 2}}));
        let reserved = extract_reserved_columns(&mut data).unwrap();
        assert_eq!(reserved.state, Some(serde_json::json!({"a": 1, "b": 2})));
    }

    /// da-rori: `normalize_state` is idempotent on already-normalized shapes
    /// (scalars, arrays), so the insert path's pre-slug collapse and the
    /// shared boundary collapse agree.
    #[test]
    fn normalize_state_idempotent_on_scalars_and_arrays() {
        assert_eq!(
            normalize_state(serde_json::json!("happy")),
            serde_json::json!("happy"),
        );
        assert_eq!(
            normalize_state(serde_json::json!(["a", "b"])),
            serde_json::json!(["a", "b"]),
        );
        assert_eq!(
            normalize_state(serde_json::json!({"id": 5})),
            serde_json::json!(5)
        );
    }

    /// da-rori parity: `switch mood happy` produces a scalar `entry.state`.
    /// The update-side parity assertion lives in `update_core` tests; both
    /// now route through the same `normalize_state` boundary.
    #[test]
    fn switch_yields_scalar_state() {
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("mood");
        args.rest = vec!["happy".to_string()];
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "mood");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, Some(serde_json::json!("happy")));
    }

    #[test]
    fn switch_string_state_joins_positionals_into_multiword_state() {
        // da-vo0y Feature 1: `-I`/`--string-state` joins ALL positionals into
        // the switch state — no single-positional peel, and the da-ooe2 `dur=`
        // slot is suppressed so it can't eat the first token — so a multi-word
        // state Just Works.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("tv");
        args.rest = vec!["adventure".to_string(), "time".to_string()];
        args.string_state = true;
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "tv");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, Some(serde_json::json!("adventure time")));
    }

    #[test]
    fn switch_string_state_single_token() {
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("movie");
        args.rest = vec!["blueberry".to_string()];
        args.string_state = true;
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "movie");
        assert_eq!(entries[0].state, Some(serde_json::json!("blueberry")));
    }

    #[test]
    fn string_state_rejected_for_non_switch_actions() {
        // Review fix (da-vo0y): `-I` on start/stop/block would silently
        // suppress the injected dur= slot — reject it outright.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("work");
        args.string_state = true;
        assert!(log_entry(Action::Start, args, &db).is_err());
    }

    #[test]
    fn string_state_conflicts_with_remainder_flags() {
        // Review fix (da-vo0y): `-I` + `-R`/`-S`/`-C` on switch would route
        // the joined tokens away from state, leaving a surprise state=true.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("tv");
        args.rest = vec!["a".to_string(), "b".to_string()];
        args.string_state = true;
        args.string_remainder = true;
        assert!(log_entry(Action::Switch, args, &db).is_err());
    }

    #[test]
    fn switch_underscore_resolves_previous_distinct_state() {
        // da-state-back: `switch field _` toggles back to the previous
        // distinct state, end-to-end through log_entry.
        let db = Db::open_in_memory().unwrap();
        for s in ["focus", "rest"] {
            let mut args = LogArgs::with_field("mode");
            args.rest = vec![s.to_string()];
            log_entry(Action::Switch, args, &db).unwrap();
        }
        let mut args = LogArgs::with_field("mode");
        args.rest = vec!["_".to_string()];
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "mode");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].state, Some(serde_json::json!("focus")));
    }

    #[test]
    fn switch_underscore_errors_when_history_too_short() {
        // Explicitly asking for a back-state on a field with no
        // alternatives must error, not store the literal "_".
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("mode");
        args.rest = vec!["_".to_string()];
        assert!(log_entry(Action::Switch, args, &db).is_err());
    }

    #[test]
    fn switch_string_state_trailing_duration_peek() {
        // da-vo0y F2 e2e: `switch tv -I adventure time 30m` → multi-word
        // state AND the trailing duration lands as dur.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("tv");
        args.rest = vec![
            "adventure".to_string(),
            "time".to_string(),
            "30m".to_string(),
        ];
        args.string_state = true;
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "tv");
        assert_eq!(entries[0].state, Some(serde_json::json!("adventure time")));
        assert!(entries[0].dur.is_some(), "trailing 30m must land as dur");
    }

    #[test]
    fn switch_string_state_dot_skips_dur() {
        // `switch tv -I adventure time .` → explicit skip; no dur.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("tv");
        args.rest = vec!["adventure".to_string(), "time".to_string(), ".".to_string()];
        args.string_state = true;
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "tv");
        assert_eq!(entries[0].state, Some(serde_json::json!("adventure time")));
        assert!(entries[0].dur.is_none(), ". must skip the dur peek");
    }

    // ─── da-vh30: -I (string-state) composed with the `,` chain ─────────

    #[test]
    fn switch_string_state_then_chain_block_shape() {
        // da-vh30: the -I + chain + trailing-dur-peek triple —
        // `switch body -I a b 30m , sit`. The chain peels `, sit` into
        // `following_state=sit` BEFORE the string join; the join leaves the
        // key=value alone and folds the bare tokens into the state; the
        // tail peek claims `30m` as dur → BlockSwitch.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("body");
        args.rest = ["a", "b", "30m", ",", "sit"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.string_state = true;
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "body");
        assert_eq!(entries.len(), 1, "exactly one row");
        let e = &entries[0];
        assert_eq!(e.state, Some(serde_json::json!("a b")));
        assert_eq!(e.following_state, Some(serde_json::json!("sit")));
        assert_eq!(e.duration_seconds(), Some(30 * 60));
        assert_eq!(e.verb, Verb::BlockSwitch);
    }

    #[test]
    fn switch_string_state_then_chain_point_shape() {
        // da-vh30: `switch body -I a b -m , sit` → zero-duration
        // PointSwitch with the joined multi-word state. The peek leaves
        // `b` alone (not duration-parseable), so the full window joins.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("body");
        args.duration.moment = true;
        args.rest = ["a", "b", ",", "sit"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.string_state = true;
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "body");
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.state, Some(serde_json::json!("a b")));
        assert_eq!(e.following_state, Some(serde_json::json!("sit")));
        assert_eq!(e.duration_seconds(), Some(0));
        assert_eq!(e.verb, Verb::PointSwitch);
    }

    #[test]
    fn switch_string_state_suppresses_leading_keyword_translation() {
        // da-vh30 e2e: `switch song -I end of the road 5m , next` — under
        // -I the positionals are the LITERAL string state, so the da-b4k5
        // chain-context `end` keyword translation must not fire ("end"
        // stays the first word of the state; without the gate it would
        // set state=false and try `of` as a duration). The tail peek
        // claims `5m`.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("song");
        args.rest = ["end", "of", "the", "road", "5m", ",", "next"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.string_state = true;
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "song");
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.state, Some(serde_json::json!("end of the road")));
        assert_eq!(e.following_state, Some(serde_json::json!("next")));
        assert_eq!(e.duration_seconds(), Some(5 * 60));
        assert_eq!(e.verb, Verb::BlockSwitch);
    }

    #[test]
    fn switch_string_state_post_comma_keyword_is_literal() {
        // da-vh30 decision: post-comma keywords are literal in BOTH modes
        // (there is no tail translation) — `switch body -I a b 5m , end`
        // chains into a literal "end" state, not state=false.
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("body");
        args.rest = ["a", "b", "5m", ",", "end"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.string_state = true;
        log_entry(Action::Switch, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "body");
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.state, Some(serde_json::json!("a b")));
        assert_eq!(e.following_state, Some(serde_json::json!("end")));
        assert_eq!(e.verb, Verb::BlockSwitch);
    }

    #[test]
    fn start_string_remainder_trailing_duration_peek() {
        // da-vo0y F2 e2e on a non-switch dur-bearing verb:
        // `start movie -SR title terminator 90` → title joined, dur=90m,
        // and `terminator` is NOT tried as a duration (the old dur-slot
        // front-eat).
        let db = Db::open_in_memory().unwrap();
        let mut args = LogArgs::with_field("movie");
        args.rest = vec!["terminator".to_string(), "90".to_string()];
        args.string_remainder = true;
        args.remainder = Some("title".to_string());
        log_entry(Action::Start, args, &db).unwrap();
        let entries = datum_core::test_util::logged_entries(&db, "movie");
        let data = entries[0].data.clone().expect("entry carries data");
        assert_eq!(data["title"], serde_json::json!("terminator"));
        assert!(entries[0].dur.is_some(), "trailing 90 must land as dur");
    }
}
