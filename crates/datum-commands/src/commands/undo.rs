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
    commands::helpers::{
        drop_orphaned_bare_specs, for_each_id, is_id_list, load_registry_opt,
        resolve_prefix_or_tombstone, warn_if_stale,
    },
    render::{show_create, show_delete, show_update, OutputArgs},
};

pub fn undo(args: UndoArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);
    let output = args.output()?;

    // Slug-targeted undo. Prefer the history-based revert (D4): back off to
    // the prior recorded version. Fall back to the legacy undelete path when
    // no history exists for the entry (e.g. a pre-D4 tombstone imported from
    // a backup) so the command remains useful for those rows.
    // A list steps each entry back along its OWN trail (da-9p3o); a version
    // number belongs to one trail, so `--to-version` with a list is refused.
    if let Some(list) = args.id_or_slug.as_deref() {
        if args.to_version.is_some() && is_id_list(db, list) {
            anyhow::bail!(
                "--to-version names a version of ONE entry's history; give one id, not the list '{list}'"
            );
        }
        // Every step lands before any echo: a reader closing the pipe ends
        // the process at the print (da-ki5j3).
        let mut steps: Vec<(Entry, Entry)> = Vec::new();
        let resolved = for_each_id(db, list, |id| {
            let current = resolve_prefix_or_tombstone(db, id)?;
            let after = match args.to_version {
                Some(v) => undo_to_version(db, &current, v)?,
                None => step_back(db, &current)?,
            };
            steps.push((current, after));
            Ok(())
        });
        // Undoing a first log takes the spec it wrote with it (da-elv6d).
        let (specs, dropped) =
            drop_orphaned_bare_specs(db, &tombstoned_by(&steps), ChangeKind::Undo);
        show_steps(&steps, &specs, registry.as_ref(), &output);
        dropped?;
        resolved
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
                let restored = step_back(db, &entry)?;
                show_step(&entry, &restored, registry.as_ref(), &output);
                Ok(())
            }
            None => anyhow::bail!("Nothing to undo — no soft-deleted entries found"),
        }
    }
}

/// Move a named entry back to `version` of its own trail.
fn undo_to_version(db: &Db, current: &Entry, version: u32) -> Result<Entry> {
    // Bounds-check against the entry's version range [1, latest].
    // `state_at_version` is version-agnostic by design and misbehaves
    // silently at the edges (v=0 reverts to create-state; v>latest
    // no-ops), so the clarity check belongs here where `latest` is
    // known. An out-of-range version must point at the problem, not
    // quietly do the wrong thing (da-96yy).
    let latest = history::latest_version(db.conn(), &current.hid)?.unwrap_or(0);
    if version < 1 || version > latest {
        anyhow::bail!("version {version} doesn't exist; entry has {latest} version(s)");
    }
    Ok(db.revert_entry_to_as(&current.hid, version, ChangeKind::Undo)?)
}

/// One step back along one entry's trail — the one rule every undo path
/// takes (da-ksxkx), returning what the entry became. With a prior version
/// it reverts to it; with none, a tombstone comes back (a legacy one, say,
/// imported without history) and a live entry that is only its create is
/// deleted — tagged `Undo`, so `redo` brings it back under the same hid and
/// a second `undo` restores it through the revert. Every step addresses the
/// row by hid: a keep-both leaves two rows on one slug (da-4ptyy).
fn step_back(db: &Db, current: &Entry) -> Result<Entry> {
    match history::latest_version(db.conn(), &current.hid)? {
        Some(n) if n >= 2 => Ok(db.revert_entry_to_as(&current.hid, n - 1, ChangeKind::Undo)?),
        _ if current.deleted_at.is_some() => {
            db.undelete(&current.hid)?;
            Ok(db.get_entry(&current.hid)?)
        }
        _ => {
            db.soft_delete_entry_as(&current.hid, ChangeKind::Undo)?;
            Ok(db.get_entry_including_deleted(&current.hid)?)
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

    // A list's ids that do not resolve are named and make the command fail,
    // after the ones that do have moved — `delete`'s precedent (da-9p3o).
    let mut targets: Vec<Hid> = Vec::new();
    let resolved = match args.id_or_slug.as_deref() {
        Some(list) => for_each_id(db, list, |id| {
            targets.push(resolve_prefix_or_tombstone(db, id)?.hid);
            Ok(())
        }),
        None => {
            let reference = pointer_with_live_targets(db)
                .context("Nothing to redo — no entry has been touched yet")?;
            if !args.dry_run {
                warn_if_stale("redoing a change", reference.time);
            }
            targets = reference.hids;
            Ok(())
        }
    };

    let mut steps: Vec<(Entry, Entry)> = Vec::new();
    for hid in &targets {
        let current = match resolve_prefix_or_tombstone(db, hid) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("redo {hid}: {e}");
                continue;
            }
        };
        match redo_target_version(db, &current.hid)? {
            // `--dry-run` asks only whether this would move anything, and
            // one row that would is the answer: exit 0, nothing written.
            Some(_) if args.dry_run => return resolved,
            Some(version) => {
                let after = db.revert_entry_to_as(&current.hid, version, ChangeKind::Redo)?;
                steps.push((current, after));
            }
            None => continue,
        }
    }

    if steps.is_empty() {
        resolved?;
        anyhow::bail!("Nothing to redo — the last change here was not an undo");
    }
    let (specs, dropped) = drop_orphaned_bare_specs(db, &tombstoned_by(&steps), ChangeKind::Redo);
    // `_LAST` follows the entries that moved, the way undo's does, so a
    // second `undo` after a `redo` lands on the same rows.
    let _ = last_docs::set_last_docs(db, &pointer_after_step(db, moved_hids(&steps, &specs)));
    show_steps(&steps, &specs, registry.as_ref(), &output);
    dropped?;
    resolved
}

/// What `_LAST` names after an `undo` or `redo`: the rows it moved, less a
/// spec live again beside a live entry of its field (da-elv6d). While the
/// pair is tombstoned the spec rides in `_LAST`, so the next step brings it
/// back with the entry; live, it would make a bare `get` ambiguous and a
/// bare `update` write to it.
fn pointer_after_step(db: &Db, hids: Vec<Hid>) -> Vec<Hid> {
    let rows: Vec<Entry> = hids
        .iter()
        .filter_map(|hid| db.get_entry_including_deleted(hid).ok())
        .collect();
    let live = |e: &&Entry| e.deleted_at.is_none();
    let fields: Vec<&str> = rows.iter().filter(live).map(|e| e.field.as_str()).collect();
    let beside_its_entry = |e: &&Entry| {
        live(e)
            && e.field == "spec"
            && e.slug
                .0
                .strip_prefix("spec:")
                .is_some_and(|f| fields.contains(&f))
    };
    rows.iter()
        .filter(|e| !beside_its_entry(e))
        .map(|e| e.hid.clone())
        .collect()
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
    let mut steps: Vec<(Entry, Entry)> = Vec::new();
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
        match step_back(db, &current) {
            Ok(entry) => steps.push((current, entry)),
            Err(e) => {
                eprintln!("undo {hid}: {e}");
                first_err.get_or_insert(e);
            }
        }
    }
    // Undoing a first log takes the spec it wrote with it (da-elv6d).
    let (specs, dropped) = drop_orphaned_bare_specs(db, &tombstoned_by(&steps), ChangeKind::Undo);
    let restored_hids = moved_hids(&steps, &specs);
    if !restored_hids.is_empty() {
        let _ = last_docs::set_last_docs(db, &pointer_after_step(db, restored_hids.clone()));
    }
    show_steps(&steps, &specs, registry.as_ref(), output);
    dropped?;
    match first_err {
        Some(e) if restored_hids.is_empty() => Err(e),
        _ => Ok(()),
    }
}

/// The entries a batch of steps tombstoned, for their fields' bare specs.
fn tombstoned_by(steps: &[(Entry, Entry)]) -> Vec<Entry> {
    steps
        .iter()
        .filter(|(before, after)| before.deleted_at.is_none() && after.deleted_at.is_some())
        .map(|(_, after)| after.clone())
        .collect()
}

/// The hids a batch of steps moved, the specs it dropped after them.
fn moved_hids(steps: &[(Entry, Entry)], specs: &[Entry]) -> Vec<Hid> {
    steps
        .iter()
        .map(|(_, after)| &after.hid)
        .chain(specs.iter().map(|s| &s.hid))
        .cloned()
        .collect()
}

/// Echo a batch of steps once every write, `_LAST` included, has landed:
/// a reader closing the pipe ends the process at the print (da-ki5j3).
fn show_steps(
    steps: &[(Entry, Entry)],
    specs: &[Entry],
    registry: Option<&SpecRegistry>,
    output: &OutputArgs,
) {
    for (before, after) in steps {
        show_step(before, after, registry, output);
    }
    for spec in specs {
        show_delete(spec, registry, output);
    }
}
