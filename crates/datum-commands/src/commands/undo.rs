//! `datum undo` and `datum redo` — the two directions along an entry's
//! EntryHistory trail.
//!
//! The trail keeps every version, so neither direction loses anything: `undo`
//! steps back, `redo` steps forward again, and the row each one writes records
//! which way it went. That label is the whole of redo's bookkeeping — `redo`
//! is offered only when the newest step was an `undo`, which is what makes
//! `undo; redo; redo` move once instead of oscillating (da-s6no).
//!
//! Bare, they walk the UNDO STACK instead (da-z0zh3, `datum_core::undo_log`):
//! `undo` reverts the newest command not yet undone, every entry it wrote at
//! once, and the next `undo` the one before it; `redo` re-applies the oldest
//! undone one. Neither is a command on the stack itself, which is what lets a
//! second `undo` keep walking back instead of undoing the first. The stack
//! moves only by writes, so `get foo; undo` reverts the last write, not foo.
//!
//! Neither command refuses on age. Undo used to guard bare `_LAST` behind a
//! fifteen-minute window (a prompt on a tty, a refusal without one); with a
//! way forward that guard cost more than it saved, so what is left of it is a
//! warning past ten minutes naming how old the change is.

use anyhow::{Context, Result};
use datum_core::{
    db::{revert_in_tx, soft_delete_in_tx, Db},
    entry::Entry,
    error::DatumError,
    field_spec::SpecRegistry,
    history::{self, ChangeKind},
    ids::Hid,
    last_docs, undo_log,
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
    // `_UNDO` IS the bare undo, so it takes the stack; with `--to-version`
    // it names entries like any other id.
    let named = args
        .id_or_slug
        .as_deref()
        .filter(|id| *id != datum_core::ids::resolve::UNDO_TOKEN || args.to_version.is_some());
    if let Some(list) = named.map(|list| every_last(db, list)) {
        let list = list.as_str();
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
        undo_stack(db, &registry, &output)
    }
}

/// Bare `undo`: put every entry the action at the cursor wrote back as it was
/// before it, and move the cursor back one.
fn undo_stack(db: &Db, registry: &Option<SpecRegistry>, output: &OutputArgs) -> Result<()> {
    let action = undo_log::cursor(db.conn())?
        .context("Nothing to undo — no command has changed anything here yet")?;
    // One step, one transaction (da-pxsa8): every member moves and the
    // cursor with them, or a failure part-way rolls all of it back.
    let tx = db.conn().unchecked_transaction()?;
    refuse_if_moved(db, &action, "undo")?;
    warn_if_stale("undoing a change", action.time);
    let mut steps: Vec<(Entry, Entry)> = Vec::new();
    for member in &action.members {
        let Some(current) = still_there(db, &member.hid)? else {
            continue;
        };
        let after = if member.first > 1 {
            revert_in_tx(&tx, &member.hid, member.first - 1, ChangeKind::Undo)?
        } else if current.deleted_at.is_none() {
            // The action created it: before it, there was nothing.
            soft_delete_in_tx(&tx, &member.hid, ChangeKind::Undo, None)?;
            db.get_entry_including_deleted(&member.hid)?
        } else {
            continue;
        };
        steps.push((current, after));
    }
    undo_log::set_undone(&tx, action.id, true)?;
    tx.commit()?;
    finish_stack(db, &steps, "undo", registry, output)
}

/// Bare `redo`: put every entry the oldest undone action wrote back as it
/// left them, and move the cursor forward one.
fn redo_stack(
    db: &Db,
    dry_run: bool,
    registry: &Option<SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    let action = undo_log::next_redo(db.conn())?
        .context("Nothing to redo — the last change here was not an undo")?;
    if dry_run {
        return Ok(());
    }
    // One step, one transaction (da-pxsa8): every member moves and the
    // cursor with them, or a failure part-way rolls all of it back.
    let tx = db.conn().unchecked_transaction()?;
    refuse_if_moved(db, &action, "redo")?;
    warn_if_stale("redoing a change", action.time);
    let mut steps: Vec<(Entry, Entry)> = Vec::new();
    for member in &action.members {
        let Some(current) = still_there(db, &member.hid)? else {
            continue;
        };
        // An untracked member's state as the action left it is its version 1.
        let after = revert_in_tx(&tx, &member.hid, member.last.max(1), ChangeKind::Redo)?;
        steps.push((current, after));
    }
    undo_log::set_undone(&tx, action.id, false)?;
    tx.commit()?;
    finish_stack(db, &steps, "redo", registry, output)
}

/// A member's entry as it is now, or `None` when it is gone since
/// (forgotten, say), which leaves it nothing to move. Any other error stops
/// the step: skipping it would report a partial undo as complete.
fn still_there(db: &Db, hid: &Hid) -> Result<Option<Entry>> {
    match db.get_entry_including_deleted(hid) {
        Ok(entry) => Ok(Some(entry)),
        Err(DatumError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Refuse, before anything moves, an action one of whose entries has changed
/// since in a way the stack does not hold — a sync from another copy, or a
/// write outside any command. Reverting it would throw that change away.
fn refuse_if_moved(db: &Db, action: &undo_log::Action, verb: &str) -> Result<()> {
    for member in &action.members {
        let Some((version, kind)) = undo_log::blocker(db.conn(), member)? else {
            continue;
        };
        let hid = &member.hid;
        let name = db
            .get_entry_including_deleted(hid)
            .map(|e| e.slug.to_string())
            .unwrap_or_else(|_| hid.to_string());
        let why = match kind {
            ChangeKind::SyncMerge | ChangeKind::SyncConflictResolve => {
                "a sync brought in a change to it from another copy"
            }
            _ => "it was changed by a write that is not on the undo stack",
        };
        anyhow::bail!(
            "Cannot {verb}: {name} ({hid}) has changed since — {why} (version {version}). \
             Nothing was changed; `datum {verb} {hid}` steps that entry alone."
        );
    }
    Ok(())
}

/// `_LAST` names what a stack step moved, as after any command, and then
/// the steps are echoed. A step that moved nothing says so rather than
/// passing in silence.
fn finish_stack(
    db: &Db,
    steps: &[(Entry, Entry)],
    verb: &str,
    registry: &Option<SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    let moved = moved_hids(steps, &[]);
    if moved.is_empty() {
        eprintln!(
            "{verb}: nothing moved — that command's entries were already as it would leave them"
        );
    } else {
        let _ = last_docs::set_last_docs(db, &pointer_after_step(db, moved));
    }
    show_steps(steps, &[], registry.as_ref(), output);
    Ok(())
}

/// A named `_LAST` is every row the pointer holds, tombstones too, as a
/// comma list: each steps along its own trail.
fn every_last(db: &Db, list: &str) -> String {
    match last_docs::get_last_docs(db) {
        Ok(Some(last)) if list == datum_core::ids::resolve::LAST_TOKEN && !last.hids.is_empty() => {
            let hids: Vec<&str> = last.hids.iter().map(|h| h.as_str()).collect();
            hids.join(",")
        }
        _ => list.to_string(),
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

    let Some(list) = args.id_or_slug.as_deref().map(|list| every_last(db, list)) else {
        return redo_stack(db, args.dry_run, &registry, &output);
    };
    let list = list.as_str();
    // A list's ids that do not resolve are named and make the command fail,
    // after the ones that do have moved — `delete`'s precedent (da-9p3o).
    let mut targets: Vec<Hid> = Vec::new();
    let resolved = for_each_id(db, list, |id| {
        targets.push(resolve_prefix_or_tombstone(db, id)?.hid);
        Ok(())
    });

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
