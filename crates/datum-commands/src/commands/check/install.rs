//! The `install` checker (da-uphgn): what `datum install` set up and is
//! gone, read by [`crate::commands::install::findings`], and the database's
//! backup daemon not running or failing (da-gbac0). Warnings all.

use datum_core::db::Db;

use super::{Domain, Issue, Severity};

/// `None` — the checker did not run — on a datum `datum install` did not
/// set up: the browser's, and one built in a checkout, have no install to
/// check.
pub(super) fn collect_install(db: &Db) -> Option<Vec<Issue>> {
    // A build without `net` (the browser's) has no install or upgrade.
    #[cfg(not(feature = "net"))]
    let found: Option<Vec<(&'static str, String)>> = {
        let _ = db;
        None
    };
    // A backup daemon is checked whoever installed datum (da-gbac0).
    #[cfg(feature = "net")]
    let found = {
        use crate::commands::{backup_daemon, install};
        let backups = (db.conn().path().filter(|p| !p.is_empty()))
            .map_or_else(Vec::new, |p| backup_daemon::findings(p.as_ref()));
        match install::findings(db.sync_chain().is_ok_and(|c| c.exists())) {
            None if backups.is_empty() => None,
            found => Some(
                found
                    .into_iter()
                    .flatten()
                    .chain(backups)
                    .collect::<Vec<_>>(),
            ),
        }
    };
    Some(
        found?
            .into_iter()
            .map(|(category, message)| Issue {
                domain: Domain::Install,
                severity: Severity::Warning,
                category,
                message,
                count: 1,
                example: None,
                entries: Vec::new(),
                rendered_block: String::new(),
            })
            .collect(),
    )
}
