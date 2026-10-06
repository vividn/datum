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
//!   Two entries with one slug are one entry ([`merge_rows`], da-04glw):
//!   the earlier-created shows both, the later wins a key both have, and the
//!   later stays superseded, the record of what was merged in.
//! - **Conflict.** Two ops are concurrent when neither's `deps` covers the
//!   other. Concurrent ops that give one key different values, or a delete
//!   and a concurrent edit, file a `sync_conflict` for the loser.
//!
//! Seq, HLC and the version vector are all read off the `ops` table rather
//! than kept as counters beside it, so there is no second copy of them to
//! drift — but for a filtered reader's (da-u81u1), below.
//!
//! - **Filtered readers (da-u81u1).** A reader whose membership carries a
//!   [`Filter`] is sent only the entries that belong to its fields (an
//!   entry op's `field`: the entry's field, or for a spec the field it
//!   specifies), each entry's ops on ONE line (a JSON array) so no apply
//!   chunk splits it, an entry that moved into the filter whole, a
//!   `{"remove": hid}` for one that moved out, and last `{"caught_up":
//!   vector, "fields": […], "since": …}`: everything up to those seqs that
//!   passes those fields was sent, as an answer to that `since`. Such a
//!   copy applies what it receives without waiting for gaps, drops what
//!   falls outside its filter, and its vector is the marks made under a
//!   filter covering its own (`sync_caught_up`), so a widened filter is sent
//!   in full again.

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
mod pause;
pub mod peers;
pub mod status;
use chain::{Chain, Filter, Role, CHAIN_HID};

// ─── The invocation ─────────────────────────────────────────────────────────

#[derive(Default)]
struct Invocation {
    raw: Option<String>,
    alias_chain: Option<Value>,
    client_source: Option<fn() -> Option<String>>,
    client: Option<String>,
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

/// How to learn this client's id, asked the first time an op is signed.
/// Lazy, so a command that writes nothing reads nothing for it.
pub fn set_client_source(source: fn() -> Option<String>) {
    INVOCATION.with(|i| i.borrow_mut().client_source = Some(source));
}

fn client_id() -> String {
    INVOCATION.with(|i| {
        let mut i = i.borrow_mut();
        if i.client.is_none() {
            i.client = Some(i.client_source.and_then(|f| f()).unwrap_or_default());
        }
        i.client.clone().unwrap_or_default()
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
/// client and the file path it was minted at, and a file that opens under
/// another client, or at another path while the original is still there
/// (copied, restored beside it), mints its own rather than sign as the copy
/// it came from: two copies with one key would number their ops from one
/// seq. A file MOVED keeps its key, and so its membership. A copy put back
/// over the original's own path is the case this cannot see; `sync_apply`
/// refuses the fork it makes, and `datum sync chain rekey` mints a new key.
fn endpoint(conn: &Connection) -> Result<Endpoint> {
    let client = client_id();
    let path = conn.path().unwrap_or_default().to_string();
    let held: Option<(String, String, String)> = conn
        .prepare_cached("SELECT endpoint_secret, client_id, path FROM sync_local WHERE id = 1")?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    let held = held.filter(|(_, c, p)| {
        *c == client && (*p == path || p.is_empty() || !std::path::Path::new(p).exists())
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
        "INSERT INTO sync_local (id, endpoint_secret, endpoint_id, client_id, path)
         VALUES (1, ?1, ?2, ?3, ?4)
         ON CONFLICT (id) DO UPDATE SET endpoint_secret = excluded.endpoint_secret,
           endpoint_id = excluded.endpoint_id, client_id = excluded.client_id, path = excluded.path",
        params![hex(&key.to_bytes()), id, client, path],
    )?;
    Ok(Endpoint { key, id })
}

/// What decides whether this copy is a reader: its endpoint, and the count
/// and latest HLC of the chain records held.
type ReaderKey = (String, i64, Option<String>);

thread_local! {
    /// Whether this copy is a reader, by its [`ReaderKey`]. Every write
    /// asks, and a batch signs thousands of ops: the fold is dearer than
    /// the look.
    static READER: RefCell<Option<(ReaderKey, bool)>> = const { RefCell::new(None) };
}

/// Said when a reader's copy is asked to write (da-9yvb6).
const READ_ONLY: &str = "this copy is a reader in its sync chain: it receives, and can't write";

/// Whether this copy is a reader in its chain now.
fn is_reader(conn: &Connection, ep: &Endpoint) -> Result<bool> {
    let key: ReaderKey = conn
        .prepare_cached("SELECT ?1, COUNT(*), MAX(hlc) FROM ops WHERE hid = ?2 AND applied = 1")?
        .query_row(params![ep.id, CHAIN_HID], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    let cached = READER.with(|c| c.borrow().as_ref().filter(|(k, _)| *k == key).map(|r| r.1));
    if let Some(reader) = cached {
        return Ok(reader);
    }
    let chain = Chain::load(conn)?;
    let reader = chain
        .member(&ep.id)
        .is_some_and(|m| m.role() == Role::Reader);
    READER.with(|c| *c.borrow_mut() = Some((key, reader)));
    Ok(reader)
}

/// Said wherever two copies are found signing with one key (da-p79vq).
const SHARED_KEY: &str = "these are two copies of one database sharing an endpoint key; \
     run `datum sync chain rekey` on one of them";

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
const NOT_A_CONFLICT: &[&str] = &["modify_time", "content_hash", "verb", "slug", "client_id"];

// ─── The op ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct Op {
    endpoint: String,
    seq: i64,
    hlc: String,
    hid: String,
    slug: String,
    kind: String,
    /// The field its entry belongs to after it (da-u81u1); none on a chain
    /// record and on an op signed before filters.
    #[serde(default)]
    field: Option<String>,
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
        // An op signed before v48 names the client `device_id` (da-lfsei):
        // its body stays as signed, and its effect reads as today's.
        for k in ["create", "set"] {
            if let Some(Value::Object(m)) = op.effect.get_mut(k) {
                if let Some(id) = m.remove("device_id") {
                    m.insert("client_id".into(), id);
                }
            }
        }
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

/// Per endpoint, the highest seq held: applied, or refused — a refused op
/// counts as received, so the ops after it never wait on it (da-9yvb6: a
/// member made a reader and a member again writes on).
fn vector_where(conn: &Connection, hid: Option<&str>) -> Result<BTreeMap<String, i64>> {
    // Two statements, not `?1 IS NULL OR hid = ?1`: the OR keeps SQLite off
    // `ops_hid`, and every op signed would scan every op held.
    let pair = |r: &rusqlite::Row<'_>| Ok((r.get(0)?, r.get(1)?));
    let rows = match hid {
        Some(h) => conn
            .prepare_cached(
                "SELECT endpoint, MAX(seq) FROM ops WHERE hid = ?1 AND applied > 0 GROUP BY endpoint",
            )?
            .query_map(params![h], pair)?
            .collect::<rusqlite::Result<_>>(),
        None => conn
            .prepare_cached("SELECT endpoint, MAX(seq) FROM ops WHERE applied > 0 GROUP BY endpoint")?
            .query_map([], pair)?
            .collect::<rusqlite::Result<_>>(),
    };
    rows.map_err(DatumError::Db)
}

/// Sign and store a new op of this copy's; its seq.
fn push(
    conn: &Connection,
    ep: &Endpoint,
    (hid, slug, field): (&str, &str, Option<String>),
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
    if let Some(f) = &field {
        v["field"] = f.as_str().into();
    }
    let sig = hex(&ep.key.sign(&digest(&v)).to_bytes());
    v["sig"] = sig.clone().into();
    let op = Op {
        endpoint: ep.id.clone(),
        seq,
        hlc,
        hid: hid.into(),
        slug: slug.into(),
        kind: kind.into(),
        field,
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
    let field = belongs_to(&row(e));
    push(conn, ep, (e.hid.as_str(), &e.slug.0, field), kind, effect).map(drop)
}

/// The field an entry row belongs to: its own, or for a spec the field it
/// specifies — what a reader's filter is matched against (da-u81u1).
fn belongs_to(r: &Map<String, Value>) -> Option<String> {
    let field = r.get("field").and_then(Value::as_str)?;
    match field == super::SPEC_FIELD {
        true => r.get("data")?.get("field")?.as_str().map(Into::into),
        false => Some(field.into()),
    }
}

/// The field an entry belongs to after `ops` (fold order): the last op's
/// `field`, or, for an op signed before it had one, the fold's.
fn belongs_after(ops: &[Op]) -> Option<String> {
    match ops.last()?.field.clone() {
        Some(f) => Some(f),
        None => belongs_to(&fold(ops, None)?),
    }
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
    if is_reader(conn, &ep)? {
        return Err(DatumError::Other(READ_ONLY.into()));
    }
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
    push_entry(conn, &ep, &post, kind, effect)?;
    // An edit of a row merged into another moves the entry it shows, as the
    // op does on every other copy. An edit of the entry shown needs nothing:
    // its row was the merge, and is the merge with the edit.
    match &post.superseded_by {
        Some(s) => rederive(conn, s.as_str()),
        None => Ok(()),
    }
}

// ─── Apply ──────────────────────────────────────────────────────────────────

/// What one `sync pipe apply` did.
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
/// A kit's index is never a third: a kit may not declare one `unique`
/// (da-9lo90).
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

/// A row's content as [`diff`] names its keys, without its identity and the
/// keys [`NOT_A_CONFLICT`] names: what two rows merged into one compare.
fn content(r: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in r.iter().filter(|(_, v)| !v.is_null()) {
        match (k.as_str(), v) {
            ("hid" | "create_time", _) => {}
            (k, _) if NOT_A_CONFLICT.contains(&k) => {}
            ("data", Value::Object(d)) => {
                out.extend(d.iter().map(|(dk, v)| (format!("data.{dk}"), v.clone())))
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// An entry's own row: the fold of its ops, or the row as it stands when it
/// has none.
fn own(conn: &Connection, e: &Entry) -> Result<Entry> {
    let ops = ops_of(conn, e.hid.as_str())?;
    let base = ops
        .first()
        .is_none_or(|o| o.effect.get("create").is_none())
        .then_some(e);
    let mut out = match fold(&ops, base) {
        Some(r) => entry_from(r)?,
        None => e.clone(),
    };
    out.superseded_by = e.superseded_by.clone();
    Ok(out)
}

/// The rows merged into `s`: superseded by it, and holding or having held
/// a slug `s` holds or held, so the merge follows a retime of either
/// (da-yose2). A row superseded under another slug alone — one state at one
/// instant, which an id template declared two events — is not merged.
fn merged_into(conn: &Connection, s: &Entry) -> Result<Vec<Entry>> {
    conn.prepare_cached(
        "WITH held(slug) AS (SELECT ?2 UNION SELECT slug FROM ops WHERE hid = ?1 AND applied = 1)
         SELECT * FROM entries e WHERE superseded_by = ?1
           AND (slug IN (SELECT slug FROM held)
             OR EXISTS (SELECT 1 FROM ops o WHERE o.hid = e.hid AND o.applied = 1
                          AND o.slug IN (SELECT slug FROM held)))",
    )?
    .query_map(params![s.hid, s.slug], row_to_entry)?
    .collect::<rusqlite::Result<_>>()
    .map_err(DatumError::Db)
}

/// The row `s` shows with `members` merged into it (da-04glw): their
/// creates in creation order, each over the one before — so of a key two
/// have, the later-created row's value — then every edit of any of them in
/// fold order, so an edit addressed to either lands. Identity stays `s`'s.
/// It is derived from the ops alone, the same on every copy, and sent as
/// none.
fn merge_rows(conn: &Connection, s: &Entry, members: &[Entry]) -> Result<Entry> {
    if members.is_empty() {
        return Ok(s.clone());
    }
    let mut rows: Vec<&Entry> = std::iter::once(s).chain(members).collect();
    rows.sort_by(|a, b| (&a.create_time, &a.hid.0).cmp(&(&b.create_time, &b.hid.0)));
    // `s` is not always the first created (a `restore --merge` pair), so
    // its own create is the base — the bookkeeping — and every create's
    // content, its own included, goes over it in creation order.
    let (mut base, mut creates, mut edits) = (None, Vec::new(), Vec::new());
    for e in rows {
        let ops = ops_of(conn, e.hid.as_str())?;
        let n = ops
            .iter()
            .take_while(|o| o.effect.get("create").is_some())
            .count();
        creates.push(match n {
            0 => row(e),
            _ => fold(&ops[..n], None).unwrap_or_default(),
        });
        edits.extend(ops.into_iter().skip(n));
        if e.hid == s.hid {
            base = creates.last().cloned();
        }
    }
    let mut shown = base;
    for created in &creates {
        fold_into(&mut shown, &json!({ "set": content(created) }));
    }
    edits.sort_by(|a, b| a.order().cmp(&b.order()));
    for op in &edits {
        fold_into(&mut shown, &op.effect);
    }
    let mut out = entry_from(shown.unwrap_or_default())?;
    out.hid = s.hid.clone();
    out.slug = s.slug.clone();
    out.create_time = s.create_time;
    out.superseded_by = None;
    Ok(out)
}

/// Rewrite `hid`'s row to what it shows: its own fold when superseded, and
/// otherwise with what is merged into it.
fn rederive(conn: &Connection, hid: &str) -> Result<()> {
    let Some(pre) = fetch_by_hid(conn, hid)? else {
        return Ok(());
    };
    let own = own(conn, &pre)?;
    let next = match pre.superseded_by {
        Some(_) => own,
        None => merge_rows(conn, &own, &merged_into(conn, &own)?)?,
    };
    if row(&pre) == row(&next) {
        return Ok(());
    }
    history::record(conn, &pre, ChangeKind::SyncMerge)?;
    apply_entry_state(conn, &next)
}

/// File the `identity_collision` for `lost`, newly superseded by `kept`,
/// unless they are one event twice. A pair merged into one entry names, on
/// the superseded side's `extra_metadata.merge`, each key the two gave
/// different values: the value the merge took and the one it took it over.
fn file_collision(conn: &Connection, lost: &str, kept: &str) -> Result<()> {
    let (Some(l), Some(k)) = (fetch_by_hid(conn, lost)?, fetch_by_hid(conn, kept)?) else {
        return Ok(());
    };
    let (mut lost, kept_own) = (own(conn, &l)?, own(conn, &k)?);
    if same_event(&lost, &kept_own) {
        return Ok(());
    }
    if lost.slug == k.slug {
        let (a, b, shown) = (
            content(&row(&kept_own)),
            content(&row(&lost)),
            content(&row(&k)),
        );
        let mut clash = Map::new();
        for (key, x) in &a {
            let Some(y) = b.get(key).filter(|y| *y != x) else {
                continue;
            };
            let took = shown.get(key).cloned().unwrap_or(Value::Null);
            let over = if &took == y { x } else { y };
            clash.insert(key.clone(), json!({ "took": took, "over": over }));
        }
        if !clash.is_empty() {
            let mut meta = lost.extra_metadata.take().unwrap_or_else(|| json!({}));
            if let Some(m) = meta.as_object_mut() {
                m.insert("merge".into(), Value::Object(clash));
            }
            lost.extra_metadata = Some(meta);
        }
    }
    crate::integrity::file_identity_collision(conn, &lost, &kept_own).map(drop)
}

/// Write the row an entry's ops fold to. Two distinct entries claiming one
/// identity decide it from the rows alone, so every copy reaches the same
/// answer: the earlier-created keeps it and the other is superseded
/// (`superseded_by`), with an `identity_collision` naming the pair unless
/// they are one event twice. Under one slug the two are one entry
/// ([`merge_rows`]); under one live state-bearing instant alone, two.
fn write_merged(conn: &Connection, pre: Option<&Entry>, mut next: Entry) -> Result<()> {
    next.superseded_by = pre.and_then(|p| p.superseded_by.clone());
    let (mut pairs, mut demoted) = (Vec::new(), Vec::new());
    if next.superseded_by.is_none() {
        // What a row loses its identity with goes over to the row that
        // took it, so a pair's rows point at one survivor on every copy.
        let repoint = "UPDATE entries SET superseded_by = ?2 WHERE hid = ?1 OR superseded_by = ?1";
        for b in blockers(conn, &next)? {
            if (&next.create_time, &next.hid.0) < (&b.create_time, &b.hid.0) {
                history::record(conn, &b, ChangeKind::SyncMerge)?;
                conn.execute(repoint, params![b.hid, next.hid])?;
                pairs.push((b.hid.0.clone(), next.hid.0.clone()));
                demoted.push(b.hid.0);
            } else if next.superseded_by.is_none() {
                // A row older than its ops is signed as it stands before
                // the merge is written over it, so its own row stays known.
                if !has_ops(conn, &b.hid)? && !is_reader(conn, &endpoint(conn)?)? {
                    push_entry(conn, &endpoint(conn)?, &b, "baseline", create_effect(&b))?;
                }
                conn.execute(repoint, params![next.hid, b.hid])?;
                next.superseded_by = Some(b.hid.clone());
                pairs.push((next.hid.0.clone(), b.hid.0));
            }
        }
    }
    if next.superseded_by.is_none() {
        next = merge_rows(conn, &next, &merged_into(conn, &next)?)?;
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
            if row(pre) != row(&next) {
                history::record(conn, pre, ChangeKind::SyncMerge)?;
                apply_entry_state(conn, &next)?;
            }
        }
        None => {
            try_insert(conn, &next.slug, &next.hid, &next)?;
            if let Some(stored) = fetch_by_hid(conn, next.hid.as_str())? {
                history::record(conn, &stored, ChangeKind::SyncMerge)?;
            }
        }
    }
    // A demoted survivor goes back to its own row; the survivor of a row
    // merged into it takes in what changed.
    for hid in demoted
        .iter()
        .chain(next.superseded_by.as_ref().map(|h| &h.0))
    {
        rederive(conn, hid)?;
    }
    for (lost, kept) in pairs {
        file_collision(conn, &lost, &kept)?;
    }
    Ok(())
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

/// Apply every held op that is ready, records first, until none is; an
/// entry op the chain does not allow is refused when it is ready, so the
/// refused are as contiguous as the applied, and what waits for an op
/// before it stays held.
fn settle(conn: &Connection, s: &mut ApplySummary) -> Result<()> {
    let mut chain = Chain::load(conn)?;
    let mut vector = vector_where(conn, None)?;
    // A filtered reader is never sent what its filter leaves out, so it
    // waits on no gap and no dep — the fold reads only the set of an
    // entry's ops, so order is no matter — and what it holds is ready;
    // `settle_filter` refuses what falls outside its filter (da-u81u1).
    let filtered = Chain::load_with_waiting(conn)?
        .filter(&endpoint(conn)?.id)
        .is_some();
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
            // At or below `have`: a refused op the membership now allows.
            let ready = filtered
                || op.seq <= have(&op.endpoint) + 1 && op.deps.iter().all(|(e, n)| have(e) >= *n);
            if !ready {
                continue;
            }
            progressed = true;
            if !op.is_chain() {
                match chain.allows(&op.endpoint, &op.hlc) {
                    Ok(()) => {
                        s.conflicts += apply_one(conn, op)?;
                        s.applied += 1;
                    }
                    Err(why) => refuse(conn, op, &why, s)?,
                }
                let head = vector.entry(op.endpoint.clone()).or_default();
                *head = (*head).max(op.seq);
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
    s.held = waiting.len();
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
/// `sync_unauthorised` warning, which `datum sync chain ack` settles.
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
    // A revoked endpoint's applied ops from its revoke on, and a reader's
    // from when it became one: its own, by key.
    let mut hids = BTreeSet::new();
    for m in &chain.members {
        let Some(from) = m.refused_from() else {
            continue;
        };
        let sql = "SELECT body FROM ops
                   WHERE endpoint = ?1 AND applied = 1 AND hlc >= ?2 AND hid != ?3";
        for op in bodies(sql, &[&m.endpoint, &from, &CHAIN_HID])? {
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

// ─── Filtered readers (da-u81u1) ────────────────────────────────────────────

/// The ops of `hid` held, applied or refused, in fold order, each with
/// whether it is refused: what a sender and a filtered reader both judge an
/// entry's field by.
fn held_ops(conn: &Connection, hid: &str) -> Result<Vec<(Op, bool)>> {
    let mut stmt =
        conn.prepare_cached("SELECT body, applied FROM ops WHERE hid = ?1 AND applied > 0")?;
    let rows = stmt
        .query_map(params![hid], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut ops = rows
        .iter()
        .map(|(b, applied)| Ok((Op::parse(b)?, *applied == 2)))
        .collect::<Result<Vec<_>>>()?;
    ops.sort_by(|a, b| a.0.order().cmp(&b.0.order()));
    Ok(ops)
}

/// What a filtered reader is sent after `since`, and how many ops that is:
/// the chain's records, then per entry with new ops one line of them — all
/// of its ops if it moved into the filter since — or a removal if it moved
/// out, and last the mark — always, with nothing else too: it is what moves
/// the reader past ops outside its filter. `only`: this copy is a reader,
/// and serves its own ops alone.
fn filtered_lines(
    conn: &Connection,
    since: &BTreeMap<String, i64>,
    filter: &Filter,
    chain: &Chain,
    only: Option<&str>,
) -> Result<(Vec<String>, usize)> {
    let after = |e: &str| since.get(e).copied().unwrap_or(0);
    // As `sync_ops_since` serves: a non-member's refused op stays here.
    let served = |e: &str, refused: bool| {
        only.is_none_or(|m| m == e) && (!refused || chain.member(e).is_some())
    };
    let mut lines = Vec::new();
    let mut touched = BTreeSet::new();
    let mut stmt = conn.prepare(
        "SELECT endpoint, seq, hid, applied, CASE WHEN hid = ?1 THEN body END
         FROM ops WHERE applied > 0 ORDER BY endpoint, seq",
    )?;
    let rows = stmt.query_map(params![CHAIN_HID], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)? == 2,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    for row in rows {
        let (e, seq, hid, refused, body) = row?;
        if seq <= after(&e) || !served(&e, refused) {
            continue;
        }
        // Chain records go to every reader, and first: its filter may be
        // among them.
        match body {
            Some(b) => lines.push(b),
            None => {
                touched.insert(hid);
            }
        }
    }
    drop(stmt);
    let mut n = lines.len();
    for hid in touched {
        let ops: Vec<Op> = held_ops(conn, &hid)?
            .into_iter()
            .filter(|(o, refused)| served(&o.endpoint, *refused))
            .map(|(o, _)| o)
            .collect();
        let then: Vec<Op> = ops
            .iter()
            .filter(|o| o.seq <= after(&o.endpoint))
            .cloned()
            .collect();
        let now_in = filter.admits(belongs_after(&ops).as_deref());
        let then_in = !then.is_empty() && filter.admits(belongs_after(&then).as_deref());
        let send: Vec<&str> = match (now_in, then_in) {
            (true, true) => ops
                .iter()
                .filter(|o| o.seq > after(&o.endpoint))
                .map(|o| o.body.as_str())
                .collect(),
            (true, false) => ops.iter().map(|o| o.body.as_str()).collect(),
            (false, true) => {
                lines.push(json!({ "remove": hid }).to_string());
                continue;
            }
            (false, false) => continue,
        };
        n += send.len();
        lines.push(format!("[{}]", send.join(",")));
    }
    let caught_up = match only {
        Some(_) => BTreeMap::from([own_head(conn)?]),
        None => held_vector(conn)?,
    };
    let mark = json!({ "caught_up": caught_up, "fields": filter.fields, "since": since });
    lines.push(mark.to_string());
    Ok((lines, n))
}

/// A filtered reader's vector: per endpoint, the highest mark made under
/// fields covering `filter`, and its own head.
fn caught_up(
    conn: &Connection,
    filter: &Filter,
    (mine, signed): (&str, i64),
) -> Result<BTreeMap<String, i64>> {
    let mut v = BTreeMap::new();
    let mut stmt = conn.prepare_cached("SELECT endpoint, fields, seq FROM sync_caught_up")?;
    let marks = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?))
    })?;
    for mark in marks {
        let (e, fields, seq): (String, String, i64) = mark?;
        let fields: BTreeSet<String> = serde_json::from_str(&fields).unwrap_or_default();
        if fields.is_superset(&filter.fields) {
            let head = v.entry(e).or_insert(seq);
            *head = seq.max(*head);
        }
    }
    if signed > 0 {
        v.insert(mine.to_string(), signed);
    }
    Ok(v)
}

/// [`Db::sync_vector`]: what this copy holds, as a vector it can stand by.
fn held_vector(conn: &Connection) -> Result<BTreeMap<String, i64>> {
    let chain = Chain::load(conn)?;
    if !chain.exists() {
        return vector_where(conn, None);
    }
    let (mine, signed) = own_head(conn)?;
    if let Some(filter) = chain.filter(&mine) {
        return caught_up(conn, filter, (&mine, signed));
    }
    match marked(conn)? {
        true => contiguous(conn),
        false => vector_where(conn, None),
    }
}

fn marked(conn: &Connection) -> Result<bool> {
    Ok(conn
        .prepare_cached("SELECT 1 FROM sync_caught_up LIMIT 1")?
        .exists([])?)
}

/// Per endpoint, the highest seq held with every seq below it held too.
fn contiguous(conn: &Connection) -> Result<BTreeMap<String, i64>> {
    Ok(conn
        .prepare(
            "SELECT o.endpoint, MIN(o.seq) FROM (
                 SELECT endpoint, seq FROM ops WHERE applied > 0
                 UNION ALL SELECT DISTINCT endpoint, 0 FROM ops WHERE applied > 0
             ) o
             WHERE NOT EXISTS (SELECT 1 FROM ops p WHERE p.endpoint = o.endpoint
                                 AND p.seq = o.seq + 1 AND p.applied > 0)
             GROUP BY o.endpoint HAVING MIN(o.seq) > 0",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Remove an entry from a filtered reader's copy: its row, its ops and its
/// history. The reader's own software does it; nothing is synced.
fn drop_entry(conn: &Connection, hid: &str) -> Result<()> {
    conn.execute("DELETE FROM ops WHERE hid = ?1", params![hid])?;
    conn.execute("DELETE FROM entries WHERE hid = ?1", params![hid])?;
    conn.execute(
        "DELETE FROM entry_history WHERE entry_hid = ?1",
        params![hid],
    )?;
    Ok(())
}

/// After a filtered reader applies: drop what the call brought that falls
/// outside its filter — every entry received, when the filter itself moved
/// (`before`) — act on the sender's removals, and keep its marks.
fn settle_filter(
    conn: &Connection,
    mine: &str,
    before: Option<&Filter>,
    got: &[(String, i64, String)],
    words: &[Value],
    s: &mut ApplySummary,
) -> Result<()> {
    let chain = Chain::load(conn)?;
    let Some(filter) = chain.filter(mine) else {
        // Unfiltered now, with its gaps filled: the marks have nothing
        // left to say. Here, on a write, not in a read of the vector. The
        // whole vector, not per endpoint: while any marks stand the vector
        // is `contiguous` for every endpoint, so they go when that and the
        // highest held agree everywhere, and meanwhile cost only the
        // slower, still true, vector.
        if marked(conn)? && contiguous(conn)? == vector_where(conn, None)? {
            conn.execute("DELETE FROM sync_caught_up", [])?;
        }
        return Ok(());
    };
    let hids: BTreeSet<String> = match before == Some(filter) {
        true => got.iter().map(|g| g.2.clone()).collect(),
        false => conn
            .prepare("SELECT DISTINCT hid FROM ops WHERE endpoint != ?1 AND hid != ?2")?
            .query_map(params![mine, CHAIN_HID], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
    };
    for hid in hids.iter().filter(|h| *h != CHAIN_HID) {
        let ops: Vec<Op> = held_ops(conn, hid)?.into_iter().map(|o| o.0).collect();
        if filter.admits(belongs_after(&ops).as_deref()) {
            continue;
        }
        for (e, seq, _) in got.iter().filter(|g| g.2 == *hid) {
            s.refused
                .push(format!("{e}/{seq}: outside this reader's filter"));
        }
        drop_entry(conn, hid)?;
    }
    // A mark answers the vector the sender was given: it holds only if this
    // copy is caught up that far under its filter now. A filter widened
    // since — by a record in this very stream — lowered that vector, and
    // the next round asks again (da-u81u1).
    let signed = own_head(conn)?.1;
    let now = caught_up(conn, filter, (mine, signed))?;
    for w in words {
        if let Some(hid) = w["remove"].as_str() {
            drop_entry(conn, hid)?;
        }
        let answered = w["since"]
            .as_object()
            .into_iter()
            .flatten()
            .all(|(e, n)| n.as_i64().unwrap_or(0) <= now.get(e).copied().unwrap_or(0));
        if !answered {
            continue;
        }
        let fields = w["fields"].to_string();
        for (e, seq) in w["caught_up"].as_object().into_iter().flatten() {
            conn.execute(
                "INSERT INTO sync_caught_up (endpoint, fields, seq) VALUES (?1, ?2, ?3)
                 ON CONFLICT (endpoint, fields) DO UPDATE SET seq = MAX(seq, excluded.seq)",
                params![e, fields, seq.as_i64().unwrap_or(0)],
            )?;
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
        // Blanked, not deleted: the pause is this copy's, whatever its key.
        self.conn
            .execute("UPDATE sync_local SET endpoint_secret = ''", [])?;
        Ok(endpoint(&self.conn)?.id)
    }

    /// This copy's version vector: per endpoint, the highest seq applied.
    /// Ops apply only in seq order, so that seq is also the highest
    /// contiguous one. A filtered reader's holds gaps, so its vector is its
    /// marks instead (da-u81u1): per endpoint, the highest a sender marked
    /// under fields covering its filter now, and its own head. One whose
    /// filter is lifted still holds its gaps until they are sent: its vector
    /// is the highest contiguous seq, and its marks go once it has none.
    pub fn sync_vector(&self) -> Result<BTreeMap<String, i64>> {
        held_vector(&self.conn)
    }

    /// Every applied op `since` lacks, as canonical JSON lines in
    /// `(endpoint, seq)` order. First signs a `baseline` create for every
    /// entry no op covers — one older than its ops, or bulk-imported — so a
    /// copy syncing from empty receives every entry. With `overlap`, this
    /// copy's own op AT `since`'s seq for it comes too — the one line that
    /// shows a copy sharing this key, whose vector agrees with this one's,
    /// a different op there (the pipe; iroh refuses such a peer by id).
    /// For `peer`, a filtered reader, only what its filter lets through
    /// ([`filtered_lines`], da-u81u1).
    pub fn sync_ops_since(
        &self,
        since: &BTreeMap<String, i64>,
        overlap: bool,
        peer: Option<&str>,
    ) -> Result<Vec<String>> {
        pause::refuse_paused(&self.conn)?;
        let tx = self.conn.unchecked_transaction()?;
        // A vector holding more of this copy's ops than it signed was made
        // by another copy signing with this key.
        let (mine, signed) = own_head(&tx)?;
        if since.get(&mine).is_some_and(|s| *s > signed) {
            return Err(DatumError::Other(SHARED_KEY.into()));
        }
        // A reader never relays: it serves its own ops alone, and signs no
        // baselines, which its chain would refuse.
        let reader = is_reader(&tx, &endpoint(&tx)?)?;
        let chain = Chain::load(&tx)?;
        let uncovered: Vec<Entry> = tx
            .prepare("SELECT * FROM entries WHERE hid NOT IN (SELECT hid FROM ops) ORDER BY hid")?
            .query_map([], row_to_entry)?
            .collect::<rusqlite::Result<_>>()?;
        if !uncovered.is_empty() && !reader {
            let ep = endpoint(&tx)?;
            for e in &uncovered {
                push_entry(&tx, &ep, e, "baseline", create_effect(e))?;
            }
        }
        if let Some(filter) = peer.and_then(|p| chain.filter(p)) {
            let (lines, _) = filtered_lines(&tx, since, filter, &chain, reader.then_some(&mine))?;
            tx.commit()?;
            return Ok(lines);
        }
        let mut stmt = tx.prepare(
            "SELECT endpoint, seq, body, applied FROM ops WHERE applied > 0 ORDER BY endpoint, seq",
        )?;
        let lines = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .filter_map(|r| match r {
                Ok((e, ..)) if reader && e != mine => None,
                // A member's refused op goes too, for every copy to refuse
                // itself: the ops after it are counted past it. A
                // non-member's stays here.
                Ok((e, _, _, 2)) if chain.member(&e).is_none() => None,
                Ok((e, s, body, _))
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
    /// op. A line may also be one entry's ops as an array, or a filtered
    /// sender's removal or mark, which only a filtered reader acts on.
    pub fn sync_apply(&self, lines: &str) -> Result<ApplySummary> {
        pause::refuse_paused(&self.conn)?;
        let tx = self.conn.unchecked_transaction()?;
        let (mine, signed) = own_head(&tx)?;
        let before = Chain::load(&tx)?.filter(&mine).cloned();
        let mut s = ApplySummary::default();
        let (mut got, mut words) = (Vec::new(), Vec::new());
        let mut each = Vec::new();
        for line in lines.lines().map(str::trim).filter(|l| !l.is_empty()) {
            // Canonical JSON: an op's first key is `alias_chain`.
            if line.starts_with("{\"caught_up\"") || line.starts_with("{\"remove\"") {
                match serde_json::from_str::<Value>(line) {
                    Ok(w) => words.push(w),
                    Err(e) => s.refused.push(format!("unreadable line: {e}")),
                }
                continue;
            }
            match line.starts_with('[') {
                true => match serde_json::from_str::<Vec<Value>>(line) {
                    Ok(ops) => each.extend(ops.iter().map(Value::to_string)),
                    Err(e) => s.refused.push(format!("unreadable ops: {e}")),
                },
                false => each.push(line.to_string()),
            }
        }
        for line in each.iter().map(String::as_str) {
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
                None if !forked => {
                    store(&tx, &op, false)?;
                    got.push((op.endpoint, op.seq, op.hid));
                }
                Some(body) if body == op.body => s.already_had += 1,
                _ => s
                    .refused
                    .push(format!("{}/{}: {SHARED_KEY}", op.endpoint, op.seq)),
            }
        }
        settle(&tx, &mut s)?;
        settle_filter(&tx, &mine, before.as_ref(), &got, &words, &mut s)?;
        tx.commit()?;
        Ok(s)
    }

    /// The state hash: SHA-256 over the canonical JSON of every entry row,
    /// live and tombstoned, in `(slug, hid)` order, each with its
    /// `superseded_by` — so two copies agree only if they hold exactly the
    /// same data, bookkeeping included. The map is the same hash per column,
    /// to narrow a mismatch down. With a filtered reader on either side —
    /// this copy, or `peer` — only the entries that pass both filters count
    /// (da-u81u1), which both sides compute alike.
    pub fn sync_state_hash(
        &self,
        peer: Option<&str>,
    ) -> Result<(String, BTreeMap<String, String>)> {
        let chain = Chain::load(&self.conn)?;
        let mine = match chain.exists() {
            true => Some(endpoint(&self.conn)?.id),
            false => None,
        };
        let filters: Vec<&Filter> = [mine.as_deref(), peer]
            .into_iter()
            .flatten()
            .filter_map(|e| chain.filter(e))
            .collect();
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
            .filter(|r| {
                r.as_ref().map_or(true, |r| {
                    let belongs = belongs_to(r);
                    filters.iter().all(|f| f.admits(belongs.as_deref()))
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

    /// An op signed before v48 folds as today's (da-lfsei): its effect's
    /// `device_id` reads as `client_id`, and its body, which is signed,
    /// is kept as it came.
    #[test]
    fn an_op_from_before_v48_folds_its_device_id_as_the_client_id() {
        let old = |effect: Value| {
            json!({"endpoint": "e", "seq": 1, "hlc": "h", "hid": "x", "slug": "s",
                   "kind": "entry", "effect": effect, "sig": "g"})
            .to_string()
        };
        let create = Op::parse(&old(json!({"create": {"device_id": "d1"}}))).unwrap();
        let set = Op::parse(&old(json!({"set": {"device_id": "d2"}}))).unwrap();
        assert!(create.body.contains("device_id"));
        let mut row = None;
        fold_into(&mut row, &create.effect);
        assert_eq!(row.as_ref().unwrap()["client_id"], "d1");
        fold_into(&mut row, &set.effect);
        assert_eq!(
            row,
            Some(json!({"client_id": "d2"}).as_object().unwrap().clone())
        );
    }

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
