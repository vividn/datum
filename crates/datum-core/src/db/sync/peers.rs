//! The copies this one syncs with, and its one-time invites (da-rugg8): the
//! local tables a sync session reads and writes as it opens. Here rather than
//! in the transport so that every build runs them — the native one's iroh
//! session, and datum.day's, whose transport is in the page and reaches these
//! through the CLI (`datum sync handshake`, da-6kpj5).

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, OptionalExtension};

use super::chain;
use super::sha256_hex;
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
    /// Synced with: in a chain, an unrevoked member; else a peer.
    pub synced: bool,
}

/// Also the reason a copy signing with this one's own key is refused.
pub const COPY: &str =
    "this peer is a copy of this database (same endpoint key); run `datum sync rekey` on one of them";

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

    /// A copy this one syncs with: in a chain, an unrevoked member; else a peer.
    pub fn sync_is_peer(&self, id: &str) -> Result<bool> {
        let chain = self.sync_chain()?;
        if chain.exists() {
            return Ok(chain.member(id).is_some_and(|m| m.revoked.is_none()));
        }
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM sync_peers WHERE endpoint_id = ?1",
                [id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
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

    /// A new pairwise invite (`datum sync add-client`): its secret, which
    /// only the ticket carries, and when it expires.
    pub fn sync_new_invite(&self, expires: std::time::Duration) -> Result<(String, String)> {
        let (secret, expires_at) = invite_secret(expires);
        self.conn.execute(
            "INSERT INTO sync_invites (secret_hash, expires_at) VALUES (?1, ?2)",
            params![sha256_hex(&secret), expires_at],
        )?;
        Ok((secret, expires_at))
    }

    /// Mark the invite whose secret is `secret` used, if it is unused and
    /// unexpired; otherwise say why not.
    fn sync_redeem(&self, secret: &str) -> Result<()> {
        let hash = sha256_hex(secret);
        let row: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT expires_at, used_at FROM sync_invites WHERE secret_hash = ?1",
                [&hash],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let why = match row {
            None => "unknown invite".to_string(),
            Some((_, Some(used))) => format!("invite already used ({used})"),
            Some((expires, None)) if expires <= now() => format!("invite expired ({expires})"),
            Some(_) => {
                self.conn.execute(
                    "UPDATE sync_invites SET used_at = ?2 WHERE secret_hash = ?1",
                    params![hash, now()],
                )?;
                return Ok(());
            }
        };
        Err(DatumError::Other(why))
    }

    /// A session's handshake, as it bears on this copy: `id` introduced
    /// itself as `label` at `addrs`. Dialed (it answered this copy's dial):
    /// recorded. Served (it dialed this copy): admitted — a peer, the
    /// redeemer of `invite`, or in a chain a member or the redeemer of an
    /// admin-signed invite — and recorded; anyone else is refused, and the
    /// error says why. An invite presented is redeemed or refused, even by a
    /// peer: one use. Whether `id` is new to this copy comes back.
    pub fn sync_handshake(
        &self,
        id: &str,
        label: &str,
        addrs: &str,
        invite: Option<&str>,
        dialed: bool,
    ) -> Result<bool> {
        let refuse = |why: String| Err(DatumError::Other(why));
        if let Some(why) = chain::label_problem(label) {
            return refuse(why);
        }
        if id == self.sync_endpoint_id()? {
            return refuse(COPY.into());
        }
        let known = self.sync_is_peer(id)?;
        if !dialed {
            let chained = self.sync_chain()?.exists();
            match invite {
                Some(secret) if chained => self.sync_admit(id, label, secret)?,
                Some(secret) => self.sync_redeem(secret)?,
                None if known => {}
                None if chained => return refuse("not a member of this sync chain".into()),
                None => {
                    return refuse("not a peer of this copy: redeem an invite from it first".into())
                }
            }
        }
        self.sync_record_peer(id, label, addrs)?;
        Ok(!known)
    }

    /// Drop the peer `who` names, by label or a prefix of its id; its label
    /// and id come back.
    pub fn sync_forget(&self, who: &str) -> Result<(String, String)> {
        let hits: Vec<Peer> = self
            .sync_peers()?
            .into_iter()
            .filter(|p| p.label == who || p.id.starts_with(who))
            .collect();
        match hits.as_slice() {
            [p] => {
                self.conn
                    .execute("DELETE FROM sync_peers WHERE endpoint_id = ?1", [&p.id])?;
                Ok((p.label.clone(), p.id.clone()))
            }
            [] => Err(DatumError::Other(format!("no sync peer {who}"))),
            _ => Err(DatumError::Other(format!(
                "{who} names {} sync peers; give more of the id",
                hits.len()
            ))),
        }
    }
}
