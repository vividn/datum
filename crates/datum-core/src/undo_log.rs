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
//! which, and its redo leaves them too (da-2ahs4).
//!
//! `undo <field>` takes back part of an action (da-il29m): the field's entries
//! in it, which `undo_taken` holds while the rest of the action stands, so a
//! bare `undo` takes back the rest next. Every undo is a STEP, numbered in the
//! order they ran: `undone` is the step that took back the action's standing
//! entries, `undo_taken.step` the one that took a part, and `redo` re-applies
//! the newest step whole, so `undo a,b` is one redo. The tables are
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
            .prepare_cached("SELECT 1 FROM undo_actions WHERE id = ?1")?
            .query_row([id], |_| Ok(()))
            .optional()?;
        if alive.is_some() {
            return Ok(Some(id));
        }
    }
    // A new action drops the redo tail: what was undone can no longer be
    // redone over it, as in an editor.
    conn.execute_batch(
        "DELETE FROM undo_taken WHERE action_id IN (SELECT id FROM undo_actions WHERE undone <> 0);
         UPDATE undo_taken SET step = -1 WHERE step > 0;
         DELETE FROM undo_actions WHERE undone <> 0;",
    )?;
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

/// An action on the stack, with the entries of it in question (those standing,
/// or for a redo those its step took back), and those its undo left as they
/// are. A redo's step is one Action, whatever actions it reached: `id` is then
/// the step's number.
#[derive(Debug, Clone)]
pub struct Action {
    pub id: i64,
    pub time: DateTime<Utc>,
    pub members: Vec<Member>,
    pub skipped: Vec<Hid>,
}

/// The action a bare `undo` reverts next: the newest one not undone.
pub fn cursor(conn: &Connection) -> Result<Option<Action>> {
    load(conn, "undone = 0 ORDER BY id DESC")
}

/// The newest action with an entry of `field` standing, older than `below`,
/// holding only those entries: the field's rows, and its spec's.
pub fn last_on(conn: &Connection, field: &str, below: i64) -> Result<Option<Action>> {
    let spec = format!("spec:{field}");
    let id: Option<i64> = conn.query_row(
        "SELECT MAX(id) FROM (
             SELECT a.id FROM entries e
               JOIN entry_history h ON h.entry_hid = e.hid
               JOIN undo_actions a ON a.id = h.action_id
              WHERE (e.field = ?1 OR e.slug = ?2) AND a.undone = 0 AND a.id < ?3
                AND NOT EXISTS (SELECT 1 FROM undo_taken t
                                 WHERE t.action_id = a.id AND t.hid = e.hid)
             UNION ALL
             SELECT a.id FROM undo_actions a, json_each(a.untracked) j
               JOIN entries e ON e.hid = j.value
              WHERE a.untracked IS NOT NULL AND (e.field = ?1 OR e.slug = ?2)
                AND a.undone = 0 AND a.id < ?3
                AND NOT EXISTS (SELECT 1 FROM undo_taken t
                                 WHERE t.action_id = a.id AND t.hid = e.hid))",
        params![field, spec, below],
        |r| r.get(0),
    )?;
    let Some(id) = id else {
        return Ok(None);
    };
    let mut action = action_at(conn, id, 0)?;
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

/// What a bare `redo` re-applies next: the newest undo step, as one action.
pub fn next_redo(conn: &Connection) -> Result<Option<Action>> {
    let step: Option<i64> = conn.query_row(
        "SELECT MAX(step) FROM (
             SELECT undone AS step FROM undo_actions a
              WHERE undone <> 0 AND (untracked IS NOT NULL
                    OR EXISTS (SELECT 1 FROM entry_history WHERE action_id = a.id))
             UNION ALL SELECT step FROM undo_taken WHERE step > 0)",
        [],
        |r| r.get(0),
    )?;
    let Some(step) = step else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(
        "SELECT id FROM undo_actions WHERE undone = ?1
         UNION SELECT action_id FROM undo_taken WHERE step = ?1 ORDER BY 1",
    )?;
    let ids = stmt
        .query_map([step], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut merged: Option<Action> = None;
    for id in ids {
        let action = action_at(conn, id, step)?;
        match &mut merged {
            None => merged = Some(Action { id: step, ..action }),
            Some(all) => {
                all.time = all.time.max(action.time);
                all.members.extend(action.members);
                all.skipped.extend(action.skipped);
            }
        }
    }
    Ok(merged)
}

/// The number the next undo step takes.
fn next_step(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT 1 + MAX(COALESCE((SELECT MAX(undone) FROM undo_actions WHERE undone <> 0), 0),
                        COALESCE((SELECT MAX(step) FROM undo_taken), 0))",
        [],
        |r| r.get(0),
    )?)
}

/// Mark an action undone, leaving `skipped` as they are.
pub fn set_undone(conn: &Connection, id: i64, skipped: &[Hid]) -> Result<()> {
    let skipped = (!skipped.is_empty())
        .then(|| serde_json::to_string(skipped))
        .transpose()?;
    conn.execute(
        "UPDATE undo_actions SET undone = ?3, skipped = ?2 WHERE id = ?1",
        params![id, skipped, next_step(conn)?],
    )?;
    Ok(())
}

/// One undo step taking these entries of these actions. An action left with
/// nothing standing is undone; otherwise the rest of it stands.
pub fn take(conn: &Connection, parts: &[(i64, Hid)]) -> Result<()> {
    let step = next_step(conn)?;
    let mut actions: Vec<i64> = parts.iter().map(|(id, _)| *id).collect();
    actions.sort_unstable();
    actions.dedup();
    for id in actions {
        let named = |hid: &Hid| parts.iter().any(|(i, h)| *i == id && h == hid);
        let members = action_at(conn, id, 0)?.members;
        if members.iter().all(|m| named(&m.hid)) {
            conn.execute(
                "UPDATE undo_actions SET undone = ?2 WHERE id = ?1",
                params![id, step],
            )?;
            continue;
        }
        for member in members.iter().filter(|m| named(&m.hid)) {
            conn.execute(
                "INSERT INTO undo_taken (action_id, hid, step) VALUES (?1, ?2, ?3)",
                params![id, member.hid.as_str(), step],
            )?;
        }
    }
    Ok(())
}

/// Mark a step redone: what it took back stands again, nothing left out.
pub fn set_redone(conn: &Connection, step: i64) -> Result<()> {
    conn.execute(
        "UPDATE undo_actions SET undone = 0, skipped = NULL WHERE undone = ?1",
        [step],
    )?;
    conn.execute("DELETE FROM undo_taken WHERE step = ?1", [step])?;
    Ok(())
}

/// A spooled undo landing: its ops name what it took back, each from the
/// newest action where that entry stands.
pub fn took_back(conn: &Connection, hids: &[&str]) -> Result<()> {
    let mut parts = Vec::new();
    for hid in hids {
        let id: Option<i64> = conn.query_row(
            "SELECT MAX(a.id) FROM undo_actions a JOIN entry_history h ON h.action_id = a.id
              WHERE h.entry_hid = ?1 AND a.undone = 0
                AND NOT EXISTS (SELECT 1 FROM undo_taken t
                                 WHERE t.action_id = a.id AND t.hid = ?1)",
            [hid],
            |r| r.get(0),
        )?;
        parts.extend(id.map(|id| (id, Hid::from(hid.to_string()))));
    }
    take(conn, &parts)
}

/// A spooled redo landing: the newest step, when its ops are of it.
pub fn put_back(conn: &Connection, hids: &[&str]) -> Result<()> {
    if let Some(step) = next_redo(conn)? {
        if step.members.iter().any(|m| hids.contains(&m.hid.as_str())) {
            set_redone(conn, step.id)?;
        }
    }
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

/// The first action matching `filter` that still names an entry, as it stands:
/// one whose rows are gone (a `restore --hard`, a forgotten entry) has nothing
/// to move.
fn load(conn: &Connection, filter: &str) -> Result<Option<Action>> {
    let id: Option<i64> = conn
        .query_row(
            &format!(
                "SELECT id FROM undo_actions a
                 WHERE (untracked IS NOT NULL
                        OR EXISTS (SELECT 1 FROM entry_history WHERE action_id = a.id))
                   AND {filter} LIMIT 1"
            ),
            [],
            |r| r.get(0),
        )
        .optional()?;
    id.map(|id| action_at(conn, id, 0)).transpose()
}

/// Action `id` with the members a step took back: an entry's step is its
/// `undo_taken` row's, else the action's `undone`, and 0 is standing.
fn action_at(conn: &Connection, id: i64, step: i64) -> Result<Action> {
    let (time, undone, untracked, skipped): (String, i64, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT time, undone, untracked, skipped FROM undo_actions WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
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
    let mut taken =
        conn.prepare_cached("SELECT step FROM undo_taken WHERE action_id = ?1 AND hid = ?2")?;
    let mut held = Vec::new();
    for member in members {
        let of: Option<i64> = taken
            .query_row(params![id, member.hid.as_str()], |r| r.get(0))
            .optional()?;
        if of.unwrap_or(undone) == step {
            held.push(member);
        }
    }
    let time = DateTime::parse_from_rfc3339(&time)
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Ok(Action {
        id,
        time,
        members: held,
        skipped,
    })
}
