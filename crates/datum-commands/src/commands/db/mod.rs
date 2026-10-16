//! `datum db <subcommand>` — DB-lifecycle group.
//!
//! Houses every database administration verb: `purge`, `backup`,
//! `restore`, `migrate`, `v2import`. da-33g consolidated the standalone
//! `setup` / `backup` / `restore` / `migrate` top-level verbs under here,
//! and da-a4ptr folded `db setup` into `db migrate`.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::Result;
use datum_core::db::Db;

use crate::args::DbCommand;
use crate::args::DbPurgeArgs;

pub mod import;

pub fn dispatch(cmd: DbCommand, cli_db: Option<&str>, db: &Db) -> Result<()> {
    match cmd {
        DbCommand::Purge(args) => purge(args, cli_db, db),
        DbCommand::Backup(args) => super::backup_restore::cmd_backup(args, cli_db, db),
        DbCommand::BackupRotation(args) => {
            super::backup_rotation::cmd_backup_rotation(args, cli_db, db)
        }
        DbCommand::Restore(args) => super::backup_restore::cmd_restore(args, cli_db, db),
        DbCommand::Migrate(sub) => super::migrate::migrate(sub, db),
        DbCommand::V2import(args) => import::import(args, db),
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
pub(crate) fn purge_confirmation_token(cli_db: Option<&str>, db: &Db) -> String {
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
    // Whole copies of earlier states of this database, so a purge that left
    // them would leave the data (da-nc8i1). Beside the path the handle was
    // opened at, where they were written: SQLite's own name for the file is
    // the target of a symlinked database.
    let snapshots = db
        .path()
        .map(datum_core::migrations::pre_migration_backups)
        .unwrap_or_default();
    // A chained copy leaves its chain or rejoins it (da-qjxfx), or purges
    // the whole chain (`--chain`, parsed as `sync chain purge --data`):
    // wiped silently, it would leave the other members dialling it.
    // One that will not read (no `ops`: a damaged file) is in none, and so
    // is one a purge dissolved (da-qzrj9): nothing to leave or rejoin.
    let chain = db
        .sync_chain()
        .ok()
        .filter(|c| c.exists() && c.dissolved().is_none());
    let chain_id = chain.as_ref().map_or("", |c| c.id.as_str());
    let (leave, rejoin) = match (chain.is_some(), args.leave, args.rejoin) {
        (false, false, false) => (false, false),
        (false, ..) => anyhow::bail!(
            "db purge: this copy is in no sync chain; --leave and --rejoin are for one that is"
        ),
        // Which it means is said, as `db restore --hard` has it (da-n0zx1).
        (true, false, false) => anyhow::bail!(
            "db purge: this copy is in sync chain {chain_id}, whose other copies would keep \
             dialling it. --leave tells the chain this copy leaves, then wipes it; --chain \
             purges every copy in the chain (--admin-key in a locked chain); --rejoin wipes it \
             and refetches everything from the chain, staying a member"
        ),
        (true, leave, rejoin) => (leave, rejoin),
    };
    let sources = match rejoin {
        true => db.sync_rejoin_sources()?,
        false => Vec::new(),
    };
    if !args.yes {
        let path = current_db_path(db).unwrap_or_else(|_| "(in-memory)".to_string());
        let mut headline = purge_headline(&path, &snapshots);
        if let n @ 1.. = db.spool_queued() {
            headline += &format!("\nIts {n} queued change(s), not written yet, go too.");
        }
        if rejoin {
            headline += &format!(
                "\nIt stays a member of sync chain {}, and refetches everything from it.",
                chain_id
            );
        }
        confirm_by_name("db purge", &headline, stdin, is_tty, confirm)?;
    }
    let told = match leave && !args.force {
        true => super::sync::depart(db, chain_id, "datum db purge --leave")?,
        false => None,
    };

    // A change queued before the purge is the old copy's (da-owy96).
    let (dropped, queued) = db.spool_discard_after(|| match rejoin {
        true => db.sync_purge_rejoin(),
        false => db.force_recreate_schema(),
    })?;
    // Only once that succeeded: a purge it failed is run again, and removes
    // them then.
    for snapshot in &snapshots {
        std::fs::remove_file(snapshot)
            .map_err(|e| anyhow::anyhow!("db purge: {} is not removed: {e}", snapshot.display()))?;
    }
    println!("db purge: complete");
    println!("  dropped: {}", dropped.join(", "));
    for snapshot in &snapshots {
        println!("  removed: {}", snapshot.display());
    }
    if queued > 0 {
        println!("  removed: {queued} queued change(s), never written");
    }
    println!("  file: rewritten, none of the old pages are left in it");
    println!(
        "  schema: v{}",
        datum_core::migrations::CURRENT_SCHEMA_VERSION
    );

    if leave {
        println!("  left sync chain {}", chain_id);
        if let Some(told) = told {
            println!("  its departure is with: {}", told.join(", "));
        }
    }
    if rejoin {
        println!(
            "  kept: this copy's endpoint key, so it stays a member of sync chain {}",
            chain_id
        );
    }
    println!("  OK");
    if rejoin {
        refetch(db, &sources);
    }
    Ok(())
}

/// A rejoined copy's first sync, with the members it refetches from. One
/// that does not finish is for `datum sync` to resume, or the daemon.
fn refetch(db: &Db, sources: &[String]) {
    println!("refetching from {}…", sources.join(", "));
    #[cfg(feature = "net")]
    let done = datum_sync::now_all(db);
    #[cfg(not(feature = "net"))]
    let done: Result<()> = Err(anyhow::anyhow!("no network here"));
    // Its chain is back with the first sync's records, which come last;
    // a member that did not answer is no matter once one did.
    if !db.sync_chain().is_ok_and(|c| c.exists()) {
        let why = done.err().map(|e| format!(" ({e:#})")).unwrap_or_default();
        println!("not refetched yet{why}: `datum sync` resumes it");
    }
}

/// What a purge says it will take, before it asks.
fn purge_headline(path: &str, snapshots: &[std::path::PathBuf]) -> String {
    let mut headline = format!("This will destroy ALL data in {path}.");
    if !snapshots.is_empty() {
        headline += "\nIts pre-migration snapshots go too:";
        for snapshot in snapshots {
            headline += &format!("\n  {}", snapshot.display());
        }
    }
    headline
}

/// The confirmation a command that destroys the whole database asks for:
/// `cmd` names it, `headline` (if any) says what goes, then the database's identifier, retyped —
/// reflexive `y`-mashing won't do it. Off a TTY it refuses; `--yes` is
/// how a script says it means it.
pub(crate) fn confirm_by_name(
    cmd: &str,
    headline: &str,
    stdin: &mut dyn BufRead,
    is_tty: bool,
    confirm: &str,
) -> Result<()> {
    if !is_tty {
        anyhow::bail!("{cmd}: stdin is not a TTY; re-run with --yes to confirm without a prompt");
    }
    let mut stderr = std::io::stderr().lock();
    if !headline.is_empty() {
        writeln!(stderr, "{headline}")?;
    }
    // Branch the prompt copy on confirmation-token shape so the
    // user sees "Type 'vremya'" instead of "Type the database
    // path" when the resolution was name-based.
    let looks_like_path =
        confirm.contains('/') || confirm.starts_with('.') || confirm.ends_with(".db");
    if looks_like_path {
        write!(stderr, "Type the database path to confirm: ")?;
    } else {
        write!(stderr, "Type '{confirm}' to confirm {cmd}: ")?;
    }
    stderr.flush()?;
    drop(stderr);

    let mut line = String::new();
    stdin.read_line(&mut line)?;
    if line.trim() != confirm {
        anyhow::bail!("{cmd}: aborted (input did not match {confirm})");
    }
    Ok(())
}

/// Read the active database file path from SQLite's
/// `pragma_database_list`. Returns the empty string for an in-memory
/// connection, which is fine for tests — they pass `--yes`.
pub(crate) fn current_db_path(db: &Db) -> Result<String> {
    let path: String = db.conn().query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get(0),
    )?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::purge_headline;

    /// da-nc8i1: the warning names every snapshot the purge removes.
    #[test]
    fn the_purge_warning_names_the_snapshots_it_removes() {
        assert_eq!(
            purge_headline("/d/x.db", &[]),
            "This will destroy ALL data in /d/x.db."
        );
        assert_eq!(
            purge_headline(
                "/d/x.db",
                &["/d/x.db.pre-v1-to-v2-20260101T000000Z.bak".into()]
            ),
            "This will destroy ALL data in /d/x.db.\n\
             Its pre-migration snapshots go too:\n  \
             /d/x.db.pre-v1-to-v2-20260101T000000Z.bak"
        );
    }
}
