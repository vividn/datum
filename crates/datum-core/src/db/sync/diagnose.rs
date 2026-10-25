//! Sync's diagnostics (da-n0ruf): what one copy can tell of a sync problem
//! from its own ops and rows ([`Db::sync_fold_check`]), the repair that
//! needs no peer — a row written again from its ops ([`Db::sync_refold`]) —
//! the repair of an entry whose ops have no row ([`Db::sync_fix_orphans`]),
//! and what a copy holds of an entry, for two copies to compare
//! ([`Db::sync_digest`], [`Db::sync_held`]). Nothing here sends an op, and
//! only that repair signs one: an ordinary `delete`.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::chain::{Chain, CHAIN_HID};
use super::{
    add, blockers, canonical_json, conflicts_of, derived, device, endpoint, entry_from,
    fetch_by_hid, fold, has_ops, hex, met, ops_of, read_only, rederive, refold, refresh_conflicts,
    row, shared_rows, Op, FOLD_RULE,
};
use crate::db::insert::as_stored;
use crate::db::row_mapper::row_to_entry;
use crate::db::tombstone::soft_delete_in_tx;
use crate::db::{busy, Db, WriteBatch};
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::history::ChangeKind;

/// How many rows a refold writes a transaction, so the write lock is
/// another's between them (da-8i7f0).
const REFOLD_CHUNK: usize = 256;

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
    /// The slug of a live entry holding an identity its row needs: a row
    /// written again from its ops merges with it, as on every copy holding
    /// both, so [`Db::sync_fix_orphans`] writes it and deletes nothing.
    pub shares: Option<String>,
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
    /// This copy signs nothing in its chain: a reader, or a member that
    /// signed its departure (da-dsob1).
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

/// The slug of a live entry holding an identity the row `r` needs, as the
/// fold would find it ([`blockers`], [`met`]).
fn shares(conn: &Connection, r: Map<String, Value>) -> Result<Option<String>> {
    let Ok(e) = entry_from(r) else {
        return Ok(None);
    };
    let mut found = blockers(conn, &e)?;
    found.extend(met(conn, &e)?);
    Ok(found.into_iter().next().map(|b| b.slug.0))
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
        reader: chain.exists() && read_only(conn, &endpoint(conn)?)?.is_some(),
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
                shares: shares(conn, r)?,
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
    /// Orphans are left as found: [`Db::sync_fix_orphans`] repairs them.
    /// What was found, and what still differs.
    /// The whole database is read before the write lock and after it, and
    /// the lock is held for the stale rows alone (da-8i7f0).
    pub fn sync_refold(&self) -> Result<(FoldCheck, Vec<Stale>)> {
        let found = self.read(|view| fold_check(view.conn()))?;
        self.refold_rows(&found.stale)?;
        let left = self.read(|view| fold_check(view.conn()))?.stale;
        Ok((found, left))
    }

    /// Write `stale` again from its ops, [`REFOLD_CHUNK`] rows a
    /// transaction. Each is read again under the lock: one that moved since
    /// it was found is written as its ops fold now, and one right by then
    /// is left as it is.
    fn refold_rows(&self, stale: &[Stale]) -> Result<()> {
        self.in_chunks(stale, |tx, s| {
            rederive(tx, &s.hid, ChangeKind::SyncMerge)
                .and_then(|()| refresh_conflicts(tx, &s.hid).map(drop))
                .map_err(|e| DatumError::Other(format!("refolding {} ({}): {e}", s.slug, s.hid)))
        })
    }

    /// Repair every entry whose ops this copy holds and has no row for
    /// (da-53whj): its row is written again from its ops, as a sync would.
    /// One a live entry [`Orphan::shares`] an identity with is then merged
    /// with it, as every copy holding both holds them, and no op is signed
    /// (da-t5kxx). Any other is deleted as `datum delete` deletes, so the
    /// `delete` op signed syncs and every copy, one that applied those ops
    /// included, folds the entry to the same tombstone; one whose ops fold
    /// to a tombstone already needs only its row. How many it found.
    pub fn sync_fix_orphans(&self) -> Result<usize> {
        let found = self.read(|view| fold_check(view.conn()))?.orphans;
        self.in_chunks(&found, |tx, o| {
            // A row that came back since it was found is no orphan; whether
            // a live entry holds its identity is read again under the lock.
            let shared = match fold(&ops_of(tx, &o.hid)?, None) {
                Some(r) => shares(tx, r)?.is_some(),
                None => return Ok(()),
            };
            if fetch_by_hid(tx, &o.hid)?.is_some() {
                return Ok(());
            }
            refold(tx, &o.hid, ChangeKind::SyncMerge)?;
            if !shared && fetch_by_hid(tx, &o.hid)?.is_some_and(|e| e.deleted_at.is_none()) {
                soft_delete_in_tx(tx, &o.hid, ChangeKind::UserEdit, None)?;
            }
            Ok(())
        })?;
        Ok(found.len())
    }

    /// `each` of `items` under the write lock, [`REFOLD_CHUNK`] a
    /// transaction, so the lock is another's between them (da-8i7f0).
    fn in_chunks<T>(
        &self,
        items: &[T],
        each: impl Fn(&Connection, &T) -> Result<()>,
    ) -> Result<()> {
        for chunk in items.chunks(REFOLD_CHUNK) {
            self.write_with(|tx| chunk.iter().try_for_each(|i| each(tx, i)))?;
        }
        Ok(())
    }

    /// On open (da-sqpjt): a copy whose rows were folded under an older
    /// [`FOLD_RULE`], or before there was one — no stamp, or one that is no
    /// number — writes again every row that is not what its ops fold to now,
    /// as `sync refold` does, and stamps the rule in `_meta` `fold_rule`; a
    /// copy on it reads that one row. A stamp newer than this build's is a
    /// newer build's rule, which this one does not fold back to its own; but
    /// a row this connection writes is folded by its rule, so each write
    /// lowers such a stamp to it (da-w13ei, a trigger of this connection's
    /// alone: a daemon or REPL opened before the newer build stamped is
    /// covered too), and the newer build's next open refolds.
    /// With the lock another's, nothing is checked or stamped, and the next
    /// open tries again. A refold that fails otherwise is stamped all the same, and why
    /// is [`Db::refold_failed`], for the CLI to say: what it left is
    /// `datum check sync`'s to name, which a database no command could open
    /// would not let it do.
    pub(crate) fn refold_for_rule(&self) -> Result<()> {
        for on in ["INSERT", "UPDATE", "DELETE"] {
            self.conn.execute_batch(&format!(
                "CREATE TEMP TRIGGER IF NOT EXISTS fold_rule_on_{on} AFTER {on} ON main.entries
                 BEGIN
                   UPDATE _meta SET value = '{FOLD_RULE}'
                   WHERE key = 'fold_rule' AND CAST(value AS INTEGER) > {FOLD_RULE};
                 END;"
            ))?;
        }
        let stamped: Option<String> = self
            .conn
            .query_row("SELECT value FROM _meta WHERE key = 'fold_rule'", [], |r| {
                r.get(0)
            })
            .optional()?;
        if stamped.and_then(|v| v.parse::<u32>().ok()) >= Some(FOLD_RULE) {
            return Ok(());
        }
        // The check reads every row (~3 s for 20k on a laptop): not paid
        // by an open that could not write what it finds.
        if !self.can_write(std::time::Duration::from_millis(100))? {
            return Ok(());
        }
        let refolded = self
            .read(|view| fold_check(view.conn()))
            .and_then(|found| self.refold_rows(&found.stale));
        match refolded {
            Err(e) if busy(&e) => return Ok(()),
            Err(e) => *self.refold_failed.borrow_mut() = Some(e.to_string()),
            Ok(()) => {}
        }
        let mut stamp = WriteBatch::default();
        stamp.push(
            "INSERT INTO _meta (key, value) VALUES ('fold_rule', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            vec![FOLD_RULE.into()],
        );
        match self.write(&stamp) {
            Err(e) if busy(&e) => Ok(()),
            r => r.map(drop),
        }
    }

    /// The state hash ([`Db::sync_state_hash`]) and, under it, each row's
    /// own by hid: two copies whose state hashes differ find the entries
    /// that differ by comparing these.
    pub fn sync_digest(&self, peer: Option<&str>) -> Result<(String, BTreeMap<String, String>)> {
        let (rows, _) = shared_rows(&self.conn, None, peer)?;
        let mut sum = [0; 32];
        let digest = rows
            .into_iter()
            .map(|r| {
                let hid = r["hid"].as_str().unwrap_or_default().to_string();
                let h: [u8; 32] = Sha256::digest(canonical_json(&Value::Object(r))).into();
                add(&mut sum, &h, 1);
                (hid, hex(&h))
            })
            .collect();
        Ok((hex(&sum), digest))
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
