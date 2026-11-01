//! The undo stack (da-z0zh3): what a bare `undo` walks back and `redo`
//! forward, as an editor's do.
//!
//! An ACTION is one command that changed entries on this copy. Every
//! `entry_history` row it writes names it (`action_id`), so the grouping is
//! recorded at write time: the command layer runs a command inside a
//! [`scope`], and [`crate::history::record`] stamps each row with the
//! scope's action, made on its first write — a command that writes nothing
//! (a `get`) is no action, and moves nothing here. Undo, redo and sync open
//! no scope, and their rows are not a command's own writes, so they are
//! never actions. A command that fails after its first write is still an
//! action: what it wrote is on the stack, and one `undo` reverts it.
//!
//! The cursor is the newest action not `undone`. `undo` puts its entries
//! back as they were before it and marks it undone, `redo` re-applies the
//! oldest undone one, and a new action drops every undone one. An undo that
//! left some entries as another copy left them (`--skip-changed`) records
//! which, and its redo leaves them too (da-2ahs4). `revert` is a command like
//! any other (da-k5vg1): its rows are an action of their own, which `undo`
//! takes back; `revert <field>` finds what it takes with [`last_on`], and says
//! what it took on its own row (`took`), so the next goes on past it. The table
//! is this copy's alone: nothing here syncs, and an entry a sync changed after
//! an action is refused rather than reverted ([`blocker`]).

use std::cell::Cell;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::entry::Entry;
use crate::error::Result;
use crate::history::ChangeKind;
use crate::ids::Hid;

thread_local! {
    /// The running command's action: `None` outside a scope, `Some(None)`
    /// before its first write, `Some(Some(id))` after.
    static ACTION: Cell<Option<Option<i64>>> = const { Cell::new(None) };
}

/// Run `f` as one action. A scope inside a scope is the outer one's, so a
/// multi-side log is one action however many sides it dispatches.
pub fn scope<T>(f: impl FnOnce() -> T) -> T {
    struct Close;
    impl Drop for Close {
        fn drop(&mut self) {
            ACTION.set(None);
        }
    }
    if ACTION.get().is_some() {
        return f();
    }
    ACTION.set(Some(None));
    let _close = Close;
    f()
}

/// The action a history row of `kind` belongs to: the running scope's, made
/// here on its first write, which drops the redo tail. `None` outside a
/// scope, and for a row that is not a command's own write.
pub(crate) fn current(conn: &Connection, kind: ChangeKind) -> Result<Option<i64>> {
    if !matches!(
        kind,
        ChangeKind::Create | ChangeKind::UserEdit | ChangeKind::Retime | ChangeKind::Revert
    ) {
        return Ok(None);
    }
    let Some(open) = ACTION.get() else {
        return Ok(None);
    };
    if let Some(id) = open {
        // A rolled-back transaction takes the action's row with it.
        let alive = conn
            .prepare_cached("SELECT 1 FROM undo_actions WHERE id = ?1")?
            .query_row([id], |_| Ok(()))
            .optional()?;
        if alive.is_some() {
            return Ok(Some(id));
        }
    }
    // A new action drops the redo tail: what was undone can no longer be
    // redone over it, as in an editor.
    conn.execute("DELETE FROM undo_actions WHERE undone = 1", [])?;
    conn.execute(
        "INSERT INTO undo_actions (time) VALUES (?1)",
        [Utc::now().to_rfc3339()],
    )?;
    let id = conn.last_insert_rowid();
    ACTION.set(Some(Some(id)));
    Ok(Some(id))
}

/// Entries the running action wrote with no history row (`v2import`'s bulk
/// rows): it created them, so undo deletes them and redo brings them back.
pub fn add_untracked(conn: &Connection, hids: &[Hid]) -> Result<()> {
    if hids.is_empty() {
        return Ok(());
    }
    let Some(id) = current(conn, ChangeKind::Create)? else {
        return Ok(());
    };
    let held: Option<String> = conn.query_row(
        "SELECT untracked FROM undo_actions WHERE id = ?1",
        [id],
        |r| r.get(0),
    )?;
    let mut all: Vec<Hid> = held
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    all.extend(hids.iter().cloned());
    conn.execute(
        "UPDATE undo_actions SET untracked = ?2 WHERE id = ?1",
        params![id, serde_json::to_string(&all)?],
    )?;
    Ok(())
}

/// One entry an action wrote: the versions of its trail the action wrote,
/// `first..=last`. `first == 1` is an entry the action created; an
/// untracked one (`last == 0`) has no row of its own, so its whole trail
/// came after the action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub hid: Hid,
    pub first: u32,
    pub last: u32,
}

/// An action on the stack, with every entry it wrote, and those of them its
/// undo left as they are.
#[derive(Debug, Clone)]
pub struct Action {
    pub id: i64,
    pub time: DateTime<Utc>,
    pub members: Vec<Member>,
    pub skipped: Vec<Hid>,
}

/// The action a bare `undo` reverts next: the newest one not undone, without
/// the entries a partial undo from schema 77 already took (`skipped`).
pub fn cursor(conn: &Connection) -> Result<Option<Action>> {
    let mut action = load(conn, "undone = 0 ORDER BY id DESC")?;
    if let Some(a) = &mut action {
        let skipped = &a.skipped;
        a.members.retain(|m| !skipped.contains(&m.hid));
    }
    Ok(action)
}

/// The newest action older than `below` that wrote `field` or its spec, is no
/// `revert` and no live revert took back there ([`set_took`]), holding only those
/// entries: what `revert <field>` takes back.
pub fn last_on(conn: &Connection, field: &str, below: i64) -> Result<Option<Action>> {
    let spec = format!("spec:{field}");
    let id: Option<i64> = conn.query_row(
        &format!(
            "SELECT MAX(id) FROM (
                 SELECT a.id FROM entries e
                   JOIN entry_history h ON h.entry_hid = e.hid
                   JOIN undo_actions a ON a.id = h.action_id
                  WHERE (e.field = ?1 OR e.slug = ?2) AND a.undone = 0 AND a.id < ?3
                    AND h.change_kind <> '{revert}' AND NOT {took}
                 UNION ALL
                 SELECT a.id FROM undo_actions a, json_each(a.untracked) j
                   JOIN entries e ON e.hid = j.value
                  WHERE a.untracked IS NOT NULL AND (e.field = ?1 OR e.slug = ?2)
                    AND a.undone = 0 AND a.id < ?3 AND NOT {took})",
            revert = ChangeKind::Revert.as_str(),
            took = "EXISTS (SELECT 1 FROM undo_actions r, json_each(r.took) t
                             WHERE r.took IS NOT NULL AND r.undone = 0
                               AND t.value ->> 0 = a.id AND t.value ->> 1 = ?1)",
        ),
        params![field, spec, below],
        |r| r.get(0),
    )?;
    let Some(id) = id else {
        return Ok(None);
    };
    let Some(mut action) = load(conn, &format!("id = {id}"))? else {
        return Ok(None);
    };
    let mut on_field =
        conn.prepare_cached("SELECT 1 FROM entries WHERE hid = ?1 AND (field = ?2 OR slug = ?3)")?;
    let mut members = Vec::new();
    for member in action.members {
        if on_field.exists(params![member.hid.as_str(), field, spec])? {
            members.push(member);
        }
    }
    action.members = members;
    Ok(Some(action))
}

/// Record on the running action, a `revert <field>`, which action it took back
/// on each field. It lasts as long as the revert stands: undone, it says nothing.
pub fn set_took(conn: &Connection, took: &[(i64, String)]) -> Result<()> {
    if let Some(id) = current(conn, ChangeKind::Revert)? {
        conn.execute(
            "UPDATE undo_actions SET took = ?2 WHERE id = ?1",
            params![id, serde_json::to_string(took)?],
        )?;
    }
    Ok(())
}

/// Whether taking `member` back would change `current`: an entry already as it
/// was before its action (reverted since) has nothing left to move.
pub fn would_move(conn: &Connection, member: &Member, current: &Entry) -> Result<bool> {
    if member.first <= 1 {
        return Ok(current.deleted_at.is_none());
    }
    let before =
        crate::history::state_at_version(conn, &member.hid, member.first - 1, Some(current))?;
    Ok(before.is_none_or(|b| b.content_hash != current.content_hash))
}

/// The action a bare `redo` re-applies next: the oldest one undone.
pub fn next_redo(conn: &Connection) -> Result<Option<Action>> {
    load(conn, "undone = 1 ORDER BY id ASC")
}

/// Mark an action undone, leaving `skipped` as they are.
pub fn set_undone(conn: &Connection, id: i64, skipped: &[Hid]) -> Result<()> {
    let skipped = (!skipped.is_empty())
        .then(|| serde_json::to_string(skipped))
        .transpose()?;
    conn.execute(
        "UPDATE undo_actions SET undone = 1, skipped = ?2 WHERE id = ?1",
        params![id, skipped],
    )?;
    Ok(())
}

/// Mark an action redone: whole again, nothing left out.
pub fn set_redone(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE undo_actions SET undone = 0, skipped = NULL WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

/// The first row on `member`'s trail after the action that the stack does
/// not account for — a sync's, or a write outside any action — as its
/// version and kind. Undoing or redoing over it would throw it away. Undo
/// and redo rows, and the rows of actions since undone (or undone and then
/// dropped), are the stack's own: their changes were already taken back.
/// So is a `create` at version 1 above an untracked member: the entry as the
/// action left it, seeded by the first write after (`history::record`).
/// Versions, not times: clocks differ across devices.
pub fn blocker(conn: &Connection, member: &Member) -> Result<Option<(u32, ChangeKind)>> {
    let row: Option<(u32, String)> = conn
        .query_row(
            &format!(
                "SELECT version, change_kind FROM entry_history
                 WHERE entry_hid = ?1 AND version > ?2
                   -- a v1 create above an untracked member (last = 0) is the
                   -- entry as the import left it, not a later change
                   AND NOT (version = 1 AND change_kind = '{create}')
                   AND (change_kind IN {sync}
                        OR (action_id IS NULL AND change_kind NOT IN {stack}))
                 ORDER BY version LIMIT 1",
                create = ChangeKind::Create.as_str(),
                sync = ChangeKind::sql_list(ChangeKind::SYNC),
                stack = ChangeKind::sql_list(ChangeKind::STACK),
            ),
            params![member.hid.as_str(), member.last],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(v, kind)| kind.parse().ok().map(|k| (v, k))))
}

/// The first action matching `filter` that still names an entry: one whose
/// rows are gone (a `restore --hard`, a forgotten entry) has nothing to move.
fn load(conn: &Connection, filter: &str) -> Result<Option<Action>> {
    let row: Option<(i64, String, Option<String>, Option<String>)> = conn
        .query_row(
            &format!(
                "SELECT id, time, untracked, skipped FROM undo_actions a
                 WHERE (untracked IS NOT NULL
                        OR EXISTS (SELECT 1 FROM entry_history WHERE action_id = a.id))
                   AND {filter} LIMIT 1"
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((id, time, untracked, skipped)) = row else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(
        "SELECT entry_hid, MIN(version), MAX(version) FROM entry_history
         WHERE action_id = ?1 GROUP BY entry_hid ORDER BY MIN(rowid)",
    )?;
    let mut members = stmt
        .query_map([id], |r| {
            Ok(Member {
                hid: Hid::from(r.get::<_, String>(0)?),
                first: r.get(1)?,
                last: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let hids = |json: Option<String>| -> Result<Vec<Hid>> {
        Ok(json
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default())
    };
    let (untracked, skipped) = (hids(untracked)?, hids(skipped)?);
    members.extend(untracked.into_iter().map(|hid| Member {
        hid,
        first: 1,
        last: 0,
    }));
    let time = DateTime::parse_from_rfc3339(&time)
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(Some(Action {
        id,
        time,
        members,
        skipped,
    }))
}
