//! `datum undo` and `datum redo` — the two directions along an entry's
//! EntryHistory trail.
//!
//! The trail keeps every version, so neither direction loses anything: `undo`
//! steps back, `redo` steps forward again, and the row each one writes records
//! which way it went. That label is the whole of redo's bookkeeping — `redo`
//! is offered only when the newest step was an `undo`, which is what makes
//! `undo; redo; redo` move once instead of oscillating (da-s6no).
//!
//! Neither command refuses on age. Undo used to guard bare `_LAST` behind a
//! fifteen-minute window (a prompt on a tty, a refusal without one); with a
//! way forward that guard cost more than it saved, so what is left of it is a
//! warning past ten minutes naming how old the change is.

use anyhow::{Context, Result};
use chrono::Utc;
use datum_core::{
    db::Db,
    entry::Entry,
    field_spec::SpecRegistry,
    history::{self, ChangeKind},
    ids::Hid,
    last_docs,
};

use crate::{
    args::{OutputFlagsExt, RedoArgs, UndoArgs},
    commands::helpers::{load_registry_opt, resolve_prefix_or_tombstone, warn_if_stale},
    render::{show_create, show_delete, show_update, OutputArgs},
};

pub fn undo(args: UndoArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);
    let output = args.output()?;

    // Slug-targeted undo. Prefer the history-based revert (D4): back off to
    // the prior recorded version. Fall back to the legacy undelete path when
    // no history exists for the entry (e.g. a pre-D4 tombstone imported from
    // a backup) so the command remains useful for those rows.
    if let Some(id_or_slug) = args.id_or_slug.as_deref() {
        let current = resolve_prefix_or_tombstone(db, id_or_slug)?;
        let latest = history::latest_version(db.conn(), &current.hid)?;

        match (args.to_version, latest) {
            (Some(v), _) => {
                // Bounds-check against the entry's version range [1, latest].
                // `state_at_version` is version-agnostic by design and misbehaves
                // silently at the edges (v=0 reverts to create-state; v>latest
                // no-ops), so the clarity check belongs here where `latest` is
                // known. An out-of-range version must point at the problem, not
                // quietly do the wrong thing (da-96yy).
                let latest_n = latest.unwrap_or(0);
                if v < 1 || v > latest_n {
                    anyhow::bail!("version {v} doesn't exist; entry has {latest_n} version(s)");
                }
                step_to(
                    db,
                    &current,
                    v,
                    ChangeKind::Undo,
                    registry.as_ref(),
                    &output,
                )
            }
            (None, Some(n)) if n >= 2 => step_to(
                db,
                &current,
                n - 1,
                ChangeKind::Undo,
                registry.as_ref(),
                &output,
            ),
            _ => {
                // No prior version to revert to — fall back to legacy undelete
                // so tombstoned entries without history still come back. Use
                // the resolved full slug/hid so prefixes keep working here too.
                db.undelete(&current.slug)?;
                if let Ok(entry) = db.get_entry(&current.slug) {
                    show_step(&current, &entry, registry.as_ref(), &output);
                }
                Ok(())
            }
        }
    } else {
        if args.to_version.is_some() {
            anyhow::bail!("--to-version requires an <id_or_slug> to target");
        }

        // Bare `undo` acts on `_LAST` (the broader mutating-command
        // convention; da-uzxj) whenever the pointer names something that
        // still exists. A pointer naming nothing restorable falls through
        // to the most recent tombstone, which is all a pre-pointer database
        // has to offer.
        if let Some(reference) = pointer_with_live_targets(db) {
            warn_if_stale("undoing a change", reference.time);
            return undo_via_last_docs(db, &reference.hids, &registry, &output);
        }

        match db.most_recent_tombstone()? {
            Some(entry) => {
                warn_if_stale(
                    "undoing a change",
                    entry.deleted_at.unwrap_or_else(Utc::now),
                );
                // Prefer the history-based revert so the undo shows up in the
                // entry's trail; fall back to undelete for legacy tombstones.
                match history::latest_version(db.conn(), &entry.hid)? {
                    Some(n) if n >= 2 => step_to(
                        db,
                        &entry,
                        n - 1,
                        ChangeKind::Undo,
                        registry.as_ref(),
                        &output,
                    ),
                    _ => {
                        db.undelete(&entry.slug)?;
                        let restored = db.get_entry(&entry.slug).unwrap_or_else(|_| entry.clone());
                        show_step(&entry, &restored, registry.as_ref(), &output);
                        Ok(())
                    }
                }
            }
            None => anyhow::bail!("Nothing to undo — no soft-deleted entries found"),
        }
    }
}

/// `datum redo [ID_OR_SLUG]` — the step back that `undo` just took, taken
/// forward again.
///
/// The trail after an undo ends with an `Undo` row whose snapshot IS the state
/// the undo stepped away from, so going forward is the same revert machinery
/// aimed one version later: `state_at_version(latest - 1)` is exactly that
/// snapshot. The new row is tagged `Redo`, so the next `redo` sees that the
/// newest step was not an undo and says there is nothing ahead.
pub fn redo(args: RedoArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);
    let output = args.output()?;

    let targets: Vec<Hid> = match args.id_or_slug.as_deref() {
        Some(id_or_slug) => vec![resolve_prefix_or_tombstone(db, id_or_slug)?.hid],
        None => {
            let reference = pointer_with_live_targets(db)
                .context("Nothing to redo — no entry has been touched yet")?;
            warn_if_stale("redoing a change", reference.time);
            reference.hids
        }
    };

    let mut moved: Vec<Hid> = Vec::new();
    for hid in &targets {
        let current = match resolve_prefix_or_tombstone(db, hid) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("redo {hid}: {e}");
                continue;
            }
        };
        match redo_target_version(db, &current.hid)? {
            Some(version) => {
                step_to(
                    db,
                    &current,
                    version,
                    ChangeKind::Redo,
                    registry.as_ref(),
                    &output,
                )?;
                moved.push(current.hid.clone());
            }
            None => continue,
        }
    }

    if moved.is_empty() {
        anyhow::bail!("Nothing to redo — the last change here was not an undo");
    }
    // `_LAST` follows the entries that moved, the way undo's does, so a
    // second `undo` after a `redo` lands on the same rows.
    let _ = last_docs::set_last_docs(db, &moved);
    Ok(())
}

/// The version `redo` must revert to, or `None` when nothing lies ahead.
///
/// Something lies ahead exactly when the newest row on the trail is an `Undo`:
/// that row's snapshot is the pre-undo state, reachable as the state at the
/// version one below it.
fn redo_target_version(db: &Db, hid: &str) -> Result<Option<u32>> {
    let Some(latest) = history::latest_version(db.conn(), hid)? else {
        return Ok(None);
    };
    if latest < 2 {
        return Ok(None);
    }
    let newest = history::get(db.conn(), hid, latest)?;
    Ok(match newest.map(|row| row.change_kind) {
        Some(ChangeKind::Undo) => Some(latest - 1),
        _ => None,
    })
}

/// Move `current` to the state it held at `version` and echo the step.
fn step_to(
    db: &Db,
    current: &Entry,
    version: u32,
    kind: ChangeKind,
    registry: Option<&SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    let restored = db.revert_entry_to_as(&current.hid, version, kind)?;
    show_step(current, &restored, registry, output);
    Ok(())
}

/// The echo for one step names the action the step PERFORMS, not the command
/// that asked for it: bringing a tombstone back is a CREATE, tombstoning is a
/// DELETE, and anything else is an UPDATE. The same three words the write
/// commands print, so the bar reads the same either way.
fn show_step(before: &Entry, after: &Entry, registry: Option<&SpecRegistry>, output: &OutputArgs) {
    match (before.deleted_at.is_some(), after.deleted_at.is_some()) {
        (_, true) => show_delete(after, registry, output),
        (true, false) => show_create(after, registry, output),
        (false, false) => show_update(after, registry, output),
    }
}

/// The last-docs pointer, but only when at least one hid it names still
/// resolves to a row (live or tombstoned). A pointer into nothing is no
/// better than no pointer at all.
fn pointer_with_live_targets(db: &Db) -> Option<last_docs::LastDocsRef> {
    let reference = last_docs::get_last_docs(db).ok().flatten()?;
    reference
        .hids
        .iter()
        .any(|hid| db.get_entry_including_deleted(hid).is_ok())
        .then_some(reference)
}

/// Bare `undo` path: revert each hid named by the last-docs pointer.
/// Tombstoned hids are restored (history-revert preferred, undelete as
/// fallback); live hids fall back to history (undoing the most recent
/// non-delete change). The pointer itself is rewritten to the restored
/// hids so `_LAST` keeps tracking the user's "current" set.
fn undo_via_last_docs(
    db: &Db,
    hids: &[Hid],
    registry: &Option<SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    if hids.is_empty() {
        anyhow::bail!("Last-docs pointer is empty — nothing to undo");
    }
    let mut restored_hids: Vec<Hid> = Vec::new();
    let mut first_err: Option<anyhow::Error> = None;
    for hid in hids {
        let current = match resolve_prefix_or_tombstone(db, hid) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("undo {hid}: {e}");
                first_err.get_or_insert(e);
                continue;
            }
        };
        let latest = history::latest_version(db.conn(), &current.hid)?;
        let restored: Result<Entry> = match latest {
            Some(n) if n >= 2 => db
                .revert_entry_to_as(&current.hid, n - 1, ChangeKind::Undo)
                .map_err(anyhow::Error::from),
            _ => {
                if current.deleted_at.is_some() {
                    db.undelete(&current.slug)?;
                    db.get_entry(&current.slug).map_err(anyhow::Error::from)
                } else {
                    // Undoing a just-created entry = deleting it. The history
                    // table has only version 1 (the create); no prior version
                    // exists to revert to. Soft-delete instead, which tombstones
                    // the entry — tagged as an Undo so `redo` can bring it back
                    // under the same hid, and so a second `undo` on the same hid
                    // restores it via the revert branch above.
                    db.soft_delete_entry_as(&current.slug, ChangeKind::Undo)?;
                    db.get_entry_including_deleted(&current.slug)
                        .map_err(anyhow::Error::from)
                }
            }
        };
        match restored {
            Ok(entry) => {
                restored_hids.push(entry.hid.clone());
                show_step(&current, &entry, registry.as_ref(), output);
            }
            Err(e) => {
                eprintln!("undo {hid}: {e}");
                first_err.get_or_insert(e);
            }
        }
    }
    if !restored_hids.is_empty() {
        let _ = last_docs::set_last_docs(db, &restored_hids);
    }
    match first_err {
        Some(e) if restored_hids.is_empty() => Err(e),
        _ => Ok(()),
    }
}
