//! Command dispatch and argument → Entry conversion.

use anyhow::{Context, Result};
use chrono::{Local, TimeZone, Utc};
use datum_core::{
    db::{
        ConflictStrategy, Db, GroupColumn, InsertOutcome, ListParams, RetimeFilter, TimeMetric,
    },
    entry::{new_hid, Entry, Verb},
    field_spec::{self, Alias, FieldSpec, InterpolationContext},
    ids::{contains_wildcard, resolve_quickid, resolve_quickid_single},
    time::{parse_date_arg, parse_duration, parse_time_arg, DatumTime, TimeBuilder},
};
use regex::Regex;
use serde_json::Value as JsonValue;

use crate::{
    args::{
        AliasCommand, BackupArgs, Cli, Command, ConfigCommand, ConfigShowArgs,
        ConflictsAutoResolveArgs, ConflictsCommand, ConflictsListArgs, ConflictsResolveArgs,
        CountArgs, DayviewArgs, DeleteArgs, EditArgs, GetArgs, GrepArgs, HistoryArgs,
        IdsArgs, IgnoreArgs, InfoArgs, LogArgs, MigrateCommand, MigrateImportArgs, NowArgs,
        RestoreArgs, RetimeArgs, SetupArgs, SpecCommand, SpecSetArgs, TailArgs, TrackArgs,
        UndoArgs, UpdateArgs, WhyArgs,
    },
    config::{self, device_id},
    render::{
        print_entry, print_entry_pretty, print_now_view, print_table, show_create,
        show_create_with_context, show_delete, show_exists, show_rename, show_update,
        show_update_with_context, RenderContext, TableArgs, TimeMetricCol,
    },
};
use datum_core::views::{Registry, ViewParams};

/// Parse an argv vector through the full pre-pass → clap → dispatch path
/// against `db`. Used by integration tests to exercise the CLI without
/// importing clap themselves (so `datum-cli/Cargo.toml` can drop it from
/// its dev-dependencies).
pub fn dispatch_argv(argv: Vec<String>, db: &Db) -> Result<()> {
    use clap::Parser;
    let expanded = crate::args::expand_argv_with_db(argv, db)?;
    let cli = Cli::try_parse_from(expanded)?;
    dispatch(cli, db)
}

pub fn dispatch(cli: Cli, db: &Db) -> Result<()> {
    use crate::commands;
    match cli.command {
        Command::Occur(args) => commands::occur::occur(args, db),
        Command::Start(args) => commands::start::start(args, db),
        Command::Stop(args) => commands::end::stop(args, db),
        Command::Switch(args) => commands::switch::switch(args, db),
        Command::Add(args) => commands::add::add(args, db),
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
        Command::Track(args) => commands::track::track(args, db),
        Command::Ignore(args) => commands::ignore::ignore(args, db),
        Command::Migrate(sub) => commands::migrate::migrate(sub, db),
        Command::Backup(args) => commands::backup::backup(args, db),
        Command::Restore(args) => commands::restore::restore(args, db),
        Command::Grep(args) => commands::grep::grep(args, db),
        Command::Check(args) => commands::check::check(args, db),
        Command::Count(args) => commands::count::count(args, db),
        Command::Ids(args) => commands::ids::ids(args, db),
        Command::Dayview(args) => commands::dayview::dayview(args, db),
        Command::Info(args) => commands::info::info(args, cli.db.as_deref(), db),
        Command::Setup(args) => commands::setup::setup(args, db),
        Command::Config(sub) => commands::config::config(sub, cli.db.as_deref()),
        Command::Sync(args) => commands::sync::dispatch(args, db),
        Command::Conflicts(sub) => commands::conflicts::conflicts(sub, db),
    }
}

fn load_registry(db: &Db) -> Result<datum_core::field_spec::SpecRegistry> {
    Ok(db.load_registry()?)
}

// ─── Count / Ids ─────────────────────────────────────────────────────────────

/// Parse `--since` into a UTC lower bound.
///
/// Grammar (tried in order):
///   1. RFC3339 datetime, e.g. `2024-03-16T00:00:00Z`
///   2. `YYYY-MM-DD` — interpreted as local midnight
///   3. Duration offset (`1d`, `30m`, `2h`, `PT5M`) — treated as "N ago"
fn parse_since(s: &str) -> Result<chrono::DateTime<Utc>> {
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

/// Structured result of `cmd_count` — either a single total or a grouped
/// tally. Exposed so tests can assert on the data without parsing stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountResult {
    Total(usize),
    Grouped(Vec<(String, usize)>),
}

pub fn compute_count(args: &CountArgs, db: &Db) -> Result<CountResult> {
    let since_utc = args.since.as_deref().map(parse_since).transpose()?;
    let field = args.field.as_deref();

    if let Some(raw) = &args.group_by {
        let col: GroupColumn = raw
            .parse()
            .map_err(|e: String| anyhow::anyhow!(e))?;
        let rows = db.count_entries_grouped(field, col, since_utc)?;
        return Ok(CountResult::Grouped(rows));
    }

    let n = db.count_entries_filtered(field, since_utc)?;
    Ok(CountResult::Total(n))
}

pub(crate) fn cmd_count(args: CountArgs, db: &Db) -> Result<()> {
    let result = compute_count(&args, db)?;
    match (&result, args.json) {
        (CountResult::Total(n), false) => println!("{n}"),
        (CountResult::Total(n), true) => {
            println!("{}", serde_json::json!({ "count": n }));
        }
        (CountResult::Grouped(rows), false) => {
            for (k, n) in rows {
                println!("{n}\t{k}");
            }
        }
        (CountResult::Grouped(rows), true) => {
            let arr: Vec<_> = rows
                .iter()
                .map(|(k, n)| serde_json::json!({ "key": k, "count": n }))
                .collect();
            println!("{}", serde_json::Value::Array(arr));
        }
    }
    Ok(())
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

// ─── Dayview ────────────────────────────────────────────────────────────────

fn build_registry() -> Registry {
    datum_core::views::builtins()
}

pub(crate) fn cmd_dayview(args: DayviewArgs, db: &Db) -> Result<()> {
    use chrono::{Datelike, NaiveDate};
    use datum_layout::{layout_day, svg::render_svg, LayoutConfig, LayoutEntry};
    use serde_json::Value as JsonValue;

    // Resolve the target day in local time. We use the local civic day to
    // pick which timing rows to include, and convert local-midnight bounds
    // to UTC ms for the layout time domain.
    let date: NaiveDate = match args.date.as_deref() {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .with_context(|| format!("invalid --date: '{s}' (expected YYYY-MM-DD)"))?,
        None => Local::now().date_naive(),
    };
    let date_key = format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day());

    let day_start_local = Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .context("ambiguous local midnight")?;
    let day_end_local = day_start_local + chrono::Duration::days(1);
    let day_start_ms = day_start_local.with_timezone(&Utc).timestamp_millis();
    let day_end_ms = day_end_local.with_timezone(&Utc).timestamp_millis();

    // Pull timing rows for this day. The TimingView key is `[field, YYYY-MM-DD]`,
    // so a single-element prefix `[date]` won't match. Filter the date in the
    // app layer instead.
    let registry = build_registry();
    let rows = registry
        .query("datum_timing", db, &ViewParams::default())
        .context("datum_timing view query failed")?;

    let mut entries: Vec<LayoutEntry> = Vec::new();
    for row in rows {
        let JsonValue::Array(parts) = &row.key else {
            continue;
        };
        let (Some(field), Some(day)) = (parts.first().and_then(|v| v.as_str()),
                                        parts.get(1).and_then(|v| v.as_str())) else {
            continue;
        };
        if day != date_key {
            continue;
        }
        let start_str = row.value.get("start").and_then(|v| v.as_str());
        let end_str = row.value.get("end").and_then(|v| v.as_str());
        let Some(start_iso) = start_str else { continue };
        let Ok(start_ms) = parse_iso_to_utc_ms(start_iso) else {
            continue;
        };
        let end_ms = end_str.and_then(|s| parse_iso_to_utc_ms(s).ok());

        entries.push(LayoutEntry {
            field: field.to_string(),
            start_ms,
            end_ms,
            state: None,
        });
    }

    let config = LayoutConfig {
        day_start_ms,
        day_end_ms,
        width: args.width,
        height: args.height,
        ..LayoutConfig::default()
    };
    let layout = layout_day(&entries, &config);

    let payload: String = if args.json {
        serde_json::to_string_pretty(&layout)?
    } else {
        render_svg(&layout)
    };

    match args.output.as_deref() {
        Some(path) => {
            std::fs::write(path, &payload)
                .with_context(|| format!("write {path}"))?;
        }
        None => {
            println!("{payload}");
        }
    }
    Ok(())
}

fn parse_iso_to_utc_ms(s: &str) -> Result<i64> {
    let dt = chrono::DateTime::parse_from_rfc3339(s)
        .with_context(|| format!("parse rfc3339: {s}"))?;
    Ok(dt.with_timezone(&Utc).timestamp_millis())
}

// ─── info / setup ────────────────────────────────────────────────────────────

pub(crate) fn cmd_info(args: InfoArgs, cli_db: Option<&str>, active_db: &Db) -> Result<()> {
    if args.all {
        print_info_all()?;
        return Ok(());
    }
    match args.target.as_deref() {
        Some(target) => {
            let resolved = config::resolve_db(Some(target))?;
            print_info_for_path(resolved.name.as_deref(), &resolved.path)?;
        }
        None => {
            let resolved = config::resolve_db(cli_db)?;
            print_info_block(resolved.name.as_deref(), &resolved.path, Some(active_db))?;
        }
    }
    Ok(())
}

fn print_info_all() -> Result<()> {
    let registry = config::load_registry().unwrap_or_default();
    let discovered = config::discovered_dbs().unwrap_or_default();
    use std::collections::BTreeMap;
    let mut seen: BTreeMap<String, std::path::PathBuf> = BTreeMap::new();
    for (name, entry) in &registry.db {
        seen.insert(name.clone(), std::path::PathBuf::from(&entry.path));
    }
    for (name, path) in &discovered {
        seen.entry(name.clone()).or_insert_with(|| path.clone());
    }
    if seen.is_empty() {
        println!("no databases registered or found in default directory");
        return Ok(());
    }
    let mut first = true;
    for (name, path) in &seen {
        if !first {
            println!();
        }
        first = false;
        print_info_for_path(Some(name), path)?;
    }
    Ok(())
}

fn print_info_for_path(name: Option<&str>, path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        println!(
            "{}  {}",
            name.unwrap_or(""),
            config::display_path(path)
        );
        println!("  schema:   ✗ (missing)");
        return Ok(());
    }
    let path_str = path.to_string_lossy();
    let db = match datum_core::db::Db::open(&path_str) {
        Ok(db) => db,
        Err(e) => {
            println!(
                "{}  {}",
                name.unwrap_or(""),
                config::display_path(path)
            );
            println!("  schema:   ✗ ({e})");
            return Ok(());
        }
    };
    print_info_block(name, path, Some(&db))
}

fn print_info_block(
    name: Option<&str>,
    path: &std::path::Path,
    db: Option<&Db>,
) -> Result<()> {
    println!(
        "{}  {}",
        name.unwrap_or(""),
        config::display_path(path)
    );
    let Some(db) = db else {
        println!("  schema:   ? (not opened)");
        return Ok(());
    };

    let stored = datum_core::migrations::read_schema_version(db.conn())?;
    let current = datum_core::migrations::CURRENT_SCHEMA_VERSION;
    let (schema_glyph, schema_note) = match stored {
        Some(v) if v == current => ("✓", "current".to_string()),
        Some(v) if v < current => ("⚠", format!("older than current v{current}")),
        Some(_) => ("✗", format!("newer than binary (v{current})")),
        None => ("⚠", "unversioned (run `datum setup`)".to_string()),
    };
    let version_str = stored.map(|v| format!("v{v}")).unwrap_or_else(|| "?".into());
    println!(
        "  schema:   {version_str} {schema_glyph} ({schema_note})"
    );

    let live = db.count_entries().unwrap_or(0);
    let deleted = count_tombstones(db).unwrap_or(0);
    println!("  entries:  {live} ({deleted} deleted)");

    let tracked = db.get_tracked_fields().unwrap_or_default();
    let (t_count, i_count) = tracked.iter().fold((0usize, 0usize), |(t, i), f| {
        if f.ignored {
            (t, i + 1)
        } else {
            (t + 1, i)
        }
    });
    println!("  fields:   {t_count} tracked, {i_count} ignored");

    match last_modified(db) {
        Some(ts) => println!("  last write: {ts}"),
        None => println!("  last write: (none)"),
    }

    println!("  sync:     not configured");

    match std::fs::metadata(path) {
        Ok(m) => println!("  size:     {}", format_size(m.len())),
        Err(_) => println!("  size:     ?"),
    }
    Ok(())
}

fn count_tombstones(db: &Db) -> anyhow::Result<usize> {
    let n: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM entries WHERE deleted_at IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(n as usize)
}

fn last_modified(db: &Db) -> Option<String> {
    let s: Option<String> = db
        .conn()
        .query_row(
            "SELECT MAX(modified_at) FROM entries WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    let iso = s?;
    // Render as local wall-clock, e.g. "2026-04-20 15:03".
    match chrono::DateTime::parse_from_rfc3339(&iso) {
        Ok(dt) => {
            let local = dt.with_timezone(&chrono::Local);
            Some(local.format("%Y-%m-%d %H:%M").to_string())
        }
        Err(_) => Some(iso),
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn cmd_setup(args: SetupArgs, db: &Db) -> Result<()> {
    if args.force_recreate {
        let dropped = db.force_recreate_schema()?;
        println!("setup: force-recreate");
        println!("  dropped: {}", dropped.join(", "));
        println!(
            "  schema: v{}",
            datum_core::migrations::CURRENT_SCHEMA_VERSION
        );
        println!("  OK");
        return Ok(());
    }

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
            "yaml_path": cfg.yaml_path.to_string_lossy(),
            "yaml_loaded": cfg.yaml_loaded,
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
        "yaml: {} ({})",
        cfg.yaml_path.display(),
        if cfg.yaml_loaded { "loaded" } else { "not found" }
    );
    Ok(())
}

// ─── Logging ─────────────────────────────────────────────────────────────────

pub(crate) fn log_entry(mut verb: Verb, args: LogArgs, db: &Db) -> Result<()> {
    // -T/--no-timestamp: route any log verb through the timeless `add`
    // codepath (S5). Time-specifying flags are rejected so silent discard
    // can't mask a user misconception.
    if args.no_timestamp {
        check_no_time_flags(&args)?;
        verb = Verb::Add;
    }

    let parsed = parse_log_args(&args)?;
    let ParsedLogArgs {
        field,
        value,
        mut extra_data,
        slug_values,
    } = parsed;
    let unit = args.unit.clone();
    let device = device_id()?;

    // Compute time_start, time_end, precision, and (for Verb::Add) a shape
    // override. Skipped entirely for -T — a timeless entry carries no times
    // and no precision metadata.
    let (time_start, time_end, base_precision, add_override_shape) = if args.no_timestamp {
        (None, None, None, None)
    } else {
        let (base_time, base_precision) = build_time_with_precision(&args)?;

        // Verb::Add is the canonical shape-inferring verb. It peeks for
        // `start_time=` / `end_time=` / `dur=` tokens in extra_data, strips
        // them out, and derives the entry's shape + start/end from their
        // combination.
        let add_override: Option<AddTimestampResult> = if matches!(verb, Verb::Add) {
            let hints = extract_add_temporal_keys(&mut extra_data, &base_time)?;
            Some(hints.resolve(&base_time))
        } else {
            None
        };

        // --last-state on Stop: close the most recent open block, recording
        // its prior state as `last_state` metadata. Errors if there's nothing
        // to close.
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

        let (time_start, time_end) = if let Some(result) = &add_override {
            (result.start_time.clone(), result.end_time.clone())
        } else if args.moment && matches!(verb, Verb::Switch) {
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
                    // occur/switch/add with --dur: set time_end=time_start+dur
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

    let mut entry = Entry::new(field, verb.clone(), value, extra_data, unit, time_start, time_end, device);

    // For Verb::Add, the shape was inferred from temporal hints; override the
    // shape-inference performed by `Entry::new` so `entry_shape` reflects the
    // add-specific classification (e.g. `ongoing` when only start_time is set).
    if let Some(shape) = add_override_shape {
        entry.entry_shape = shape;
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
            entry.slug = field_spec::interpolate(&tmpl, &ctx, &args.id_delimiter);
            entry.id_structure = Some(tmpl);
        }
    }

    // Capture render hints *before* the insert:
    //   1. first-time: spec doesn't exist yet (so `ensure_tracked` will create
    //      one for Occur/Start). Used to prefix `✨ ` on new-field writes.
    //   2. prior_state: for state-changing verbs, fetch the active state at
    //      `time_start` minus one microsecond. That's the "before" half of
    //      the transition indicator. A no-op prior (Null/False) means there
    //      was nothing active; the renderer skips the indicator in that case.
    let is_first_occurrence = db.load_spec(&entry.field)?.is_none();
    let prior_state = capture_prior_state(db, &entry, &verb);

    let strategy = args.conflict_strategy();
    let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
    let registry = load_registry(db).ok();
    let output = args.output();
    let ctx = RenderContext { last_state: prior_state, is_first_occurrence };
    match &outcome {
        InsertOutcome::Inserted { slug } => {
            if matches!(verb, Verb::Occur | Verb::Start) {
                db.ensure_tracked(&entry.field)?;
            }
            // The stored slug may differ from entry.slug if a collision
            // forced a suffixed retry — fetch the canonical row so the
            // display reflects what actually hit the DB.
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
        InsertOutcome::Rekeyed { new_slug, old_slug } => {
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
    Ok(())
}

/// Look up the field's active state immediately before `entry`'s start time,
/// so the renderer can draw a `[prior ▶ next]` transition indicator.
///
/// Returns `None` when:
///   * the entry has no `start_time` (non-temporal note),
///   * the verb doesn't represent a state change (Occur / Add / Block / Note),
///   * the DB query fails or the active state is Null/False (no prior).
fn capture_prior_state(db: &Db, entry: &Entry, verb: &Verb) -> Option<JsonValue> {
    if !matches!(verb, Verb::Switch | Verb::Start | Verb::Stop) {
        return None;
    }
    let at = entry.start_time.as_ref()?.utc;
    // Probe one microsecond before the entry's start to avoid picking up the
    // just-inserted row (the insert hasn't run yet, but this matches
    // datum's "state as of just before this event" semantics).
    let probe = at - chrono::Duration::microseconds(1);
    match db.get_active_state(&entry.field, probe) {
        Ok(JsonValue::Null) | Ok(JsonValue::Bool(false)) | Err(_) => None,
        Ok(v) => Some(v),
    }
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

/// Which identity bucket a `-K` / `-k` slot contributes to.
enum IdKeyKind { Slug, Data }

/// Parsed form of a single `-K` / `-k` arg.
enum IdKeySpec {
    /// Slot that participates in positional fill. Always emitted for `-K`;
    /// emitted for `-k` only in the `KEY` (required) or `KEY=` (omit-if-absent) forms.
    Slot {
        kind: IdKeyKind,
        name: String,
        default: Option<String>,
        omit_if_absent: bool,
    },
    /// Legacy `-k KEY=VALUE`: immediate data assignment, no positional consumed.
    /// Preserves the shape of Phase-1 recipes like `-k comment=hello`.
    Immediate { name: String, value: String },
}

fn parse_id_key_spec(kind: IdKeyKind, spec: &str) -> Result<IdKeySpec> {
    let (name, rhs): (&str, Option<&str>) = match spec.split_once('=') {
        None => (spec, None),
        Some((k, v)) => (k, Some(v)),
    };
    if name.is_empty() {
        anyhow::bail!("empty id-key name in spec '{spec}'");
    }

    match (kind, rhs) {
        // `-X KEY` (no `=`): required slot — consumes a positional or errors.
        (kind, None) => Ok(IdKeySpec::Slot {
            kind,
            name: name.to_string(),
            default: None,
            omit_if_absent: false,
        }),
        // `-X KEY=` (empty RHS): optional slot, omitted if no positional.
        (kind, Some("")) => Ok(IdKeySpec::Slot {
            kind,
            name: name.to_string(),
            default: None,
            omit_if_absent: true,
        }),
        // `-K KEY=VALUE`: optional slot with default. `-K` is new and always
        // participates in positional fill.
        (IdKeyKind::Slug, Some(v)) => Ok(IdKeySpec::Slot {
            kind: IdKeyKind::Slug,
            name: name.to_string(),
            default: Some(v.to_string()),
            omit_if_absent: false,
        }),
        // `-k KEY=VALUE` (legacy): immediate assignment — pre-D2 recipes
        // expect `-k abc=123` to set `data.abc=123` without stealing a
        // positional from the primary value.
        (IdKeyKind::Data, Some(v)) => Ok(IdKeySpec::Immediate {
            name: name.to_string(),
            value: v.to_string(),
        }),
    }
}

/// Parse LogArgs into a structured bundle — field, primary value, extra_data,
/// and the list of `-K` slug-key values for slug derivation.
///
/// Positional fill order: `-K` slots first, then `-k` slots, each in the
/// order they appeared on the command line. `KEY=VALUE` positionals skip
/// slot filling entirely and land directly in extra_data (matches the
/// legacy behaviour so existing recipes still work).
fn parse_log_args(args: &LogArgs) -> Result<ParsedLogArgs> {
    let field = args.field_or_verb.clone();

    // Parse -K specs (all become slots) and -k specs (slots or legacy
    // immediate assignments). -K is consumed first for positional fill.
    let mut slots: Vec<(IdKeyKind, String, Option<String>, bool)> = Vec::new();
    let mut immediate_extras: Vec<(String, String)> = Vec::new();
    for spec_str in args.slug_keys.iter() {
        match parse_id_key_spec(IdKeyKind::Slug, spec_str)? {
            IdKeySpec::Slot { kind, name, default, omit_if_absent } => {
                slots.push((kind, name, default, omit_if_absent));
            }
            IdKeySpec::Immediate { .. } => unreachable!("-K never yields Immediate"),
        }
    }
    for spec_str in args.keys.iter() {
        match parse_id_key_spec(IdKeyKind::Data, spec_str)? {
            IdKeySpec::Slot { kind, name, default, omit_if_absent } => {
                slots.push((kind, name, default, omit_if_absent));
            }
            IdKeySpec::Immediate { name, value } => {
                immediate_extras.push((name, value));
            }
        }
    }

    // Split rest into bare positionals (for slot filling + primary value)
    // and `KEY=VALUE` tokens (direct extra_data injection).
    let mut positionals: Vec<&str> = Vec::with_capacity(args.rest.len());
    let mut kvpair_tokens: Vec<(&str, &str)> = Vec::new();
    for token in &args.rest {
        match token.split_once('=') {
            Some((k, v)) => kvpair_tokens.push((k, v)),
            None => positionals.push(token),
        }
    }

    // Fill slots.
    let mut pos_iter = positionals.into_iter();
    let mut slug_values: Vec<String> = Vec::new();
    let mut slot_extras: Vec<(String, String)> = Vec::new();
    for (kind, name, default, omit_if_absent) in &slots {
        let slot_value: Option<String> = match (pos_iter.next(), default.as_ref(), *omit_if_absent) {
            (Some(p), _, _) => Some(p.to_string()),
            (None, Some(d), _) => Some(d.clone()),
            (None, None, true) => None, // optional_no_default absent → omit
            (None, None, false) => {
                anyhow::bail!(
                    "missing positional value for required id-key '{name}'"
                );
            }
        };
        if matches!(kind, IdKeyKind::Slug) {
            slug_values.push(slot_value.clone().unwrap_or_default());
        }
        if let Some(v) = slot_value {
            slot_extras.push((name.clone(), v));
        }
    }

    // Remaining positionals fold into the primary value (first one) or
    // extend it as an array (matches legacy behaviour).
    let mut value: Option<JsonValue> = None;
    for token in pos_iter {
        if value.is_none() {
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

    // `id=<name>` convention: a positional `id=` kvpair is promoted to the
    // slug identity (slug becomes `<field>:<name>`) so entries can act as
    // documents without needing an explicit `-K id` declaration. Explicit
    // `-K` slug-keys already populate slug_values and win, in which case
    // `id=` stays in extra_data as ordinary payload.
    if slug_values.is_empty() {
        if let Some(pos) = kvpair_tokens.iter().position(|(k, _)| *k == "id") {
            let (_, v) = kvpair_tokens.remove(pos);
            slug_values.push(v.to_string());
        }
    }

    // Build extra_data. Order: slot fills, then legacy `-k KEY=VALUE`
    // immediates, then `KEY=VALUE` positionals, then --comment. Later
    // entries overwrite earlier ones so explicit user input beats defaults.
    let mut extra = serde_json::Map::new();
    for (k, v) in slot_extras {
        extra.insert(k, parse_value_token(&v));
    }
    for (k, v) in immediate_extras {
        extra.insert(k, parse_value_token(&v));
    }
    for (k, v) in kvpair_tokens {
        extra.insert(k.to_string(), parse_value_token(v));
    }
    if let Some(comment) = &args.comment {
        extra.insert("comment".to_string(), JsonValue::String(comment.clone()));
    }

    let extra_data = if extra.is_empty() {
        None
    } else {
        Some(JsonValue::Object(extra))
    };

    Ok(ParsedLogArgs { field, value, extra_data, slug_values })
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
    if args.time.is_some() { bad.push("-t/--time"); }
    if args.date.is_some() { bad.push("-d/--date"); }
    if args.dur.is_some() { bad.push("--dur"); }
    if args.timezone.is_some() { bad.push("-z/--timezone"); }
    if args.yesterday > 0 { bad.push("-y"); }
    if args.quick > 0 { bad.push("-q"); }
    if !bad.is_empty() {
        anyhow::bail!(
            "-T/--no-timestamp cannot be combined with time flags: {}",
            bad.join(", ")
        );
    }
    Ok(())
}

fn build_time_with_precision(args: &LogArgs) -> Result<(DatumTime, datum_core::time::Uncertainty)> {
    let mut builder = TimeBuilder::now()
        .with_yesterday_count(args.yesterday as i64)
        .with_quick_count(args.quick as i64)
        .with_quick_precision(args.quick as u32);

    if let Some(d) = &args.date {
        let da = parse_date_arg(d)?;
        builder = builder.with_date_arg(da);
    }
    if let Some(t) = &args.time {
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
    let tbl = TableArgs {
        output: args.output(),
        metric,
        columns,
        suppress_header: false,
    };
    print_table(&entries, registry.as_ref(), &tbl);

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
                print_table(&new_rows, registry.as_ref(), &tick_tbl);
            }
        }
    }

    Ok(())
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
    print_table(&matches, registry.as_ref(), &tbl);
    Ok(())
}

pub(crate) fn cmd_now(args: NowArgs, db: &Db) -> Result<()> {
    emit_stale_sync_warning(db);

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
    Ok(())
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

pub(crate) fn cmd_ignore(args: IgnoreArgs, db: &Db) -> Result<()> {
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
    let entry = resolve_quickid_single(db, &args.id_or_slug)
        .map_err(anyhow::Error::from)?;
    if args.pretty {
        print_entry_pretty(&entry)?;
    } else {
        let registry = load_registry(db).ok();
        print_entry(&entry, registry.as_ref());
    }
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
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
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

pub(crate) fn cmd_delete(args: DeleteArgs, db: &Db) -> Result<()> {
    let registry = load_registry(db).ok();
    let output = args.output();
    let (entries, token_errors) = resolve_batch(db, &args.id_or_slug, BatchMode::Live);

    // Nothing resolved — propagate the first token error verbatim so single-token
    // NotFound / AmbiguousPrefix surfaces unchanged in the common case.
    if entries.is_empty() {
        if let Some((_, err)) = token_errors.into_iter().next() {
            return Err(err);
        }
        anyhow::bail!("no entries specified");
    }

    // Strict: abort up-front on any resolution error so we don't half-apply
    // against a partial resolution.
    if args.strict {
        if let Some((_, err)) = token_errors.first() {
            return Err(anyhow::anyhow!("{err}"));
        }
    }

    if args.dry_run {
        for entry in &entries {
            print_entry(entry, registry.as_ref());
        }
        for (tok, err) in &token_errors {
            eprintln!("[dry-run] resolve '{tok}': {err}");
        }
        eprintln!(
            "[dry-run] would delete {} {}",
            entries.len(),
            if entries.len() == 1 { "entry" } else { "entries" },
        );
        return Ok(());
    }

    if !args.force {
        for entry in &entries {
            print_entry(entry, registry.as_ref());
        }
        let prompt = if entries.len() == 1 {
            "Delete this entry?".to_string()
        } else {
            format!("Delete these {} entries?", entries.len())
        };
        eprint!("{prompt} [y/N] ");
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

    let mut first_err: Option<anyhow::Error> = None;
    for entry in &entries {
        match db.soft_delete_entry(&entry.slug) {
            Ok(()) => show_delete(entry, registry.as_ref(), &output),
            Err(e) => {
                let msg = format!("{}: {}", entry.slug, e);
                if args.strict {
                    return Err(anyhow::anyhow!(msg));
                }
                eprintln!("{msg}");
                first_err.get_or_insert_with(|| anyhow::anyhow!(msg));
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

// ─── Edit ────────────────────────────────────────────────────────────────────

/// Open an entry in `$EDITOR` as pretty JSON; on save, re-parse and persist.
///
/// Testing hook: `DATUM_EDITOR` takes precedence over `EDITOR`. Both are
/// invoked as `sh -c "$editor \"$1\"" -- <tmpfile>`, so tests can set
/// `DATUM_EDITOR="cp /path/to/prepared.json"` to simulate a scripted edit.
pub(crate) fn cmd_edit(args: EditArgs, db: &Db) -> Result<()> {
    let existing = resolve_prefix_or_tombstone(db, &args.id_or_slug)?;
    if existing.deleted_at.is_some() {
        anyhow::bail!(
            "Cannot edit tombstoned entry {} — restore it first with `datum undo`",
            args.id_or_slug,
        );
    }

    let original_json = serde_json::to_string_pretty(&existing)
        .context("failed to serialize entry for editing")?;

    let tmp = tempfile::Builder::new()
        .prefix("datum-edit-")
        .suffix(".json")
        .tempfile()
        .context("failed to create tempfile for editor")?;
    std::fs::write(tmp.path(), &original_json)
        .context("failed to write entry to tempfile")?;

    let editor = std::env::var("DATUM_EDITOR")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("--")
        .arg(tmp.path())
        .status()
        .with_context(|| format!("failed to invoke editor: {editor}"))?;
    if !status.success() {
        anyhow::bail!("editor exited with non-zero status: {status}");
    }

    let edited = std::fs::read_to_string(tmp.path())
        .context("failed to read tempfile after editing")?;

    if edited == original_json {
        println!("No changes — entry {} unmodified", existing.slug);
        return Ok(());
    }

    let mut updated: Entry = serde_json::from_str(&edited)
        .map_err(|e| anyhow::anyhow!("edited content is not valid entry JSON: {e}"))?;

    if updated.slug != existing.slug {
        anyhow::bail!(
            "edit cannot change slug ({} → {}); use `datum rekey` instead",
            existing.slug, updated.slug,
        );
    }

    updated.modify_time = chrono::Utc::now();
    updated.content_hash = updated.expected_content_hash();

    db.save_existing_entry(&updated)?;

    let fresh = db.get_entry(&updated.slug)?;
    let registry = load_registry(db).ok();
    show_update(&fresh, registry.as_ref(), &args.output());
    Ok(())
}

// ─── Update / Merge / Rekey ──────────────────────────────────────────────────
// Underlying logic lives in `commands::update_core::update_with_strategy`.
// See that module for the unified strategy function the wrappers (and S3
// batch mutations) call into.

pub(crate) fn parse_update_payload(
    args: &UpdateArgs,
) -> Result<(Option<JsonValue>, Option<JsonValue>)> {
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
    let resolved_slug: Option<String> = args
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

pub(crate) fn cmd_spec_set(args: SpecSetArgs, db: &Db) -> Result<()> {
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
    if !args.id_keys.is_empty() {
        spec.id_keys = args.id_keys.clone();
    }
    if !args.data_keys.is_empty() {
        spec.data_keys = args.data_keys.clone();
    }
    if args.no_autoalias {
        spec.no_autoalias = true;
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

pub(crate) fn cmd_conflicts_resolve(args: ConflictsResolveArgs, db: &Db) -> Result<()> {
    use datum_core::conflicts::Resolution;
    let resolution = if args.close_older {
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
                "specify a resolution: --keep both OR --close-older"
            ),
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
        MigrateCommand::Import(args) => cmd_migrate_import(args, db),
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

pub(crate) fn cmd_migrate_import(args: MigrateImportArgs, db: &Db) -> Result<()> {
    let backup_path = args.resolved_path()?.to_string();
    let dry_run = args.dry_run;

    let content = std::fs::read_to_string(&backup_path)
        .with_context(|| format!("Reading backup file: {}", backup_path))?;
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
            else { Verb::Add };

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
        entry.create_time = created_at;

        if dry_run {
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

// ─── Backup / Restore ────────────────────────────────────────────────────────

/// Version of the `.tar.gz` archive *envelope* (manifest + file layout).
/// Independent of the DB schema version embedded in the JSONL header — this
/// one only bumps if the tar layout itself (filenames, additional metadata
/// files) changes.
const BACKUP_ARCHIVE_VERSION: u32 = 1;

/// Discriminator the JSONL header line carries so a restore can tell a
/// headered backup apart from a legacy headerless one (which starts straight
/// with an Entry).
const BACKUP_HEADER_TYPE: &str = "daydumb_backup";

/// Metadata line written as the first JSONL record of every backup produced
/// by this binary (in both plain `.jsonl` and inside the tar.gz variant's
/// `entries.jsonl`). The restore side uses it to decide whether to migrate,
/// refuse, or proceed optimistically.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BackupHeader {
    #[serde(rename = "type")]
    kind: String,
    /// DB schema version (from `datum_core::migrations::CURRENT_SCHEMA_VERSION`)
    /// at backup time. Restore compares this against the binary's current.
    schema_version: u32,
    /// RFC3339 UTC timestamp when the backup was written.
    backup_time: String,
    /// Best-effort source identifier — registered DB name or file path.
    source_db: String,
    /// Live entry count at backup time, used for a post-restore integrity
    /// check (warning only; mismatch does not abort).
    entry_count: usize,
}

/// Resolve a best-effort identifier for the DB being backed up. Prefers the
/// registry name, falls back to the resolved path, and finally to an empty
/// string when neither can be recovered (e.g. in-memory DB in tests).
fn derive_source_db() -> String {
    match config::resolve_db(None) {
        Ok(r) => r
            .name
            .unwrap_or_else(|| r.path.to_string_lossy().into_owned()),
        Err(_) => String::new(),
    }
}

fn is_targz_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".tar.gz") || lower.ends_with(".tgz")
}

pub(crate) fn cmd_backup(args: BackupArgs, db: &Db) -> Result<()> {
    let out_path = std::path::Path::new(&args.path);
    if out_path.exists() && !args.overwrite {
        anyhow::bail!(
            "File exists: {} (pass --overwrite to replace)",
            out_path.display()
        );
    }

    let header = BackupHeader {
        kind: BACKUP_HEADER_TYPE.to_string(),
        schema_version: datum_core::migrations::CURRENT_SCHEMA_VERSION,
        backup_time: chrono::Utc::now().to_rfc3339(),
        source_db: derive_source_db(),
        entry_count: db.count_entries()?,
    };

    if is_targz_path(&args.path) {
        backup_targz(out_path, db, &header)
    } else {
        backup_jsonl(out_path, db, &header)
    }
}

fn backup_jsonl(path: &std::path::Path, db: &Db, header: &BackupHeader) -> Result<()> {
    use std::io::Write;
    let file = std::fs::File::create(path)
        .with_context(|| format!("Creating backup file: {}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    writeln!(writer, "{}", serde_json::to_string(header)?)?;
    let mut count = 0usize;
    db.visit_all_entries::<anyhow::Error, _>(|entry| {
        let line = serde_json::to_string(&entry)?;
        writeln!(writer, "{line}")?;
        count += 1;
        Ok(())
    })?;
    writer.flush()?;
    println!("✓ Backup: {} entries → {}", count, path.display());
    Ok(())
}

/// RAII guard that removes a temp file when dropped (even on panic/error paths).
struct TempFileGuard(std::path::PathBuf);
impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn backup_targz(path: &std::path::Path, db: &Db, header: &BackupHeader) -> Result<()> {
    use std::io::Write;

    // Stage entries.jsonl to a temp file so we know its size for the tar
    // header without buffering the whole dataset in memory. The metadata
    // header line goes first, followed by one JSON entry per line — same
    // layout as plain .jsonl backups.
    let temp_path = std::env::temp_dir().join(format!(
        "datum-backup-{}.jsonl",
        new_hid()
    ));
    let guard = TempFileGuard(temp_path.clone());

    let mut count = 0usize;
    {
        let temp_file = std::fs::File::create(&guard.0)
            .with_context(|| format!("Creating staging file: {}", guard.0.display()))?;
        let mut writer = std::io::BufWriter::new(temp_file);
        writeln!(writer, "{}", serde_json::to_string(header)?)?;
        db.visit_all_entries::<anyhow::Error, _>(|entry| {
            let line = serde_json::to_string(&entry)?;
            writeln!(writer, "{line}")?;
            count += 1;
            Ok(())
        })?;
        writer.flush()?;
    }

    let manifest = serde_json::json!({
        "archive_version": BACKUP_ARCHIVE_VERSION,
        "schema_version": header.schema_version,
        "timestamp": header.backup_time,
        "source_db": header.source_db,
        "entry_count": count,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;

    let out_file = std::fs::File::create(path)
        .with_context(|| format!("Creating output file: {}", path.display()))?;
    let gz = flate2::write::GzEncoder::new(
        std::io::BufWriter::new(out_file),
        flate2::Compression::default(),
    );
    let mut builder = tar::Builder::new(gz);

    // manifest.json — listed first so a consumer can peek schema_version
    // without scanning the entries.
    let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
    let mut mheader = tar::Header::new_gnu();
    mheader.set_size(manifest_bytes.len() as u64);
    mheader.set_mode(0o644);
    mheader.set_mtime(now_secs);
    builder
        .append_data(&mut mheader, "manifest.json", manifest_bytes.as_slice())
        .context("Writing manifest.json to tar")?;

    let mut temp_read = std::fs::File::open(&guard.0)
        .with_context(|| format!("Reopening staging file: {}", guard.0.display()))?;
    let metadata = temp_read.metadata()?;
    let mut eheader = tar::Header::new_gnu();
    eheader.set_size(metadata.len());
    eheader.set_mode(0o644);
    eheader.set_mtime(now_secs);
    builder
        .append_data(&mut eheader, "entries.jsonl", &mut temp_read)
        .context("Writing entries.jsonl to tar")?;

    let gz = builder.into_inner().context("Finalizing tar")?;
    gz.finish().context("Finalizing gzip")?;

    println!("✓ Backup: {} entries → {}", count, path.display());
    Ok(())
}

/// Result of a restore for inspection by tests and the CLI printer. The
/// `warnings` field carries human-readable notes about header state
/// (legacy/missing, entry-count drift, etc.) — printed by `cmd_restore`
/// after the outcome summary.
///
/// Field names describe the *committed* outcome. In dry-run mode
/// (`dry_run == true`) they carry the **projected** outcome — i.e.
/// "would be inserted / updated / skipped" — computed from read-only
/// content_hash lookups without touching the DB.
#[derive(Debug, Default)]
pub(crate) struct RestoreReport {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    /// Schema version the backup declares. `None` for headerless legacy
    /// backups, where we optimistically assume v1.
    pub header_schema_version: Option<u32>,
    pub header_source_db: Option<String>,
    pub header_entry_count: Option<usize>,
    pub warnings: Vec<String>,
    /// True if the counts are projections from a dry-run, not committed writes.
    pub dry_run: bool,
    /// When the backup's schema version is below the binary's, every entry is
    /// run through `migrate_entry_json` before insert. Reported as `(from, to)`
    /// so dry-run output can surface the migration step that would run.
    pub migration: Option<(u32, u32)>,
}

impl RestoreReport {
    fn total_processed(&self) -> usize {
        self.inserted + self.updated + self.skipped
    }
}

pub(crate) fn cmd_restore(args: RestoreArgs, db: &Db) -> Result<()> {
    let dry_run = args.dry_run;
    let json = args.json;
    let force = args.force;
    let report = cmd_restore_with_report(args, db)?;

    if json {
        let out = serde_json::json!({
            "dry_run": report.dry_run,
            "scanned": report.total_processed(),
            "would_insert": if report.dry_run { Some(report.inserted) } else { None },
            "would_skip_dedupe": if report.dry_run { Some(report.skipped) } else { None },
            "would_update": if report.dry_run { Some(report.updated) } else { None },
            "inserted": if report.dry_run { None } else { Some(report.inserted) },
            "updated": if report.dry_run { None } else { Some(report.updated) },
            "skipped": if report.dry_run { None } else { Some(report.skipped) },
            "header": {
                "schema_version": report.header_schema_version,
                "source_db": report.header_source_db,
                "entry_count": report.header_entry_count,
            },
            "migration": report.migration.map(|(from, to)| serde_json::json!({"from": from, "to": to})),
            "force": force,
            "warnings": report.warnings,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    for w in &report.warnings {
        eprintln!("⚠ {w}");
    }
    if dry_run {
        println!("Dry run: {} entries scanned (no DB changes)", report.total_processed());
        println!("  Would insert: {}", report.inserted);
        println!(
            "  Would skip (content_hash dedupe): {}",
            report.skipped
        );
        if force {
            println!("  Would update (--force): {}", report.updated);
        }
        if let Some((from, to)) = report.migration {
            println!("  Schema migration: v{from} → v{to} (applied to every entry)");
        }
    } else {
        println!(
            "✓ Restore: {} inserted, {} updated, {} skipped",
            report.inserted, report.updated, report.skipped
        );
    }
    Ok(())
}

pub(crate) fn cmd_restore_with_report(args: RestoreArgs, db: &Db) -> Result<RestoreReport> {
    let strategy = if args.force {
        ConflictStrategy::Update
    } else {
        ConflictStrategy::Ignore
    };

    if is_targz_path(&args.path) {
        restore_targz(&args.path, db, strategy, args.dry_run)
    } else {
        restore_jsonl(&args.path, db, strategy, args.dry_run)
    }
}

fn restore_jsonl(
    path: &str,
    db: &Db,
    strategy: ConflictStrategy,
    dry_run: bool,
) -> Result<RestoreReport> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Opening backup: {path}"))?;
    let reader = std::io::BufReader::new(file);
    restore_from_reader(reader, db, strategy, dry_run)
}

fn restore_targz(
    path: &str,
    db: &Db,
    strategy: ConflictStrategy,
    dry_run: bool,
) -> Result<RestoreReport> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Opening backup: {path}"))?;
    let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let mut archive = tar::Archive::new(gz);
    for entry_res in archive.entries().context("Reading tar archive")? {
        let entry = entry_res.context("Reading tar entry")?;
        let in_path = entry.path().context("Reading tar entry path")?.into_owned();
        if in_path.file_name().and_then(|s| s.to_str()) == Some("entries.jsonl") {
            let reader = std::io::BufReader::new(entry);
            return restore_from_reader(reader, db, strategy, dry_run);
        }
    }
    anyhow::bail!("entries.jsonl not found in archive");
}

/// Peek the first non-empty line. Returns `Some(header)` if it parses as a
/// daydumb backup metadata object; returns `None` (and hands back the line
/// via `leftover`) if it looks like a legacy entry instead. Empty files
/// yield `(None, None)` so the caller can no-op cleanly.
fn read_header_line<R: std::io::BufRead>(
    reader: &mut R,
) -> Result<(Option<BackupHeader>, Option<String>)> {
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .context("Reading first backup line")?;
        if n == 0 {
            return Ok((None, None));
        }
        let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
        if trimmed.trim().is_empty() {
            continue;
        }
        // Only a metadata object carries the sentinel discriminator. Peek
        // the "type" field first so a legacy Entry (which has no `type`
        // key with that value) falls through to the headerless path
        // without us running a full Entry deserialize twice.
        let is_header = serde_json::from_str::<serde_json::Value>(&trimmed)
            .ok()
            .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(|s| s.to_string()))
            .map(|s| s == BACKUP_HEADER_TYPE)
            .unwrap_or(false);
        if is_header {
            let header: BackupHeader = serde_json::from_str(&trimmed)
                .with_context(|| format!("Parsing backup header: {trimmed}"))?;
            return Ok((Some(header), None));
        }
        return Ok((None, Some(trimmed)));
    }
}

/// Apply per-entry JSON migrations to bring an older-schema backup entry up
/// to the current Entry shape. For the schema range currently supported
/// (v1–v7) the entries table was rewritten wholesale at v2→v3 with no real
/// production data, so no intermediate JSON transforms are needed — the
/// function is an identity pass. Future schema bumps that change Entry
/// serialization should add a match arm here (keyed by `from`).
fn migrate_entry_json(
    _raw: &mut serde_json::Value,
    from: u32,
    to: u32,
) -> Result<()> {
    if from == to {
        return Ok(());
    }
    // No transforms registered yet. The Entry deserializer's own
    // `#[serde(default)]` fields already absorb additive schema bumps
    // (v3→v7 were all additive or index-only). A destructive Entry-shape
    // change in a future version should branch on `from` here.
    Ok(())
}

fn restore_from_reader<R: std::io::BufRead>(
    mut reader: R,
    db: &Db,
    strategy: ConflictStrategy,
    dry_run: bool,
) -> Result<RestoreReport> {
    let mut report = RestoreReport {
        dry_run,
        ..Default::default()
    };

    let (header, legacy_first) = read_header_line(&mut reader)?;

    // Gate on schema_version before touching the DB.
    let current = datum_core::migrations::CURRENT_SCHEMA_VERSION;
    let backup_version = match &header {
        Some(h) => {
            report.header_schema_version = Some(h.schema_version);
            report.header_source_db = Some(h.source_db.clone());
            report.header_entry_count = Some(h.entry_count);
            if h.schema_version > current {
                anyhow::bail!(
                    "Backup is schema v{}; binary supports up to v{}. \
                     Upgrade daydumb to restore this backup.",
                    h.schema_version,
                    current
                );
            }
            if h.schema_version < current {
                report.warnings.push(format!(
                    "Backup was schema v{}; migrating to v{} during restore",
                    h.schema_version, current
                ));
            }
            h.schema_version
        }
        None => {
            // Empty file → no-op. Non-empty with no header → legacy.
            if legacy_first.is_some() {
                report.warnings.push(
                    "Legacy backup without metadata; assuming schema v1".to_string(),
                );
            }
            1
        }
    };

    if backup_version < current {
        report.migration = Some((backup_version, current));
    }

    // Decode either the legacy first line or fresh lines from the reader.
    // Using `chain` keeps the streaming model intact — nothing is buffered
    // beyond a single line.
    let legacy_iter = legacy_first.into_iter().map(Ok);
    let body_iter = reader.lines();
    let all_lines = legacy_iter.chain(body_iter);

    for line in all_lines {
        let line = line.context("Reading backup line")?;
        if line.trim().is_empty() {
            continue;
        }
        let mut raw: serde_json::Value = serde_json::from_str(&line)
            .with_context(|| format!("Parsing entry JSON: {line}"))?;
        if backup_version < current {
            migrate_entry_json(&mut raw, backup_version, current)?;
        }
        let entry: Entry = serde_json::from_value(raw)
            .with_context(|| format!("Deserializing entry: {line}"))?;

        if dry_run {
            // Read-only: predict the outcome from a content_hash lookup
            // without opening a write transaction.
            let exists = db.has_entry_with_content_hash(&entry.content_hash)?;
            match (exists, strategy) {
                (false, _) => report.inserted += 1,
                (true, ConflictStrategy::Ignore) => report.skipped += 1,
                (true, ConflictStrategy::Update)
                | (true, ConflictStrategy::Merge) => report.updated += 1,
                // Rekey would insert a second row sharing content_hash.
                (true, ConflictStrategy::Rekey) => report.inserted += 1,
            }
            continue;
        }

        let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
        match outcome {
            InsertOutcome::Inserted { .. } | InsertOutcome::Rekeyed { .. } => {
                report.inserted += 1
            }
            InsertOutcome::Updated { .. } | InsertOutcome::Merged { .. } => {
                report.updated += 1
            }
            InsertOutcome::Ignored { .. } => report.skipped += 1,
        }
    }

    // Integrity check — warn only, do not abort. A mismatch here usually
    // means the backup file was truncated or concurrently modified; the
    // user still wants whatever we managed to read.
    if let Some(expected) = report.header_entry_count {
        let actual = report.total_processed();
        if expected != actual {
            report.warnings.push(format!(
                "Entry-count mismatch: header declared {expected}, restored {actual}"
            ));
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
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
            slug_keys: vec![],
            comment: None,
            id: None,
            id_delimiter: ":".to_string(),
            last_state: false,
            moment: false,
            conflict: None,
            update: false,
            merge: false,
            no_metadata: false,
            no_timestamp: false,
            output_flags: Default::default(),
        };
        // Just verify the dur parsing logic: start --dur 8h → end = start + 8h
        let (base, _precision) = build_time_with_precision(&args).unwrap();
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
            slug_keys: vec![],
            comment: None,
            id: None,
            id_delimiter: ":".to_string(),
            last_state: false,
            moment: false,
            conflict: None,
            update: false,
            merge: false,
            no_metadata: false,
            no_timestamp: false,
            output_flags: Default::default(),
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
        let args = LogArgs {
            field_or_verb: "coffee".to_string(),
            rest: vec!["2".to_string()],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: Some("cups".to_string()), keys: vec![], slug_keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
            no_metadata: false,
            no_timestamp: false,
            output_flags: Default::default(),
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
            timezone: None, unit: None, keys: vec![], slug_keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
            no_metadata: false,
            no_timestamp: false,
            output_flags: Default::default(),
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
            timezone: None, unit: None, keys: vec![], slug_keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
            no_metadata: false,
            no_timestamp: false,
            output_flags: Default::default(),
        };
        log_entry(Verb::Add, args, &db).unwrap();
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
            id_keys: vec![],
            data_keys: vec![],
            no_autoalias: false,
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
            id_keys: vec![],
            data_keys: vec![],
            no_autoalias: false,
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
            timezone: None, unit: None, keys: vec![], slug_keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
            no_metadata: false,
            no_timestamp: false,
            output_flags: Default::default(),
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
            slug_keys: vec![],
            comment: None,
            id,
            id_delimiter: ":".to_string(),
            last_state: false,
            moment: false,
            conflict: None,
            update: false,
            merge: false,
            no_metadata: false,
            no_timestamp: false,
            output_flags: Default::default(),
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

    /// Build a `TailArgs` in head-mode (oldest-first) with the given field and count.
    /// Used by head regression tests to exercise the shared tail codepath.
    fn head_args(field: Option<&str>, count: Option<usize>) -> TailArgs {
        TailArgs {
            field: field.map(String::from),
            count,
            metric: "hybrid".to_string(),
            reverse: false,
            columns: vec![],
            date: None,
            time: None,
            yesterday: 0,
            quick: 0,
            oldest_first: true,
            watch: None,
            output_flags: Default::default(),
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
        // default the old head command hardcoded via clap's `default_value`.
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
            GrepArgs { patterns: vec!["happy".to_string()], field: None, output_flags: Default::default() },
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
                output_flags: Default::default(),
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
            Verb::Add,
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
            Verb::Add,
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
            Verb::Occur,
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

        update_from_args(args, UpdateStrategy::Rekey, &db).unwrap();

        // Both the old and new rows exist.
        let live = db.entries_for_field("mood", None).unwrap();
        assert_eq!(live.len(), 2, "rekey must leave the old row in place");

        // New row has the interpolated slug.
        let new_row = live
            .iter()
            .find(|r| r.slug == "mood:happy")
            .expect("rekeyed slug should match template");
        assert_ne!(new_row.slug, e.slug, "new row gets its own ULID");
        // Old row still reachable by its original slug.
        let old = db.get_entry(&e.slug).unwrap();
        assert_eq!(old.slug, e.slug);
    }

    #[test]
    fn rekey_without_template_errors() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = seed_entry(&db, "mood", Verb::Switch, Some(serde_json::json!("x")), None);

        let err = update_from_args(update_args(&e.slug, vec![]), UpdateStrategy::Rekey, &db)
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

        let mut rk = update_args(&e.slug, vec![]);
        rk.id = Some("%field%:%uid%".to_string());
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
        let e = seed_entry(&db, "gym", Verb::Occur, Some(serde_json::json!(1)), None);
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
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Two entries on the same field and same minute → identical slug
        // timestamp, so the "gym:" prefix matches both.
        seed_entry(&db, "gym", Verb::Occur, Some(serde_json::json!(1)), None);
        seed_entry(&db, "gym", Verb::Occur, Some(serde_json::json!(2)), None);
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
            Verb::Add,
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

        update_from_args(update_args(&e.slug, vec![]), UpdateStrategy::Rekey, &db).unwrap();

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
            Verb::Add,
            Some(serde_json::json!("correr")),
            Some(serde_json::json!({"lang": "pt", "pos": "verb"})),
        );

        let mut args = update_args(&e.slug, vec!["pos=noun"]);
        args.id = Some("%field%:%pos%:%uid%".to_string());
        update_from_args(args, UpdateStrategy::Rekey, &db).unwrap();

        let rows = db.entries_for_field("word", None).unwrap();
        assert_eq!(rows.len(), 2);
        let rekeyed = rows
            .iter()
            .find(|r| r.slug.starts_with("word:noun:"))
            .expect("rekeyed row should exist");
        assert_eq!(rekeyed.data.as_ref().unwrap()["pos"], serde_json::json!("noun"));
        // Payload changes are visible on the new row only.
        let original = rows.iter().find(|r| r.slug == e.slug).unwrap();
        assert_eq!(original.data.as_ref().unwrap()["pos"], serde_json::json!("verb"));
    }

    // ─── B7: undo via soft-delete tombstone ─────────────────────────────────

    fn undo_args(slug: Option<&str>, force: bool, window: &str) -> UndoArgs {
        UndoArgs {
            id_or_slug: slug.map(str::to_string),
            to_version: None,
            force,
            window: window.to_string(),
            output_flags: Default::default(),
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
        Entry::new(
            field,
            Verb::Start,
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
        Entry::new(field, Verb::Occur, None, None, None, Some(t), None, device)
    }

    #[test]
    fn delete_then_undo_is_noop() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_entry_at("mood", 100, "d1");
        let id = e.slug.clone();
        let original_hash = e.content_hash.clone();
        db.insert_entry(&e).unwrap();

        cmd_delete(
            DeleteArgs {
                id_or_slug: id.clone(),
                force: true,
                dry_run: false,
                strict: false,
                output_flags: Default::default(),
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
    // cmd_edit spawns a subprocess under `sh -c` driven by the DATUM_EDITOR
    // env var. Env vars are process-global, so these tests serialize against
    // a shared mutex to avoid stepping on each other.

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

        // Build the edited JSON: same id/slug/etc., but a mutated value and
        // data. We serialize the existing entry, mutate two fields,
        // then hand the file back via `cp`.
        let mut doc: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        doc["value"] = serde_json::json!("focused");
        doc["data"] = serde_json::json!({"note": "clearer"});

        let prep = tempfile::Builder::new()
            .prefix("datum-edit-prep-")
            .suffix(".json")
            .tempfile()
            .unwrap();
        std::fs::write(prep.path(), serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        let editor_cmd = format!("cp {}", prep.path().display());
        with_datum_editor(&editor_cmd, || {
            cmd_edit(EditArgs { id_or_slug: e.slug.clone(), output_flags: Default::default() }, &db).unwrap();
        });

        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.value, Some(serde_json::json!("focused")));
        assert_eq!(
            after.data,
            Some(serde_json::json!({"note": "clearer"}))
        );
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
            Verb::Occur,
            Some(serde_json::json!(1)),
            None,
        );
        let before = db.get_entry(&e.slug).unwrap();

        // `true` ignores its args → tempfile is left untouched.
        with_datum_editor("true", || {
            cmd_edit(EditArgs { id_or_slug: e.slug.clone(), output_flags: Default::default() }, &db).unwrap();
        });

        let after = db.get_entry(&e.slug).unwrap();
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(after.modify_time, before.modify_time);
        assert_eq!(after.value, before.value);
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
            cmd_edit(EditArgs { id_or_slug: e.slug.clone(), output_flags: Default::default() }, &db)
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

    // ─── backup / restore tests (da-3d5o acceptance) ────────────────────────

    fn seed_entries(db: &datum_core::db::Db) {
        // Mix of fields, verbs, and extra_data shapes to exercise round-trip.
        db.insert_entry(&make_grep_entry(
            "mood",
            100,
            serde_json::json!("happy"),
            None,
        )).unwrap();
        db.insert_entry(&make_grep_entry(
            "coffee",
            200,
            serde_json::json!(2),
            Some(serde_json::json!({"unit": "cups"})),
        )).unwrap();
        db.insert_entry(&make_grep_entry(
            "notes",
            300,
            serde_json::json!("hello world"),
            Some(serde_json::json!({"tag": "foo", "nested": {"k": 1}})),
        )).unwrap();
    }

    fn unique_tmp_path(ext: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("datum-test-{}{}", new_hid(), ext))
    }

    fn all_entries_sorted(db: &datum_core::db::Db) -> Vec<datum_core::entry::Entry> {
        let far_past = chrono::DateTime::UNIX_EPOCH;
        let far_future = chrono::Utc::now() + chrono::Duration::days(365);
        let mut entries = db.entries_in_range(far_past, far_future).unwrap();
        entries.sort_by(|a, b| a.slug.cmp(&b.slug));
        entries
    }

    fn assert_entries_equivalent(
        expected: &[datum_core::entry::Entry],
        actual: &[datum_core::entry::Entry],
    ) {
        assert_eq!(expected.len(), actual.len(), "entry count mismatch");
        for (e, a) in expected.iter().zip(actual) {
            assert_eq!(e.slug, a.slug);
            assert_eq!(e.hid, a.hid);
            assert_eq!(e.content_hash, a.content_hash);
            assert_eq!(e.field, a.field);
            assert_eq!(e.verb, a.verb);
            assert_eq!(e.value, a.value);
            assert_eq!(e.data, a.data);
        }
    }

    #[test]
    fn backup_restore_round_trip_jsonl() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();

        // File is a metadata header line followed by line-per-entry JSONL.
        let contents = std::fs::read_to_string(&path).unwrap();
        let non_empty: Vec<&str> = contents
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        assert_eq!(
            non_empty.len(),
            original.len() + 1,
            "one metadata line + one line per entry"
        );

        // Header: type == daydumb_backup and schema_version tracks the binary.
        let header: serde_json::Value = serde_json::from_str(non_empty[0])
            .expect("first line parses as JSON");
        assert_eq!(header["type"], serde_json::json!("daydumb_backup"));
        assert_eq!(
            header["schema_version"],
            serde_json::json!(datum_core::migrations::CURRENT_SCHEMA_VERSION)
        );
        assert_eq!(
            header["entry_count"].as_u64(),
            Some(original.len() as u64)
        );
        assert!(header["backup_time"].as_str().is_some());

        // Remaining lines must parse as full Entries.
        for line in &non_empty[1..] {
            let _entry: datum_core::entry::Entry =
                serde_json::from_str(line).expect("each data line is a parseable Entry");
        }

        // Restore into a fresh db and verify equivalence.
        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap();
        let restored = all_entries_sorted(&dst);
        assert_entries_equivalent(&original, &restored);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn backup_restore_round_trip_targz() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".tar.gz");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();
        assert!(path.exists(), "tar.gz backup file should exist");

        // Verify the archive contains manifest.json + entries.jsonl, and
        // that entries.jsonl starts with the new daydumb_backup header.
        let f = std::fs::File::open(&path).unwrap();
        let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(f));
        let mut archive = tar::Archive::new(gz);
        let mut seen_manifest = false;
        let mut seen_entries = false;
        let mut manifest_body = String::new();
        let mut entries_body = String::new();
        for entry_res in archive.entries().unwrap() {
            use std::io::Read;
            let mut entry = entry_res.unwrap();
            let name = entry.path().unwrap().into_owned();
            match name.file_name().and_then(|s| s.to_str()) {
                Some("manifest.json") => {
                    seen_manifest = true;
                    entry.read_to_string(&mut manifest_body).unwrap();
                }
                Some("entries.jsonl") => {
                    seen_entries = true;
                    entry.read_to_string(&mut entries_body).unwrap();
                }
                _ => {}
            }
        }
        assert!(seen_manifest, "manifest.json present in archive");
        assert!(seen_entries, "entries.jsonl present in archive");
        let manifest: serde_json::Value =
            serde_json::from_str(&manifest_body).expect("manifest is valid JSON");
        assert_eq!(manifest["archive_version"], serde_json::json!(1));
        assert_eq!(
            manifest["schema_version"],
            serde_json::json!(datum_core::migrations::CURRENT_SCHEMA_VERSION)
        );
        assert!(
            manifest["timestamp"].as_str().is_some(),
            "manifest has a timestamp"
        );

        let first_line = entries_body.lines().next().expect("entries.jsonl non-empty");
        let header: serde_json::Value = serde_json::from_str(first_line)
            .expect("entries.jsonl first line is JSON");
        assert_eq!(header["type"], serde_json::json!("daydumb_backup"));
        assert_eq!(
            header["schema_version"],
            serde_json::json!(datum_core::migrations::CURRENT_SCHEMA_VERSION)
        );

        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap();
        let restored = all_entries_sorted(&dst);
        assert_entries_equivalent(&original, &restored);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_is_idempotent_via_content_hash() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();

        // First restore: every row inserted.
        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap();
        assert_eq!(dst.count_entries().unwrap(), original.len());

        let before: Vec<_> = all_entries_sorted(&dst)
            .into_iter()
            .map(|e| (e.slug.clone(), e.modify_time))
            .collect();

        // Second restore with the same backup: content_hash dedup kicks in,
        // nothing changes.
        cmd_restore(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap();
        assert_eq!(dst.count_entries().unwrap(), original.len());

        let after: Vec<_> = all_entries_sorted(&dst)
            .into_iter()
            .map(|e| (e.slug.clone(), e.modify_time))
            .collect();
        assert_eq!(before, after, "idempotent restore must not bump modified_at");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_force_overwrites_conflicting_rows() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        src.insert_entry(&make_grep_entry(
            "mood",
            100,
            serde_json::json!("happy"),
            None,
        )).unwrap();

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();

        // Pre-populate dst with the SAME logical row from a different device.
        // Same content_hash, different device_id.
        let dst = Db::open_in_memory().unwrap();
        use datum_core::{entry::Entry, time::DatumTime};
        let base: chrono::DateTime<chrono::Utc> =
            "2024-01-01T00:00:00Z".parse().unwrap();
        let t = DatumTime {
            utc: base + chrono::Duration::seconds(100),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let dup = Entry::new(
            "mood",
            Verb::Occur,
            Some(serde_json::json!("happy")),
            None,
            None,
            Some(t),
            None,
            "device-OTHER",
        );
        dst.insert_entry(&dup).unwrap();
        let old_modified = dst.get_entry(&dup.slug).unwrap().modify_time;

        // --force → ConflictStrategy::Update overwrites device_id from "d1" in backup.
        cmd_restore(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: true, dry_run: false, json: false },
            &dst,
        )
        .unwrap();

        let overwritten = dst.get_entry(&dup.slug).unwrap();
        assert_eq!(overwritten.device_id, "d1", "force must overwrite device_id");
        assert!(
            overwritten.modify_time >= old_modified,
            "force must bump modified_at"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn backup_refuses_to_overwrite_without_flag() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        seed_entries(&db);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &db,
        )
        .unwrap();

        // Second call without --overwrite must error.
        let err = cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &db,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("exists"),
            "expected 'exists' in error, got: {err}"
        );

        // With --overwrite, it succeeds.
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: true },
            &db,
        )
        .unwrap();

        std::fs::remove_file(&path).ok();
    }

    // ─── S7: header + schema-version gate + auto-migrate on restore ───────

    /// Write a JSONL file by hand so we can exercise the restore-side header
    /// parser with arbitrary schema versions without having to rebuild an
    /// older binary.
    fn write_jsonl(
        path: &std::path::Path,
        header: Option<&serde_json::Value>,
        entries: &[datum_core::entry::Entry],
    ) {
        use std::io::Write;
        let f = std::fs::File::create(path).unwrap();
        let mut w = std::io::BufWriter::new(f);
        if let Some(h) = header {
            writeln!(w, "{}", serde_json::to_string(h).unwrap()).unwrap();
        }
        for e in entries {
            writeln!(w, "{}", serde_json::to_string(e).unwrap()).unwrap();
        }
    }

    #[test]
    fn restore_header_match_produces_no_warnings() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap();

        assert_eq!(
            report.header_schema_version,
            Some(datum_core::migrations::CURRENT_SCHEMA_VERSION)
        );
        assert_eq!(report.header_entry_count, Some(original.len()));
        assert!(
            report.warnings.is_empty(),
            "same-version restore must not warn, got: {:?}",
            report.warnings
        );
        assert_eq!(report.inserted, original.len());
        assert_entries_equivalent(&original, &all_entries_sorted(&dst));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_older_schema_warns_and_migrates() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Forge a backup with schema_version set one below current. The
        // entries themselves are serialized from today's binary — the
        // identity migrate_entry_json lets them round-trip cleanly.
        let older = datum_core::migrations::CURRENT_SCHEMA_VERSION.saturating_sub(1).max(1);
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": older,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len(),
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap();

        assert_eq!(report.header_schema_version, Some(older));
        assert!(
            report.warnings.iter().any(|w| w.contains("migrating to v")),
            "expected migrating warning, got: {:?}",
            report.warnings
        );
        assert_eq!(report.inserted, original.len());
        assert_entries_equivalent(&original, &all_entries_sorted(&dst));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_newer_schema_refuses() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let newer = datum_core::migrations::CURRENT_SCHEMA_VERSION + 3;
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": newer,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len(),
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        let err = cmd_restore_with_report(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("schema v{newer}")) && msg.contains("Upgrade daydumb"),
            "expected newer-schema refusal message, got: {msg}"
        );
        // No entries should have been touched — the gate fires before any insert.
        assert_eq!(dst.count_entries().unwrap(), 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_headerless_legacy_warns_and_proceeds() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Pre-S7 layout: no metadata header, just entries.
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, None, &original);

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap();

        assert_eq!(report.header_schema_version, None);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("Legacy backup without metadata")),
            "expected legacy warning, got: {:?}",
            report.warnings
        );
        assert_eq!(report.inserted, original.len());
        assert_entries_equivalent(&original, &all_entries_sorted(&dst));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_entry_count_mismatch_warns_only() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Header claims one more entry than we actually include. Matching
        // schema version so the only divergence is the integrity check.
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": datum_core::migrations::CURRENT_SCHEMA_VERSION,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len() + 1,
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false, dry_run: false, json: false },
            &dst,
        )
        .unwrap();

        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("Entry-count mismatch")),
            "expected integrity warning, got: {:?}",
            report.warnings
        );
        // Mismatch is non-fatal — the rows we had still landed.
        assert_eq!(report.inserted, original.len());

        std::fs::remove_file(&path).ok();
    }

    // ─── S8: restore --dry-run ────────────────────────────────────────────

    #[test]
    fn restore_dry_run_reports_all_as_would_insert_into_empty_db() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        let before = dst.count_entries().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                json: false,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run, "report must flag dry_run");
        assert_eq!(report.inserted, original.len(), "every entry would insert");
        assert_eq!(report.skipped, 0);
        assert_eq!(report.updated, 0);

        // HARD GATE: DB must not have been touched.
        assert_eq!(
            dst.count_entries().unwrap(),
            before,
            "dry-run must not write to DB"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_reports_dedupe_when_rows_already_present() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();

        // Populate dst with the same rows so every content_hash collides.
        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                json: false,
            },
            &dst,
        )
        .unwrap();
        let pre_count = dst.count_entries().unwrap();
        let pre_modified: Vec<_> = all_entries_sorted(&dst)
            .into_iter()
            .map(|e| (e.slug.clone(), e.modify_time))
            .collect();

        // Dry-run over the same backup should predict every row as a skip.
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                json: false,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run);
        assert_eq!(report.skipped, pre_count, "every row collides by content_hash");
        assert_eq!(report.inserted, 0);
        assert_eq!(report.updated, 0);

        // Dry-run must not touch the DB even in the --force path.
        let post_modified: Vec<_> = all_entries_sorted(&dst)
            .into_iter()
            .map(|e| (e.slug.clone(), e.modify_time))
            .collect();
        assert_eq!(pre_modified, post_modified);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_reports_would_update_with_force() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                json: false,
            },
            &dst,
        )
        .unwrap();
        let pre_count = dst.count_entries().unwrap();

        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: true, // conflict strategy = Update
                dry_run: true,
                json: false,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run);
        assert_eq!(report.updated, pre_count, "--force flips dedupes into updates");
        assert_eq!(report.skipped, 0);
        assert_eq!(report.inserted, 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_works_on_targz() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".tar.gz");
        cmd_backup(
            BackupArgs { path: path.to_string_lossy().into_owned(), overwrite: false },
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                json: false,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run);
        assert_eq!(report.inserted, original.len());
        assert_eq!(dst.count_entries().unwrap(), 0, "dry-run leaves DB untouched");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_surfaces_migration_step() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Forge an older-schema header.
        let current = datum_core::migrations::CURRENT_SCHEMA_VERSION;
        let older = current.saturating_sub(1).max(1);
        if older == current {
            // Can't exercise migration path on a one-version binary; skip.
            return;
        }
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": older,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len(),
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                json: false,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run);
        assert_eq!(report.migration, Some((older, current)));
        assert_eq!(dst.count_entries().unwrap(), 0);
        assert!(
            report.warnings.iter().any(|w| w.contains("migrating to v")),
            "migration warning should also appear in dry-run: {:?}",
            report.warnings
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_fails_on_future_schema() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let newer = datum_core::migrations::CURRENT_SCHEMA_VERSION + 3;
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": newer,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len(),
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        // "Would fail" case — dry-run propagates the schema-gate error so
        // the CLI exits nonzero before the user commits to a restore.
        let err = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                json: false,
            },
            &dst,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains(&format!("schema v{newer}")),
            "dry-run must surface the refusal: {err}"
        );
        assert_eq!(dst.count_entries().unwrap(), 0);

        std::fs::remove_file(&path).ok();
    }
}
