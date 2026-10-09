//! An entry's history, read from its ops (da-s4erq): one version per op in
//! fold order, so every copy holding the same ops shows the same history.
//! Where two ops set a key without seeing each other the history FORKS, and
//! the edit that saw every side JOINS it — [`contention`] over the ops up to
//! each version, the definition the `conflicts` column is written from.
//! `entry_history` is this copy's undo journal, and is not read here.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};

use super::chain::hlc_time;
use super::{about, contention, contestable, device, entry_from, fold, merge_from, merged_into};
use super::{side, values, Chain, Op};
use crate::db::Db;
use crate::entry::Entry;
use crate::error::Result;

/// One version of an entry: one of its ops.
#[derive(Debug, Clone, Serialize)]
pub struct Version {
    pub version: u32,
    /// The row the op was made on: the entry's, or one it supersedes.
    pub hid: String,
    /// `endpoint/seq`.
    pub op: String,
    /// The device that made it, as `conflicts` names one.
    pub device: String,
    pub hlc: String,
    /// The HLC's wall time, RFC 3339.
    pub time: String,
    pub kind: String,
    /// The line that wrote it.
    pub raw: Option<String>,
    pub alias_chain: Option<Value>,
    /// What it changed: `create`, or `set` / `unset` / `deleted`.
    pub effect: Value,
    /// The keys this op left contested: per key each side as `conflicts`
    /// names it, with the side's `version`.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub fork: Map<String, Value>,
    /// The contested keys this op settled: per key the `value` it took and
    /// `took`, the devices whose side that was — none, a value of its own.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub join: Map<String, Value>,
}

impl Version {
    /// What an edit set (`Some`) and unset (`None`), key by key, without the
    /// keys every edit moves or that follow from others. A create's is its
    /// whole row, and none here.
    pub fn changes(&self) -> Vec<(String, Option<Value>)> {
        let set = self.effect["set"].as_object().into_iter().flatten();
        let unset = self.effect["unset"].as_array().into_iter().flatten();
        set.map(|(k, v)| (k.clone(), Some(v.clone())))
            .chain(unset.filter_map(|k| Some((k.as_str()?.to_string(), None))))
            .filter(|(k, _)| contestable(k))
            .collect()
    }
}

fn id(op: &Op) -> String {
    format!("{}/{}", op.endpoint, op.seq)
}

fn versions(conn: &Connection, e: &Entry) -> Result<Vec<Version>> {
    let ops = about(conn, e)?;
    let chain = Chain::load(conn)?;
    // Of two ops of one copy on one row the later saw the earlier
    // ([`Op::saw`]: its own endpoint's lower seq), so `contention` leaves one
    // standing setter per key and finds nothing: it is not run for them.
    let one_line = ops
        .iter()
        .all(|o| o.endpoint == ops[0].endpoint && o.hid == ops[0].hid);
    let version: BTreeMap<String, usize> = ops.iter().map(id).zip(1..).collect();
    let who = |sides: &[(&Op, Value)]| sides.iter().map(|s| id(s.0)).collect::<Vec<_>>();
    let mut before: BTreeMap<String, Vec<(&Op, Value)>> = BTreeMap::new();
    let mut out = Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        let now = match one_line {
            true => BTreeMap::new(),
            false => contention(&ops[..=i]),
        };
        let mut fork = Map::new();
        for (k, sides) in &now {
            if before.get(k).map(|b| who(b)) == Some(who(sides)) {
                continue;
            }
            let sides = sides.iter().map(|s| {
                let mut v = side(&chain, s);
                v["version"] = json!(version[&id(s.0)]);
                v
            });
            fork.insert(k.clone(), sides.collect());
        }
        let set = values(op);
        let mut join = Map::new();
        for (k, sides) in before.iter().filter(|(k, _)| !now.contains_key(*k)) {
            // `deleted` is contention's key for the entry's deletes and
            // undeletes: this op's side of it is whether it set `deleted_at`
            // (a delete) or unset it (an undelete, `Null` in `values`).
            let value = match k.as_str() {
                "deleted" => set.get("deleted_at").map(|d| Value::Bool(!d.is_null())),
                k => set.get(k).cloned(),
            };
            let took: BTreeSet<String> = sides
                .iter()
                .filter(|(_, v)| Some(v) == value.as_ref())
                .map(|(o, _)| device(&chain, o))
                .collect();
            join.insert(k.clone(), json!({ "value": value, "took": took }));
        }
        before = now;
        // A baseline is signed by whatever first needed it; the line that
        // wrote what it holds is the row's own.
        let created = |k: &str| op.effect["create"].get(k).filter(|v| !v.is_null());
        let (raw, alias_chain) = match op.is_baseline() {
            true => (
                created("raw_input").and_then(Value::as_str).map(Into::into),
                created("alias_chain").cloned(),
            ),
            false => (op.raw.clone(), op.alias_chain.clone()),
        };
        out.push(Version {
            version: i as u32 + 1,
            hid: op.hid.clone(),
            op: id(op),
            device: device(&chain, op),
            hlc: op.hlc.clone(),
            time: hlc_time(&op.hlc).to_string(),
            kind: op.kind.clone(),
            raw,
            alias_chain,
            effect: op.effect.clone(),
            fork,
            join,
        });
    }
    Ok(out)
}

/// `e` as its first `version` ops leave it: its own row's fold, with the
/// rows merged into it as far as they had come.
fn entry_at(conn: &Connection, e: &Entry, version: u32) -> Result<Option<Entry>> {
    let ops = about(conn, e)?;
    let Some(seen) = (version as usize)
        .checked_sub(1)
        .and_then(|i| ops.get(..=i))
    else {
        return Ok(None);
    };
    let of = |hid: &str| -> Vec<Op> { seen.iter().filter(|o| o.hid == hid).cloned().collect() };
    let at = |hid: &str| fold(&of(hid), None).map(entry_from).transpose();
    let Some(own) = at(e.hid.as_str())? else {
        return Ok(None);
    };
    let members: Vec<Entry> = merged_into(conn, e)?
        .iter()
        .filter_map(|m| at(m.hid.as_str()).transpose())
        .collect::<Result<_>>()?;
    merge_from(&own, &members, |hid| Ok(of(hid))).map(Some)
}

impl Db {
    /// `e`'s history, oldest first: a version per op of its own and of the
    /// rows it supersedes. An entry older than its ops has none yet. A row
    /// superseded by another has its own ops alone, and no fork: what it
    /// clashes with shows in the entry it went into, as its conflicts do.
    pub fn versions(&self, e: &Entry) -> Result<Vec<Version>> {
        versions(&self.conn, e)
    }

    /// `e` as version `version` of [`Db::versions`] left it; none when it
    /// has no such version, or none that had made its own row yet.
    pub fn entry_at_version(&self, e: &Entry, version: u32) -> Result<Option<Entry>> {
        entry_at(&self.conn, e, version)
    }
}
