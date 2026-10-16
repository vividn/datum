//! What `datum sync status` says (da-rcb5m): per chain member, how this copy
//! stands with it — from the last session the transport recorded, native or
//! the browser's, and what this copy holds that the member's last-seen
//! vector lacks. Every state carries its time: it is a claim about a moment.

use std::collections::BTreeMap;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};

use super::chain::{Chain, Filter, Member, Role};
use super::{belongs_to, clock_ahead, filtered_lines, now_ms, row, row_to_entry, unhex};
use crate::db::Db;
use crate::error::{DatumError, Result};
use crate::migrations::CURRENT_SCHEMA_VERSION;

/// One member, as `sync status` reports it.
#[derive(Debug, Clone)]
pub struct PeerStatus {
    pub endpoint: String,
    pub client: Option<String>,
    pub label: String,
    pub kind: Option<String>,
    pub revoked: bool,
    /// Every op of its set aside (da-ry11c).
    pub purged: bool,
    /// A reader (da-9yvb6): it receives, and writes nothing.
    pub reader: bool,
    /// A filtered reader's filter (da-u81u1).
    pub filter: Option<Filter>,
    /// `synced`, `pending`, `offline`, `error` or `never`: `offline` when
    /// the last session failed before it reached the member (da-ueb4p).
    pub state: &'static str,
    pub to_send: i64,
    pub last_seen: Option<String>,
    /// When the last verify ran, and whether the hashes matched.
    pub last_verify: Option<(String, bool)>,
    pub error: Option<String>,
    pub error_at: Option<String>,
    /// Where it was last seen, or a member said it is (the transport's
    /// address, as JSON), if anywhere.
    pub addr: Option<String>,
}

impl PeerStatus {
    /// What `sync` and `sync status` say of a member this copy cannot dial:
    /// it has never met it, and no member has passed its address on yet
    /// (da-fly3f). Its ops arrive meanwhile through the members of `all`,
    /// the status it is one row of, that this copy does dial: a reader is
    /// none of them, it relays nothing.
    pub fn no_address(&self, all: &[PeerStatus]) -> Option<String> {
        if self.state != "never" || self.addr.is_some() || self.revoked {
            return None;
        }
        let via: Vec<&str> = all
            .iter()
            .filter(|p| p.addr.is_some() && !p.revoked && !p.reader)
            .map(|p| p.label.as_str())
            .collect();
        Some(match via.is_empty() {
            true => "no address for it yet".into(),
            false => format!("no address for it yet: it syncs through {}", via.join(", ")),
        })
    }
}

/// Ops from one device held because its clock runs ahead of this copy's
/// (da-ns8dq): distinct from ops held for a missing earlier op.
#[derive(Debug, Clone)]
pub struct ClockWait {
    pub endpoint: String,
    pub label: String,
    pub ops: usize,
    /// How far ahead its latest held op is, in ms.
    pub ahead_ms: i64,
}

impl std::fmt::Display for ClockWait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let min = self.ahead_ms / 60_000;
        let (n, unit) = [
            (525_600, "year"),
            (1_440, "day"),
            (60, "hour"),
            (1, "minute"),
        ]
        .into_iter()
        .find(|(m, _)| min >= *m)
        .map_or((0, "minute"), |(m, u)| ((min + m / 2) / m, u));
        write!(
            f,
            "{} change{} from {} {} waiting: its clock is {n} {unit}{} ahead. \
             Fix that device's clock; they apply once the time catches up.",
            self.ops,
            if self.ops == 1 { "" } else { "s" },
            self.label,
            if self.ops == 1 { "is" } else { "are" },
            if n == 1 { "" } else { "s" },
        )
    }
}

type Session = (
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<bool>,
    Option<String>,
    Option<String>,
    Option<String>,
);

impl Db {
    /// Record how a session with `endpoint` ended: the vector it had at the
    /// end, its verify, and the error it failed with (none: it succeeded,
    /// which clears the last one), `offline` when it failed before it reached
    /// the peer. What is not given is kept, with one
    /// exception: a session that succeeded and gave no verify, one copy
    /// being ahead of the other, clears the verify recorded before it,
    /// which says nothing of the two copies now (da-t3ey2). A failed
    /// session's vector is the peer's as its last round began, before that
    /// round's ops reached it, so `to_send` may count more than is waiting,
    /// never less. A failed session that changes neither the error nor the
    /// vector writes nothing. A chain
    /// member gets a row if it has none; anyone else has none to update.
    /// Nor has a session's file that was purged under it: it is asked who it
    /// is in the transaction that writes (da-otz0d).
    pub fn sync_record_session(
        &self,
        endpoint: &str,
        vector: Option<&BTreeMap<String, i64>>,
        verify: Option<bool>,
        error: Option<&str>,
        offline: bool,
    ) -> Result<()> {
        let offline = offline && error.is_some();
        if unhex::<32>(endpoint).is_none() {
            return Err(DatumError::Other(format!("not an endpoint id: {endpoint}")));
        }
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let vector = vector.map(serde_json::to_string).transpose()?;
        // A failure the row already says writes nothing (da-u1ul1): a peer
        // that is down is dialed again and again, failing alike each time,
        // and `last_error_at` stays when it first failed so.
        if error.is_some() && verify.is_none() {
            let same: Option<bool> = self
                .conn
                .query_row(
                    "SELECT last_error IS ?2 AND last_error_offline = ?4
                            AND (?3 IS NULL OR last_vector IS ?3)
                     FROM sync_peers WHERE endpoint_id = ?1",
                    params![endpoint, error, vector, offline],
                    |r| r.get(0),
                )
                .optional()?;
            if same == Some(true) {
                return Ok(());
            }
        }
        // The chain is read before the lock (da-8i7f0).
        self.compute_then_write_with(
            |_| Ok(self.sync_chain()?.member(endpoint).map(|m| m.label.clone())),
            |tx, label| {
                super::endpoint(tx)?;
                if let Some(label) = label {
                    tx.execute(
                "INSERT OR IGNORE INTO sync_peers (endpoint_id, label, addrs, added_at, announced)
                 VALUES (?1, ?2, '', ?3, 1)",
                params![endpoint, label, now],
            )?;
                }
                let n = tx.execute(
                    "UPDATE sync_peers SET
               last_vector = COALESCE(?2, last_vector),
               last_verify_at = CASE WHEN ?3 IS NOT NULL THEN ?5
                                     WHEN ?4 IS NOT NULL THEN last_verify_at
                                     ELSE NULL END,
               last_verify_match = CASE WHEN ?4 IS NULL THEN ?3
                                        ELSE COALESCE(?3, last_verify_match) END,
               last_error = ?4,
               last_error_at = CASE WHEN ?4 IS NULL THEN NULL ELSE ?5 END,
               last_error_offline = ?6,
               last_seen = CASE WHEN ?4 IS NULL THEN ?5 ELSE last_seen END
             WHERE endpoint_id = ?1",
                    params![endpoint, vector, verify, error, now, offline],
                )?;
                if n == 0 {
                    return Err(DatumError::Other(format!(
                        "{endpoint} is not a member of this copy's sync chain"
                    )));
                }
                Ok(())
            },
        )
    }

    /// The devices whose ops wait for this copy's clock to catch up with
    /// theirs, each named as the chain names it.
    pub fn sync_clock_waits(&self) -> Result<Vec<ClockWait>> {
        let now = now_ms();
        let mut by: BTreeMap<String, (usize, i64)> = BTreeMap::new();
        let mut stmt = self
            .conn
            .prepare("SELECT endpoint, hlc FROM ops WHERE applied = 0")?;
        for r in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (endpoint, hlc) = r?;
            if let Some(ahead) = clock_ahead(&hlc, now) {
                let w = by.entry(endpoint).or_default();
                *w = (w.0 + 1, w.1.max(ahead));
            }
        }
        let chain = self.sync_chain()?;
        Ok(by
            .into_iter()
            .map(|(endpoint, (ops, ahead_ms))| ClockWait {
                label: chain.member(&endpoint).map_or_else(
                    || endpoint[..10.min(endpoint.len())].into(),
                    |m| m.label.clone(),
                ),
                endpoint,
                ops,
                ahead_ms,
            })
            .collect())
    }

    /// Every member of the chain but this copy and those that left, as it serves it
    /// ([`Chain::served`]) — in none yet, every copy it is joining one
    /// through — and how it stands with it.
    pub fn sync_status(&self) -> Result<Vec<PeerStatus>> {
        let chain = Chain::served(&self.conn)?;
        let mine = self.sync_endpoint_id()?;
        // A reader never relays and signs no baselines (`sync_ops_since`):
        // what it has to send is its own ops alone (da-b2krf).
        let reader = chain.member(&mine).map(Member::role) == Some(Role::Reader);
        let mut held = self.sync_vector()?;
        held.retain(|e, _| !reader || *e == mine);
        // Entries no op covers yet go as baselines on the next session: to a
        // filtered reader, those of its fields alone. By the field each
        // belongs to.
        let uncovered: Vec<Option<String>> = self
            .conn
            .prepare("SELECT * FROM entries WHERE NOT ?1 AND hid NOT IN (SELECT hid FROM ops)")?
            .query_map([reader], row_to_entry)?
            .map(|e| e.map(|e| belongs_to(&row(&e))))
            .collect::<rusqlite::Result<_>>()?;
        // In no chain yet, the copies it is joining one through (da-idw54).
        let joining: Vec<Member> = self
            .sync_joining()?
            .into_iter()
            .map(|p| Member {
                endpoint: p.id,
                label: p.label,
                admitted: p.added_at,
                ..Default::default()
            })
            .collect();
        let mut out = Vec::new();
        // A member that left (da-dsob1) is no copy to stand with.
        for m in chain
            .members
            .iter()
            .chain(&joining)
            .filter(|m| m.endpoint != mine && m.left.is_none())
        {
            let row: Option<Session> = self
                .conn
                .query_row(
                    "SELECT last_seen, last_vector, last_verify_at, last_verify_match,
                            last_error, last_error_at, NULLIF(addrs, ''), last_error_offline
                     FROM sync_peers WHERE endpoint_id = ?1",
                    [&m.endpoint],
                    |r| {
                        Ok((
                            r.get(7)?,
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )
                .optional()?;
            let (
                offline,
                last_seen,
                vector,
                verify_at,
                verify_match,
                mut error,
                mut error_at,
                addr,
            ) = row.unwrap_or_default();
            let theirs: BTreeMap<String, i64> = vector
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default();
            // A filtered reader is sent only what passes its filter, so what
            // it lacks past a gap it will never be sent is not waiting.
            let filter = chain.filter(&m.endpoint);
            let to_send = match filter {
                Some(f) => {
                    let only = reader.then_some(mine.as_str());
                    let baselines = uncovered.iter().filter(|b| f.admits(b.as_deref())).count();
                    (baselines + filtered_lines(&self.conn, &theirs, f, &chain, only)?.1) as i64
                }
                None => {
                    uncovered.len() as i64
                        + held
                            .iter()
                            .map(|(e, s)| (s - theirs.get(e).copied().unwrap_or(0)).max(0))
                            .sum::<i64>()
                }
            };
            let last_verify = verify_at.zip(verify_match);
            let state = match (&error, last_verify.as_ref()) {
                (Some(_), _) if !offline => "error",
                // A mismatch stands until a session says otherwise: a write
                // since does not make it merely pending, nor the member
                // being down since.
                (_, Some((at, false))) => {
                    error = Some("verify MISMATCH".into());
                    error_at = Some(at.clone());
                    "error"
                }
                // It never answered: not up, which is no fault (da-ueb4p).
                (Some(_), _) => "offline",
                // No session yet: a row may hold only an address passed on.
                _ if last_seen.is_none() => "never",
                _ if to_send > 0 => "pending",
                (_, Some((_, true))) => "synced",
                // Nothing to send, but no verify says the copies agree.
                (_, None) => "pending",
            };
            // Nothing waits for a member this copy cannot try (da-fly3f).
            let to_send = match state == "never" && addr.is_none() {
                true => 0,
                false => to_send,
            };
            out.push(PeerStatus {
                endpoint: m.endpoint.clone(),
                client: m.client.clone(),
                label: m.label.clone(),
                kind: m.kind.clone(),
                revoked: m.revoked.is_some(),
                purged: m.purged.is_some(),
                reader: m.role() == Role::Reader,
                filter: filter.cloned(),
                state,
                to_send,
                last_seen,
                last_verify,
                error,
                error_at,
                addr,
            });
        }
        Ok(out)
    }
}

impl Db {
    /// Record what `endpoint` said it runs in a handshake (da-3zqyv). When
    /// its schema is newer than this copy's, and newly so, the warning to
    /// upgrade comes back, to print once: syncing goes on regardless.
    pub fn sync_record_version(
        &self,
        endpoint: &str,
        datum: &str,
        schema: u32,
    ) -> Result<Option<String>> {
        let before: Option<(String, Option<u32>)> = self
            .conn
            .query_row(
                "SELECT label, schema FROM sync_peers WHERE endpoint_id = ?1",
                [endpoint],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((label, was)) = before else {
            return Ok(None);
        };
        self.conn.execute(
            "UPDATE sync_peers SET datum = ?2, schema = ?3 WHERE endpoint_id = ?1",
            params![endpoint, datum, schema],
        )?;
        let label = self
            .sync_chain()?
            .member(endpoint)
            .map_or(label, |m| m.label.clone());
        Ok((schema > CURRENT_SCHEMA_VERSION && was != Some(schema))
            .then(|| upgrade(&label, datum, schema)))
    }

    /// What each peer last said it runs, by endpoint: its build and schema.
    pub fn sync_versions(&self) -> Result<BTreeMap<String, (String, u32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT endpoint_id, COALESCE(datum, ''), schema FROM sync_peers
             WHERE schema IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A warning for each member of the chain whose schema is newer than
    /// this copy's, for `sync status` and `datum check`.
    pub fn sync_upgrade_warnings(&self) -> Result<Vec<String>> {
        let versions = self.sync_versions()?;
        Ok(self
            .sync_chain()?
            .members
            .iter()
            .filter(|m| m.revoked.is_none())
            .filter_map(|m| {
                let (datum, schema) = versions.get(&m.endpoint)?;
                (*schema > CURRENT_SCHEMA_VERSION).then(|| upgrade(&m.label, datum, *schema))
            })
            .collect())
    }
}

/// What a copy behind a member says: both versions, and how to upgrade.
fn upgrade(label: &str, datum: &str, schema: u32) -> String {
    let how = match cfg!(target_family = "wasm") {
        true => "reload datum.day to update this copy",
        false => "to upgrade this copy, `git pull && scripts/install.sh`",
    };
    format!(
        "{label} runs datum {datum} (schema {schema}), newer than this copy's {} \
         (schema {CURRENT_SCHEMA_VERSION}); syncing goes on, but {how}",
        crate::build()
    )
}

#[cfg(test)]
mod tests {
    use super::PeerStatus;

    fn peer(label: &str, addr: bool) -> PeerStatus {
        PeerStatus {
            endpoint: label.into(),
            client: None,
            label: label.into(),
            kind: None,
            revoked: false,
            purged: false,
            reader: false,
            filter: None,
            state: if addr { "synced" } else { "never" },
            to_send: 0,
            last_seen: None,
            last_verify: None,
            error: None,
            error_at: None,
            addr: addr.then(String::new),
        }
    }

    /// A member with no address syncs through the members this copy dials:
    /// not a reader, which relays nothing, nor a revoked one; and with none
    /// to name, it names none.
    #[test]
    fn no_address_names_only_the_members_that_relay() {
        let c = peer("c", false);
        let reader = PeerStatus {
            reader: true,
            ..peer("r", true)
        };
        let revoked = PeerStatus {
            revoked: true,
            ..peer("x", true)
        };
        let all = [peer("a", true), reader.clone(), revoked.clone(), c.clone()];
        let said = c.no_address(&all);
        assert_eq!(
            said.as_deref(),
            Some("no address for it yet: it syncs through a")
        );
        let said = c.no_address(&[reader, revoked, c.clone()]);
        assert_eq!(said.as_deref(), Some("no address for it yet"));
        assert_eq!(all[0].no_address(&all), None);
    }
}
