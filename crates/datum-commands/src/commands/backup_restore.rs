//! `datum backup` / `datum restore` — JSONL or tar.gz snapshot of the
//! entry set, plus the streaming restore that replays it.
//!
//! Two file shapes are supported:
//! * `.jsonl` — one metadata header line followed by one Entry per line.
//! * `.tar.gz` — a tar archive containing `manifest.json` and `entries.jsonl`
//!   (same line layout as the plain JSONL form).
//!
//! Restore REPLACES by default (da-5fooh): afterwards the live entries are
//! exactly the backup's, and the rest are tombstoned — ordinary deletions,
//! so they sync and can be seen. `--merge` only adds, content-hash
//! idempotent; an entry edited since the backup keeps its live version, and
//! one whose slug or state's instant a different live entry holds comes in
//! as a sync from a copy frozen at the backup would bring it (da-4ldvb): a
//! row and an op of its own under the live entry, merged by the fold, what
//! the two give differently in the entry's `conflicts`. Either way it gates on the `schema_version` recorded in the
//! header and commits in a single transaction — any failure rolls back to
//! "nothing restored". See `RestoreReport` for the projected/committed
//! outcome contract.

use anyhow::{Context, Result};
use datum_core::{
    db::{ConflictStrategy, Db, InsertOutcome},
    entry::{new_hid, Entry},
    error::DatumError,
};

use crate::{
    args::{BackupArgs, RestoreArgs, ShowFlag},
    config,
    progress::Progress,
};

/// Version of the `.tar.gz` archive *envelope* (manifest + file layout).
/// Independent of the DB schema version embedded in the JSONL header — this
/// one only bumps if the tar layout itself (filenames, additional metadata
/// files) changes.
const BACKUP_ARCHIVE_VERSION: u32 = 1;

/// Discriminator the JSONL header line carries so a restore can tell a
/// headered backup apart from a legacy headerless one (which starts straight
/// with an Entry).
const BACKUP_HEADER_TYPE: &str = "daydumb_backup";

/// Metadata line written as the first JSONL record of every backup produced
/// by this binary (in both plain `.jsonl` and inside the tar.gz variant's
/// `entries.jsonl`). The restore side uses it to decide whether to migrate,
/// refuse, or proceed optimistically.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct BackupHeader {
    #[serde(rename = "type")]
    kind: String,
    /// DB schema version (from `datum_core::migrations::CURRENT_SCHEMA_VERSION`)
    /// at backup time. Restore compares this against the binary's current.
    schema_version: u32,
    /// RFC3339 UTC timestamp when the backup was written.
    backup_time: String,
    /// Best-effort source identifier — registered DB name or file path.
    source_db: String,
    /// Live entry count at backup time, used for a post-restore integrity
    /// check (warning only; mismatch does not abort).
    entry_count: usize,
}

impl BackupHeader {
    pub(crate) fn new(
        db: &Db,
        source_db: String,
        backup_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<Self> {
        Ok(Self {
            kind: BACKUP_HEADER_TYPE.to_string(),
            schema_version: datum_core::migrations::CURRENT_SCHEMA_VERSION,
            backup_time: backup_time.to_rfc3339(),
            source_db,
            entry_count: db.count_entries()?,
        })
    }
}

/// On-disk encoding of a backup, picked from the path extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackupFormat {
    Jsonl,
    /// `.jsonl.gz` (any other `.gz`): the JSONL form, gzipped — what a
    /// rotation's slots hold (da-d5cmj).
    JsonlGz,
    TarGz,
}

/// Single source of truth for backup-format detection. Both `cmd_backup`
/// and `cmd_restore_with_report` route through this so the extension rules
/// stay in one place.
fn detect_backup_format(path: &str) -> BackupFormat {
    let lower = path.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        BackupFormat::TarGz
    } else if lower.ends_with(".gz") {
        BackupFormat::JsonlGz
    } else {
        BackupFormat::Jsonl
    }
}

/// Resolve a best-effort identifier for the DB being backed up — the one
/// `cli_db` (the global `--db`) opened, else `DATUM_DB` or the default, as
/// the command itself resolved it. Prefers the registry name, falls back to
/// the resolved path, and finally to an empty string when neither can be
/// recovered (e.g. in-memory DB in tests).
pub(crate) fn derive_source_db(cli_db: Option<&str>) -> String {
    match config::resolve_db(cli_db) {
        Ok(r) => r
            .name
            .unwrap_or_else(|| r.path.to_string_lossy().into_owned()),
        Err(_) => String::new(),
    }
}

/// The path that means "not a file": stdout for a backup, stdin for a
/// restore, always JSONL. What a caller with no filesystem of its own — the
/// browser, which offers the backup as a download (da-g8o8) — reads and
/// writes instead.
const STDIO_PATH: &str = "-";

pub fn cmd_backup(args: BackupArgs, cli_db: Option<&str>, db: &Db) -> Result<()> {
    let out_path = std::path::Path::new(&args.path);
    if args.path != STDIO_PATH && out_path.exists() && !args.overwrite {
        anyhow::bail!(
            "File exists: {} (pass --overwrite to replace)",
            out_path.display()
        );
    }

    let header = BackupHeader::new(db, derive_source_db(cli_db), chrono::Utc::now())?;

    if args.path == STDIO_PATH {
        // The document is the output, so there is no "✓ Backup" line.
        write_jsonl(std::io::stdout().lock(), db, &header)?;
        return Ok(());
    }
    match detect_backup_format(&args.path) {
        BackupFormat::TarGz => backup_targz(out_path, db, &header),
        BackupFormat::Jsonl => backup_jsonl(out_path, db, &header, false),
        BackupFormat::JsonlGz => backup_jsonl(out_path, db, &header, true),
    }
}

fn backup_jsonl(path: &std::path::Path, db: &Db, header: &BackupHeader, gz: bool) -> Result<()> {
    let file = std::fs::File::create(path)
        .with_context(|| format!("Creating backup file: {}", path.display()))?;
    let count = if gz {
        write_jsonl_gz(file, db, header)?
    } else {
        write_jsonl(file, db, header)?
    };
    println!("✓ Backup: {} entries → {}", count, path.display());
    Ok(())
}

/// The header line, then one entry per line. Returns how many entries.
fn write_jsonl(out: impl std::io::Write, db: &Db, header: &BackupHeader) -> Result<usize> {
    use std::io::Write;
    let mut writer = std::io::BufWriter::new(out);
    writeln!(writer, "{}", serde_json::to_string(header)?)?;
    let mut count = 0usize;
    db.visit_all_entries::<anyhow::Error, _>(|entry| {
        let line = serde_json::to_string(&entry)?;
        writeln!(writer, "{line}")?;
        count += 1;
        Ok(())
    })?;
    writer.flush()?;
    Ok(count)
}

/// [`write_jsonl`], gzipped, and on the disk before it returns.
pub(crate) fn write_jsonl_gz(file: std::fs::File, db: &Db, header: &BackupHeader) -> Result<usize> {
    let mut gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let count = write_jsonl(&mut gz, db, header)?;
    gz.finish()?.sync_all()?;
    Ok(count)
}

/// RAII guard that removes a temp file when dropped (even on panic/error paths).
struct TempFileGuard(std::path::PathBuf);
impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Create+truncate a file owned read/write by the current user only (0600).
/// Used for the backup staging file, which transiently holds the full
/// plaintext dataset in a shared temp dir (da-903m). On non-Unix we fall
/// back to a plain create — those platforms don't share `/tmp` the same way.
pub(crate) fn create_private_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::File::create(path)
    }
}

fn backup_targz(path: &std::path::Path, db: &Db, header: &BackupHeader) -> Result<()> {
    use std::io::Write;

    // Stage entries.jsonl to a temp file so we know its size for the tar
    // header without buffering the whole dataset in memory. The metadata
    // header line goes first, followed by one JSON entry per line — same
    // layout as plain .jsonl backups.
    let temp_path = std::env::temp_dir().join(format!("datum-backup-{}.jsonl", new_hid()));
    let guard = TempFileGuard(temp_path.clone());

    let mut count = 0usize;
    {
        // da-903m: the staging file holds the user's entire plaintext entry
        // set in a shared temp dir. Create it 0600 so another local user
        // can't read the export during the backup window. (`{hid}` already
        // makes the name unpredictable; this closes the read permission.)
        let temp_file = create_private_file(&guard.0)
            .with_context(|| format!("Creating staging file: {}", guard.0.display()))?;
        let mut writer = std::io::BufWriter::new(temp_file);
        writeln!(writer, "{}", serde_json::to_string(header)?)?;
        db.visit_all_entries::<anyhow::Error, _>(|entry| {
            let line = serde_json::to_string(&entry)?;
            writeln!(writer, "{line}")?;
            count += 1;
            Ok(())
        })?;
        writer.flush()?;
    }

    let manifest = serde_json::json!({
        "archive_version": BACKUP_ARCHIVE_VERSION,
        "schema_version": header.schema_version,
        "timestamp": header.backup_time,
        "source_db": header.source_db,
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

/// Result of a restore for inspection by tests and the CLI printer. The
/// `warnings` field carries human-readable notes about header state
/// (legacy/missing, entry-count drift, etc.) — printed by `cmd_restore`
/// after the outcome summary.
///
/// Field names describe the *committed* outcome. In dry-run mode
/// (`dry_run == true`) they carry the **projected** outcome — i.e.
/// "would be inserted / updated / skipped" — from the real writes in a
/// transaction that is rolled back.
#[derive(Debug, Default)]
pub(crate) struct RestoreReport {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    /// Live entries a replace tombstoned (would tombstone, in a dry run)
    /// because the backup lacks them. Always 0 for `--merge`.
    pub deleted: usize,
    /// A replace (the default), not a `--merge`.
    pub replace: bool,
    /// What a `--hard` restore dropped (would drop, in a dry run); `None`
    /// for any other restore.
    pub hard: Option<datum_core::db::HardReplaceCounts>,
    /// Schema version the backup declares. `None` for headerless legacy
    /// backups, where we optimistically assume v1.
    pub header_schema_version: Option<u32>,
    pub header_source_db: Option<String>,
    pub header_entry_count: Option<usize>,
    pub warnings: Vec<String>,
    /// Slugs of the entries a `--merge` merged a backup entry with, one
    /// holding its slug or its state's instant, as a sync merges two
    /// (da-4ldvb): what the pair gives differently is in `datum conflicts`.
    /// Always empty for a replace.
    pub merged: Vec<String>,
    /// True if the counts are projections from a dry-run, not committed writes.
    pub dry_run: bool,
    /// When the backup's schema version is below the binary's, every entry is
    /// run through `migrate_entry_json` before insert. Reported as `(from, to)`
    /// so dry-run output can surface the migration step that would run.
    pub migration: Option<(u32, u32)>,
    /// A replace's fingerprint of the live set it counted (da-2zqad), for
    /// `--if-unchanged`, a `--hard`'s included (da-h3yb3); `None` for a merge.
    pub fingerprint: Option<u64>,
}

impl RestoreReport {
    fn total_processed(&self) -> usize {
        self.inserted + self.updated + self.skipped + self.merged.len()
    }
}

pub fn cmd_restore(args: RestoreArgs, cli_db: Option<&str>, db: &Db) -> Result<()> {
    use std::io::IsTerminal;
    let dry_run = args.dry_run;
    let silent = args.show == ShowFlag::None;
    let json = args.show == ShowFlag::Json;
    let force = args.force;
    let mut mode = Mode::of(&args);
    // The chain a hard restore leaves (da-95y4o), as `db purge` drops it.
    let chain = match mode.hard {
        true => db
            .sync_chain()
            .ok()
            .filter(|c| c.exists() && c.dissolved().is_none())
            .map(|c| c.id),
        false => None,
    };
    // A chained copy says which it means: the whole chain, or itself alone.
    match (&chain, args.leave, args.chain) {
        (Some(c), false, false) => anyhow::bail!(
            "db restore --hard: this copy is in sync chain {c}, whose other copies would keep \
             what it drops. --chain restores every copy in the chain to the backup (--admin-key \
             in a locked chain); --leave leaves the chain for this copy alone, telling it first"
        ),
        (Some(c), _, true) => return restore_chain(&args, cli_db, db, c, mode),
        (None, leave, _) => anyhow::ensure!(
            !leave && !args.chain,
            "db restore --hard: this copy is in no sync chain; --{} is for one that is",
            if leave { "leave" } else { "chain" }
        ),
        _ => {}
    }
    // A hard restore asks as `db purge` does: the database's name, typed
    // (da-rle8g).
    if mode.hard && !dry_run && !args.yes {
        // The counts come from a dry run, which `-` cannot afford: its
        // stdin is the backup, read once — and no TTY to answer on.
        let is_tty = args.path != STDIO_PATH && std::io::stdin().is_terminal();
        let h = if is_tty {
            let dry = Mode {
                dry_run: true,
                ..mode
            };
            let preview = restore_pinned(&args.path, db, dry)?;
            // The name typed is a yes to these counts too (da-h3yb3).
            mode.if_unchanged = preview.fingerprint;
            preview.hard.unwrap_or_default()
        } else {
            Default::default()
        };
        super::db::confirm_by_name(
            "restore --hard",
            &format!(
                "This replaces ALL data in {} with the backup ({} written); dropped: \
                 {} live, {} tombstoned or superseded, {} in history, {} sync ops. It cannot \
                 be undone: the history `undo` reads goes with it.{}",
                super::db::current_db_path(db).unwrap_or_default(),
                h.written,
                h.entries,
                h.tombstones,
                h.history,
                h.ops,
                chain.as_ref().map_or(String::new(), |c| format!(
                    "\nIt leaves sync chain {c} as a new copy; a fresh invite rejoins it."
                ))
            ),
            &mut std::io::stdin().lock(),
            is_tty,
            &super::db::purge_confirmation_token(cli_db, db),
        )?;
    }
    // Told before its ops go, as `db purge --leave` tells it: a member
    // left undeparted is dialled by the chain for good. Refused, it names
    // `sync chain leave`, whose `--force` tells no one.
    if let Some(c) = chain.as_ref().filter(|_| !dry_run) {
        super::sync::depart(db, c, "datum sync chain leave")?;
    }
    // A replace that deletes asks first, on a TTY (da-5fooh). The count
    // comes from a dry run rather than a transaction held open across the
    // question, which would stall every other writer while a person reads.
    if mode.replace
        && !mode.hard
        && !dry_run
        && !args.yes
        && args.path != STDIO_PATH
        && std::io::stdin().is_terminal()
    {
        let preview = restore_pinned(
            &args.path,
            db,
            Mode {
                dry_run: true,
                ..mode
            },
        )?;
        confirm_deletions(&preview, &mut std::io::stdin().lock())?;
        // The yes is to these counts: a write since refuses (da-2zqad).
        mode.if_unchanged = preview.fingerprint;
    }
    let report = restore_pinned(&args.path, db, mode)?;
    crate::cmd::stamp_batch(db);

    if silent {
        return Ok(());
    }

    if json {
        let out = serde_json::json!({
            "dry_run": report.dry_run,
            "scanned": report.total_processed(),
            "would_insert": if report.dry_run { Some(report.inserted) } else { None },
            "would_skip": if report.dry_run { Some(report.skipped) } else { None },
            "would_update": if report.dry_run { Some(report.updated) } else { None },
            "would_delete": if report.dry_run { Some(report.deleted) } else { None },
            "inserted": if report.dry_run { None } else { Some(report.inserted) },
            "updated": if report.dry_run { None } else { Some(report.updated) },
            "skipped": if report.dry_run { None } else { Some(report.skipped) },
            "deleted": if report.dry_run { None } else { Some(report.deleted) },
            "merged": report.merged,
            "fingerprint": report.fingerprint.map(|f| format!("{f:016x}")),
            "mode": if report.hard.is_some() { "hard" } else if report.replace { "replace" } else { "merge" },
            "dropped": report.hard.map(|h| serde_json::json!({
                "entries": h.entries, "tombstones": h.tombstones, "history": h.history,
                "ops": h.ops,
            })),
            "left_chain": chain.filter(|_| report.hard.is_some() && !report.dry_run),
            "header": {
                "schema_version": report.header_schema_version,
                "source_db": report.header_source_db,
                "entry_count": report.header_entry_count,
            },
            "migration": report.migration.map(|(from, to)| serde_json::json!({"from": from, "to": to})),
            "force": force,
            "warnings": report.warnings,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    for w in &report.warnings {
        eprintln!("⚠ {w}");
    }
    if dry_run {
        println!(
            "Dry run: {} entries scanned (no DB changes)",
            report.total_processed()
        );
        println!("  Would insert: {}", report.inserted);
        if let Some(h) = report.hard {
            println!(
                "  Would drop: {} live, {} tombstoned or superseded, {} in history, {} sync ops",
                h.entries, h.tombstones, h.history, h.ops
            );
            if let Some(c) = &chain {
                println!("  Would leave sync chain {c}");
            }
        } else if report.replace {
            println!("  Would update: {}", report.updated);
            println!("  Would skip (already there): {}", report.skipped);
            println!("  Would delete: {}", report.deleted);
        } else {
            println!("  Would skip (already there): {}", report.skipped);
            print_merged(&report.merged, "Would reconcile");
            if force {
                println!("  Would update (--force): {}", report.updated);
            }
        }
        if let Some((from, to)) = report.migration {
            println!("  Schema migration: v{from} → v{to} (applied to every entry)");
        }
    } else if let Some(h) = report.hard {
        println!(
            "✓ Restore --hard: {} written; dropped {} live, {} tombstoned or superseded, {} in history, {} sync ops — it cannot be undone",
            report.inserted, h.entries, h.tombstones, h.history, h.ops
        );
        if let Some(c) = &chain {
            println!(
                "  left sync chain {c}: this copy is a new one, in none; a fresh invite rejoins it"
            );
        }
    } else if report.replace {
        println!(
            "✓ Restore: {} inserted, {} updated, {} skipped, {} deleted",
            report.inserted, report.updated, report.skipped, report.deleted
        );
    } else {
        println!(
            "✓ Restore: {} inserted, {} updated, {} skipped",
            report.inserted, report.updated, report.skipped
        );
        print_merged(&report.merged, "Reconciled");
    }

    Ok(())
}

/// `db restore --hard --chain` (da-ncmmj): every copy in the chain restored
/// to the backup. A chain wipe, as `sync chain purge --data` orders it, then
/// the backup written into the emptied copy as a replace, so each entry is
/// an op signed after the wipe and every other copy holds exactly the
/// backup once it next syncs. An entry made before the wipe is dropped
/// wherever it arrives, an offline copy's too; one made after it is kept.
fn restore_chain(
    args: &RestoreArgs,
    cli_db: Option<&str>,
    db: &Db,
    chain: &str,
    mode: Mode,
) -> Result<()> {
    use std::io::{IsTerminal, Read};
    // Read twice, the first time to know it restores before anything is
    // wiped: a backup on stdin is held for the second.
    let piped = match args.path == STDIO_PATH {
        true => {
            let mut bytes = Vec::new();
            std::io::stdin().lock().read_to_end(&mut bytes)?;
            Some(bytes)
        }
        false => None,
    };
    let run = |mode: Mode| match &piped {
        Some(bytes) => restore_from_reader(bytes.as_slice(), db, mode)
            .and_then(|report| {
                db.derive_kit_indices()?;
                Ok(report)
            })
            .map_err(unpinned),
        None => restore_pinned(&args.path, db, mode),
    };
    // A live set changed since `--if-unchanged` counted it stops here too.
    let preview = run(Mode {
        dry_run: true,
        ..mode
    })?;
    let h = preview.hard.unwrap_or_default();
    let said = format!(
        "every copy in sync chain {chain} is restored to the backup ({} written), its own data \
         dropped:\n{}  this copy now, dropping {} live, {} tombstoned or superseded, {} in \
         history, after a backup of its entries beside the database\n\
         An entry a copy made before the restore is dropped as it syncs, offline or not; one \
         made after it is kept.",
        h.written,
        super::sync::wipe_reach(db, "restores to the backup")?,
        h.entries,
        h.tombstones,
        h.history
    );
    let json = args.show == ShowFlag::Json;
    if !json && args.show != ShowFlag::None {
        for w in &preview.warnings {
            eprintln!("⚠ {w}");
        }
    }
    let out = |written: usize, backup: Option<&str>| {
        serde_json::json!({
            "dry_run": args.dry_run, "mode": "chain", "chain": chain, "written": written,
            "would_insert": args.dry_run.then_some(written),
            "dropped": {"entries": h.entries, "tombstones": h.tombstones, "history": h.history},
            "fingerprint": preview.fingerprint.map(|f| format!("{f:016x}")),
            "warnings": preview.warnings, "backup": backup,
        })
    };
    if args.dry_run {
        match args.show {
            ShowFlag::None => {}
            _ if json => println!("{}", serde_json::to_string_pretty(&out(h.written, None))?),
            _ => println!("Dry run (no DB changes): {said}"),
        }
        return Ok(());
    }
    if !args.yes {
        super::db::confirm_by_name(
            "restore --hard --chain",
            &format!("{said}\nIt cannot be undone."),
            &mut std::io::stdin().lock(),
            // A backup piped in took stdin: there is no one to ask on it.
            piped.is_none() && std::io::stdin().is_terminal(),
            &super::db::purge_confirmation_token(cli_db, db),
        )?;
    }
    // The yes was to the live set the preview counted: a write while the
    // question was open refuses, as `--if-unchanged` does (exit 12).
    let live = db.live_set_fingerprint()?;
    if preview.fingerprint.is_some_and(|f| f != live) {
        return Err(unpinned(DatumError::Changed.into()));
    }
    let backup = super::sync::order_wipe(
        db,
        &db.sync_chain()?,
        "data",
        true,
        args.admin_key.as_deref(),
    )?;
    // The wipe emptied this copy; a replace writes the backup into it, each
    // entry an op signed after the wipe.
    let restore = Mode {
        hard: false,
        replace: true,
        if_unchanged: None,
        ..mode
    };
    let report = run(restore)?;
    crate::cmd::stamp_batch(db);
    match args.show {
        ShowFlag::None => {}
        _ if json => println!(
            "{}",
            serde_json::to_string_pretty(&out(report.inserted, Some(&backup)))?
        ),
        _ => println!(
            "✓ Restore --hard --chain: {} written; every other copy in sync chain {chain} is \
             restored to the backup when it next connects; what was here: {backup}",
            report.inserted
        ),
    }
    Ok(())
}

/// Name each live entry a merge reconciled a backup entry with (da-4ldvb):
/// merged into it under one slug, kept over it under one state instant.
fn print_merged(slugs: &[String], verb: &str) {
    if slugs.is_empty() {
        return;
    }
    println!(
        "  {verb} with the live entry holding its id: {} — see `datum conflicts`",
        slugs.len()
    );
    for slug in slugs {
        println!("    {slug}");
    }
}

/// Ask before a replace deletes anything; refuse on anything but yes.
fn confirm_deletions(preview: &RestoreReport, stdin: &mut dyn std::io::BufRead) -> Result<()> {
    use std::io::Write;
    if preview.deleted == 0 {
        return Ok(());
    }
    let mut stderr = std::io::stderr().lock();
    write!(
        stderr,
        "This restore deletes {} entries the backup lacks ({} inserted, {} updated). \
         Continue? [y/N] ",
        preview.deleted, preview.inserted, preview.updated
    )?;
    stderr.flush()?;
    let mut line = String::new();
    stdin.read_line(&mut line)?;
    if !matches!(line.trim(), "y" | "Y" | "yes") {
        anyhow::bail!("db restore: aborted (nothing changed; --merge deletes nothing)");
    }
    Ok(())
}

/// How a restore writes, from its flags.
#[derive(Debug, Clone, Copy)]
struct Mode {
    strategy: ConflictStrategy,
    replace: bool,
    hard: bool,
    dry_run: bool,
    if_unchanged: Option<u64>,
}

impl Mode {
    fn of(args: &RestoreArgs) -> Self {
        Mode {
            strategy: if args.force {
                ConflictStrategy::Update
            } else {
                ConflictStrategy::Ignore
            },
            replace: !args.merge,
            hard: args.hard,
            dry_run: args.dry_run,
            if_unchanged: args.if_unchanged,
        }
    }
}

#[cfg(test)]
fn cmd_restore_with_report(args: RestoreArgs, db: &Db) -> Result<RestoreReport> {
    restore_path(&args.path, db, Mode::of(&args))
}

/// [`restore_path`], with a pin that no longer holds (`--if-unchanged`,
/// da-2zqad) said in one line and made a question unanswered: it was asked
/// about one database and handed another, and datum.day asks it again.
fn restore_pinned(path: &str, db: &Db, mode: Mode) -> Result<RestoreReport> {
    restore_path(path, db, mode).map_err(unpinned)
}

fn unpinned(e: anyhow::Error) -> anyhow::Error {
    match e.downcast_ref() {
        Some(DatumError::Changed) => {
            eprint!("db restore: the database changed since it was counted — nothing changed; count it again");
            super::helpers::Unanswered.into()
        }
        _ => e,
    }
}

fn restore_path(path: &str, db: &Db, mode: Mode) -> Result<RestoreReport> {
    let report = read_backup(path, db, mode)?;
    // A kit index's definition is an entry; its SQLite index is this
    // device's, built from what the restore brought (da-fwwtn).
    db.derive_kit_indices()?;
    Ok(report)
}

fn read_backup(path: &str, db: &Db, mode: Mode) -> Result<RestoreReport> {
    if path == STDIO_PATH {
        return restore_from_reader(std::io::stdin().lock(), db, mode);
    }
    match detect_backup_format(path) {
        BackupFormat::TarGz => restore_targz(path, db, mode),
        BackupFormat::Jsonl => restore_jsonl(path, db, mode),
        BackupFormat::JsonlGz => {
            let file =
                std::fs::File::open(path).with_context(|| format!("Opening backup: {path}"))?;
            let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
            restore_from_reader(std::io::BufReader::new(gz), db, mode)
        }
    }
}

fn restore_jsonl(path: &str, db: &Db, mode: Mode) -> Result<RestoreReport> {
    let file = std::fs::File::open(path).with_context(|| format!("Opening backup: {path}"))?;
    let reader = std::io::BufReader::new(file);
    restore_from_reader(reader, db, mode)
}

fn restore_targz(path: &str, db: &Db, mode: Mode) -> Result<RestoreReport> {
    let file = std::fs::File::open(path).with_context(|| format!("Opening backup: {path}"))?;
    let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let mut archive = tar::Archive::new(gz);
    for entry_res in archive.entries().context("Reading tar archive")? {
        let entry = entry_res.context("Reading tar entry")?;
        let in_path = entry.path().context("Reading tar entry path")?.into_owned();
        if in_path.file_name().and_then(|s| s.to_str()) == Some("entries.jsonl") {
            let reader = std::io::BufReader::new(entry);
            return restore_from_reader(reader, db, mode);
        }
    }
    anyhow::bail!("entries.jsonl not found in archive");
}

/// Peek the first non-empty line. Returns `Some(header)` if it parses as a
/// daydumb backup metadata object; returns `None` (and hands back the line
/// via `leftover`) if it looks like a legacy entry instead. Empty files
/// yield `(None, None)` so the caller can no-op cleanly.
fn read_header_line<R: std::io::BufRead>(
    reader: &mut R,
) -> Result<(Option<BackupHeader>, Option<String>)> {
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .context("Reading first backup line")?;
        if n == 0 {
            return Ok((None, None));
        }
        let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
        if trimmed.trim().is_empty() {
            continue;
        }
        // Only a metadata object carries the sentinel discriminator. Peek
        // the "type" field first so a legacy Entry (which has no `type`
        // key with that value) falls through to the headerless path
        // without us running a full Entry deserialize twice.
        let is_header = serde_json::from_str::<serde_json::Value>(&trimmed)
            .ok()
            .and_then(|v| {
                v.get("type")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            })
            .map(|s| s == BACKUP_HEADER_TYPE)
            .unwrap_or(false);
        if is_header {
            let header: BackupHeader = serde_json::from_str(&trimmed)
                .with_context(|| format!("Parsing backup header: {trimmed}"))?;
            return Ok((Some(header), None));
        }
        return Ok((None, Some(trimmed)));
    }
}

/// Apply per-entry JSON migrations to bring an older-schema backup entry up
/// to the current Entry shape, one arm per destructive Entry-shape change,
/// keyed by `from`. Additive bumps need none: the Entry deserializer's own
/// `#[serde(default)]` fields absorb them.
fn migrate_entry_json(raw: &mut serde_json::Value, from: u32, _to: u32) -> Result<()> {
    // v48 (da-lfsei): `device_id` became `client_id`.
    if from < 48 {
        if let Some(e) = raw.as_object_mut() {
            if let Some(id) = e.remove("device_id") {
                e.insert("client_id".into(), id);
            }
        }
    }
    Ok(())
}

fn restore_from_reader<R: std::io::BufRead>(
    mut reader: R,
    db: &Db,
    mode: Mode,
) -> Result<RestoreReport> {
    let Mode {
        strategy,
        replace,
        hard,
        dry_run,
        if_unchanged,
    } = mode;
    let mut report = RestoreReport {
        dry_run,
        replace,
        hard: hard.then(Default::default),
        ..Default::default()
    };

    let (header, legacy_first) = read_header_line(&mut reader)?;

    // Gate on schema_version before touching the DB.
    let current = datum_core::migrations::CURRENT_SCHEMA_VERSION;
    let backup_version = match &header {
        Some(h) => {
            report.header_schema_version = Some(h.schema_version);
            report.header_source_db = Some(h.source_db.clone());
            report.header_entry_count = Some(h.entry_count);
            if h.schema_version > current {
                anyhow::bail!(
                    "Backup is schema v{}; binary supports up to v{}. \
                     Upgrade daydumb to restore this backup.",
                    h.schema_version,
                    current
                );
            }
            if h.schema_version < current {
                report.warnings.push(format!(
                    "Backup was schema v{}; migrating to v{} during restore",
                    h.schema_version, current
                ));
            }
            h.schema_version
        }
        None => {
            // Empty file → no-op. Non-empty with no header → legacy.
            if legacy_first.is_some() {
                report
                    .warnings
                    .push("Legacy backup without metadata; assuming schema v1".to_string());
            }
            1
        }
    };

    if backup_version < current {
        report.migration = Some((backup_version, current));
    }

    // Decode either the legacy first line or fresh lines from the reader.
    // Parsing streams line-by-line; decoded entries are collected so the
    // whole restore commits as ONE transaction via `insert_entries_batch`
    // (da-e3rr). Per-row autocommit paid a journal sync per entry — measured
    // ~10ms each on ext4, ~97s per 10k entries, minutes for a multi-year
    // backup — where one wrapping tx takes ~1.4s for the same 10k.
    // Batching also upgrades the failure story: a parse error or
    // mid-batch insert failure now rolls back to "nothing restored" instead
    // of leaving a half-restored DB.
    let legacy_iter = legacy_first.into_iter().map(Ok);
    let body_iter = reader.lines();
    let all_lines = legacy_iter.chain(body_iter);

    let mut pending: Vec<Entry> = Vec::new();
    let mut reading = Progress::new("restore: reading", report.header_entry_count);
    let mut read = 0;
    for line in all_lines {
        let line = line.context("Reading backup line")?;
        if line.trim().is_empty() {
            continue;
        }
        reading.tick(read);
        read += 1;
        let mut raw: serde_json::Value =
            serde_json::from_str(&line).with_context(|| format!("Parsing entry JSON: {line}"))?;
        if backup_version < current {
            migrate_entry_json(&mut raw, backup_version, current)?;
        }
        let entry: Entry =
            serde_json::from_value(raw).with_context(|| format!("Deserializing entry: {line}"))?;

        if !replace && edited_since_backup(db, &entry)? {
            report.skipped += 1;
            continue;
        }
        pending.push(entry);
    }
    reading.finish(read);

    if hard {
        // As a replace: a header over zero entries empties the database.
        if header.is_some() || !pending.is_empty() {
            let mut inserting = Progress::new("restore: inserting", Some(pending.len()));
            let counts = db.hard_replace_entries(&pending, !dry_run, if_unchanged, &mut |n| {
                inserting.tick(n)
            })?;
            inserting.finish(pending.len());
            report.inserted = counts.written;
            report.fingerprint = Some(counts.fingerprint);
            report.hard = Some(counts);
        }
    } else if replace {
        // Input with neither a header nor an entry is not a backup of
        // anything, so it replaces nothing; a header over zero entries is
        // a backup of an empty database, and empties this one.
        if header.is_some() || !pending.is_empty() {
            let mut inserting = Progress::new("restore: inserting", Some(pending.len()));
            let counts =
                db.replace_entries(&pending, strategy, !dry_run, if_unchanged, &mut |n| {
                    inserting.tick(n)
                })?;
            inserting.finish(pending.len());
            report.inserted = counts.added;
            report.updated = counts.changed;
            report.skipped = counts.unchanged;
            report.deleted = counts.deleted;
            report.fingerprint = Some(counts.fingerprint);
        }
    } else if !pending.is_empty() {
        // Same per-row semantics as insert_entry_with_strategy (history rows,
        // conflict strategy dispatch), one commit for the whole batch — or
        // none, for a dry run — except an entry a different live one holds
        // the identity of is merged into it (da-4ldvb). Tallies come from
        // the outcomes, so the entry-count integrity check below reflects
        // what actually landed.
        let mut inserting = Progress::new("restore: inserting", Some(pending.len()));
        let outcomes =
            db.merge_entries(&pending, strategy, !dry_run, &mut |n| inserting.tick(n))?;
        inserting.finish(pending.len());
        for outcome in outcomes {
            match outcome {
                InsertOutcome::Inserted { .. } | InsertOutcome::Rekeyed { .. } => {
                    report.inserted += 1
                }
                InsertOutcome::Updated { .. } => report.updated += 1,
                InsertOutcome::Merged { slug } => report.merged.push(slug.0),
                InsertOutcome::Ignored { .. } => report.skipped += 1,
            }
        }
    }

    // Integrity check — warn only, do not abort. A mismatch here usually
    // means the backup file was truncated or concurrently modified; the
    // user still wants whatever we managed to read.
    if let Some(expected) = report.header_entry_count {
        let actual = report.total_processed();
        if expected != actual {
            report.warnings.push(format!(
                "Entry-count mismatch: header declared {expected}, restored {actual}"
            ));
        }
    }

    Ok(report)
}

/// An entry `--merge` leaves alone (da-cv1i8): a live row holds its hid,
/// the durable identity, with other content — it was edited since the
/// backup. A merge only adds, so the live version stays and the entry
/// counts as skipped, where writing it would collide on its slug and abort
/// the whole restore.
fn edited_since_backup(db: &Db, entry: &Entry) -> Result<bool> {
    match db.get_entry(&entry.hid.0) {
        Ok(live) => Ok(live.content_hash != entry.content_hash),
        Err(datum_core::error::DatumError::NotFound(_)) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::{entry::Verb, time::DatumTime};

    fn make_entry(
        field: &str,
        secs_offset: i64,
        value: serde_json::Value,
        extra: Option<serde_json::Value>,
    ) -> datum_core::entry::Entry {
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs_offset);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        Entry::new(field, Verb::Point, Some(value), extra, Some(t), None, "d1")
    }

    fn seed_entries(db: &datum_core::db::Db) {
        // Mix of fields, verbs, and extra_data shapes to exercise round-trip.
        db.insert_entry(&make_entry("mood", 100, serde_json::json!("happy"), None))
            .unwrap();
        db.insert_entry(&make_entry(
            "coffee",
            200,
            serde_json::json!(2),
            Some(serde_json::json!({"roast": "dark"})),
        ))
        .unwrap();
        db.insert_entry(&make_entry(
            "notes",
            300,
            serde_json::json!("hello world"),
            Some(serde_json::json!({"tag": "foo", "nested": {"k": 1}})),
        ))
        .unwrap();
    }

    fn unique_tmp_path(ext: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("datum-test-{}{}", new_hid(), ext))
    }

    fn all_entries_sorted(db: &datum_core::db::Db) -> Vec<datum_core::entry::Entry> {
        // visit_all_entries yields live rows in slug order already.
        let mut entries = Vec::new();
        db.visit_all_entries(|e| {
            entries.push(e);
            Ok::<(), datum_core::error::DatumError>(())
        })
        .unwrap();
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
            assert_eq!(e.state, a.state);
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
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();

        // File is a metadata header line followed by line-per-entry JSONL.
        let contents = std::fs::read_to_string(&path).unwrap();
        let non_empty: Vec<&str> = contents.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            non_empty.len(),
            original.len() + 1,
            "one metadata line + one line per entry"
        );

        // Header: type == daydumb_backup and schema_version tracks the binary.
        let header: serde_json::Value =
            serde_json::from_str(non_empty[0]).expect("first line parses as JSON");
        assert_eq!(header["type"], serde_json::json!("daydumb_backup"));
        assert_eq!(
            header["schema_version"],
            serde_json::json!(datum_core::migrations::CURRENT_SCHEMA_VERSION)
        );
        assert_eq!(header["entry_count"].as_u64(), Some(original.len() as u64));
        assert!(header["backup_time"].as_str().is_some());

        // Remaining lines must parse as full Entries.
        for line in &non_empty[1..] {
            let _entry: datum_core::entry::Entry =
                serde_json::from_str(line).expect("each data line is a parseable Entry");
        }

        // Restore into a fresh db and verify equivalence.
        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            None,
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
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();
        assert!(path.exists(), "tar.gz backup file should exist");

        // Verify the archive contains manifest.json + entries.jsonl, and
        // that entries.jsonl starts with the new daydumb_backup header.
        let f = std::fs::File::open(&path).unwrap();
        let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(f));
        let mut archive = tar::Archive::new(gz);
        let mut seen_manifest = false;
        let mut seen_entries = false;
        let mut manifest_body = String::new();
        let mut entries_body = String::new();
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
                    entry.read_to_string(&mut entries_body).unwrap();
                }
                _ => {}
            }
        }
        assert!(seen_manifest, "manifest.json present in archive");
        assert!(seen_entries, "entries.jsonl present in archive");
        let manifest: serde_json::Value =
            serde_json::from_str(&manifest_body).expect("manifest is valid JSON");
        assert_eq!(manifest["archive_version"], serde_json::json!(1));
        assert_eq!(
            manifest["schema_version"],
            serde_json::json!(datum_core::migrations::CURRENT_SCHEMA_VERSION)
        );
        assert!(
            manifest["timestamp"].as_str().is_some(),
            "manifest has a timestamp"
        );

        let first_line = entries_body
            .lines()
            .next()
            .expect("entries.jsonl non-empty");
        let header: serde_json::Value =
            serde_json::from_str(first_line).expect("entries.jsonl first line is JSON");
        assert_eq!(header["type"], serde_json::json!("daydumb_backup"));
        assert_eq!(
            header["schema_version"],
            serde_json::json!(datum_core::migrations::CURRENT_SCHEMA_VERSION)
        );

        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            None,
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
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();

        // First restore: every row inserted.
        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            None,
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
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            None,
            &dst,
        )
        .unwrap();
        assert_eq!(dst.count_entries().unwrap(), original.len());

        let after: Vec<_> = all_entries_sorted(&dst)
            .into_iter()
            .map(|e| (e.slug.clone(), e.modify_time))
            .collect();
        assert_eq!(
            before, after,
            "idempotent restore must not bump modified_at"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_force_overwrites_conflicting_rows() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        src.insert_entry(&make_entry("mood", 100, serde_json::json!("happy"), None))
            .unwrap();

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();

        // Pre-populate dst with the SAME logical row from a different device.
        // Same content_hash, different client_id.
        let dst = Db::open_in_memory().unwrap();
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let t = DatumTime {
            utc: base + chrono::Duration::seconds(100),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        let dup = Entry::new(
            "mood",
            Verb::Point,
            Some(serde_json::json!("happy")),
            None,
            Some(t),
            None,
            "device-OTHER",
        );
        dst.insert_entry(&dup).unwrap();
        let old_modified = dst.get_entry(&dup.slug).unwrap().modify_time;

        // --force → ConflictStrategy::Update overwrites client_id from "d1" in backup.
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: true,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            None,
            &dst,
        )
        .unwrap();

        let overwritten = dst.get_entry(&dup.slug).unwrap();
        assert_eq!(
            overwritten.client_id, "d1",
            "force must overwrite client_id"
        );
        assert!(
            overwritten.modify_time >= old_modified,
            "force must bump modified_at"
        );

        std::fs::remove_file(&path).ok();
    }

    /// A backup from before v48 names the client `device_id` (da-lfsei);
    /// it restores under `client_id`.
    #[test]
    fn restore_reads_a_pre_v48_backups_device_id_as_the_client_id() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        let e = make_entry("mood", 100, serde_json::json!("happy"), None);
        src.insert_entry(&e).unwrap();
        let path = unique_tmp_path(".jsonl");
        let args = BackupArgs {
            path: path.to_string_lossy().into_owned(),
            overwrite: false,
        };
        cmd_backup(args, None, &src).unwrap();
        let old: Vec<String> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| {
                let mut v: serde_json::Value = serde_json::from_str(l).unwrap();
                let o = v.as_object_mut().unwrap();
                match o.remove("client_id") {
                    Some(id) => o.insert("device_id".into(), id),
                    None => o.insert("schema_version".into(), 47.into()),
                };
                v.to_string()
            })
            .collect();
        std::fs::write(&path, old.join("\n")).unwrap();

        let dst = Db::open_in_memory().unwrap();
        let args = RestoreArgs {
            path: path.to_string_lossy().into_owned(),
            force: false,
            dry_run: false,
            merge: true,
            hard: false,
            leave: false,
            chain: false,
            admin_key: None,
            yes: false,
            if_unchanged: None,
            show: ShowFlag::Default,
        };
        cmd_restore(args, None, &dst).unwrap();
        assert_eq!(dst.get_entry(&e.slug).unwrap().client_id, "d1");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn backup_refuses_to_overwrite_without_flag() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        seed_entries(&db);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &db,
        )
        .unwrap();

        // Second call without --overwrite must error.
        let err = cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &db,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("exists"),
            "expected 'exists' in error, got: {err}"
        );

        // With --overwrite, it succeeds.
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: true,
            },
            None,
            &db,
        )
        .unwrap();

        std::fs::remove_file(&path).ok();
    }

    // ─── S7: header + schema-version gate + auto-migrate on restore ───────

    /// Write a JSONL file by hand so we can exercise the restore-side header
    /// parser with arbitrary schema versions without having to rebuild an
    /// older binary.
    fn write_jsonl(
        path: &std::path::Path,
        header: Option<&serde_json::Value>,
        entries: &[datum_core::entry::Entry],
    ) {
        use std::io::Write;
        let f = std::fs::File::create(path).unwrap();
        let mut w = std::io::BufWriter::new(f);
        if let Some(h) = header {
            writeln!(w, "{}", serde_json::to_string(h).unwrap()).unwrap();
        }
        for e in entries {
            writeln!(w, "{}", serde_json::to_string(e).unwrap()).unwrap();
        }
    }

    #[test]
    fn restore_header_match_produces_no_warnings() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert_eq!(
            report.header_schema_version,
            Some(datum_core::migrations::CURRENT_SCHEMA_VERSION)
        );
        assert_eq!(report.header_entry_count, Some(original.len()));
        assert!(
            report.warnings.is_empty(),
            "same-version restore must not warn, got: {:?}",
            report.warnings
        );
        assert_eq!(report.inserted, original.len());
        assert_entries_equivalent(&original, &all_entries_sorted(&dst));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_older_schema_warns_and_migrates() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Forge a backup with schema_version set one below current. The
        // entries themselves are serialized from today's binary — the
        // identity migrate_entry_json lets them round-trip cleanly.
        let older = datum_core::migrations::CURRENT_SCHEMA_VERSION
            .saturating_sub(1)
            .max(1);
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": older,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len(),
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert_eq!(report.header_schema_version, Some(older));
        assert!(
            report.warnings.iter().any(|w| w.contains("migrating to v")),
            "expected migrating warning, got: {:?}",
            report.warnings
        );
        assert_eq!(report.inserted, original.len());
        assert_entries_equivalent(&original, &all_entries_sorted(&dst));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_newer_schema_refuses() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let newer = datum_core::migrations::CURRENT_SCHEMA_VERSION + 3;
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": newer,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len(),
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        let err = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains(&format!("schema v{newer}")) && msg.contains("Upgrade daydumb"),
            "expected newer-schema refusal message, got: {msg}"
        );
        // No entries should have been touched — the gate fires before any insert.
        assert_eq!(dst.count_entries().unwrap(), 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_headerless_legacy_warns_and_proceeds() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Pre-S7 layout: no metadata header, just entries.
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, None, &original);

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert_eq!(report.header_schema_version, None);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("Legacy backup without metadata")),
            "expected legacy warning, got: {:?}",
            report.warnings
        );
        assert_eq!(report.inserted, original.len());
        assert_entries_equivalent(&original, &all_entries_sorted(&dst));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_entry_count_mismatch_warns_only() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Header claims one more entry than we actually include. Matching
        // schema version so the only divergence is the integrity check.
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": datum_core::migrations::CURRENT_SCHEMA_VERSION,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len() + 1,
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("Entry-count mismatch")),
            "expected integrity warning, got: {:?}",
            report.warnings
        );
        // Mismatch is non-fatal — the rows we had still landed.
        assert_eq!(report.inserted, original.len());

        std::fs::remove_file(&path).ok();
    }

    // ─── S8: restore --dry-run ────────────────────────────────────────────

    #[test]
    fn restore_dry_run_reports_all_as_would_insert_into_empty_db() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        let before = dst.count_entries().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run, "report must flag dry_run");
        assert_eq!(report.inserted, original.len(), "every entry would insert");
        assert_eq!(report.skipped, 0);
        assert_eq!(report.updated, 0);

        // HARD GATE: DB must not have been touched.
        assert_eq!(
            dst.count_entries().unwrap(),
            before,
            "dry-run must not write to DB"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_reports_dedupe_when_rows_already_present() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();

        // Populate dst with the same rows so every content_hash collides.
        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            None,
            &dst,
        )
        .unwrap();
        let pre_count = dst.count_entries().unwrap();
        let pre_modified: Vec<_> = all_entries_sorted(&dst)
            .into_iter()
            .map(|e| (e.slug.clone(), e.modify_time))
            .collect();

        // Dry-run over the same backup should predict every row as a skip.
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run);
        assert_eq!(
            report.skipped, pre_count,
            "every row collides by content_hash"
        );
        assert_eq!(report.inserted, 0);
        assert_eq!(report.updated, 0);

        // Dry-run must not touch the DB even in the --force path.
        let post_modified: Vec<_> = all_entries_sorted(&dst)
            .into_iter()
            .map(|e| (e.slug.clone(), e.modify_time))
            .collect();
        assert_eq!(pre_modified, post_modified);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_reports_would_update_with_force() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);

        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            None,
            &dst,
        )
        .unwrap();
        let pre_count = dst.count_entries().unwrap();

        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: true, // conflict strategy = Update
                dry_run: true,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run);
        assert_eq!(
            report.updated, pre_count,
            "--force flips dedupes into updates"
        );
        assert_eq!(report.skipped, 0);
        assert_eq!(report.inserted, 0);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_works_on_targz() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let path = unique_tmp_path(".tar.gz");
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run);
        assert_eq!(report.inserted, original.len());
        assert_eq!(
            dst.count_entries().unwrap(),
            0,
            "dry-run leaves DB untouched"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_dry_run_surfaces_migration_step() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Forge an older-schema header.
        let current = datum_core::migrations::CURRENT_SCHEMA_VERSION;
        let older = current.saturating_sub(1).max(1);
        if older == current {
            // Can't exercise migration path on a one-version binary; skip.
            return;
        }
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": older,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len(),
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();

        assert!(report.dry_run);
        assert_eq!(report.migration, Some((older, current)));
        assert_eq!(dst.count_entries().unwrap(), 0);
        assert!(
            report.warnings.iter().any(|w| w.contains("migrating to v")),
            "migration warning should also appear in dry-run: {:?}",
            report.warnings
        );

        std::fs::remove_file(&path).ok();
    }

    // ─── da-e3rr: single-transaction restore ──────────────────────────────

    #[test]
    fn restore_failure_mid_file_rolls_back_everything() {
        use datum_core::db::Db;
        use std::io::Write;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        // Valid header + valid entries, then a corrupt line LAST — every
        // valid entry is parsed before the failure. Under the old per-row
        // autocommit they'd all be committed; single-tx restore must leave
        // the DB untouched.
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": datum_core::migrations::CURRENT_SCHEMA_VERSION,
            "backup_time": "2026-06-04T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len() + 1,
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(f, "{{not valid json").unwrap();
        }

        let dst = Db::open_in_memory().unwrap();
        let err = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Parsing entry JSON"),
            "expected parse error, got: {err}"
        );
        assert_eq!(
            dst.count_entries().unwrap(),
            0,
            "failed restore must roll back cleanly — nothing restored"
        );

        std::fs::remove_file(&path).ok();
    }

    fn replace_args(path: &std::path::Path, dry_run: bool) -> RestoreArgs {
        RestoreArgs {
            path: path.to_string_lossy().into_owned(),
            force: false,
            dry_run,
            merge: false,
            hard: false,
            leave: false,
            chain: false,
            admin_key: None,
            yes: false,
            if_unchanged: None,
            show: ShowFlag::Default,
        }
    }

    /// The live set as (hid, content_hash): what "exactly the backup's
    /// entries" means for a replace restore.
    fn live_set(entries: &[datum_core::entry::Entry]) -> Vec<(String, String)> {
        let mut set: Vec<_> = entries
            .iter()
            .map(|e| (e.hid.to_string(), e.content_hash.clone()))
            .collect();
        set.sort();
        set
    }

    fn counts(r: &RestoreReport) -> (usize, usize, usize, usize) {
        (r.inserted, r.updated, r.skipped, r.deleted)
    }

    /// A db that has moved on from its backup (da-5fooh): since it, one
    /// entry was deleted (mood), one edited (coffee) and two logged (walk).
    /// Returns the db, the backup, what it holds, and the two new hids.
    fn diverged_from_backup() -> (
        datum_core::db::Db,
        std::path::PathBuf,
        Vec<datum_core::entry::Entry>,
        Vec<String>,
    ) {
        let db = datum_core::db::Db::open_in_memory().unwrap();
        seed_entries(&db);
        let path = unique_tmp_path(".jsonl");
        cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
            None,
            &db,
        )
        .unwrap();
        let backed = all_entries_sorted(&db);
        let by_field = |f: &str| backed.iter().find(|e| e.field == f).unwrap().clone();
        db.soft_delete_entry(&by_field("mood").slug.0).unwrap();
        let mut coffee = by_field("coffee");
        coffee.state = Some(serde_json::json!(3));
        db.save_existing_entry(&coffee).unwrap();
        let new: Vec<String> = [400, 500]
            .into_iter()
            .map(|t| {
                let e = make_entry("walk", t, serde_json::json!(1), None);
                db.insert_entry(&e).unwrap();
                e.hid.to_string()
            })
            .collect();
        (db, path, backed, new)
    }

    #[test]
    fn restore_replaces_by_default() {
        let (db, path, backed, new) = diverged_from_backup();
        let report = cmd_restore_with_report(replace_args(&path, false), &db).unwrap();
        // mood back, coffee put back, notes untouched, both walks deleted.
        assert_eq!(counts(&report), (1, 1, 1, 2));
        assert_eq!(live_set(&all_entries_sorted(&db)), live_set(&backed));
        for hid in &new {
            let row = db.get_entry_including_deleted(hid).unwrap();
            assert!(row.deleted_at.is_some(), "{hid} is a tombstone, not gone");
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_replace_dry_run_counts_what_the_run_does() {
        let (db, path, _, _) = diverged_from_backup();
        let before = live_set(&all_entries_sorted(&db));
        let dry = cmd_restore_with_report(replace_args(&path, true), &db).unwrap();
        assert_eq!(live_set(&all_entries_sorted(&db)), before, "dry run wrote");
        let real = cmd_restore_with_report(replace_args(&path, false), &db).unwrap();
        assert_eq!(counts(&dry), counts(&real));
        assert_eq!(counts(&dry), (1, 1, 1, 2));
        std::fs::remove_file(&path).ok();
    }

    /// A yes to a dry run's counts acts on what they counted (da-2zqad):
    /// a write between the count and the replace — another tab's — refuses
    /// it whole, and the counts taken again, juice among them, go through.
    #[test]
    fn restore_if_unchanged_refuses_a_write_since_the_count() {
        let (db, path, _, _) = diverged_from_backup();
        let pinned = |fingerprint| RestoreArgs {
            if_unchanged: fingerprint,
            ..replace_args(&path, false)
        };
        let dry = cmd_restore_with_report(replace_args(&path, true), &db).unwrap();
        db.insert_entry(&make_entry("juice", 600, serde_json::json!(1), None))
            .unwrap();
        let before = live_set(&all_entries_sorted(&db));
        let err = cmd_restore_with_report(pinned(dry.fingerprint), &db).unwrap_err();
        assert!(
            matches!(err.downcast_ref(), Some(DatumError::Changed)),
            "{err:?}"
        );
        assert_eq!(
            live_set(&all_entries_sorted(&db)),
            before,
            "a refused replace wrote"
        );
        let again = cmd_restore_with_report(replace_args(&path, true), &db).unwrap();
        assert_eq!(again.deleted, dry.deleted + 1);
        let real = cmd_restore_with_report(pinned(again.fingerprint), &db).unwrap();
        assert_eq!(counts(&real), counts(&again));
        std::fs::remove_file(&path).ok();
    }

    /// A stale pin is a question unanswered (exit 12) on the TTY's preview,
    /// a dry run, as on the run itself — not a bare error (PR #455).
    #[test]
    fn restore_pinned_answers_a_stale_pin_as_unanswered_dry_run_or_not() {
        let (db, path, _, _) = diverged_from_backup();
        for dry_run in [true, false] {
            let mode = Mode {
                if_unchanged: Some(0),
                ..Mode::of(&replace_args(&path, dry_run))
            };
            let err = restore_pinned(path.to_str().unwrap(), &db, mode).unwrap_err();
            assert!(err.is::<crate::commands::helpers::Unanswered>(), "{err:?}");
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn restore_replace_failing_midway_changes_nothing() {
        let (db, _, backed, _) = diverged_from_backup();
        let before = live_set(&all_entries_sorted(&db));
        let tombstones = db.tombstone_ids().unwrap();
        // The backup's entries, then one the db refuses: by then the walks
        // are tombstoned and mood and coffee are back, all in the one tx.
        let mut bad = make_entry("walk", 900, serde_json::json!(1), None);
        bad.field = "bad:field".into();
        let mut entries = backed.clone();
        entries.push(bad);
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, None, &entries);
        let err = cmd_restore_with_report(replace_args(&path, false), &db).unwrap_err();
        assert!(err.to_string().contains("field name"), "{err}");
        assert_eq!(live_set(&all_entries_sorted(&db)), before);
        assert_eq!(db.tombstone_ids().unwrap(), tombstones);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_replace_that_deletes_asks_first() {
        let preview = |deleted| RestoreReport {
            deleted,
            ..Default::default()
        };
        let ask = |deleted, answer: &str| {
            confirm_deletions(
                &preview(deleted),
                &mut std::io::Cursor::new(answer.to_string()),
            )
        };
        assert!(ask(2, "y\n").is_ok());
        assert!(ask(2, "\n").is_err(), "the default is no");
        assert!(ask(2, "").is_err(), "no answer is no");
        assert!(ask(0, "").is_ok(), "nothing to delete, nothing to ask");
    }

    #[test]
    fn restore_merge_deletes_nothing() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        seed_entries(&db);
        let backed = all_entries_sorted(&db);
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, None, &backed);
        db.soft_delete_entry(&backed[0].slug.0).unwrap();
        let walk = make_entry("walk", 400, serde_json::json!(1), None);
        db.insert_entry(&walk).unwrap();
        let mut args = replace_args(&path, false);
        args.merge = true;
        let report = cmd_restore_with_report(args, &db).unwrap();
        assert_eq!(counts(&report), (1, 0, 2, 0));
        assert!(db.get_entry(&walk.slug.0).unwrap().deleted_at.is_none());
        assert_eq!(db.count_entries().unwrap(), backed.len() + 1);
        std::fs::remove_file(&path).ok();
    }

    /// da-cv1i8: an entry edited since the backup (coffee) keeps its live
    /// version and counts as skipped, instead of aborting the whole merge
    /// on its slug — and the dry run says so too. So does one retimed
    /// (notes), which is not restored as a second copy under a new hid.
    #[test]
    fn restore_merge_keeps_an_entry_edited_since_the_backup() {
        let (db, path, backed, _) = diverged_from_backup();
        let notes = backed.iter().find(|e| e.field == "notes").unwrap();
        let filter = datum_core::db::RetimeFilter {
            slug_or_hid: Some(&notes.hid.0),
            ..Default::default()
        };
        db.update_times(&filter, chrono::Duration::hours(1))
            .unwrap();
        let merge = |dry_run| RestoreArgs {
            merge: true,
            hard: false,
            leave: false,
            chain: false,
            admin_key: None,
            ..replace_args(&path, dry_run)
        };
        let dry = cmd_restore_with_report(merge(true), &db).unwrap();
        let real = cmd_restore_with_report(merge(false), &db).unwrap();
        // mood back; coffee and notes skipped; the walks untouched.
        assert_eq!(counts(&real), (1, 0, 2, 0));
        assert_eq!(counts(&dry), counts(&real));
        let coffee = all_entries_sorted(&db)
            .into_iter()
            .filter(|e| e.field == "coffee")
            .collect::<Vec<_>>();
        assert_eq!(coffee.len(), 1);
        assert_eq!(coffee[0].state, Some(serde_json::json!(3)));
        let live: Vec<_> = all_entries_sorted(&db)
            .into_iter()
            .filter(|e| e.field == "notes")
            .collect();
        assert_eq!(live.len(), 1);
        let retimed = notes.occur_time.as_ref().unwrap().utc + chrono::Duration::hours(1);
        assert_eq!(live[0].occur_time.as_ref().unwrap().utc, retimed);
        std::fs::remove_file(&path).ok();
    }

    /// Manual perf pin for da-e3rr (single-tx restore). Not a CI gate —
    /// timing-sensitive — run by hand with:
    /// `cargo test -p datum-commands --release restore_bench_10k -- --ignored --nocapture`
    /// Uses a file-backed DB so per-transaction journal syncs are actually
    /// paid (in-memory DBs would hide the cost being measured). The DB lives
    /// in `temp_dir()` — on systems where /tmp is tmpfs, set TMPDIR to a
    /// real-disk path or the sync cost vanishes and the comparison is
    /// meaningless. Measured on ext4: per-row autocommit ~9.7ms/entry
    /// (~97s per 10k); single-tx batch 1.4s per 10k — ~70x.
    ///
    /// Entries are state-less Points on purpose: a state-bearing entry is
    /// classified `switch` by `derive_verb` and pays a full-field state
    /// read in `detect_for_entry` on every insert — O(n²) across the
    /// batch, which swamps the journal-sync cost this bench pins (~685s
    /// for 10k switch rows vs ~1s for 10k points). That scan is a separate
    /// pre-existing cost, tracked in its own bead; it dominates regardless
    /// of per-row vs single-tx commit.
    #[test]
    #[ignore]
    fn restore_bench_10k() {
        use datum_core::db::Db;
        let n = 10_000usize;
        let entries: Vec<_> = (0..n)
            .map(|i| {
                let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
                let t = DatumTime {
                    utc: base + chrono::Duration::seconds(i as i64),
                    offset_minutes: 0,
                    timezone: Some("UTC".to_string()),
                    uncertainty: None,
                };
                // state=None → Verb::Point: skips the per-insert
                // state scan (see doc comment above).
                Entry::new(
                    "bench",
                    Verb::Point,
                    None,
                    Some(serde_json::json!({"i": i})),
                    Some(t),
                    None,
                    "d1",
                )
            })
            .collect();
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": datum_core::migrations::CURRENT_SCHEMA_VERSION,
            "backup_time": "2026-06-04T00:00:00Z",
            "source_db": "bench",
            "entry_count": n,
        });
        let backup_path = unique_tmp_path(".jsonl");
        write_jsonl(&backup_path, Some(&header), &entries);

        let db_path = unique_tmp_path(".db");
        let dst = Db::open(&db_path.to_string_lossy()).unwrap();
        let start = std::time::Instant::now();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: backup_path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap();
        let elapsed = start.elapsed();
        assert_eq!(report.inserted, n);
        println!("restore_bench_10k: {n} entries in {elapsed:?}");

        std::fs::remove_file(&backup_path).ok();
        drop(dst);
        std::fs::remove_file(&db_path).ok();
    }

    #[test]
    fn restore_dry_run_fails_on_future_schema() {
        use datum_core::db::Db;
        let src = Db::open_in_memory().unwrap();
        seed_entries(&src);
        let original = all_entries_sorted(&src);

        let newer = datum_core::migrations::CURRENT_SCHEMA_VERSION + 3;
        let header = serde_json::json!({
            "type": "daydumb_backup",
            "schema_version": newer,
            "backup_time": "2026-04-22T00:00:00Z",
            "source_db": "forged",
            "entry_count": original.len(),
        });
        let path = unique_tmp_path(".jsonl");
        write_jsonl(&path, Some(&header), &original);

        let dst = Db::open_in_memory().unwrap();
        // "Would fail" case — dry-run propagates the schema-gate error so
        // the CLI exits nonzero before the user commits to a restore.
        let err = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                yes: false,
                if_unchanged: None,
                show: ShowFlag::Default,
            },
            &dst,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains(&format!("schema v{newer}")),
            "dry-run must surface the refusal: {err}"
        );
        assert_eq!(dst.count_entries().unwrap(), 0);

        std::fs::remove_file(&path).ok();
    }
}
