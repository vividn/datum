//! `datum db <subcommand>` — DB-lifecycle group.
//!
//! Houses every database administration verb: `setup`, `purge`, `backup`,
//! `restore`, `migrate`, `import`. da-33g consolidated the standalone
//! `setup` / `backup` / `restore` / `migrate` top-level verbs under here.
//! da-57t removed the deprecated `setup --force-recreate` alias; `db purge`
//! is its replacement.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::Result;
use datum_core::db::Db;
use datum_core::sync_state;

use crate::args::DbCommand;
use crate::args::DbPurgeArgs;
use crate::commands::sync::cluster_ops;

pub mod import;

pub fn dispatch(cmd: DbCommand, cli_db: Option<&str>, db: &Db) -> Result<()> {
    match cmd {
        DbCommand::Setup(args) => super::setup::setup(args, db),
        DbCommand::Purge(args) => purge(args, cli_db, db),
        DbCommand::Backup(args) => super::backup_restore::cmd_backup(args, db),
        DbCommand::Restore(args) => super::backup_restore::cmd_restore(args, db),
        DbCommand::Migrate(sub) => super::migrate::migrate(sub, db),
        DbCommand::Import(args) => import::import(args, db),
    }
}

/// `datum db purge` — destructive, prompts for the database identifier
/// as confirmation. Mirrors the PostgreSQL `DROP DATABASE`-with-name
/// pattern: reflexive `y`-mashing won't trigger a wipe; the user has
/// to retype the real identifier.
///
/// da-5df1: the confirmation token follows the resolution shape — if
/// the user opened the DB via `--db <name>` (registry or default-dir
/// lookup), the prompt accepts that bare name. If they passed an
/// explicit `--db <path>`, the prompt still wants the full path
/// (legacy behaviour). `--yes` skips the prompt for non-interactive
/// runs.
pub fn purge(args: DbPurgeArgs, cli_db: Option<&str>, db: &Db) -> Result<()> {
    let mut stdin = std::io::stdin().lock();
    let is_tty = std::io::stdin().is_terminal();
    let confirm = purge_confirmation_token(cli_db, db);
    purge_with_io(args, db, &mut stdin, is_tty, &confirm)
}

/// What the user has to type to confirm the wipe. Built from the
/// resolution path: a name-resolved DB confirms by name (the same
/// token the user passed in `--db <name>` / `DATUM_DB=<name>`); a
/// path-resolved DB still requires the full path.
fn purge_confirmation_token(cli_db: Option<&str>, db: &Db) -> String {
    if let Ok(resolved) = crate::config::resolve_db(cli_db) {
        if let Some(name) = resolved.name {
            return name;
        }
    }
    // Path-resolved (or resolution itself failed) — fall back to the
    // pragma-derived path. Empty for in-memory test DBs; harmless,
    // since interactive prompts can't fire on those (no TTY).
    current_db_path(db).unwrap_or_default()
}

/// Test seam — same logic as [`purge`] but with the stdin reader, TTY
/// flag, and confirmation token injected. Production paths go through
/// [`purge`].
pub fn purge_with_io(
    args: DbPurgeArgs,
    db: &Db,
    stdin: &mut dyn BufRead,
    is_tty: bool,
    confirm: &str,
) -> Result<()> {
    if !args.yes {
        if !is_tty {
            anyhow::bail!(
                "db purge: stdin is not a TTY; re-run with --yes to purge non-interactively"
            );
        }
        let mut stderr = std::io::stderr().lock();
        // Render WHAT will be destroyed using the resolved path so the
        // user sees the real on-disk location even when they confirm
        // by name. Always include the path; the differing bit is only
        // the confirmation token.
        let path = current_db_path(db).unwrap_or_else(|_| "(in-memory)".to_string());
        writeln!(stderr, "This will destroy ALL data in {path}.")?;
        // Branch the prompt copy on confirmation-token shape so the
        // user sees "Type 'vremya'" instead of "Type the database
        // path" when the resolution was name-based.
        let looks_like_path =
            confirm.contains('/') || confirm.starts_with('.') || confirm.ends_with(".db");
        if looks_like_path {
            write!(stderr, "Type the database path to confirm: ")?;
        } else {
            write!(stderr, "Type '{confirm}' to confirm purge: ")?;
        }
        stderr.flush()?;
        drop(stderr);

        let mut line = String::new();
        stdin.read_line(&mut line)?;
        let answer = line.trim();
        if answer != confirm {
            anyhow::bail!("db purge: aborted (input did not match {confirm})");
        }
    }

    // Capture the cluster_id BEFORE the purge wipes sync_state. The
    // bump-generation HTTP call happens AFTER the local drop succeeds —
    // failing partway through is fine because the wipe is the
    // load-bearing change and the user re-runs to retry the network
    // step. (da-ws1q sub-5)
    let prior_cluster = if args.cluster {
        sync_state::get_cluster_id(db).ok().flatten()
    } else {
        None
    };

    let dropped = db.force_recreate_schema()?;
    println!("db purge: complete");
    println!("  dropped: {}", dropped.join(", "));
    println!(
        "  schema: v{}",
        datum_core::migrations::CURRENT_SCHEMA_VERSION
    );

    if args.cluster {
        match prior_cluster {
            Some(cluster_id) => {
                match cluster_ops::bump_generation_for_current_server(&cluster_id) {
                    Ok(new_generation) => {
                        // Re-stamp local sync_state so the NEXT push from
                        // this device sends the new generation rather than
                        // the stale 0 the purge reset us to.
                        sync_state::set_cluster_id(db, &cluster_id).ok();
                        sync_state::set_cluster_generation(db, new_generation).ok();
                        println!("  cluster {cluster_id}: bumped to generation {new_generation}");
                        println!("  peers on the old generation will see GenerationMismatchError next sync");
                    }
                    Err(e) => {
                        eprintln!(
                            "⚠ db purge: local wipe succeeded, but cluster bump failed: {e:#}"
                        );
                        eprintln!(
                        "  re-run `datum sync` once you can reach the relay; peers may otherwise re-push the wiped state"
                    );
                    }
                }
            }
            None => {
                println!("  cluster: nothing to bump (no recorded cluster_id; never synced)");
            }
        }
    }

    println!("  OK");
    Ok(())
}

/// Read the active database file path from SQLite's
/// `pragma_database_list`. Returns the empty string for an in-memory
/// connection, which is fine for tests — they pass `--yes`.
fn current_db_path(db: &Db) -> Result<String> {
    let path: String = db.conn().query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get(0),
    )?;
    Ok(path)
}
