//! Sync's diagnostics (da-n0ruf): what one copy can tell of a sync problem
//! from its own ops and rows ([`Db::sync_fold_check`]), the repair that
//! needs no peer — a row written again from its ops ([`Db::sync_refold`]) —
//! and what a copy holds of an entry, for two copies to compare
//! ([`Db::sync_digest`], [`Db::sync_held`]). Nothing here signs or sends an
//! op.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use super::chain::{Chain, CHAIN_HID};
use super::{
    canonical_json, conflicts_of, derived, device, endpoint, fold, has_ops, is_reader, ops_of,
    rederive, refresh_conflicts, row, sha256_hex, shared_rows, Op,
};
use crate::db::insert::as_stored;
use crate::db::row_mapper::row_to_entry;
use crate::db::Db;
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::history::ChangeKind;

/// An entry whose applied ops fold to a row this copy does not hold: the
/// row was dropped and its ops kept, so every copy this one syncs with
/// makes the entry again (da-95y4o).
#[derive(Debug, Clone)]
pub struct Orphan {
    pub hid: String,
    pub slug: String,
    pub field: String,
    /// Its ops, as [`Db::sync_held`] names them.
    pub ops: Vec<Value>,
}

/// A row that is not what its ops fold to: per key that differs
/// ([`row_diff`]), what the row holds and what the ops give.
#[derive(Debug, Clone)]
pub struct Stale {
    pub hid: String,
    pub slug: String,
    pub field: String,
    pub columns: BTreeMap<String, (Value, Value)>,
}

/// An entry a finding names.
#[derive(Debug, Clone)]
pub struct Named {
    pub hid: String,
    pub slug: String,
    pub field: String,
}

/// What a copy's ops and rows say of each other.
#[derive(Debug, Default)]
pub struct FoldCheck {
    /// Entry rows examined, tombstones and superseded rows among them.
    pub entries: usize,
    /// Applied entry ops examined.
    pub ops: usize,
    pub orphans: Vec<Orphan>,
    pub stale: Vec<Stale>,
    /// Rows whose ops fold to nothing this build can store, each with why:
    /// neither checked against the fold nor refolded.
    pub unfoldable: Vec<(Named, String)>,
    /// Rows no op covers, on a copy in a chain. No other copy holds them
    /// until one is signed.
    pub uncovered: Vec<Named>,
    /// This copy is a reader of its chain: it signs nothing.
    pub reader: bool,
}

/// The keys two rows give different values, named as an op names them: a
/// column, or `data.<key>` where both hold an object.
pub fn row_diff(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
) -> BTreeMap<String, (Value, Value)> {
    let at = |m: &Map<String, Value>, k: &str| m.get(k).cloned().unwrap_or(Value::Null);
    let mut out = BTreeMap::new();
    for k in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
        match (at(a, k), at(b, k)) {
            (x, y) if x == y => {}
            (Value::Object(x), Value::Object(y)) if k == "data" => {
                for dk in x.keys().chain(y.keys()).collect::<BTreeSet<_>>() {
                    if x.get(dk) != y.get(dk) {
                        out.insert(format!("data.{dk}"), (at(&x, dk), at(&y, dk)));
                    }
                }
            }
            (x, y) => {
                out.insert(k.clone(), (x, y));
            }
        }
    }
    out
}

/// Every op held for `hid`, in fold order: its id, device, kind, HLC and
/// line, and whether it is applied, waiting or refused.
fn op_lines(conn: &Connection, chain: &Chain, hid: &str) -> Result<Vec<Value>> {
    let mut ops = conn
        .prepare_cached("SELECT body, applied FROM ops WHERE hid = ?1")?
        .query_map(params![hid], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?))
        })?
        .map(|r| {
            let (body, applied) = r?;
            Ok((Op::parse(&body)?, applied))
        })
        .collect::<Result<Vec<_>>>()?;
    ops.sort_by(|a, b| a.0.order().cmp(&b.0.order()));
    Ok(ops
        .iter()
        .map(|(op, applied)| {
            let held = ["waiting", "applied", "refused"].get(*applied);
            json!({
                "op": format!("{}/{}", op.endpoint, op.seq),
                "device": device(chain, op),
                "kind": op.kind,
                "hlc": op.hlc,
                "raw": op.raw,
                "held": held,
            })
        })
        .collect())
}

fn fold_check(conn: &Connection) -> Result<FoldCheck> {
    let chain = Chain::load(conn)?;
    let mut out = FoldCheck {
        ops: conn.query_row(
            "SELECT COUNT(*) FROM ops WHERE applied = 1 AND hid != ?1",
            params![CHAIN_HID],
            |r| r.get(0),
        )?,
        // In no chain there is no reader, and a read mints no endpoint key.
        reader: chain.exists() && is_reader(conn, &endpoint(conn)?)?,
        ..FoldCheck::default()
    };
    let rows: Vec<Entry> = conn
        .prepare("SELECT * FROM entries ORDER BY slug, hid")?
        .query_map([], row_to_entry)?
        .collect::<rusqlite::Result<_>>()?;
    out.entries = rows.len();
    let with = |e: &Entry, conflicts: Option<Value>| {
        let mut r = row(e);
        r.insert("conflicts".into(), conflicts.unwrap_or(Value::Null));
        r
    };
    for e in &rows {
        let named = || Named {
            hid: e.hid.0.clone(),
            slug: e.slug.0.clone(),
            field: e.field.clone(),
        };
        if !has_ops(conn, &e.hid)? {
            if chain.exists() {
                out.uncovered.push(named());
            }
            continue;
        }
        // As the fold is written: storage derives the verb and the content
        // hash from the rest of the row, and a merged row's are not its ops'.
        // A fold that is no row, or one storage refuses, is a finding of its
        // own: not a stale row, and not the end of the check for the rest.
        let folded = match derived(conn, e).and_then(|f| as_stored(&f)) {
            Ok(folded) => with(&folded, conflicts_of(conn, e)?),
            Err(DatumError::Db(e)) => return Err(e.into()),
            Err(why) => {
                out.unfoldable.push((named(), why.to_string()));
                continue;
            }
        };
        let columns = row_diff(&with(e, e.conflicts.clone()), &folded);
        if !columns.is_empty() {
            out.stale.push(Stale {
                hid: e.hid.0.clone(),
                slug: e.slug.0.clone(),
                field: e.field.clone(),
                columns,
            });
        }
    }
    // Applied ops alone: a refused op's entry is gone on purpose (a purged
    // device's), and a waiting op's is not here yet.
    let rowless: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT hid FROM ops WHERE applied = 1 AND hid != ?1
               AND hid NOT IN (SELECT hid FROM entries) ORDER BY hid",
        )?
        .query_map(params![CHAIN_HID], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for hid in rowless {
        // Edits with no create fold to nothing, and have no row to miss.
        if let Some(r) = fold(&ops_of(conn, &hid)?, None) {
            out.orphans.push(Orphan {
                slug: r["slug"].as_str().unwrap_or_default().into(),
                field: r["field"].as_str().unwrap_or_default().into(),
                ops: op_lines(conn, &chain, &hid)?,
                hid,
            });
        }
    }
    Ok(out)
}

impl Db {
    /// Read this copy's rows against its ops: the entries whose ops fold to
    /// a row it lacks, the rows that are not their ops' fold, and in a chain
    /// the rows no op covers. A copy that applied its ops whole has none of
    /// the first two.
    pub fn sync_fold_check(&self) -> Result<FoldCheck> {
        fold_check(&self.conn)
    }

    /// Write every row that is not its ops' fold again from its ops, and its
    /// `conflicts` with it: what a correct copy holds is unchanged, and no
    /// op is signed, the ops being what every copy already agrees on. The
    /// row as it stood goes to `entry_history`, as a sync's merge does.
    /// Orphans are left as found. What was found, and what still differs.
    pub fn sync_refold(&self) -> Result<(FoldCheck, Vec<Stale>)> {
        let tx = self.conn.unchecked_transaction()?;
        let found = fold_check(&tx)?;
        for s in &found.stale {
            rederive(&tx, &s.hid, ChangeKind::SyncMerge)
                .and_then(|()| refresh_conflicts(&tx, &s.hid))
                .map_err(|e| DatumError::Other(format!("refolding {} ({}): {e}", s.slug, s.hid)))?;
        }
        let left = fold_check(&tx)?.stale;
        tx.commit()?;
        Ok((found, left))
    }

    /// The state hash ([`Db::sync_state_hash`]) and, under it, each row's
    /// own by hid: two copies whose state hashes differ find the entries
    /// that differ by comparing these.
    pub fn sync_digest(&self, peer: Option<&str>) -> Result<(String, BTreeMap<String, String>)> {
        let (rows, _) = shared_rows(&self.conn, None, peer)?;
        let digest = rows
            .iter()
            .map(|r| {
                let hid = r["hid"].as_str().unwrap_or_default().to_string();
                (hid, sha256_hex(&canonical_json(&Value::Object(r.clone()))))
            })
            .collect();
        let all = Value::Array(rows.into_iter().map(Value::Object).collect());
        Ok((sha256_hex(&canonical_json(&all)), digest))
    }

    /// What this copy holds of each of `hids`, for a session with `peer` to
    /// compare: `{hid, row, ops}` — the row as the state hash reads it, null
    /// when this copy has none, and every op held for it ([`op_lines`]). A
    /// filtered reader, on either side, is told nothing of an entry outside
    /// its filter.
    pub fn sync_held(&self, peer: Option<&str>, hids: &[String]) -> Result<Vec<Value>> {
        let (rows, filtered) = shared_rows(&self.conn, None, peer)?;
        let chain = Chain::load(&self.conn)?;
        let rows: BTreeMap<&str, &Map<String, Value>> = rows
            .iter()
            .filter_map(|r| Some((r.get("hid")?.as_str()?, r)))
            .collect();
        hids.iter()
            .map(|hid| {
                let row = rows.get(hid.as_str());
                let ops = match row.is_none() && filtered {
                    true => Vec::new(),
                    false => op_lines(&self.conn, &chain, hid)?,
                };
                Ok(json!({ "hid": hid, "row": row, "ops": ops }))
            })
            .collect()
    }
}
