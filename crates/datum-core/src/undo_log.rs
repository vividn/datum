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
//! oldest undone one, and a new action drops every undone one. The table is
//! this copy's alone: nothing here syncs, and an entry a sync changed after
//! an action is refused rather than reverted ([`blocker`]).

use std::cell::Cell;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

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
        ChangeKind::Create | ChangeKind::UserEdit | ChangeKind::Retime
    ) {
        return Ok(None);
    }
    let Some(open) = ACTION.get() else {
        return Ok(None);
    };
    if let Some(id) = open {
        // A rolled-back transaction takes the action's row with it.
        let alive = conn
            .query_row("SELECT 1 FROM undo_actions WHERE id = ?1", [id], |_| Ok(()))
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

/// An action on the stack, with every entry it wrote.
#[derive(Debug, Clone)]
pub struct Action {
    pub id: i64,
    pub time: DateTime<Utc>,
    pub members: Vec<Member>,
}

/// The action a bare `undo` reverts next: the newest one not undone.
pub fn cursor(conn: &Connection) -> Result<Option<Action>> {
    load(conn, "undone = 0 ORDER BY id DESC")
}

/// The action a bare `redo` re-applies next: the oldest one undone.
pub fn next_redo(conn: &Connection) -> Result<Option<Action>> {
    load(conn, "undone = 1 ORDER BY id ASC")
}

/// Mark an action undone (by `undo`) or not (by `redo`).
pub fn set_undone(conn: &Connection, id: i64, undone: bool) -> Result<()> {
    conn.execute(
        "UPDATE undo_actions SET undone = ?2 WHERE id = ?1",
        params![id, undone],
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
            "SELECT version, change_kind FROM entry_history
             WHERE entry_hid = ?1 AND version > ?2
               -- a v1 create above an untracked member (last = 0) is the
               -- entry as the import left it, not a later change
               AND NOT (version = 1 AND change_kind = 'create')
               AND (change_kind IN ('sync_merge', 'sync_conflict_resolve')
                    OR (action_id IS NULL AND change_kind NOT IN ('undo', 'redo')))
             ORDER BY version LIMIT 1",
            params![member.hid.as_str(), member.last],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(v, kind)| kind.parse().ok().map(|k| (v, k))))
}

/// The first action matching `filter` that still names an entry: one whose
/// rows are gone (a `restore --hard`, a forgotten entry) has nothing to move.
fn load(conn: &Connection, filter: &str) -> Result<Option<Action>> {
    let row: Option<(i64, String, Option<String>)> = conn
        .query_row(
            &format!(
                "SELECT id, time, untracked FROM undo_actions a
                 WHERE (untracked IS NOT NULL
                        OR EXISTS (SELECT 1 FROM entry_history WHERE action_id = a.id))
                   AND {filter} LIMIT 1"
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((id, time, untracked)) = row else {
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
    let untracked: Vec<Hid> = untracked
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    members.extend(untracked.into_iter().map(|hid| Member {
        hid,
        first: 1,
        last: 0,
    }));
    let time = DateTime::parse_from_rfc3339(&time)
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(Some(Action { id, time, members }))
}
