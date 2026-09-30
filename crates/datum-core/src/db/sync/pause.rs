//! `datum sync pause | resume` (da-wlbyx): a flag on this copy, never
//! synced — pausing one device is that device's business — under which it
//! neither sends nor receives. And the warning a write gives when a copy
//! with other members has not synced for a day, whatever the cause.

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use super::endpoint;
use crate::db::Db;
use crate::error::{DatumError, Result};

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
        Some(at) => Err(DatumError::Other(format!(
            "sync is paused on this copy since {at}: `datum sync resume` resumes it"
        ))),
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

    /// The warning a write gives, at most once a day: this copy is in a chain
    /// with another member and has synced with none for over a day. It names
    /// the cause it knows — the pause, or the last session's error. A lone
    /// member gets none: its danger is the same, paused or not.
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
        // A peer never synced with counts from when it was first heard of.
        let last: Option<String> = self.conn.query_row(
            "SELECT MAX(COALESCE(last_seen, added_at)) FROM sync_peers",
            [],
            |r| r.get(0),
        )?;
        if today(warned.as_deref()) || last.is_none() || today(last.as_deref()) {
            return Ok(None);
        }
        let (chain, mine) = (self.sync_chain()?, self.sync_endpoint_id()?);
        let others: Vec<_> = chain
            .members
            .iter()
            .filter(|m| m.revoked.is_none() && m.endpoint != mine)
            .map(|m| m.endpoint.as_str())
            .collect();
        let mut stmt = self.conn.prepare(
            "SELECT endpoint_id, COALESCE(last_seen, added_at), last_error FROM sync_peers
             ORDER BY last_error_at DESC",
        )?;
        let peers = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let peers: Vec<_> = peers
            .into_iter()
            .filter(|(id, _, _)| others.contains(&id.as_str()))
            .collect();
        let Some(last) = peers.iter().map(|(_, at, _)| at).max() else {
            return Ok(None);
        };
        let Ok(last) = DateTime::parse_from_rfc3339(last) else {
            return Ok(None);
        };
        let days = (Utc::now() - last.with_timezone(&Utc)).num_days();
        if days < 1 {
            return Ok(None);
        }
        let cause = match (paused, peers.iter().find_map(|(_, _, e)| e.as_ref())) {
            (Some(at), _) => format!(" (paused since {at})"),
            (None, Some(e)) => format!(" (last error: {e})"),
            (None, None) => String::new(),
        };
        self.conn
            .execute("UPDATE sync_local SET warned_at = ?1", params![now()])?;
        Ok(Some(format!(
            "⚠ not synced for {days} day{}{cause} — `datum sync status`",
            if days == 1 { "" } else { "s" }
        )))
    }
}
