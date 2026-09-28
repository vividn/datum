//! `datum info [<target>]` — print database metadata (schema, counts, size).

use anyhow::Result;
use datum_core::db::Db;
use datum_core::time::Zone;

use crate::args::InfoArgs;
use crate::config;
use crate::render::colors::paint;

pub fn info(args: InfoArgs, cli_db: Option<&str>, active_db: &Db) -> Result<()> {
    let failed = if args.all {
        print_info_all()?
    } else {
        match args.target.as_deref() {
            Some(target) => {
                let resolved = config::resolve_db(Some(target))?;
                print_info_for_path(resolved.name.as_deref(), &resolved.path)?
            }
            None => {
                let resolved = config::resolve_db(cli_db)?;
                print_info_block(resolved.name.as_deref(), &resolved.path, Some(active_db))?
            }
        }
    };
    if failed > 0 {
        anyhow::bail!("info: {failed} line(s) could not be read");
    }
    Ok(())
}

/// Returns how many lines, over every block, could not be read.
fn print_info_all() -> Result<usize> {
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
        return Ok(0);
    }
    let mut failed = 0;
    let mut first = true;
    for (name, path) in &seen {
        if !first {
            println!();
        }
        first = false;
        failed += print_info_for_path(Some(name), path)?;
    }
    Ok(failed)
}

fn print_info_for_path(name: Option<&str>, path: &std::path::Path) -> Result<usize> {
    if !path.exists() {
        println!("{}  {}", name.unwrap_or(""), config::display_path(path));
        println!("  schema:   ✗ (missing)");
        return Ok(0);
    }
    // Unmigrated: a report must not rewrite what it reports on, and the
    // version it prints is only worth reading from the untouched file
    // (da-3n9t1). Any other command run with `--db` on it migrates it.
    let path_str = path.to_string_lossy();
    let db = match datum_core::db::Db::open_unmigrated(&path_str) {
        Ok(db) => db,
        Err(e) => {
            println!("{}  {}", name.unwrap_or(""), config::display_path(path));
            println!("  schema:   ✗ ({e})");
            return Ok(0);
        }
    };
    print_info_block(name, path, Some(&db))
}

/// Prints one database's block and returns how many of its lines could
/// not be read. Such a line says `QUERY ERROR` in red, never the `0` or
/// `(none)` of an empty database, and says why on stderr (da-gffn4).
fn print_info_block(name: Option<&str>, path: &std::path::Path, db: Option<&Db>) -> Result<usize> {
    println!("{}  {}", name.unwrap_or(""), config::display_path(path));
    let Some(db) = db else {
        println!("  schema:   ? (not opened)");
        return Ok(0);
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

    let mut failed = 0;
    let mut read = |line: &str, value: Result<String>| {
        value.unwrap_or_else(|e| {
            eprintln!("info: {line}: {e}");
            failed += 1;
            paint("\x1b[31m", "QUERY ERROR")
        })
    };

    let live = read(
        "entries",
        db.count_entries()
            .map(|n| n.to_string())
            .map_err(Into::into),
    );
    let deleted = read("deleted", count_tombstones(db).map(|n| n.to_string()));
    println!("  entries:  {live} ({deleted} deleted)");

    let fields = read(
        "fields",
        tracked_ignored_counts(db).map(|(t, i)| format!("{t} tracked, {i} ignored")),
    );
    println!("  fields:   {fields}");

    // `(none)` is a read that found nothing; a query naming a column that
    // does not exist once hid behind it (da-lrn59).
    let last = read(
        "last write",
        last_modified(db).map(|ts| ts.unwrap_or_else(|| "(none)".into())),
    );
    println!("  last write: {last}");

    let size = read(
        "size",
        std::fs::metadata(path)
            .map(|m| format_size(m.len()))
            .map_err(Into::into),
    );
    println!("  size:     {size}");
    Ok(failed)
}

/// `info`'s field tally: `(tracked, ignored)` over
/// [`Db::get_tracked_fields`], which counts a field ignored only when its
/// latest state-bearing row says so (da-m1l8) — the same rule `datum
/// fields --json` reports per field.
pub fn tracked_ignored_counts(db: &Db) -> Result<(usize, usize)> {
    Ok(db
        .get_tracked_fields(chrono::Utc::now())?
        .iter()
        .fold(
            (0, 0),
            |(t, i), f| {
                if f.ignored {
                    (t, i + 1)
                } else {
                    (t + 1, i)
                }
            },
        ))
}

fn count_tombstones(db: &Db) -> Result<usize> {
    let n: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM entries WHERE deleted_at IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(n as usize)
}

fn last_modified(db: &Db) -> Result<Option<String>> {
    let s: Option<String> = db.conn().query_row(
        "SELECT MAX(modify_time) FROM entries WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(
        s.map(|iso| match chrono::DateTime::parse_from_rfc3339(&iso) {
            Ok(dt) => {
                let local = Zone::host().local_naive(dt.to_utc());
                local.format("%Y-%m-%d %H:%M").to_string()
            }
            Err(_) => iso,
        }),
    )
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
