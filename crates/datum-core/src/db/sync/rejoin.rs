//! `datum db purge --rejoin` (da-qjxfx): wipe a chained copy and refetch it
//! from its chain, staying the member it was. It keeps its endpoint key, where
//! its members were last seen and its notices, and nothing else. Its own ops
//! come back from the members as everyone's do; until they have, `_meta`
//! `sync_rejoin` holds the highest seq its key signed before the wipe, which
//! this copy counts as signed ([`signed_before`]): it takes its own ops back
//! and serves a member's vector without reading either as a second copy
//! sharing its key, and it signs nothing, which would number a new op from a
//! seq it already used.

use rusqlite::{params, Connection, OptionalExtension};

use super::chain::{Member, Role};
use crate::db::Db;
use crate::error::{DatumError, Result};

const KEY: &str = "sync_rejoin";

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
        let chain = self.sync_chain()?;
        let me = self.sync_endpoint_id()?;
        let refuse = |why: String| Err(DatumError::Other(why));
        if !chain.member(&me).is_some_and(|m| m.revoked.is_none()) {
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
        let (met, reached): (i64, i64) = self.conn.query_row(
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
        // Its ops past that, and the entries no op covers yet, which only a
        // sync signs (a bulk import's).
        let unsent: i64 = self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM ops WHERE endpoint = ?1 AND seq > ?2)
                  + (SELECT COUNT(*) FROM entries WHERE hid NOT IN (SELECT hid FROM ops))",
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
        const KEPT: [&str; 3] = ["sync_local", "sync_peers", "sync_notices"];
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
