//! The `install` checker (da-uphgn): what `datum install` set up and is
//! gone, read by [`crate::commands::install::findings`]. Warnings all.

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
    #[cfg(feature = "net")]
    let found = crate::commands::install::findings(db.sync_chain().is_ok_and(|c| c.exists()));
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
