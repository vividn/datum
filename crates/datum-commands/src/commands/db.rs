//! `datum db <subcommand>` — DB-lifecycle group.
//!
//! Replaces `datum setup --force-recreate` (now deprecated) and is the
//! landing pad for the rest of the `db` group (migrate / backup / restore
//! / import / setup) tracked in da-9u9.3. The current shape is `db purge`
//! only, the verb-naming lock-in from da-e16.

use std::io::{BufRead, IsTerminal, Write};

use anyhow::Result;
use datum_core::db::Db;

use crate::args::{DbCommand, DbPurgeArgs};

pub fn dispatch(cmd: DbCommand, db: &Db) -> Result<()> {
    match cmd {
        DbCommand::Purge(args) => purge(args, db),
    }
}

/// `datum db purge` — destructive, prompts for the database path as
/// confirmation. Mirrors the PostgreSQL `DROP DATABASE`-with-name pattern:
/// reflexive `y`-mashing won't trigger a wipe; the user has to retype the
/// real path. `--yes` skips the prompt for non-interactive runs.
pub fn purge(args: DbPurgeArgs, db: &Db) -> Result<()> {
    let mut stdin = std::io::stdin().lock();
    let is_tty = std::io::stdin().is_terminal();
    purge_with_io(args, db, &mut stdin, is_tty)
}

/// Test seam — same logic as [`purge`] but with the stdin reader and TTY
/// flag injected. Production paths go through [`purge`].
pub fn purge_with_io(
    args: DbPurgeArgs,
    db: &Db,
    stdin: &mut dyn BufRead,
    is_tty: bool,
) -> Result<()> {
    if !args.yes {
        if !is_tty {
            anyhow::bail!(
                "db purge: stdin is not a TTY; re-run with --yes to purge non-interactively"
            );
        }
        let path = current_db_path(db)?;
        let mut stderr = std::io::stderr().lock();
        writeln!(stderr, "This will destroy ALL data in {path}.")?;
        write!(stderr, "Type the database path to confirm: ")?;
        stderr.flush()?;
        drop(stderr);

        let mut line = String::new();
        stdin.read_line(&mut line)?;
        let answer = line.trim();
        if answer != path {
            anyhow::bail!("db purge: aborted (input did not match database path)");
        }
    }

    let dropped = db.force_recreate_schema()?;
    println!("db purge: complete");
    println!("  dropped: {}", dropped.join(", "));
    println!(
        "  schema: v{}",
        datum_core::migrations::CURRENT_SCHEMA_VERSION
    );
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
