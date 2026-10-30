//! What a run keeps for the next (da-3zq6g): each checker's findings, and
//! the change it read through (`datum-core`'s `check_kept.sql`). A finding
//! of these checkers is one entry's, read with the field specs, so an entry
//! that was clean and has not changed is clean still: a second run reads
//! again only the entries found last time and those changed since. Where a
//! finding is a field's (`integrity`'s repeats), it reads again only the
//! fields changed since. A field's changed spec reads that field's rows
//! again, the one meta row these findings read; a template's spec, a kit, a
//! chain record, another datum or schema, `--fix`, or a mark the log's cap or
//! another stamp's prune has passed (da-4a3cv, da-tuhig) reads every one.

use std::collections::BTreeSet;

use datum_core::db::{chain::CHAIN_HID, Db, SPEC_FIELD};

use super::Issue;

/// Rows changed since a run: `(hid, field, occur_time)`.
pub(super) type Changed = Vec<(String, String, String)>;

/// Which change this run reads through, and what it ran as. No `through`
/// where none can be kept: `--fix`, or a file holding no change log.
pub(super) struct Plan {
    through: Option<i64>,
    stamp: String,
}

impl Plan {
    pub(super) fn new(db: &Db, fix: bool) -> Plan {
        let delim = crate::config::resolve_id_delimiter(None);
        let schema = db.schema_version().unwrap_or_default();
        Plan {
            through: (!fix).then(|| db.check_through().ok()).flatten(),
            stamp: format!("{} v{schema} {delim}", datum_core::build()),
        }
    }

    /// What `checker` kept, and the rows changed since, `(hid, field,
    /// occur_time)`, an op's field and time `''`: `None` where it must read
    /// everything.
    pub(super) fn since(&self, db: &Db, checker: &str) -> Option<(Vec<String>, Changed)> {
        self.through?;
        let kept = db.check_kept(checker).ok()??;
        if kept.stamp != self.stamp {
            return None;
        }
        let mut changed = db.check_changed(kept.through).ok()??;
        // Another run that kept since moved the mark and may have pruned the
        // log under it: what was read of it is not the whole.
        if db.check_kept(checker).ok()??.through != kept.through {
            return None;
        }
        let mut specs = BTreeSet::new();
        for (hid, field, _) in &changed {
            match (hid.as_str(), field.as_str()) {
                (CHAIN_HID, _) | (_, "kit") => return None,
                (_, SPEC_FIELD) => {
                    specs.insert(hid.clone());
                }
                _ => {}
            }
        }
        // A spec is read by its field's rows: they are read again, with no
        // instant, as no row of them moved. A template's covers fields by
        // name (two `%`, as the `entries_spec_templates` index reads one),
        // and a spec row gone names no field.
        let specs: Vec<String> = specs.into_iter().collect();
        let specs_read = db.entries_of(&specs).ok()?;
        if specs_read.len() < specs.len() {
            return None;
        }
        for spec in specs_read {
            let of = spec.slug.as_str().strip_prefix("spec:")?;
            if of.matches('%').count() >= 2 {
                return None;
            }
            let rows = db.check_field_hids(of).ok()?.into_iter();
            changed.extend(rows.map(|hid| (hid, of.to_string(), String::new())));
        }
        Some((kept.kept, changed))
    }

    /// The hids `checker` must read again: those it kept and those changed
    /// since. `None` is every one.
    pub(super) fn again(&self, db: &Db, checker: &str) -> Option<BTreeSet<String>> {
        let (kept, changed) = self.since(db, checker)?;
        Some(
            kept.into_iter()
                .chain(changed.into_iter().map(|(hid, ..)| hid))
                .collect(),
        )
    }

    /// Keep `kept` as what `checker` found. Kept or not, the run's answer is
    /// the same: a write that cannot land now makes the next run read all.
    pub(super) fn keep(&self, db: &Db, checker: &str, kept: impl IntoIterator<Item = String>) {
        let Some(through) = self.through else {
            return;
        };
        let kept: Vec<String> = kept
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let _ = db.check_keep(checker, &self.stamp, through, &kept);
    }
}

/// The hids `issues` name.
pub(super) fn named(issues: &[Issue]) -> impl Iterator<Item = String> + '_ {
    issues
        .iter()
        .flat_map(|i| &i.entries)
        .map(|e| e.hid.clone())
}
