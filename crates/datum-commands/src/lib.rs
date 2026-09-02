#![doc = include_str!("../README.md")]

//! datum commands — the command layer between argv parsing (in `datum-cli`
//! via `datum-parse`) and the core data model (`datum-core`).
//!
//! For R1 this crate holds:
//! * `args` — typed argument structs (migrated wholesale from `datum-cli`)
//! * `config` — XDG path resolution, db registry, datumrc.toml
//! * `render` — entry/table output formatting (formerly `output.rs`)
//! * `commands` — one module per command (check, sync, plus the legacy
//!   `cmd` dispatcher while R1 is in-flight)
//!
//! In R2 a bespoke parser powers each command, which grows typed +
//! string entry points. For R1 the arg structs in `args` double as the
//! typed API.

pub mod args;
pub mod cmd;
pub mod commands;
pub mod config;
pub mod render;

/// Tell the user, on stderr, that opening the database migrated it.
///
/// Migrations are automatic and invisible: any command that opens the DB
/// runs them, so the first `datum` invocation after an upgrade silently
/// reshapes the user's data. `datum-core` deliberately never prints — it
/// returns [`datum_core::db::MigrationReport`] instead and the telling
/// happens here, at the edge (da-ksao).
///
/// stderr, not stdout, so `datum tail -o … | …` keeps working across an
/// upgrade.
///
/// Call this after **every** `Db::open`. Whichever open happens to find the
/// schema stale is the one that migrates, and only that one carries a
/// report — so covering all the open sites cannot double-print, while
/// missing one loses the notice entirely.
pub fn announce_migration(db: &datum_core::db::Db) {
    let Some(report) = db.last_migration() else {
        return;
    };
    let n = report.applied.len();
    eprintln!(
        "note: database schema migrated v{} → v{} ({n} step{})",
        report.from_version,
        report.to_version,
        if n == 1 { "" } else { "s" }
    );
    if let Some(path) = &report.backup_path {
        eprintln!("note: pre-migration backup saved to {}", path.display());
    }
    if !report.pruned.is_empty() {
        eprintln!(
            "note: removed {} older pre-migration backup(s); the newest {} are kept",
            report.pruned.len(),
            datum_core::db::MAX_PRE_MIGRATION_BACKUPS
        );
    }
}
