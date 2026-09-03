//! argv → the row the browser inserts.
//!
//! ## What is shared and what is assembled here
//!
//! Every *grammatical* rule comes from `datum-parse` and is therefore the
//! CLI's own: the pre-pass ([`expand_argv_no_alias`]), the argv parser
//! ([`parse_argv`]), the time-flag composition ([`TimeBuilder`]), the
//! `KEY=VALUE` / positional machinery ([`handle_data_args`]), the duration
//! grammar ([`Dur`]), the verb truth table ([`derive_verb`]) and entry
//! identity ([`entry_id`]). Not one of those is re-implemented; the bugs
//! this bead exists to end (`-q` off by a factor, `-y` read as minutes)
//! all lived in that layer.
//!
//! What this module *does* own is the assembly between those calls — which
//! slots to declare, which remainder bucket applies, and the action-driven
//! state canonicalization. That logic lives in
//! `datum_commands::commands::log_core`, which cannot compile to wasm
//! (it reaches the database). Each step below names the `log_core`
//! decision it mirrors, and
//! `crates/datum-commands/tests/wasm_parse_parity.rs` drives the real
//! `log_entry` and this module over the same argv matrix and fails the
//! build if they ever disagree. That test, not this comment, is what makes
//! the agreement hold.
//!
//! ## What is deliberately not supported
//!
//! The flags in [`unsupported_flag`] and the timeless verbs `add` / `note`
//! return a "not available in the browser" response rather than a wrong
//! entry. `add`/`note` store `occur_time = NULL`, and the web app's
//! `Entry` has no representation for a timeless row yet; the interim
//! TypeScript runner accepted them and stored them as *timed*, which is
//! precisely the silent-corruption failure this bead removes. Refusing is
//! the honest answer until the web side can hold a timeless entry.

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use datum_parse::alias::expand::expand_argv_no_alias;
use datum_parse::cli::{Command, LogArgs};
use datum_parse::entry_id::{self, HashInputs};
use datum_parse::input::data_args::{handle_data_args, DataArgs};
use datum_parse::input::flexible_positional::flexible_positional;
use datum_parse::time::{
    parse_date, parse_time, resolve_timezone, DatumTime, TimeBuilder, TimeCtx, Zone,
};
use datum_parse::verb::derive_verb;
use datum_parse::{parse_argv, Dur, ParseError, Result};
use serde_json::{Map, Value as JsonValue};

/// The command intents this module builds an entry for. Mirrors the
/// `Action` variants `log_core` accepts, minus the timeless pair — see the
/// module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Occur,
    Start,
    Stop,
    Switch,
}

impl Action {
    fn name(self) -> &'static str {
        match self {
            Action::Occur => "occur",
            Action::Start => "start",
            Action::Stop => "stop",
            Action::Switch => "switch",
        }
    }
}

/// What the caller knows and this crate cannot find out for itself.
pub struct Host<'a> {
    /// Wall clock, as a UTC instant.
    pub now: DateTime<Utc>,
    /// The viewer's IANA zone, when the browser can name one.
    pub timezone: Option<&'a str>,
    /// The viewer's current UTC offset in minutes. Stands in for the zone
    /// when [`Host::timezone`] is absent or is not a name we recognise —
    /// an offset is self-verifying where a name is a claim (da-2pgd).
    pub offset_minutes: i32,
    pub device_id: &'a str,
    /// Random bytes for the hid. See [`entry_id::hid_from_bytes`].
    pub hid_bytes: &'a [u8],
}

/// One entry, in the column shapes the row is stored with.
///
/// Field names and value encodings match what `try_insert` binds on the
/// Rust side (`crates/datum-core/src/db/insert.rs`), so the browser writes
/// the same bytes the CLI would: `occur_time` as RFC-3339, `state` /
/// `data` / `following_state` as JSON text, `dur` in its storage form.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltEntry {
    pub hid: String,
    pub slug: String,
    pub content_hash: String,
    pub field: String,
    pub verb: String,
    pub state: Option<JsonValue>,
    pub dur: Option<String>,
    pub following_state: Option<JsonValue>,
    pub data: Option<JsonValue>,
    pub occur_time: DatumTime,
    pub create_time: DateTime<Utc>,
    pub device_id: String,
    pub raw_input: String,
}

/// The outcome of one argv.
pub enum Parsed {
    /// A log command the browser can store.
    Entry(Box<BuiltEntry>),
    /// A well-formed command this crate does not build an entry for. The
    /// payload is the reason, for the seam's "not available yet" message.
    Unsupported(String),
}

fn unsupported(reason: impl Into<String>) -> Parsed {
    Parsed::Unsupported(reason.into())
}

/// Run one argv (without the program name) against `host`.
pub fn build(argv: &[String], host: &Host) -> Result<Parsed> {
    let raw_input = argv.join(" ");

    // The pre-pass the CLI runs before its parser: terminal-verb override,
    // field/verb swap (`mood switch happy`), switch-verb promotion, and
    // stacked short-flag expansion (`-yy` → `-y -y`). Alias expansion is
    // the one step skipped — it is DB-backed, and the browser has no alias
    // table yet.
    let mut full: Vec<String> = Vec::with_capacity(argv.len() + 1);
    full.push("datum".to_string());
    full.extend(argv.iter().cloned());
    let expanded = expand_argv_no_alias(full);

    // A `,` region is a multi-entry write (`switch a , b`) or a switch
    // "then" chain. Both produce more than one row, or a `following_state`
    // peeled by `log_core::extract_switch_then_chain`; neither is assembled
    // here, so say so rather than dropping half the user's input.
    if expanded.iter().any(|t| t == ",") {
        return Ok(unsupported(
            "`,` chains write more than one entry — not available yet in the browser",
        ));
    }

    let cli = parse_argv(&expanded)?;
    let (action, args) = match cli.command {
        Command::Occur(a) => (Action::Occur, a),
        Command::Start(a) => (Action::Start, a),
        Command::Stop(a) => (Action::Stop, a),
        Command::Switch(a) => (Action::Switch, a),
        Command::Add(_) | Command::Note(_) => {
            return Ok(unsupported(
                "`add` / `note` store a timeless entry — not available yet in the browser",
            ))
        }
        _ => return Ok(unsupported("not available yet in the browser")),
    };

    if cli.db.is_some() {
        return Ok(unsupported(
            "--db picks another database — not available yet in the browser",
        ));
    }
    if let Some(flag) = unsupported_flag(&args) {
        return Ok(unsupported(format!(
            "`{flag}` is not available yet in the browser"
        )));
    }

    Ok(Parsed::Entry(Box::new(build_entry(
        action, &args, host, &raw_input,
    )?)))
}

/// The first log flag present that this module does not assemble.
///
/// Every one of these changes where a positional lands or what the write
/// does, so accepting them silently would store something other than what
/// the user asked for. `-A`/`--show`/`-o` are absent from the list on
/// purpose: they only shape the CLI's *render*, and the browser renders
/// its own way.
fn unsupported_flag(args: &LogArgs) -> Option<&'static str> {
    let checks: [(bool, &'static str); 15] = [
        (args.undo, "-u/--undo"),
        (!args.keys.is_empty(), "-k"),
        (!args.slug_keys.is_empty(), "-K"),
        (!args.comment.is_empty(), "-c/--comment"),
        (args.remainder.is_some(), "-R/--remainder"),
        (args.string_remainder, "-S/--string-remainder"),
        (args.comment_remainder, "-C/--comment-remainder"),
        (args.string_state, "-I/--string-state"),
        (args.id.is_some(), "--id"),
        (args.last_state.is_some(), "-L/--last-state"),
        (
            args.conflict.is_some() || args.update || args.merge,
            "--update/--merge/--rekey",
        ),
        (args.no_metadata, "--no-metadata"),
        (args.alias_name.is_some(), "--alias"),
        (args.dry_run, "--dry-run"),
        // `-m`/`--moment` and `-T`/`--no-timestamp` both produce shapes the
        // web `Entry` cannot hold yet (a MOMENT-stamped switch, a timeless
        // row); refusing beats storing something else.
        (args.duration.moment || args.time.omit_timestamp, "-m/-T"),
    ];
    checks
        .into_iter()
        .find(|(hit, _)| *hit)
        .map(|(_, name)| name)
}

fn build_entry(action: Action, args: &LogArgs, host: &Host, raw_input: &str) -> Result<BuiltEntry> {
    let (zone, tz_name) = effective_zone(args.time.timezone.as_deref(), host)?;
    let occur_time = build_time(args, host.now, zone.clone(), tz_name)?;

    // `log_core` resolves the entry's local time first so relative offsets
    // on `*_time` / `*_date` data keys (`wake_time=+30`) anchor against it.
    let base_local = Some(occur_time.local_time().naive_local());
    let (field, mut data) = data_payload(action, args, base_local)?;

    let mut reserved = extract_reserved(&mut data)?;
    let field = reserved.field.take().unwrap_or(field);
    if field.is_empty() {
        return Err(ParseError::MissingRequiredKey("field".to_string()));
    }

    let mut state = reserved.state;
    let mut dur = reserved.dur;

    // da-ju71.2 (`log_core`): `occur` is a point by default, so it stamps
    // `MOMENT`, which locks `derive_verb` to `Verb::Point` no matter what
    // shape the state has. An explicit `dur=` was honoured above.
    if dur.is_none() && action == Action::Occur {
        dur = Some(Dur::moment());
    }

    // da-8y2q.2 (`log_core`): action-driven state canonicalization. With no
    // explicit state, `stop` means "the field has ended" → false, and
    // `start` / `switch` mean "transition to active" → true. Without this
    // the verb derives to `point` and a field's on/off history — every
    // duration datum computes — never exists.
    if state.is_none() {
        match action {
            Action::Stop => state = Some(JsonValue::Bool(false)),
            Action::Start | Action::Switch => state = Some(JsonValue::Bool(true)),
            Action::Occur => {}
        }
    }

    let (verb, state) = derive_verb(state, dur.as_ref(), reserved.following_state.clone())?;

    // The storage strings, exactly as `try_insert` binds them — so the row
    // the browser writes and the row the CLI writes are the same bytes, and
    // the content hash over them is the same hash.
    let occur_str = occur_time.utc.to_rfc3339();
    let dur_str = dur.as_ref().map(Dur::to_storage_string);
    let state_str = state.as_ref().map(|v| v.to_string()).unwrap_or_default();
    let data_str = data.as_ref().map(|v| v.to_string()).unwrap_or_default();
    let following_str = reserved
        .following_state
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_default();

    let content_hash = entry_id::content_hash(&HashInputs {
        field: &field,
        occur: &occur_str,
        dur: dur_str.as_deref().unwrap_or_default(),
        state: &state_str,
        data: &data_str,
        following_state: &following_str,
        deleted: false,
    });

    let slug = entry_id::build_slug(
        &field,
        Some(&occur_time),
        occur_time.uncertainty.as_deref(),
        &host.now,
    );

    Ok(BuiltEntry {
        hid: entry_id::hid_from_bytes(host.hid_bytes),
        slug,
        content_hash,
        field,
        verb: verb.to_string(),
        state,
        dur: dur_str,
        following_state: reserved.following_state,
        data,
        occur_time,
        create_time: host.now,
        device_id: host.device_id.to_string(),
        raw_input: raw_input.to_string(),
    })
}

/// The zone the entry's times are composed in.
///
/// Mirrors `TimeBuilder::host_zone`, with the browser standing in for the
/// host: `-z` wins when supplied, then the viewer's IANA name, then the
/// viewer's raw offset. The offset fallback is not `UTC` — a viewer whose
/// zone we cannot name still knows how far from Greenwich they are, and
/// naming a zone we were not given invents history (da-2pgd).
fn effective_zone(flag: Option<&str>, host: &Host) -> Result<(Zone, Option<String>)> {
    if let Some(spec) = flag {
        let name = resolve_timezone(spec)?;
        let zone = Zone::iana(&name)?;
        return Ok((zone, Some(name)));
    }
    if let Some(name) = host.timezone {
        if let Ok(zone) = Zone::iana(name) {
            return Ok((zone, Some(name.to_string())));
        }
    }
    let offset = FixedOffset::east_opt(host.offset_minutes * 60).ok_or_else(|| {
        ParseError::InvalidTime(format!("offset out of range: {} min", host.offset_minutes))
    })?;
    Ok((Zone::Fixed(offset), None))
}

/// Compose the timing flags, mirroring `log_core::build_time_with_uncertainty`
/// but anchored on the caller's clock and zone instead of the host's.
fn build_time(
    args: &LogArgs,
    now: DateTime<Utc>,
    zone: Zone,
    tz_name: Option<String>,
) -> Result<DatumTime> {
    // Ambiguous forms (`sunday`, `apr29`) resolve against the viewer's own
    // wall clock, not UTC — the same reference the CLI hands `TimeCtx::now`.
    let ctx = TimeCtx::at(zone.local_naive(now));

    let mut builder = TimeBuilder {
        days_back: 0,
        minutes_back: 0,
        time_offset: chrono::Duration::zero(),
        date_override: None,
        time_override: None,
        iso_instant_override: None,
        iso_naive_override: None,
        quick_uncertainty: None,
        explicit_uncertainty: None,
        timezone: tz_name.clone(),
        time_direction: None,
        full_day: false,
    }
    .with_yesterday_count(args.time.yesterday as i64)
    .with_quick_count(args.time.quick as i64)
    .with_quick_uncertainty(args.time.quick)
    .with_full_day(args.time.full_day);

    if let Some(d) = &args.time.date {
        builder = builder.with_date_arg(parse_date(d, ctx)?);
    }
    if let Some(t) = &args.time.time {
        builder = builder.with_time_arg(parse_time(t, ctx)?);
    }

    Ok(builder.build_with_uncertainty_at(now, zone, tz_name))
}

/// Run the positional/`KEY=VALUE` machinery, mirroring
/// `log_core::prepare_log_args` + `log_core::parse_log_args` for the four
/// actions this module supports. Returns the resolved field and the data
/// payload the reserved-column peeler then runs over.
fn data_payload(
    action: Action,
    args: &LogArgs,
    base_local: Option<chrono::NaiveDateTime>,
) -> Result<(String, Option<JsonValue>)> {
    let mut data_args = DataArgs {
        preserve_null_strings: true,
        base_local,
        ..DataArgs::default()
    };

    // `prepare_log_args`: the duration-bearing verbs declare a trailing
    // `dur=` slot, which the normal slot machinery fills from the trailing
    // positional (`stop work 30m`) or from an explicit `dur=` anywhere in
    // argv. `.` is the skip-dur sentinel and is stripped first.
    let mut rest: Vec<String> = args.rest.clone();
    if matches!(action, Action::Start | Action::Stop) {
        rest.retain(|t| t != "." && t != r"\.");
    }
    if matches!(action, Action::Switch | Action::Start | Action::Stop) {
        data_args.keys.push("dur=".to_string());
    }

    // da-r6ds (`parse_log_args`): for `switch`, the FIRST bare positional is
    // the state and is injected as the `state=true` slot's value rather than
    // competing for slot-fill. `start`/`stop` hard-set state below, so a
    // bare positional there must never coerce into it.
    let mut switch_state: Option<String> = None;
    for pos in rest {
        if action == Action::Switch && switch_state.is_none() && !pos.contains('=') {
            switch_state = Some(pos);
        } else {
            data_args.data.push(pos);
        }
    }
    if action == Action::Switch {
        if switch_state.is_some() {
            flexible_positional(&mut data_args, switch_state, "state=true", false, false);
        } else {
            data_args.keys.insert(0, "state=true".to_string());
        }
    }

    // The field goes through the same slot machinery, so `occur coffee` and
    // `occur field=coffee` are interchangeable.
    flexible_positional(
        &mut data_args,
        Some(args.field_or_verb.clone()),
        "field",
        false,
        false,
    );

    // No remainder bucket for any of these four: state is hard-set
    // (start/stop) or flows through the `state=true` slot (switch), and the
    // `dur=` slot absorbs the duration, so a leftover positional has nowhere
    // meaningful to land. Erroring beats silent coercion (da-r6ds/da-ooe2).
    data_args.remainder = None;

    let datum_data = handle_data_args(&data_args)?;
    let data = if datum_data.is_empty() {
        None
    } else {
        Some(JsonValue::Object(datum_data))
    };
    Ok((String::new(), data))
}

/// The reserved column-bound keys, peeled out of the data payload.
#[derive(Default)]
struct Reserved {
    field: Option<String>,
    state: Option<JsonValue>,
    dur: Option<Dur>,
    following_state: Option<JsonValue>,
}

/// da-5ns.3 (`log_core::extract_reserved_columns`): route `field=`,
/// `state=`, `dur=` and `following_state=` out of the user's data payload
/// onto entry columns. Unknown keys (including `value=`) stay in the
/// payload. The keys this module does not accept — `occur_time=`,
/// `dur_uncertainty=`, `occur_time_uncertainty=` — are refused rather than
/// left to ride along in `data`, where they would look like user data.
fn extract_reserved(data: &mut Option<JsonValue>) -> Result<Reserved> {
    let mut out = Reserved::default();
    let Some(JsonValue::Object(obj)) = data.as_mut() else {
        return Ok(out);
    };
    for key in ["occur_time", "dur_uncertainty", "occur_time_uncertainty"] {
        if obj.contains_key(key) {
            return Err(ParseError::InvalidArg(format!(
                "`{key}=` is not available yet in the browser"
            )));
        }
    }
    if let Some(v) = obj.remove("field") {
        out.field = Some(slug_string(&v));
    }
    if let Some(v) = obj.remove("state") {
        out.state = Some(normalize_state(v));
    }
    if let Some(v) = obj.remove("dur") {
        // `dur=null` (or the string `"null"`) is the sentinel for "no
        // duration" — leave it unset rather than writing the string "null",
        // which `derive_verb` would reject.
        if !(v.is_null() || matches!(v.as_str(), Some("null"))) {
            let s = dur_string(&v);
            out.dur = Some(
                Dur::parse(&s)
                    .ok_or_else(|| ParseError::InvalidArg(format!("invalid dur value: {s:?}")))?,
            );
        }
    }
    if let Some(v) = obj.remove("following_state") {
        out.following_state = Some(v);
    }
    if obj.is_empty() {
        *data = None;
    }
    Ok(out)
}

/// da-rori (`log_core::normalize_state`): collapse the trivial single-key
/// `{id: x}` object back to the scalar `x`, so a bare `state=v` round-trips
/// to `entry.state = v` instead of the gratuitously nested `{id: v}`.
/// Structured state (more than one key) is left intact.
fn normalize_state(state: JsonValue) -> JsonValue {
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

/// A JSON value as the plain string a slug / field name is built from.
/// Mirrors `log_core::datum_value_to_slug_string`: strings unquote, unit
/// annotations recompose, everything else takes its JSON form.
fn slug_string(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        other => raw_token(other),
    }
}

/// Recompose the token the user typed from its parsed value. The unit-
/// annotated form (`dur=5m` → `{"v": 5, "u": "m"}`) is the case that
/// matters: the duration grammar wants `5m` back, not the object.
fn raw_token(v: &JsonValue) -> String {
    if let JsonValue::Object(obj) = v {
        if obj.len() == 2 {
            if let (Some(value), Some(JsonValue::String(unit))) = (obj.get("v"), obj.get("u")) {
                return format!("{}{}", plain(value), unit);
            }
        }
    }
    plain(v)
}

fn plain(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn dur_string(v: &JsonValue) -> String {
    match v.as_str() {
        Some(s) => s.to_string(),
        None => raw_token(v),
    }
}

/// `BuiltEntry` as the JSON the browser's `Entry` is shaped like.
pub fn entry_to_json(e: &BuiltEntry) -> JsonValue {
    let mut occur = Map::new();
    occur.insert(
        "utc".to_string(),
        JsonValue::String(e.occur_time.utc.to_rfc3339()),
    );
    occur.insert(
        "offsetMinutes".to_string(),
        JsonValue::from(e.occur_time.offset_minutes),
    );
    if let Some(tz) = &e.occur_time.timezone {
        occur.insert("timezone".to_string(), JsonValue::String(tz.clone()));
    }

    let mut out = Map::new();
    out.insert("hid".to_string(), JsonValue::String(e.hid.clone()));
    out.insert("slug".to_string(), JsonValue::String(e.slug.clone()));
    out.insert(
        "contentHash".to_string(),
        JsonValue::String(e.content_hash.clone()),
    );
    out.insert("field".to_string(), JsonValue::String(e.field.clone()));
    out.insert("verb".to_string(), JsonValue::String(e.verb.clone()));
    if let Some(v) = &e.state {
        out.insert("state".to_string(), v.clone());
    }
    if let Some(d) = &e.dur {
        out.insert("dur".to_string(), JsonValue::String(d.clone()));
    }
    if let Some(v) = &e.following_state {
        out.insert("followingState".to_string(), v.clone());
    }
    if let Some(v) = &e.data {
        out.insert("data".to_string(), v.clone());
    }
    out.insert("occurTime".to_string(), JsonValue::Object(occur));
    if let Some(u) = &e.occur_time.uncertainty {
        out.insert(
            "occurTimeUncertainty".to_string(),
            JsonValue::String(u.clone()),
        );
    }
    let stamp = e.create_time.to_rfc3339();
    out.insert("createTime".to_string(), JsonValue::String(stamp.clone()));
    out.insert("modifyTime".to_string(), JsonValue::String(stamp));
    out.insert(
        "deviceId".to_string(),
        JsonValue::String(e.device_id.clone()),
    );
    out.insert(
        "rawInput".to_string(),
        JsonValue::String(e.raw_input.clone()),
    );
    JsonValue::Object(out)
}

/// A UTC instant from epoch milliseconds.
pub fn utc_from_millis(ms: i64) -> Result<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .ok_or_else(|| ParseError::InvalidTime(format!("now_ms out of range: {ms}")))
}

/// The `Action` name, for callers that want to report what was parsed.
pub fn action_name(a: Action) -> &'static str {
    a.name()
}
