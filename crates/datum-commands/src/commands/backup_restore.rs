//! `datum backup` / `datum restore` — JSONL or tar.gz snapshot of the
//! entry set, plus the streaming restore that replays it.
//!
//! Two file shapes are supported:
//! * `.jsonl` — one metadata header line followed by one Entry per line.
//! * `.tar.gz` — a tar archive containing `manifest.json` and `entries.jsonl`
//!   (same line layout as the plain JSONL form).
//!
//! Restore is content-hash idempotent, gates on the `schema_version`
//! recorded in the header, and commits all entries in a single transaction —
//! any failure rolls back to "nothing restored". See `RestoreReport` for the
//! projected/committed outcome contract.

use anyhow::{Context, Result};
use datum_core::{
    db::{ConflictStrategy, Db, InsertOutcome},
    entry::{new_hid, Entry},
};

use crate::{
    args::{BackupArgs, RestoreArgs},
    config,
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
struct BackupHeader {
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

/// On-disk encoding of a backup, picked from the path extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackupFormat {
    Jsonl,
    TarGz,
}

/// Single source of truth for backup-format detection. Both `cmd_backup`
/// and `cmd_restore_with_report` route through this so the extension rules
/// stay in one place.
fn detect_backup_format(path: &str) -> BackupFormat {
    let lower = path.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        BackupFormat::TarGz
    } else {
        BackupFormat::Jsonl
    }
}

/// Resolve a best-effort identifier for the DB being backed up. Prefers the
/// registry name, falls back to the resolved path, and finally to an empty
/// string when neither can be recovered (e.g. in-memory DB in tests).
fn derive_source_db() -> String {
    match config::resolve_db(None) {
        Ok(r) => r
            .name
            .unwrap_or_else(|| r.path.to_string_lossy().into_owned()),
        Err(_) => String::new(),
    }
}

pub fn cmd_backup(args: BackupArgs, db: &Db) -> Result<()> {
    let out_path = std::path::Path::new(&args.path);
    if out_path.exists() && !args.overwrite {
        anyhow::bail!(
            "File exists: {} (pass --overwrite to replace)",
            out_path.display()
        );
    }

    let header = BackupHeader {
        kind: BACKUP_HEADER_TYPE.to_string(),
        schema_version: datum_core::migrations::CURRENT_SCHEMA_VERSION,
        backup_time: chrono::Utc::now().to_rfc3339(),
        source_db: derive_source_db(),
        entry_count: db.count_entries()?,
    };

    match detect_backup_format(&args.path) {
        BackupFormat::TarGz => backup_targz(out_path, db, &header),
        BackupFormat::Jsonl => backup_jsonl(out_path, db, &header),
    }
}

fn backup_jsonl(path: &std::path::Path, db: &Db, header: &BackupHeader) -> Result<()> {
    use std::io::Write;
    let file = std::fs::File::create(path)
        .with_context(|| format!("Creating backup file: {}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    writeln!(writer, "{}", serde_json::to_string(header)?)?;
    let mut count = 0usize;
    db.visit_all_entries::<anyhow::Error, _>(|entry| {
        let line = serde_json::to_string(&entry)?;
        writeln!(writer, "{line}")?;
        count += 1;
        Ok(())
    })?;
    writer.flush()?;
    println!("✓ Backup: {} entries → {}", count, path.display());
    Ok(())
}

/// RAII guard that removes a temp file when dropped (even on panic/error paths).
struct TempFileGuard(std::path::PathBuf);
impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
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
        let temp_file = std::fs::File::create(&guard.0)
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
/// "would be inserted / updated / skipped" — computed from read-only
/// content_hash lookups without touching the DB.
#[derive(Debug, Default)]
pub(crate) struct RestoreReport {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    /// Schema version the backup declares. `None` for headerless legacy
    /// backups, where we optimistically assume v1.
    pub header_schema_version: Option<u32>,
    pub header_source_db: Option<String>,
    pub header_entry_count: Option<usize>,
    pub warnings: Vec<String>,
    /// True if the counts are projections from a dry-run, not committed writes.
    pub dry_run: bool,
    /// When the backup's schema version is below the binary's, every entry is
    /// run through `migrate_entry_json` before insert. Reported as `(from, to)`
    /// so dry-run output can surface the migration step that would run.
    pub migration: Option<(u32, u32)>,
}

impl RestoreReport {
    fn total_processed(&self) -> usize {
        self.inserted + self.updated + self.skipped
    }
}

pub fn cmd_restore(args: RestoreArgs, db: &Db) -> Result<()> {
    let dry_run = args.dry_run;
    let json = args.json;
    let force = args.force;
    let bump_cluster = args.cluster;
    let report = cmd_restore_with_report(args, db)?;

    if json {
        let out = serde_json::json!({
            "dry_run": report.dry_run,
            "scanned": report.total_processed(),
            "would_insert": if report.dry_run { Some(report.inserted) } else { None },
            "would_skip_dedupe": if report.dry_run { Some(report.skipped) } else { None },
            "would_update": if report.dry_run { Some(report.updated) } else { None },
            "inserted": if report.dry_run { None } else { Some(report.inserted) },
            "updated": if report.dry_run { None } else { Some(report.updated) },
            "skipped": if report.dry_run { None } else { Some(report.skipped) },
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
        println!("  Would skip (content_hash dedupe): {}", report.skipped);
        if force {
            println!("  Would update (--force): {}", report.updated);
        }
        if let Some((from, to)) = report.migration {
            println!("  Schema migration: v{from} → v{to} (applied to every entry)");
        }
    } else {
        println!(
            "✓ Restore: {} inserted, {} updated, {} skipped",
            report.inserted, report.updated, report.skipped
        );
    }

    if bump_cluster && !dry_run {
        bump_cluster_after_restore(db)?;
    }
    Ok(())
}

/// da-ws1q sub-5: after a non-dry-run restore, optionally bump the
/// sync cluster's generation so peers see GenerationMismatchError
/// instead of re-pushing their stale snapshot on top of the restored
/// state. No-op when the local DB has no `sync_state.cluster_id` —
/// nothing has ever been synced, so there are no peers to invalidate.
fn bump_cluster_after_restore(db: &Db) -> Result<()> {
    use datum_sync_client::sync_state;

    let Some(cluster_id) = sync_state::get_cluster_id(db).ok().flatten() else {
        eprintln!(
            "⚠ db restore --cluster: no cluster_id recorded locally (never synced); nothing to bump"
        );
        return Ok(());
    };
    match crate::commands::sync::cluster_ops::bump_generation_for_current_server(&cluster_id) {
        Ok(new_generation) => {
            // Stamp the new generation locally so the next push from
            // this device carries it; otherwise we'd hit our own
            // GenerationMismatch on the next sync.
            sync_state::set_cluster_generation(db, new_generation).ok();
            println!("  cluster {cluster_id}: bumped to generation {new_generation}");
            println!("  peers on the old generation will see GenerationMismatchError next sync");
        }
        Err(e) => {
            eprintln!("⚠ db restore: local restore succeeded, but cluster bump failed: {e:#}");
            eprintln!(
                "  re-run `datum sync` once you can reach the relay; peers may otherwise re-push their stale state on top of the restored snapshot"
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_restore_with_report(args: RestoreArgs, db: &Db) -> Result<RestoreReport> {
    let strategy = if args.force {
        ConflictStrategy::Update
    } else {
        ConflictStrategy::Ignore
    };

    match detect_backup_format(&args.path) {
        BackupFormat::TarGz => restore_targz(&args.path, db, strategy, args.dry_run),
        BackupFormat::Jsonl => restore_jsonl(&args.path, db, strategy, args.dry_run),
    }
}

fn restore_jsonl(
    path: &str,
    db: &Db,
    strategy: ConflictStrategy,
    dry_run: bool,
) -> Result<RestoreReport> {
    let file = std::fs::File::open(path).with_context(|| format!("Opening backup: {path}"))?;
    let reader = std::io::BufReader::new(file);
    restore_from_reader(reader, db, strategy, dry_run)
}

fn restore_targz(
    path: &str,
    db: &Db,
    strategy: ConflictStrategy,
    dry_run: bool,
) -> Result<RestoreReport> {
    let file = std::fs::File::open(path).with_context(|| format!("Opening backup: {path}"))?;
    let gz = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let mut archive = tar::Archive::new(gz);
    for entry_res in archive.entries().context("Reading tar archive")? {
        let entry = entry_res.context("Reading tar entry")?;
        let in_path = entry.path().context("Reading tar entry path")?.into_owned();
        if in_path.file_name().and_then(|s| s.to_str()) == Some("entries.jsonl") {
            let reader = std::io::BufReader::new(entry);
            return restore_from_reader(reader, db, strategy, dry_run);
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
/// to the current Entry shape. For the schema range currently supported
/// (v1–v7) the entries table was rewritten wholesale at v2→v3 with no real
/// production data, so no intermediate JSON transforms are needed — the
/// function is an identity pass. Future schema bumps that change Entry
/// serialization should add a match arm here (keyed by `from`).
fn migrate_entry_json(_raw: &mut serde_json::Value, from: u32, to: u32) -> Result<()> {
    if from == to {
        return Ok(());
    }
    // No transforms registered yet. The Entry deserializer's own
    // `#[serde(default)]` fields already absorb additive schema bumps
    // (v3→v7 were all additive or index-only). A destructive Entry-shape
    // change in a future version should branch on `from` here.
    Ok(())
}

fn restore_from_reader<R: std::io::BufRead>(
    mut reader: R,
    db: &Db,
    strategy: ConflictStrategy,
    dry_run: bool,
) -> Result<RestoreReport> {
    let mut report = RestoreReport {
        dry_run,
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
    for line in all_lines {
        let line = line.context("Reading backup line")?;
        if line.trim().is_empty() {
            continue;
        }
        let mut raw: serde_json::Value =
            serde_json::from_str(&line).with_context(|| format!("Parsing entry JSON: {line}"))?;
        if backup_version < current {
            migrate_entry_json(&mut raw, backup_version, current)?;
        }
        let entry: Entry =
            serde_json::from_value(raw).with_context(|| format!("Deserializing entry: {line}"))?;

        if dry_run {
            // Read-only: predict the outcome from a content_hash lookup
            // without opening a write transaction.
            let exists = db.has_entry_with_content_hash(&entry.content_hash)?;
            match (exists, strategy) {
                (false, _) => report.inserted += 1,
                (true, ConflictStrategy::Ignore) => report.skipped += 1,
                (true, ConflictStrategy::Update) | (true, ConflictStrategy::Merge) => {
                    report.updated += 1
                }
                // Rekey would insert a second row sharing content_hash.
                (true, ConflictStrategy::Rekey) => report.inserted += 1,
            }
            continue;
        }

        pending.push(entry);
    }

    if !pending.is_empty() {
        // Same per-row semantics as insert_entry_with_strategy (history rows,
        // conflict strategy dispatch), one commit for the whole batch.
        // Tallies are recomputed from the committed outcomes, so the
        // entry-count integrity check below reflects what actually landed.
        let outcomes = db.insert_entries_batch(&pending, strategy)?;
        for outcome in outcomes {
            match outcome {
                InsertOutcome::Inserted { .. } | InsertOutcome::Rekeyed { .. } => {
                    report.inserted += 1
                }
                InsertOutcome::Updated { .. } | InsertOutcome::Merged { .. } => report.updated += 1,
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
            timezone: "UTC".to_string(),
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
            Some(serde_json::json!({"unit": "cups"})),
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
                json: false,
                cluster: false,
            },
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
                json: false,
                cluster: false,
            },
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
                json: false,
                cluster: false,
            },
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
                json: false,
                cluster: false,
            },
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
            &src,
        )
        .unwrap();

        // Pre-populate dst with the SAME logical row from a different device.
        // Same content_hash, different device_id.
        let dst = Db::open_in_memory().unwrap();
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let t = DatumTime {
            utc: base + chrono::Duration::seconds(100),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
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

        // --force → ConflictStrategy::Update overwrites device_id from "d1" in backup.
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: true,
                dry_run: false,
                json: false,
                cluster: false,
            },
            &dst,
        )
        .unwrap();

        let overwritten = dst.get_entry(&dup.slug).unwrap();
        assert_eq!(
            overwritten.device_id, "d1",
            "force must overwrite device_id"
        );
        assert!(
            overwritten.modify_time >= old_modified,
            "force must bump modified_at"
        );

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
            &db,
        )
        .unwrap();

        // Second call without --overwrite must error.
        let err = cmd_backup(
            BackupArgs {
                path: path.to_string_lossy().into_owned(),
                overwrite: false,
            },
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
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
            },
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
                json: false,
                cluster: false,
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
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        cmd_restore(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                json: false,
                cluster: false,
            },
            &dst,
        )
        .unwrap();
        let pre_count = dst.count_entries().unwrap();

        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: true, // conflict strategy = Update
                dry_run: true,
                json: false,
                cluster: false,
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
            &src,
        )
        .unwrap();

        let dst = Db::open_in_memory().unwrap();
        let report = cmd_restore_with_report(
            RestoreArgs {
                path: path.to_string_lossy().into_owned(),
                force: false,
                dry_run: true,
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
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
    /// classified `switch` by `derive_verb` and pays `last_state_at`'s
    /// full-field scan in `detect_for_entry` on every insert — O(n²) across
    /// the batch, which swamps the journal-sync cost this bench pins
    /// (~685s for 10k switch rows vs ~1s for 10k points). That scan is a
    /// separate pre-existing cost, tracked in its own bead; it dominates
    /// regardless of per-row vs single-tx commit.
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
                    timezone: "UTC".to_string(),
                };
                // state=None → Verb::Point: skips the per-insert
                // `last_state_at` scan (see doc comment above).
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
                json: false,
                cluster: false,
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
                json: false,
                cluster: false,
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
