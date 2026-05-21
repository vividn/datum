//! `datum undo [steps]` — roll back recent writes via the EntryHistory trail.

use anyhow::{Context, Result};
use chrono::Utc;
use datum_core::{db::Db, time::parse_duration};

use crate::{
    args::{OutputFlagsExt, UndoArgs},
    commands::helpers::{load_registry_opt, resolve_prefix_or_tombstone},
    render::{show_create, OutputArgs},
};

pub fn undo(args: UndoArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);

    // Slug-targeted undo. Prefer the history-based revert (D4): back off to
    // the prior recorded version. Fall back to the legacy undelete path when
    // no history exists for the entry (e.g. a pre-D4 tombstone imported from
    // a backup) so the command remains useful for those rows.
    if let Some(id_or_slug) = args.id_or_slug.as_deref() {
        let current = resolve_prefix_or_tombstone(db, id_or_slug)?;
        let latest = datum_core::history::latest_version(db.conn(), &current.hid)
            .map_err(anyhow::Error::from)?;

        let output = args.output()?;
        match (args.to_version, latest) {
            (Some(v), _) => {
                let restored = db
                    .revert_entry_to(&current.hid, v)
                    .map_err(anyhow::Error::from)?;
                show_create(&restored, registry.as_ref(), &output);
                Ok(())
            }
            (None, Some(n)) if n >= 2 => {
                let restored = db
                    .revert_entry_to(&current.hid, n - 1)
                    .map_err(anyhow::Error::from)?;
                show_create(&restored, registry.as_ref(), &output);
                Ok(())
            }
            _ => {
                // No prior version to revert to — fall back to legacy undelete
                // so tombstoned entries without history still come back. Use
                // the resolved full slug/hid so prefixes keep working here too.
                db.undelete(&current.slug).map_err(anyhow::Error::from)?;
                if let Ok(entry) = db.get_entry(&current.slug) {
                    show_create(&entry, registry.as_ref(), &output);
                }
                Ok(())
            }
        }
    } else {
        if args.to_version.is_some() {
            anyhow::bail!("--to-version requires an <id_or_slug> to target");
        }

        // Bare `undo` defaults to `_LAST_WITH_PROTECTION` (matches datum):
        // revert whatever the last command targeted, refusing if the
        // pointer is older than the protection window. `--force` skips
        // the pointer entirely and falls through to the legacy
        // most-recent-tombstone path so existing scripts keep working.
        if !args.force {
            match datum_core::last_docs::get_last_docs(db) {
                Ok(Some(reference))
                    if datum_core::last_docs::within_protection_window(
                        reference.time,
                        Utc::now(),
                    ) =>
                {
                    return undo_via_last_docs(db, &reference.hids, &registry, &args.output()?);
                }
                Ok(Some(_)) => {
                    // Pointer exists but is stale — fall through to the
                    // window-based fallback so legacy `undo --window` and
                    // bare-undo against ancient tombstones still works.
                }
                Ok(None) | Err(_) => {
                    // No pointer (fresh DB or pre-da-e65 schema) — fall through.
                }
            }
        }

        let window = parse_duration(&args.window)
            .with_context(|| format!("Invalid --window value '{}'", args.window))?;

        let target = if args.force {
            db.most_recent_tombstone()?
        } else {
            db.list_recent_tombstones(window)?.into_iter().next()
        };

        match target {
            Some(entry) => {
                // Prefer the history-based revert so the undo shows up in the
                // entry's trail; fall back to undelete for legacy tombstones.
                let latest = datum_core::history::latest_version(db.conn(), &entry.hid)
                    .map_err(anyhow::Error::from)?;
                let output = args.output()?;
                match latest {
                    Some(n) if n >= 2 => {
                        let restored = db
                            .revert_entry_to(&entry.hid, n - 1)
                            .map_err(anyhow::Error::from)?;
                        show_create(&restored, registry.as_ref(), &output);
                    }
                    _ => {
                        db.undelete(&entry.slug).map_err(anyhow::Error::from)?;
                        let restored = db.get_entry(&entry.slug).unwrap_or(entry);
                        show_create(&restored, registry.as_ref(), &output);
                    }
                }
                Ok(())
            }
            None => {
                // Nothing within the window. If --force was off and there's
                // still an older tombstone, nudge the user toward --force;
                // otherwise the store has no tombstones at all.
                if !args.force {
                    if let Some(older) = db.most_recent_tombstone()? {
                        anyhow::bail!(
                            "No deletes within {}; most recent is '{}' (use --force to restore it)",
                            args.window,
                            older.slug
                        );
                    }
                }
                anyhow::bail!("Nothing to undo — no soft-deleted entries found")
            }
        }
    }
}

/// Bare `undo` path: revert each hid named by the last-docs pointer.
/// Tombstoned hids are restored (history-revert preferred, undelete as
/// fallback); live hids fall back to history (undoing the most recent
/// non-delete change). The pointer itself is rewritten to the restored
/// hids so `_LAST` keeps tracking the user's "current" set.
fn undo_via_last_docs(
    db: &Db,
    hids: &[datum_core::ids::Hid],
    registry: &Option<datum_core::field_spec::SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    if hids.is_empty() {
        anyhow::bail!("Last-docs pointer is empty — nothing to undo");
    }
    let mut restored_hids: Vec<datum_core::ids::Hid> = Vec::new();
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
        let latest = datum_core::history::latest_version(db.conn(), &current.hid)
            .map_err(anyhow::Error::from)?;
        let restored: Result<datum_core::entry::Entry> = match latest {
            Some(n) if n >= 2 => db
                .revert_entry_to(&current.hid, n - 1)
                .map_err(anyhow::Error::from),
            _ => {
                if current.deleted_at.is_some() {
                    db.undelete(&current.slug).map_err(anyhow::Error::from)?;
                    db.get_entry(&current.slug).map_err(anyhow::Error::from)
                } else {
                    Err(anyhow::anyhow!(
                        "Nothing to undo for {} — no prior version recorded",
                        current.slug,
                    ))
                }
            }
        };
        match restored {
            Ok(entry) => {
                restored_hids.push(entry.hid.clone());
                show_create(&entry, registry.as_ref(), output);
            }
            Err(e) => {
                eprintln!("undo {hid}: {e}");
                first_err.get_or_insert(e);
            }
        }
    }
    if !restored_hids.is_empty() {
        let _ = datum_core::last_docs::set_last_docs(db, &restored_hids);
    }
    match first_err {
        Some(e) if restored_hids.is_empty() => Err(e),
        _ => Ok(()),
    }
}
