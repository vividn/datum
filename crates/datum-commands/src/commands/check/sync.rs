//! The `sync` checker: this copy's rows read against its ops, and what
//! waits on sync. Under `--fix` it first repairs the entries whose ops have
//! no row (da-53whj, da-t5kxx), and the check then names any left.

use anyhow::Result;
use datum_core::db::Db;

use super::{FixReport, Issue, SyncExamined};

/// What was examined, the findings, and under `fix` what the repair did:
/// of every entry, or of `hids` alone (da-3zq6g).
pub(super) fn collect_sync(
    db: &Db,
    fix: bool,
    hids: Option<&[String]>,
) -> Result<(SyncExamined, Vec<Issue>, Option<FixReport>)> {
    let repaired = match fix {
        true => Some(db.sync_fix_orphans()?),
        false => None,
    };
    let (found, mut issues) = crate::commands::sync_diagnose::fold_issues(db, hids)?;
    let report = repaired.map(|detected| {
        // `found` is read after the repair: the orphans still here are the
        // ones it set out to repair and did not.
        let unrepaired = found.orphans.len();
        FixReport {
            label: "sync_orphan_ops",
            hint: "`datum check sync` names them above.",
            detected,
            applied: detected.saturating_sub(unrepaired),
            still_mismatched: unrepaired,
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
