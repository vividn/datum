//! `datum db setup` — apply pending schema migrations to the active DB.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::SetupArgs;

pub fn setup(_args: SetupArgs, db: &Db) -> Result<()> {
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
