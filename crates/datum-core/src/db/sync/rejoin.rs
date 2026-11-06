//! `datum db purge --rejoin` (da-qjxfx): wipe a chained copy and refetch it
//! from its chain, staying the member it was. It keeps its endpoint key, where
//! its members were last seen and its notices, and nothing else. Its own ops
//! come back from the members as everyone's do; until they have, `_meta`
//! `sync_rejoin` holds the highest seq its key signed before the wipe, which
//! this copy counts as signed ([`signed_before`]): it takes its own ops back
//! and serves a member's vector without reading either as a second copy
//! sharing its key, and it signs nothing, which would number a new op from a
//! seq it already used.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::{json, Value};

use super::chain::{Chain, Member, Role};
use super::{entry_from, fold_into, row, Op};
use crate::db::Db;
use crate::entry::{Entry, Verb};
use crate::error::{DatumError, Result};
use crate::history::ChangeKind;
use crate::migrations::{read_schema_version, CURRENT_SCHEMA_VERSION};

const KEY: &str = "sync_rejoin";

/// What a copy refetching its chain keeps of itself: its endpoint, where its
/// members were last seen, and its notices.
const KEPT: [&str; 3] = ["sync_local", "sync_peers", "sync_notices"];

/// Said to a write while this copy's own ops are still coming back.
pub(super) const REJOINING: &str = "this copy is refetching its sync chain after `datum db purge \
     --rejoin` and writes nothing until its own changes are back: `datum sync`";

/// The highest seq `mine` signed before a `db purge --rejoin` emptied this
/// copy, or 0. Keyed by the endpoint, so a copy given a new key since signs
/// from 1.
pub(super) fn signed_before(conn: &Connection, mine: &str) -> Result<i64> {
    Ok(conn
        .prepare_cached(
            "SELECT json_extract(value, '$.seq') FROM _meta
             WHERE key = ?1 AND json_extract(value, '$.endpoint') = ?2",
        )?
        .query_row(params![KEY, mine], |r| r.get(0))
        .optional()?
        .unwrap_or(0))
}

/// Every other live member of `chain` that relays, since a reader serves
/// its own ops alone, and the highest seq of `me`'s any of them held when a
/// session with it last ended. Refused when there is none, when `me` is no
/// live member, and when this copy has met none of them.
fn sources(conn: &Connection, chain: Chain, me: &str) -> Result<(Vec<Member>, i64)> {
    let refuse = |why: String| Err(DatumError::Other(why));
    if !chain.member(me).is_some_and(|m| m.revoked.is_none()) {
        return refuse(format!(
            "this copy is no member of sync chain {} any more: `datum db purge --leave`",
            chain.id
        ));
    }
    let sources: Vec<Member> = chain
        .members
        .into_iter()
        .filter(|m| m.endpoint != me && m.revoked.is_none() && m.role() == Role::Member)
        .collect();
    if sources.is_empty() {
        return refuse(format!(
            "no other member of sync chain {} holds everything to refetch from",
            chain.id
        ));
    }
    // Where this copy knows them from (a session, or an address passed
    // on), and what each held of its ops when a session last ended.
    let ids = serde_json::to_string(&sources.iter().map(|m| &m.endpoint).collect::<Vec<_>>())
        .expect("strings serialize");
    let (met, reached): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(MAX(json_extract(last_vector, ?1)), 0) FROM sync_peers
         WHERE endpoint_id IN (SELECT value FROM json_each(?2))",
        params![format!("$.\"{me}\""), ids],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if met == 0 {
        return refuse(format!(
            "this copy knows of no member of sync chain {} to refetch from: `datum sync` first",
            chain.id
        ));
    }
    Ok((sources, reached))
}

/// A re-entry's chain back (da-vcu8a): its mark comes down to what of its
/// own ops came back with it, from which this copy signs again, the replay
/// first. Before the chain's records, which a first sync carries after its
/// history, nothing changes.
pub(super) fn settle_reentry(conn: &Connection) -> Result<()> {
    let pending: Option<String> = conn
        .query_row(
            "SELECT json_extract(value, '$.endpoint') FROM _meta
             WHERE key = ?1 AND json_extract(value, '$.reentry') = 1",
            [KEY],
            |r| r.get(0),
        )
        .optional()?;
    if pending.is_some() && Chain::load(conn)?.exists() {
        conn.execute("DELETE FROM _meta WHERE key = ?1", [KEY])?;
    }
    Ok(())
}

impl Db {
    /// The labels of the members a `db purge --rejoin` refetches from:
    /// every other live member that relays, since a reader serves its own
    /// ops alone. Refused when there is none, when this copy is no live
    /// member, and while a change made here has reached none of them by its
    /// last known vector, since the wipe would lose it.
    pub fn sync_rejoin_sources(&self) -> Result<Vec<String>> {
        Ok(self
            .rejoin_sources()?
            .into_iter()
            .map(|m| m.label)
            .collect())
    }

    fn rejoin_sources(&self) -> Result<Vec<Member>> {
        let me = self.sync_endpoint_id()?;
        let (sources, reached) = sources(&self.conn, self.sync_chain()?, &me)?;
        let refuse = |why: String| Err(DatumError::Other(why));
        // Its ops past that, and the entries no op covers yet, which only a
        // sync signs (a bulk import's).
        let unsent: i64 = self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM ops WHERE endpoint = ?1 AND seq > ?2)
                  + (SELECT COUNT(*) FROM sync_uncovered u
                      WHERE EXISTS (SELECT 1 FROM entries e WHERE e.hid = u.hid)
                        AND NOT EXISTS (SELECT 1 FROM ops o WHERE o.hid = u.hid))",
            params![me, reached],
            |r| r.get(0),
        )?;
        if unsent > 0 {
            return refuse(format!(
                "{unsent} change{} made here {} reached no other member yet, and a purge would \
                 lose {}: `datum sync` first",
                if unsent == 1 { "" } else { "s" },
                if unsent == 1 { "has" } else { "have" },
                if unsent == 1 { "it" } else { "them" },
            ));
        }
        Ok(sources)
    }

    /// `db purge --rejoin`: drop every table as a purge does, keeping this
    /// copy's endpoint, its address book and its notices, and resume from
    /// the members [`Db::sync_rejoin_sources`] names as a join cut short
    /// does (da-38eye), the first sync bringing the chain back. The tables
    /// dropped.
    pub fn sync_purge_rejoin(&self) -> Result<Vec<String>> {
        let sources = self.rejoin_sources()?;
        let (me, signed) = super::own_head(&self.conn)?;
        for t in KEPT {
            self.conn.execute_batch(&format!(
                "DROP TABLE IF EXISTS temp.kept_{t}; CREATE TEMP TABLE kept_{t} AS SELECT * FROM {t};"
            ))?;
        }
        // Put back in the purge's own transaction: a copy killed before it
        // commits is the file as it was, and after, a member rejoining.
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self.recreate_schema_with(|tx| {
            for t in KEPT {
                tx.execute_batch(&format!(
                    "INSERT INTO {t} SELECT * FROM temp.kept_{t}; DROP TABLE temp.kept_{t};"
                ))?;
            }
            for m in &sources {
                tx.execute(
                    "UPDATE sync_peers SET redeemed_at = ?2 WHERE endpoint_id = ?1",
                    params![m.endpoint, at],
                )?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO _meta (key, value) VALUES (?1, json_object('endpoint', ?2, 'seq', ?3))",
                params![KEY, me, signed],
            )?;
            Ok(())
        })
    }
}

/// What `datum sync chain reenter` did (da-vcu8a).
#[derive(Debug)]
pub struct Reentry {
    /// The schema the file was at, past this datum's.
    pub schema: u32,
    /// The file as it was, whole, beside it.
    pub backup: PathBuf,
    /// The members it refetches from.
    pub sources: Vec<String>,
    /// How many changes made there, which no member held, wait in its spool
    /// to land once its own are back.
    pub queued: usize,
    /// Each change made there that this datum cannot hold, named and why:
    /// it is in the backup alone.
    pub kept: Vec<String>,
}

/// Why this datum cannot hold what `op` does, if it cannot (da-vcu8a): a
/// chain record, a kind of change it does not know, or a key or value its
/// entries do not have. Data keys are any.
fn inexpressible(op: &Op) -> Option<String> {
    if op.is_chain() {
        return Some("a sync chain record".into());
    }
    let kinds = ["update", "delete", "undelete", "baseline", "restore"];
    if !kinds.contains(&op.kind.as_str()) && op.kind.parse::<ChangeKind>().is_err() {
        return Some(format!(
            "a kind of change this datum does not know, {}",
            op.kind
        ));
    }
    let effect = op.effect.as_object()?;
    let known = ["create", "set", "unset", "deleted"];
    if let Some(k) = effect.keys().find(|k| !known.contains(&k.as_str())) {
        return Some(format!("a change this datum does not know, {k}"));
    }
    let mut sample = Entry::new("f", Verb::Point, None, None, None, None, "");
    sample.raw_input = Some("f".into());
    let columns = row(&sample);
    let set = effect.get("create").or_else(|| effect.get("set"));
    let named = set.and_then(Value::as_object).into_iter().flatten();
    let named = named.filter(|(_, v)| !v.is_null()).map(|(k, _)| k.as_str());
    let unset = effect
        .get("unset")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    let mut named = named.chain(unset.filter_map(Value::as_str));
    if let Some(k) = named.find(|k| !k.starts_with("data.") && !columns.contains_key(*k)) {
        return Some(format!("a key this datum's entries do not have, {k}"));
    }
    let mut folded = Some(columns);
    fold_into(&mut folded, &op.effect);
    entry_from(folded?)
        .err()
        .map(|e| format!("a value this datum does not read: {e}"))
}

impl Db {
    /// `datum sync chain reenter` (da-vcu8a): the file at `path`, which a
    /// newer datum migrated past this one, made a copy of its chain at this
    /// datum's schema again. Its file is kept whole beside it; a fresh one
    /// keeping its endpoint, its address book and its notices takes its
    /// place and refetches the chain as `db purge --rejoin` does; and what it
    /// signed that no member held waits in its spool, to land as this copy's
    /// next changes once its own are back, oldest first. What this datum
    /// cannot hold is named, and stays in the backup alone. Nothing of the
    /// newer file is written but its log, checkpointed into it.
    pub fn sync_reenter(path: &Path) -> Result<Reentry> {
        let refuse = |why: String| Err(DatumError::Other(why));
        let shown = path.display();
        let beside = |ext: &str| PathBuf::from(format!("{shown}{ext}"));
        let old = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        let schema = read_schema_version(&old)?.unwrap_or(0);
        if schema <= CURRENT_SCHEMA_VERSION {
            return refuse(format!(
                "{shown} is at schema v{schema}, which this datum (v{CURRENT_SCHEMA_VERSION}) \
                 opens: nothing to re-enter (`datum db purge --rejoin` refetches a copy)"
            ));
        }
        if beside(".spool").exists() {
            return refuse(format!(
                "changes the newer datum queued wait in {shown}.spool: open {shown} with it \
                 once to land them, then re-enter; or move that file aside, the changes \
                 staying in it alone"
            ));
        }
        let me: Option<String> = old
            .query_row("SELECT endpoint_id FROM sync_local WHERE id = 1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let chain = Chain::load(&old).map_err(|e| {
            DatumError::Other(format!("its sync chain does not read in this datum: {e}"))
        })?;
        let (Some(me), true) = (me, chain.exists() && chain.dissolved().is_none()) else {
            return refuse(format!(
                "{shown} is in no sync chain, so none to re-enter at this datum's version: \
                 open it with the datum that migrated it"
            ));
        };
        let (sources, reached) = sources(&old, chain, &me)?;
        // Every seq this key signed counts as signed while the chain comes
        // back: a member may hold more of them than its last vector here
        // says, a session cut after it carried them, and a seq it holds above
        // the mark would read as another copy signing with this key.
        let signed: i64 = old.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM ops WHERE endpoint = ?1",
            [&me],
            |r| r.get(0),
        )?;

        // What it made that no member held, oldest first, and what of that
        // this datum can hold. Its own deps go: the ops are numbered again.
        let mut kept = Vec::new();
        let mut queued: Vec<Value> = Vec::new();
        let mut stmt =
            old.prepare("SELECT body FROM ops WHERE endpoint = ?1 AND seq > ?2 ORDER BY seq")?;
        let bodies = stmt.query_map(params![me, reached], |r| r.get::<_, String>(0))?;
        for body in bodies {
            let body = body?;
            let named = serde_json::from_str::<Value>(&body).unwrap_or_default();
            let what = format!(
                "change {} of {}",
                named["seq"],
                named["field"]
                    .as_str()
                    .or(named["hid"].as_str())
                    .unwrap_or("?")
            );
            match Op::parse(&body) {
                Err(e) => kept.push(format!("{what}: it does not read in this datum: {e}")),
                Ok(op) => match inexpressible(&op) {
                    Some(why) => kept.push(format!("{what}: {why}")),
                    None => {
                        let mut v = named;
                        if let Some(deps) = v["deps"].as_object_mut() {
                            deps.remove(&me);
                        }
                        queued.push(v);
                    }
                },
            }
        }
        drop(stmt);
        // An entry no op covers yet, a bulk import's, only a sync signs.
        let mut stmt = old.prepare(
            "SELECT field, hid FROM entries e WHERE NOT EXISTS (SELECT 1 FROM ops o WHERE o.hid = e.hid)",
        )?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (field, hid) = row?;
            kept.push(format!(
                "entry {hid} of {field}: no change of it was signed yet"
            ));
        }
        drop(stmt);

        // Its log in the file, so the file is all of it, and no other
        // connection holds it.
        let busy: i64 = old.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
        drop(old);
        if busy != 0 || beside("-wal").metadata().is_ok_and(|m| m.len() > 0) {
            return refuse(format!(
                "{shown} is open in another process: close it first"
            ));
        }

        // The fresh copy, made beside it and moved in whole.
        let fresh = beside(".reentering");
        for ext in ["", "-wal", "-shm", "-journal"] {
            let _ = std::fs::remove_file(format!("{}{ext}", fresh.display()));
        }
        {
            let db = Db::open(&fresh.to_string_lossy())?;
            db.conn
                .execute("ATTACH DATABASE ?1 AS old", [path.to_string_lossy()])?;
            let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            crate::db::write::whole(&db.conn, |tx| {
                for t in KEPT {
                    let common: Vec<String> = tx
                        .prepare(
                            "SELECT name FROM pragma_table_info(?1, 'main')
                             WHERE name IN (SELECT name FROM pragma_table_info(?1, 'old'))",
                        )?
                        .query_map([t], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    let cols = common.join(", ");
                    tx.execute_batch(&format!(
                        "DELETE FROM main.{t}; INSERT INTO main.{t} ({cols}) SELECT {cols} FROM old.{t};"
                    ))?;
                }
                // Taken by the endpoint at the path it opens at next.
                tx.execute("UPDATE sync_local SET path = ''", [])?;
                for m in &sources {
                    tx.execute(
                        "UPDATE sync_peers SET redeemed_at = ?2 WHERE endpoint_id = ?1",
                        params![m.endpoint, at],
                    )?;
                }
                tx.execute(
                    "INSERT OR REPLACE INTO _meta (key, value)
                     VALUES (?1, json_object('endpoint', ?2, 'seq', ?3, 'reentry', 1))",
                    params![KEY, me, signed],
                )?;
                Ok(())
            })?;
            db.conn.execute("DETACH DATABASE old", [])?;
        }
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let backup = beside(&format!(".reentered-from-v{schema}-{stamp}.bak"));
        for ext in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(beside(ext));
        }
        // The file stays at its path until the fresh one replaces it in one
        // rename: a copy killed before that is the file as it was.
        std::fs::hard_link(path, &backup).or_else(|_| std::fs::copy(path, &backup).map(|_| ()))?;
        std::fs::rename(&fresh, path)?;

        // One line a command: its ops are consecutive, of one line, at one
        // millisecond.
        let spool = beside(".spool");
        let command = |op: &Value| {
            (
                op["raw"].clone(),
                // An HLC is its wall clock in ms, a slash, its counter.
                op["hlc"]
                    .as_str()
                    .and_then(|h| h.split_once('/'))
                    .map(|(ms, _)| ms.to_string()),
            )
        };
        let mut lines: Vec<Vec<Value>> = Vec::new();
        for op in &queued {
            match lines.last_mut() {
                Some(line) if command(&line[0]) == command(op) => line.push(op.clone()),
                _ => lines.push(vec![op.clone()]),
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        for ops in lines {
            let line = json!({"id": crate::entry::new_hid().to_string(), "at": now, "ops": ops, "last": null});
            crate::db::spool::append_line(&spool, line.to_string().as_bytes())?;
        }
        Ok(Reentry {
            schema,
            backup,
            sources: sources.into_iter().map(|m| m.label).collect(),
            queued: queued.len(),
            kept,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use crate::error::DatumError;

    /// The kept rows go back in the purge's own transaction (da-qjxfx): one
    /// that fails there, as one killed there would, leaves the file as it
    /// was, its key and its data, and the failure is the error.
    #[test]
    fn a_restore_that_fails_leaves_the_file_as_it_was() {
        let db = Db::open_in_memory().unwrap();
        let key = db.sync_endpoint_id().unwrap();
        db.conn()
            .execute("INSERT INTO _meta (key, value) VALUES ('kept', 'yes')", [])
            .unwrap();
        let err = db
            .recreate_schema_with(|_| Err(DatumError::Other("killed here".into())))
            .unwrap_err();
        assert!(err.to_string().contains("killed here"), "{err}");
        assert_eq!(db.sync_endpoint_id().unwrap(), key);
        let kept: String = db
            .conn()
            .query_row("SELECT value FROM _meta WHERE key = 'kept'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kept, "yes");
    }
}
