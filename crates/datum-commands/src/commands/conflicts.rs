//! `datum conflicts` — triage for the two conflict kinds sync-pull files
//! when two devices claim one identity (da-ttsm; scope per da-q0fm).
//!
//! Deliberately *not* a lever over the whole `integrity_warnings` table.
//! da-z46t removed that surface: a double_open or an orphan_end is
//! resolved by looking at the data and fixing it (`datum check` shows
//! them, `datum edit` / `datum delete` clears them), and a resolution
//! menu would suggest otherwise. `lww_overwrite` and
//! `identity_collision` are different — the losing version only exists
//! in the warning row, so someone has to say which one wins. That is the
//! whole job of this command.
//!
//! - `list [--all]` — the sync-conflict queue, pending-only by default,
//!   each row showing both versions so the choice can be made from it.
//! - `resolve <HID> (--take-local | --take-remote | --keep-both)` — record
//!   the decision and rewrite the entry row to the winning snapshot.

use anyhow::Result;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::integrity::{IntegrityKind, IntegrityWarning, ListFilter, Resolution};

use crate::args::{ConflictsCommand, ConflictsKeep, ConflictsListArgs, ConflictsResolveArgs};

/// The kinds this command owns. Both are only ever filed with
/// `detected_by = sync_pull`; every other kind belongs to `datum check`.
const SYNC_KINDS: [IntegrityKind; 2] = [
    IntegrityKind::LwwOverwrite,
    IntegrityKind::IdentityCollision,
];

pub fn conflicts(cmd: ConflictsCommand, db: &Db) -> Result<()> {
    match cmd {
        ConflictsCommand::List(args) => list(args, db),
        ConflictsCommand::Resolve(args) => resolve(args, db),
    }
}

fn sync_conflicts(db: &Db, pending_only: bool) -> Result<Vec<IntegrityWarning>> {
    let mut rows = Vec::new();
    for kind in SYNC_KINDS {
        rows.extend(db.list_integrity_warnings(&ListFilter {
            kind: Some(kind),
            pending: if pending_only { Some(true) } else { None },
        })?);
    }
    // `list` is newest-first per kind; re-sort so the merged view is too.
    rows.sort_by_key(|w| std::cmp::Reverse(w.detected_at));
    Ok(rows)
}

/// One side of a conflict, compact enough to sit under its row: what the
/// entry says and when that version was last touched.
fn side(label: &str, e: &Entry) -> String {
    let state = match e.state.as_ref() {
        Some(v) => v.to_string(),
        None => "—".to_string(),
    };
    format!(
        "    {label:<7} {slug}  state={state}  modified={modified}",
        slug = e.slug,
        modified = e.modify_time.format("%Y-%m-%d %H:%M"),
    )
}

fn list(args: ConflictsListArgs, db: &Db) -> Result<()> {
    let rows = sync_conflicts(db, !args.all)?;
    if rows.is_empty() {
        println!(
            "No {}sync conflicts. (`datum check` reports integrity findings.)",
            if args.all { "" } else { "pending " }
        );
        return Ok(());
    }

    for w in &rows {
        println!(
            "{hid}  {kind}  {field}  detected {detected}  {status}",
            hid = w.hid,
            kind = w.kind.as_str(),
            field = w.local_entry.field,
            detected = w
                .detected_at
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M"),
            status = w.resolution.as_deref().unwrap_or("pending"),
        );
        println!("{}", side("local", &w.local_entry));
        match w.remote_entry.as_ref() {
            Some(remote) => println!("{}", side("remote", remote)),
            None => println!("    remote  (not recorded)"),
        }
    }
    if rows.iter().any(|w| w.is_pending()) {
        println!(
            "\nResolve with: datum conflicts resolve <hid> \
             (--take-local | --take-remote | --keep-both)"
        );
    }
    Ok(())
}

fn resolve(args: ConflictsResolveArgs, db: &Db) -> Result<()> {
    // Refuse hids outside this command's scope rather than silently
    // resolving an integrity finding through a sync-shaped lever.
    if let Some(w) = db.get_integrity_warning(&args.hid)? {
        if !SYNC_KINDS.contains(&w.kind) {
            anyhow::bail!(
                "{hid} is a {kind} finding, not a sync conflict — fix the data \
                 (see `datum check`); resolving it by picking a side would not \
                 change what is wrong",
                hid = w.hid,
                kind = w.kind.as_str(),
            );
        }
    }
    let resolution = match args.keep {
        ConflictsKeep::Local => Resolution::TakeLocal,
        ConflictsKeep::Remote => Resolution::TakeRemote,
        ConflictsKeep::Both => Resolution::KeepBoth,
    };
    let resolved = db.resolve_integrity_warning(&args.hid, resolution)?;
    println!(
        "Resolved {hid} ({kind}) on {field}: {resolution}",
        hid = resolved.hid,
        kind = resolved.kind.as_str(),
        field = resolved.local_entry.field,
    );
    Ok(())
}
