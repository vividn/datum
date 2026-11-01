//! `datum undo`, `datum redo` and `datum revert` (da-k5vg1).
//!
//! `undo` and `redo` are an editor's: they walk the UNDO STACK
//! (da-z0zh3, `datum_core::undo_log`) and take no names. `undo` reverts the
//! newest command not yet undone, every entry it wrote at once, and the next
//! `undo` the one before it; `redo` re-applies the oldest undone one. Neither is
//! a command on the stack itself, which is what lets a second `undo` keep walking
//! back instead of undoing the first. The stack moves only by writes, so `get
//! foo; undo` reverts the last write, not foo.
//!
//! `revert` is the one that takes names, and an ordinary command that writes: it
//! goes on the stack, so a bare `undo` takes it back and `redo` restores it.
//! `revert <hid>` steps each entry back to its previous state, or `--to-version`
//! to one of `datum history`'s. `revert <field>` takes back the last command on
//! that field that is no revert, as far as it wrote the field and its spec, so a
//! second one goes on to the command before it.
//!
//! Nothing refuses on age. Undo used to guard bare `_LAST` behind a fifteen-minute
//! window (a prompt on a tty, a refusal without one); with a way forward that
//! guard cost more than it saved, so what is left of it is a warning past ten
//! minutes naming how old the change is.

use anyhow::{Context, Result};
use datum_core::{
    db::{revert_in_tx, take_back_in_tx, Db},
    entry::Entry,
    error::DatumError,
    field_spec::SpecRegistry,
    history::{self, ChangeKind},
    ids::Hid,
    last_docs, undo_log,
};

use crate::{
    args::{OutputFlagsExt, RedoArgs, RevertArgs, UndoArgs},
    commands::helpers::{
        drop_orphaned_bare_specs, for_each_id, id_field_named, id_list, is_id_list,
        load_registry_opt, resolve_prefix_or_tombstone, warn_if_stale,
    },
    render::{show_create, show_delete, show_undone, show_update_from, OutputArgs},
};

pub fn undo(args: UndoArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);
    let output = args.output()?;
    undo_stack(db, args.skip_changed, &registry, &output)
}

pub fn revert(args: RevertArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);
    let output = args.output()?;
    let list = every_last(db, &args.id_or_slug);
    let list = list.as_str();
    if let Some(fields) = named_fields(db, list)? {
        if args.to_version.is_some() {
            anyhow::bail!(
                "--to-version names a version of ONE entry's history, and '{list}' is a field; \
                 give an entry's id"
            );
        }
        return revert_fields(db, &fields, &registry, &output);
    }
    // A list steps each entry back along its OWN trail (da-9p3o); a version
    // number belongs to one trail, so `--to-version` with a list is refused.
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
            Some(v) => to_version(db, &current, v)?,
            None => step_back(db, &current)?,
        };
        steps.push((current, after));
        Ok(())
    });
    // Reverting a first log takes the spec it wrote with it (da-elv6d).
    let (specs, dropped) = drop_orphaned_bare_specs(db, &tombstoned_by(&steps), ChangeKind::Revert);
    let moved = moved_hids(&steps, &specs);
    if !moved.is_empty() {
        let _ = last_docs::set_last_docs(db, &pointer_after_step(db, moved));
    }
    show_steps(&steps, &specs, Echo::Undo(None), registry.as_ref(), &output);
    dropped?;
    resolved
}

/// What a step reads before it takes the lock.
struct Step {
    /// The action an undo marks undone.
    action: Option<i64>,
    /// What a `revert <field>` took back, on which field.
    took: Vec<(i64, String)>,
    time: chrono::DateTime<chrono::Utc>,
    skipped: Vec<Hid>,
    report: String,
    members: Vec<(undo_log::Member, Entry)>,
}

/// Bare `undo`: put every entry the action at the cursor wrote back as it was
/// before it, and move the cursor back one. `skip_changed` leaves the entries
/// another copy changed since as they are instead of refusing.
fn undo_stack(
    db: &Db,
    skip_changed: bool,
    registry: &Option<SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    take_back(db, ChangeKind::Undo, registry, output, || {
        step_at_cursor(db, skip_changed)
    })
}

/// `revert <field>[,<field>…]`: the last command on each.
fn revert_fields(
    db: &Db,
    fields: &[String],
    registry: &Option<SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    take_back(db, ChangeKind::Revert, registry, output, || {
        last_on_fields(db, fields)
    })
}

fn take_back(
    db: &Db,
    kind: ChangeKind,
    registry: &Option<SpecRegistry>,
    output: &OutputArgs,
    read: impl Fn() -> Result<Step>,
) -> Result<()> {
    // One step, one transaction (da-pxsa8): every member moves and the
    // cursor with them, or a failure part-way rolls all of it back. What it
    // moves is read before the lock, and moved on the database it was read
    // from (da-8i7f0).
    let (time, skipped, report, steps) = db.compute_then_write_with(
        |_| read(),
        |tx, step: Step| {
            let steps = take_back_in_tx(tx, step.members, kind)?;
            if let Some(id) = step.action {
                undo_log::set_undone(tx, id, &step.skipped)?;
            }
            if !step.took.is_empty() {
                undo_log::set_took(tx, &step.took)?;
            }
            Ok((step.time, step.skipped, step.report, steps))
        },
    )?;
    let doing = match kind {
        ChangeKind::Revert => "reverting a change from",
        _ => "undoing a change from",
    };
    warn_if_stale(doing, time);
    // Said once it is true: a failure above rolls the step back.
    if !report.is_empty() {
        eprintln!("{report}");
    }
    if steps.is_empty() && !skipped.is_empty() {
        // Every entry was left as it is, which the skip already said.
        return Ok(());
    }
    finish_stack(db, &steps, Echo::Undo(Some(time)), registry, output)
}

fn step_at_cursor(db: &Db, skip_changed: bool) -> Result<Step> {
    let action = undo_log::cursor(db.conn())?
        .context("Nothing to undo — no command has changed anything here yet")?;
    let (skipped, report) = refuse_if_moved(db, &action, "undo", skip_changed)?;
    let mut members = Vec::new();
    for member in action.members.iter().filter(|m| !skipped.contains(&m.hid)) {
        if let Some(current) = still_there(db, &member.hid)? {
            members.push((member.clone(), current));
        }
    }
    Ok(Step {
        action: Some(action.id),
        took: Vec::new(),
        time: action.time,
        skipped: [action.skipped, skipped].concat(),
        report,
        members,
    })
}

/// The last command on each field, as far as it wrote that field. A field with
/// nothing to take back, or an entry another copy changed since, refuses the
/// whole step.
fn last_on_fields(db: &Db, fields: &[String]) -> Result<Step> {
    let mut time = None;
    let mut took = Vec::new();
    let mut all: Vec<(undo_log::Member, Entry)> = Vec::new();
    for field in fields {
        let mut below = i64::MAX;
        // An entry a revert already took back has nothing left to move, so
        // the field's last action is the one before it.
        let (action, members) = loop {
            let Some(action) = undo_log::last_on(db.conn(), field, below)? else {
                let so = if fields.len() > 1 {
                    ", so nothing was reverted"
                } else {
                    ""
                };
                anyhow::bail!(
                    "Nothing to revert on field '{field}': no command here has changed it{so}"
                );
            };
            below = action.id;
            let mut members = Vec::new();
            for member in &action.members {
                if let Some(current) = still_there(db, &member.hid)? {
                    if undo_log::would_move(db.conn(), member, &current)? {
                        members.push((member.clone(), current));
                    }
                }
            }
            if !members.is_empty() {
                break (action, members);
            }
        };
        refuse_if_moved(db, &action, "revert", false)?;
        time = time.max(Some(action.time));
        took.push((action.id, field.clone()));
        for (member, current) in members {
            if !all.iter().any(|(m, _)| m.hid == member.hid) {
                all.push((member, current));
            }
        }
    }
    Ok(Step {
        action: None,
        took,
        time: time.unwrap_or_else(chrono::Utc::now),
        skipped: Vec::new(),
        report: String::new(),
        members: all,
    })
}

/// The fields a list names, or `None` when it names entries. A whole hid is its
/// entry. Past that a field wins, as in `get`, over a hid or slug it merely
/// starts, or `revert a` would turn on what hids happen to begin with; a field
/// whose every row is tombstoned is still named, so a second `revert a` says
/// there is nothing left and does not bring the first back.
fn named_fields(db: &Db, list: &str) -> Result<Option<Vec<String>>> {
    let mut fields: Vec<String> = Vec::new();
    let mut ids = Vec::new();
    for token in id_list(db, list) {
        let field = if db.hid_exists(token)? {
            None
        } else {
            match id_field_named(db, token)? {
                Some(field) => Some(field),
                None => db.tombstoned_field_starting(token)?,
            }
        };
        match field {
            Some(field) if !fields.contains(&field) => fields.push(field),
            Some(_) => {}
            None => ids.push(token),
        }
    }
    if fields.is_empty() {
        return Ok(None);
    }
    if ids.is_empty() {
        return Ok(Some(fields));
    }
    if let Some(id) = ids
        .iter()
        .find(|id| resolve_prefix_or_tombstone(db, id).is_err())
    {
        anyhow::bail!("No field or entry '{id}', so nothing was reverted");
    }
    anyhow::bail!(
        "'{list}' names fields ({}) and entries ({}): give one or the other",
        fields.join(", "),
        ids.join(", ")
    )
}

/// Bare `redo`: put every entry the newest undo took back as its action left
/// them, and move the cursor forward one. The entries its undo left as
/// they are (`--skip-changed`) it leaves too: redo re-applies exactly what
/// the undo took back (da-2ahs4).
fn redo_stack(
    db: &Db,
    dry_run: bool,
    registry: &Option<SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    // The refusal reads only, so a dry run exits as the redo would
    // (da-38x8z), and takes no lock.
    let read = |_: &datum_core::db::View| {
        let mut action = undo_log::next_redo(db.conn())?
            .context("Nothing to redo — the last change here was not an undo")?;
        let skipped = std::mem::take(&mut action.skipped);
        action.members.retain(|m| !skipped.contains(&m.hid));
        let all_skipped = action.members.is_empty() && !skipped.is_empty();
        refuse_if_moved(db, &action, "redo", false)?;
        let mut members = Vec::new();
        for member in &action.members {
            if let Some(current) = still_there(db, &member.hid)? {
                members.push((member.clone(), current));
            }
        }
        Ok::<_, anyhow::Error>((action.id, action.time, all_skipped, members))
    };
    if dry_run {
        return db.read(read).map(drop);
    }
    // One step, one transaction (da-pxsa8): every member moves and the
    // cursor with them, or a failure part-way rolls all of it back. What it
    // moves is read before the lock (da-8i7f0).
    let (time, all_skipped, steps) =
        db.compute_then_write_with(read, |tx, (id, time, all_skipped, members)| {
            let mut steps: Vec<(Entry, Entry)> = Vec::new();
            for (member, current) in members {
                // An untracked member's state as the action left it is its
                // version 1.
                let after = revert_in_tx(tx, &member.hid, member.last.max(1), ChangeKind::Redo)?;
                steps.push((current, after));
            }
            undo_log::set_redone(tx, id)?;
            Ok((time, all_skipped, steps))
        })?;
    warn_if_stale("redoing a change from", time);
    if all_skipped {
        eprintln!(
            "redo: the undo left every entry of that command as another copy left it, \
             so there is nothing to re-apply"
        );
        return Ok(());
    }
    finish_stack(db, &steps, Echo::Redo, registry, output)
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

/// Refuse, before anything moves, an action any of whose entries has changed
/// since in a way the stack does not hold — a sync from another copy, or a
/// write outside any command. Reverting it would throw that change away, and
/// a step is the whole action or nothing (da-e3be3), so the refusal names
/// every such entry and what to do about each. With `skip` it returns them,
/// for the step to leave as they are, with the report naming them that the
/// step prints once it has committed.
fn refuse_if_moved(
    db: &Db,
    action: &undo_log::Action,
    verb: &str,
    skip: bool,
) -> Result<(Vec<Hid>, String)> {
    let mut hids = Vec::new();
    let mut lines = Vec::new();
    for member in &action.members {
        let Some((version, kind)) = undo_log::blocker(db.conn(), member)? else {
            continue;
        };
        let hid = &member.hid;
        let entry = db.get_entry_including_deleted(hid).ok();
        let name = entry
            .as_ref()
            .map_or(hid.to_string(), |e| e.slug.to_string());
        let why = match kind {
            ChangeKind::SyncMerge | ChangeKind::SyncConflictResolve => {
                "a sync brought in a change from another copy"
            }
            _ => "a write that is not on the undo stack",
        };
        // What to do about it: settle a conflict, or see what changed.
        let next = match entry.and_then(|e| e.conflicts) {
            Some(_) => "conflicts resolve",
            None => "history",
        };
        lines.push(format!(
            "  {name} ({hid}): {why} (version {version}) — `datum {next} {hid}`"
        ));
        hids.push(hid.clone());
    }
    let (n, of, lines) = (lines.len(), action.members.len(), lines.join("\n"));
    if n == 0 {
        return Ok((hids, String::new()));
    }
    if skip {
        let report = format!("{verb}: left {n} of the {of} entries as they are:\n{lines}");
        return Ok((hids, report));
    }
    // Only the stack's undo walks past them; redo always refuses.
    let rest = match verb {
        "undo" if n < of => format!(
            " `datum undo --skip-changed` takes back the other {} and leaves these as they are.",
            of - n
        ),
        "undo" => " `datum undo --skip-changed` steps past it and takes nothing back.".into(),
        _ => String::new(),
    };
    let whole = match verb {
        "redo" => "redo re-applies the whole command",
        "revert" => "revert takes back all it wrote of the field",
        _ => "undo takes back the whole command",
    };
    anyhow::bail!(
        "Cannot {verb}: {n} of the {of} entries that command wrote changed since:\n{lines}\n\
         {whole} or nothing, and this would throw \
         those changes away, so nothing was changed.{rest}"
    )
}

/// `_LAST` names what a stack step moved, as after any command, and then
/// the steps are echoed. A step that moved nothing says so rather than
/// passing in silence.
fn finish_stack(
    db: &Db,
    steps: &[(Entry, Entry)],
    echo: Echo,
    registry: &Option<SpecRegistry>,
    output: &OutputArgs,
) -> Result<()> {
    let moved = moved_hids(steps, &[]);
    if moved.is_empty() {
        let verb = match echo {
            Echo::Undo(_) => "undo",
            Echo::Redo => "redo",
        };
        eprintln!(
            "{verb}: nothing moved — that command's entries were already as it would leave them"
        );
    } else {
        let _ = last_docs::set_last_docs(db, &pointer_after_step(db, moved));
    }
    show_steps(steps, &[], echo, registry.as_ref(), output);
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

/// Move a named entry back to `version` of its history — the number `datum
/// history` prints, a version per op (da-s4erq).
fn to_version(db: &Db, current: &Entry, version: u32) -> Result<Entry> {
    // An out-of-range version must point at the problem, not quietly do the
    // wrong thing (da-96yy).
    let Some(target) = db.entry_at_version(current, version)? else {
        let latest = db.versions(current)?.len();
        anyhow::bail!("version {version} doesn't exist; entry has {latest} version(s)");
    };
    Ok(db.revert_entry_to_state(current, &target)?)
}

/// One step back along one entry's trail, returning what the entry became.
/// With a prior version it reverts to it, which a second revert steps back from
/// in turn; with none, a tombstone comes back (a legacy one, say, imported without
/// history) and a live entry that is only its create is deleted. Every step
/// addresses the row by hid: a keep-both leaves two rows on one slug (da-4ptyy).
fn step_back(db: &Db, current: &Entry) -> Result<Entry> {
    match history::latest_version(db.conn(), &current.hid)? {
        Some(n) if n >= 2 => Ok(db.revert_entry_to_as(&current.hid, n - 1, ChangeKind::Revert)?),
        _ if current.deleted_at.is_some() => {
            db.undelete(&current.hid)?;
            Ok(db.get_entry(&current.hid)?)
        }
        _ => {
            db.soft_delete_entry_as(&current.hid, ChangeKind::Revert)?;
            Ok(db.get_entry_including_deleted(&current.hid)?)
        }
    }
}

/// `datum redo` — the step back that `undo` just took, taken forward again.
pub fn redo(args: RedoArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);
    let output = args.output()?;
    redo_stack(db, args.dry_run, &registry, &output)
}

/// What `_LAST` names after an `undo`, `redo` or `revert`: the rows it moved, less a
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

/// How a batch of steps is echoed.
#[derive(Clone, Copy)]
enum Echo {
    /// UNDONE or REVERT, naming the change taken back (da-3mryj): made at the
    /// stack action's time, or for a named entry at its own last change's.
    Undo(Option<chrono::DateTime<chrono::Utc>>),
    /// The change put back, as the forward action it was: bringing a
    /// tombstone back is a CREATE, tombstoning is a DELETE, and anything
    /// else is an UPDATE drawn as what changed.
    Redo,
}

fn show_step(
    before: &Entry,
    after: &Entry,
    echo: Echo,
    registry: Option<&SpecRegistry>,
    output: &OutputArgs,
) {
    if let Echo::Undo(time) = echo {
        let time = time.unwrap_or(before.modify_time);
        return show_undone(before, after, time, registry, output);
    }
    match (before.deleted_at.is_some(), after.deleted_at.is_some()) {
        (_, true) => show_delete(after, registry, output),
        (true, false) => show_create(after, registry, output),
        (false, false) => show_update_from(before, after, registry, output),
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
    echo: Echo,
    registry: Option<&SpecRegistry>,
    output: &OutputArgs,
) {
    for (before, after) in steps {
        show_step(before, after, echo, registry, output);
    }
    // A bare spec goes with the first log it was made by: undone, the
    // create taken back with it.
    for spec in specs {
        match echo {
            Echo::Undo(_) => {
                let was = Entry {
                    deleted_at: None,
                    ..spec.clone()
                };
                show_undone(&was, spec, spec.create_time, registry, output);
            }
            Echo::Redo => show_delete(spec, registry, output),
        }
    }
}
