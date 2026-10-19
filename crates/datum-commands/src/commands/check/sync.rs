//! The `sync` checker: this copy's rows read against its ops, and what
//! waits on sync. Under `--fix` it first deletes the entries whose ops have
//! no row (da-53whj), and the check then names any left.

use anyhow::Result;
use datum_core::db::Db;

use super::{FixReport, Issue, SyncExamined};

/// What was examined, the findings, and under `fix` what the deletion did.
pub(super) fn collect_sync(
    db: &Db,
    fix: bool,
) -> Result<(SyncExamined, Vec<Issue>, Option<FixReport>)> {
    let removed = match fix {
        true => Some(db.sync_drop_orphans()?),
        false => None,
    };
    let (found, mut issues) = crate::commands::sync_diagnose::fold_issues(db)?;
    let report = removed.map(|detected| {
        // `found` is read after the deletion: of the orphans still here, the
        // ones it set out to delete and did not. One a live entry shares an
        // identity with is left on purpose, and no failure.
        let undeleted = found.orphans.iter().filter(|o| o.shares.is_none()).count();
        FixReport {
            label: "sync_orphan_ops",
            hint: "`datum check sync` names them above.",
            detected,
            applied: detected.saturating_sub(undeleted),
            still_mismatched: undeleted,
            leftover_samples: Vec::new(),
        }
    });
    issues.extend(crate::commands::sync::notice_issues(db)?);
    let examined = SyncExamined {
        rows: found.entries,
        ops: found.ops,
    };
    Ok((examined, issues, report))
}
