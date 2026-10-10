//! Where this copy last saw the members of its chain (da-rugg8), and the
//! handshake a sync session opens with. Every sync is between members of a
//! chain (da-6177s): `sync_peers` is an address book, and grants nothing.
//! Here rather than
//! in the transport so that every build runs them — the native one's iroh
//! session, and datum.day's, whose transport is in the page and reaches these
//! through the CLI (`datum sync handshake`, da-6kpj5).

use chrono::{SecondsFormat, Utc};
use rusqlite::params;

use super::chain::{self, Chain, Role};
use super::endpoint;
use crate::db::Db;
use crate::error::{DatumError, Refusal, Result};

/// A copy of this database that dialing it, or dialing from it, found.
#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub label: String,
    /// Where it was last seen, or where a member said it is (the transport's
    /// address, as JSON); empty for a member only `sync record-session` has
    /// named.
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

    /// A copy this one syncs with: an unrevoked member of its chain as it
    /// serves it ([`Chain::served`]), or, in none yet, a copy it is joining
    /// one through ([`Db::sync_joining`]).
    pub fn sync_is_peer(&self, id: &str) -> Result<bool> {
        let chain = Chain::served(&self.conn)?;
        if !chain.exists() {
            return Ok(self.sync_joining()?.iter().any(|p| p.id == id));
        }
        Ok(chain.member(id).is_some_and(|m| m.revoked.is_none()))
    }

    /// The copies this one is joining a sync chain through (da-idw54): in
    /// no chain, those that admitted its invite (da-38eye) or that it met in
    /// a handshake whose ops it holds. A first sync carries the inviter's
    /// history before its chain records — what it held got its ops before
    /// its chain began — so a join cut short (a reload, a lost connection)
    /// leaves the copy holding part of it, or nothing at all, and in no
    /// chain yet; these are where it resumes. A copy that left a chain
    /// holds none of its ops, and starting it cleared every mark, so none
    /// of its old peers is one.
    pub fn sync_joining(&self) -> Result<Vec<Peer>> {
        if self.sync_chain()?.exists() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT endpoint_id, label, addrs, added_at, last_seen FROM sync_peers p
             WHERE redeemed_at IS NOT NULL
                OR EXISTS (SELECT 1 FROM ops WHERE endpoint = p.endpoint_id)
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

    /// What a dialer labelled `label` is told when this copy's chain does not
    /// hold it: words that read on both ends, and what would fix it.
    fn not_a_member(&self, label: &str) -> Result<String> {
        let chain = self.sync_chain()?;
        let no = "not a member of this sync chain";
        Ok(match chain.member(&self.sync_endpoint_id()?) {
            Some(me) => format!(
                "{no} as {0} holds it: if {label} has joined, sync {0} with the copy that invited it",
                me.label
            ),
            None => no.into(),
        })
    }

    /// The members' addresses a peer passed on in its handshake (da-fly3f):
    /// `addrs` is a JSON list of them, each naming its `id`. Two copies one
    /// inviter admitted have never dialed each other, and this is how each
    /// learns where the other is. Kept for a member this copy has not itself
    /// seen; an address a handshake gave stands.
    pub fn sync_introduce(&self, addrs: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let chain = self.sync_chain()?;
        let mine = endpoint(&tx)?.id;
        let addrs: Vec<serde_json::Value> = serde_json::from_str(addrs)
            .map_err(|e| DatumError::Other(format!("not a list of addresses: {e}")))?;
        for a in addrs {
            let member = a["id"].as_str().and_then(|id| chain.member(id));
            let Some(m) = member.filter(|m| m.endpoint != mine) else {
                continue;
            };
            // Only a row no handshake has filled: one never seen takes the
            // newest address passed on, one seen takes it while it has none.
            tx.execute(
                "INSERT INTO sync_peers (endpoint_id, label, addrs, added_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (endpoint_id) DO UPDATE SET addrs = excluded.addrs
                 WHERE last_seen IS NULL OR addrs = ''",
                params![m.endpoint, m.label, a.to_string(), now()],
            )?;
        }
        Ok(tx.commit()?)
    }

    /// A session's handshake, as it bears on this copy: `id` introduced
    /// itself as `label` at `addrs`. Dialed (it answered this copy's dial):
    /// recorded, and marked as where this copy redeemed `invite`, when it
    /// did. Served (it dialed this copy): admitted — a member, or the
    /// redeemer of `invite` — and recorded; anyone else is refused, and the
    /// error says why. An invite presented is redeemed or refused, even by a
    /// member: one use. A redeemer says which `chain` it is in (empty: none);
    /// one in another chain is refused, as is one that does not say, since a
    /// copy belongs to one chain (da-mepav). It says how many entries it
    /// `holds` too, which a reader invite refuses (da-tti3w). Whether `id` is
    /// new to this copy comes back. A paused copy refuses every session
    /// (da-wlbyx).
    ///
    /// A member admitted since this copy last synced is no member as this
    /// copy holds the chain, and is refused as [`Refusal::NotMember`]. Asked
    /// again with `records`, the dialer's own chain records as op lines, it
    /// is admitted when they make it one (da-2u95o: [`Chain::load_with`]).
    /// Nothing of a member's is kept: the session brings every op this copy
    /// lacks, those among them. A reader brings none but its own, so its
    /// records are kept (da-91wab): applied as any received, they wait on
    /// what this copy lacks of its inviter's, and the copy serves by them
    /// meanwhile ([`Chain::served`]).
    #[allow(clippy::too_many_arguments)]
    pub fn sync_handshake(
        &self,
        id: &str,
        label: &str,
        addrs: &str,
        invite: Option<&str>,
        chain: Option<&str>,
        holds: u64,
        records: Option<&str>,
        dialed: bool,
    ) -> Result<bool> {
        super::pause::refuse_paused(&self.conn)?;
        super::debug::refuse(&self.conn)?;
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
                None => {
                    let (shown, read) = Chain::load_with(&self.conn, records.unwrap_or(""))?;
                    match shown.member(id).filter(|m| m.revoked.is_none()) {
                        Some(m) if m.role() == Role::Reader => {
                            self.sync_apply(&read)?;
                        }
                        Some(_) => {}
                        None => {
                            return Err(DatumError::Refused(
                                Refusal::NotMember,
                                self.not_a_member(label)?,
                            ))
                        }
                    }
                }
            }
        }
        // Asked who it is where it writes: a session's file purged since the
        // look above keeps no row of its peer (da-otz0d).
        let tx = self.conn.unchecked_transaction()?;
        endpoint(&tx)?;
        self.sync_record_peer(id, label, addrs)?;
        if dialed && invite.is_some() {
            tx.execute(
                "UPDATE sync_peers SET redeemed_at = ?2 WHERE endpoint_id = ?1",
                params![id, now()],
            )?;
        }
        tx.commit()?;
        Ok(!known)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn id(db: &Db) -> String {
        db.sync_endpoint_id().unwrap()
    }

    fn genesis(db: &Db, label: &str) {
        let record = json!({"type": "genesis", "chain": "c1", "admins": [], "label": label});
        db.sync_chain_write(record, true).unwrap();
    }

    /// `member` admits `copy` by a new invite granting `caps`, and sends it
    /// all it holds.
    fn admit(member: &Db, copy: &Db, label: &str, caps: Value) {
        let invite = json!({
            "type": "invite", "chain": "c1", "invite_id": label, "caps": caps,
            "secret_hash": chain::secret_hash(label), "expires": "2099-01-01T00:00:00Z",
        });
        member.sync_chain_write(invite, true).unwrap();
        let hello = member.sync_handshake(
            &id(copy),
            label,
            "{}",
            Some(label),
            Some(""),
            0,
            None,
            false,
        );
        assert!(hello.unwrap());
        send(member, copy);
    }

    /// A session's file purged under it takes no peer row (da-otz0d): each of
    /// the three that write one asks who the copy is, and no key is minted.
    #[test]
    fn a_sessions_purged_file_takes_no_peer_row() {
        let db = Db::open_in_memory().unwrap();
        db.sync_session_as(Some(&id(&db)));
        db.force_recreate_schema().unwrap();
        let peer = "ab".repeat(32);
        let hello = db.sync_handshake(&peer, "b", "{}", None, None, 0, None, true);
        let ended = db.sync_record_session(&peer, None, Some(true), None);
        let met = db.sync_introduce(&json!([{ "id": peer }]).to_string());
        db.sync_session_as(None);
        for refused in [hello.map(drop), ended, met] {
            let why = refused.unwrap_err().to_string();
            assert!(why.contains("endpoint key changed"), "{why}");
        }
        let rows = "SELECT (SELECT COUNT(*) FROM sync_peers) + (SELECT COUNT(*) FROM sync_local)";
        let rows: i64 = db.conn.query_row(rows, [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }

    /// `from` sends `to` what it lacks, as a session does.
    fn send(from: &Db, to: &Db) {
        let ops = from.sync_ops_since(&to.sync_vector().unwrap(), false, Some(&id(to)));
        to.sync_apply(&ops.unwrap().join("\n")).unwrap();
    }

    fn log(db: &Db, field: &str) {
        let verb = crate::entry::Verb::Point;
        let e = crate::entry::Entry::new(field, verb, None, None, None, None, "t");
        db.insert_entry(&e).unwrap();
    }

    fn fields(db: &Db) -> String {
        let sql =
            "SELECT COALESCE(GROUP_CONCAT(field), '') FROM (SELECT field FROM entries ORDER BY 1)";
        db.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    /// A member admitted since this copy last synced is admitted by its own
    /// chain records, and by nothing less (da-2u95o): not without them, not
    /// by another's, nor by another's rewritten to name it, not once the
    /// records shown revoke it, and not by a genesis of its own
    /// for this chain, though it is the earlier. Nothing shown is kept. One
    /// a member admitted is admitted in turn, until this copy holds that
    /// member as revoked: then only the revoked one's clock says which came
    /// first.
    #[test]
    fn a_member_not_heard_of_is_admitted_by_its_chain_records_alone() {
        let [a, b, c, x, y] = [(); 5].map(|()| Db::open_in_memory().unwrap());
        genesis(&x, "x");
        std::thread::sleep(std::time::Duration::from_millis(2));
        genesis(&a, "a");
        admit(&a, &b, "b", json!({}));
        admit(&a, &c, "c", json!({}));
        let records = |of: &Db| of.sync_chain_records().unwrap().join("\n");
        let hello = |who: &Db, shown: Option<String>| {
            b.sync_handshake(&id(who), "l", "{}", None, None, 0, shown.as_deref(), false)
        };
        let refused = |what: &str, who: &Db, shown: Option<String>| match hello(who, shown) {
            Err(DatumError::Refused(Refusal::NotMember, why)) => why,
            other => panic!("{what}: {other:?}"),
        };
        assert_eq!(
            refused("showing nothing", &c, None),
            "not a member of this sync chain as b holds it: if l has joined, sync b with the \
             copy that invited it"
        );
        refused("records without its admission", &c, Some(records(&b)));
        refused("another's records", &x, Some(records(&c)));
        let rewritten = records(&c).replace(&id(&c), &id(&x));
        refused("another's records naming it", &x, Some(rewritten));
        refused("a genesis of its own", &x, Some(records(&x)));
        let held = records(&b);
        assert!(hello(&c, Some(records(&c))).unwrap());
        assert_eq!(records(&b), held);
        assert!(b.sync_chain().unwrap().member(&id(&c)).is_none());

        admit(&c, &y, "y", json!({}));
        assert!(hello(&y, Some(records(&y))).unwrap());
        let revoke = json!({"type": "revoke", "chain": "c1", "endpoint": id(&c)});
        a.sync_chain_write(revoke, true).unwrap();
        refused("revoked by the records shown", &c, Some(records(&a)));
        b.sync_apply(&records(&a)).unwrap();
        let by = "admitted by one this copy holds as revoked";
        refused(by, &y, Some(records(&y)));
    }

    /// A reader not heard of is admitted by its chain records too, and they
    /// are kept (da-91wab): it brings no op of its inviter's, so past an
    /// entry of the inviter's they wait, and this copy serves by them. The
    /// reader is a peer from then on, is sent what its filter passes and
    /// verified over it, and `sync status` lists it. A plain reader holds
    /// the inviter's entry, which this copy lacks until the inviter's ops
    /// reach it: then the records apply, and the two agree.
    #[test]
    fn a_reader_not_heard_of_is_kept_by_its_chain_records_and_served_by_them() {
        let filtered = json!({"role": "reader", "fields": ["tea"], "of": 3});
        for caps in [json!({"role": "reader"}), filtered] {
            let plain = caps["fields"].is_null();
            let [a, b, r] = [(); 3].map(|()| Db::open_in_memory().unwrap());
            genesis(&a, "a");
            admit(&a, &b, "b", json!({}));
            log(&a, "coffee");
            admit(&a, &r, "r", caps);
            log(&b, "tea");
            log(&b, "juice");
            let records = |of: &Db| of.sync_chain_records().unwrap().join("\n");
            let hello = |shown: Option<String>| {
                b.sync_handshake(&id(&r), "r", "{}", None, None, 0, shown.as_deref(), false)
            };
            assert!(hello(None).is_err() && hello(Some(records(&b))).is_err());
            assert!(hello(Some(records(&r))).unwrap());
            assert!(b.sync_chain().unwrap().member(&id(&r)).is_none());
            assert!(!hello(None).unwrap(), "known by the records kept");

            // What passes its filter and no more, before the reader could drop it.
            let since = r.sync_vector().unwrap();
            let sent = b.sync_ops_since(&since, false, Some(&id(&r))).unwrap();
            assert_eq!(sent.iter().any(|l| l.contains("juice")), plain);
            send(&b, &r);
            assert_eq!(fields(&r), if plain { "coffee,juice,tea" } else { "tea" });
            let hash = |of: &Db, with: &Db| of.sync_state_hash(Some(&id(with))).unwrap().0;
            assert_eq!(hash(&b, &r) == hash(&r, &b), !plain);
            let listed = |of: &Db| {
                let all = of.sync_status().unwrap();
                let r = all.into_iter().find(|p| p.endpoint == id(&r));
                r.map(|p| (p.reader, p.filter.map(|f| f.describe())))
            };
            let filter = (!plain).then(|| "filtered: 1 of 3 fields".to_string());
            assert_eq!(listed(&b), Some((true, filter.clone())));

            send(&a, &b);
            assert!(b.sync_chain().unwrap().member(&id(&r)).is_some());
            assert_eq!(
                (listed(&b), listed(&a)),
                (Some((true, filter.clone())), Some((true, filter)))
            );
            assert_eq!(hash(&b, &r), hash(&r, &b));
        }
    }
}
