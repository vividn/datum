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

pub mod chain;
pub mod peers;
pub mod status;
use chain::{Chain, CHAIN_HID};

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
/// device and the file path it was minted at, and a file that opens on
/// another device, or at another path while the original is still there
/// (copied, restored beside it), mints its own rather than sign as the copy
/// it came from: two copies with one key would number their ops from one
/// seq. A file MOVED keeps its key, and so its membership. A copy put back
/// over the original's own path is the case this cannot see; `sync_apply`
/// refuses the fork it makes, and `datum sync rekey` mints a new key.
fn endpoint(conn: &Connection) -> Result<Endpoint> {
    let device = device_id();
    let path = conn.path().unwrap_or_default().to_string();
    let held: Option<(String, String, String)> = conn
        .prepare_cached("SELECT endpoint_secret, device_id, path FROM sync_local WHERE id = 1")?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    let held = held.filter(|(_, d, p)| {
        *d == device && (*p == path || p.is_empty() || !std::path::Path::new(p).exists())
    });
    if held.as_ref().is_some_and(|(_, _, p)| *p != path) {
        conn.execute("UPDATE sync_local SET path = ?1", params![path])?;
    }
    if let Some((secret, _, _)) = held {
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
        "INSERT OR REPLACE INTO sync_local (id, endpoint_secret, endpoint_id, device_id, path)
         VALUES (1, ?1, ?2, ?3, ?4)",
        params![hex(&key.to_bytes()), id, device, path],
    )?;
    Ok(Endpoint { key, id })
}

/// Said wherever two copies are found signing with one key (da-p79vq).
const SHARED_KEY: &str = "these are two copies of one database sharing an endpoint key; \
     run `datum sync rekey` on one of them";

/// This copy's endpoint id, and the highest seq it has signed.
fn own_head(conn: &Connection) -> Result<(String, i64)> {
    let id = endpoint(conn)?.id;
    let seq = conn
        .prepare_cached("SELECT COALESCE(MAX(seq), 0) FROM ops WHERE endpoint = ?1")?
        .query_row(params![id], |r| r.get(0))?;
    Ok((id, seq))
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
    let (ms, counter) = advance(
        last.as_deref().and_then(parse_hlc),
        Utc::now().timestamp_millis(),
    );
    Ok(format_hlc(ms, counter))
}

/// The HLC after `last` at wall time `now`. A counter that would outgrow its
/// four digits moves the wall on a millisecond instead, so it stays fixed
/// width and still orders as a string.
fn advance(last: Option<(i64, u32)>, now: i64) -> (i64, u32) {
    match last {
        Some((ms, _)) if now > ms => (now, 0),
        Some((ms, c)) if c < HLC_COUNTER_MAX => (ms, c + 1),
        Some((ms, _)) => (ms + 1, 0),
        None => (now, 0),
    }
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
    // `data` is diffed key by key so that two copies setting different keys
    // of it both land, rather than one whole object replacing the other.
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
    // A whole `data` object — a create's, or an edit that gave data to an
    // entry that had none — is its keys, as `diff` names them.
    let set = op.effect.get("create").or_else(|| op.effect.get("set"));
    for (k, v) in set.and_then(Value::as_object).into_iter().flatten() {
        match (k.as_str(), v) {
            ("hid", _) => {}
            ("data", Value::Object(d)) => {
                out.extend(d.iter().map(|(dk, v)| (format!("data.{dk}"), v.clone())))
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
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
    /// even when a copy signed it later than another copy's edits. Of two
    /// baselines of one entry — copies that diverged before either had ops —
    /// a tombstone goes last and so stands, as a delete stands against a
    /// concurrent edit (da-p79vq).
    fn order(&self) -> (bool, bool, &str, &str) {
        let tombstone = self.is_baseline() && !self.effect["create"]["deleted_at"].is_null();
        (!self.is_baseline(), tombstone, &self.hlc, &self.endpoint)
    }

    fn is_chain(&self) -> bool {
        self.kind == "chain"
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

/// Sign and store a new op of this copy's; its seq.
fn push(
    conn: &Connection,
    ep: &Endpoint,
    hid: &str,
    slug: &str,
    kind: &str,
    effect: Value,
) -> Result<i64> {
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
        "slug": slug,
        "hid": hid,
        "kind": kind,
        "raw": raw,
        "alias_chain": alias_chain,
        "effect": effect,
        "deps": vector_where(conn, Some(hid))?,
    });
    let sig = hex(&ep.key.sign(&digest(&v)).to_bytes());
    v["sig"] = sig.clone().into();
    let op = Op {
        endpoint: ep.id.clone(),
        seq,
        hlc,
        hid: hid.into(),
        slug: slug.into(),
        kind: kind.into(),
        raw: None,
        // `store` reads the head and the body; the effect is in the body.
        effect: Value::Null,
        deps: BTreeMap::new(),
        sig,
        body: canonical_json(&v),
    };
    store(conn, &op, true)?;
    Ok(seq)
}

/// Sign and store an op of an entry's.
fn push_entry(
    conn: &Connection,
    ep: &Endpoint,
    e: &Entry,
    kind: &str,
    effect: Value,
) -> Result<()> {
    push(conn, ep, e.hid.as_str(), &e.slug.0, kind, effect).map(drop)
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
            push_entry(conn, &ep, pre, "baseline", create_effect(pre))?;
        }
    }
    let kind = match (change, effect.get("deleted").and_then(Value::as_bool)) {
        (ChangeKind::UserEdit, Some(true)) => "delete",
        (ChangeKind::UserEdit, Some(false)) => "undelete",
        (ChangeKind::UserEdit, None) => "update",
        (k, _) => k.as_str(),
    };
    push_entry(conn, &ep, &post, kind, effect)
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
        .prepare_cached(
            "SELECT * FROM entries WHERE slug = ?1 AND hid != ?2 AND superseded_by IS NULL",
        )?
        .query_map(params![e.slug, e.hid], row_to_entry)?
        .collect::<rusqlite::Result<_>>()?;
    if let (None, true, Some(occur)) = (e.deleted_at, e.asserts_state(), e.occur_time.as_ref()) {
        let rows: Vec<Entry> = conn
            .prepare_cached(
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
            if pre.superseded_by != next.superseded_by {
                conn.execute(
                    "UPDATE entries SET superseded_by = ?2 WHERE hid = ?1",
                    params![next.hid, next.superseded_by],
                )?;
            }
            // Bookkeeping alone is no change of content, and no history.
            if row(pre) == row(&next) {
                return Ok(());
            }
            history::record(conn, pre, ChangeKind::SyncMerge)?;
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

/// Apply every held op that is ready, records first, until none is; then
/// refuse what still waits from an endpoint the chain does not allow.
fn settle(conn: &Connection, s: &mut ApplySummary) -> Result<()> {
    let mut chain = Chain::load(conn)?;
    let mut vector = vector_where(conn, None)?;
    let waiting = loop {
        let mut waiting = waiting_ops(conn)?;
        // Records first: membership decides which of the rest apply. Then
        // HLC order, which is causal — an op's clock passes every op its
        // copy held — so one pass applies what is ready. Fold order is
        // `ops_of`'s; `Op::order` here put tombstone baselines after live
        // ones, one pass per run of each: quadratic (da-kg95e).
        waiting.sort_by(|a, b| {
            (!a.is_chain(), &a.hlc, &a.endpoint).cmp(&(!b.is_chain(), &b.hlc, &b.endpoint))
        });
        let mut progressed = false;
        for op in &waiting {
            let have = |e: &str| vector.get(e).copied().unwrap_or(0);
            let ready =
                op.seq == have(&op.endpoint) + 1 && op.deps.iter().all(|(e, n)| have(e) >= *n);
            if !ready || !op.is_chain() && chain.allows(&op.endpoint, &op.hlc).is_err() {
                continue;
            }
            progressed = true;
            if !op.is_chain() {
                s.conflicts += apply_one(conn, op)?;
                vector.insert(op.endpoint.clone(), op.seq);
                s.applied += 1;
                continue;
            }
            conn.execute(
                "UPDATE ops SET applied = 1 WHERE endpoint = ?1 AND seq = ?2",
                params![op.endpoint, op.seq],
            )?;
            // The chain is refolded whole per record: chains hold a handful
            // of records, and the fold is what every copy must agree on.
            chain = Chain::load(conn)?;
            match chain.rejected.get(&(op.endpoint.clone(), op.seq)) {
                Some(why) => s.refused.push(format!("{}/{}: {why}", op.endpoint, op.seq)),
                None => s.applied += 1,
            }
            reconsider(conn, &chain, s)?;
            chain::note(conn, &chain, false)?;
            vector = vector_where(conn, None)?;
            // What waits has changed with the membership.
            break;
        }
        if !progressed {
            break waiting;
        }
    };
    // What still waits from an endpoint the chain refuses is refused.
    let mut held = 0;
    for op in &waiting {
        match chain.allows(&op.endpoint, &op.hlc) {
            Err(why) if !op.is_chain() => refuse(conn, op, &why, s)?,
            _ => held += 1,
        }
    }
    s.held = held;
    Ok(())
}

fn waiting_ops(conn: &Connection) -> Result<Vec<Op>> {
    conn.prepare_cached("SELECT body FROM ops WHERE applied = 0")?
        .query_map([], |r| r.get::<_, String>(0))?
        .map(|b| Op::parse(&b?))
        .collect()
}

/// Refuse an op the chain does not allow: it is kept, applied = 2, so it is
/// not asked for again, and a membership that later allows it lets it wait
/// again ([`reconsider`]). The first refused of an endpoint files a
/// `sync_unauthorised` warning, which `datum sync ack` settles.
fn refuse(conn: &Connection, op: &Op, why: &str, s: &mut ApplySummary) -> Result<()> {
    conn.execute(
        "UPDATE ops SET applied = 2 WHERE endpoint = ?1 AND seq = ?2",
        params![op.endpoint, op.seq],
    )?;
    s.refused.push(format!("{}/{}: {why}", op.endpoint, op.seq));
    let tag = format!("\"op\":\"{}/", op.endpoint);
    let warned = conn
        .prepare_cached(
            "SELECT 1 FROM integrity_warnings WHERE kind = 'sync_unauthorised'
               AND resolution IS NULL AND instr(local_entry, ?1) > 0",
        )?
        .exists(params![tag])?;
    let mut r = fetch_by_hid(conn, &op.hid)?.map(|e| row(&e));
    fold_into(&mut r, &op.effect);
    if let (false, Some(r)) = (warned, r) {
        let mut side = conflict_side(&entry_from(r)?, op, false)?;
        if let Some(m) = side.extra_metadata.as_mut() {
            m["sync_op"]["refused"] = why.into();
        }
        crate::integrity::file_sync_unauthorised(conn, &side)?;
    }
    Ok(())
}

/// The membership moved: refused ops it now allows wait again, and applied
/// ops it no longer allows — a revoked endpoint's, made after the revoke —
/// are refused and their entries refolded without them.
fn reconsider(conn: &Connection, chain: &Chain, s: &mut ApplySummary) -> Result<()> {
    let bodies = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> Result<Vec<Op>> {
        conn.prepare_cached(sql)?
            .query_map(p, |r| r.get::<_, String>(0))?
            .map(|b| Op::parse(&b?))
            .collect()
    };
    for op in bodies("SELECT body FROM ops WHERE applied = 2", &[])? {
        if chain.allows(&op.endpoint, &op.hlc).is_ok() {
            conn.execute(
                "UPDATE ops SET applied = 0 WHERE endpoint = ?1 AND seq = ?2",
                params![op.endpoint, op.seq],
            )?;
        }
    }
    // A revoked endpoint's applied ops from its revoke on: its own, by key.
    let mut hids = BTreeSet::new();
    for m in &chain.members {
        let Some(revoked) = &m.revoked else { continue };
        let sql = "SELECT body FROM ops
                   WHERE endpoint = ?1 AND applied = 1 AND hlc >= ?2 AND hid != ?3";
        for op in bodies(sql, &[&m.endpoint, revoked, &CHAIN_HID])? {
            if let Err(why) = chain.allows(&op.endpoint, &op.hlc) {
                refuse(conn, &op, &why, s)?;
                hids.insert(op.hid);
            }
        }
    }
    for hid in hids {
        let pre = fetch_by_hid(conn, &hid)?;
        match fold(&ops_of(conn, &hid)?, None) {
            Some(r) => write_merged(conn, pre.as_ref(), entry_from(r)?)?,
            None => {
                if let Some(pre) = pre {
                    history::record(conn, &pre, ChangeKind::SyncMerge)?;
                    conn.execute("DELETE FROM entries WHERE hid = ?1", params![hid])?;
                }
            }
        }
    }
    Ok(())
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

    /// Mint a new endpoint key for this copy — one of two copies found
    /// sharing a key (da-p79vq). Its ops so far stay signed by the old key;
    /// new ones use the new, which is also a new iroh endpoint id and no
    /// member of the chain: the copy joins it again. Returns the new id.
    pub fn sync_rekey(&self) -> Result<String> {
        self.conn.execute("DELETE FROM sync_local", [])?;
        Ok(endpoint(&self.conn)?.id)
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
    /// copy syncing from empty receives every entry. With `overlap`, this
    /// copy's own op AT `since`'s seq for it comes too — the one line that
    /// shows a copy sharing this key, whose vector agrees with this one's,
    /// a different op there (the pipe; iroh refuses such a peer by id).
    pub fn sync_ops_since(
        &self,
        since: &BTreeMap<String, i64>,
        overlap: bool,
    ) -> Result<Vec<String>> {
        let tx = self.conn.unchecked_transaction()?;
        // A vector holding more of this copy's ops than it signed was made
        // by another copy signing with this key.
        let (mine, signed) = own_head(&tx)?;
        if since.get(&mine).is_some_and(|s| *s > signed) {
            return Err(DatumError::Other(SHARED_KEY.into()));
        }
        let uncovered: Vec<Entry> = tx
            .prepare("SELECT * FROM entries WHERE hid NOT IN (SELECT hid FROM ops) ORDER BY hid")?
            .query_map([], row_to_entry)?
            .collect::<rusqlite::Result<_>>()?;
        if !uncovered.is_empty() {
            let ep = endpoint(&tx)?;
            for e in &uncovered {
                push_entry(&tx, &ep, e, "baseline", create_effect(e))?;
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
                Ok((e, s, body))
                    if s > since.get(&e).copied().unwrap_or(0)
                        || overlap && e == mine && since.get(&e) == Some(&s) =>
                {
                    Some(Ok(body))
                }
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
        let (mine, signed) = own_head(&tx)?;
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
            let held: Option<String> = tx
                .prepare_cached("SELECT body FROM ops WHERE endpoint = ?1 AND seq = ?2")?
                .query_row(params![op.endpoint, op.seq], |r| r.get(0))
                .optional()?;
            // Two copies signing with one key each number their ops from the
            // same seq: another op at a seq held, or an op of this copy's own
            // that it never signed. Say so rather than take one for the other.
            let forked = op.endpoint == mine && op.seq > signed;
            match held {
                None if !forked => store(&tx, &op, false)?,
                Some(body) if body == op.body => s.already_had += 1,
                _ => s
                    .refused
                    .push(format!("{}/{}: {SHARED_KEY}", op.endpoint, op.seq)),
            }
        }
        settle(&tx, &mut s)?;
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

    #[test]
    fn hlc_never_runs_behind_and_a_full_counter_moves_the_wall() {
        assert_eq!(advance(None, 50), (50, 0));
        assert_eq!(advance(Some((40, 7)), 50), (50, 0));
        assert_eq!(
            advance(Some((60, 7)), 50),
            (60, 8),
            "a clock behind the last op"
        );
        let full = advance(Some((60, HLC_COUNTER_MAX)), 50);
        assert_eq!(full, (61, 0));
        assert!(format_hlc(full.0, full.1) > format_hlc(60, HLC_COUNTER_MAX));
    }
}
