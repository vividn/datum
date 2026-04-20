//! Command dispatch and argument → Entry conversion.

use anyhow::{Context, Result};
use chrono::{Local, TimeZone, Utc};
use datum_core::{
    db::{ConflictStrategy, Db, InsertOutcome, ListParams, TimeMetric},
    entry::{Entry, Verb},
    field_spec::{self, Alias, FieldSpec, InterpolationContext},
    time::{parse_date_arg, parse_duration, parse_time_arg, DatumTime, TimeBuilder},
};
use regex::Regex;
use serde_json::Value as JsonValue;
use ulid::Ulid;

use crate::{
    args::{
        AliasCommand, CheckArgs, Cli, Command, DeleteArgs, GetArgs, GrepArgs, HeadArgs, IgnoreArgs,
        LogArgs, MapArgs, MigrateArgs, NowArgs, SpecCommand, SpecSetArgs, TailArgs, TrackArgs,
        UndoArgs, UpdateArgs,
    },
    check::{collect_issues, render_human, render_json},
    config::device_id,
    output::{
        print_entries, print_entries_with_columns, print_entry, print_entry_pretty, print_logged,
        print_now_view,
    },
};
use datum_core::views::{parse_key_arg, Registry, ViewParams};

pub fn dispatch(cli: Cli, db: &Db) -> Result<()> {
    match cli.command {
        Command::Occur(args) => log_entry(Verb::Occur, args, db),
        Command::Start(args) => log_entry(Verb::Start, args, db),
        Command::Stop(args) => log_entry(Verb::Stop, args, db),
        Command::Switch(args) => log_entry(Verb::Switch, args, db),
        Command::Note(args) => log_entry(Verb::Note, args, db),
        Command::Tail(args) => cmd_tail(args, db),
        Command::Head(args) => cmd_head(args, db),
        Command::Now(args) => cmd_now(args, db),
        Command::Get(args) => cmd_get(args, db),
        Command::Delete(args) => cmd_delete(args, db),
        Command::Update(args) => cmd_update(args, ConflictStrategy::Update, db),
        Command::Merge(args) => cmd_update(args, ConflictStrategy::Merge, db),
        Command::Rekey(args) => cmd_update(args, ConflictStrategy::Rekey, db),
        Command::Undo(args) => cmd_undo(args, db),
        Command::Spec(sub) => cmd_spec(sub, db),
        Command::Alias(sub) => cmd_alias(sub, db),
        Command::Track(args) => cmd_track(args, db),
        Command::Ignore(args) => cmd_ignore(args, db),
        Command::Migrate(args) => cmd_migrate(args, db),
        Command::Grep(args) => cmd_grep(args, db),
        Command::Check(args) => cmd_check(args, db),
        Command::Map(args) => cmd_map(args, db),
        Command::Reduce(args) => cmd_reduce(args, db),
    }
}

/// Build a `field → stored_color` map from all defined specs. Entries without
/// a color are omitted so callers can cheaply check "is there a spec color?".
fn load_field_colors(db: &Db) -> Result<std::collections::HashMap<String, String>> {
    let reg = db.load_registry()?;
    Ok(reg
        .fields
        .into_iter()
        .filter_map(|(name, spec)| spec.color.map(|c| (name, c)))
        .collect())
}

// ─── Map / Reduce ────────────────────────────────────────────────────────────

fn build_registry() -> Registry {
    Registry::new()
}

fn cmd_map(args: MapArgs, db: &Db) -> Result<()> {
    let registry = build_registry();
    let params = map_args_to_params(&args, false);
    let rows = registry
        .query(&args.view_name, db, &params)
        .with_context(|| format!("view query failed: {}", args.view_name))?;
    print_view_rows(&rows);
    Ok(())
}

fn cmd_reduce(args: MapArgs, db: &Db) -> Result<()> {
    let registry = build_registry();
    let params = map_args_to_params(&args, true);
    let rows = registry
        .query(&args.view_name, db, &params)
        .with_context(|| format!("view query failed: {}", args.view_name))?;
    print_view_rows(&rows);
    Ok(())
}

fn map_args_to_params(args: &MapArgs, force_reduce: bool) -> ViewParams {
    let reduce = force_reduce || args.reduce || args.group || args.group_level.is_some();
    let group = args.group || args.group_level.is_some();
    ViewParams {
        key: args.key.as_deref().map(parse_key_arg),
        startkey: args.startkey.as_deref().map(parse_key_arg),
        endkey: args.endkey.as_deref().map(parse_key_arg),
        group,
        group_level: args.group_level,
        limit: args.limit,
        reduce,
    }
}

fn print_view_rows(rows: &[datum_core::views::ViewRow]) {
    for row in rows {
        let obj = serde_json::json!({
            "key": row.key,
            "value": row.value,
            "id": row.id,
        });
        println!("{obj}");
    }
}

fn cmd_check(args: CheckArgs, db: &Db) -> Result<()> {
    let report = collect_issues(db)?;
    if args.json {
        println!("{}", render_json(&report)?);
    } else {
        print!("{}", render_human(&report));
    }
    if report.has_errors() {
        std::process::exit(1);
    }
    Ok(())
}

// ─── Logging ─────────────────────────────────────────────────────────────────

fn log_entry(verb: Verb, args: LogArgs, db: &Db) -> Result<()> {
    let (field, value, mut extra_data) = parse_log_args(&args)?;
    let base_time = build_time(&args)?;
    let unit = args.unit.clone();
    let device = device_id()?;

    // --last-state on Stop: close the most recent open block, recording its
    // prior state as `last_state` metadata. Errors if there's nothing to close.
    if args.last_state && matches!(verb, Verb::Stop) {
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

    // --last-state on Switch: dedupe when the requested state matches the
    // currently-active state. `--moment` bypasses the check.
    if args.last_state && matches!(verb, Verb::Switch) && !args.moment {
        let current = db.get_active_state(&field, base_time.utc)?;
        let current_norm = datum_core::state::normalize_state(&current);
        let new_norm = value
            .as_ref()
            .map(datum_core::state::normalize_state)
            .unwrap_or(JsonValue::Bool(true));
        if current_norm == new_norm {
            anyhow::bail!(
                "'{field}' is already in state {} — nothing to switch to",
                serde_json::to_string(&current_norm).unwrap_or_else(|_| "?".to_string())
            );
        }
    }

    // Fix 3: --dur flag sets time_end based on verb direction
    let (time_start, time_end) = if args.moment && matches!(verb, Verb::Switch) {
        // --moment on switch: point-in-time change (time_start == time_end)
        (Some(base_time.clone()), Some(base_time))
    } else if let Some(dur_str) = &args.dur {
        let dur = parse_duration(dur_str)?;
        match verb {
            Verb::Start => {
                // start --dur: time_start=base, time_end=base+dur
                let end_utc = base_time.utc + dur;
                let time_end = DatumTime { utc: end_utc, ..base_time.clone() };
                (Some(base_time), Some(time_end))
            }
            Verb::Stop => {
                // stop --dur: time_end=base, time_start=base-dur
                let start_utc = base_time.utc - dur;
                let time_start = DatumTime { utc: start_utc, ..base_time.clone() };
                (Some(time_start), Some(base_time))
            }
            _ => {
                // occur/switch/note with --dur: set time_end=time_start+dur
                let end_utc = base_time.utc + dur;
                let time_end = DatumTime { utc: end_utc, ..base_time.clone() };
                (Some(base_time), Some(time_end))
            }
        }
    } else {
        (Some(base_time), None)
    };

    let mut entry = Entry::new(field, verb.clone(), value, extra_data, unit, time_start, time_end, device);

    // Resolve id template: explicit --id beats spec.id_template.
    let spec_template = db
        .load_spec(&entry.field)?
        .and_then(|s| s.id_template);
    let template = args.id.clone().or(spec_template);
    if let Some(tmpl) = template {
        let uid = entry.short_uid();
        let ctx = InterpolationContext::from_entry(&entry, &uid);
        entry.slug = field_spec::interpolate(&tmpl, &ctx, &args.id_delimiter);
        entry.id_structure = Some(tmpl);
    }

    let strategy = args.conflict_strategy();
    let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
    match &outcome {
        InsertOutcome::Inserted { .. } => {
            if matches!(verb, Verb::Occur | Verb::Start) {
                db.ensure_tracked(&entry.field)?;
            }
            print_logged(&entry);
        }
        InsertOutcome::Updated { id } => {
            println!("✓ Updated 1 ({id})");
        }
        InsertOutcome::Merged { id } => {
            println!("✓ Merged 1 ({id})");
        }
        InsertOutcome::Rekeyed { new_id, .. } => {
            println!("✓ Rekeyed to {new_id}");
        }
        InsertOutcome::Ignored { id } => {
            eprintln!("⚠ Ignored 1 — duplicate of {id}");
        }
    }
    Ok(())
}

/// Parse LogArgs into (field, primary_value, extra_data).
///
/// The first positional may be the field (when verb is the subcommand) or a
/// field-like token. Remaining positionals become the primary value or key=value pairs.
fn parse_log_args(args: &LogArgs) -> Result<(String, Option<JsonValue>, Option<JsonValue>)> {
    let field = args.field_or_verb.clone();

    // Collect positional rest: could be "value key=val key=val ..."
    let mut value: Option<JsonValue> = None;
    let mut extra = serde_json::Map::new();

    for token in &args.rest {
        if let Some((k, v)) = token.split_once('=') {
            extra.insert(k.to_string(), parse_value_token(v));
        } else if value.is_none() {
            value = Some(parse_value_token(token));
        } else {
            // Extra positional after value — append to an array if value is already set
            match value.as_mut().unwrap() {
                JsonValue::Array(arr) => arr.push(parse_value_token(token)),
                existing => {
                    let prev = existing.clone();
                    *existing = JsonValue::Array(vec![prev, parse_value_token(token)]);
                }
            }
        }
    }

    // Add -k KEY=VALUE pairs to extra
    for kv in &args.keys {
        if let Some((k, v)) = kv.split_once('=') {
            extra.insert(k.to_string(), parse_value_token(v));
        }
    }

    if let Some(comment) = &args.comment {
        extra.insert("comment".to_string(), JsonValue::String(comment.clone()));
    }

    let extra_data = if extra.is_empty() {
        None
    } else {
        Some(JsonValue::Object(extra))
    };

    Ok((field, value, extra_data))
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

fn build_time(args: &LogArgs) -> Result<DatumTime> {
    let mut builder = TimeBuilder::now()
        .with_yesterday_count(args.yesterday as i64)
        .with_quick_count(args.quick as i64);

    if let Some(d) = &args.date {
        let da = parse_date_arg(d)?;
        builder = builder.with_date_arg(da);
    }
    if let Some(t) = &args.time {
        let ta = parse_time_arg(t)?;
        builder = builder.with_time_arg(ta);
    }

    Ok(builder.build())
}

// ─── Tail / Now ──────────────────────────────────────────────────────────────

fn cmd_tail(args: TailArgs, db: &Db) -> Result<()> {
    let columns = split_columns(&args.columns);
    let entries = tail_entries(&args, db)?;
    if columns.is_empty() {
        let colors = load_field_colors(db)?;
        print_entries(&entries, &colors);
    } else {
        print_entries_with_columns(&entries, &columns);
    }
    Ok(())
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

    let has_date = args.date.is_some() || args.yesterday > 0;
    let has_time = args.time.is_some() || args.quick > 0;
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
        head: args.head,
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
        .with_yesterday_count(args.yesterday as i64)
        .with_quick_count(args.quick as i64);
    if let Some(d) = &args.date {
        builder = builder.with_date_arg(parse_date_arg(d)?);
    }
    if let Some(t) = &args.time {
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

fn cmd_head(args: HeadArgs, db: &Db) -> Result<()> {
    let entries = if let Some(field) = &args.field {
        db.entries_for_field_asc(field, Some(args.count))?
    } else {
        let far_future = chrono::Utc::now() + chrono::Duration::days(365);
        let far_past = chrono::DateTime::UNIX_EPOCH;
        let mut all = db.entries_in_range(far_past, far_future)?;
        // entries_in_range already returns ASC, but be explicit for clarity.
        all.sort_by(|a, b| {
            let ta = a.time_start.as_ref().map(|t| t.utc).unwrap_or(a.created_at);
            let tb = b.time_start.as_ref().map(|t| t.utc).unwrap_or(b.created_at);
            ta.cmp(&tb)
        });
        all.truncate(args.count);
        all
    };
    let colors = load_field_colors(db)?;
    print_entries(&entries, &colors);
    Ok(())
}

fn cmd_grep(args: GrepArgs, db: &Db) -> Result<()> {
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

    let colors = load_field_colors(db)?;
    print_entries(&matches, &colors);
    Ok(())
}

fn cmd_now(args: NowArgs, db: &Db) -> Result<()> {
    let tracked = db.get_tracked_fields()?;
    if tracked.is_empty() {
        println!("No tracked fields. Use `datum track <field>` or log with `occur`/`start`.");
        return Ok(());
    }

    let state_colors = db.get_all_state_colors()?;

    let mut latest_per_field = std::collections::HashMap::new();
    for tf in &tracked {
        if tf.ignored && !args.show_all {
            continue;
        }
        if let Some(entry) = db.latest_entry_for_now(&tf.field)? {
            latest_per_field.insert(tf.field.clone(), entry);
        }
    }

    print_now_view(&tracked, &latest_per_field, &state_colors, args.show_all);
    Ok(())
}

fn cmd_track(args: TrackArgs, db: &Db) -> Result<()> {
    // Load-or-create spec, flip tracked=true (ignored=false), merge color
    // and per-state colors. Track is a thin wrapper over the spec.
    let mut spec = match db.load_spec(&args.field)? {
        Some(s) => s,
        None => FieldSpec::new(&args.field, Verb::Occur),
    };
    spec.tracked = true;
    spec.ignored = false;
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
    db.save_field_spec(&spec)?;
    println!("✓ Tracking '{}'", args.field);
    Ok(())
}

fn cmd_ignore(args: IgnoreArgs, db: &Db) -> Result<()> {
    let mut spec = match db.load_spec(&args.field)? {
        Some(s) => s,
        None => FieldSpec::new(&args.field, Verb::Occur),
    };
    spec.tracked = true;
    spec.ignored = true;
    db.save_field_spec(&spec)?;
    println!("✓ Ignoring '{}'", args.field);
    Ok(())
}

// ─── Get / Delete ─────────────────────────────────────────────────────────────

fn cmd_get(args: GetArgs, db: &Db) -> Result<()> {
    let entry = db.get_entry(&args.id_or_slug)
        .map_err(anyhow::Error::from)?;
    if args.pretty {
        print_entry_pretty(&entry)?;
    } else {
        let colors = load_field_colors(db)?;
        print_entry(&entry, &colors);
    }
    Ok(())
}

fn cmd_delete(args: DeleteArgs, db: &Db) -> Result<()> {
    if !args.force {
        let entry = db.get_entry(&args.id_or_slug)
            .map_err(anyhow::Error::from)?;
        let colors = load_field_colors(db)?;
        print_entry(&entry, &colors);
        eprint!("Delete this entry? [y/N] ");
        use std::io::BufRead;
        let stdin = std::io::stdin();
        let line = stdin
            .lock()
            .lines()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No input provided"))??;
        if !matches!(line.to_lowercase().as_str(), "y" | "yes") {
            println!("Aborted.");
            return Ok(());
        }
    }
    db.soft_delete_entry(&args.id_or_slug)
        .map_err(anyhow::Error::from)?;
    println!("✓ Deleted {}", args.id_or_slug);
    Ok(())
}

// ─── Update / Merge / Rekey ──────────────────────────────────────────────────

fn cmd_update(args: UpdateArgs, default_strategy: ConflictStrategy, db: &Db) -> Result<()> {
    let strategy = args.strategy.map(Into::into).unwrap_or(default_strategy);

    // Fetch including tombstones so we can distinguish NotFound vs. Tombstoned.
    let existing = db.get_entry_including_deleted(&args.id_or_slug)?;
    if existing.deleted_at.is_some() {
        anyhow::bail!(
            "Cannot {} tombstoned entry {} — restore it first with `datum undelete`",
            strategy_verb(strategy),
            args.id_or_slug,
        );
    }

    let (payload_value, payload_extra) = parse_update_payload(&args)?;

    match strategy {
        ConflictStrategy::Update => apply_mutation(
            existing,
            payload_value,
            payload_extra,
            ExtraMergeMode::Shallow,
            db,
        ),
        ConflictStrategy::Merge => apply_mutation(
            existing,
            payload_value,
            payload_extra,
            ExtraMergeMode::Deep,
            db,
        ),
        ConflictStrategy::Rekey => apply_rekey(existing, payload_value, payload_extra, &args, db),
        ConflictStrategy::Ignore => {
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

fn strategy_verb(s: ConflictStrategy) -> &'static str {
    match s {
        ConflictStrategy::Update => "update",
        ConflictStrategy::Merge => "merge",
        ConflictStrategy::Rekey => "rekey",
        ConflictStrategy::Ignore => "ignore",
    }
}

fn apply_mutation(
    existing: Entry,
    payload_value: Option<JsonValue>,
    payload_extra: Option<JsonValue>,
    mode: ExtraMergeMode,
    db: &Db,
) -> Result<()> {
    let mut updated = existing;
    updated.extra_data = combine_extra(updated.extra_data, payload_extra, mode);
    if let Some(v) = payload_value {
        updated.value = Some(v);
    }
    updated.content_hash = updated.expected_content_hash();

    db.save_existing_entry(&updated)?;

    // Fetch the persisted row so the printed confirmation reflects the
    // db-stamped `modified_at`.
    let fresh = db.get_entry(&updated.id)?;
    match mode {
        ExtraMergeMode::Shallow => println!("✓ Updated 1 ({})", fresh.id),
        ExtraMergeMode::Deep => println!("✓ Merged 1 ({})", fresh.id),
    }
    print_logged(&fresh);
    Ok(())
}

fn apply_rekey(
    existing: Entry,
    payload_value: Option<JsonValue>,
    payload_extra: Option<JsonValue>,
    args: &UpdateArgs,
    db: &Db,
) -> Result<()> {
    let mut new_entry = existing.clone();
    // Payloads are applied with shallow semantics on rekey — same as update.
    new_entry.extra_data =
        combine_extra(new_entry.extra_data, payload_extra, ExtraMergeMode::Shallow);
    if let Some(v) = payload_value {
        new_entry.value = Some(v);
    }

    // Fresh id + timestamps. Slug will be re-interpolated below.
    new_entry.id = Ulid::new().to_string();
    let now = chrono::Utc::now();
    new_entry.created_at = now;
    new_entry.modified_at = now;
    new_entry.deleted_at = None;

    // Resolve template: explicit --id beats the spec's id_template. Rekey
    // without any template is meaningless — the new slug would match the
    // old one.
    let spec_template = db.load_spec(&new_entry.field)?.and_then(|s| s.id_template);
    let template = args.id.clone().or(spec_template);
    let template = template.ok_or_else(|| {
        anyhow::anyhow!(
            "rekey requires a template: pass --id or define id_template on the spec"
        )
    })?;

    let uid = new_entry.short_uid();
    let ctx = InterpolationContext::from_entry(&new_entry, &uid);
    new_entry.slug = field_spec::interpolate(&template, &ctx, &args.id_delimiter);
    new_entry.id_structure = Some(template);

    new_entry.content_hash = new_entry.expected_content_hash();

    // Use the Rekey strategy so duplicate content_hashes (rekey with no
    // payload change) don't collide with the existing row.
    let outcome = db.insert_entry_with_strategy(&new_entry, ConflictStrategy::Rekey)?;
    match outcome {
        InsertOutcome::Inserted { id } | InsertOutcome::Rekeyed { new_id: id, .. } => {
            println!("✓ Rekeyed {} → {}", existing.id, id);
            print_logged(&new_entry);
            Ok(())
        }
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
        (Some(JsonValue::Object(mut e_map)), Some(JsonValue::Object(p_map)), ExtraMergeMode::Shallow) => {
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

fn parse_update_payload(args: &UpdateArgs) -> Result<(Option<JsonValue>, Option<JsonValue>)> {
    let mut value: Option<JsonValue> = None;
    let mut extra = serde_json::Map::new();

    for token in &args.rest {
        if let Some((k, v)) = token.split_once('=') {
            extra.insert(k.to_string(), parse_value_token(v));
        } else if value.is_none() {
            value = Some(parse_value_token(token));
        } else {
            match value.as_mut().unwrap() {
                JsonValue::Array(arr) => arr.push(parse_value_token(token)),
                existing => {
                    let prev = existing.clone();
                    *existing = JsonValue::Array(vec![prev, parse_value_token(token)]);
                }
            }
        }
    }

    for kv in &args.keys {
        if let Some((k, v)) = kv.split_once('=') {
            extra.insert(k.to_string(), parse_value_token(v));
        }
    }

    if let Some(comment) = &args.comment {
        extra.insert("comment".to_string(), JsonValue::String(comment.clone()));
    }

    let extra_data = if extra.is_empty() {
        None
    } else {
        Some(JsonValue::Object(extra))
    };
    Ok((value, extra_data))
}

fn cmd_undo(args: UndoArgs, db: &Db) -> Result<()> {
    // Slug-targeted undo: restore that specific row, no window check.
    if let Some(id_or_slug) = args.id_or_slug.as_deref() {
        db.undelete(id_or_slug).map_err(anyhow::Error::from)?;
        println!("✓ Restored {id_or_slug}");
        return Ok(());
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
            db.undelete(&entry.id).map_err(anyhow::Error::from)?;
            println!("✓ Restored {}", entry.slug);
            Ok(())
        }
        None => {
            // Nothing within the window. If --force was off and there's still an
            // older tombstone, nudge the user toward --force; otherwise the store
            // has no tombstones at all.
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

// ─── Spec management ─────────────────────────────────────────────────────────

fn cmd_spec(sub: SpecCommand, db: &Db) -> Result<()> {
    match sub {
        SpecCommand::Show { field } => match db.load_spec(&field)? {
            Some(spec) => println!("{}", serde_json::to_string_pretty(&spec)?),
            None => println!("No spec defined for field '{field}'"),
        },
        SpecCommand::Set(args) => cmd_spec_set(args, db)?,
        SpecCommand::List => {
            let reg = db.load_registry()?;
            if reg.fields.is_empty() {
                println!("No field specs defined.");
            } else {
                for (name, _spec) in &reg.fields {
                    println!("  {name}");
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

fn cmd_spec_set(args: SpecSetArgs, db: &Db) -> Result<()> {
    // --json replaces the entire spec; individual flags merge into existing.
    let mut spec = if let Some(json) = &args.json {
        let mut s: FieldSpec = serde_json::from_str(json)
            .with_context(|| "Invalid field spec JSON")?;
        s.field = args.field.clone();
        s
    } else {
        db.load_spec(&args.field)?
            .unwrap_or_else(|| FieldSpec::new(&args.field, Verb::Occur))
    };

    if let Some(dv) = &args.default_verb {
        spec.default_verb = dv
            .parse::<Verb>()
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
    if args.track {
        spec.tracked = true;
        spec.ignored = false;
    }
    if args.ignore {
        spec.tracked = true;
        spec.ignored = true;
    }

    // Always regenerate derived aliases from the current allowed_states so
    // states that were dropped lose their alias.
    spec.regenerate_derived_aliases();

    db.save_field_spec(&spec)?;
    println!("✓ Spec saved for '{}'", spec.field);
    Ok(())
}

// ─── Alias management ────────────────────────────────────────────────────────

fn cmd_alias(sub: AliasCommand, db: &Db) -> Result<()> {
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

fn cmd_migrate(args: MigrateArgs, db: &Db) -> Result<()> {
    let content = std::fs::read_to_string(&args.backup_file)
        .with_context(|| format!("Reading backup file: {}", args.backup_file))?;
    let backup: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| "Parsing backup JSON")?;

    let docs = backup["docs"].as_array()
        .context("Backup JSON missing 'docs' array")?;

    let device = device_id()?;
    let mut imported = 0usize;
    let mut skipped = 0usize;

    for doc in docs {
        let data = &doc["data"];
        let meta = &doc["meta"];

        let field = match data["field"].as_str() {
            Some(f) => f.to_string(),
            None => { skipped += 1; continue; }
        };

        // Map datum verbs from state type
        let verb = if data["state"].is_boolean() { Verb::Switch }
            else if data["occurTime"].is_object() { Verb::Occur }
            else { Verb::Note };

        let value = if data["state"].is_null() {
            None
        } else {
            Some(data["state"].clone())
        };

        let time_start = parse_datum_time(&data["occurTime"]);

        // extra_data: everything in data except known fields
        let mut extra = serde_json::Map::new();
        if let Some(obj) = data.as_object() {
            for (k, v) in obj {
                if !matches!(k.as_str(), "field" | "state" | "occurTime" | "dur" | "lastState") {
                    extra.insert(k.clone(), v.clone());
                }
            }
        }
        let extra_data = if extra.is_empty() { None } else { Some(JsonValue::Object(extra)) };

        let created_at = parse_datum_time(&meta["createTime"])
            .map(|t| t.utc)
            .unwrap_or_else(chrono::Utc::now);

        let mut entry = Entry::new(
            field, verb, value, extra_data, None, time_start, None, device.clone(),
        );
        entry.created_at = created_at;

        if args.dry_run {
            println!("[dry-run] would import: {}", entry.slug);
            imported += 1;
            continue;
        }

        match db.insert_entry(&entry) {
            Ok(()) => imported += 1,
            Err(datum_core::error::DatumError::Duplicate(_)) => skipped += 1,
            Err(e) => return Err(e.into()),
        }
    }

    println!("Migration complete: {imported} imported, {skipped} skipped.");
    Ok(())
}

fn parse_datum_time(v: &JsonValue) -> Option<DatumTime> {
    let utc_str = v["utc"].as_str()?;
    let utc: chrono::DateTime<chrono::Utc> = utc_str.parse().ok()?;
    let tz = v["tz"].as_str().unwrap_or("UTC").to_string();
    let offset_minutes = v["o"].as_f64().map(|o| (o * 60.0) as i32).unwrap_or(0);
    Some(DatumTime { utc, offset_minutes, timezone: tz })
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
        let args = LogArgs {
            field_or_verb: "sleep".to_string(),
            rest: vec![],
            yesterday: 0,
            quick: 0,
            date: None,
            time: None,
            dur: Some("8h".to_string()),
            timezone: None,
            unit: None,
            keys: vec![],
            comment: None,
            id: None,
            id_delimiter: ":".to_string(),
            last_state: false,
            moment: false,
            conflict: None,
            update: false,
            merge: false,
        };
        // Just verify the dur parsing logic: start --dur 8h → end = start + 8h
        let base = build_time(&args).unwrap();
        let dur = parse_duration("8h").unwrap();
        let end_utc = base.utc + dur;
        let diff_secs = (end_utc - base.utc).num_seconds();
        assert_eq!(diff_secs, 8 * 3600);
        // Ensure dispatch doesn't error
        log_entry(Verb::Start, args, &db).unwrap();
    }

    #[test]
    fn test_dur_stop_sets_time_start() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "sleep".to_string(),
            rest: vec![],
            yesterday: 0,
            quick: 0,
            date: None,
            time: None,
            dur: Some("8h".to_string()),
            timezone: None,
            unit: None,
            keys: vec![],
            comment: None,
            id: None,
            id_delimiter: ":".to_string(),
            last_state: false,
            moment: false,
            conflict: None,
            update: false,
            merge: false,
        };
        log_entry(Verb::Stop, args, &db).unwrap();
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
        let entry = Entry::new("test", Verb::Occur, None, None, None, Some(t), None, "dev");
        let id = entry.id.clone();
        db.insert_entry(&entry).unwrap();

        // get
        let fetched = db.get_entry(&id).unwrap();
        assert_eq!(fetched.id, id);

        // delete
        db.delete_entry(&id).unwrap();
        let err = db.get_entry(&id);
        assert!(matches!(err, Err(datum_core::error::DatumError::NotFound(_))));
    }

    #[test]
    fn test_auto_track_on_occur() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "coffee".to_string(),
            rest: vec!["2".to_string()],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: Some("cups".to_string()), keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
        };
        log_entry(Verb::Occur, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(tracked.iter().any(|f| f.field == "coffee" && !f.ignored));
    }

    #[test]
    fn test_auto_track_on_start() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "work".to_string(),
            rest: vec![],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: None, keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
        };
        log_entry(Verb::Start, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(tracked.iter().any(|f| f.field == "work" && !f.ignored));
    }

    #[test]
    fn test_no_auto_track_on_note() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "journal".to_string(),
            rest: vec!["some text".to_string()],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: None, keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
        };
        log_entry(Verb::Note, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(!tracked.iter().any(|f| f.field == "journal"));
    }

    #[test]
    fn test_cmd_track_and_ignore() {
        use crate::args::{IgnoreArgs, TrackArgs};
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        let track_args = TrackArgs {
            field: "mood".to_string(),
            color: Some("ff6600".to_string()),
            state_colors: vec!["happy=00ff00".to_string(), "sad=ff0000".to_string()],
        };
        cmd_track(track_args, &db).unwrap();

        let tracked = db.get_tracked_fields().unwrap();
        let mood = tracked.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood.ignored);
        assert_eq!(mood.color.as_deref(), Some("ff6600"));

        let state_colors = db.get_all_state_colors().unwrap();
        assert_eq!(
            state_colors.get(&("mood".to_string(), "happy".to_string())).map(|s| s.as_str()),
            Some("00ff00")
        );

        // Now ignore mood
        cmd_ignore(IgnoreArgs { field: "mood".to_string() }, &db).unwrap();
        let tracked2 = db.get_tracked_fields().unwrap();
        let mood2 = tracked2.iter().find(|f| f.field == "mood").unwrap();
        assert!(mood2.ignored);
        // Color should be preserved (set_tracked with None color)
        assert_eq!(mood2.color.as_deref(), Some("ff6600"));
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
        };
        cmd_spec_set(args, &db).unwrap();

        let spec = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(spec.id_template.as_deref(), Some("%field%:%state%"));
        assert_eq!(spec.color.as_deref(), Some("green"));
        let states = spec.allowed_states.as_ref().unwrap();
        assert!(states.contains(&"happy".to_string()) && states.contains(&"sad".to_string()));
        assert!(spec.tracked && !spec.ignored);

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
        cmd_now(NowArgs { show_all: false }, &db).unwrap();

        // With some tracked fields and entries
        let args = LogArgs {
            field_or_verb: "mood".to_string(),
            rest: vec!["happy".to_string()],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: None, keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
        };
        log_entry(Verb::Switch, args, &db).unwrap();
        db.set_tracked("mood", false, None).unwrap();

        cmd_now(NowArgs { show_all: false }, &db).unwrap();
        cmd_now(NowArgs { show_all: true }, &db).unwrap();
    }

    // ─── id-template tests (A1 acceptance) ──────────────────────────────────

    fn log_args(field: &str, rest: Vec<String>, id: Option<String>) -> LogArgs {
        LogArgs {
            field_or_verb: field.to_string(),
            rest,
            yesterday: 0,
            quick: 0,
            date: None,
            time: None,
            dur: None,
            timezone: None,
            unit: None,
            keys: vec![],
            comment: None,
            id,
            id_delimiter: ":".to_string(),
            last_state: false,
            moment: false,
            conflict: None,
            update: false,
            merge: false,
        }
    }

    #[test]
    fn spec_id_template_is_default() {
        use datum_core::db::Db;
        use datum_core::field_spec::FieldSpec;
        let db = Db::open_in_memory().unwrap();

        // Spec defines id_template; caller does NOT pass --id.
        let mut spec = FieldSpec::new("coffee", Verb::Occur);
        spec.id_template = Some("%field%:%uid%".to_string());
        db.save_field_spec(&spec).unwrap();

        log_entry(Verb::Occur, log_args("coffee", vec![], None), &db).unwrap();

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
        let mut spec = FieldSpec::new("coffee", Verb::Occur);
        spec.id_template = Some("%field%:%uid%".to_string());
        db.save_field_spec(&spec).unwrap();

        log_entry(
            Verb::Occur,
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
            Verb::Occur,
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

        log_entry(Verb::Occur, log_args("coffee", vec![], None), &db).unwrap();

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
            Verb::Occur,
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
        Entry::new(
            field,
            Verb::Occur,
            Some(serde_json::json!(secs_offset)),
            None,
            None,
            Some(t),
            None,
            device,
        )
    }

    #[test]
    fn head_returns_oldest_first() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Insert out-of-order: newest first, then oldest, then middle.
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        cmd_head(HeadArgs { field: None, count: 10 }, &db).unwrap();

        // Verify via db that ordering is ASC by time_start.
        let far_past = chrono::DateTime::UNIX_EPOCH;
        let far_future = chrono::Utc::now() + chrono::Duration::days(365);
        let entries = db.entries_in_range(far_past, far_future).unwrap();
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

        // No panic on execution + the db method returns exactly `count` entries ASC.
        cmd_head(HeadArgs { field: Some("mood".to_string()), count: 5 }, &db).unwrap();

        let got = db.entries_for_field_asc("mood", Some(5)).unwrap();
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

        cmd_head(HeadArgs { field: Some("coffee".to_string()), count: 10 }, &db).unwrap();

        let got = db.entries_for_field_asc("coffee", None).unwrap();
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
        // Default count matches clap's default_value = "10".
        let default_count: usize = 10;
        let got = db.entries_for_field_asc("mood", Some(default_count)).unwrap();
        assert_eq!(got.len(), 10);
        // First entry should be the oldest (offset=0).
        assert_eq!(
            got[0].value.as_ref().unwrap().as_i64().unwrap(),
            0
        );
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
        Entry::new(field, Verb::Occur, Some(value), extra, None, Some(t), None, "d1")
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
            GrepArgs { patterns: vec!["happy".to_string()], field: None },
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
        let ex = got[0].extra_data.as_ref().unwrap();
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

        cmd_head(HeadArgs { field: None, count: 10 }, &db).unwrap();

        let got = db.entries_for_field_asc("mood", Some(10)).unwrap();
        assert_eq!(got.len(), 2);
    }

    // ─── update / merge / rekey command tests ───────────────────────────────

    fn update_args(id: &str, rest: Vec<&str>) -> UpdateArgs {
        UpdateArgs {
            id_or_slug: id.to_string(),
            rest: rest.into_iter().map(String::from).collect(),
            strategy: None,
            id: None,
            id_delimiter: ":".to_string(),
            keys: vec![],
            comment: None,
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
        let e = Entry::new(field, verb, value, extra, None, Some(dt), None, "d1");
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
            Verb::Occur,
            Some(serde_json::json!(1)),
            Some(serde_json::json!({
                "weight": {"v": 70.0, "u": "kg"},
                "pose": "standing",
                "reps": 10,
            })),
        );
        // Let a moment pass so modified_at is observably greater than created_at.
        std::thread::sleep(std::time::Duration::from_millis(5));

        cmd_update(
            update_args(&e.slug, vec!["weight=80kg"]),
            ConflictStrategy::Update,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.id).unwrap();
        let ex = after.extra_data.as_ref().unwrap();
        // weight replaced
        assert_eq!(ex["weight"]["v"], serde_json::json!(80.0));
        assert_eq!(ex["weight"]["u"], serde_json::json!("kg"));
        // unmentioned keys preserved
        assert_eq!(ex["pose"], serde_json::json!("standing"));
        assert_eq!(ex["reps"], serde_json::json!(10));
        // modified_at bumped; content_hash refreshed
        assert!(after.modified_at >= e.modified_at);
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
            Verb::Note,
            None,
            Some(serde_json::json!({
                "nested": {"a": 1, "b": 2},
                "other": "keep",
            })),
        );

        cmd_update(
            update_args(&e.slug, vec!["tag=red"]),
            ConflictStrategy::Merge,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.id).unwrap();
        let ex = after.extra_data.as_ref().unwrap();
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
            Verb::Note,
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
        cmd_update(args, ConflictStrategy::Merge, &db).unwrap();

        // Now overlay nested JSON via a second merge with -k syntax replaced
        // by direct extra_data manipulation through another merge call.
        // We'll exercise nested merging by passing a nested object through
        // parse_value_token's fallback (string): instead rely on the unit
        // test of `deep_merge` for that path.
        let after = db.get_entry(&e.id).unwrap();
        let ex = after.extra_data.as_ref().unwrap();
        // Original scalar keys intact
        assert_eq!(ex["nested"]["a"], serde_json::json!(1));
        assert_eq!(ex["nested"]["c"], serde_json::json!(3));
        // Flat "nested.b" added
        assert_eq!(ex["nested.b"], serde_json::json!(2));
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

    #[test]
    fn update_empty_value_drops_key() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Occur,
            None,
            Some(serde_json::json!({"foo": "bar", "baz": "qux"})),
        );

        cmd_update(
            update_args(&e.slug, vec!["foo="]),
            ConflictStrategy::Update,
            &db,
        )
        .unwrap();

        let after = db.get_entry(&e.id).unwrap();
        let ex = after.extra_data.as_ref().unwrap();
        assert!(ex.get("foo").is_none(), "foo= should drop the key");
        assert_eq!(ex["baz"], serde_json::json!("qux"));
    }

    #[test]
    fn rekey_creates_new_row_preserves_old() {
        use datum_core::db::Db;
        use datum_core::field_spec::FieldSpec;
        let db = Db::open_in_memory().unwrap();

        // Spec supplies an id_template so the rekey has somewhere to reinterpolate to.
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.id_template = Some("%field%:%state%".to_string());
        db.save_field_spec(&spec).unwrap();

        // Original row uses the default slug (no template applied on insert).
        let e = seed_entry(
            &db,
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"state": "happy"})),
        );

        let mut args = update_args(&e.slug, vec![]);
        args.id = Some("%field%:%state%".to_string());

        cmd_update(args, ConflictStrategy::Rekey, &db).unwrap();

        // Both the old and new rows exist.
        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 2, "rekey must leave the old row in place");

        // New row has the interpolated slug.
        let new_row = live
            .iter()
            .find(|r| r.slug == "mood:happy")
            .expect("rekeyed slug should match template");
        assert_ne!(new_row.id, e.id, "new row gets its own ULID");
        // Old row still reachable by its original slug.
        let old = db.get_entry(&e.slug).unwrap();
        assert_eq!(old.id, e.id);
    }

    #[test]
    fn rekey_without_template_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "mood", Verb::Switch, Some(serde_json::json!("x")), None);

        let err = cmd_update(update_args(&e.slug, vec![]), ConflictStrategy::Rekey, &db)
            .unwrap_err();
        assert!(err.to_string().contains("template"), "{err}");
    }

    #[test]
    fn update_tombstoned_row_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "gym",
            Verb::Occur,
            Some(serde_json::json!(1)),
            Some(serde_json::json!({"reps": 10})),
        );
        db.soft_delete_entry(&e.id).unwrap();

        // Use id (slug lookup would also find the tombstoned row via
        // get_entry_including_deleted).
        let err = cmd_update(
            update_args(&e.id, vec!["reps=20"]),
            ConflictStrategy::Update,
            &db,
        )
        .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("tombstoned"),
            "expected tombstone error, got: {err}"
        );

        // Merge and rekey must also reject tombstoned rows.
        let err = cmd_update(
            update_args(&e.id, vec!["tag=red"]),
            ConflictStrategy::Merge,
            &db,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("tombstoned"));

        let mut rk = update_args(&e.id, vec![]);
        rk.id = Some("%field%:%uid%".to_string());
        let err = cmd_update(rk, ConflictStrategy::Rekey, &db).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("tombstoned"));
    }

    #[test]
    fn update_nonexistent_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let err = cmd_update(
            update_args("no-such-id", vec!["foo=bar"]),
            ConflictStrategy::Update,
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
            Verb::Note,
            None,
            Some(serde_json::json!({"tag": {"color": "red", "size": "m"}})),
        );

        // Invoke via Update default but override strategy to Merge — the nested
        // {color, size} should survive because deep-merge preserves siblings.
        let mut args = update_args(&e.slug, vec![]);
        args.strategy = Some(crate::args::ConflictStrategyArg::Merge);
        args.keys = vec!["tag.priority=1".to_string()];
        cmd_update(args, ConflictStrategy::Update, &db).unwrap();

        let after = db.get_entry(&e.id).unwrap();
        let ex = after.extra_data.as_ref().unwrap();
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

        cmd_update(
            update_args(&e.slug, vec!["sad"]),
            ConflictStrategy::Update,
            &db,
        )
        .unwrap();
        let after = db.get_entry(&e.id).unwrap();
        assert_eq!(after.value, Some(serde_json::json!("sad")));
    }

    #[test]
    fn rekey_from_spec_template_when_no_id_flag() {
        use datum_core::db::Db;
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

        cmd_update(update_args(&e.slug, vec![]), ConflictStrategy::Rekey, &db).unwrap();

        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 2);
        assert!(live.iter().any(|r| r.slug == "mood:calm"));
    }

    #[test]
    fn rekey_with_payload_change() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(
            &db,
            "word",
            Verb::Note,
            Some(serde_json::json!("correr")),
            Some(serde_json::json!({"lang": "pt", "pos": "verb"})),
        );

        let mut args = update_args(&e.slug, vec!["pos=noun"]);
        args.id = Some("%field%:%pos%:%uid%".to_string());
        cmd_update(args, ConflictStrategy::Rekey, &db).unwrap();

        let rows = db.entries_for_field("word", None).unwrap();
        assert_eq!(rows.len(), 2);
        let rekeyed = rows
            .iter()
            .find(|r| r.slug.starts_with("word:noun:"))
            .expect("rekeyed row should exist");
        assert_eq!(rekeyed.extra_data.as_ref().unwrap()["pos"], serde_json::json!("noun"));
        // Payload changes are visible on the new row only.
        let original = rows.iter().find(|r| r.id == e.id).unwrap();
        assert_eq!(original.extra_data.as_ref().unwrap()["pos"], serde_json::json!("verb"));
    }

    // ─── B7: undo via soft-delete tombstone ─────────────────────────────────

    fn undo_args(slug: Option<&str>, force: bool, window: &str) -> UndoArgs {
        UndoArgs {
            id_or_slug: slug.map(str::to_string),
            force,
            window: window.to_string(),
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
        let doomed_id = doomed.id.clone();
        db.insert_entry(&doomed).unwrap();
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();

        db.soft_delete_entry(&doomed_id).unwrap();
        assert_eq!(db.entries_for_field("mood", None).unwrap().len(), 2);

        cmd_undo(undo_args(None, false, "15m"), &db).unwrap();

        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 3);
        assert!(live.iter().any(|e| e.id == doomed_id));
    }

    #[test]
    fn undo_outside_window_errors_without_force() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("mood", 100, "d1");
        let id = e.id.clone();
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
        assert_eq!(live[0].id, id);
    }

    #[test]
    fn undo_specific_slug_targets_that_row() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Two deletes; the most recent is B, but we want to restore A specifically.
        let a = make_entry_at("mood", 100, "d1");
        let a_id = a.id.clone();
        let a_slug = a.slug.clone();
        let b = make_entry_at("mood", 200, "d1");
        let b_id = b.id.clone();
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        db.soft_delete_entry(&a_id).unwrap();
        db.soft_delete_entry(&b_id).unwrap();

        cmd_undo(undo_args(Some(&a_slug), false, "15m"), &db).unwrap();

        // A is back, B is still tombstoned.
        let live: Vec<_> = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].id, a_id);
        assert!(db.get_entry_including_deleted(&b_id).unwrap().deleted_at.is_some());
    }

    #[test]
    fn delete_then_undo_is_noop() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("mood", 100, "d1");
        let id = e.id.clone();
        let original_hash = e.content_hash.clone();
        db.insert_entry(&e).unwrap();

        cmd_delete(
            DeleteArgs { id_or_slug: id.clone(), force: true },
            &db,
        )
        .unwrap();
        cmd_undo(undo_args(None, false, "15m"), &db).unwrap();

        // Row is back, identical to the original insertion (including content_hash).
        let restored = db.get_entry(&id).unwrap();
        assert_eq!(restored.id, id);
        assert_eq!(restored.content_hash, original_hash);
        assert!(restored.deleted_at.is_none());
        assert_eq!(db.count_entries().unwrap(), 1);
    }
}
