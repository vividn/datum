//! Easy Sync, the core (da-k329q): every change to an entry is a signed op,
//! and two copies that hold the same ops hold the same entries.
//!
//! - **Endpoint.** Each copy of a database has an Ed25519 key, minted on
//!   first need into `sync_local` (never synced). The hex public key is the
//!   endpoint id every op of this copy carries.
//! - **Op.** Every mutation of an entry emits one op in its own transaction
//!   ([`emit`], called beside each `history::record`): `endpoint`, a gapless
//!   per-endpoint `seq`, an `hlc`, the entry's `hid` and `slug`, the change
//!   `kind`, the command line that made it (`raw`, `alias_chain`), what it
//!   changed (`effect`), what this copy had seen of the entry (`deps`) and
//!   `sig`, the endpoint's signature over the SHA-256 of the op's
//!   [`canonical_json`] without `sig`. An entry's first op is a create — a
//!   `baseline` for an entry older than its ops.
//! - **HLC.** `<wall ms, RFC 3339>/<counter>`, fixed width so it orders as a
//!   string. It never runs behind an op this copy holds, so an op made after
//!   seeing another sorts after it, whatever the device clocks say.
//! - **Merge.** An entry's row is the FOLD of its applied ops in
//!   `(hlc, endpoint)` order, baselines first ([`fold`]): a create sets the
//!   whole row and an edit sets and unsets keys. Per key the latest HLC wins, ops touching
//!   different keys both land, and because the fold reads only the SET of ops,
//!   the same ops in any arrival order, applied any number of times, give the
//!   same row. An op is applied from its `effect`, never by re-running `raw`.
//! - **Conflict.** Two ops are concurrent when neither's `deps` covers the
//!   other. Concurrent ops that give one key different values, or a delete
//!   and a concurrent edit, file a `sync_conflict` for the loser.
//!
//! Seq, HLC and the version vector are all read off the `ops` table rather
//! than kept as counters beside it, so there is no second copy of them to
//! drift.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::history::{self, ChangeKind};
use crate::ids::Hid;

use super::insert::{apply_entry_state, try_insert};
use super::row_mapper::row_to_entry;
use super::Db;

// ─── The invocation ─────────────────────────────────────────────────────────

#[derive(Default)]
struct Invocation {
    raw: Option<String>,
    alias_chain: Option<Value>,
    device_source: Option<fn() -> Option<String>>,
    device: Option<String>,
}

thread_local! {
    static INVOCATION: RefCell<Invocation> = RefCell::new(Invocation::default());
}

/// The command line the ops written from here on were made by — THIS
/// change's own line, which the op carries as `raw` (the entry's
/// `raw_input` stays the line that first logged it).
pub fn set_invocation(raw: Option<String>, alias_chain: Option<Value>) {
    INVOCATION.with(|i| {
        let mut i = i.borrow_mut();
        i.raw = raw;
        i.alias_chain = alias_chain;
    });
}

/// How to learn this device's id, asked the first time an op is signed.
/// Lazy, so a command that writes nothing reads nothing for it.
pub fn set_device_source(source: fn() -> Option<String>) {
    INVOCATION.with(|i| i.borrow_mut().device_source = Some(source));
}

fn device_id() -> String {
    INVOCATION.with(|i| {
        let mut i = i.borrow_mut();
        if i.device.is_none() {
            i.device = Some(i.device_source.and_then(|f| f()).unwrap_or_default());
        }
        i.device.clone().unwrap_or_default()
    })
}

// ─── Canonical JSON, hex ────────────────────────────────────────────────────

/// Object keys sorted at every depth, no whitespace: the one serialization
/// that is signed and hashed, so every copy computes the same bytes.
pub fn canonical_json(v: &Value) -> String {
    fn sorted(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut pairs: Vec<(&String, &Value)> = m.iter().collect();
                pairs.sort_by(|a, b| a.0.cmp(b.0));
                Value::Object(
                    pairs
                        .into_iter()
                        .map(|(k, v)| (k.clone(), sorted(v)))
                        .collect(),
                )
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            v => v.clone(),
        }
    }
    sorted(v).to_string()
}

fn sha256_hex(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != 2 * N {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

// ─── Endpoint ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Endpoint {
    key: SigningKey,
    id: String,
}

thread_local! {
    /// The last endpoint read, by its secret: deriving the public key is
    /// the dearest step of signing, and a batch signs thousands of ops.
    static ENDPOINT: RefCell<Option<(String, Endpoint)>> = const { RefCell::new(None) };
}

/// This copy's endpoint, minted on first need. The key is stored with the
/// device it was minted on, and a file that opens on another device (copied,
/// restored) mints its own rather than sign as the copy it came from. Two
/// copies of one file on ONE device share the device id, and so the key —
/// a known limit: their ops would collide on `(endpoint, seq)`.
fn endpoint(conn: &Connection) -> Result<Endpoint> {
    let device = device_id();
    let held: Option<(String, String)> = conn
        .prepare_cached("SELECT endpoint_secret, device_id FROM sync_local WHERE id = 1")?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    if let Some((secret, _)) = held.filter(|(_, d)| *d == device) {
        let cached = ENDPOINT.with(|c| {
            c.borrow()
                .as_ref()
                .filter(|(s, _)| *s == secret)
                .map(|(_, ep)| ep.clone())
        });
        if let Some(ep) = cached {
            return Ok(ep);
        }
        if let Some(bytes) = unhex::<32>(&secret) {
            let key = SigningKey::from_bytes(&bytes);
            let ep = Endpoint {
                id: hex(key.verifying_key().as_bytes()),
                key,
            };
            ENDPOINT.with(|c| *c.borrow_mut() = Some((secret, ep.clone())));
            return Ok(ep);
        }
    }
    let key = SigningKey::generate(&mut rand::rngs::OsRng);
    let id = hex(key.verifying_key().as_bytes());
    conn.execute(
        "INSERT OR REPLACE INTO sync_local (id, endpoint_secret, endpoint_id, device_id)
         VALUES (1, ?1, ?2, ?3)",
        params![hex(&key.to_bytes()), id, device],
    )?;
    Ok(Endpoint { key, id })
}

// ─── HLC ────────────────────────────────────────────────────────────────────

const HLC_COUNTER_MAX: u32 = 9999;

fn format_hlc(ms: i64, counter: u32) -> String {
    let wall = Utc.timestamp_millis_opt(ms).single().unwrap_or_default();
    format!("{}/{counter:04}", wall.format("%Y-%m-%dT%H:%M:%S%.3fZ"))
}

fn parse_hlc(s: &str) -> Option<(i64, u32)> {
    let (wall, counter) = s.split_once('/')?;
    let ms = DateTime::parse_from_rfc3339(wall).ok()?.timestamp_millis();
    let counter = counter.parse().ok()?;
    // Only the canonical spelling orders correctly as a string.
    (format_hlc(ms, counter) == s).then_some((ms, counter))
}

/// The next HLC: past the wall clock and past every op this copy holds,
/// its own and received alike.
fn next_hlc(conn: &Connection) -> Result<String> {
    let last: Option<String> = conn
        .prepare_cached("SELECT MAX(hlc) FROM ops")?
        .query_row([], |r| r.get(0))?;
    let now = Utc::now().timestamp_millis();
    let (ms, counter) = match last.as_deref().and_then(parse_hlc) {
        Some((ms, _)) if now > ms => (now, 0),
        Some((ms, c)) if c < HLC_COUNTER_MAX => (ms, c + 1),
        Some((ms, _)) => (ms + 1, 0),
        None => (now, 0),
    };
    Ok(format_hlc(ms, counter))
}

// ─── Rows and effects ───────────────────────────────────────────────────────

/// An entry as the ops see it: its serialized columns. `superseded_by`
/// is not among them — every copy derives it from the rows it holds.
fn row(e: &Entry) -> Map<String, Value> {
    match serde_json::to_value(e) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

fn present(m: &Map<String, Value>, k: &str) -> Option<Value> {
    m.get(k).filter(|v| !v.is_null()).cloned()
}

/// What changed from `pre` to `post`, at key granularity: top-level columns
/// by name and, when `data` is an object on both sides, its keys one level
/// deep as `data.<key>`.
fn diff(pre: &Entry, post: &Entry) -> Value {
    let (a, b) = (row(pre), row(post));
    let mut set = Map::new();
    let mut unset = Vec::new();
    let mut put = |k: String, v: Option<Value>| match v {
        Some(v) => {
            set.insert(k, v);
        }
        None => unset.push(Value::String(k)),
    };
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for k in keys.into_iter().filter(|k| *k != "hid") {
        let (x, y) = (present(&a, k), present(&b, k));
        if x == y {
            continue;
        }
        match (k.as_str(), &x, &y) {
            ("data", Some(Value::Object(x)), Some(Value::Object(y))) => {
                for dk in x.keys().chain(y.keys()).collect::<BTreeSet<_>>() {
                    if x.get(dk) != y.get(dk) {
                        put(format!("data.{dk}"), y.get(dk).cloned());
                    }
                }
            }
            _ => put(k.clone(), y),
        }
    }
    let mut effect = Map::new();
    if !set.is_empty() {
        effect.insert("set".into(), Value::Object(set));
    }
    if !unset.is_empty() {
        effect.insert("unset".into(), Value::Array(unset));
    }
    if pre.deleted_at.is_some() != post.deleted_at.is_some() {
        effect.insert("deleted".into(), post.deleted_at.is_some().into());
    }
    Value::Object(effect)
}

/// Fold one effect into a row. A create replaces the row; an edit on no
/// row has nothing to edit.
fn fold_into(row: &mut Option<Map<String, Value>>, effect: &Value) {
    if let Some(Value::Object(c)) = effect.get("create") {
        *row = Some(c.clone());
        return;
    }
    let Some(r) = row.as_mut() else { return };
    if let Some(Value::Object(set)) = effect.get("set") {
        for (k, v) in set {
            match k.strip_prefix("data.") {
                Some(dk) => {
                    let data = r.entry("data").or_insert_with(|| json!({}));
                    if !data.is_object() {
                        *data = json!({});
                    }
                    if let Some(d) = data.as_object_mut() {
                        d.insert(dk.to_string(), v.clone());
                    }
                }
                None => {
                    r.insert(k.clone(), v.clone());
                }
            }
        }
    }
    for k in effect["unset"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        match k.strip_prefix("data.") {
            Some(dk) => {
                if let Some(Value::Object(d)) = r.get_mut("data") {
                    d.remove(dk);
                }
            }
            None => {
                r.insert(k.to_string(), Value::Null);
            }
        }
    }
}

/// The keys an op gives a value to (`Null` for an unset), with a create's
/// row flattened the way [`diff`] names keys, so two ops compare key by key.
fn values(op: &Op) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    if let Some(Value::Object(c)) = op.effect.get("create") {
        for (k, v) in c.iter().filter(|(k, _)| *k != "hid") {
            match (k.as_str(), v) {
                ("data", Value::Object(d)) => {
                    out.extend(d.iter().map(|(dk, v)| (format!("data.{dk}"), v.clone())))
                }
                _ => {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
    }
    if let Some(Value::Object(set)) = op.effect.get("set") {
        out.extend(set.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    for k in op.effect["unset"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        out.insert(k.to_string(), Value::Null);
    }
    out
}

/// Keys every edit moves, or that follow from other keys: two ops setting
/// them differently is not two intents.
const NOT_A_CONFLICT: &[&str] = &["modify_time", "content_hash", "verb", "slug", "device_id"];

// ─── The op ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct Op {
    endpoint: String,
    seq: i64,
    hlc: String,
    hid: String,
    slug: String,
    kind: String,
    #[serde(default)]
    raw: Option<String>,
    effect: Value,
    #[serde(default)]
    deps: BTreeMap<String, i64>,
    sig: String,
    #[serde(skip)]
    body: String,
}

impl Op {
    fn parse(body: &str) -> Result<Op> {
        let v: Value = serde_json::from_str(body)?;
        let mut op: Op = serde_json::from_value(v.clone())?;
        op.body = canonical_json(&v);
        Ok(op)
    }

    /// Fold order: baselines first, then `(hlc, endpoint)`. A baseline is
    /// an entry as it stood before it had ops, so it goes under every edit
    /// even when a copy signed it later than another copy's edits.
    fn order(&self) -> (bool, &str, &str) {
        (!self.is_baseline(), &self.hlc, &self.endpoint)
    }

    fn is_baseline(&self) -> bool {
        self.kind == "baseline"
    }

    fn same(&self, other: &Op) -> bool {
        self.endpoint == other.endpoint && self.seq == other.seq
    }

    /// Had the copy that made this op seen `other`?
    fn saw(&self, other: &Op) -> bool {
        self.endpoint == other.endpoint && self.seq > other.seq
            || self
                .deps
                .get(&other.endpoint)
                .is_some_and(|s| *s >= other.seq)
    }

    fn deletes(&self) -> bool {
        self.effect.get("deleted") == Some(&Value::Bool(true))
    }
}

fn digest(unsigned: &Value) -> [u8; 32] {
    Sha256::digest(canonical_json(unsigned).as_bytes()).into()
}

/// Why `body` is not a genuine op of its endpoint, if it is not.
fn check_signature(body: &str, op: &Op) -> std::result::Result<(), String> {
    let key = unhex::<32>(&op.endpoint)
        .and_then(|b| VerifyingKey::from_bytes(&b).ok())
        .ok_or("endpoint is not an Ed25519 public key")?;
    let sig = unhex::<64>(&op.sig).ok_or("sig is not an Ed25519 signature")?;
    let mut unsigned: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if let Some(m) = unsigned.as_object_mut() {
        m.remove("sig");
    }
    key.verify_strict(&digest(&unsigned), &Signature::from_bytes(&sig))
        .map_err(|_| "bad signature".to_string())?;
    if op.seq < 1 || parse_hlc(&op.hlc).is_none() {
        return Err("malformed seq or hlc".into());
    }
    Ok(())
}

fn store(conn: &Connection, op: &Op, applied: bool) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO ops (endpoint, seq, hlc, hid, slug, kind, applied, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?
    .execute(params![
        op.endpoint,
        op.seq,
        op.hlc,
        op.hid,
        op.slug,
        op.kind,
        applied,
        op.body
    ])?;
    Ok(())
}

fn vector_where(conn: &Connection, hid: Option<&str>) -> Result<BTreeMap<String, i64>> {
    // Two statements, not `?1 IS NULL OR hid = ?1`: the OR keeps SQLite off
    // `ops_hid`, and every op signed would scan every op held.
    let pair = |r: &rusqlite::Row<'_>| Ok((r.get(0)?, r.get(1)?));
    let rows = match hid {
        Some(h) => conn
            .prepare_cached(
                "SELECT endpoint, MAX(seq) FROM ops WHERE hid = ?1 AND applied = 1 GROUP BY endpoint",
            )?
            .query_map(params![h], pair)?
            .collect::<rusqlite::Result<_>>(),
        None => conn
            .prepare_cached("SELECT endpoint, MAX(seq) FROM ops WHERE applied = 1 GROUP BY endpoint")?
            .query_map([], pair)?
            .collect::<rusqlite::Result<_>>(),
    };
    rows.map_err(DatumError::Db)
}

/// Sign and store a new op of this copy's.
fn push(conn: &Connection, ep: &Endpoint, e: &Entry, kind: &str, effect: Value) -> Result<()> {
    let seq: i64 = conn
        .prepare_cached("SELECT COALESCE(MAX(seq), 0) + 1 FROM ops WHERE endpoint = ?1")?
        .query_row(params![ep.id], |r| r.get(0))?;
    let (raw, alias_chain) =
        INVOCATION.with(|i| (i.borrow().raw.clone(), i.borrow().alias_chain.clone()));
    let hlc = next_hlc(conn)?;
    let mut v = json!({
        "endpoint": ep.id,
        "seq": seq,
        "hlc": hlc,
        "slug": e.slug,
        "hid": e.hid,
        "kind": kind,
        "raw": raw,
        "alias_chain": alias_chain,
        "effect": effect,
        "deps": vector_where(conn, Some(e.hid.as_str()))?,
    });
    let sig = hex(&ep.key.sign(&digest(&v)).to_bytes());
    v["sig"] = sig.clone().into();
    let op = Op {
        endpoint: ep.id.clone(),
        seq,
        hlc,
        hid: e.hid.0.clone(),
        slug: e.slug.0.clone(),
        kind: kind.into(),
        raw: None,
        // `store` reads the head and the body; the effect is in the body.
        effect: Value::Null,
        deps: BTreeMap::new(),
        sig,
        body: canonical_json(&v),
    };
    store(conn, &op, true)
}

fn create_effect(e: &Entry) -> Value {
    json!({ "create": row(e) })
}

fn has_ops(conn: &Connection, hid: &Hid) -> Result<bool> {
    Ok(conn
        .prepare_cached("SELECT 1 FROM ops WHERE hid = ?1 LIMIT 1")?
        .exists(params![hid])?)
}

fn fetch_by_hid(conn: &Connection, hid: &str) -> Result<Option<Entry>> {
    conn.prepare_cached("SELECT * FROM entries WHERE hid = ?1")?
        .query_row(params![hid], row_to_entry)
        .optional()
        .map_err(DatumError::Db)
}

/// Emit the op for a change to entry `hid` that has just been written.
/// `pre` is the row before it (`None` for a create). Call it where the
/// change calls `history::record`, after the write, in the same
/// transaction. An entry older than its ops gets a `baseline` op of its
/// pre-change row first, as `history::record` seeds a trail, so every
/// entry's ops open with a create.
pub(super) fn emit(
    conn: &Connection,
    pre: Option<&Entry>,
    hid: &Hid,
    change: ChangeKind,
) -> Result<()> {
    // A migration walk writes entries at a schema from before ops (v42):
    // those rows are older than their ops, and get a baseline when synced.
    let has_ops_table = conn
        .prepare_cached("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'ops'")?
        .exists([])?;
    let Some(post) = fetch_by_hid(conn, hid.as_str())?.filter(|_| has_ops_table) else {
        return Ok(());
    };
    let effect = match pre {
        Some(pre) => diff(pre, &post),
        None => create_effect(&post),
    };
    if effect.as_object().is_some_and(Map::is_empty) {
        return Ok(());
    }
    let ep = endpoint(conn)?;
    if let Some(pre) = pre {
        if !has_ops(conn, hid)? {
            push(conn, &ep, pre, "baseline", create_effect(pre))?;
        }
    }
    let kind = match (change, effect.get("deleted").and_then(Value::as_bool)) {
        (ChangeKind::UserEdit, Some(true)) => "delete",
        (ChangeKind::UserEdit, Some(false)) => "undelete",
        (ChangeKind::UserEdit, None) => "update",
        (k, _) => k.as_str(),
    };
    push(conn, &ep, &post, kind, effect)
}

// ─── Apply ──────────────────────────────────────────────────────────────────

/// What one `sync apply` did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplySummary {
    pub applied: usize,
    pub already_had: usize,
    /// Ops still waiting for an op before them — held, not dropped.
    pub held: usize,
    pub conflicts: usize,
    /// Ops refused, each named with the reason.
    pub refused: Vec<String>,
}

/// The ops of `hid` that are applied, in fold order.
fn ops_of(conn: &Connection, hid: &str) -> Result<Vec<Op>> {
    let mut stmt = conn.prepare_cached("SELECT body FROM ops WHERE hid = ?1 AND applied = 1")?;
    let bodies = stmt
        .query_map(params![hid], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut ops = bodies
        .iter()
        .map(|b| Op::parse(b))
        .collect::<Result<Vec<_>>>()?;
    ops.sort_by(|a, b| a.order().cmp(&b.order()));
    Ok(ops)
}

/// The row an entry's ops make, folded in [`Op::order`]. `base` is
/// the row to start from when the first op is not a create.
fn fold(ops: &[Op], base: Option<&Entry>) -> Option<Map<String, Value>> {
    let mut row_now = base.map(row);
    for op in ops {
        fold_into(&mut row_now, &op.effect);
    }
    row_now
}

fn entry_from(row: Map<String, Value>) -> Result<Entry> {
    Ok(serde_json::from_value(Value::Object(row))?)
}

/// A snapshot for a conflict row: the entry as `op` left it, with the op
/// named under `extra_metadata.sync_op`.
fn conflict_side(entry: &Entry, op: &Op, apply_it: bool) -> Result<Entry> {
    let mut r = Some(row(entry));
    if apply_it {
        fold_into(&mut r, &op.effect);
    }
    let mut side = entry_from(r.unwrap_or_default())?;
    let mut meta = side.extra_metadata.take().unwrap_or_else(|| json!({}));
    if let Some(m) = meta.as_object_mut() {
        m.insert(
            "sync_op".into(),
            json!({ "op": format!("{}/{}", op.endpoint, op.seq), "hlc": op.hlc, "raw": op.raw }),
        );
    }
    side.extra_metadata = Some(meta);
    Ok(side)
}

/// File a `sync_conflict` for every op of the entry concurrent with `x`
/// that gives one of its keys a different value, or that is a delete where
/// the other is an edit. The later HLC is the winner; with a delete, the
/// delete stands and the edit is the loser whatever the clocks say.
fn file_conflicts(conn: &Connection, x: &Op, ops: &[Op], merged: &Entry) -> Result<usize> {
    let xv = values(x);
    let edits = |o: &Op, v: &BTreeMap<String, Value>| {
        o.effect.get("deleted").is_none() && v.keys().any(|k| !NOT_A_CONFLICT.contains(&k.as_str()))
    };
    let mut filed = 0;
    // A baseline is under every edit, so it is concurrent only with another
    // copy's baseline of the same entry.
    let concurrent =
        |y: &&Op| !y.same(x) && !x.saw(y) && !y.saw(x) && x.is_baseline() == y.is_baseline();
    for y in ops.iter().filter(concurrent) {
        let yv = values(y);
        let clash = xv.iter().any(|(k, v)| {
            !NOT_A_CONFLICT.contains(&k.as_str()) && yv.get(k).is_some_and(|w| w != v)
        });
        let loser = match (x.deletes(), y.deletes()) {
            (true, false) if edits(y, &yv) => y,
            (false, true) if edits(x, &xv) => x,
            _ if clash => std::cmp::min_by(x, y, |a, b| a.order().cmp(&b.order())),
            _ => continue,
        };
        let winner = if loser.same(x) { y } else { x };
        crate::integrity::file_sync_conflict(
            conn,
            &conflict_side(merged, loser, true)?,
            &conflict_side(merged, winner, false)?,
        )?;
        filed += 1;
    }
    Ok(filed)
}

/// Rows other than `e` holding an identity `e` needs: its slug, or its live
/// state-bearing `(field, occur_time)` — the two UNIQUE indexes, both of
/// which a superseded row is outside (ported from the relay sync's apply).
fn blockers(conn: &Connection, e: &Entry) -> Result<Vec<Entry>> {
    let mut found: Vec<Entry> = conn
        .prepare("SELECT * FROM entries WHERE slug = ?1 AND hid != ?2 AND superseded_by IS NULL")?
        .query_map(params![e.slug, e.hid], row_to_entry)?
        .collect::<rusqlite::Result<_>>()?;
    if let (None, true, Some(occur)) = (e.deleted_at, e.asserts_state(), e.occur_time.as_ref()) {
        let rows: Vec<Entry> = conn
            .prepare(
                "SELECT * FROM entries WHERE field = ?1 AND occur_time = ?2
                   AND deleted_at IS NULL AND superseded_by IS NULL
                   AND state IS NOT NULL AND hid != ?3",
            )?
            .query_map(
                params![e.field, occur.utc.to_rfc3339(), e.hid],
                row_to_entry,
            )?
            .collect::<rusqlite::Result<_>>()?;
        found.extend(rows.into_iter().filter(|r| r.slug != e.slug));
    }
    Ok(found)
}

/// One event logged on two copies — the same fact, place included — as
/// two copies each declaring a field's `spec` before they ever synced.
fn same_event(a: &Entry, b: &Entry) -> bool {
    a.expected_content_hash() == b.expected_content_hash()
        && (a.lat, a.lon, &a.location) == (b.lat, b.lon, &b.location)
}

/// Write the row an entry's ops fold to. Two distinct entries claiming one
/// identity are both kept and the later-created one is superseded
/// (`superseded_by`) — the supersession `conflicts` already triages,
/// decided from the rows alone so every copy reaches the same one. An
/// `identity_collision` names the pair unless they are one event twice.
fn write_merged(conn: &Connection, pre: Option<&Entry>, mut next: Entry) -> Result<()> {
    next.superseded_by = pre.and_then(|p| p.superseded_by.clone());
    if next.superseded_by.is_none() {
        for b in blockers(conn, &next)? {
            let (lost, kept) = if (&next.create_time, &next.hid.0) < (&b.create_time, &b.hid.0) {
                history::record(conn, &b, ChangeKind::SyncMerge)?;
                conn.execute(
                    "UPDATE entries SET superseded_by = ?2 WHERE hid = ?1",
                    params![b.hid, next.hid],
                )?;
                let mut lost = b.clone();
                lost.superseded_by = Some(next.hid.clone());
                (lost, next.clone())
            } else if next.superseded_by.is_none() {
                next.superseded_by = Some(b.hid.clone());
                (next.clone(), b)
            } else {
                continue;
            };
            if !same_event(&lost, &kept) {
                crate::integrity::file_identity_collision(conn, &lost, &kept)?;
            }
        }
    }
    match pre {
        Some(pre) => {
            if row(pre) == row(&next) && pre.superseded_by == next.superseded_by {
                return Ok(());
            }
            history::record(conn, pre, ChangeKind::SyncMerge)?;
            conn.execute(
                "UPDATE entries SET superseded_by = ?2 WHERE hid = ?1",
                params![next.hid, next.superseded_by],
            )?;
            apply_entry_state(conn, &next)
        }
        None => {
            try_insert(conn, &next.slug, &next.hid, &next)?;
            if let Some(stored) = fetch_by_hid(conn, next.hid.as_str())? {
                history::record(conn, &stored, ChangeKind::SyncMerge)?;
            }
            Ok(())
        }
    }
}

/// Apply one op that is ready: refold its entry and write the result.
fn apply_one(conn: &Connection, x: &Op) -> Result<usize> {
    conn.execute(
        "UPDATE ops SET applied = 1 WHERE endpoint = ?1 AND seq = ?2",
        params![x.endpoint, x.seq],
    )?;
    let pre = fetch_by_hid(conn, &x.hid)?;
    let ops = ops_of(conn, &x.hid)?;
    let base = pre
        .as_ref()
        .filter(|_| ops.first().is_none_or(|o| o.effect.get("create").is_none()));
    let Some(merged) = fold(&ops, base) else {
        return Ok(0);
    };
    let merged = entry_from(merged)?;
    let filed = file_conflicts(conn, x, &ops, &merged)?;
    write_merged(conn, pre.as_ref(), merged)?;
    Ok(filed)
}

impl Db {
    /// This copy's endpoint secret, minted on first need. It is also the
    /// iroh endpoint's key, so the iroh endpoint id IS the ops' `endpoint`
    /// (da-rugg8).
    /// Stored in the clear in `sync_local`, like the rest of the file: who
    /// can read the database can sign as this copy and, since da-rugg8,
    /// answer to its endpoint id on the network.
    pub fn sync_endpoint_secret(&self) -> Result<[u8; 32]> {
        Ok(endpoint(&self.conn)?.key.to_bytes())
    }

    /// This copy's version vector: per endpoint, the highest seq applied.
    /// Ops apply only in seq order, so that seq is also the highest
    /// contiguous one.
    pub fn sync_vector(&self) -> Result<BTreeMap<String, i64>> {
        vector_where(&self.conn, None)
    }

    /// Every applied op `since` lacks, as canonical JSON lines in
    /// `(endpoint, seq)` order. First signs a `baseline` create for every
    /// entry no op covers — one older than its ops, or bulk-imported — so a
    /// copy syncing from empty receives every entry.
    pub fn sync_ops_since(&self, since: &BTreeMap<String, i64>) -> Result<Vec<String>> {
        let tx = self.conn.unchecked_transaction()?;
        let uncovered: Vec<Entry> = tx
            .prepare("SELECT * FROM entries WHERE hid NOT IN (SELECT hid FROM ops) ORDER BY hid")?
            .query_map([], row_to_entry)?
            .collect::<rusqlite::Result<_>>()?;
        if !uncovered.is_empty() {
            let ep = endpoint(&tx)?;
            for e in &uncovered {
                push(&tx, &ep, e, "baseline", create_effect(e))?;
            }
        }
        let mut stmt = tx.prepare(
            "SELECT endpoint, seq, body FROM ops WHERE applied = 1 ORDER BY endpoint, seq",
        )?;
        let lines = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get(2)?))
            })?
            .filter_map(|r| match r {
                Ok((e, s, body)) if s > since.get(&e).copied().unwrap_or(0) => Some(Ok(body)),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<rusqlite::Result<Vec<String>>>()?;
        drop(stmt);
        tx.commit()?;
        Ok(lines)
    }

    /// Apply op lines. Each signature is checked (a bad one is refused and
    /// named), ops already held are skipped, and an op whose predecessor on
    /// its endpoint — or any op it depends on — is missing is held until it
    /// arrives. The rest apply in HLC order. An applied op writes
    /// `entry_history` (`sync_merge`) and emits no op of its own: it is that
    /// op.
    pub fn sync_apply(&self, lines: &str) -> Result<ApplySummary> {
        let tx = self.conn.unchecked_transaction()?;
        let mut s = ApplySummary::default();
        for line in lines.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let op = match Op::parse(line) {
                Ok(op) => op,
                Err(e) => {
                    s.refused.push(format!("unreadable op: {e}"));
                    continue;
                }
            };
            if let Err(why) = check_signature(line, &op) {
                s.refused.push(format!("{}/{}: {why}", op.endpoint, op.seq));
                continue;
            }
            let held: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM ops WHERE endpoint = ?1 AND seq = ?2)",
                params![op.endpoint, op.seq],
                |r| r.get(0),
            )?;
            if held {
                s.already_had += 1;
            } else {
                store(&tx, &op, false)?;
            }
        }
        let mut vector = vector_where(&tx, None)?;
        loop {
            let mut waiting: Vec<Op> = tx
                .prepare("SELECT body FROM ops WHERE applied = 0")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .map(|b| Op::parse(b))
                .collect::<Result<_>>()?;
            waiting.sort_by(|a, b| a.order().cmp(&b.order()));
            let mut progressed = false;
            for op in &waiting {
                let have = |e: &str| vector.get(e).copied().unwrap_or(0);
                if op.seq == have(&op.endpoint) + 1 && op.deps.iter().all(|(e, n)| have(e) >= *n) {
                    s.conflicts += apply_one(&tx, op)?;
                    vector.insert(op.endpoint.clone(), op.seq);
                    s.applied += 1;
                    progressed = true;
                }
            }
            if !progressed {
                s.held = waiting.len();
                break;
            }
        }
        tx.commit()?;
        Ok(s)
    }

    /// The state hash: SHA-256 over the canonical JSON of every entry row,
    /// live and tombstoned, in `(slug, hid)` order, each with its
    /// `superseded_by` — so two copies agree only if they hold exactly the
    /// same data, bookkeeping included. The map is the same hash per column,
    /// to narrow a mismatch down.
    pub fn sync_state_hash(&self) -> Result<(String, BTreeMap<String, String>)> {
        let rows: Vec<Map<String, Value>> = self
            .conn
            .prepare("SELECT * FROM entries ORDER BY slug, hid")?
            .query_map([], row_to_entry)?
            .map(|e| {
                e.map(|e| {
                    let mut r = row(&e);
                    r.insert("superseded_by".into(), json!(e.superseded_by));
                    r
                })
            })
            .collect::<rusqlite::Result<_>>()?;
        let keys: BTreeSet<&String> = rows.iter().flat_map(|r| r.keys()).collect();
        let per_key = keys
            .into_iter()
            .map(|k| {
                let column: Vec<Value> = rows
                    .iter()
                    .map(|r| r.get(k).cloned().unwrap_or(Value::Null))
                    .collect();
                (
                    k.clone(),
                    sha256_hex(&canonical_json(&Value::Array(column))),
                )
            })
            .collect();
        let all = Value::Array(rows.into_iter().map(Value::Object).collect());
        Ok((sha256_hex(&canonical_json(&all)), per_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_sorts_keys_at_every_depth() {
        let v = json!({"b": 1, "a": {"d": [{"f": 1, "e": 2}], "c": null}});
        assert_eq!(
            canonical_json(&v),
            r#"{"a":{"c":null,"d":[{"e":2,"f":1}]},"b":1}"#
        );
    }

    #[test]
    fn hlc_orders_as_a_string_and_round_trips() {
        let a = format_hlc(1_790_000_000_123, 3);
        assert_eq!(a, "2026-09-21T14:13:20.123Z/0003");
        assert_eq!(parse_hlc(&a), Some((1_790_000_000_123, 3)));
        assert!(format_hlc(1_790_000_000_123, 10) > a);
        assert!(format_hlc(1_790_000_000_124, 0) > format_hlc(1_790_000_000_123, 9999));
        assert_eq!(parse_hlc("2026-09-21T14:13:20.123Z/3"), None);
    }
}
