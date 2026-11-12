//! `datum db ops export` and `db ops import` (da-tllzg): a database's op
//! table as it is, envelopes and signatures, taken back into another copy,
//! so every author and every edit comes with it.
//!
//! The chain's own records come too, and the import holds them DORMANT:
//! applied, they would make the copy one of that chain without its key being
//! a member, but held, they fill their endpoint's seqs, which its entry ops
//! share, so the ops after them do not wait on them. The copy's vector counts
//! them, so a sync never sends them again: once it holds a record of that
//! chain from anywhere else (it joined it), they wake and apply
//! ([`wake_dormant`]). The seqs a purge order took, which no op fills, are
//! named on one `{"skipped": {endpoint: [[from, to], …]}}` line, and held
//! as the purge's stubs.

use rusqlite::{params, types::Value as Sql, Connection};
use serde_json::{json, Map, Value};

use super::chain::{Chain, CHAIN_HID};
use super::{check_signature, ApplySummary, Op, PURGED, UNCOVERED_SQL};
use crate::db::Db;
use crate::db::WriteBatch;
use crate::error::{DatumError, Result};

impl Db {
    /// Every op this copy holds, one canonical JSON line each by endpoint
    /// and seq, after the `skipped` line where a purge left a seq no op. Signs a baseline first for each entry no op
    /// covers (a bulk import's), as a sync does, so every entry is in it:
    /// where it signs none (paused, or a reader), such an entry refuses the
    /// export rather than be left out of it.
    pub fn ops_export(&self) -> Result<Vec<String>> {
        let paused = self.sync_paused()?.is_some();
        if !paused {
            self.sync_sign_baselines(&Default::default(), None)?;
        }
        self.read(|view| {
            let left: i64 = view.conn().query_row(
                &format!("SELECT COUNT(*) FROM ({UNCOVERED_SQL})"),
                [],
                |r| r.get(0),
            )?;
            if left > 0 {
                let why = match paused {
                    true => "sync is paused on this copy: `datum sync resume` first",
                    false => "a reader signs none: `datum export --jsonl` carries them as rows",
                };
                return Err(DatumError::Other(format!(
                    "db ops export: {left} entries have no op yet, and {why}"
                )));
            }
            let mut stmt = view
                .conn()
                .prepare("SELECT endpoint, seq, body FROM ops ORDER BY endpoint, seq")?;
            let mut rows = stmt.query([])?;
            let (mut lines, mut skipped) = (Vec::new(), Map::new());
            while let Some(r) = rows.next()? {
                let (e, seq, body): (String, i64, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
                // A purge's stub has no body.
                if !body.is_empty() {
                    lines.push(body);
                    continue;
                }
                let runs = skipped.entry(e).or_insert_with(|| json!([]));
                // Every run is pushed here as `[from, to]`.
                let runs = runs.as_array_mut().expect("an array");
                match runs.last_mut() {
                    Some(r) if r[1] == seq - 1 => r[1] = seq.into(),
                    _ => runs.push(json!([seq, seq])),
                }
            }
            if !skipped.is_empty() {
                lines.insert(0, json!({ "skipped": skipped }).to_string());
            }
            Ok(lines)
        })
    }

    /// Apply [`Db::ops_export`]'s lines: a copy in no chain allows every
    /// op, so they apply as they are, the chain's records held dormant. A
    /// copy in a chain refuses the import whole: its chain never admitted
    /// their endpoints.
    pub fn ops_import(&self, lines: &str) -> Result<ApplySummary> {
        let chain = self.read(|view| Chain::load_with_waiting(view.conn()))?;
        if chain.exists() {
            return Err(DatumError::Other(format!(
                "db ops import: this copy is in sync chain {}, which never admitted the endpoints \
                 that signed these ops, so it would refuse them. Bring the entries in as rows \
                 instead: `datum export --jsonl` on the other copy, then `datum import` here",
                chain.id
            )));
        }
        let (mut held, mut ops, mut records) = (WriteBatch::default(), Vec::new(), Vec::new());
        for line in lines.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if line.starts_with("{\"skipped\":") {
                stubs(&mut held, line)?;
                continue;
            }
            match Op::parse(line) {
                Ok(op) if op.is_chain() => records.push(line),
                // `sync_apply` names an unreadable one.
                _ => ops.push(line.to_string()),
            }
        }
        // As a copy holding them dormant serves them: so they are held.
        if !records.is_empty() {
            ops.push(json!({ "dormant": records }).to_string());
        }
        // Before the ops, so each op after a stub finds its predecessor held.
        // On their own, as `sync_apply` commits in chunks: an import cut
        // short is run again, and what is held already is skipped.
        self.write(&held)?;
        self.sync_apply(&ops.join("\n"))
    }
}

/// A chain record held but not applied (da-tllzg): `db ops import` brought
/// it, or a copy holding it so served it, to a copy not in its chain.
pub(super) const DORMANT: i64 = 4;

/// A sender's `{"dormant": [op, …]}` lines: each record of the chain this
/// copy is in goes on as an op (into `each`), and any other is held dormant,
/// its signature checked, filling its seq and changing no chain.
pub(super) fn hold_dormant(
    conn: &Connection,
    chain: &Chain,
    words: &[Value],
    each: &mut Vec<String>,
    s: &mut ApplySummary,
) -> Result<()> {
    let bodies = words.iter().filter_map(|w| w["dormant"].as_array());
    for body in bodies.flatten().filter_map(Value::as_str) {
        let op = match Op::parse(body) {
            Ok(op) if op.is_chain() => op,
            Ok(op) => {
                s.refused
                    .push(format!("{}/{}: no record of a chain", op.endpoint, op.seq));
                continue;
            }
            Err(e) => {
                s.refused.push(format!("unreadable op: {e}"));
                continue;
            }
        };
        if chain.exists() && op.effect["chain"].as_str() == Some(&chain.id) {
            each.push(body.to_string());
            continue;
        }
        if let Err(why) = check_signature(body, &op) {
            s.refused.push(format!("{}/{}: {why}", op.endpoint, op.seq));
            continue;
        }
        conn.prepare_cached(
            "INSERT OR IGNORE INTO ops (endpoint, seq, hlc, hid, slug, kind, applied, body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?
        .execute(params![
            op.endpoint,
            op.seq,
            op.hlc,
            op.hid,
            op.slug,
            op.kind,
            DORMANT,
            body
        ])?;
    }
    Ok(())
}

/// The stubs a `skipped` line names, as a purge's are held.
fn stubs(held: &mut WriteBatch, line: &str) -> Result<()> {
    let word: Value = serde_json::from_str(line)
        .map_err(|e| DatumError::Other(format!("unreadable skipped line {line}: {e}")))?;
    for (e, runs) in word["skipped"].as_object().into_iter().flatten() {
        for run in runs.as_array().into_iter().flatten() {
            let (Some(from), Some(to)) = (run[0].as_i64(), run[1].as_i64()) else {
                return Err(DatumError::Other(format!(
                    "a skipped run is [from, to], two seqs: {run}"
                )));
            };
            for seq in from..=to {
                held.push(
                    "INSERT OR IGNORE INTO ops (endpoint, seq, hlc, hid, slug, kind, applied, body)
                     VALUES (?1, ?2, '', ?3, '', 'purged', 3, '')",
                    vec![
                        Sql::Text(e.clone()),
                        Sql::Integer(seq),
                        Sql::Text(PURGED.into()),
                    ],
                );
            }
        }
    }
    Ok(())
}

/// The dormant records of the chains `ids` names wait to apply: this copy
/// is receiving that chain's records, so it is joining it.
pub(super) fn wake_dormant<'a>(
    conn: &Connection,
    ids: impl IntoIterator<Item = &'a str>,
) -> Result<()> {
    let mut wake = conn.prepare_cached(
        "UPDATE ops SET applied = 0
         WHERE applied = ?1 AND hid = ?2 AND json_extract(body, '$.effect.chain') = ?3",
    )?;
    for id in ids {
        wake.execute(params![DORMANT, CHAIN_HID, id])?;
    }
    Ok(())
}
