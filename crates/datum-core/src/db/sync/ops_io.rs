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

use std::collections::{BTreeMap, HashMap};

use rusqlite::{params, types::Value as Sql, Connection};
use serde_json::{json, Map, Value};

use super::chain::{Chain, CHAIN_HID};
use super::{check_signature, ApplySummary, Op, PURGED, SHARED_KEY, UNCOVERED_SQL};
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
        let (mut ops, mut records, mut skipped, mut parsed) = (Vec::new(), vec![], vec![], vec![]);
        let lines = lines.lines().map(str::trim).enumerate();
        for (n, line) in lines.filter(|(_, l)| !l.is_empty()) {
            if line.starts_with("{\"skipped\":") {
                skipped.push((n + 1, line));
                continue;
            }
            match Op::parse(line) {
                Ok(op) => {
                    match op.is_chain() {
                        true => records.push(line),
                        false => ops.push(line.to_string()),
                    }
                    parsed.push((op, line));
                }
                // `sync_apply` names an unreadable one.
                Err(_) => ops.push(line.to_string()),
            }
        }
        // As a copy holding them dormant serves them: so they are held.
        if !records.is_empty() {
            ops.push(json!({ "dormant": records }).to_string());
        }
        let (held, mut stubbed) = stubs(&skipped, &parsed)?;
        // Before the ops, so each op after a stub finds its predecessor held.
        // On their own, as `sync_apply` commits in chunks: an import cut
        // short is run again, and what is held already is skipped.
        self.write(&held)?;
        let mut s = self.sync_apply(&ops.join("\n"))?;
        stubbed.refused.append(&mut s.refused);
        s.refused = stubbed.refused;
        s.stubs = stubbed.stubs;
        s.past_head = stubbed.past_head;
        Ok(s)
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

/// The most seqs a file's `skipped` lines may name in all (da-9jdsf). A
/// purge's stubs are the ops it took, at most every op a copy held: the
/// largest copy measured, vividn's phone, held 287k entries (da-dwm2y), a
/// few ops each, and this is several times that. Holding it all takes
/// seconds, where a run to 2^63 never ended.
const MOST_SKIPPED: i64 = 1 << 22;

/// The stubs a file's `skipped` lines name, as a purge's are held: one
/// statement a run, the runs bounded. The lines are unsigned (da-9jdsf), so
/// a seq a signed op in the file fills is refused, never stubbed, and that
/// op applies; and an endpoint with signed ops in the file is stubbed only
/// below the highest of them. A stub above it holds back no op of the file,
/// so leaving it out costs the import nothing, where a forged one would make
/// that endpoint's next op read as a second at its seq. An endpoint with no
/// signed op in the file is held as the line says, until a stub can be
/// checked (da-lnmyf).
fn stubs(lines: &[(usize, &str)], ops: &[(Op, &str)]) -> Result<(WriteBatch, ApplySummary)> {
    let (mut runs, mut total) = (Vec::new(), 0i64);
    for &(n, line) in lines {
        let bad = |why: String| {
            DatumError::Other(format!("db ops import: line {n}, the skipped line: {why}"))
        };
        let word: Value = serde_json::from_str(line).map_err(|e| bad(e.to_string()))?;
        for (e, rs) in word["skipped"].as_object().into_iter().flatten() {
            for run in rs.as_array().into_iter().flatten() {
                let (Some(from), Some(to)) = (run[0].as_i64(), run[1].as_i64()) else {
                    return Err(bad(format!("a run is [from, to], two seqs: {run}")));
                };
                if from < 1 || to < from {
                    return Err(bad(format!("{e}: [{from}, {to}] is no run of seqs")));
                }
                total = total.saturating_add(to - from + 1);
                if total > MOST_SKIPPED {
                    return Err(bad(format!(
                        "its runs name more than {MOST_SKIPPED} seqs, more than a purge leaves"
                    )));
                }
                runs.push((e.clone(), from, to));
            }
        }
    }
    let mut of: HashMap<&str, BTreeMap<i64, &(Op, &str)>> = HashMap::new();
    for op in ops {
        of.entry(&op.0.endpoint).or_default().insert(op.0.seq, op);
    }
    let signed = |(op, body): &(Op, &str)| check_signature(body, op).is_ok();
    let (mut held, mut s) = (WriteBatch::default(), ApplySummary::default());
    for (e, from, to) in runs {
        let mine = of.get(e.as_str());
        let head = mine
            .and_then(|m| m.values().rev().find(|op| signed(op)))
            .map(|op| op.0.seq);
        let filled: Vec<i64> = (mine.into_iter().flat_map(|m| m.range(from..=to)))
            .filter(|(_, op)| signed(op))
            .map(|(&n, _)| n)
            .collect();
        for n in &filled {
            s.refused.push(format!(
                "{e}/{n}: the skipped line names a seq an op in the file fills, so no stub"
            ));
        }
        if let Some(h) = head.filter(|&h| h < to) {
            s.past_head.push(format!(
                "{e}/{}..{to}: no stub past its last op in the file, {h}",
                from.max(h + 1)
            ));
        }
        let top = head.map_or(to, |h| to.min(h - 1));
        let mut next = from;
        for end in filled
            .iter()
            .map(|n| n - 1)
            .filter(|&end| end < top)
            .chain([top])
        {
            if next <= end {
                held.push(
                    "WITH RECURSIVE s(n) AS (SELECT ?2 UNION ALL SELECT n + 1 FROM s WHERE n < ?3)
                     INSERT OR IGNORE INTO ops (endpoint, seq, hlc, hid, slug, kind, applied, body)
                     SELECT ?1, n, '', ?4, '', 'purged', 3, '' FROM s",
                    vec![
                        Sql::Text(e.clone()),
                        Sql::Integer(next),
                        Sql::Integer(end),
                        Sql::Text(PURGED.into()),
                    ],
                );
                s.stubs += end - next + 1;
            }
            next = end.saturating_add(2);
        }
    }
    Ok((held, s))
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

/// Why a received op at a seq this copy holds another op at, or of its own
/// endpoint past what it signed, is refused. Of this copy's own endpoint, a
/// copy shares its key; of another's, that endpoint signed two different
/// ops at the seq, and this copy shares no key (da-hdd9w).
pub(super) fn second_at_seq(chain: &Chain, op: &Op, mine: &str) -> String {
    let (e, n) = (&op.endpoint, op.seq);
    if e == mine {
        return format!("{e}/{n}: {SHARED_KEY}");
    }
    let label = chain.member(e).map(|m| m.label.as_str());
    let who = label.filter(|l| !l.is_empty()).unwrap_or(e);
    format!("{e}/{n}: {who} signed two different ops at seq {n}; this copy keeps the one it had")
}
