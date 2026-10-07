//! Where this copy last saw the members of its chain (da-rugg8), and the
//! handshake a sync session opens with. Every sync is between members of a
//! chain (da-6177s): `sync_peers` is an address book, and grants nothing.
//! Here rather than
//! in the transport so that every build runs them — the native one's iroh
//! session, and datum.day's, whose transport is in the page and reaches these
//! through the CLI (`datum sync handshake`, da-6kpj5).

use chrono::{SecondsFormat, Utc};
use rusqlite::params;

use super::chain;
use crate::db::Db;
use crate::error::{DatumError, Result};

/// A copy of this database that dialing it, or dialing from it, found.
#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub label: String,
    /// Where it was last seen (the transport's address, as JSON); empty for
    /// a member only `sync record-session` has named.
    pub addrs: String,
    pub added_at: String,
    pub last_seen: Option<String>,
    /// Synced with: an unrevoked member of this copy's chain.
    pub synced: bool,
}

/// Also the reason a copy signing with this one's own key is refused.
pub const COPY: &str =
    "this peer is a copy of this database (same endpoint key); run `datum sync chain rekey` on one of them";

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// A new invite secret, and when an invite made now for `expires` ends.
pub fn invite_secret(expires: std::time::Duration) -> (String, String) {
    let expires_at = (Utc::now() + expires).to_rfc3339_opts(SecondsFormat::Secs, true);
    let secret = rand::random::<[u8; 32]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    (secret, expires_at)
}

impl Db {
    /// Every peer this copy has recorded, oldest first.
    pub fn sync_peers(&self) -> Result<Vec<Peer>> {
        let mut stmt = self.conn.prepare(
            "SELECT endpoint_id, label, addrs, added_at, last_seen FROM sync_peers ORDER BY added_at",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Peer {
                    id: r.get(0)?,
                    label: r.get(1)?,
                    addrs: r.get(2)?,
                    added_at: r.get(3)?,
                    last_seen: r.get(4)?,
                    synced: false,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|p| {
                Ok(Peer {
                    synced: self.sync_is_peer(&p.id)?,
                    ..p
                })
            })
            .collect()
    }

    /// A copy this one syncs with: an unrevoked member of its chain, or, in
    /// none yet, a copy it is joining one through ([`Db::sync_joining`]).
    pub fn sync_is_peer(&self, id: &str) -> Result<bool> {
        let chain = self.sync_chain()?;
        if !chain.exists() {
            return Ok(self.sync_joining()?.iter().any(|p| p.id == id));
        }
        Ok(chain.member(id).is_some_and(|m| m.revoked.is_none()))
    }

    /// The copies this one is joining a sync chain through (da-idw54): in
    /// no chain, those it met in a handshake whose ops it holds. A first
    /// sync carries the inviter's history before its chain records — what
    /// it held got its ops before its chain began — so a join cut short (a
    /// reload, a lost connection) leaves the copy holding part of it and in
    /// no chain yet; these are where it resumes. A copy that left a chain
    /// holds none of its ops, so none of its old peers is one.
    pub fn sync_joining(&self) -> Result<Vec<Peer>> {
        if self.sync_chain()?.exists() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT endpoint_id, label, addrs, added_at, last_seen FROM sync_peers p
             WHERE EXISTS (SELECT 1 FROM ops WHERE endpoint = p.endpoint_id)
             ORDER BY added_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Peer {
                id: r.get(0)?,
                label: r.get(1)?,
                addrs: r.get(2)?,
                added_at: r.get(3)?,
                last_seen: r.get(4)?,
                synced: true,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Record `id` as a peer, or refresh its label, address and last seen.
    fn sync_record_peer(&self, id: &str, label: &str, addrs: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_peers (endpoint_id, label, addrs, added_at, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT (endpoint_id) DO UPDATE SET
               label = excluded.label, addrs = excluded.addrs, last_seen = excluded.last_seen",
            params![id, label, addrs, now()],
        )?;
        Ok(())
    }

    /// A session's handshake, as it bears on this copy: `id` introduced
    /// itself as `label` at `addrs`. Dialed (it answered this copy's dial):
    /// recorded. Served (it dialed this copy): admitted — a member, or the
    /// redeemer of `invite` — and recorded; anyone else is refused, and the
    /// error says why. An invite presented is redeemed or refused, even by a
    /// member: one use. A redeemer says which `chain` it is in (empty: none);
    /// one in another chain is refused, as is one that does not say, since a
    /// copy belongs to one chain (da-mepav). It says how many entries it
    /// `holds` too, which a reader invite refuses (da-tti3w). Whether `id` is
    /// new to this copy comes back. A paused copy refuses every session
    /// (da-wlbyx).
    #[allow(clippy::too_many_arguments)]
    pub fn sync_handshake(
        &self,
        id: &str,
        label: &str,
        addrs: &str,
        invite: Option<&str>,
        chain: Option<&str>,
        holds: u64,
        dialed: bool,
    ) -> Result<bool> {
        super::pause::refuse_paused(&self.conn)?;
        let refuse = |why: String| Err(DatumError::Other(why));
        if let Some(why) = chain::label_problem(label) {
            return refuse(why);
        }
        if id == self.sync_endpoint_id()? {
            return refuse(COPY.into());
        }
        let known = self.sync_is_peer(id)?;
        if !dialed {
            match invite {
                Some(secret) => match chain {
                    None => {
                        return refuse(format!(
                        "{label} does not say which sync chain it is in: update its datum to join"
                    ))
                    }
                    Some(c) if !c.is_empty() && c != self.sync_chain()?.id => {
                        return refuse(format!(
                            "{label} is in another sync chain ({c}); a copy belongs to one chain"
                        ))
                    }
                    Some(_) => self.sync_admit(id, label, secret, holds)?,
                },
                None if known => {}
                None => return refuse("not a member of this sync chain".into()),
            }
        }
        self.sync_record_peer(id, label, addrs)?;
        Ok(!known)
    }
}
