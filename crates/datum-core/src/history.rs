//! Per-entry modification trail (`entry_history` table): this copy's undo
//! journal. What `datum history` prints is read from the entry's ops instead
//! ([`crate::db::Db::versions`], da-s4erq), which every copy holds alike.
//!
//! Every mutating operation on the `entries` table writes a companion row
//! here in the same transaction. Version numbering is monotonic per entry:
//! version=1 is the original create, version=2 is the first edit, and so on.
//!
//! `snapshot` stores the full entry JSON. For version=1 (`create`) it holds
//! the newly-created entry; for later versions it holds the **pre-operation**
//! state so a revert can restore that earlier shape without consulting the
//! live row.
//!
//! `author` is who made the change, `user@host` ([`author`], da-zpc98): this
//! process for a change made here, the op's for one a sync applied (NULL
//! where the op carries none). A trigger keeps `entries.author` the author of
//! the entry's latest change. An audit signal: anything that can write the
//! file can write another name.

use std::cell::RefCell;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::entry::Entry;
use crate::error::{DatumError, Result};

/// Classification of the operation that produced a history row.
///
/// The string form is what gets stored in the `change_kind` column, and an
/// op's `kind` for the ones a command writes ([`crate::db::Version`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// First-ever write of the entry.
    Create,
    /// User-driven mutation (update / merge / edit / delete / undelete).
    UserEdit,
    /// Batch time shift via `datum retime`.
    Retime,
    /// Non-trivial apply of a remote changeset (content actually changed).
    SyncMerge,
    /// Explicit resolution of a sync conflict.
    SyncConflictResolve,
    /// A step back of the undo stack via `datum undo`.
    Undo,
    /// A step forward of the undo stack via `datum redo`, the inverse of
    /// [`ChangeKind::Undo`] on the same trail.
    Redo,
    /// A step back via `datum revert`: a command's own write, an action on the
    /// stack, and what `revert <field>` does not take back in turn (da-k5vg1).
    Revert,
}

impl ChangeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeKind::Create => "create",
            ChangeKind::UserEdit => "user_edit",
            ChangeKind::Retime => "retime",
            ChangeKind::SyncMerge => "sync_merge",
            ChangeKind::SyncConflictResolve => "sync_conflict_resolve",
            ChangeKind::Undo => "undo",
            ChangeKind::Redo => "redo",
            ChangeKind::Revert => "revert",
        }
    }

    /// The kinds a sync writes: rows another copy made, never a command here.
    pub const SYNC: &[ChangeKind] = &[ChangeKind::SyncMerge, ChangeKind::SyncConflictResolve];
    /// The kinds `undo` and `redo` write: the undo stack's own rows.
    pub const STACK: &[ChangeKind] = &[ChangeKind::Undo, ChangeKind::Redo];

    /// `kinds` as an SQL list, `('a', 'b')`, for `change_kind IN …`, so a
    /// query names kinds through [`ChangeKind::as_str`] and never as literals.
    pub fn sql_list(kinds: &[ChangeKind]) -> String {
        let quoted: Vec<String> = kinds.iter().map(|k| format!("'{}'", k.as_str())).collect();
        format!("({})", quoted.join(", "))
    }
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ChangeKind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "create" => Ok(ChangeKind::Create),
            "user_edit" => Ok(ChangeKind::UserEdit),
            "retime" => Ok(ChangeKind::Retime),
            "sync_merge" => Ok(ChangeKind::SyncMerge),
            "sync_conflict_resolve" => Ok(ChangeKind::SyncConflictResolve),
            "undo" => Ok(ChangeKind::Undo),
            "redo" => Ok(ChangeKind::Redo),
            "revert" => Ok(ChangeKind::Revert),
            other => Err(format!("unknown change_kind: '{other}'")),
        }
    }
}

/// A single row from `entry_history`, with the snapshot rehydrated as an
/// [`Entry`]. The snapshot for version=1 is the newly-created entry; for
/// later versions it is the pre-operation state.
#[derive(Debug, Clone)]
pub struct HistoryVersion {
    pub entry_hid: String,
    pub version: u32,
    pub snapshot: Entry,
    pub modify_time: DateTime<Utc>,
    pub raw_input: Option<String>,
    pub alias_chain: Option<JsonValue>,
    pub client_id: String,
    pub change_kind: ChangeKind,
    /// Who made the change, `user@host`; none where nobody said.
    pub author: Option<String>,
}

thread_local! {
    /// This process's [`author`], read once.
    static AUTHOR: RefCell<Option<Option<String>>> = const { RefCell::new(None) };
    /// While [`as_author`] runs: the author of the change being applied.
    static APPLYING: RefCell<Option<Option<String>>> = const { RefCell::new(None) };
}

/// The process making a change, `user@host`: the passwd name of `getuid()`
/// (its number where it has none) and the host name. None where either is
/// unknowable, as in a browser. No environment variable changes it.
pub fn author() -> Option<String> {
    AUTHOR.with(|a| a.borrow_mut().get_or_insert_with(read_author).clone())
}

#[cfg(unix)]
fn read_author() -> Option<String> {
    use std::ffi::CStr;
    // SAFETY: getuid cannot fail; getpwuid_r writes into `pw` and `buf`,
    // both writable for the lengths passed, and `out` points into `pw`.
    let uid = unsafe { libc::getuid() };
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut out = std::ptr::null_mut();
    let rc = unsafe { libc::getpwuid_r(uid, &mut pw, buf.as_mut_ptr(), buf.len(), &mut out) };
    let name = (rc == 0 && !out.is_null() && !pw.pw_name.is_null())
        .then(|| {
            unsafe { CStr::from_ptr(pw.pw_name) }
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| uid.to_string());
    Some(format!("{name}@{}", host()?))
}

#[cfg(unix)]
pub(crate) fn host() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: `buf` is writable for its whole length, which is what is passed.
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
    let end = buf.iter().position(|&b| b == 0)?;
    (ok && end > 0).then(|| String::from_utf8_lossy(&buf[..end]).into_owned())
}

#[cfg(not(unix))]
pub(crate) fn host() -> Option<String> {
    std::env::var("COMPUTERNAME").ok().filter(|v| !v.is_empty())
}

#[cfg(not(unix))]
fn read_author() -> Option<String> {
    None
}

/// Run `f` writing its changes as `who`'s: a synced op's author, which the
/// op may not carry.
pub(crate) fn as_author<T>(who: Option<String>, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Option<String>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            APPLYING.with(|a| *a.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(APPLYING.with(|a| a.borrow_mut().replace(who)));
    f()
}

/// Who the change being written is by.
pub(crate) fn writing() -> Option<String> {
    APPLYING.with(|a| a.borrow().clone()).unwrap_or_else(author)
}

/// Make this thread's process [`author`] `who`, as a test's second user.
#[cfg(test)]
pub(crate) fn stub_author(who: Option<&str>) {
    AUTHOR.with(|a| *a.borrow_mut() = Some(who.map(Into::into)));
}

/// Smallest currently-unused version number for the given entry: `last + 1`,
/// or `1` if no rows exist yet.
pub fn next_version(conn: &Connection, entry_hid: &str) -> Result<u32> {
    let max: Option<i64> = conn
        .prepare_cached("SELECT MAX(version) FROM entry_history WHERE entry_hid = ?1")?
        .query_row(params![entry_hid], |row| row.get(0))
        .optional()?
        .flatten();
    Ok(match max {
        Some(n) => (n as u32) + 1,
        None => 1,
    })
}

/// Insert a history row for `snapshot` under the next available version.
/// Returns the stored version. The `executor` parameter lets this run inside
/// a caller-owned transaction (recommended — the companion `entries` mutation
/// should live in the same transaction to keep the trail in lockstep).
///
/// A trail opens with the entry as it was created. An entry older than its
/// trail (bulk-imported by `v2import`, or written before history) has none,
/// so its first edit seeds a `create` from the pre-edit state: `undo` then
/// reverts that edit instead of reading it as a create and deleting the row,
/// and `redo` has a step to come back along (da-yyjob, da-ult68). That seeded
/// row is the entry as it was before, so it belongs to no action; the row
/// itself belongs to the running command's (`undo_log`, da-z0zh3).
pub fn record(executor: &Connection, snapshot: &Entry, change_kind: ChangeKind) -> Result<u32> {
    let mut version = next_version(executor, &snapshot.hid)?;
    if version == 1 && !matches!(change_kind, ChangeKind::Create | ChangeKind::SyncMerge) {
        // The entry as it was before: nobody here made it.
        insert(executor, snapshot, 1, ChangeKind::Create, None)?;
        version = 2;
    }
    // Asked first: making the action is itself an insert. Stamped apart, so
    // a write outside any action reads as it always did, on any schema.
    let action = crate::undo_log::current(executor, change_kind)?;
    insert(executor, snapshot, version, change_kind, writing())?;
    if let Some(id) = action {
        executor
            .prepare_cached(
                "UPDATE entry_history SET action_id = ?1 WHERE rowid = last_insert_rowid()",
            )?
            .execute([id])?;
    }
    Ok(version)
}

fn insert(
    executor: &Connection,
    snapshot: &Entry,
    version: u32,
    change_kind: ChangeKind,
    author: Option<String>,
) -> Result<()> {
    let snapshot_json = serde_json::to_string(snapshot)?;
    let alias_chain_json = snapshot.alias_chain.as_ref().map(|v| v.to_string());
    let (mut stmt, n) = authored(
        executor,
        r#"INSERT INTO entry_history (
            entry_hid, version, snapshot, modify_time,
            raw_input, alias_chain, client_id, change_kind, author
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
        (
            r#"INSERT INTO entry_history (
            entry_hid, version, snapshot, modify_time,
            raw_input, alias_chain, client_id, change_kind
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            8,
        ),
        9,
    )?;
    let all = params![
        snapshot.hid,
        version,
        snapshot_json,
        snapshot.modify_time.to_rfc3339(),
        // §5 #11: see `db::insert` for the same bind compat note —
        // empty-string-when-None preserves migration-walk
        // compatibility against historical NOT NULL versions.
        snapshot.raw_input.as_deref().unwrap_or(""),
        alias_chain_json,
        snapshot.client_id,
        change_kind.as_str(),
        author,
    ];
    stmt.execute(&all[..n])?;
    Ok(())
}

/// `sql`, which writes `author` and takes `n` parameters, or where a
/// migration walk writes at a schema from before v80 and has no `author`
/// yet, `before`'s statement and count, the same but for it (da-zpc98).
pub(crate) fn authored<'c>(
    conn: &'c Connection,
    sql: &str,
    before: (&str, usize),
    n: usize,
) -> Result<(rusqlite::CachedStatement<'c>, usize)> {
    match conn.prepare_cached(sql) {
        Ok(stmt) => Ok((stmt, n)),
        Err(_) => Ok((conn.prepare_cached(before.0)?, before.1)),
    }
}

/// Who made `entry_hid`'s latest change, `user@host`, if anyone said.
pub fn author_of(conn: &Connection, entry_hid: &str) -> Result<Option<String>> {
    Ok(conn
        .prepare_cached("SELECT author FROM entries WHERE hid = ?1")?
        .query_row(params![entry_hid], |r| r.get(0))
        .optional()?
        .flatten())
}

/// List every history row for `entry_hid`. When `newest_first` is true the
/// highest version comes first (natural `datum history` ordering); otherwise
/// rows are returned in chronological / version-ascending order.
pub fn list(conn: &Connection, entry_hid: &str, newest_first: bool) -> Result<Vec<HistoryVersion>> {
    let sql = if newest_first {
        "SELECT entry_hid, version, snapshot, modify_time, raw_input,
                alias_chain, client_id, change_kind, author
         FROM entry_history
         WHERE entry_hid = ?1
         ORDER BY version DESC"
    } else {
        "SELECT entry_hid, version, snapshot, modify_time, raw_input,
                alias_chain, client_id, change_kind, author
         FROM entry_history
         WHERE entry_hid = ?1
         ORDER BY version ASC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![entry_hid], row_to_version)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}

/// Fetch the row for a specific (entry_hid, version) tuple, if present.
pub fn get(conn: &Connection, entry_hid: &str, version: u32) -> Result<Option<HistoryVersion>> {
    conn.prepare_cached(
        "SELECT entry_hid, version, snapshot, modify_time, raw_input,
                alias_chain, client_id, change_kind, author
         FROM entry_history
         WHERE entry_hid = ?1 AND version = ?2",
    )?
    .query_row(params![entry_hid, version], row_to_version)
    .optional()
    .map_err(DatumError::Db)
}

/// Highest recorded version for `entry_hid`, or `None` if no history exists
/// (pre-D4 rows, or an unknown hid).
pub fn latest_version(conn: &Connection, entry_hid: &str) -> Result<Option<u32>> {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(version) FROM entry_history WHERE entry_hid = ?1",
            params![entry_hid],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(max.map(|n| n as u32))
}

/// Reconstruct the entry state *as of* version N.
///
/// Rule: for version=1 the snapshot itself is the post-create state. For any
/// later version, the state that existed after version N's operation equals
/// version (N+1)'s pre-operation snapshot. When N is the latest recorded
/// version, the live `entries` row (passed in as `live`) holds that state —
/// there's no successor snapshot to consult.
pub fn state_at_version(
    conn: &Connection,
    entry_hid: &str,
    version: u32,
    live: Option<&Entry>,
) -> Result<Option<Entry>> {
    // v1 is the only version where the stored snapshot is the post-op state.
    if version == 1 {
        if let Some(v1) = get(conn, entry_hid, 1)? {
            return Ok(Some(v1.snapshot));
        }
        return Ok(None);
    }
    // For later versions, prefer the next version's snapshot (= post-vN state).
    if let Some(next) = get(conn, entry_hid, version + 1)? {
        return Ok(Some(next.snapshot));
    }
    // No successor recorded — vN is the latest, so the live row holds its state.
    Ok(live.cloned())
}

fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryVersion> {
    let snapshot_json: String = row.get("snapshot")?;
    let snapshot: Entry = serde_json::from_str(&snapshot_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let modify_time: DateTime<Utc> =
        row.get::<_, String>("modify_time")?
            .parse()
            .map_err(|e: chrono::ParseError| {
                rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
            })?;
    let alias_chain: Option<JsonValue> = row
        .get::<_, Option<String>>("alias_chain")?
        .and_then(|s| serde_json::from_str(&s).ok());
    let change_kind_str: String = row.get("change_kind")?;
    let change_kind: ChangeKind = change_kind_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;
    Ok(HistoryVersion {
        entry_hid: row.get("entry_hid")?,
        version: row.get::<_, i64>("version")? as u32,
        snapshot,
        modify_time,
        // §5 #11: empty-string sentinel → None at the boundary; mirrors
        // `crate::db::row_mapper::row_to_entry`.
        raw_input: row
            .get::<_, Option<String>>("raw_input")?
            .filter(|s| !s.is_empty()),
        alias_chain,
        client_id: row.get("client_id")?,
        change_kind,
        author: row.get("author")?,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{stub_author, ChangeKind};
    use crate::db::Db;
    use crate::entry::{Entry, Verb};

    /// Each `entry_history` author of `hid`, by version, and the entry's.
    fn authors(db: &Db, hid: &str) -> (Vec<Option<String>>, Option<String>) {
        let mut stmt = db
            .conn()
            .prepare("SELECT author FROM entry_history WHERE entry_hid = ?1 ORDER BY version")
            .unwrap();
        let rows = stmt.query_map([hid], |r| r.get(0)).unwrap();
        let rows = rows.collect::<rusqlite::Result<_>>().unwrap();
        (rows, super::author_of(db.conn(), hid).unwrap())
    }

    fn some(a: &str) -> Option<String> {
        Some(a.into())
    }

    /// A change names the process making it (da-zpc98): a create its own
    /// author, an update its updater, not the creator its pre-change
    /// snapshot was written by; the entry, its latest change's.
    #[test]
    fn each_change_names_who_made_it_and_the_entry_its_latest() {
        let db = Db::open_in_memory().unwrap();
        stub_author(Some("ada@one"));
        let mut e = Entry::new("coffee", Verb::Point, None, None, None, None, "t");
        db.insert_entry(&e).unwrap();
        assert_eq!(
            authors(&db, e.hid.as_str()),
            (vec![some("ada@one")], some("ada@one"))
        );
        stub_author(Some("bo@two"));
        e.data = Some(serde_json::json!({ "cups": 2 }));
        db.save_existing_entry(&e).unwrap();
        assert_eq!(
            authors(&db, e.hid.as_str()),
            (vec![some("ada@one"), some("bo@two")], some("bo@two"))
        );
        let ops = db.sync_ops_since(&BTreeMap::new(), false, None).unwrap();
        let theirs =
            |o: &String| serde_json::from_str::<serde_json::Value>(o).unwrap()["author"].clone();
        let named: Vec<_> = ops
            .iter()
            .filter(|o| o.contains("coffee"))
            .map(theirs)
            .collect();
        assert_eq!(
            named,
            ["ada@one", "bo@two"],
            "each op signs its author: {ops:?}"
        );
    }

    /// A synced change names the op's author on the copy applying it, and
    /// one whose op carries none, as an older copy's, names nobody.
    #[test]
    fn a_synced_change_names_its_ops_author_or_nobody() {
        let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
        for (made_by, named) in [(Some("ada@one"), some("ada@one")), (None, None)] {
            stub_author(made_by);
            let e = Entry::new("tea", Verb::Point, None, None, None, None, "t");
            a.insert_entry(&e).unwrap();
            let ops = a.sync_ops_since(&BTreeMap::new(), false, None).unwrap();
            stub_author(Some("bo@two"));
            b.sync_apply(&ops.join("\n")).unwrap();
            assert_eq!(
                authors(&b, e.hid.as_str()),
                (vec![named.clone()], named),
                "made by {made_by:?}"
            );
        }
    }

    /// The lists the queries splice in, spelled the way the column stores them.
    #[test]
    fn sql_list_quotes_each_kind_as_stored() {
        assert_eq!(
            ChangeKind::sql_list(ChangeKind::SYNC),
            "('sync_merge', 'sync_conflict_resolve')"
        );
        assert_eq!(ChangeKind::sql_list(ChangeKind::STACK), "('undo', 'redo')");
    }
}
