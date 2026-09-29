//! What `datum sync status` says (da-rcb5m): per chain member, how this copy
//! stands with it — from the last session the transport recorded, native or
//! the browser's, and what this copy holds that the member's last-seen
//! vector lacks. Every state carries its time: it is a claim about a moment.

use std::collections::BTreeMap;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};

use super::unhex;
use crate::db::Db;
use crate::error::{DatumError, Result};

/// One member, as `sync status` reports it.
#[derive(Debug, Clone)]
pub struct PeerStatus {
    pub endpoint: String,
    pub device: Option<String>,
    pub label: String,
    pub client: Option<String>,
    pub revoked: bool,
    /// `synced`, `pending`, `error` or `never`.
    pub state: &'static str,
    pub to_send: i64,
    pub last_seen: Option<String>,
    /// When the last verify ran, and whether the hashes matched.
    pub last_verify: Option<(String, bool)>,
    pub error: Option<String>,
    pub error_at: Option<String>,
    /// Where it was last seen (the transport's address, as JSON), if ever.
    pub addr: Option<String>,
}

type Session = (
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
    /// which clears the last one). What is not given is kept. A failed
    /// session's vector is the peer's as its last round began, before that
    /// round's ops reached it, so `to_send` may count more than is waiting,
    /// never less. A chain
    /// member gets a row if it has none; anyone else has none to update.
    pub fn sync_record_session(
        &self,
        endpoint: &str,
        vector: Option<&BTreeMap<String, i64>>,
        verify: Option<bool>,
        error: Option<&str>,
    ) -> Result<()> {
        if unhex::<32>(endpoint).is_none() {
            return Err(DatumError::Other(format!("not an endpoint id: {endpoint}")));
        }
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        if let Some(m) = self.sync_chain()?.member(endpoint) {
            self.conn.execute(
                "INSERT OR IGNORE INTO sync_peers (endpoint_id, label, addrs, added_at, announced)
                 VALUES (?1, ?2, '', ?3, 1)",
                params![endpoint, m.label, now],
            )?;
        }
        let vector = vector.map(serde_json::to_string).transpose()?;
        let n = self.conn.execute(
            "UPDATE sync_peers SET
               last_vector = COALESCE(?2, last_vector),
               last_verify_at = CASE WHEN ?3 IS NULL THEN last_verify_at ELSE ?5 END,
               last_verify_match = COALESCE(?3, last_verify_match),
               last_error = ?4,
               last_error_at = CASE WHEN ?4 IS NULL THEN NULL ELSE ?5 END,
               last_seen = CASE WHEN ?4 IS NULL THEN ?5 ELSE last_seen END
             WHERE endpoint_id = ?1",
            params![endpoint, vector, verify, error, now],
        )?;
        if n == 0 {
            return Err(DatumError::Other(format!(
                "{endpoint} is not a member of this copy's sync chain"
            )));
        }
        Ok(())
    }

    /// Every member of the chain but this copy, and how it stands with it.
    pub fn sync_status(&self) -> Result<Vec<PeerStatus>> {
        let chain = self.sync_chain()?;
        let mine = self.sync_endpoint_id()?;
        let held = self.sync_vector()?;
        // Entries no op covers yet go as baselines on the next session.
        let uncovered: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE hid NOT IN (SELECT hid FROM ops)",
            [],
            |r| r.get(0),
        )?;
        let mut out = Vec::new();
        for m in chain.members.iter().filter(|m| m.endpoint != mine) {
            let row: Option<Session> = self
                .conn
                .query_row(
                    "SELECT last_seen, last_vector, last_verify_at, last_verify_match,
                            last_error, last_error_at, NULLIF(addrs, '')
                     FROM sync_peers WHERE endpoint_id = ?1",
                    [&m.endpoint],
                    |r| {
                        Ok((
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
            let had_session = row.is_some();
            let (last_seen, vector, verify_at, verify_match, mut error, mut error_at, addr) =
                row.unwrap_or_default();
            let theirs: BTreeMap<String, i64> = vector
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default();
            let to_send = uncovered
                + held
                    .iter()
                    .map(|(e, s)| (s - theirs.get(e).copied().unwrap_or(0)).max(0))
                    .sum::<i64>();
            let last_verify = verify_at.zip(verify_match);
            let state = match (&error, last_verify.as_ref()) {
                (Some(_), _) => "error",
                _ if !had_session => "never",
                // A mismatch stands until a session says otherwise: a write
                // since does not make it merely pending.
                (_, Some((at, false))) => {
                    error = Some("verify MISMATCH".into());
                    error_at = Some(at.clone());
                    "error"
                }
                _ if to_send > 0 => "pending",
                (_, Some((_, true))) => "synced",
                // Nothing to send, but no verify says the copies agree.
                (_, None) => "pending",
            };
            out.push(PeerStatus {
                endpoint: m.endpoint.clone(),
                device: m.device.clone(),
                label: m.label.clone(),
                client: m.client.clone(),
                revoked: m.revoked.is_some(),
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
