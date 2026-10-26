//! The `keys` checker (da-p9gf5): a key a `+=` / `-=` changed against its
//! field's record whose value the merged history now gives otherwise
//! ([`Db::key_drift`]). Under `--fix` each is written as a note on its field
//! holding the history's value, the same on every copy, so two copies fixing
//! apart write one value.

use anyhow::Result;
use datum_core::db::{Db, KeyDrift};
use datum_core::entry::{Entry, Verb};
use serde_json::json;

use super::{run_fix_pass, Domain, FixReport, Issue, Severity};

pub(super) fn collect_key_drift(db: &Db, fix: bool) -> Result<(Vec<Issue>, Option<FixReport>)> {
    let mut drift = db.key_drift()?;
    let report = match fix {
        true => {
            let report = run_fix_pass(
                "key_drift",
                "`datum check keys` names them above.",
                &drift,
                |d| write_fix(db, d),
                || Ok(db.key_drift()?.iter().map(sample).collect()),
            )?;
            drift = db.key_drift()?;
            Some(report)
        }
        false => None,
    };
    let issues = drift
        .iter()
        .map(|d| Issue {
            domain: Domain::Entries,
            severity: Severity::Warning,
            category: "key_drift",
            message: format!(
                "{}'s {} reads {}, and its history makes it {}{}. `datum check keys --fix` writes it",
                d.field,
                d.key,
                d.held,
                d.replayed,
                match d.contradicted.as_slice() {
                    [] => String::new(),
                    hids => format!(
                        ": the history before {} does not hold what each read",
                        hids.join(", ")
                    ),
                },
            ),
            count: 1,
            example: None,
            entries: Vec::new(),
            rendered_block: String::new(),
        })
        .collect();
    Ok((issues, report))
}

/// A note on the field setting the key to its history's value: a timeless
/// entry, which the record folds at its create time, after the rest.
fn write_fix(db: &Db, d: &KeyDrift) -> Result<()> {
    let data = json!({ d.key.as_str(): d.replayed });
    let client = crate::config::client_id()?;
    let mut e = Entry::new(&d.field, Verb::Point, None, Some(data), None, None, client);
    e.id_structure = Some("%create_time%".into());
    db.insert_entry(&e)?;
    Ok(())
}

fn sample(d: &KeyDrift) -> (String, String, String) {
    (
        format!("{}::{}", d.field, d.key),
        d.held.to_string(),
        d.replayed.to_string(),
    )
}
