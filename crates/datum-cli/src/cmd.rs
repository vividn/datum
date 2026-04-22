//! Command dispatch and argument → Entry conversion.

use anyhow::{Context, Result};
use chrono::{Local, TimeZone, Utc};
use datum_core::{
    db::{ConflictStrategy, Db, InsertOutcome, ListParams, RetimeFilter, TimeMetric},
    entry::{new_hid, Entry, Verb},
    field_spec::{self, Alias, FieldSpec, InterpolationContext},
    time::{parse_date_arg, parse_duration, parse_time_arg, DatumTime, TimeBuilder},
};
use regex::Regex;
use serde_json::Value as JsonValue;

use crate::{
    args::{
        AliasCommand, BackupArgs, CheckArgs, Cli, Command, ConfigCommand, ConfigShowArgs,
        DayviewArgs, DeleteArgs, EditArgs, GetArgs, GrepArgs, HeadArgs, HistoryArgs, IgnoreArgs,
        InfoArgs, LogArgs, MapArgs, MigrateCommand, MigrateImportArgs, NowArgs, RestoreArgs,
        RetimeArgs, SetupArgs, SpecCommand, SpecSetArgs, TailArgs, TrackArgs, UndoArgs,
        UpdateArgs, WhyArgs,
    },
    check::{collect_issues, render_human, render_json},
    config::{self, device_id},
    output::{
        print_entry, print_entry_pretty, print_now_view, print_table, show_create, show_delete,
        show_exists, show_rename, show_update, OutputArgs, TableArgs, TimeMetricCol,
    },
};
use datum_core::views::{parse_key_arg, Registry, ViewParams};

pub fn dispatch(cli: Cli, db: &Db) -> Result<()> {
    match cli.command {
        Command::Occur(args) => log_entry(Verb::Occur, args, db),
        Command::Start(args) => log_entry(Verb::Start, args, db),
        Command::Stop(args) => log_entry(Verb::Stop, args, db),
        Command::Switch(args) => log_entry(Verb::Switch, args, db),
        Command::Add(args) => log_entry(Verb::Add, args, db),
        Command::Tail(args) => cmd_tail(args, db),
        Command::Head(args) => cmd_head(args, db),
        Command::Now(args) => cmd_now(args, db),
        Command::Get(args) => cmd_get(args, db),
        Command::Delete(args) => cmd_delete(args, db),
        Command::Edit(args) => cmd_edit(args, db),
        Command::Update(args) => cmd_update(args, ConflictStrategy::Update, db),
        Command::Merge(args) => cmd_update(args, ConflictStrategy::Merge, db),
        Command::Rekey(args) => cmd_update(args, ConflictStrategy::Rekey, db),
        Command::Retime(args) => cmd_retime(args, db),
        Command::Undo(args) => cmd_undo(args, db),
        Command::History(args) => cmd_history(args, db),
        Command::Why(args) => cmd_why(args, db),
        Command::Spec(sub) => cmd_spec(sub, db),
        Command::Alias(sub) => cmd_alias(sub, db),
        Command::Track(args) => cmd_track(args, db),
        Command::Ignore(args) => cmd_ignore(args, db),
        Command::Migrate(sub) => cmd_migrate(sub, db),
        Command::Backup(args) => cmd_backup(args, db),
        Command::Restore(args) => cmd_restore(args, db),
        Command::Grep(args) => cmd_grep(args, db),
        Command::Check(args) => cmd_check(args, db),
        Command::Map(args) => cmd_map(args, db),
        Command::Reduce(args) => cmd_reduce(args, db),
        Command::Dayview(args) => cmd_dayview(args, db),
        Command::Info(args) => cmd_info(args, cli.db.as_deref(), db),
        Command::Setup(args) => cmd_setup(args, db),
        Command::Config(sub) => cmd_config(sub, cli.db.as_deref()),
        Command::Sync(sub) => crate::sync::dispatch(sub),
    }
}

fn load_registry(db: &Db) -> Result<datum_core::field_spec::SpecRegistry> {
    Ok(db.load_registry()?)
}

// ─── Map / Reduce ────────────────────────────────────────────────────────────

fn build_registry() -> Registry {
    datum_core::views::builtins()
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

// ─── Dayview ────────────────────────────────────────────────────────────────

fn cmd_dayview(args: DayviewArgs, db: &Db) -> Result<()> {
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
        // The view row's `id` carries the source entry's slug (its PK).
        let obj = serde_json::json!({
            "key": row.key,
            "value": row.value,
            "slug": row.id,
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

// ─── info / setup ────────────────────────────────────────────────────────────

fn cmd_info(args: InfoArgs, cli_db: Option<&str>, active_db: &Db) -> Result<()> {
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

fn cmd_setup(args: SetupArgs, db: &Db) -> Result<()> {
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

fn cmd_config(sub: ConfigCommand, cli_db: Option<&str>) -> Result<()> {
    match sub {
        ConfigCommand::Show(args) => cmd_config_show(args, cli_db),
    }
}

fn cmd_config_show(args: ConfigShowArgs, cli_db: Option<&str>) -> Result<()> {
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

fn log_entry(verb: Verb, args: LogArgs, db: &Db) -> Result<()> {
    let parsed = parse_log_args(&args)?;
    let ParsedLogArgs {
        field,
        value,
        mut extra_data,
        slug_values,
    } = parsed;
    let (base_time, base_precision) = build_time_with_precision(&args)?;
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

    // Persist the effective precision derived from the -q stack and any
    // explicit `±N` uncertainty. The end_time precision mirrors start's
    // since both times flow from the same parse.
    entry.start_time_precision = base_precision.seconds;
    entry.start_time_precision_unit = base_precision.unit;
    if entry.end_time.is_some() {
        entry.end_time_precision = base_precision.seconds;
        entry.end_time_precision_unit = base_precision.unit;
    }

    if args.no_metadata {
        entry.create_time = chrono::DateTime::UNIX_EPOCH;
        entry.modify_time = chrono::DateTime::UNIX_EPOCH;
        entry.device_id.clear();
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

    let strategy = args.conflict_strategy();
    let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
    let registry = load_registry(db).ok();
    let output = args.output();
    match &outcome {
        InsertOutcome::Inserted { slug } => {
            if matches!(verb, Verb::Occur | Verb::Start) {
                db.ensure_tracked(&entry.field)?;
            }
            // The stored slug may differ from entry.slug if a collision
            // forced a suffixed retry — fetch the canonical row so the
            // display reflects what actually hit the DB.
            let fresh = db.get_entry(slug).unwrap_or_else(|_| entry.clone());
            show_create(&fresh, registry.as_ref(), &output);
        }
        InsertOutcome::Updated { slug } => {
            let fresh = db.get_entry(slug).unwrap_or_else(|_| entry.clone());
            show_update(&fresh, registry.as_ref(), &output);
        }
        InsertOutcome::Merged { slug } => {
            let fresh = db.get_entry(slug).unwrap_or_else(|_| entry.clone());
            show_update(&fresh, registry.as_ref(), &output);
        }
        InsertOutcome::Rekeyed { new_slug, old_slug } => {
            show_rename(old_slug, new_slug, &output);
            if let Ok(fresh) = db.get_entry(new_slug) {
                show_create(&fresh, registry.as_ref(), &output);
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

/// Build the log entry's base time together with its effective precision
/// (implicit from `-q*` stacks or explicit from a `±N` uncertainty suffix).
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

fn cmd_tail(args: TailArgs, db: &Db) -> Result<()> {
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
    };
    print_table(&entries, registry.as_ref(), &tbl);
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
            let ta = a.start_time.as_ref().map(|t| t.utc).unwrap_or(a.create_time);
            let tb = b.start_time.as_ref().map(|t| t.utc).unwrap_or(b.create_time);
            ta.cmp(&tb)
        });
        all.truncate(args.count);
        all
    };
    let registry = load_registry(db).ok();
    let tbl = TableArgs {
        output: args.output(),
        metric: TimeMetricCol::Hybrid,
        columns: vec![],
    };
    print_table(&entries, registry.as_ref(), &tbl);
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

    let registry = load_registry(db).ok();
    let tbl = TableArgs {
        output: args.output(),
        metric: TimeMetricCol::Hybrid,
        columns: vec![],
    };
    print_table(&matches, registry.as_ref(), &tbl);
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

/// Resolve `input` to an entry via C3 prefix matching, preserving the
/// "Tombstoned" signal so update/edit can produce their tailored error.
///
/// `resolve_prefix` filters out soft-deleted rows, so a prefix of a tombstone
/// comes back as `NotFound`. When that happens we retry with an exact
/// id/slug lookup that includes tombstones, then one more attempt that
/// scans tombstones by hid/slug prefix — so the undo / history / why
/// commands still work on a soft-deleted row's short hid (e.g. `5arfa`).
fn resolve_prefix_or_tombstone(db: &Db, input: &str) -> Result<Entry> {
    match db.resolve_prefix(input) {
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

fn cmd_get(args: GetArgs, db: &Db) -> Result<()> {
    let entry = db.resolve_prefix(&args.id_or_slug)
        .map_err(anyhow::Error::from)?;
    if args.pretty {
        print_entry_pretty(&entry)?;
    } else {
        let registry = load_registry(db).ok();
        print_entry(&entry, registry.as_ref());
    }
    Ok(())
}

fn cmd_delete(args: DeleteArgs, db: &Db) -> Result<()> {
    let registry = load_registry(db).ok();
    let entry = db
        .resolve_prefix(&args.id_or_slug)
        .map_err(anyhow::Error::from)?;
    if !args.force {
        print_entry(&entry, registry.as_ref());
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
    db.soft_delete_entry(&entry.slug)
        .map_err(anyhow::Error::from)?;
    show_delete(&entry, registry.as_ref(), &args.output());
    Ok(())
}

// ─── Edit ────────────────────────────────────────────────────────────────────

/// Open an entry in `$EDITOR` as pretty JSON; on save, re-parse and persist.
///
/// Testing hook: `DATUM_EDITOR` takes precedence over `EDITOR`. Both are
/// invoked as `sh -c "$editor \"$1\"" -- <tmpfile>`, so tests can set
/// `DATUM_EDITOR="cp /path/to/prepared.json"` to simulate a scripted edit.
fn cmd_edit(args: EditArgs, db: &Db) -> Result<()> {
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

fn cmd_update(args: UpdateArgs, default_strategy: ConflictStrategy, db: &Db) -> Result<()> {
    let strategy = args.strategy.map(Into::into).unwrap_or(default_strategy);

    // C3: accept unique prefixes, but preserve the "Cannot X tombstoned entry"
    // error when a user types a full id/slug of a soft-deleted row.
    let existing = resolve_prefix_or_tombstone(db, &args.id_or_slug)?;
    if existing.deleted_at.is_some() {
        anyhow::bail!(
            "Cannot {} tombstoned entry {} — restore it first with `datum undelete`",
            strategy_verb(strategy),
            args.id_or_slug,
        );
    }

    let (payload_value, payload_extra) = parse_update_payload(&args)?;
    let output = args.output();

    match strategy {
        ConflictStrategy::Update => apply_mutation(
            existing,
            payload_value,
            payload_extra,
            ExtraMergeMode::Shallow,
            db,
            &output,
        ),
        ConflictStrategy::Merge => apply_mutation(
            existing,
            payload_value,
            payload_extra,
            ExtraMergeMode::Deep,
            db,
            &output,
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
    output: &OutputArgs,
) -> Result<()> {
    let mut updated = existing;
    updated.data = combine_extra(updated.data, payload_extra, mode);
    if let Some(v) = payload_value {
        updated.value = Some(v);
    }
    updated.content_hash = updated.expected_content_hash();

    db.save_existing_entry(&updated)?;

    // Fetch the persisted row so the printed confirmation reflects the
    // db-stamped `modified_at`.
    let fresh = db.get_entry(&updated.slug)?;
    let registry = load_registry(db).ok();
    let _ = mode; // shallow vs deep is cosmetic to the user; output is unified UPDATE.
    show_update(&fresh, registry.as_ref(), output);
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
    new_entry.data =
        combine_extra(new_entry.data, payload_extra, ExtraMergeMode::Shallow);
    if let Some(v) = payload_value {
        new_entry.value = Some(v);
    }

    // Fresh hid + timestamps. Slug will be re-interpolated below.
    new_entry.hid = new_hid();
    let now = chrono::Utc::now();
    new_entry.create_time = now;
    new_entry.modify_time = now;
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
    let registry = load_registry(db).ok();
    match outcome {
        InsertOutcome::Inserted { .. } | InsertOutcome::Rekeyed { .. } => {
            show_rename(&existing.slug, &new_entry.slug, &args.output());
            show_create(&new_entry, registry.as_ref(), &args.output());
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

// ─── Retime ──────────────────────────────────────────────────────────────────

fn cmd_retime(args: RetimeArgs, db: &Db) -> Result<()> {
    if args.slug.is_none() && args.field.is_none() && args.after.is_none() && args.before.is_none()
    {
        anyhow::bail!(
            "retime requires at least one selector: <slug>, --field, --after, or --before"
        );
    }

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
        slug_or_hid: args.slug.as_deref(),
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

fn cmd_undo(args: UndoArgs, db: &Db) -> Result<()> {
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

fn cmd_history(args: HistoryArgs, db: &Db) -> Result<()> {
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

fn cmd_why(args: WhyArgs, db: &Db) -> Result<()> {
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

fn cmd_migrate(cmd: MigrateCommand, db: &Db) -> Result<()> {
    match cmd {
        MigrateCommand::Run => cmd_migrate_run(db),
        MigrateCommand::Status => cmd_migrate_status(db),
        MigrateCommand::Edit => cmd_migrate_edit(),
        MigrateCommand::Import(args) => cmd_migrate_import(args, db),
    }
}

fn cmd_migrate_run(db: &Db) -> Result<()> {
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

fn cmd_migrate_status(db: &Db) -> Result<()> {
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

fn cmd_migrate_edit() -> Result<()> {
    anyhow::bail!(
        "`datum migrate edit` (per-entry data migration) is not yet implemented; \
         tracked in bead da-glnr"
    )
}

fn cmd_migrate_import(args: MigrateImportArgs, db: &Db) -> Result<()> {
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

const BACKUP_SCHEMA_VERSION: u32 = 1;

fn is_targz_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".tar.gz") || lower.ends_with(".tgz")
}

fn cmd_backup(args: BackupArgs, db: &Db) -> Result<()> {
    let out_path = std::path::Path::new(&args.path);
    if out_path.exists() && !args.overwrite {
        anyhow::bail!(
            "File exists: {} (pass --overwrite to replace)",
            out_path.display()
        );
    }

    if is_targz_path(&args.path) {
        backup_targz(out_path, db)
    } else {
        backup_jsonl(out_path, db)
    }
}

fn backup_jsonl(path: &std::path::Path, db: &Db) -> Result<()> {
    use std::io::Write;
    let file = std::fs::File::create(path)
        .with_context(|| format!("Creating backup file: {}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);
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

fn backup_targz(path: &std::path::Path, db: &Db) -> Result<()> {
    use std::io::Write;

    // Stage entries.jsonl to a temp file so we know its size for the tar
    // header without buffering the whole dataset in memory.
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
        db.visit_all_entries::<anyhow::Error, _>(|entry| {
            let line = serde_json::to_string(&entry)?;
            writeln!(writer, "{line}")?;
            count += 1;
            Ok(())
        })?;
        writer.flush()?;
    }

    let manifest = serde_json::json!({
        "schema_version": BACKUP_SCHEMA_VERSION,
        "timestamp": chrono::Utc::now().to_rfc3339(),
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

fn cmd_restore(args: RestoreArgs, db: &Db) -> Result<()> {
    let strategy = if args.force {
        ConflictStrategy::Update
    } else {
        ConflictStrategy::Ignore
    };

    if is_targz_path(&args.path) {
        restore_targz(&args.path, db, strategy)
    } else {
        restore_jsonl(&args.path, db, strategy)
    }
}

fn restore_jsonl(path: &str, db: &Db, strategy: ConflictStrategy) -> Result<()> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Opening backup: {path}"))?;
    let reader = std::io::BufReader::new(file);
    restore_from_reader(reader, db, strategy)
}

fn restore_targz(path: &str, db: &Db, strategy: ConflictStrategy) -> Result<()> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Opening backup: {path}"))?;
    let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let mut archive = tar::Archive::new(gz);
    for entry_res in archive.entries().context("Reading tar archive")? {
        let entry = entry_res.context("Reading tar entry")?;
        let in_path = entry.path().context("Reading tar entry path")?.into_owned();
        if in_path.file_name().and_then(|s| s.to_str()) == Some("entries.jsonl") {
            let reader = std::io::BufReader::new(entry);
            return restore_from_reader(reader, db, strategy);
        }
    }
    anyhow::bail!("entries.jsonl not found in archive");
}

fn restore_from_reader<R: std::io::BufRead>(
    reader: R,
    db: &Db,
    strategy: ConflictStrategy,
) -> Result<()> {
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    let mut updated = 0usize;
    for line in reader.lines() {
        let line = line.context("Reading backup line")?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: Entry = serde_json::from_str(&line)
            .with_context(|| format!("Parsing entry: {line}"))?;
        let outcome = db.insert_entry_with_strategy(&entry, strategy)?;
        match outcome {
            InsertOutcome::Inserted { .. } | InsertOutcome::Rekeyed { .. } => inserted += 1,
            InsertOutcome::Updated { .. } | InsertOutcome::Merged { .. } => updated += 1,
            InsertOutcome::Ignored { .. } => skipped += 1,
        }
    }
    println!("✓ Restore: {inserted} inserted, {updated} updated, {skipped} skipped");
    Ok(())
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
            timezone: None, unit: None, keys: vec![], slug_keys: vec![], comment: None,
            id: None, id_delimiter: ":".to_string(),
            last_state: false, moment: false,
            conflict: None, update: false, merge: false,
            no_metadata: false,
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

    #[test]
    fn head_returns_oldest_first() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Insert out-of-order: newest first, then oldest, then middle.
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        cmd_head(HeadArgs { field: None, count: 10, output_flags: Default::default() }, &db).unwrap();

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
        cmd_head(HeadArgs { field: Some("mood".to_string()), count: 5, output_flags: Default::default() }, &db).unwrap();

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

        cmd_head(HeadArgs { field: Some("coffee".to_string()), count: 10, output_flags: Default::default() }, &db).unwrap();

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

        cmd_head(HeadArgs { field: None, count: 10, output_flags: Default::default() }, &db).unwrap();

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

        cmd_update(
            update_args(&e.slug, vec!["weight=80kg"]),
            ConflictStrategy::Update,
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

        cmd_update(
            update_args(&e.slug, vec!["tag=red"]),
            ConflictStrategy::Merge,
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
        cmd_update(args, ConflictStrategy::Merge, &db).unwrap();

        // Now overlay nested JSON via a second merge with -k syntax replaced
        // by direct extra_data manipulation through another merge call.
        // We'll exercise nested merging by passing a nested object through
        // parse_value_token's fallback (string): instead rely on the unit
        // test of `deep_merge` for that path.
        let after = db.get_entry(&e.slug).unwrap();
        let ex = after.data.as_ref().unwrap();
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

        cmd_update(args, ConflictStrategy::Rekey, &db).unwrap();

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
        db.soft_delete_entry(&e.slug).unwrap();

        // Use id (slug lookup would also find the tombstoned row via
        // get_entry_including_deleted).
        let err = cmd_update(
            update_args(&e.slug, vec!["reps=20"]),
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
            update_args(&e.slug, vec!["tag=red"]),
            ConflictStrategy::Merge,
            &db,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("tombstoned"));

        let mut rk = update_args(&e.slug, vec![]);
        rk.id = Some("%field%:%uid%".to_string());
        let err = cmd_update(rk, ConflictStrategy::Rekey, &db).unwrap_err();
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
        cmd_update(
            update_args(&prefix, vec!["note=fixed"]),
            ConflictStrategy::Update,
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
        let err = cmd_update(
            update_args("gym:", vec!["note=x"]),
            ConflictStrategy::Update,
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
            Verb::Add,
            None,
            Some(serde_json::json!({"tag": {"color": "red", "size": "m"}})),
        );

        // Invoke via Update default but override strategy to Merge — the nested
        // {color, size} should survive because deep-merge preserves siblings.
        let mut args = update_args(&e.slug, vec![]);
        args.strategy = Some(crate::args::ConflictStrategyArg::Merge);
        args.keys = vec!["tag.priority=1".to_string()];
        cmd_update(args, ConflictStrategy::Update, &db).unwrap();

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

        cmd_update(
            update_args(&e.slug, vec!["sad"]),
            ConflictStrategy::Update,
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
            Verb::Add,
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
            DeleteArgs { id_or_slug: id.clone(), force: true, output_flags: Default::default() },
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

        // File is line-per-entry JSONL (not a JSON array).
        let contents = std::fs::read_to_string(&path).unwrap();
        let line_count = contents
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count();
        assert_eq!(line_count, original.len());

        // Each line must parse as a full Entry.
        for line in contents.lines().filter(|l| !l.trim().is_empty()) {
            let _entry: datum_core::entry::Entry =
                serde_json::from_str(line).expect("each line is a parseable Entry");
        }

        // Restore into a fresh db and verify equivalence.
        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false },
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

        // Verify the archive contains manifest.json + entries.jsonl, and the
        // manifest carries schema_version + timestamp.
        let f = std::fs::File::open(&path).unwrap();
        let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(f));
        let mut archive = tar::Archive::new(gz);
        let mut seen_manifest = false;
        let mut seen_entries = false;
        let mut manifest_body = String::new();
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
                }
                _ => {}
            }
        }
        assert!(seen_manifest, "manifest.json present in archive");
        assert!(seen_entries, "entries.jsonl present in archive");
        let manifest: serde_json::Value =
            serde_json::from_str(&manifest_body).expect("manifest is valid JSON");
        assert_eq!(manifest["schema_version"], serde_json::json!(1));
        assert!(
            manifest["timestamp"].as_str().is_some(),
            "manifest has a timestamp"
        );

        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false },
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
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false },
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
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: false },
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
            RestoreArgs { path: path.to_string_lossy().into_owned(), force: true },
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
}
