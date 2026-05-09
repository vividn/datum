//! `datum db migrate` — schema migration management.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::MigrateCommand;

pub fn migrate(cmd: MigrateCommand, db: &Db) -> Result<()> {
    match cmd {
        MigrateCommand::Run => migrate_run(db),
        MigrateCommand::Status => migrate_status(db),
        MigrateCommand::Edit => migrate_edit(),
    }
}

fn migrate_run(db: &Db) -> Result<()> {
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

fn migrate_status(db: &Db) -> Result<()> {
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

fn migrate_edit() -> Result<()> {
    anyhow::bail!(
        "`datum migrate edit` (per-entry data migration) is not yet implemented; \
         tracked in bead da-glnr"
    )
}
