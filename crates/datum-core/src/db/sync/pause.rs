//! `datum sync pause | resume` (da-wlbyx): a flag on this copy, never
//! synced — pausing one device is that device's business — under which it
//! neither sends nor receives. And the warning a write gives when changes
//! made on a copy have reached no other member for a day, whatever the cause.

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::endpoint;
use crate::db::Db;
use crate::error::{DatumError, Refusal, Result};

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn paused(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT paused_at FROM sync_local", [], |r| r.get(0))
        .optional()?
        .flatten())
}

/// A paused copy's refusal: of a session's handshake, and of the pipe's ops
/// and apply.
pub(super) fn refuse_paused(conn: &Connection) -> Result<()> {
    match paused(conn)? {
        Some(at) => Err(DatumError::Refused(
            Refusal::Paused,
            format!("sync is paused on this copy since {at}: `datum sync resume` resumes it"),
        )),
        None => Ok(()),
    }
}

/// Within a day of now.
fn today(t: Option<&str>) -> bool {
    t.and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|t| Utc::now() - t.with_timezone(&Utc) < chrono::Duration::days(1))
}

impl Db {
    /// When this copy's sync was paused, if it is.
    pub fn sync_paused(&self) -> Result<Option<String>> {
        paused(&self.conn)
    }

    /// Pause (`true`) or resume this copy's sync. A pause keeps the time it
    /// was first set. What it was before comes back.
    pub fn sync_set_paused(&self, pause: bool) -> Result<Option<String>> {
        endpoint(&self.conn)?;
        let was = paused(&self.conn)?;
        let at = if pause {
            was.clone().or_else(|| Some(now()))
        } else {
            None
        };
        self.conn
            .execute("UPDATE sync_local SET paused_at = ?1", params![at])?;
        Ok(was)
    }

    /// The warning a write gives, at most once a day: changes made on this
    /// copy have reached no other member of its chain, as far as it knows,
    /// the oldest of them over a day ago (da-u10rl). A member counts as
    /// having received what its last known vector covers, however long it
    /// has been off since, so another device being away is no reason. Only a
    /// member that holds everything counts: a copy with none but itself and
    /// filtered readers gets none, as a lone member gets none, since no sync
    /// could carry all of it off. It names the cause it knows — the pause,
    /// or the last session's error.
    pub fn sync_stale_warning(&self) -> Result<Option<String>> {
        let row: Option<(Option<String>, Option<String>)> = self
            .conn
            .query_row("SELECT paused_at, warned_at FROM sync_local", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        let Some((paused, warned)) = row else {
            return Ok(None);
        };
        if today(warned.as_deref()) {
            return Ok(None);
        }
        let (chain, mine) = (self.sync_chain()?, self.sync_endpoint_id()?);
        let peers = self
            .conn
            .prepare(
                "SELECT endpoint_id, COALESCE(json_extract(last_vector, '$.\"' || ?1 || '\"'), 0),
                        last_error
                 FROM sync_peers ORDER BY last_error_at DESC",
            )?
            .query_map([&mine], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // The live members that hold everything: a filtered reader holds a part.
        let peers: Vec<_> = peers
            .into_iter()
            .filter(|(id, _, _)| {
                *id != mine
                    && chain.filter(id).is_none()
                    && chain.member(id).is_some_and(|m| m.revoked.is_none())
            })
            .collect();
        let Some(seq) = peers.iter().map(|p| p.1).max() else {
            return Ok(None);
        };
        // This copy's ops past it: how many, and when the first was made.
        let (n, first): (i64, Option<String>) = self.conn.query_row(
            "SELECT COUNT(*), MIN(hlc) FROM ops WHERE endpoint = ?1 AND seq > ?2",
            params![mine, seq],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let Some(since) = first
            .as_deref()
            .and_then(|h| h.split('/').next())
            .filter(|t| !today(Some(t)))
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        else {
            return Ok(None);
        };
        let cause = match (paused, peers.iter().find_map(|(_, _, e)| e.as_ref())) {
            (Some(at), _) => format!(" (paused since {at})"),
            (None, Some(e)) => format!(" (last error: {e})"),
            (None, None) => String::new(),
        };
        self.conn
            .execute("UPDATE sync_local SET warned_at = ?1", params![now()])?;
        Ok(Some(format!(
            "⚠ {n} change{} made here since {} {} reached no other device{cause} — `datum sync status`",
            if n == 1 { "" } else { "s" },
            since.with_timezone(&chrono::Local).format("%Y-%m-%d"),
            if n == 1 { "has" } else { "have" },
        )))
    }
}
