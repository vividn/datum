//! `datum debug` (da-4dvk7): test situations made in the current database,
//! the first a db full of sync conflicts. A conflict needs a second device,
//! so this mints one — its own endpoint key, kept in `_meta` and nowhere
//! else — whose ops go in through [`Db::sync_apply`]'s own path, so what
//! `datum conflicts` shows is what a real two-device conflict shows. Made-up
//! data stays in a database of its own: one in a sync chain, or holding an
//! entry that is not made up, is refused; and a copy holding the device's
//! ops never starts, joins or syncs with a chain ([`refuse`]).

use ed25519_dalek::{Signer, SigningKey};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use super::{advance, canonical_json, diff, digest, fetch_by_hid, format_hlc, hex, next_hlc};
use super::{apply_in, now_ms, parse_hlc, unhex, vector_where, Chain};
use crate::db::insert::{insert_entry_in_tx_pub, save_existing_entry_with_kind_in_tx};
use crate::db::spec::NOT_META_FIELD_SQL;
use crate::db::tombstone::soft_delete_in_tx;
use crate::db::ConflictStrategy;
use crate::db::Db;
use crate::entry::{Entry, Verb};
use crate::error::{DatumError, Result};
use crate::history::ChangeKind;
use crate::time::DatumTime;

/// The `_meta` key holding this copy's debug device's secret key.
const DEBUG_DEVICE: &str = "debug_device";

/// This copy's debug device, once `debug conflicts` has made one.
fn device(conn: &Connection) -> Result<Option<SigningKey>> {
    let secret: Option<String> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = ?1",
            params![DEBUG_DEVICE],
            |r| r.get(0),
        )
        .optional()?;
    Ok(secret
        .and_then(|s| unhex::<32>(&s))
        .map(|b| SigningKey::from_bytes(&b)))
}

/// A copy holding a debug device's ops starts, joins and syncs with no
/// chain, in either direction.
pub(super) fn refuse(conn: &Connection) -> Result<()> {
    match device(conn)? {
        Some(_) => Err(DatumError::Other(DEBUG_COPY.into())),
        None => Ok(()),
    }
}

/// Said wherever a debug copy is asked to sync.
const DEBUG_COPY: &str = "this copy holds a debug device's ops (`datum debug conflicts`), \
     which must never travel: it does not sync, nor start or join a sync chain";

/// What `debug conflicts` made.
#[derive(Debug)]
pub struct DebugConflicts {
    /// The debug device's endpoint id.
    pub device: String,
    /// The entries, the delete-against-edit last.
    pub hids: Vec<String>,
    /// How many `sync_conflict`s the apply filed.
    pub filed: usize,
}

impl Db {
    /// Make `n` entries whose state this copy and a debug device each edited
    /// without seeing the other's edit, and one more this copy deleted while
    /// the device edited it: `n + 1` sync conflicts, filed by the apply.
    pub fn debug_conflicts(&self, n: usize) -> Result<DebugConflicts> {
        let chain = Chain::load(&self.conn)?;
        if chain.exists() {
            return Err(DatumError::Other(format!(
                "this copy is in sync chain {}: a debug device's ops must never travel; \
                 use a database in no chain (--db)",
                chain.id
            )));
        }
        let key =
            device(&self.conn)?.unwrap_or_else(|| SigningKey::generate(&mut rand::rngs::OsRng));
        let device = hex(key.verifying_key().as_bytes());
        // Specs, layouts and kits are no one's data, and DEBUG's sample
        // seeder tags what it writes `sample=true` (web/src/lib/sample.ts).
        let (real, field): (i64, Option<String>) = self.conn.query_row(
            &format!(
                "SELECT COUNT(*), MIN(field) FROM entries e WHERE {NOT_META_FIELD_SQL}
                   AND COALESCE(json_extract(data, '$.sample'), 0) NOT IN (1, 'true')
                   AND NOT EXISTS (SELECT 1 FROM ops o WHERE o.hid = e.hid AND o.endpoint = ?1)"
            ),
            params![device],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if real > 0 {
            return Err(DatumError::Other(format!(
                "this database holds {real} {} `datum debug` did not make ({}, …): \
                 made-up data goes in a database of its own — `datum --db <scratch> debug \
                 conflicts`, or ?db=<scratch> on datum.day",
                if real == 1 { "entry" } else { "entries" },
                field.unwrap_or_default()
            )));
        }
        let first: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM ops WHERE endpoint = ?1",
            params![device],
            |r| r.get(0),
        )?;
        // One transaction: a run that fails leaves nothing, the device's
        // key included.
        let tx = self.conn.unchecked_transaction()?;
        // What the device saw of each entry (its create), and its row then.
        let mut seen = Vec::new();
        for i in 0..=n {
            let mut at = DatumTime::now();
            at.utc -= chrono::Duration::minutes(i as i64 + 1);
            at.refresh_offset()?;
            let state = Some(json!("logged"));
            let e = Entry::new("debug", Verb::Point, state, None, Some(at), None, "debug");
            insert_entry_in_tx_pub(&tx, &e, ConflictStrategy::Ignore)?;
            let base = fetch_by_hid(&tx, e.hid.as_str())?
                .ok_or_else(|| DatumError::NotFound(e.hid.0.clone()))?;
            seen.push((vector_where(&tx, Some(e.hid.as_str()))?, base));
        }
        for (i, (_, base)) in seen.iter().enumerate() {
            match i == n {
                true => soft_delete_in_tx(&tx, base.hid.as_str(), ChangeKind::UserEdit, None)
                    .map(drop)?,
                false => save_existing_entry_with_kind_in_tx(
                    &tx,
                    &edited(base, "edited here"),
                    ChangeKind::UserEdit,
                )?,
            }
        }
        // The device's edits come after this copy's, so they win the keys;
        // the delete stands whatever the clocks say.
        let mut at = parse_hlc(&next_hlc(&tx)?);
        let mut lines = Vec::new();
        for (seq, (deps, base)) in (first..).zip(&seen) {
            let (ms, counter) = at.unwrap_or_default();
            let mut v = json!({
                "endpoint": device,
                "seq": seq,
                "hlc": format_hlc(ms, counter),
                "slug": base.slug.0,
                "hid": base.hid.as_str(),
                "kind": "update",
                "field": base.field,
                "raw": "debug conflicts (the debug device)",
                "alias_chain": null,
                "effect": diff(base, &edited(base, "edited there")),
                "deps": deps,
            });
            v["sig"] = hex(&key.sign(&digest(&v)).to_bytes()).into();
            lines.push(canonical_json(&v));
            at = Some(advance(at, now_ms()));
        }
        tx.execute(
            "INSERT OR REPLACE INTO _meta (key, value) VALUES (?1, ?2)",
            params![DEBUG_DEVICE, hex(&key.to_bytes())],
        )?;
        let applied = apply_in(&tx, &lines.join("\n"))?;
        tx.commit()?;
        Ok(DebugConflicts {
            device,
            hids: seen.into_iter().map(|(_, e)| e.hid.0).collect(),
            filed: applied.conflicts,
        })
    }

    /// Whether this copy holds a debug device's ops, and so never syncs.
    pub fn debug_copy(&self) -> Result<bool> {
        Ok(device(&self.conn)?.is_some())
    }

    /// Refuse, saying why, when this copy is a debug copy.
    pub fn sync_refuse_debug(&self) -> Result<()> {
        refuse(&self.conn)
    }

    /// Every op held of `hid`, in HLC order: its body, with `applied` —
    /// 1 applied, 0 waiting, 2 refused.
    pub fn debug_ops(&self, hid: &str) -> Result<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT body, applied FROM ops WHERE hid = ?1 ORDER BY hlc, endpoint")?;
        let rows = stmt
            .query_map(params![hid], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(body, applied)| {
                let mut v: Value = serde_json::from_str(&body)?;
                v["applied"] = applied.into();
                Ok(v)
            })
            .collect()
    }
}

/// `base` with its state set to `state`, as an edit leaves it.
fn edited(base: &Entry, state: &str) -> Entry {
    let mut e = base.clone();
    e.state = Some(json!(state));
    e.modify_time = chrono::Utc::now();
    e.content_hash = e.expected_content_hash();
    e
}
