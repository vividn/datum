//! `datum info [<target>]` — print database metadata (schema, counts, size).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::InfoArgs;
use crate::config;

pub fn info(args: InfoArgs, cli_db: Option<&str>, active_db: &Db) -> Result<()> {
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
        println!("{}  {}", name.unwrap_or(""), config::display_path(path));
        println!("  schema:   ✗ (missing)");
        return Ok(());
    }
    let path_str = path.to_string_lossy();
    let db = match datum_core::db::Db::open(&path_str) {
        Ok(db) => db,
        Err(e) => {
            println!("{}  {}", name.unwrap_or(""), config::display_path(path));
            println!("  schema:   ✗ ({e})");
            return Ok(());
        }
    };
    print_info_block(name, path, Some(&db))
}

fn print_info_block(name: Option<&str>, path: &std::path::Path, db: Option<&Db>) -> Result<()> {
    println!("{}  {}", name.unwrap_or(""), config::display_path(path));
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
        None => ("⚠", "unversioned (run `datum db setup`)".to_string()),
    };
    let version_str = stored
        .map(|v| format!("v{v}"))
        .unwrap_or_else(|| "?".into());
    println!("  schema:   {version_str} {schema_glyph} ({schema_note})");

    let live = db.count_entries().unwrap_or(0);
    let deleted = count_tombstones(db).unwrap_or(0);
    println!("  entries:  {live} ({deleted} deleted)");

    let tracked = db
        .get_tracked_fields(chrono::Utc::now())
        .unwrap_or_default();
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
