//! A pending join (da-4qpwf): the invite a copy was handed, kept until a
//! member admits it. `chain join` stores it and returns at once (vividn,
//! 2026-11-10: «It should just add the keys and be done. And then start
//! syncing once both devices are online»); `sync run`, the daemon, a bare
//! `sync` and datum.day's sync leader redeem it once a member answers.
//! One per database, the newer replacing the older, in `_meta` `sync_join`:
//! local, never synced. The handshake that records the member which admitted
//! it removes it ([`Db::sync_handshake`]), so the native and the page's
//! store both end it alike.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::Result;

/// What a pending join holds: the ticket, as handed, and what it says of
/// itself, for `sync status` to say without decoding it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PendingJoin {
    pub ticket: String,
    /// The label it joins as (`--label`), else the one assembled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// The inviting device's label, as the ticket names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<String>,
    /// When the invite expires, unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<u64>,
    /// When it was stored, RFC 3339.
    #[serde(default)]
    pub since: String,
    /// A member's refusal that no retry changes (used, expired): the words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}

impl Db {
    /// The pending join, if any.
    pub fn sync_join_pending(&self) -> Result<Option<PendingJoin>> {
        let held: Option<String> = self
            .conn
            .query_row("SELECT value FROM _meta WHERE key = 'sync_join'", [], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(held.and_then(|v| serde_json::from_str(&v).ok()))
    }

    /// Store `join` as the pending join, replacing any.
    pub fn sync_join_hold(&self, join: &PendingJoin) -> Result<()> {
        let mut join = join.clone();
        if join.since.is_empty() {
            join.since = chrono::Utc::now().to_rfc3339();
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO _meta (key, value) VALUES ('sync_join', ?1)",
            params![serde_json::to_string(&join)?],
        )?;
        Ok(())
    }

    /// Record that a member refused the pending join for good: retries end.
    pub fn sync_join_refused(&self, why: &str) -> Result<()> {
        if let Some(mut join) = self.sync_join_pending()? {
            join.refused = Some(why.into());
            self.sync_join_hold(&join)?;
        }
        Ok(())
    }

    /// The pending join is over: admitted, or given up.
    pub fn sync_join_clear(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM _meta WHERE key = 'sync_join'", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pending_join_is_held_replaced_refused_and_cleared() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.sync_join_pending().unwrap(), None);
        let first = PendingJoin {
            ticket: "a".into(),
            ..Default::default()
        };
        db.sync_join_hold(&first).unwrap();
        db.sync_join_hold(&PendingJoin {
            ticket: "b".into(),
            from: Some("laptop".into()),
            ..Default::default()
        })
        .unwrap();
        let held = db.sync_join_pending().unwrap().unwrap();
        assert_eq!(
            (held.ticket.as_str(), held.from.as_deref()),
            ("b", Some("laptop"))
        );
        assert!(!held.since.is_empty());
        db.sync_join_refused("invite used").unwrap();
        let held = db.sync_join_pending().unwrap().unwrap();
        assert_eq!(held.refused.as_deref(), Some("invite used"));
        db.sync_join_clear().unwrap();
        assert_eq!(db.sync_join_pending().unwrap(), None);
    }
}
