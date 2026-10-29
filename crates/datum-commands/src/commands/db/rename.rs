//! `datum --db OLD db rename NEW` (da-o7ay8): the database under another
//! name. Its sync chain, ops and endpoint key are tables in the file, and a
//! file MOVED keeps its key (`sync::endpoint`), so it is the same member to
//! every other copy.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use datum_core::db::Db;

use crate::config::{self, ResolvedDb};

/// How long the daemon, or a `sync run`, has to let go of the file.
const LET_GO: Duration = Duration::from_secs(10);

/// What lies beside a database and is no part of it: locks, held by what has
/// it open, and the socket a `sync run` serves.
const RUNTIME: &[&str] = &[".joining", ".writing", ".applying", ".sync", ".spool.lock"];

pub fn rename(new: &str, cli_db: Option<&str>) -> Result<()> {
    // The page keeps its databases in OPFS, by name, which a move here misses.
    if cfg!(target_family = "wasm") {
        bail!("db rename: not in datum.day yet");
    }
    let from = config::locate_db(cli_db).context("Cannot determine data directory")?;
    ensure!(
        from.exists(),
        "db rename: there is no database at {}",
        from.display()
    );
    let from = from.canonicalize()?;
    let to = config::locate_db(Some(new)).context("Cannot determine data directory")?;
    ensure!(
        to.canonicalize().ok().as_ref() != Some(&from),
        "db rename: it is «{new}» already"
    );
    if to.exists() {
        refuse_unless_empty(&to, new, cli_db)?;
    }
    // The daemon steps aside, as for a `connect`, and no process has it open.
    #[cfg(feature = "net")]
    let _aside = crate::commands::sync_daemon::hold(&from)?;
    // Its lock held through the move, so no write lands meanwhile: in
    // exclusive locking mode a read keeps it until the handle closes.
    let held = alone(&from)?;
    held.conn()
        .execute_batch("PRAGMA locking_mode = EXCLUSIVE; SELECT count(*) FROM sqlite_master;")?;
    if to.exists() {
        drop(alone(&to)?);
        for gone in ["-wal", "-shm", "-journal", ".spool"].map(|s| beside(&to, s)) {
            if gone.exists() {
                std::fs::remove_file(&gone)?;
            }
        }
    }
    if let Some(dir) = to.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    // Every name it is registered under, read while the file is there.
    let mut registry = config::load_registry().unwrap_or_default();
    registry.db.retain(|_, e| {
        Path::new(&e.path)
            .canonicalize()
            .map_or(true, |p| p != from)
    });
    let stem = file_name(&from);
    for path in std::fs::read_dir(from.parent().context("no directory")?)? {
        let path = path?.path();
        let name = file_name(&path);
        let Some(rest) = name.strip_prefix(&stem) else {
            continue;
        };
        if (rest.starts_with('-') || rest.starts_with('.')) && !RUNTIME.contains(&rest) {
            std::fs::rename(&path, beside(&to, rest))?;
        }
    }
    std::fs::rename(&from, &to)?;
    drop(held);
    config::save_registry(&registry)?;
    match config::looks_like_path(new) {
        true => config::ensure_known(&to)?,
        false => config::ensure_registered(&ResolvedDb {
            name: Some(new.into()),
            path: to.clone(),
            created_new: false,
        })?,
    }
    println!("db rename: {} → {}", from.display(), to.display());
    Ok(())
}

/// A database at the new name is replaced only when it holds nothing.
fn refuse_unless_empty(to: &Path, new: &str, cli_db: Option<&str>) -> Result<()> {
    let there = Db::open_read_only(&to.to_string_lossy())?;
    let chain = there.sync_chain()?;
    // Its specs, aliases and settings too: what a person set up is theirs.
    let entries: i64 = there.conn().query_row(
        "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL AND superseded_by IS NULL",
        [],
        |r| r.get(0),
    )?;
    if entries == 0
        && !chain.exists()
        && there.sync_joining()?.is_empty()
        && there.spool_queued() == 0
    {
        return Ok(());
    }
    let mut what = match entries {
        1 => "1 entry".to_string(),
        n => format!("{n} entries"),
    };
    if chain.exists() {
        what += &format!(" and is in sync chain {}", chain.id);
    }
    let old = cli_db
        .map(str::to_string)
        .or_else(|| std::env::var("DATUM_DB").ok().filter(|v| !v.is_empty()))
        .map_or_else(String::new, |v| format!("--db {v} "));
    bail!(
        "db rename: «{new}» exists and holds {what}. To put this database in its place, \
         empty that one first: `datum --db {new} db purge` (with --leave or --rejoin, as it \
         asks, for a copy in a chain), then `datum {old}db rename {new}`"
    )
}

/// A handle on `db` once no other has it open: out of WAL, which only
/// the one connection to a file can leave, so its `-wal` is folded in and
/// gone. The daemon has [`LET_GO`] to close it.
fn alone(db: &Path) -> Result<Db> {
    let held = Db::open_unmigrated(&db.to_string_lossy())?;
    let conn = held.conn();
    conn.busy_timeout(Duration::ZERO)?;
    let until = Instant::now() + LET_GO;
    loop {
        let mode = conn.query_row("PRAGMA journal_mode = DELETE", [], |r| {
            r.get::<_, String>(0)
        });
        if mode.is_ok_and(|m| m == "delete") {
            return Ok(held);
        }
        if Instant::now() > until {
            bail!(
                "db rename: another process has {} open (a `datum sync run`, a `--watch`): \
                 stop it, then rename",
                db.display()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into()
}

fn beside(db: &Path, suffix: &str) -> PathBuf {
    let mut path = db.as_os_str().to_owned();
    path.push(suffix);
    path.into()
}
