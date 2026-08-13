//! GenerationMismatch recovery — Option C (da-74xx).
//!
//! When the relay returns `409 generation_mismatch{expected, found,
//! generation_started_at}`, this module orchestrates the client-side
//! recovery: partition local entries into pre-reinit and post-reinit by
//! `generation_started_at`, optionally export the pre-reinit-unsynced
//! subset to a JSONL backup, delete pre-reinit entries, reset the pull
//! watermark, and adopt the new generation. The caller then re-runs
//! `sync_once` and pulls the cluster's post-reinit snapshot.
//!
//! Semantics (vividn, 2026-06-01):
//! - **Pre-reinit** local entries (`modify_time < generation_started_at`)
//!   are stale state — discarded. The user explicitly asked for the
//!   cluster's post-reinit state to be authoritative.
//! - **Post-reinit** local entries (`modify_time >= generation_started_at`)
//!   are fresh work made after the reinit but before this device caught
//!   up. Preserved; LWW reconciles them against the re-pulled snapshot.
//! - **Pre-reinit-unsynced** is a *subset* of pre-reinit (also gated on
//!   `modify_time_ms > push_mark`). These are the user's most-recent
//!   pre-reinit edits — the most likely "I forgot to sync this" data, and
//!   the only set offered for JSONL export before deletion.

use anyhow::Result;
use datum_core::db::Db;
use datum_sync_client::sync_state;
use std::path::PathBuf;

use crate::commands::sync::Prompter;

pub use datum_sync_client::sync_state::UnsyncedPartition;

/// What the recovery actually did. Returned to the caller so it can
/// re-issue the sync request that mismatched and report a summary.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Number of pre-reinit entries deleted from the local DB.
    pub pre_reinit_deleted: u64,
    /// Number of post-reinit entries preserved (re-pushed on next sync).
    pub post_reinit_kept: u64,
    /// Path the pre-reinit-unsynced subset was exported to, if any.
    pub export_path: Option<PathBuf>,
    /// Number of entries written to the export.
    pub exported_count: u64,
}

/// Recover from a `GenerationMismatch` by aligning the local DB with the
/// cluster's new generation: partition unsynced edits, prompt the user,
/// optionally export the recoverable subset, delete pre-reinit entries,
/// reset the pull watermark, adopt `expected_generation`. The caller is
/// responsible for re-running the sync afterwards.
///
/// `prompter.confirm` provides the y/N gate. Returning `Err(...)` means
/// the user declined (or no `Prompter` was available, headless); the
/// caller surfaces that as a sync failure rather than silently mutating.
pub fn run(
    db: &Db,
    expected_generation: u64,
    generation_started_at: &str,
    prompter: &mut dyn Prompter,
) -> Result<RecoveryReport> {
    run_with_export_dir(
        db,
        expected_generation,
        generation_started_at,
        prompter,
        None,
    )
}

/// Internal entry point that lets the caller (or tests) override where
/// the JSONL export lands. Production goes through [`run`] which feeds
/// `None` and resolves the path under DATUM_HOME / the XDG data dir.
fn run_with_export_dir(
    db: &Db,
    expected_generation: u64,
    generation_started_at: &str,
    prompter: &mut dyn Prompter,
    export_dir_override: Option<&std::path::Path>,
) -> Result<RecoveryReport> {
    let part = sync_state::partition_unsynced_by_generation(db, generation_started_at)?;

    // Edge case: no pre-reinit data at all (e.g., the local DB only
    // existed after the reinit). Nothing to discard — adopt + reset
    // straight through (still atomic), no prompt.
    if part.pre_reinit_total == 0 {
        sync_state::apply_recovery_mutations(db, generation_started_at, expected_generation)?;
        return Ok(RecoveryReport {
            pre_reinit_deleted: 0,
            post_reinit_kept: part.post_reinit,
            export_path: None,
            exported_count: 0,
        });
    }

    let prompt = format_prompt(&part, expected_generation, generation_started_at);
    if !prompter.confirm(&prompt)? {
        anyhow::bail!(
            "recovery aborted: the local pre-reinit data is intact, re-run \
             `datum sync` to retry or back up manually first"
        );
    }

    // Export the recoverable pre-reinit-unsynced subset before deletion.
    let (export_path, exported_count) = if part.pre_reinit_unsynced > 0 {
        let path = match export_dir_override {
            Some(dir) => {
                std::fs::create_dir_all(dir)?;
                dir.join(recovery_export_filename())
            }
            None => recovery_export_path()?,
        };
        let n = sync_state::export_pre_reinit_unsynced_to_jsonl(db, generation_started_at, &path)?;
        prompter.status(&format!(
            "recovery: exported {n} pre-reinit edit{s} to {}",
            path.display(),
            s = if n == 1 { "" } else { "s" },
        ));
        (Some(path), n)
    } else {
        (None, 0)
    };

    // Delete pre-reinit entries, rewind the pull watermark, and adopt
    // the new generation — atomically (one BEGIN/COMMIT) so a process
    // death partway through can't leave the DB half-recovered
    // (Copilot review on PR #8).
    let deleted =
        sync_state::apply_recovery_mutations(db, generation_started_at, expected_generation)?;

    Ok(RecoveryReport {
        pre_reinit_deleted: deleted,
        post_reinit_kept: part.post_reinit,
        export_path,
        exported_count,
    })
}

fn format_prompt(part: &UnsyncedPartition, expected: u64, ts: &str) -> String {
    let pre = part.pre_reinit_total;
    let post = part.post_reinit;
    let unsynced = part.pre_reinit_unsynced;
    if unsynced > 0 {
        format!(
            "Cluster was reinit'd by another device at {ts} (new generation: {expected}). \
             {pre} pre-reinit edit{ps} will be discarded ({unsynced} unsynced edit{us} \
             will be EXPORTED to a JSONL backup first); {post} post-reinit edit{pos} \
             will be kept. Proceed?",
            ps = plural(pre),
            us = plural(unsynced),
            pos = plural(post),
        )
    } else {
        format!(
            "Cluster was reinit'd by another device at {ts} (new generation: {expected}). \
             {pre} pre-reinit edit{ps} will be discarded; {post} post-reinit edit{pos} \
             will be kept. Proceed?",
            ps = plural(pre),
            pos = plural(post),
        )
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Compute a unique recovery-export path under DATUM_HOME (or the XDG
/// data dir). The filename embeds an RFC 3339-ish UTC timestamp so
/// repeated recoveries don't collide.
fn recovery_export_path() -> Result<PathBuf> {
    let dir = crate::config::data_dir()?;
    Ok(dir.join(recovery_export_filename()))
}

fn recovery_export_filename() -> String {
    // Millisecond precision so two recoveries within the same second
    // don't collide on the export path (Copilot review on PR #8); the
    // export fn refuses to overwrite via `create_new` regardless.
    format!(
        "recovery-{}.jsonl",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::sync::TestPrompter;
    use chrono::{TimeZone, Utc};
    use datum_core::entry::{Entry, Verb};
    use datum_core::time::DatumTime;
    use datum_sync_client::sync_state;

    /// Build an Entry with `modify_time` (and `create_time`) pinned to a
    /// specific epoch-ms so partitioning around `generation_started_at`
    /// is deterministic.
    fn make_entry_at_ms(field: &str, ms: i64, device: &str) -> Entry {
        let at = Utc.timestamp_millis_opt(ms).single().unwrap();
        let t = DatumTime {
            utc: at,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
        };
        let mut e = Entry::new(
            field,
            Verb::Point,
            Some(serde_json::json!(1)),
            None,
            Some(t),
            None,
            device,
        );
        e.create_time = at;
        e.modify_time = at;
        e
    }

    fn rfc3339_to_ms(s: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .timestamp_millis()
    }

    /// The happy path: pre-reinit-and-pushed + pre-reinit-unsynced both
    /// get deleted, the unsynced subset gets exported, the post-reinit
    /// entry survives, pull_seq is reset, and the new generation is
    /// adopted.
    #[test]
    fn recovery_purges_pre_reinit_exports_unsynced_keeps_post_reinit() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = Db::open_in_memory().unwrap();
        let gen_started_at = "2026-06-01T12:00:00Z";
        let gen_ms = rfc3339_to_ms(gen_started_at);

        // Three entries:
        // - pre-reinit-and-pushed (modify_time_ms < push_mark < gen_ms)
        // - pre-reinit-unsynced (push_mark < modify_time_ms < gen_ms)
        // - post-reinit (modify_time_ms > gen_ms)
        db.insert_entry(&make_entry_at_ms("pre_pushed", gen_ms - 300_000, "dev-A"))
            .unwrap();
        db.insert_entry(&make_entry_at_ms("pre_unsynced", gen_ms - 100_000, "dev-A"))
            .unwrap();
        db.insert_entry(&make_entry_at_ms("post", gen_ms + 100_000, "dev-A"))
            .unwrap();

        // push_mark sits between pre-pushed and pre-unsynced.
        sync_state::advance_push_mark(&db, (gen_ms - 200_000) as u64).unwrap();

        // Sanity-check the partition before recovery.
        let before = sync_state::partition_unsynced_by_generation(&db, gen_started_at).unwrap();
        assert_eq!(before.pre_reinit_total, 2);
        assert_eq!(before.pre_reinit_unsynced, 1);
        assert_eq!(before.post_reinit, 1);

        let mut prompter = TestPrompter::new(Vec::<&str>::new());
        prompter.push_confirm(true);
        let report =
            run_with_export_dir(&db, 7, gen_started_at, &mut prompter, Some(tmp.path())).unwrap();

        assert_eq!(report.pre_reinit_deleted, 2);
        assert_eq!(report.post_reinit_kept, 1);
        assert_eq!(report.exported_count, 1);
        let export_path = report.export_path.expect("export path must be Some");
        let content = std::fs::read_to_string(&export_path).unwrap();
        let line_count = content.lines().count();
        assert_eq!(line_count, 1, "one JSON line per exported entry");
        assert!(
            content.contains("pre_unsynced"),
            "exported JSONL must contain the pre-reinit-unsynced field, got: {content}",
        );

        // Post-recovery state: only the post-reinit entry remains; the
        // pull_seq is reset; the cluster generation is the expected
        // value.
        let after = sync_state::partition_unsynced_by_generation(&db, gen_started_at).unwrap();
        assert_eq!(after.pre_reinit_total, 0);
        assert_eq!(after.post_reinit, 1);
        assert_eq!(sync_state::get_pull_seq(&db).unwrap(), 0);
        assert_eq!(sync_state::get_cluster_generation(&db).unwrap(), 7);
    }

    /// When the user declines the prompt, the local state is left
    /// untouched and the recovery returns an error so the caller
    /// surfaces it to the user.
    #[test]
    fn recovery_user_decline_preserves_state() {
        let db = Db::open_in_memory().unwrap();
        let gen_started_at = "2026-06-01T12:00:00Z";
        let gen_ms = rfc3339_to_ms(gen_started_at);
        db.insert_entry(&make_entry_at_ms("pre", gen_ms - 100_000, "dev-A"))
            .unwrap();

        let mut prompter = TestPrompter::new(Vec::<&str>::new());
        prompter.push_confirm(false);

        let err = run(&db, 5, gen_started_at, &mut prompter).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("recovery aborted"),
            "expected `recovery aborted` in error, got: {msg}",
        );

        // Pre-reinit entry still there; no watermark changes.
        let after = sync_state::partition_unsynced_by_generation(&db, gen_started_at).unwrap();
        assert_eq!(after.pre_reinit_total, 1);
        assert_eq!(sync_state::get_pull_seq(&db).unwrap(), 0);
        assert_eq!(sync_state::get_cluster_generation(&db).unwrap(), 0);
    }

    /// Edge case: no pre-reinit data at all (fresh device that came up
    /// after the reinit). Skip the prompt; adopt + reset straight
    /// through.
    #[test]
    fn recovery_no_pre_reinit_data_skips_prompt() {
        let db = Db::open_in_memory().unwrap();
        let gen_started_at = "2026-06-01T12:00:00Z";
        let gen_ms = rfc3339_to_ms(gen_started_at);
        db.insert_entry(&make_entry_at_ms("post", gen_ms + 100_000, "dev-A"))
            .unwrap();

        // No confirm queued — the orchestrator must not call confirm.
        let mut prompter = TestPrompter::new(Vec::<&str>::new());

        let report = run(&db, 3, gen_started_at, &mut prompter).unwrap();
        assert_eq!(report.pre_reinit_deleted, 0);
        assert_eq!(report.post_reinit_kept, 1);
        assert!(report.export_path.is_none());
        assert_eq!(sync_state::get_cluster_generation(&db).unwrap(), 3);
        assert_eq!(sync_state::get_pull_seq(&db).unwrap(), 0);
        assert!(
            !prompter.events.iter().any(|e| e.starts_with("confirm:")),
            "no confirm should have been called, events: {:?}",
            prompter.events,
        );
    }
}
