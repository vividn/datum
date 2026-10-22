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
//!   changed (`effect`), what this copy had seen of the entry — of every
//!   entry, for a create (`deps`) — and
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
//!   later stays superseded, the record of what was merged in. A backup's
//!   entry that `db restore --merge` finds a live one in the place of is
//!   such a second entry ([`restore`], da-4ldvb). The spec a field's first
//!   log wrote declares nothing ([`automatic`], da-m82n0): a kit's or a
//!   person's goes over it whichever is later, and no key is contested
//!   between them.
//! - **Conflict.** Two ops are concurrent when neither's `deps` covers the
//!   other. Of the ops that set a key, those no later one setting it saw
//!   stand; when they give it different values, the entry's `conflicts`
//!   column names each, written by the fold from the ops alone (da-05rkv),
//!   so every copy holding them agrees. A delete stands against an edit
//!   made without seeing it, and both show. A resolution is an ordinary
//!   edit that saw every side ([`Db::resolve_conflicts`]).
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
//! - **Purge orders (da-qzrj9).** A chain's `wipe` record takes every entry
//!   op made before it, on each copy as the copy comes to hold it ([`wipe`]):
//!   the op's row stays as a stub, `applied` = 3 and nothing but its
//!   endpoint, seq and HLC, so seqs stay gapless and none is signed twice.
//!   One endpoint's ops are in HLC order by seq, so what an order took of
//!   it is a run of its seqs, its chain records aside; a sender says which
//!   in one `{"purged": {endpoint: [[from, to], …]}}` line after the
//!   order, and a copy holding an order fills its gaps with them, which a
//!   copy behind would otherwise wait on forever.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::entry::{new_hid, Entry};
use crate::error::{DatumError, Result};
use crate::field_spec::FieldSpec;
use crate::history::{self, ChangeKind};
use crate::ids::{Hid, Slug};
use crate::migrations::{peek_schema_version, CURRENT_SCHEMA_VERSION};

use super::insert::{apply_entry_state, try_insert};
use super::row_mapper::row_to_entry;
use super::spec::{SPEC_FIELD, SPEC_KIT_KEY};
use super::{Db, WriteBatch};

pub mod chain;
mod debug;
mod diagnose;
mod pause;
pub mod peers;
mod rejoin;
pub mod status;
mod versions;
use chain::{Chain, Filter, Role, CHAIN_HID};
pub use debug::DebugConflicts;
pub use diagnose::{row_diff, FoldCheck, Named, Orphan, Stale};
pub use versions::Version;

// ─── The invocation ─────────────────────────────────────────────────────────

#[derive(Default)]
struct Invocation {
    raw: Option<String>,
    alias_chain: Option<Value>,
    client_source: Option<fn() -> Option<String>>,
    client: Option<String>,
    origin: Option<Value>,
}

thread_local! {
    /// Each parse sets `raw`; the client and `origin` outlive a REPL's lines.
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

/// Who and what signs the ops from here (da-zjc9w), carried in each as
/// `origin`: the OS `user` and `host`, the `installation` (this client's id),
/// the `program` — `datum`, or `datum.day` for the browser's CLI — its
/// `version`, and the `script` `DATUM_SCRIPT` names. A part this process
/// cannot know, as a browser cannot its user or host, is left out rather
/// than invented. Read once, at the first op.
fn origin() -> Value {
    if let Some(o) = INVOCATION.with(|i| i.borrow().origin.clone()) {
        return o;
    }
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let program = match cfg!(target_os = "wasi") {
        true => "datum.day",
        false => "datum",
    };
    let parts = [
        ("user", env("USER").or_else(|| env("USERNAME"))),
        ("host", host()),
        ("installation", Some(client_id()).filter(|c| !c.is_empty())),
        ("program", Some(program.into())),
        ("version", Some(crate::build().into())),
        ("script", env("DATUM_SCRIPT")),
    ];
    let o: Map<String, Value> = parts
        .into_iter()
        .filter_map(|(k, v)| Some((k.to_string(), Value::String(v?))))
        .collect();
    let o = Value::Object(o);
    INVOCATION.with(|i| i.borrow_mut().origin = Some(o.clone()));
    o
}

#[cfg(unix)]
fn host() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: `buf` is writable for its whole length, which is what is passed.
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
    let end = buf.iter().position(|&b| b == 0)?;
    (ok && end > 0).then(|| String::from_utf8_lossy(&buf[..end]).into_owned())
}

#[cfg(not(unix))]
fn host() -> Option<String> {
    std::env::var("COMPUTERNAME").ok().filter(|v| !v.is_empty())
}

// ─── Canonical JSON, hex ────────────────────────────────────────────────────

/// Object keys sorted at every depth, no whitespace: the one serialization
/// that is signed and hashed, so every copy computes the same bytes.
pub fn canonical_json(v: &Value) -> String {
    /// `v` written with its keys sorted, read in place: a signed batch
    /// writes thousands, and a sorted copy of each was most of the cost.
    struct Sorted<'a>(&'a Value);
    impl Serialize for Sorted<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
            match self.0 {
                Value::Object(m) => {
                    let mut pairs: Vec<_> = m.iter().collect();
                    pairs.sort_by(|a, b| a.0.cmp(b.0));
                    s.collect_map(pairs.into_iter().map(|(k, v)| (k, Sorted(v))))
                }
                Value::Array(a) => s.collect_seq(a.iter().map(Sorted)),
                v => v.serialize(s),
            }
        }
    }
    serde_json::to_string(&Sorted(v)).expect("a Value serializes")
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
    /// Kept across a REPL's lines: a rekey's new secret misses it (da-3f9nl).
    static ENDPOINT: RefCell<Option<(String, Endpoint)>> = const { RefCell::new(None) };
    /// The file a sync session on this thread works on, and the endpoint id
    /// it began as ([`Db::sync_session_as`]).
    static SESSION: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// Said to a session whose copy became another under it (da-otz0d).
const KEY_CHANGED: &str =
    "this copy's endpoint key changed (purged, rekeyed, or it left its chain): stopped";

/// This copy's endpoint, minted on first need. The key is stored with the
/// client and the file path it was minted at, and a file that opens under
/// another client, or at another path while the original is still there
/// (copied, restored beside it), mints its own rather than sign as the copy
/// it came from: two copies with one key would number their ops from one
/// seq. A file MOVED keeps its key, and so its membership. A copy put back
/// over the original's own path is the case this cannot see; `sync_apply`
/// refuses the fork it makes, and `datum sync chain rekey` mints a new key.
///
/// A session's file is the endpoint the session began as, or this fails and
/// mints nothing (da-otz0d): a `db purge` under an open session made the file
/// a new copy in no chain, which would take the peer's whole chain as one
/// joining it does. What a session writes asks here who this copy is, inside
/// the transaction it writes in.
///
/// And its file is at the schema this build writes, or this fails naming both
/// versions (da-1tnup): a newer datum migrated it under the session, whose
/// rows would be the old shape.
fn endpoint(conn: &Connection) -> Result<Endpoint> {
    let client = client_id();
    let path = conn.path().unwrap_or_default().to_string();
    let began = SESSION.with(|s| {
        let s = s.borrow();
        s.as_ref()
            .filter(|(p, _)| *p == path)
            .map(|(_, id)| id.clone())
    });
    if began.is_some() {
        let file = peek_schema_version(conn)?;
        if file != CURRENT_SCHEMA_VERSION {
            return Err(DatumError::IncompatibleSchema(format!(
                "this database is at schema v{file} and this datum writes \
                 v{CURRENT_SCHEMA_VERSION}: another datum migrated it under this sync, which stopped"
            )));
        }
    }
    let still = |ep: Endpoint| match &began {
        Some(id) if *id != ep.id => Err(DatumError::Other(KEY_CHANGED.into())),
        _ => Ok(ep),
    };
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
            return still(ep);
        }
        if let Some(bytes) = unhex::<32>(&secret) {
            let key = SigningKey::from_bytes(&bytes);
            let ep = Endpoint {
                id: hex(key.verifying_key().as_bytes()),
                key,
            };
            ENDPOINT.with(|c| *c.borrow_mut() = Some((secret, ep.clone())));
            return still(ep);
        }
    }
    let key = SigningKey::generate(&mut rand::rngs::OsRng);
    let id = hex(key.verifying_key().as_bytes());
    // A session's file holding no key was purged or rekeyed under it: the
    // new key is no session's, and is not stored.
    let ep = still(Endpoint { key, id })?;
    conn.execute(
        "INSERT INTO sync_local (id, endpoint_secret, endpoint_id, client_id, path)
         VALUES (1, ?1, ?2, ?3, ?4)
         ON CONFLICT (id) DO UPDATE SET endpoint_secret = excluded.endpoint_secret,
           endpoint_id = excluded.endpoint_id, client_id = excluded.client_id, path = excluded.path",
        params![hex(&ep.key.to_bytes()), ep.id, client, path],
    )?;
    Ok(ep)
}

/// What decides whether this copy writes: its endpoint, and the count
/// and latest HLC of the chain records held.
type ReaderKey = (String, i64, Option<String>);

thread_local! {
    /// Why this copy writes nothing, by its [`ReaderKey`]. Every write
    /// asks, and a batch signs thousands of ops: the fold is dearer than
    /// the look. Kept across a REPL's lines: one changing the key or the
    /// chain's records misses it (da-3f9nl).
    static READER: RefCell<Option<(ReaderKey, Option<&'static str>)>> = const { RefCell::new(None) };
}

/// Said when a reader's copy is asked to write (da-9yvb6).
const READ_ONLY: &str = "this copy is a reader in its sync chain: it receives, and can't write";

/// Said when a copy that signed its departure is asked to write (da-dsob1):
/// every member holding the departure would refuse the op.
const LEAVING: &str = "this copy signed its departure from its sync chain and writes nothing \
     more: `datum sync chain leave` finishes once a member holds it, or `--force` leaves now";

/// Why this copy writes nothing now, if it does not: a reader in its
/// chain, or a member that signed its departure. Either serves its own ops
/// alone, and signs no baseline.
fn read_only(conn: &Connection, ep: &Endpoint) -> Result<Option<&'static str>> {
    let key: ReaderKey = conn
        .prepare_cached("SELECT ?1, COUNT(*), MAX(hlc) FROM ops WHERE hid = ?2 AND applied = 1")?
        .query_row(params![ep.id, CHAIN_HID], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    let cached = READER.with(|c| c.borrow().as_ref().filter(|(k, _)| *k == key).map(|r| r.1));
    if let Some(why) = cached {
        return Ok(why);
    }
    let chain = Chain::load(conn)?;
    let why = chain.member(&ep.id).and_then(|m| match () {
        _ if m.left.is_some() => Some(LEAVING),
        _ if m.role() == Role::Reader => Some(READ_ONLY),
        _ => None,
    });
    READER.with(|c| *c.borrow_mut() = Some((key, why)));
    Ok(why)
}

/// Said wherever two copies are found signing with one key (da-p79vq).
const SHARED_KEY: &str = "these are two copies of one database sharing an endpoint key; \
     run `datum sync chain rekey` on one of them";

/// This copy's endpoint id, and the highest seq it has signed: held, or
/// signed before a `db purge --rejoin` and not back yet.
fn own_head(conn: &Connection) -> Result<(String, i64)> {
    let id = endpoint(conn)?.id;
    let seq: i64 = conn
        .prepare_cached("SELECT COALESCE(MAX(seq), 0) FROM ops WHERE endpoint = ?1")?
        .query_row(params![id], |r| r.get(0))?;
    let before = rejoin::signed_before(conn, &id)?;
    Ok((id, seq.max(before)))
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

/// How far a received op's HLC may run ahead of this copy's clock (da-ns8dq).
/// One further ahead is held until this clock catches up: applied, it
/// would pin every op this copy signs to the fast device's time for good,
/// as signed ops are never re-stamped. Held, never clamped or refused — a
/// decision made from the local clock may only delay, never decide, or
/// copies judging by their own clocks would fold apart.
const MAX_CLOCK_AHEAD: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// How long one transaction of [`Db::sync_apply`] holds the write lock
/// (da-2ejbs): one for a whole batch held it long enough that a write
/// beside a large apply waited out `busy_timeout` and was lost. Bounded by
/// the clock, not a count of ops, since an op costs more the larger the
/// database it lands in, and never under [`APPLY_MIN`] ops: a chunk's own
/// cost does not shrink with it, and once that passed the hold, chunks of
/// one op each paid it per op (a join on a loaded runner, 22x slower).
/// Once chunks have held it [`APPLY_HOLD`] with no pause, of one call or
/// the back-to-back calls a session makes, the lock is let go for longer
/// than SQLite's longest busy sleep (100 ms), so a waiting writer gets in.
/// A pause per call instead cost a native session of 64-op steps 7x.
const APPLY_HOLD: Duration = Duration::from_millis(500);
const APPLY_MIN: usize = 32;
const APPLY_YIELD: Duration = Duration::from_millis(110);

#[cfg(test)]
thread_local! {
    /// How far the tests' clock runs from the real one, in ms.
    static SKEW: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// This copy's wall clock, in ms.
fn now_ms() -> i64 {
    let now = Utc::now().timestamp_millis();
    #[cfg(test)]
    let now = now + SKEW.with(std::cell::Cell::get);
    now
}

/// How far `hlc` runs ahead of `now`, when it is past [`MAX_CLOCK_AHEAD`].
fn clock_ahead(hlc: &str, now: i64) -> Option<i64> {
    parse_hlc(hlc)
        .map(|(ms, _)| ms - now)
        .filter(|ahead| *ahead > MAX_CLOCK_AHEAD.as_millis() as i64)
}

/// The next HLC: past the wall clock and past every op this copy has
/// applied, its own and received alike — not one held or refused, which
/// would set this clock by an op it never folded (da-ns8dq).
fn next_hlc(conn: &Connection) -> Result<String> {
    let last: Option<String> = conn
        .prepare_cached("SELECT MAX(hlc) FROM ops WHERE applied = 1")?
        .query_row([], |r| r.get(0))?;
    let (ms, counter) = advance(last.as_deref().and_then(parse_hlc), now_ms());
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
    #[serde(default)]
    alias_chain: Option<Value>,
    /// Who and what signed it ([`origin`]); none on an op signed before.
    #[serde(default)]
    origin: Option<Value>,
    effect: Value,
    #[serde(default)]
    deps: BTreeMap<String, i64>,
    /// The HLC a spooled op was signed with on its snapshot, when its
    /// command ran (da-m6c4j): signed again as it drains, it is still that
    /// command's, and a wipe ordered between the two takes it.
    #[serde(default)]
    queued: Option<String>,
    sig: String,
    #[serde(skip)]
    body: String,
}

impl Op {
    fn parse(body: &str) -> Result<Op> {
        let v: Value = serde_json::from_str(body)?;
        let mut op = Op::deserialize(&v)?;
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

    /// Had the copy that made this op seen `other`? A `restore` is a
    /// backup's entry, signed by the copy it is restored on ([`restore`]):
    /// it saw what its `deps` name, and none of that copy's ops since.
    fn saw(&self, other: &Op) -> bool {
        self.kind != "restore" && self.endpoint == other.endpoint && self.seq > other.seq
            || self
                .deps
                .get(&other.endpoint)
                .is_some_and(|s| *s >= other.seq)
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
        // An entry's ops and those of the rows it supersedes: an edit of the
        // entry shown saw them all, and settles their conflicts too.
        Some(h) => conn
            .prepare_cached(
                "SELECT endpoint, MAX(seq) FROM ops
                 WHERE hid IN (SELECT ?1 UNION SELECT hid FROM entries WHERE superseded_by = ?1)
                   AND applied > 0
                 GROUP BY endpoint",
            )?
            .query_map(params![h], pair)?
            .collect::<rusqlite::Result<_>>(),
        // A skip scan of the primary key, endpoint to endpoint, each read
        // back from its newest op to its newest applied one: a GROUP BY
        // read every op's row for `applied`, the whole table, on every
        // apply and every push (da-89ujs).
        None => conn
            .prepare_cached(
                "WITH RECURSIVE e(endpoint) AS (
                     SELECT MIN(endpoint) FROM ops
                     UNION ALL
                     SELECT (SELECT MIN(endpoint) FROM ops WHERE endpoint > e.endpoint)
                     FROM e WHERE e.endpoint IS NOT NULL
                 )
                 SELECT endpoint, head FROM (
                     SELECT endpoint, (SELECT seq FROM ops o
                                       WHERE o.endpoint = e.endpoint AND o.applied > 0
                                       ORDER BY seq DESC LIMIT 1) AS head
                     FROM e WHERE endpoint IS NOT NULL
                 ) WHERE head IS NOT NULL",
            )?
            .query_map([], pair)?
            .collect::<rusqlite::Result<_>>(),
    };
    rows.map_err(DatumError::Db)
}

/// Sign and store a new op of this copy's; its seq.
fn push(
    conn: &Connection,
    ep: &Endpoint,
    head: (&str, &str, Option<String>),
    kind: &str,
    effect: Value,
) -> Result<i64> {
    let (raw, alias_chain) =
        INVOCATION.with(|i| (i.borrow().raw.clone(), i.borrow().alias_chain.clone()));
    // A create or a slug's move saw everything this copy holds: where another
    // entry had been, and whether it had left ([`met`], da-vkeg0, da-8mh8i).
    // A create always names its own endpoint, told so from one signed before.
    let of = (kind != "create" && effect["set"].get("slug").is_none()).then_some(head.0);
    let mut deps = vector_where(conn, of)?;
    if of.is_none() {
        deps.entry(ep.id.clone()).or_insert(0);
    }
    let made = (raw, alias_chain, Some(origin()), None);
    sign(conn, ep, head, kind, effect, made, deps)
}

/// Sign and store an op as this copy's next: its seq and HLC are taken
/// now, whatever made the change and whenever.
fn sign(
    conn: &Connection,
    ep: &Endpoint,
    (hid, slug, field): (&str, &str, Option<String>),
    kind: &str,
    effect: Value,
    (raw, alias_chain, origin, queued): (
        Option<String>,
        Option<Value>,
        Option<Value>,
        Option<&str>,
    ),
    deps: BTreeMap<String, i64>,
) -> Result<i64> {
    let seq: i64 = conn
        .prepare_cached("SELECT COALESCE(MAX(seq), 0) + 1 FROM ops WHERE endpoint = ?1")?
        .query_row(params![ep.id], |r| r.get(0))?;
    if seq <= rejoin::signed_before(conn, &ep.id)? {
        return Err(DatumError::Other(rejoin::REJOINING.into()));
    }
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
        "deps": deps,
    });
    if let Some(f) = &field {
        v["field"] = f.as_str().into();
    }
    if let Some(o) = &origin {
        v["origin"] = o.clone();
    }
    if let Some(q) = queued {
        v["queued"] = q.into();
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
        alias_chain: None,
        origin: None,
        // `store` reads the head and the body; the effect is in the body.
        effect: Value::Null,
        deps: BTreeMap::new(),
        queued: None,
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

/// Sign a `baseline` for every entry no op covers, as this copy's.
fn sign_uncovered(conn: &Connection) -> Result<()> {
    let ep = endpoint(conn)?;
    let uncovered: Vec<Entry> = conn
        .prepare("SELECT * FROM entries WHERE hid NOT IN (SELECT hid FROM ops) ORDER BY hid")?
        .query_map([], row_to_entry)?
        .collect::<rusqlite::Result<_>>()?;
    for e in &uncovered {
        push_entry(conn, &ep, e, "baseline", create_effect(e))?;
    }
    Ok(())
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
    if let Some(why) = read_only(conn, &ep)? {
        return Err(DatumError::Other(why.into()));
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
    if let Some(s) = &post.superseded_by {
        rederive(conn, s.as_str(), change)?;
    }
    // An edit of the entry shown saw every op of it, so it can settle a
    // conflict and make none: only one that has some is looked at again.
    // An edit of a row merged into another saw that row's ops alone.
    let contested = post.superseded_by.is_some()
        || conn
            .prepare_cached("SELECT 1 FROM entries WHERE hid = ?1 AND conflicts IS NOT NULL")?
            .exists(params![hid])?;
    if contested {
        refresh_conflicts(conn, hid.as_str())?;
    }
    Ok(())
}

// ─── Apply ──────────────────────────────────────────────────────────────────

/// What one `sync pipe apply` did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApplySummary {
    pub applied: usize,
    pub already_had: usize,
    /// Ops still waiting for an op before them, or for this clock to catch
    /// up with theirs ([`MAX_CLOCK_AHEAD`]) — held, not dropped.
    pub held: usize,
    pub conflicts: usize,
    /// Ops refused, each named with the reason.
    pub refused: Vec<String>,
    /// A purge order took this copy's data now (da-qzrj9).
    pub wiped: bool,
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

/// The rule ops fold into rows by, as a number: bumped by any change to
/// what a row and its `conflicts` are for the same ops — this file's fold,
/// conflicts and merge — schema change or not (da-sqpjt). A copy whose rows
/// were folded under an older rule refolds them as it opens
/// ([`Db::refold_for_rule`]); `the_fold_rule_is_fingerprinted` fails until
/// a change to that code is either bumped here or recorded as no change.
///
/// 1: the first stamped. A copy with no stamp was folded by a build before
/// it, perhaps before da-m82n0 merged a kit's spec with an automatic one.
/// 2: da-vkeg0. Two creates' agreeing value is settled by an op that saw
/// either, and a merged entry's slug is its fold's, not its survivor's.
/// 3: da-8mh8i. A row that came to a create's slug having seen it is not met by it.
pub(crate) const FOLD_RULE: u32 = 3;

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

// ─── Conflicts (da-05rkv) ───────────────────────────────────────────────────

/// Keys that say where a row came from rather than what it says: two rows
/// merged into one differ in them without disagreeing.
const PROVENANCE: &[&str] = &["create_time", "raw_input", "alias_chain", "parser_version"];

/// May two ops setting `k` differently be a conflict?
fn contestable(k: &str) -> bool {
    k != "deleted_at" && !NOT_A_CONFLICT.contains(&k) && !PROVENANCE.contains(&k)
}

/// Does `later` settle the value `op` gave a key both set: it saw `op`, or
/// `op` is a baseline of its entry and `later` is not — a baseline is under
/// every edit of its own entry, and of no other merged into it.
fn settles(later: &Op, op: &Op) -> bool {
    !later.same(op)
        && (later.saw(op) || op.is_baseline() && !later.is_baseline() && op.hid == later.hid)
}

/// The keys `op` gives a value, as [`values`] names them — but a create
/// gives none to a key it leaves empty: of two rows merged into one, a key
/// only one of them has is no clash.
fn intents(op: &Op) -> BTreeMap<String, Value> {
    let mut v = values(op);
    if op.effect.get("create").is_some() {
        v.retain(|_, x| !x.is_null());
    }
    v
}

/// Is `created` — a create's row — the spec a field's first log writes
/// ([`FieldSpec::is_bare`]) and no kit's? It declares nothing (da-m82n0):
/// what a kit or a person declared for the field applies whichever was
/// created first, and the two are no conflict.
fn automatic(created: &Map<String, Value>) -> bool {
    created.get("field").and_then(Value::as_str) == Some(SPEC_FIELD)
        && created.get("data").is_some_and(|d| {
            d.get(SPEC_KIT_KEY).is_none()
                && serde_json::from_value::<FieldSpec>(d.clone()).is_ok_and(|s| s.is_bare())
        })
}

/// What `ops` — an entry's, with those of the rows superseded by it — leave
/// contested, from the set of them alone: per key, the values of the ops
/// that set it and that no other op setting it settles, when they differ —
/// an [`automatic`] spec's create being no side of a key anything else set.
/// `deleted` is the entry's own: its deletes and undeletes as a key's
/// values, and against a delete that stands, an edit made without seeing
/// it, which wants the entry live.
fn contention(ops: &[Op]) -> BTreeMap<String, Vec<(&Op, Value)>> {
    let vals: Vec<_> = ops.iter().map(intents).collect();
    let mut setters: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, v) in vals.iter().enumerate() {
        for k in v.keys() {
            setters.entry(k.as_str()).or_default().push(i);
        }
    }
    // Of a key two creates gave one value — rows merged into one entry, one
    // event logged twice — an op that saw either settles both (da-vkeg0):
    // a retime of either is a change to the one entry.
    let create = |i: usize| ops[i].effect.get("create").is_some();
    let standing = |k: &str, set: &[usize]| -> Vec<usize> {
        let alike =
            |i: usize, m: usize| m == i || create(i) && create(m) && vals[m][k] == vals[i][k];
        let settled = |i: usize| {
            set.iter().any(|&j| {
                set.iter()
                    .any(|&m| alike(i, m) && settles(&ops[j], &ops[m]))
            })
        };
        set.iter().copied().filter(|&i| !settled(i)).collect()
    };
    let declares: Vec<bool> = ops
        .iter()
        .map(|o| !o.effect["create"].as_object().is_some_and(automatic))
        .collect();
    let mut out = BTreeMap::new();
    for (k, set) in setters.iter().filter(|(k, _)| contestable(k)) {
        let mut sides = standing(k, set);
        if sides.iter().any(|&i| declares[i]) {
            sides.retain(|&i| declares[i]);
        }
        let sides: Vec<_> = sides
            .into_iter()
            .map(|i| (&ops[i], vals[i][*k].clone()))
            .collect();
        if sides.iter().any(|(_, v)| *v != sides[0].1) {
            out.insert(k.to_string(), sides);
        }
    }
    // `deleted_at` is contested by what it says, not by its instant: two
    // deletes agree. Its standing setters are `deleted`'s sides, true or
    // false; and an edit sets no `deleted_at`, so one made beside a standing
    // delete is found here, as a side that wants the entry live.
    let Some(set) = setters.get("deleted_at") else {
        return out;
    };
    let mut sides: Vec<(usize, bool)> = standing("deleted_at", set)
        .into_iter()
        .map(|i| (i, !vals[i]["deleted_at"].is_null()))
        .collect();
    let deletes: Vec<usize> = sides.iter().filter(|s| s.1).map(|s| s.0).collect();
    let concurrent = |i: usize, d: usize| !settles(&ops[i], &ops[d]) && !settles(&ops[d], &ops[i]);
    for (i, v) in vals.iter().enumerate() {
        let edit = !v.contains_key("deleted_at") && v.keys().any(|k| contestable(k));
        if edit && deletes.iter().any(|&d| concurrent(i, d)) {
            sides.push((i, false));
        }
    }
    if sides.iter().any(|s| s.1) && sides.iter().any(|s| !s.1) {
        sides.sort();
        let sides = sides.into_iter().map(|(i, d)| (&ops[i], Value::Bool(d)));
        out.insert("deleted".into(), sides.collect());
    }
    out
}

/// `e`'s `conflicts`: per contested key, each side's value, the device that
/// set it (its label in the chain), its op's HLC, and the op. They are read
/// from `e`'s ops and those of the rows it supersedes — merged into it, or
/// set aside for the instant it holds; a superseded row has none.
fn conflicts_of(conn: &Connection, e: &Entry) -> Result<Option<Value>> {
    if e.superseded_by.is_some() {
        return Ok(None);
    }
    let ops = about(conn, e)?;
    let found = contention(&ops);
    if found.is_empty() {
        return Ok(None);
    }
    let chain = Chain::load(conn)?;
    let found = found
        .into_iter()
        .map(|(k, sides)| (k, sides.iter().map(|s| side(&chain, s)).collect()))
        .collect();
    Ok(Some(Value::Object(found)))
}

/// The ops `e`'s conflicts and history are read from, in fold order: its
/// own, and those of the rows it supersedes.
fn about(conn: &Connection, e: &Entry) -> Result<Vec<Op>> {
    let mut ops = ops_of(conn, e.hid.as_str())?;
    let behind: Vec<String> = conn
        .prepare_cached(&format!(
            "SELECT hid FROM entries e WHERE superseded_by = ?1 AND NOT ({TAKEN_BACK})"
        ))?
        .query_map(params![e.hid], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for hid in behind {
        ops.extend(ops_of(conn, &hid)?);
    }
    ops.sort_by(|a, b| a.order().cmp(&b.order()));
    Ok(ops)
}

/// The device an op is from: its endpoint's label in the chain, or the head
/// of its id — and for a `restore`, the backup, whichever copy read it in.
fn device(chain: &Chain, op: &Op) -> String {
    match chain.member(&op.endpoint) {
        _ if op.kind == "restore" => "backup".into(),
        Some(m) => m.label.clone(),
        None => op.endpoint.chars().take(8).collect(),
    }
}

/// One side of a contested key, as `conflicts` names it.
fn side(chain: &Chain, (op, value): &(&Op, Value)) -> Value {
    json!({
        "value": value,
        "device": device(chain, op),
        "hlc": op.hlc,
        "op": format!("{}/{}", op.endpoint, op.seq),
    })
}

/// Write the `conflicts` of `hid`, of the entry superseding it and of the
/// rows it supersedes — what a write to it can move; whether that gave one
/// of them conflicts it did not have.
fn refresh_conflicts(conn: &Connection, hid: &str) -> Result<bool> {
    let mut rows: Vec<Entry> = conn
        .prepare_cached(
            "SELECT * FROM entries WHERE hid = ?1 OR superseded_by = ?1
               OR hid = (SELECT superseded_by FROM entries WHERE hid = ?1)",
        )?
        .query_map(params![hid], row_to_entry)?
        .collect::<rusqlite::Result<_>>()?;
    rows.sort_by(|a, b| a.hid.0.cmp(&b.hid.0));
    let mut write = conn.prepare_cached(
        "UPDATE entries SET conflicts = ?2 WHERE hid = ?1 AND conflicts IS NOT ?2",
    )?;
    let mut new = false;
    for e in rows {
        let found = conflicts_of(conn, &e)?.map(|c| canonical_json(&c));
        new |= write.execute(params![e.hid, found])? > 0 && found.is_some();
    }
    Ok(new)
}

/// Write again every entry's `conflicts` that has some: the chain moved,
/// and a side's device may be named otherwise now.
fn relabel_conflicts(conn: &Connection) -> Result<()> {
    let hids: Vec<String> = conn
        .prepare_cached("SELECT hid FROM entries WHERE conflicts IS NOT NULL")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for hid in hids {
        refresh_conflicts(conn, &hid)?;
    }
    Ok(())
}

/// Write `conflicts` for every entry that could have any: ops from more than
/// one copy, or rows merged into it. The v57→v58 migration, once.
pub(crate) fn derive_all_conflicts(conn: &Connection) -> Result<()> {
    let hids: Vec<String> = conn
        .prepare(
            "SELECT hid FROM ops WHERE applied = 1 AND hid != ?1
               GROUP BY hid HAVING COUNT(DISTINCT endpoint) > 1
             UNION SELECT superseded_by FROM entries WHERE superseded_by IS NOT NULL",
        )?
        .query_map(params![CHAIN_HID], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for hid in hids {
        refresh_conflicts(conn, &hid)?;
    }
    Ok(())
}

/// Rows other than `e` holding an identity `e` needs: its slug, or its live
/// state-bearing `(field, occur_time)` — the two UNIQUE indexes, both of
/// which a superseded row is outside (ported from the relay sync's apply).
/// A kit's index is never a third: a kit may not declare one `unique`
/// (da-9lo90).
pub(super) fn blockers(conn: &Connection, e: &Entry) -> Result<Vec<Entry>> {
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

/// Live entries `e` is one with though their slugs differ now (da-vkeg0):
/// each had `e`'s create's slug for a stretch whose end that create did not
/// see — a retime it had not heard of, or none yet. An entry's slug is its
/// own create's, then whatever its edits and those of the rows merged into
/// it set, in fold order. A create's `deps` are all its copy held, and it
/// waits for them, so what it saw is here before it: a merge this finds is
/// never undone by an op arriving later. A create where an entry it saw had
/// left is an entry of its own, as is a row arriving having seen the create.
fn met(conn: &Connection, e: &Entry) -> Result<Vec<Entry>> {
    let ops = ops_of(conn, e.hid.as_str())?;
    // A create signed before da-vkeg0 names nothing it saw: it meets only
    // the entry at its slug now, as it always did.
    let Some(create) = ops
        .iter()
        .find(|o| o.kind == "create" && !o.deps.is_empty())
    else {
        return Ok(Vec::new());
    };
    let held: Vec<String> = conn
        .prepare_cached(
            "SELECT DISTINCT COALESCE(e.superseded_by, e.hid) FROM ops o JOIN entries e ON e.hid = o.hid
             WHERE o.slug = ?1 AND o.applied = 1 AND o.hid != ?2",
        )?
        .query_map(params![create.slug, e.hid], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for hid in held.iter().filter(|h| **h != e.hid.0) {
        let Some(r) = fetch_by_hid(conn, hid)? else {
            continue;
        };
        let mut timeline = ops_of(conn, hid)?;
        for m in merged_into(conn, &r)? {
            let theirs = ops_of(conn, m.hid.as_str())?;
            timeline.extend(
                theirs
                    .into_iter()
                    .filter(|o| o.effect.get("create").is_none()),
            );
        }
        timeline.sort_by(|a, b| a.order().cmp(&b.order()));
        let (mut came, mut overlap) = (None::<&Op>, false);
        for o in &timeline {
            if o.effect.get("create").is_none() && o.effect["set"].get("slug").is_none() {
                continue;
            }
            if let Some(c) = came.filter(|c| c.slug == create.slug && o.slug != c.slug) {
                overlap |= !create.saw(o) && !c.saw(create);
            }
            came = came.filter(|c| c.slug == o.slug).or(Some(o));
        }
        if overlap || came.is_some_and(|c| c.slug == create.slug) {
            out.push(r);
        }
    }
    Ok(out)
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

/// Was `hid`'s row brought in by `db restore --merge` ([`restore`])?
fn restored(conn: &Connection, hid: &Hid) -> Result<bool> {
    Ok(conn
        .prepare_cached("SELECT 1 FROM ops WHERE hid = ?1 AND kind = 'restore' AND applied = 1")?
        .exists(params![hid])?)
}

/// A restored row that `undo` took back out, a tombstone, as a SQL
/// condition on `entries e`: no part of the entry it went under, nor of its
/// conflicts, so that entry folds to what it was before the restore, and
/// `redo` brings the row back in.
const TAKEN_BACK: &str = "e.deleted_at IS NOT NULL AND EXISTS (SELECT 1 FROM ops r
    WHERE r.hid = e.hid AND r.kind = 'restore' AND r.applied = 1)";

/// The rows merged into `s`: superseded by it, and holding or having held
/// a slug `s` holds or held, so the merge follows a retime of either
/// (da-yose2). A row superseded under another slug alone — one state at one
/// instant, which an id template declared two events — is not merged.
fn merged_into(conn: &Connection, s: &Entry) -> Result<Vec<Entry>> {
    conn.prepare_cached(&format!(
        "WITH held(slug) AS (SELECT ?2 UNION SELECT slug FROM ops WHERE hid = ?1 AND applied = 1)
         SELECT * FROM entries e WHERE superseded_by = ?1 AND NOT ({TAKEN_BACK})
           AND (slug IN (SELECT slug FROM held)
             OR EXISTS (SELECT 1 FROM ops o WHERE o.hid = e.hid AND o.applied = 1
                          AND o.slug IN (SELECT slug FROM held)))",
    ))?
    .query_map(params![s.hid, s.slug], row_to_entry)?
    .collect::<rusqlite::Result<_>>()
    .map_err(DatumError::Db)
}

/// The row `s` shows with `members` merged into it (da-04glw): their
/// creates in creation order, each over the one before — so of a key two
/// have, the later-created row's value, an [`automatic`] spec's going under
/// every other — then every edit of any of them in
/// fold order, so an edit addressed to either lands. Identity stays `s`'s.
/// It is derived from the ops alone, the same on every copy, and sent as
/// none.
fn merge_rows(conn: &Connection, s: &Entry, members: &[Entry]) -> Result<Entry> {
    merge_from(s, members, |hid| ops_of(conn, hid))
}

/// [`merge_rows`] over the ops `ops_of` gives each row: all it has applied,
/// or those up to one version of its history.
fn merge_from(
    s: &Entry,
    members: &[Entry],
    ops_of: impl Fn(&str) -> Result<Vec<Op>>,
) -> Result<Entry> {
    if members.is_empty() {
        return Ok(s.clone());
    }
    let mut rows: Vec<&Entry> = std::iter::once(s).chain(members).collect();
    rows.sort_by(|a, b| (&a.create_time, &a.hid.0).cmp(&(&b.create_time, &b.hid.0)));
    // `s` is not always the first created, so its own create is the base —
    // the bookkeeping — and every create's content, its own included, goes
    // over it in creation order.
    let (mut base, mut creates, mut edits) = (None, Vec::new(), Vec::new());
    for e in rows {
        let ops = ops_of(e.hid.as_str())?;
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
    creates.sort_by_key(|c| !automatic(c));
    let mut shown = base;
    for created in &creates {
        fold_into(&mut shown, &json!({ "set": content(created) }));
    }
    edits.sort_by(|a, b| a.order().cmp(&b.order()));
    for op in &edits {
        fold_into(&mut shown, &op.effect);
    }
    let mut out = entry_from(shown.unwrap_or_default())?;
    // The slug is the fold's, like the instant it names: a retime of any
    // of the rows moves the one entry (da-vkeg0).
    out.hid = s.hid.clone();
    out.create_time = s.create_time;
    out.superseded_by = None;
    Ok(out)
}

/// The row `pre` shows by its ops: its own fold when superseded, and
/// otherwise with what is merged into it.
fn derived(conn: &Connection, pre: &Entry) -> Result<Entry> {
    let own = own(conn, pre)?;
    match pre.superseded_by {
        Some(_) => Ok(own),
        None => merge_rows(conn, &own, &merged_into(conn, &own)?),
    }
}

/// Rewrite `hid`'s row to what it shows ([`derived`]). `kind` is what moved
/// it: a sync's merge, or the command whose write it follows from.
fn rederive(conn: &Connection, hid: &str, kind: ChangeKind) -> Result<()> {
    let Some(pre) = fetch_by_hid(conn, hid)? else {
        return Ok(());
    };
    let next = derived(conn, &pre)?;
    if row(&pre) == row(&next) {
        return Ok(());
    }
    history::record(conn, &pre, kind)?;
    apply_entry_state(conn, &next)
}

/// Bring `incoming` in as a sync from a copy frozen at its backup would
/// (`db restore --merge`, da-4ldvb): a backup's entry whose slug or state's
/// instant a live entry holds gets a row of its own and a `restore` op that
/// saw none of that entry's ops ([`Op::saw`]). What the entry shows and its
/// `conflicts` are then the fold's ([`write_merged`]), the same on every
/// copy the op reaches. The row's hid is new, so the op's `deps` are empty
/// and its only create is the restore on every copy, and the changes are
/// the command's own, so one `undo` takes the row back out
/// ([`TAKEN_BACK`]). The slug of the entry it shows in; none when that
/// entry's own row, or a live one under it, has its content `hash` already.
pub(super) fn restore(conn: &Connection, incoming: &Entry, hash: &str) -> Result<Option<Slug>> {
    let ep = endpoint(conn)?;
    if let Some(why) = read_only(conn, &ep)? {
        return Err(DatumError::Other(why.into()));
    }
    let mut row = incoming.clone();
    row.hid = new_hid();
    let live = blockers(conn, &row)?;
    let mut under = conn.prepare_cached(
        "SELECT 1 FROM entries
         WHERE superseded_by = ?1 AND deleted_at IS NULL AND content_hash = ?2",
    )?;
    for b in &live {
        if under.exists(params![b.hid, hash])? || own(conn, b)?.content_hash == hash {
            return Ok(None);
        }
    }
    // An entry older than its ops is signed as it stands first: a baseline
    // this copy signed after the restore would have seen it.
    for b in &live {
        if !has_ops(conn, &b.hid)? {
            push_entry(conn, &ep, b, "baseline", create_effect(b))?;
        }
    }
    push_entry(conn, &ep, &row, "restore", create_effect(&row))?;
    refold(conn, row.hid.as_str(), ChangeKind::UserEdit)?;
    conn.query_row(
        "SELECT slug FROM entries
         WHERE hid = (SELECT COALESCE(superseded_by, hid) FROM entries WHERE hid = ?1)",
        params![row.hid],
        |r| r.get(0),
    )
    .optional()
    .map_err(DatumError::Db)
}

/// The highest seq this copy has signed: where a command run on a
/// snapshot starts, for [`signed_since`] to read back what it signed.
pub(super) fn own_seq(conn: &Connection) -> Result<i64> {
    Ok(own_head(conn)?.1)
}

/// The ops this copy signed after `seq`, in order, as signed.
pub(super) fn signed_since(conn: &Connection, seq: i64) -> Result<Vec<Value>> {
    let id = endpoint(conn)?.id;
    let mut stmt =
        conn.prepare("SELECT body FROM ops WHERE endpoint = ?1 AND seq > ?2 ORDER BY seq")?;
    let bodies = stmt
        .query_map(params![id, seq], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    bodies
        .iter()
        .map(|b| serde_json::from_str(b).map_err(Into::into))
        .collect()
}

/// Land ops a command signed on a snapshot of this copy while another held
/// its write lock (da-9q3fr, `db/spool.rs`): each is signed again as this
/// copy's next, with the `deps` it was made with, and its entry written
/// from its ops — so an op that landed since, which it never saw, stands
/// beside it as a sync's would. One action on the undo stack, as the
/// command was; an undo or a redo moves the stack as it did on the snapshot.
/// An op made before a wipe this copy carried out is the old copy's data
/// (da-59hwx): the order took every op signed before it, and this one,
/// signed again, would outlive it; one made before a wipe that has not
/// reached this copy yet carries when it was made (`queued`), which that
/// wipe reads (da-m6c4j). Whether any op was left to land.
pub(super) fn apply_spooled(conn: &Connection, ops: &[Value]) -> Result<bool> {
    let wiped: Option<String> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'sync_wiped'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let mut ops = ops
        .iter()
        .map(|v| serde_json::from_value::<Op>(v.clone()).map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;
    ops.retain(|op| wiped.as_deref().is_none_or(|w| op.hlc.as_str() >= w));
    if ops.is_empty() {
        return Ok(false);
    }
    let ep = endpoint(conn)?;
    if let Some(why) = read_only(conn, &ep)? {
        return Err(DatumError::Other(why.into()));
    }
    crate::undo_log::scope(|| {
        for op in &ops {
            if op.is_baseline() && has_ops(conn, &Hid::from(op.hid.clone()))? {
                continue;
            }
            let head = (op.hid.as_str(), op.slug.as_str(), op.field.clone());
            let made = (
                op.raw.clone(),
                op.alias_chain.clone(),
                op.origin.clone(),
                Some(op.hlc.as_str()),
            );
            sign(
                conn,
                &ep,
                head,
                &op.kind,
                op.effect.clone(),
                made,
                op.deps.clone(),
            )?;
            let kind = match op.kind.as_str() {
                "retime" => ChangeKind::Retime,
                "undo" => ChangeKind::Undo,
                "redo" => ChangeKind::Redo,
                "sync_conflict_resolve" => ChangeKind::SyncConflictResolve,
                _ => ChangeKind::UserEdit,
            };
            refold(conn, &op.hid, kind)?;
        }
        Ok::<_, DatumError>(())
    })?;
    // The action an undo took back is the newest on the stack that wrote
    // one of its entries; a redo's, the oldest undone one.
    for (kind, pick) in [
        ("undo", "a.undone = 0 ORDER BY a.id DESC"),
        ("redo", "a.undone = 1 ORDER BY a.id ASC"),
    ] {
        let hids: Vec<&str> = ops
            .iter()
            .filter(|o| o.kind == kind)
            .map(|o| o.hid.as_str())
            .collect();
        if hids.is_empty() {
            continue;
        }
        let action: Option<i64> = conn
            .query_row(
                &format!(
                    "SELECT a.id FROM undo_actions a JOIN entry_history h ON h.action_id = a.id
                     WHERE h.entry_hid IN (SELECT value FROM json_each(?1)) AND {pick} LIMIT 1"
                ),
                params![serde_json::to_string(&hids)?],
                |r| r.get(0),
            )
            .optional()?;
        match (kind, action) {
            ("undo", Some(id)) => crate::undo_log::set_undone(conn, id, &[])?,
            (_, Some(id)) => crate::undo_log::set_redone(conn, id)?,
            _ => {}
        }
    }
    Ok(true)
}

/// Write the row an entry's ops fold to. Two distinct entries claiming one
/// identity decide it from the rows alone, so every copy reaches the same
/// answer: the earlier-created keeps it and the other is superseded
/// (`superseded_by`) — a row a restore brought in ([`restored`]) keeping it
/// from none that was not — and what the two gave different values shows in
/// the survivor's `conflicts`. Under one slug the two are one entry
/// ([`merge_rows`]); under one live state-bearing instant alone, two.
/// `kind` is what the rows it moves record: a sync's merge, or the command
/// ([`restore`]) whose write this is, which creates the row it inserts.
fn write_merged(
    conn: &Connection,
    pre: Option<&Entry>,
    mut next: Entry,
    kind: ChangeKind,
) -> Result<()> {
    next.superseded_by = pre.and_then(|p| p.superseded_by.clone());
    let mut demoted = Vec::new();
    if next.superseded_by.is_none() {
        // What a row loses its identity with goes over to the row that
        // took it, so a pair's rows point at one survivor on every copy.
        let repoint = "UPDATE entries SET superseded_by = ?2 WHERE hid = ?1 OR superseded_by = ?1";
        let rank = |e: &Entry| {
            Ok::<_, DatumError>((restored(conn, &e.hid)?, e.create_time, e.hid.0.clone()))
        };
        let mut found = blockers(conn, &next)?;
        for m in met(conn, &next)? {
            if found.iter().all(|b| b.hid != m.hid) {
                found.push(m);
            }
        }
        for b in found {
            if rank(&next)? < rank(&b)? {
                history::record(conn, &b, kind)?;
                conn.execute(repoint, params![b.hid, next.hid])?;
                demoted.push(b.hid.0);
            } else if next.superseded_by.is_none() {
                // A row older than its ops is signed as it stands before
                // the merge is written over it, so its own row stays known.
                if !has_ops(conn, &b.hid)? && read_only(conn, &endpoint(conn)?)?.is_none() {
                    push_entry(conn, &endpoint(conn)?, &b, "baseline", create_effect(&b))?;
                }
                conn.execute(repoint, params![next.hid, b.hid])?;
                next.superseded_by = Some(b.hid.clone());
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
                history::record(conn, pre, kind)?;
                apply_entry_state(conn, &next)?;
            }
        }
        None => {
            try_insert(conn, &next.slug, &next.hid, &next)?;
            if let Some(stored) = fetch_by_hid(conn, next.hid.as_str())? {
                let made = match kind {
                    ChangeKind::SyncMerge => kind,
                    _ => ChangeKind::Create,
                };
                history::record(conn, &stored, made)?;
            }
        }
    }
    // A demoted survivor goes back to its own row; the survivor of a row
    // merged into it takes in what changed.
    for hid in demoted
        .iter()
        .chain(next.superseded_by.as_ref().map(|h| &h.0))
    {
        rederive(conn, hid, kind)?;
    }
    Ok(())
}

/// Apply one op that is ready: refold its entry.
fn apply_one(conn: &Connection, x: &Op) -> Result<usize> {
    conn.execute(
        "UPDATE ops SET applied = 1 WHERE endpoint = ?1 AND seq = ?2",
        params![x.endpoint, x.seq],
    )?;
    refold(conn, &x.hid, ChangeKind::SyncMerge)
}

/// Write the row `hid`'s applied ops fold to, as `kind`; 1 if that gave it,
/// or the entry it is merged into, a conflict it had not had.
fn refold(conn: &Connection, hid: &str, kind: ChangeKind) -> Result<usize> {
    let pre = fetch_by_hid(conn, hid)?;
    let ops = ops_of(conn, hid)?;
    let base = pre
        .as_ref()
        .filter(|_| ops.first().is_none_or(|o| o.effect.get("create").is_none()));
    let Some(merged) = fold(&ops, base) else {
        return Ok(0);
    };
    write_merged(conn, pre.as_ref(), entry_from(merged)?, kind)?;
    Ok(refresh_conflicts(conn, hid)?.into())
}

/// Apply every held op that is ready, records first, until none is; an
/// entry op the chain does not allow is refused when it is ready, so the
/// refused are as contiguous as the applied, and what waits for an op
/// before it stays held.
fn settle(conn: &Connection, s: &mut ApplySummary) -> Result<()> {
    let before = s.refused.len();
    let mut chain = Chain::load(conn)?;
    let mut vector = vector_where(conn, None)?;
    // A filtered reader is never sent what its filter leaves out, so it
    // waits on no gap and no dep — the fold reads only the set of an
    // entry's ops, so order is no matter — and what it holds is ready;
    // `settle_filter` refuses what falls outside its filter (da-u81u1).
    let mine = endpoint(conn)?.id;
    let filtered = Chain::load_with_waiting(conn)?.filter(&mine).is_some();
    let now = now_ms();
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
            // An op from a clock too far ahead waits for this one.
            let ready = (filtered
                || op.seq <= have(&op.endpoint) + 1 && op.deps.iter().all(|(e, n)| have(e) >= *n))
                && clock_ahead(&op.hlc, now).is_none();
            if !ready {
                continue;
            }
            progressed = true;
            if !op.is_chain() {
                // A purge order took it, or a dissolved chain takes nothing
                // of another copy's: neither its data nor a refusal is kept.
                let made = op.queued.as_deref().unwrap_or(&op.hlc);
                let wiped = chain.wiped(made).is_some()
                    || chain.dissolved().is_some() && op.endpoint != mine;
                match chain.allows(&op.endpoint, &op.hlc) {
                    _ if wiped => scrub(conn, &op.endpoint, op.seq)?,
                    Ok(()) => {
                        s.conflicts += apply_one(conn, op)?;
                        s.applied += 1;
                    }
                    Err(why) => refuse(conn, &chain, op, &why, s)?,
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
            relabel_conflicts(conn)?;
            vector = vector_where(conn, None)?;
            // What waits has changed with the membership.
            break;
        }
        if !progressed {
            break waiting;
        }
    };
    s.held = waiting.len();
    // What a later record allowed again — a re-admission (da-j8c4l), whose
    // deps were the very ops its revoke refused — is not refused.
    let mut again = conn.prepare_cached(
        "SELECT 1 FROM ops WHERE endpoint = ?1 AND seq = ?2 AND hid != ?3 AND applied != 2",
    )?;
    let mut kept = Vec::new();
    for r in s.refused.split_off(before) {
        let op = r.split(':').next().and_then(|o| o.rsplit_once('/'));
        if !op.map_or(Ok(false), |(e, n)| again.exists(params![e, n, CHAIN_HID]))? {
            kept.push(r);
        }
    }
    s.refused.extend(kept);
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
/// again ([`reconsider`]). The first refused of an endpoint files its
/// `refused` sync notice (da-0c2ne), which `datum sync chain ack` settles —
/// but for a purged endpoint's, set aside on purpose (da-ry11c).
fn refuse(
    conn: &Connection,
    chain: &Chain,
    op: &Op,
    why: &str,
    s: &mut ApplySummary,
) -> Result<()> {
    conn.execute(
        "UPDATE ops SET applied = 2 WHERE endpoint = ?1 AND seq = ?2",
        params![op.endpoint, op.seq],
    )?;
    s.refused.push(format!("{}/{}: {why}", op.endpoint, op.seq));
    let member = chain.member(&op.endpoint);
    // A purged endpoint's refusals are on purpose, and warn of nothing.
    if member.is_some_and(|m| m.purged.is_some()) {
        return Ok(());
    }
    // A device outside the chain has no label in it: the host it says it is.
    let host = op.origin.as_ref().and_then(|o| o["host"].as_str());
    let label = member.map_or(host.unwrap_or(&op.endpoint[..10]), |m| &m.label);
    conn.prepare_cached(
        "INSERT OR IGNORE INTO sync_notices (subject, kind, label, at, via)
         VALUES (?1, 'refused', ?2, ?3, ?4)",
    )?
    .execute(params![
        format!("refused:{}", op.endpoint),
        label,
        chain::hlc_time(&op.hlc),
        why
    ])?;
    Ok(())
}

/// The membership moved: refused ops it now allows wait again, and applied
/// ops it no longer allows — a revoked endpoint's, made after the revoke;
/// a purged one's, all of them — are refused and their entries refolded
/// without them. An entry left with no create is gone, and the rows merged
/// into it stand on their own again.
fn reconsider(conn: &Connection, chain: &Chain, s: &mut ApplySummary) -> Result<()> {
    wipe(conn, chain, s)?;
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
                refuse(conn, chain, &op, &why, s)?;
                hids.insert(op.hid);
            }
        }
    }
    let mut touched = Vec::new();
    for hid in hids {
        let pre = fetch_by_hid(conn, &hid)?;
        touched.push(hid.clone());
        match fold(&ops_of(conn, &hid)?, None) {
            Some(r) => write_merged(conn, pre.as_ref(), entry_from(r)?, ChangeKind::SyncMerge)?,
            None => {
                let Some(pre) = pre else { continue };
                history::record(conn, &pre, ChangeKind::SyncMerge)?;
                let freed: Vec<Entry> = conn
                    .prepare_cached("SELECT * FROM entries WHERE superseded_by = ?1")?
                    .query_map(params![hid], row_to_entry)?
                    .collect::<rusqlite::Result<_>>()?;
                conn.execute("DELETE FROM entries WHERE hid = ?1", params![hid])?;
                // One at a time, each still pointing at the gone row — out of
                // the identity indexes — while `write_merged` decides it: two
                // freed rows may share an identity, and the later merges into
                // the earlier. One that stands alone points at nothing after.
                for mut f in freed {
                    touched.push(f.hid.0.clone());
                    f.superseded_by = None;
                    let own = own(conn, &f)?;
                    write_merged(conn, Some(&f), own, ChangeKind::SyncMerge)?;
                    conn.execute(
                        "UPDATE entries SET superseded_by = NULL WHERE hid = ?1 AND superseded_by = ?2",
                        params![f.hid, hid],
                    )?;
                }
                if let Some(kept) = &pre.superseded_by {
                    touched.push(kept.0.clone());
                    rederive(conn, kept.as_str(), ChangeKind::SyncMerge)?;
                }
            }
        }
    }
    for hid in touched {
        refresh_conflicts(conn, &hid)?;
    }
    // An endpoint the chain now allows has nothing left to warn of.
    conn.execute(
        "DELETE FROM sync_notices WHERE kind = 'refused' AND NOT EXISTS
           (SELECT 1 FROM ops WHERE endpoint = substr(subject, 9) AND applied = 2)",
        [],
    )?;
    Ok(())
}

/// The hid of an op a purge order took (da-qzrj9): no entry's.
const PURGED: &str = "~purged";

/// Keep of an op a purge order took only its endpoint, seq and HLC.
fn scrub(conn: &Connection, endpoint: &str, seq: i64) -> Result<()> {
    conn.prepare_cached(
        "UPDATE ops SET applied = 3, hid = ?3, slug = '', kind = 'purged', body = ''
         WHERE endpoint = ?1 AND seq = ?2",
    )?
    .execute(params![endpoint, seq, PURGED])?;
    Ok(())
}

/// Carry out the chain's newest purge order, once (da-qzrj9): every entry op
/// made before it is scrubbed — in a dissolved chain, every op of another
/// copy's — and every entry, its history, the undo stack, the warnings and
/// the stats cache go, a copy that did not order it keeping a backup of its
/// entries first ([`keep_what_was_here`]). What is left, the ops made after
/// it, folds again.
fn wipe(conn: &Connection, chain: &Chain, s: &mut ApplySummary) -> Result<()> {
    let Some(w) = chain.wipes.last() else {
        return Ok(());
    };
    let done: Option<String> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'sync_wiped'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if done.is_some_and(|d| d >= w.hlc) {
        return Ok(());
    }
    let mine = endpoint(conn)?.id;
    if w.by != mine {
        keep_what_was_here(conn, chain, w)?;
    }
    // What it frees is overwritten, not left in the file's free pages.
    let secure: i64 = conn.query_row("PRAGMA secure_delete", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA secure_delete = ON")?;
    // A spooled op was made when its command ran, not when it drained.
    conn.execute(
        "UPDATE ops SET applied = 3, hid = ?1, slug = '', kind = 'purged', body = ''
         WHERE applied IN (1, 2) AND hid != ?2
           AND (COALESCE(CASE WHEN body != '' THEN body ->> '$.queued' END, hlc) < ?3
                OR ?4 AND endpoint != ?5)",
        params![PURGED, CHAIN_HID, w.hlc, w.all, mine],
    )?;
    conn.execute_batch(
        "DELETE FROM entries; DELETE FROM entry_history; DELETE FROM undo_actions;
         DELETE FROM integrity_warnings; DELETE FROM last_docs;
         DELETE FROM stats_cache; DELETE FROM stats_cache_days;
         DELETE FROM stats_cache_through; DELETE FROM stats_cache_dirty;",
    )?;
    // A PRAGMA binds no parameter; `secure` is the integer it gave.
    conn.execute_batch(&format!("PRAGMA secure_delete = {secure}"))?;
    let hids: Vec<String> = conn
        .prepare("SELECT DISTINCT hid FROM ops WHERE applied = 1 AND hid != ?1")?
        .query_map(params![CHAIN_HID], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for hid in hids {
        refold(conn, &hid, ChangeKind::SyncMerge)?;
    }
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('sync_wiped', ?1)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![w.hlc],
    )?;
    s.wiped = true;
    Ok(())
}

/// A chain restore's or purge's wipe reaching a copy that did not order it
/// (da-uk23v, da-acwey): its live entries first, as a `db backup` writes
/// them, beside the database, so one made offline before the wipe is not
/// lost with it — `db restore <it> --merge` brings it back — and a notice
/// of where, `purge:<hlc>` for a purge's and `wipe:<hlc>` for a restore's.
/// The ordering copy wrote its own; a copy with no file or no entry writes
/// none.
fn keep_what_was_here(conn: &Connection, chain: &Chain, w: &chain::Wipe) -> Result<()> {
    use std::io::Write;
    let Some(path) = conn.path().filter(|p| !p.is_empty()) else {
        return Ok(());
    };
    let live = format!("FROM entries WHERE {}", super::types::LIVE_SQL);
    let count: usize = conn.query_row(&format!("SELECT count(*) {live}"), [], |r| r.get(0))?;
    if count == 0 {
        return Ok(());
    }
    // Named by the wipe, which is carried out once: a retry of one that
    // failed writes the same file again, and no two wipes share a name.
    let to = format!(
        "{path}.wiped-{}.jsonl",
        w.hlc.replace(['-', ':'], "").replace('/', "-")
    );
    let write = || -> Result<()> {
        let io = |e: std::io::Error| DatumError::Other(format!("{to}: {e}"));
        let file = std::fs::File::create(&to).map_err(io)?;
        let mut out = std::io::BufWriter::new(&file);
        let header = json!({
            "type": "daydumb_backup", "schema_version": CURRENT_SCHEMA_VERSION,
            "backup_time": Utc::now().to_rfc3339(), "source_db": path, "entry_count": count,
        });
        writeln!(out, "{header}").map_err(io)?;
        let mut stmt = conn.prepare(&format!("SELECT * {live} ORDER BY slug"))?;
        for e in stmt.query_map([], row_to_entry)? {
            writeln!(out, "{}", serde_json::to_string(&e?)?).map_err(io)?;
        }
        out.flush().map_err(io)?;
        drop(out);
        file.sync_all().map_err(io)
    };
    // Cut short, it leaves no part of a backup to be taken for the whole.
    if let Err(e) = write() {
        let _ = std::fs::remove_file(&to);
        return Err(e);
    }
    let by = chain.member(&w.by).map_or(&w.by[..10], |m| &m.label);
    let what = if w.restore { "wipe" } else { "purge" };
    conn.execute(
        "INSERT OR IGNORE INTO sync_notices (subject, kind, label, at, via)
         VALUES (?1, 'wipe', ?2, ?3, ?4)",
        params![format!("{what}:{}", w.hlc), by, chain::hlc_time(&w.hlc), to],
    )?;
    Ok(())
}

/// Fill this copy's gaps with the seqs a sender says a purge order took
/// (da-qzrj9) — only while it holds an order, and per endpoint only on from
/// what it holds, so a seq it lacks is never skipped: `settle` applies a
/// member's ops in seq order, so its vector is a gapless head. A filtered
/// reader's is not — it holds what its filter keeps — but it is served no
/// such line ([`filtered_lines`]) and waits on no gap.
fn take_purged(conn: &Connection, words: &[Value]) -> Result<()> {
    let ordered = conn
        .prepare_cached(
            "SELECT 1 FROM ops WHERE hid = ?1 AND json_extract(body, '$.effect.type') = 'wipe'",
        )?
        .exists(params![CHAIN_HID])?;
    if !ordered {
        return Ok(());
    }
    let vector = vector_where(conn, None)?;
    let mut held = conn.prepare_cached("SELECT 1 FROM ops WHERE endpoint = ?1 AND seq = ?2")?;
    for runs in words.iter().filter_map(|w| w["purged"].as_object()) {
        for (e, runs) in runs {
            let took = |n: i64| {
                runs.as_array().into_iter().flatten().any(|r| {
                    r[0].as_i64().is_some_and(|a| a <= n) && r[1].as_i64().is_some_and(|b| n <= b)
                })
            };
            let mut n = vector.get(e).copied().unwrap_or(0) + 1;
            loop {
                if !held.exists(params![e, n])? {
                    if !took(n) {
                        break;
                    }
                    conn.execute(
                        "INSERT INTO ops (endpoint, seq, hlc, hid, slug, kind, applied, body)
                         VALUES (?1, ?2, '', ?3, '', 'purged', 3, '')",
                        params![e, n, PURGED],
                    )?;
                }
                n += 1;
            }
        }
    }
    Ok(())
}

/// A sender's removal, mark or purged seqs rather than ops. Canonical
/// JSON: an op's first key is `alias_chain`.
fn is_word(line: &str) -> bool {
    ["{\"caught_up\"", "{\"remove\"", "{\"purged\""]
        .iter()
        .any(|w| line.starts_with(w))
}

/// [`settle`]'s order for a line of ops: records first, then HLC. An
/// array is one entry's ops in fold order, never a record, so by its
/// first. An unreadable line sorts anywhere, and `apply_in` refuses it.
fn apply_order(line: &str) -> (bool, String, String) {
    let v: Value = serde_json::from_str(line).unwrap_or_default();
    let op = if v.is_array() { &v[0] } else { &v };
    let s = |k: &str| op[k].as_str().unwrap_or_default().to_string();
    (op["kind"] != "chain", s("hlc"), s("endpoint"))
}

/// [`Db::sync_apply`] in a transaction of the caller's, past its refusals:
/// `datum debug conflicts` applies its device's ops here, in its own.
fn apply_in(tx: &Connection, lines: &str) -> Result<ApplySummary> {
    let (mine, signed) = own_head(tx)?;
    let chain = Chain::load(tx)?;
    let before = chain.filter(&mine).cloned();
    let mut s = ApplySummary::default();
    let (mut got, mut words) = (Vec::new(), Vec::new());
    let mut each = Vec::new();
    for line in lines.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if is_word(line) {
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
        // A copy in a chain takes no record of another (da-d8ort): the
        // fold takes the earliest genesis it holds, so an older chain's
        // would become this copy's, and its own be refused.
        let theirs = op.effect["chain"].as_str().unwrap_or_default();
        if op.is_chain() && chain.exists() && theirs != chain.id {
            s.refused.push(format!(
                "{}/{}: a record of another sync chain ({theirs}); this copy is in {}",
                op.endpoint, op.seq, chain.id
            ));
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
                store(tx, &op, false)?;
                got.push((op.endpoint, op.seq, op.hid));
            }
            // A stub is an op a purge order took (da-qzrj9).
            Some(body) if body == op.body || body.is_empty() => s.already_had += 1,
            _ => s
                .refused
                .push(format!("{}/{}: {SHARED_KEY}", op.endpoint, op.seq)),
        }
    }
    take_purged(tx, &words)?;
    settle(tx, &mut s)?;
    settle_filter(tx, &mine, before.as_ref(), &got, &words, &mut s)?;
    Ok(s)
}

// ─── Filtered readers (da-u81u1) ────────────────────────────────────────────

/// The ops of `hid` held, applied or refused, in fold order, each with
/// whether it is refused: what a sender and a filtered reader both judge an
/// entry's field by. A purge's stub is no entry's op and has no body.
fn held_ops(conn: &Connection, hid: &str) -> Result<Vec<(Op, bool)>> {
    let mut stmt =
        conn.prepare_cached("SELECT body, applied FROM ops WHERE hid = ?1 AND applied IN (1, 2)")?;
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
         FROM ops WHERE applied IN (1, 2) ORDER BY endpoint, seq",
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
    // A purge's stubs are no entry, and dropping them would open the gaps
    // they fill (da-qzrj9).
    for hid in hids.iter().filter(|h| *h != CHAIN_HID && *h != PURGED) {
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

/// The rows [`Db::sync_state_hash`] hashes, in `(slug, hid)` order: every
/// entry, each with its `superseded_by` and any `conflicts`, that passes the
/// filter of this copy and of `peer`; and whether either has one.
/// `me` is this copy's endpoint id where the caller holds it already.
fn shared_rows(
    conn: &Connection,
    me: Option<&str>,
    peer: Option<&str>,
) -> Result<(Vec<Map<String, Value>>, bool)> {
    match shared_filters(conn, me, peer)? {
        Some(filters) => Ok((rows_passing(conn, &filters)?, !filters.is_empty())),
        None => Ok((Vec::new(), false)),
    }
}

/// The filters of this copy and of `peer` that [`shared_rows`] reads by;
/// none at all in a dissolved chain, whose copies share nothing (da-qzrj9).
fn shared_filters(
    conn: &Connection,
    me: Option<&str>,
    peer: Option<&str>,
) -> Result<Option<Vec<Filter>>> {
    let chain = Chain::load(conn)?;
    if chain.dissolved().is_some() {
        return Ok(None);
    }
    let mine = match (chain.exists(), me) {
        (false, _) => None,
        (true, Some(me)) => Some(me.to_string()),
        (true, None) => Some(endpoint(conn)?.id),
    };
    // The peer's filter is read from the chain this copy serves it by, with
    // the records still waiting (da-91wab).
    let served = Chain::served(conn)?;
    Ok(Some(
        [
            mine.as_deref().and_then(|e| chain.filter(e)),
            peer.and_then(|e| served.filter(e)),
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect(),
    ))
}

/// Every entry row that passes each of `filters`, as [`shared_rows`].
fn rows_passing(conn: &Connection, filters: &[Filter]) -> Result<Vec<Map<String, Value>>> {
    let rows = conn
        .prepare("SELECT * FROM entries ORDER BY slug, hid")?
        .query_map([], |r| {
            let e = row_to_entry(r)?;
            let conflicts: Option<String> = r.get("conflicts")?;
            let mut r = row(&e);
            r.insert("superseded_by".into(), json!(e.superseded_by));
            if let Some(c) = conflicts {
                r.insert(
                    "conflicts".into(),
                    serde_json::from_str(&c).unwrap_or_default(),
                );
            }
            Ok(r)
        })?
        .filter(|r| {
            r.as_ref().map_or(true, |r| {
                let belongs = belongs_to(r);
                filters.iter().all(|f| f.admits(belongs.as_deref()))
            })
        })
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

impl Db {
    /// Every entry with conflicts, in slug order.
    pub fn sync_conflicts(&self) -> Result<Vec<Entry>> {
        self.conn
            .prepare("SELECT * FROM entries WHERE conflicts IS NOT NULL ORDER BY slug")?
            .query_map([], row_to_entry)?
            .collect::<rusqlite::Result<_>>()
            .map_err(DatumError::Db)
    }

    /// Resolve the conflicts of entry `hid` (full) with one ordinary edit,
    /// which saw every side: each contested key set to its value in
    /// `picks` (a data key named bare or as `data.<key>`), or else to the
    /// one the entry shows (`deleted`: true keeps it deleted, false brings
    /// it back). Wherever the edit arrives, the conflicts are gone. The
    /// entry as it stands after.
    pub fn resolve_conflicts(&self, hid: &str, picks: &Map<String, Value>) -> Result<Entry> {
        // One entry: its reads are of the rows it writes.
        self.write_with(|tx| {
            let ep = endpoint(tx)?;
            if let Some(why) = read_only(tx, &ep)? {
                return Err(DatumError::Other(why.into()));
            }
            let conflicts: Option<String> = tx
                .query_row(
                    "SELECT conflicts FROM entries WHERE hid = ?1",
                    params![hid],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            let (Some(pre), Some(Value::Object(conflicts))) = (
                fetch_by_hid(tx, hid)?,
                conflicts.map(|c| serde_json::from_str(&c)).transpose()?,
            ) else {
                return Err(DatumError::Other(format!("{hid} has no conflicts")));
            };
            // A data key is picked as it is typed, `cups` for `data.cups`.
            let mut named = Map::new();
            for (k, v) in picks {
                let key = [k.clone(), format!("data.{k}")]
                    .into_iter()
                    .find(|key| conflicts.contains_key(key))
                    .ok_or_else(|| DatumError::Other(format!("{hid} has no conflict on {k}")))?;
                named.insert(key, v.clone());
            }
            let picks = named;
            let shown = row(&pre);
            let (mut set, mut unset) = (Map::new(), Vec::new());
            for k in conflicts.keys() {
                let v = match (k.as_str(), picks.get(k)) {
                    ("deleted", pick) => {
                        let gone = match pick {
                            None => pre.deleted_at.is_some(),
                            Some(p) => p.as_bool().ok_or_else(|| {
                                DatumError::Other("deleted takes true or false".into())
                            })?,
                        };
                        match gone {
                            true => {
                                let at = pre.deleted_at.unwrap_or_else(Utc::now);
                                set.insert("deleted_at".into(), json!(at));
                            }
                            false => unset.push(json!("deleted_at")),
                        }
                        continue;
                    }
                    (_, Some(v)) => v.clone(),
                    (k, None) => match k.strip_prefix("data.") {
                        Some(dk) => shown.get("data").and_then(|d| d.get(dk)).cloned(),
                        None => shown.get(k).cloned(),
                    }
                    .unwrap_or(Value::Null),
                };
                match v {
                    Value::Null => unset.push(json!(k)),
                    v => {
                        set.insert(k.clone(), v);
                    }
                }
            }
            let picked = json!({ "set": set, "unset": unset });
            let mut r = Some(shown);
            fold_into(&mut r, &picked);
            let mut post = entry_from(r.unwrap_or_default())?;
            post.modify_time = Utc::now();
            history::record(tx, &pre, ChangeKind::UserEdit)?;
            apply_entry_state(tx, &post)?;
            let post = fetch_by_hid(tx, hid)?.unwrap_or(post);
            // The edit's own changes, and every contested key even where it
            // keeps the value: setting it is what says the edit saw each side.
            let mut effect = diff(&pre, &post);
            for (k, v) in set {
                effect["set"][k] = v;
            }
            for k in unset {
                match effect["unset"].as_array_mut() {
                    Some(a) if !a.contains(&k) => a.push(k),
                    Some(_) => {}
                    None => effect["unset"] = json!([k]),
                }
            }
            let kind = match effect.get("deleted").and_then(Value::as_bool) {
                Some(true) => "delete",
                Some(false) => "undelete",
                None => "update",
            };
            push_entry(tx, &ep, &post, kind, effect)?;
            refresh_conflicts(tx, hid)?;
            Ok(post)
        })
    }

    /// From here on this thread works on this file as the copy whose endpoint
    /// id is `id`, the one a sync session began as, or as any with `None`:
    /// once the file's key is another, or its schema another build's
    /// (da-1tnup), whatever of sync asks who this copy is fails, and writes
    /// nothing (da-otz0d).
    pub fn sync_session_as(&self, id: Option<&str>) {
        let path = self.conn.path().unwrap_or_default().to_string();
        SESSION.with(|s| *s.borrow_mut() = id.map(|id| (path, id.to_string())));
    }

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
    /// Refused, as a session's copy is, it changes nothing (da-otz0d).
    pub fn sync_rekey(&self) -> Result<String> {
        self.write_with(|tx| {
            // Blanked, not deleted: the pause is this copy's, whatever its key.
            tx.execute("UPDATE sync_local SET endpoint_secret = ''", [])?;
            Ok(endpoint(tx)?.id)
        })
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

    /// Sign a `baseline` create for at most `limit` of the entries no op
    /// covers, the lowest by hid, and say how many: a session signs a bulk
    /// import's in chunks, and its peer hears from it between them
    /// (da-f7mmn). A reader signs none, which its chain would refuse.
    pub fn sync_sign_baselines(
        &self,
        since: &BTreeMap<String, i64>,
        limit: Option<usize>,
    ) -> Result<usize> {
        pause::refuse_paused(&self.conn)?;
        debug::refuse(&self.conn)?;
        // Outside any transaction, where minting or moving its key on first
        // need waits for the lock as a write.
        let (mine, signed) = own_head(&self.conn)?;
        // A vector holding more of this copy's ops than it signed was made
        // by another copy signing with this key.
        if since.get(&mine).is_some_and(|s| *s > signed) {
            return Err(DatumError::Other(SHARED_KEY.into()));
        }
        if read_only(&self.conn, &endpoint(&self.conn)?)?.is_some() {
            return Ok(0);
        }
        // The baselines are the one write a serve makes, and take the lock
        // for themselves alone; what is served is read after, on a snapshot
        // that holds none (da-8i7f0).
        const UNCOVERED: &str =
            "SELECT * FROM entries WHERE hid NOT IN (SELECT hid FROM ops) ORDER BY hid LIMIT ?";
        let limit = limit.map_or(-1, |n| n as i64);
        if !self.conn.prepare(UNCOVERED)?.exists([limit])? {
            return Ok(0);
        }
        self.compute_then_write_with(
            |view| {
                view.conn()
                    .prepare(UNCOVERED)?
                    .query_map([limit], row_to_entry)?
                    .collect::<rusqlite::Result<Vec<Entry>>>()
                    .map_err(DatumError::from)
            },
            |tx, uncovered| {
                let ep = endpoint(tx)?;
                for e in &uncovered {
                    push_entry(tx, &ep, e, "baseline", create_effect(e))?;
                }
                Ok(uncovered.len())
            },
        )
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
        self.sync_ops_round(since, overlap, peer, None)
    }

    /// [`Db::sync_ops_since`], and with a `limit` only the `limit` of them
    /// lowest by HLC (da-ymgiv), the purged seqs aside: a copy's HLC passes
    /// every op it has applied, so each op's deps sort before it, and a
    /// round of them applies whole, which the next round continues. A
    /// filtered reader's lines are not cut.
    pub fn sync_ops_round(
        &self,
        since: &BTreeMap<String, i64>,
        overlap: bool,
        peer: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<String>> {
        self.sync_sign_baselines(since, None)?;
        let mine = endpoint(&self.conn)?;
        // A reader never relays: it serves its own ops alone.
        let reader = read_only(&self.conn, &mine)?.is_some();
        let mine = mine.id;
        self.read(|view| {
            let tx = view.conn();
            let chain = Chain::load(tx)?;
            // The seqs a purge order took (da-qzrj9) on one line after the
            // order, so a copy behind holds it when it reads them; and of a
            // dissolved chain nothing but they and the records.
            let dissolved = chain.dissolved().is_some();
            let served = Chain::served(tx)?;
            if let Some(filter) = peer.and_then(|p| served.filter(p)).filter(|_| !dissolved) {
                let (lines, _) =
                    filtered_lines(tx, since, filter, &chain, reader.then_some(&mine))?;
                return Ok(lines);
            }
            // A member's refused op goes too, for every copy to refuse
            // itself: the ops after it are counted past it. A non-member's
            // stays here.
            let serves = |e: &str, applied: i64| {
                !(reader && e != mine || applied == 2 && chain.member(e).is_none())
            };
            let lacks = |e: &str, seq: i64| seq > since.get(e).copied().unwrap_or(0);
            // The round's last HLC, read without the bodies: the value of the
            // nth lowest, which no order of equal HLCs changes, and the round is
            // every op at or below it, so ops that share it go together.
            let cut = match limit {
                Some(n) => {
                    let mut stmt = tx.prepare(
                        "SELECT endpoint, seq, applied, hid, hlc FROM ops WHERE applied IN (1, 2)",
                    )?;
                    let mut hlcs = Vec::new();
                    let mut rows = stmt.query([])?;
                    while let Some(r) = rows.next()? {
                        let (e, applied, hid): (String, i64, String) =
                            (r.get(0)?, r.get(2)?, r.get(3)?);
                        if lacks(&e, r.get(1)?)
                            && serves(&e, applied)
                            && (hid == CHAIN_HID || !dissolved)
                        {
                            hlcs.push(r.get::<_, String>(4)?);
                        }
                    }
                    (n > 0 && hlcs.len() > n).then(|| hlcs.select_nth_unstable(n - 1).1.clone())
                }
                None => None,
            };
            let (mut lines, mut purged, mut after) = (Vec::new(), Map::new(), 0);
            let mut stmt = tx.prepare(
                "SELECT endpoint, seq, body, applied, hid, hlc FROM ops WHERE applied > 0
             ORDER BY endpoint, seq",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?;
            for row in rows {
                let (e, seq, body, applied, hid, hlc) = row?;
                let sent = lacks(&e, seq)
                    || overlap && e == mine && since.get(&e) == Some(&seq) && !body.is_empty();
                let past = applied != 3 && cut.as_ref().is_some_and(|c| &hlc > c);
                if !sent || !serves(&e, applied) || past {
                    continue;
                }
                if applied == 3 {
                    let runs = purged.entry(e).or_insert_with(|| json!([]));
                    let runs = runs.as_array_mut().expect("an array");
                    match runs.last_mut() {
                        Some(r) if r[1] == seq - 1 => r[1] = seq.into(),
                        _ => runs.push(json!([seq, seq])),
                    }
                } else if hid == CHAIN_HID || !dissolved {
                    let wipe = |r: Value| r["effect"]["type"] == "wipe";
                    if hid == CHAIN_HID && serde_json::from_str(&body).is_ok_and(wipe) {
                        after = lines.len() + 1;
                    }
                    lines.push(body);
                }
            }
            if !purged.is_empty() {
                lines.insert(after, json!({ "purged": purged }).to_string());
            }
            drop(stmt);
            Ok(lines)
        })
    }

    /// Apply op lines. Each signature is checked (a bad one is refused and
    /// named), ops already held are skipped, and an op whose predecessor on
    /// its endpoint — or any op it depends on — is missing is held until it
    /// arrives. The rest apply in HLC order. An applied op writes
    /// `entry_history` (`sync_merge`) and emits no op of its own: it is that
    /// op. A line may also be one entry's ops as an array, or a filtered
    /// sender's removal or mark, which only a filtered reader acts on.
    /// It commits in chunks of [`APPLY_HOLD`], so one failing leaves the
    /// chunks before it applied: ops held are skipped when sent again.
    pub fn sync_apply(&self, lines: &str) -> Result<ApplySummary> {
        pause::refuse_paused(&self.conn)?;
        debug::refuse(&self.conn)?;
        let (words, mut ops): (Vec<&str>, Vec<&str>) = lines
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .partition(|l| is_word(l));
        // In settle's order, so a chunk waits on no later one.
        ops.sort_by_cached_key(|l| apply_order(l));
        // A wasm build is one process a command, and the page's tabs take
        // the file in turn: no writer waits beside it, so one transaction,
        // and no pause, which the shim would spin through.
        let wasm = cfg!(target_family = "wasm");
        let n = if wasm { usize::MAX } else { APPLY_MIN };
        let (mut s, mut n, mut rest) = (ApplySummary::default(), n, &ops[..]);
        loop {
            // A gap as long as the pause since the last commit was one.
            let run = match self.apply_run.get().filter(|_| !wasm) {
                Some((start, last)) if last.elapsed() < APPLY_YIELD => {
                    if start.elapsed() < APPLY_HOLD {
                        start
                    } else {
                        std::thread::sleep(APPLY_YIELD - last.elapsed());
                        Instant::now()
                    }
                }
                _ => Instant::now(),
            };
            let (chunk, more) = rest.split_at(n.min(rest.len()));
            // A mark answers the whole batch: it goes with the last chunk.
            let chunk = match more.is_empty() {
                true => [chunk, &words[..]].concat().join("\n"),
                false => chunk.join("\n"),
            };
            let t = Instant::now();
            // Chunked by the clock (da-2ejbs): what an op folds over is
            // its own entry's ops, read as it writes.
            let c = self.write_with(|tx| apply_in(tx, &chunk))?;
            self.apply_run.set(Some((run, Instant::now())));
            s.applied += c.applied;
            s.already_had += c.already_had;
            // What waits in the whole copy, so the last chunk's.
            s.held = c.held;
            s.conflicts += c.conflicts;
            s.refused.extend(c.refused);
            s.wiped |= c.wiped;
            rest = more;
            if rest.is_empty() {
                break;
            }
            // The next chunk sized by what this one took to what is left of
            // the run's hold, or a whole one after the pause it then takes,
            // at most twice this one: a chunk overshoots only by how much
            // dearer its ops are than the last chunk's, and ops grow dearer
            // as the database does, which shrinks the chunks.
            let left = APPLY_HOLD.saturating_sub(run.elapsed());
            let left = if left.is_zero() { APPLY_HOLD } else { left };
            let took = t.elapsed().as_micros().max(1);
            let next = n as u128 * left.as_micros() / took;
            n = next.clamp(APPLY_MIN as u128, 2 * n as u128) as usize;
        }
        if s.wiped {
            self.after_wipe()?;
        }
        Ok(s)
    }

    /// What a purge order took leaves no copy beside the file (da-qzrj9):
    /// the pre-migration snapshots go, as `db purge` removes them. The file
    /// is rewritten if it can be, another connection to it aside; what was
    /// freed was overwritten already ([`wipe`]).
    pub(crate) fn after_wipe(&self) -> Result<()> {
        for snapshot in self
            .path()
            .map(crate::migrations::pre_migration_backups)
            .unwrap_or_default()
        {
            std::fs::remove_file(&snapshot).map_err(|e| {
                DatumError::Other(format!("{} is not removed: {e}", snapshot.display()))
            })?;
        }
        // Each fails only while another connection holds the file, and the
        // purge stands: SQLite checkpoints at the last close.
        let _ = self.conn.execute_batch("VACUUM");
        let _ = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        Ok(())
    }

    /// The state hash: SHA-256 over the canonical JSON of every entry row,
    /// live and tombstoned, in `(slug, hid)` order, each with its
    /// `superseded_by` and any `conflicts` — so two copies agree only if they
    /// hold exactly the same data, bookkeeping included. The map is the same hash per column,
    /// to narrow a mismatch down. With a filtered reader on either side —
    /// this copy, or `peer` — only the entries that pass both filters count
    /// (da-u81u1), which both sides compute alike.
    ///
    /// Kept until an entry changes (`sync_state_hashes`, da-4be5x), so the
    /// sessions after the first that brought nothing hash nothing. A hash
    /// computed while another connection wrote, or on a read-only one, is
    /// not kept.
    pub fn sync_state_hash(
        &self,
        peer: Option<&str>,
    ) -> Result<(String, BTreeMap<String, String>)> {
        self.sync_state_hash_as(None, peer)
    }

    /// [`Db::sync_state_hash`] as the copy whose endpoint id is `me`: a
    /// connection on another thread than the session's, which hashes while
    /// the session sends (da-0mwws), cannot tell it. Its endpoint is the
    /// invocation's, and there it would be another copy's.
    pub fn sync_state_hash_as(
        &self,
        me: Option<&str>,
        peer: Option<&str>,
    ) -> Result<(String, BTreeMap<String, String>)> {
        let Some(filters) = shared_filters(&self.conn, me, peer)? else {
            return Ok(state_hash(Vec::new()));
        };
        // A build may serialize a row differently: it reads its own.
        let key = json!([
            crate::build(),
            filters.iter().map(|f| &f.fields).collect::<Vec<_>>()
        ])
        .to_string();
        let (hashed, keep) = self.read(|view| {
            let kept = view
                .conn()
                .query_row(
                    "SELECT hash, per_key FROM sync_state_hashes WHERE key = ?1",
                    [&key],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )
                .optional()?
                .and_then(|(hash, per_key)| Some((hash, serde_json::from_str(&per_key).ok()?)));
            if let Some(kept) = kept {
                return Ok((kept, None));
            }
            let (hash, per_key) = state_hash(rows_passing(view.conn(), &filters)?);
            let mut keep = WriteBatch::on(view.seen());
            keep.push(
                "INSERT OR REPLACE INTO sync_state_hashes (key, hash, per_key) VALUES (?1, ?2, ?3)",
                vec![
                    key.clone().into(),
                    hash.clone().into(),
                    json!(per_key).to_string().into(),
                ],
            );
            Ok::<_, DatumError>(((hash, per_key), Some(keep)))
        })?;
        if let Some(keep) = keep {
            let _ = self.write(&keep);
        }
        Ok(hashed)
    }
}

/// SHA-256 over the canonical JSON of `rows`, and the same per column.
fn state_hash(rows: Vec<Map<String, Value>>) -> (String, BTreeMap<String, String>) {
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
    (sha256_hex(&canonical_json(&all)), per_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The code a row and its `conflicts` are folded by, against what it was
    /// at [`FOLD_RULE`] (da-sqpjt). A change to it fails here: if the same
    /// ops now fold to another row, bump `FOLD_RULE`, so every copy refolds
    /// as it opens; then record the hash beside the rule either way.
    ///
    /// The code is the items `ITEMS` names, each from its line to the first
    /// line that is `;` ended (a const) or a `}` in the first column, which
    /// rustfmt writes only where a top-level item ends. A line that is a
    /// comment is left out; a comment after code on its line is not. What
    /// it does not name it does not see: a helper a listed item comes to
    /// call joins `ITEMS`, and a renamed item is renamed there.
    #[test]
    fn the_fold_rule_is_fingerprinted() {
        const RECORDED: (u32, &str) = (
            3,
            "14354a4eff02e6bd173257b165f9e83484c5002253dacd6f0e14bca7277c3ae1",
        );
        const ITEMS: &[&str] = &[
            "fn fold_into",
            "fn values",
            "const NOT_A_CONFLICT",
            "impl Op",
            "fn fold",
            "fn entry_from",
            "const PROVENANCE",
            "fn contestable",
            "fn settles",
            "fn intents",
            "fn automatic",
            "fn contention",
            "fn conflicts_of",
            "fn about",
            "fn side",
            "fn refresh_conflicts",
            "fn content",
            "fn own",
            "const TAKEN_BACK",
            "fn merged_into",
            "fn merge_rows",
            "fn merge_from",
            "fn derived",
            "fn rederive",
            "fn write_merged",
            "fn met",
            "fn refold",
        ];
        let source = include_str!("sync.rs");
        let mut code = String::new();
        for item in ITEMS {
            let start = source
                .lines()
                .position(|l| {
                    let l = l
                        .trim_start_matches("pub(super) ")
                        .trim_start_matches("pub(crate) ");
                    l.strip_prefix(item)
                        .is_some_and(|rest| rest.starts_with([' ', '(', ':', '<']))
                })
                .unwrap_or_else(|| {
                    panic!("`{item}` is not in sync.rs: name what replaced it in ITEMS")
                });
            let end = if item.starts_with("const") { ";" } else { "}" };
            for line in source.lines().skip(start) {
                if !line.trim_start().starts_with("//") {
                    code.push_str(line);
                    code.push('\n');
                }
                if line == end || end == ";" && line.ends_with(';') {
                    break;
                }
            }
        }
        let now = (FOLD_RULE, sha256_hex(&code));
        assert_eq!(
            (now.0, now.1.as_str()),
            RECORDED,
            "the fold's code changed: if the same ops fold to another row now, bump FOLD_RULE; \
             then record {now:?} as RECORDED"
        );
    }

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

    /// A rekey a session's copy is refused leaves its key (da-otz0d).
    #[test]
    fn a_rekey_refused_a_sessions_copy_changes_nothing() {
        let db = Db::open_in_memory().unwrap();
        let id = db.sync_endpoint_id().unwrap();
        db.sync_session_as(Some(&id));
        assert!(db.sync_rekey().is_err());
        assert_eq!(db.sync_endpoint_id().unwrap(), id);
        db.sync_session_as(None);
        assert_ne!(db.sync_rekey().unwrap(), id);
    }

    #[test]
    fn canonical_json_sorts_keys_at_every_depth() {
        let v = json!({"b": 1, "a": {"d": [{"f": 1, "e": 2}], "c": null}});
        assert_eq!(
            canonical_json(&v),
            r#"{"a":{"c":null,"d":[{"e":2,"f":1}]},"b":1}"#
        );
    }

    /// Its scalars are written as `Value` writes them, the bytes every op
    /// so far was signed over (da-f4vrr wrote it without the sorted copy).
    #[test]
    fn canonical_json_writes_scalars_as_value_does() {
        let v: Value = serde_json::from_str(
            r#"{"z":[1.5,-0.0,1e300,18446744073709551615,-9223372036854775808,
                "é\"\\\n\u0001😀",true,{},[]],"a":"ü"}"#,
        )
        .unwrap();
        assert_eq!(canonical_json(&v), v.to_string());
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

    // ─── A fast clock (da-ns8dq) ────────────────────────────────────────────

    /// Past a year by a margin: at exactly 365 days the wait read «365 days»
    /// only because a millisecond passed, and «1 year» when none did.
    const YEAR: i64 = 400 * 24 * 3600 * 1000;

    /// Log an entry on `db` with its clock `skew` ms fast; its ops as lines.
    fn log_at(db: &Db, skew: i64, field: &str) -> Vec<String> {
        SKEW.with(|s| s.set(skew));
        let e = Entry::new(
            field,
            crate::entry::Verb::Point,
            None,
            None,
            None,
            None,
            "t",
        );
        db.insert_entry(&e).unwrap();
        SKEW.with(|s| s.set(0));
        db.sync_ops_since(&BTreeMap::new(), false, None).unwrap()
    }

    fn apply_at(db: &Db, skew: i64, lines: &[String]) -> ApplySummary {
        SKEW.with(|s| s.set(skew));
        let s = db.sync_apply(&lines.join("\n")).unwrap();
        SKEW.with(|c| c.set(0));
        s
    }

    /// The HLC of the latest op `db` signed itself, as ms.
    fn own_latest(db: &Db) -> i64 {
        let (mine, _) = own_head(&db.conn).unwrap();
        let hlc: String = db
            .conn
            .query_row(
                "SELECT MAX(hlc) FROM ops WHERE endpoint = ?1",
                [mine],
                |r| r.get(0),
            )
            .unwrap();
        parse_hlc(&hlc).unwrap().0
    }

    #[test]
    fn an_op_from_a_clock_a_year_ahead_waits_and_does_not_pin_this_clock() {
        let (a, b, c) = (
            Db::open_in_memory().unwrap(),
            Db::open_in_memory().unwrap(),
            Db::open_in_memory().unwrap(),
        );
        let from_a = log_at(&a, YEAR, "coffee");
        let s = apply_at(&b, 0, &from_a);
        assert_eq!((s.applied, s.held), (0, from_a.len()));
        let waits = b.sync_clock_waits().unwrap();
        assert_eq!(waits.len(), 1);
        assert_eq!(waits[0].endpoint, own_head(&a.conn).unwrap().0);
        assert_eq!(waits[0].ops, from_a.len());
        assert!(
            waits[0].to_string().contains("1 change from")
                && waits[0].to_string().contains("its clock is 1 year ahead")
        );

        let from_b = log_at(&b, 0, "tea");
        assert!(own_latest(&b) <= now_ms(), "b's op is stamped at a's year");

        // B's clock passes the op's HLC less the limit: the next apply
        // applies it, and B folds as a copy that applied it at once.
        let late = YEAR - MAX_CLOCK_AHEAD.as_millis() as i64 + 60_000;
        let s = apply_at(&b, late, &[]);
        assert_eq!((s.applied, s.held), (from_a.len(), 0));
        assert!(b.sync_clock_waits().unwrap().is_empty());
        apply_at(&c, late, &from_a);
        apply_at(&c, late, &from_b);
        assert_eq!(
            b.sync_state_hash(None).unwrap().0,
            c.sync_state_hash(None).unwrap().0
        );
    }

    /// The state hash is kept until an entry changes (da-4be5x): a verify
    /// after one reads the kept hash, and an insert, an update or a delete
    /// of any entry row forgets it.
    #[test]
    fn the_state_hash_is_kept_until_an_entry_changes() {
        let db = Db::open_in_memory().unwrap();
        log_at(&db, 0, "coffee");
        let plant = |db: &Db| {
            db.sync_state_hash(None).unwrap();
            db.conn
                .execute("UPDATE sync_state_hashes SET hash = 'kept'", [])
                .unwrap();
            assert_eq!(db.sync_state_hash(None).unwrap().0, "kept");
        };
        for change in ["UPDATE entries SET state = '\"x\"'", "DELETE FROM entries"] {
            plant(&db);
            db.conn.execute(change, []).unwrap();
            let fresh = state_hash(shared_rows(&db.conn, None, None).unwrap().0);
            assert_eq!(db.sync_state_hash(None).unwrap(), fresh, "after {change}");
        }
        plant(&db);
        log_at(&db, 0, "tea");
        assert_ne!(
            db.sync_state_hash(None).unwrap().0,
            "kept",
            "after an insert"
        );
    }

    /// An op names who and what signed it (da-zjc9w), and the build before
    /// origins reads it: that build's `Op`, this one without `origin`, denies
    /// no key it does not know, and a signature is checked over the body as
    /// sent, so the op verifies, applies and is served on whole.
    #[test]
    fn a_build_before_origins_reads_an_op_carrying_one() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Before {
            endpoint: String,
            seq: i64,
            hlc: String,
            hid: String,
            slug: String,
            kind: String,
            #[serde(default)]
            field: Option<String>,
            #[serde(default)]
            raw: Option<String>,
            #[serde(default)]
            alias_chain: Option<Value>,
            effect: Value,
            #[serde(default)]
            deps: BTreeMap<String, i64>,
            #[serde(default)]
            queued: Option<String>,
            sig: String,
        }
        let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
        let lines = log_at(&a, 0, "coffee");
        for line in &lines {
            let op: Value = serde_json::from_str(line).unwrap();
            assert_eq!(op["origin"]["program"], "datum", "{line}");
            assert_eq!(op["origin"]["version"], crate::build(), "{line}");
            serde_json::from_value::<Before>(op).expect("the build before reads it");
        }
        assert_eq!(apply_at(&b, 0, &lines).applied, lines.len());
        let served = b.sync_ops_since(&BTreeMap::new(), false, None).unwrap();
        assert!(lines.iter().all(|l| served.contains(l)), "{served:?}");
    }

    /// A round with a limit is the ops lowest by HLC (da-ymgiv): each
    /// applies whole, and the next round goes on from the vector it left.
    #[test]
    fn rounds_of_a_limit_apply_whole_and_continue_from_the_vector() {
        let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
        // Two endpoints, each op made after the other's last: a cut by
        // endpoint and seq would send ops whose deps it left out.
        let c = Db::open_in_memory().unwrap();
        for (i, f) in ["a", "b", "c", "d", "e", "f", "g"].into_iter().enumerate() {
            let (from, to) = if i % 2 == 0 { (&a, &c) } else { (&c, &a) };
            let lines = log_at(from, 0, f);
            apply_at(to, 0, &lines);
        }
        let all = a.sync_ops_since(&BTreeMap::new(), false, None).unwrap();
        let mut rounds = 0;
        loop {
            let since = b.sync_vector().unwrap();
            let round = a.sync_ops_round(&since, false, None, Some(3)).unwrap();
            if round.is_empty() {
                break;
            }
            assert!(round.len() <= 3, "{round:?}");
            let s = apply_at(&b, 0, &round);
            assert_eq!((s.applied, s.held), (round.len(), 0), "{round:?}");
            rounds += 1;
        }
        assert_eq!(rounds, all.len().div_ceil(3));
        assert_eq!(
            a.sync_state_hash(None).unwrap(),
            b.sync_state_hash(None).unwrap()
        );
    }

    #[test]
    fn an_op_a_minute_ahead_applies_at_once() {
        let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
        let from_a = log_at(&a, 60_000, "coffee");
        let s = apply_at(&b, 0, &from_a);
        assert_eq!((s.applied, s.held), (from_a.len(), 0));
        assert!(b.sync_clock_waits().unwrap().is_empty());
    }

    #[test]
    fn a_refused_op_does_not_move_the_clock() {
        let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
        apply_at(&b, 0, &log_at(&a, YEAR, "coffee"));
        b.conn.execute("UPDATE ops SET applied = 2", []).unwrap();
        log_at(&b, 0, "tea");
        assert!(own_latest(&b) <= now_ms());
    }
}
