//! Easy Sync's session protocol (da-rugg8, split from storage in da-6kpj5):
//! two copies of a database sync over a QUIC connection. The merge is
//! datum-core's; this crate moves the ops, over a [`Store`] that holds them
//! — `datum-sync`'s over `Db` natively, the CLI's in datum.day's sync worker
//! (`web`). It builds for wasm32-unknown-unknown, where iroh is relay-only.
//!
//! - **Endpoint.** The iroh endpoint's key IS the db's endpoint key, so the
//!   iroh endpoint id is the `endpoint` every op of this copy carries. ALPN
//!   `datum/sync/2`, and a [`Net`]: n0's relays and discovery, one relay of
//!   the caller's (`DATUM_SYNC_RELAY`, the tests' local one), or natively
//!   one bound address with neither (`DATUM_SYNC_BIND`).
//! - **Wire.** One bidirectional stream of frames, each a big-endian u32
//!   length and its bytes. The dialer sends a [`Hello`], the acceptor a
//!   [`Reply`], each after the store's [`Store::handshake`] has admitted the
//!   other. An acceptor that has not heard of the dialer asks first for its
//!   chain's records ([`Reply::Prove`]), and admits it if they make it a
//!   member (da-2u95o). Then rounds: each side sends its version vector, then the ops
//!   the other lacks, one frame each, and an empty frame, and applies what
//!   came — until a round in which neither vector moved. A round carries at
//!   most a [`ROUND`] of ops, the lowest by HLC, so a big catch-up moves in
//!   rounds that each apply before the next is sent (da-ymgiv). A copy that
//!   applied a full round it mostly held already, another session's, sends
//!   [`END`] for its next vector, and both sides end there (da-3l8ay). Last
//!   the verify hashes, the dialer's first, and the dialer closes.
//! - **Diff.** ALPN `datum/sync-diff/1` (da-n0ruf), which a copy before it
//!   refuses: after the same handshake the dialer sends its state hash and
//!   the acceptor its [`Digest`], or null when the hashes agree; then the
//!   dialer the hids that differ and the acceptor what it holds of each
//!   ([`Store::held`]). No op moves, and no session is recorded.
//! - **Record.** Every session this copy dials, and every one it serves past
//!   the handshake, is recorded as it ends — the peer's vector, the verify,
//!   or the error it failed with — for `datum sync status` (da-rcb5m).
//! - **Members.** Each side says in its handshake where it last saw the
//!   chain's members, and the other keeps what it lacks once the session
//!   has synced (da-fly3f): two copies one inviter admitted have never
//!   dialed each other, and this is how each learns where the other is.
//! - **Progress.** A session fails only when no frame has moved either way
//!   for [`idle`] (da-039jx). A long apply is chunked, and between chunks
//!   the side applying sends a [`BUSY`] frame, which a reader skips: a
//!   first sync of 32k ops takes minutes and is not a dead peer. A store
//!   whose apply runs elsewhere ([`Store::yields`], a browser's CLI) is sent
//!   BUSYs while it runs too, and its chunks double (da-89ujs): each is a
//!   process that commits, and a commit rewrites every index page the chunk
//!   touched, however few ops it held. So is the side verifying, whose hash
//!   of a big history takes minutes (da-0mwws), and the side signing the
//!   baselines a bulk import left, in chunks sized as an apply's, before a
//!   round's ops to a peer whose handshake says it `waits` (da-f7mmn).
//! - **One applies.** Every session of a copy runs on one thread, so a chunk
//!   that does not yield is sized by the clock and the apply yields between
//!   chunks; and one session applies at a time ([`Store::apply_gate`]),
//!   which a copy holding it answers a new one with: [`Why::Busy`], no
//!   failure on either side (da-h31wt).
//! - **Versions.** Two copies one version apart is the ordinary case:
//!   datum.day deploys on its schedule, a person's CLI updates on theirs. So
//!   a change to a frame is additive (da-2bnr6): a new field is optional and
//!   defaulted, a reader takes the old shape and the new (no
//!   `deny_unknown_fields`), and a new message or variant goes only to a copy
//!   that said it reads it, as `typed` and `proves` say. A change that can't
//!   be both bumps the ALPN, and two copies that speak different ones are
//!   told so in words, [`Failure::Protocol`]. `tests/frames.json` holds one
//!   frame of each kind as the previous version wrote it: this version must
//!   read each, and write back every field it carried, or the test fails
//!   until the ALPN is bumped on purpose. A kind or a field this version
//!   writes and the file lacks fails as well (da-igio7), so the file holds
//!   all the deployed version writes and a field is kept from its first
//!   release. `DATUM_SYNC_RECORD_FRAMES=1 cargo test -p datum-sync-wire
//!   frames` records them again — with a bump, or for a new kind or field,
//!   when the file's diff only adds. The op lines a
//!   round carries and the rows [`Store::held`] returns are datum-core's,
//!   opaque here.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::pin;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use data_encoding::BASE32_NOPAD;
use futures_util::future::{select, Either};
use iroh::endpoint::{
    presets, Connection, ConnectionError, Incoming, RecvStream, SendStream, TransportErrorCode,
};
pub use iroh::{Endpoint, EndpointAddr};
use iroh::{RelayMode, SecretKey, TransportAddr, Watcher};
use n0_future::time::{sleep, timeout};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub mod web;

pub const ALPN: &[u8] = b"datum/sync/2";
/// `datum sync diff`: what two copies hold differently, read-only.
const DIFF: &[u8] = b"datum/sync-diff/1";
const MAX_FRAME: usize = 64 << 20;
const MAX_HELLO: usize = 64 << 10;
/// A chain's records, from a dialer not yet admitted.
const MAX_RECORDS: usize = 1 << 20;
/// A session in which both sides keep writing still ends: after this many
/// rounds that were not full.
const MAX_ROUNDS: usize = 16;
/// Before its hash, from a copy whose state moved after the last round: a
/// write or another session's apply landed under the verify, so the hash is
/// not of the vector the peer judges it by (da-o2arv). A copy before it reads
/// a hash that differs, a MISMATCH, as it always did.
const MOVED: &str = "moved ";

/// The most ops a round sends (da-ymgiv): what a session cut short loses,
/// and what each side holds of it at once. A full round has more behind it,
/// and the session goes on while rounds are full, however many.
const ROUND: usize = 8192;

/// [`ROUND`], or `DATUM_SYNC_TEST_ROUND` (the tests).
fn round() -> usize {
    env("DATUM_SYNC_TEST_ROUND")
        .filter(|&n| n > 0)
        .unwrap_or(ROUND)
}

/// Ops per apply — the first, where a store [`yields`](Store::yields).
const CHUNK: usize = 64;
/// Ops in a session's first apply where the store does not yield: that
/// chunk holds every session's thread before anything has timed one, so it
/// is small, and [`pace`] sizes the rest (da-10nno).
const FIRST: usize = 8;
/// The most ops a doubling chunk grows to: one CLI process applying 8,192
/// ops peaked at 108 MiB, and a command typed meanwhile waits for it.
const MAX_CHUNK: usize = 8192;
/// The most addresses a handshake passes on: it is one [`MAX_HELLO`] frame.
const MAX_KNOWN: usize = 64;
/// datum-core's `peers::COPY`, which this crate cannot see.
const COPY: &str =
    "this peer is a copy of this database (same endpoint key); run `datum sync chain rekey` on one of them";

pub type Vector = BTreeMap<String, i64>;
/// Per hid, the hash of the entry's row.
pub type Digest = BTreeMap<String, String>;

/// What applying a chunk of ops did.
pub struct Applied {
    pub applied: usize,
    pub conflicts: usize,
    /// Ops refused, each named with the reason.
    pub refused: Vec<String>,
}

/// Where a copy's ops live, and its local record of peers and sessions.
/// Not `Send`: a session runs on one thread, the browser's or the native
/// runtime's `block_on`.
#[allow(async_fn_in_trait)]
pub trait Store {
    /// How this copy names itself to a peer.
    fn label(&self) -> String;
    /// The sync chain this copy is in, and its unrevoked members; none
    /// when it is in no chain.
    async fn chain(&self) -> Result<Option<(String, usize)>>;
    /// Leave this copy's chain to join another, keeping its key, or with
    /// `dry_run` only say why it may not: only a copy holding nothing but
    /// its own data may (da-h7pnc).
    async fn leave(&self, dry_run: bool) -> Result<()>;
    async fn vector(&self) -> Result<Vector>;
    /// Every op `since` lacks, as JSON lines — for `peer`, whose filter, if
    /// it is a filtered reader, the lines pass (da-u81u1).
    /// At most `limit` of them, the lowest by HLC (da-ymgiv), a round that
    /// applies whole: each op's deps sort before it.
    async fn ops_since(&self, peer: &str, since: &Vector, limit: usize) -> Result<Vec<String>>;
    /// Sign at most `n` of the entries no op covers as this copy's
    /// baselines, and say how many (da-f7mmn). One that signs none here
    /// leaves them to [`ops_since`](Store::ops_since), all at once.
    async fn sign(&self, _since: &Vector, _n: usize) -> Result<usize> {
        Ok(0)
    }
    async fn apply(&self, ops: &[String]) -> Result<Applied>;
    /// Whether [`apply`](Store::apply) awaits work done off this thread,
    /// so that a session can send meanwhile: a browser's CLI process.
    fn yields(&self) -> bool {
        false
    }
    /// `done` of a session's `of` received ops are applied.
    fn progress(&self, _done: usize, _of: usize) {}
    /// The peer filled the round received: the session has more rounds,
    /// though its vector, swapped before an import's baselines are signed,
    /// may not say how many ops (da-y8yde).
    fn more(&self) {}
    /// Whether a session holds this copy's [`apply_gate`](Store::apply_gate),
    /// in this process or another: a session begun meanwhile is refused as
    /// [`Why::Busy`], and would only bring the same ops again (da-h31wt).
    fn applying(&self) -> bool {
        false
    }
    /// Whether a dial is refused as busy before it connects: while this copy
    /// is [`applying`](Store::applying), unless its dials wait a turn of
    /// their own first.
    fn dial_busy(&self) -> bool {
        self.applying()
    }
    /// Wait until no other session applies into this copy, and hold it
    /// until what this returns drops: one applies at a time, and its writes
    /// wait out no other's.
    fn apply_gate(&self) -> impl Future<Output = Result<impl Sized>> {
        async { Ok(()) }
    }
    /// Round `n` of a session sends ops, `left` of them from here in all:
    /// exact in a round short of [`ROUND`], the last, and in a full one as
    /// many as this copy's vector holds past the peer's (da-ija0z).
    fn round(&self, _n: usize, _left: usize) {}
    /// Round `n` reads what it sends, `signed` of an import's baselines
    /// signed so far: the slow part of a first round (da-la01w).
    fn preparing(&self, _n: usize, _signed: usize) {}
    /// `done` of the `of` ops a round sends are on the wire.
    fn sent(&self, _done: usize, _of: usize) {}
    /// `n` of the ops a round receives have come.
    fn got(&self, _n: usize) {}
    /// A session served to `peer` begins (`on`), or ends: a copy serving
    /// one need not dial it as well (da-ymgiv).
    fn serving(&self, _peer: &str, _on: bool) {}
    /// Whether a session served to `peer` is open now.
    fn serves(&self, _peer: &str) -> bool {
        false
    }
    /// A session this copy dialed to `peer` passed its handshake (`on`), or
    /// ended: a dial from the peer meanwhile is refused as busy (da-mghh3).
    fn carrying(&self, _peer: &str, _on: bool) {}
    /// Whether a session this copy dialed to `peer` is past its handshake.
    fn carries(&self, _peer: &str) -> bool {
        false
    }
    /// The session's ops are all applied, and its verify begins.
    fn verifying(&self) {}
    /// The state hash, over what a session with `peer` shares. One whose
    /// work is done off this thread lets the session send meanwhile.
    async fn verify(&self, peer: &str) -> Result<String>;
    /// That state hash, and each row's own by hid (da-n0ruf).
    async fn digest(&self, peer: &str) -> Result<(String, Digest)>;
    /// What this copy holds of each of `hids`, as it shares it with `peer`:
    /// its row and its ops.
    async fn held(&self, peer: &str, hids: &[String]) -> Result<Vec<serde_json::Value>>;
    /// `peer` introduced itself as `label` at `addr`: record it, and when
    /// it `dialed` this copy, admit it (as a peer, or by `invite`) or fail
    /// saying why not; a redeemer names its own `chain`, and how many
    /// entries it `holds`. When this copy dialed it, `invite` is the one
    /// it just redeemed there. Whether it is new to this copy comes back.
    /// Admitted, what it said it runs is recorded, and a copy behind it
    /// warns to upgrade — and syncs on (da-3zqyv). A dialer this copy's
    /// chain does not hold fails as [`Why::NotMember`]; asked again with
    /// its `records`, it is admitted if they make it a member (da-2u95o).
    #[allow(clippy::too_many_arguments)]
    async fn handshake(
        &self,
        peer: &str,
        label: &str,
        addr: &EndpointAddr,
        invite: Option<&str>,
        chain: Option<&str>,
        holds: u64,
        version: &Version,
        records: Option<&[String]>,
        dialed: bool,
    ) -> Result<bool>;
    /// This copy's chain records, as op lines: what it shows a member that
    /// has not heard it joined (da-2u95o).
    async fn records(&self) -> Result<Vec<String>>;
    /// How many entries this copy holds, as `datum sync holds` counts them
    /// (life data: no spec, layout or alias): a reader invite refuses a
    /// redeemer holding any, which a reader's fold would hide (da-tti3w).
    async fn holds(&self) -> Result<u64>;
    /// `met`: the members' addresses `peer` passed on, to keep for each
    /// member this copy has not itself seen (da-fly3f). `offline`: the
    /// session failed before it reached `peer` (da-ueb4p).
    async fn record_session(
        &self,
        peer: &str,
        vector: Option<&Vector>,
        verify: Option<bool>,
        error: Option<&anyhow::Error>,
        offline: bool,
        met: &[EndpointAddr],
    ) -> Result<()>;
    /// Where this copy last saw the members of its chain, to pass on.
    async fn known(&self) -> Result<Vec<EndpointAddr>>;
    /// What this copy runs, said in its handshake (da-3zqyv).
    fn version(&self) -> Version;
}

fn short(id: &str) -> &str {
    &id[..id.len().min(10)]
}

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

/// [`idle`]'s seconds where there is no environment to name them: datum.day's
/// tests, through `web::Node::bind` (da-eqjri). 0 is unset. One for the
/// process, as the environment is: every node a page binds is the page's,
/// and so its test's.
static IDLE_S: AtomicU64 = AtomicU64::new(0);

/// How long a session may go with no frame moving either way
/// (`DATUM_SYNC_TIMEOUT_S` or [`IDLE_S`], for the tests).
fn idle() -> Duration {
    idle_of(IDLE_S.load(Relaxed))
}

fn idle_of(set: u64) -> Duration {
    let set = Some(set).filter(|&s| s > 0);
    Duration::from_secs(set.or_else(|| env("DATUM_SYNC_TIMEOUT_S")).unwrap_or(60))
}

async fn within<T>(f: impl Future<Output = Result<T>>) -> Result<T> {
    let limit = idle();
    timeout(limit, f).await.map_err(|_| {
        let why = format!("timed out: nothing moved for {}s", limit.as_secs());
        Failed(Failure::TimedOut, why)
    })?
}

// ─── Wire ───────────────────────────────────────────────────────────────────

/// Where datum.day redeems a ticket (da-l9kdr): the ticket rides in the
/// fragment, which a browser never sends to a server.
pub const JOIN: &str = "https://datum.day/sync/join#";

/// A one-time invite: where to dial — the inviter first, then the peers it
/// knows, any of which can admit the redeemer in a chain — and the secret.
#[derive(Serialize, Deserialize)]
pub struct Ticket {
    pub addrs: Vec<EndpointAddr>,
    pub secret: String,
    #[serde(flatten)]
    pub about: About,
}

/// What a ticket says of itself, for a redeemer to ask before it joins
/// (da-l9kdr): the inviting copy, its chain, and when the invite expires
/// (unix seconds). A ticket made before these were carried has none.
#[derive(Serialize, Deserialize, Default)]
pub struct About {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<u64>,
}

/// Seconds since the epoch, in a browser too.
pub fn unix_now() -> u64 {
    use n0_future::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Ticket {
    pub fn encode(&self) -> Result<String> {
        Ok(BASE32_NOPAD.encode(&serde_json::to_vec(self)?))
    }

    /// The join link a browser opens, and `connect` takes as well.
    pub fn link(&self) -> Result<String> {
        Ok(format!("{JOIN}{}", self.encode()?))
    }

    /// A ticket, or a join link holding one (whatever precedes its `#`).
    pub fn decode(s: &str) -> Result<Self> {
        let s = s.rsplit('#').next().unwrap_or(s);
        let bytes = BASE32_NOPAD
            .decode(s.trim().to_uppercase().as_bytes())
            .context("not a datum sync ticket")?;
        serde_json::from_slice(&bytes).context("not a datum sync ticket")
    }
}

/// What a copy runs (da-3zqyv): its datum build and its schema version.
/// Both are missing from a copy before it, and read as `""` and 0.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Version {
    #[serde(default)]
    pub datum: String,
    #[serde(default)]
    pub schema: u32,
}

#[derive(Serialize, Deserialize)]
struct Hello {
    label: String,
    addr: EndpointAddr,
    invite: Option<String>,
    /// A redeemer's own chain, empty for none (da-mepav).
    chain: Option<String>,
    /// A redeemer's entries, as `datum sync holds` counts them (da-tti3w).
    #[serde(default)]
    holds: u64,
    /// The dialer answers a [`Reply::Prove`], and reads [`Why::NotMember`];
    /// a copy before da-2u95o does neither.
    #[serde(default)]
    proves: bool,
    /// The dialer reads a [`Reply::Refusal`]; one that does not say so, a
    /// copy before da-b6xh6, is refused the old way.
    #[serde(default)]
    typed: bool,
    /// The dialer reads [`Why::Busy`]; a copy before da-h31wt does not.
    #[serde(default)]
    busy: bool,
    /// Where the dialer last saw the chain's members (da-fly3f); a redeemer,
    /// not yet one of them, says none.
    #[serde(default)]
    known: Vec<EndpointAddr>,
    /// The most ops a round of the dialer's carries ([`ROUND`]); a copy
    /// before da-ymgiv says none, and ends after [`MAX_ROUNDS`] rounds.
    #[serde(default)]
    round: usize,
    /// The dialer skips a [`BUSY`] ahead of a round's ops, which a copy
    /// signing baselines sends (da-f7mmn); a copy before it does not.
    #[serde(default)]
    waits: bool,
    /// The dialer reads an [`END`] for a round's vector (da-3l8ay); a copy
    /// before it does not, and is never sent one.
    #[serde(default)]
    ends: bool,
    #[serde(flatten)]
    version: Version,
}

#[derive(Serialize, Deserialize)]
enum Reply {
    Welcome {
        label: String,
        addr: EndpointAddr,
        /// Where the acceptor last saw the chain's members (da-fly3f).
        #[serde(default)]
        known: Vec<EndpointAddr>,
        /// As [`Hello`]'s.
        #[serde(default)]
        round: usize,
        /// As [`Hello`]'s.
        #[serde(default)]
        waits: bool,
        /// As [`Hello`]'s.
        #[serde(default)]
        ends: bool,
        #[serde(flatten)]
        version: Version,
    },
    /// A refusal's words alone: to and from a copy before da-b6xh6.
    Refused(String),
    Refusal {
        why: String,
        kind: Why,
    },
    /// The acceptor has not heard of the dialer: the dialer sends its
    /// chain's records, a frame holding a list of op lines, and reads the
    /// reply to them (da-2u95o).
    Prove,
}

impl Reply {
    /// Refuse the dialer of `hello` with `kind` and `why`, in the form it
    /// reads: a copy before da-2u95o knows no [`Why::NotMember`].
    fn refusal(hello: &Hello, kind: Why, why: String) -> Self {
        match (hello.typed, kind) {
            (false, _) => Reply::Refused(why),
            (true, Why::NotMember) if !hello.proves => Reply::Refusal {
                why,
                kind: Why::Other,
            },
            (true, Why::Busy) if !hello.busy => Reply::Refusal {
                why,
                kind: Why::Other,
            },
            (true, kind) => Reply::Refusal { why, kind },
        }
    }
}

/// A refusal's [`Failed`]. An older acceptor's has words alone; the one
/// kind a redeemer routes on, try the next address, is read from them as it
/// was before da-b6xh6, and any other is [`Why::Other`]. The words are
/// datum-core's `UNKNOWN_INVITE` as such a copy sends them: a suffix, since
/// an older datum.day sends «Error: unknown invite» (da-16d3s).
fn refused(why: String, kind: Option<Why>) -> anyhow::Error {
    let kind = kind.unwrap_or(match why.ends_with("unknown invite") {
        true => Why::UnknownInvite,
        false => Why::Other,
    });
    Failed(Failure::Refused(kind), format!("refused: {why}")).into()
}

/// Sent between the chunks of a long apply; progress, and otherwise ignored.
/// No JSON frame and no op line is a NUL byte, and it is not the empty frame.
const BUSY: &[u8] = &[0];
/// Sent for a round's vector by a copy that applied a full round it mostly
/// held already: two members sending one copy a catch-up at once read its
/// vector before either applied, and send it the same ops, round after round.
/// The session ends there, and the other carries the catch-up (da-3l8ay).
/// No vector is it.
const END: &[u8] = b"end";

async fn put(s: &mut SendStream, b: &[u8]) -> Result<()> {
    within(async {
        s.write_all(&(b.len() as u32).to_be_bytes()).await?;
        s.write_all(b).await?;
        Ok(())
    })
    .await
}

async fn get(r: &mut RecvStream, max: usize) -> Result<Vec<u8>> {
    within(async {
        let mut n = [0u8; 4];
        r.read_exact(&mut n).await?;
        let n = u32::from_be_bytes(n) as usize;
        ensure!(n <= max, "frame of {n} bytes is over the limit");
        let mut b = vec![0u8; n];
        r.read_exact(&mut b).await?;
        Ok(b)
    })
    .await
}

async fn put_json(s: &mut SendStream, v: &impl Serialize) -> Result<()> {
    put(s, &serde_json::to_vec(v)?).await
}

/// The next frame that is not a [`BUSY`].
async fn get_json<T: DeserializeOwned>(r: &mut RecvStream, max: usize) -> Result<T> {
    loop {
        let f = get(r, max).await?;
        if f != BUSY {
            return Ok(serde_json::from_slice(&f)?);
        }
    }
}

// ─── The session ────────────────────────────────────────────────────────────

/// One sync with one peer.
#[derive(Debug, Serialize)]
pub struct Report {
    pub label: String,
    pub id: String,
    /// The peer was added by this session.
    pub added: bool,
    /// The session redeemed an invite.
    pub redeemed: bool,
    pub sent: usize,
    pub got: usize,
    pub conflicts: usize,
    /// Whether the two state hashes agree. None when they differ and one
    /// copy is ahead of the other: no verdict (da-t3ey2).
    pub verified: Option<bool>,
    /// The verify in words, as [`verdict`] says it.
    pub verify: String,
    /// It was no sync: this copy answered the peer's `sync diff`.
    pub diffed: bool,
    /// The peer's version vector, as it last sent it.
    pub vector: Vector,
    /// The members' addresses the peer passed on.
    #[serde(skip)]
    met: Vec<EndpointAddr>,
    /// The most ops a round of the peer's carries, as its handshake said.
    #[serde(skip)]
    round: usize,
    /// The peer skips a BUSY ahead of a round's ops, as its handshake said.
    #[serde(skip)]
    waits: bool,
    /// The peer reads an [`END`], as its handshake said.
    #[serde(skip)]
    ends: bool,
    /// The chain of one this copy left to redeem the invite, and the chain
    /// it joined (da-h7pnc).
    pub left: Option<(String, String)>,
    /// How the session reached the peer, as `datum sync -v` says it.
    #[serde(skip)]
    pub route: Option<String>,
}

impl Report {
    fn new(label: String, id: String, added: bool, redeemed: bool, met: Vec<EndpointAddr>) -> Self {
        Report {
            label,
            id,
            added,
            redeemed,
            met,
            sent: 0,
            got: 0,
            conflicts: 0,
            verified: None,
            verify: String::new(),
            diffed: false,
            vector: BTreeMap::new(),
            left: None,
            round: 0,
            route: None,
            waits: false,
            ends: false,
        }
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.diffed {
            return write!(f, "{}: answered its sync diff", self.label);
        }
        write!(
            f,
            "{}: sent {}, got {}, conflicts {}, {}",
            self.label, self.sent, self.got, self.conflicts, self.verify
        )
    }
}

/// A session that failed after the handshake, and what it had moved by then.
#[derive(Debug)]
pub struct Lost(pub Report, pub anyhow::Error);

impl std::fmt::Display for Lost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Lost(rep, e) = self;
        if rep.got + rep.sent > 0 {
            write!(f, "applied {} op(s) and sent {}, then: ", rep.got, rep.sent)?;
        }
        write!(f, "{e:#}")
    }
}

impl std::error::Error for Lost {}

/// Why a copy refuses a session, sent beside its words so a redeemer acts
/// on the kind, never on the text (da-b6xh6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Why {
    UnknownInvite,
    InviteUsed,
    InviteExpired,
    /// The refusing copy's sync is paused.
    Paused,
    /// The dialer is no member of the chain as the refusing copy holds it.
    NotMember,
    /// The refusing copy is applying another session's ops: try again
    /// later, which is no failure (da-h31wt).
    Busy,
    Other,
}

/// A [`Store`]'s refusal of a handshake: its [`Why`], and its words.
#[derive(Debug)]
pub struct Refusal(pub Why, pub String);

/// Why a dial failed, where its caller acts on it (da-b6xh6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The other copy refused it.
    Refused(Why),
    /// It reached no one.
    Unreachable,
    /// Nothing moved for [`idle`].
    TimedOut,
    /// It reached no one, and the ticket's invite had expired.
    Late,
    /// The other copy speaks another version of the protocol (da-2bnr6).
    Protocol,
}

/// A dial's [`Failure`], and its words.
#[derive(Debug)]
pub struct Failed(pub Failure, pub String);

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}

impl std::error::Error for Refusal {}
impl std::error::Error for Failed {}

/// `e`'s [`Failure`], if it is a [`Failed`].
pub fn failure(e: &anyhow::Error) -> Option<Failure> {
    e.downcast_ref::<Failed>().map(|f| f.0)
}

/// A dial this copy closed for the peer's session with it (da-vj3ly).
fn closed_carried(label: &str) -> anyhow::Error {
    Failed(
        Failure::Refused(Why::Busy),
        format!("{label} {CARRIED_WHY}"),
    )
    .into()
}

/// Whether `e` is a dial given way to a session between the two copies
/// already: closed by this copy, or refused by the peer (da-mghh3). The
/// refusal says so only in its words: a [`Why`] of its own would be one a
/// copy a version back cannot read.
pub fn gave_way(e: &anyhow::Error) -> bool {
    e.downcast_ref::<Failed>()
        .is_some_and(|f| f.0 == Failure::Refused(Why::Busy) && f.1.ends_with(CARRIED_WHY))
}

/// Await `work`, sending a [`BUSY`] every quarter of [`idle`] until it ends.
/// Only work that awaits something done off this thread lets one out.
async fn busy<T>(s: &mut SendStream, work: impl Future<Output = Result<T>>) -> Result<T> {
    // Test knob: the keepalives off (the mutation control).
    let keepalive = env::<u8>("DATUM_SYNC_TEST_KEEPALIVE") != Some(0);
    let mut work = pin!(work);
    loop {
        // A BUSY is sent outside the race: one dropped half-sent would cut
        // the stream's framing.
        match select(work.as_mut(), pin!(sleep(idle() / 4))).await {
            Either::Left((done, _)) => return done,
            Either::Right(_) if keepalive => put(s, BUSY).await?,
            Either::Right(_) => {}
        }
    }
}

/// Apply `ops` under the store's [`apply_gate`](Store::apply_gate), in
/// chunks, and between chunks send a [`BUSY`]: where the store
/// [`yields`](Store::yields) a [`CHUNK`] first and then twice the last, each
/// run [`busy`], and otherwise a [`FIRST`] and then as many as take
/// [`idle`]`/16`, at most twice the last — never over [`MAX_CHUNK`]. `size`
/// is the session's: a round goes on at the size the last ended at, where
/// each began at its first again, and a big catch-up paid a process a chunk
/// eight times a round (da-dwm2y). Its [`progress`](Store::progress) is the session's too:
/// `before` applied in its earlier rounds, of `of` in all.
async fn apply(
    store: &impl Store,
    s: &mut SendStream,
    ops: &[String],
    rep: &mut Report,
    size: &mut usize,
    (before, of): (usize, usize),
) -> Result<()> {
    if ops.is_empty() {
        return Ok(());
    }
    // Applying from the first chunk, which may be the whole round.
    store.progress(before, of);
    // Test knob: the keepalives off (the mutation control).
    let keepalive = env::<u8>("DATUM_SYNC_TEST_KEEPALIVE") != Some(0);
    // The peer has read this round's last frame, and skips a BUSY: one as
    // the wait begins, which may follow another session's chunk (da-h31wt).
    if keepalive {
        put(s, BUSY).await?;
    }
    let _gate = busy(s, store.apply_gate()).await?;
    let mut done = 0;
    while done < ops.len() {
        if done > 0 && keepalive {
            put(s, BUSY).await?;
        }
        // Every session of a copy runs on one thread, and a chunk applied
        // and a BUSY sent never wait: without this, one catch-up held it
        // to the end, and every other session's peer heard nothing
        // (da-h31wt).
        n0_future::future::yield_now().await;
        let chunk = &ops[done..ops.len().min(done + *size)];
        let took = n0_future::time::Instant::now();
        let applied = match store.yields() {
            false => store.apply(chunk).await?,
            true => busy(s, store.apply(chunk)).await?,
        };
        *size = match store.yields() {
            true => (2 * *size).min(MAX_CHUNK),
            false => pace(*size, took),
        };
        done += chunk.len();
        store.progress(before + done, of);
        for why in &applied.refused {
            eprintln!("refused from {}: {why}", rep.label);
        }
        rep.got += applied.applied;
        rep.conflicts += applied.conflicts;
    }
    Ok(())
}

/// The next chunk after one of `size` that began at `took`. A chunk that
/// does not yield holds every session's thread: sized by the clock, so a
/// slow copy's leave the others room (da-h31wt).
fn pace(size: usize, took: n0_future::time::Instant) -> usize {
    let took = took.elapsed().as_micros().max(1);
    let pace = (size as u128 * (idle() / 16).as_micros() / took).clamp(1, 2 * size as u128);
    (pace as usize).min(MAX_CHUNK)
}

/// Sign the baselines a round's ops begin with in chunks, as [`apply`]
/// applies, a [`BUSY`] after each: a bulk import's, signed in one lock,
/// sent the peer nothing past its idle timeout (da-f7mmn).
async fn sign(store: &impl Store, s: &mut SendStream, since: &Vector, n: usize) -> Result<()> {
    // Test knob: the keepalives off (the mutation control).
    let keepalive = env::<u8>("DATUM_SYNC_TEST_KEEPALIVE") != Some(0);
    let (mut size, mut signed) = (CHUNK, 0);
    loop {
        // The round's receiving half reads meanwhile.
        n0_future::future::yield_now().await;
        let took = n0_future::time::Instant::now();
        let chunk = store.sign(since, size).await?;
        signed += chunk;
        store.preparing(n, signed);
        if chunk < size {
            return Ok(());
        }
        size = pace(size, took);
        if keepalive {
            put(s, BUSY).await?;
        }
    }
}

/// Swap ops until neither side's vector moves, then compare state hashes.
async fn exchange(
    store: &impl Store,
    s: &mut SendStream,
    r: &mut RecvStream,
    dialer: bool,
    rep: &mut Report,
) -> Result<()> {
    let size = if store.yields() { CHUNK } else { FIRST };
    let (mut last, mut quiet, mut before, mut size, mut ends) = (None, 0, 0, size, false);
    let mut n = 0;
    while quiet < MAX_ROUNDS {
        let mine = store.vector().await?;
        match ends {
            true => put(s, END).await?,
            false => put_json(s, &mine).await?,
        }
        let f = loop {
            let f = get(r, MAX_FRAME).await?;
            if f != BUSY {
                break f;
            }
        };
        // Both sides have sent this round's vector or END: both end here.
        if ends || f == END {
            break;
        }
        let theirs: Vector = serde_json::from_slice(&f)?;
        rep.vector = theirs.clone();
        let round = (mine, theirs);
        if last.as_ref() == Some(&round) {
            break;
        }
        n += 1;
        // Counted as each goes, so a session cut short says what it cost.
        let (id, waits, sent) = (&rep.id, rep.waits, &mut rep.sent);
        let send = async {
            store.preparing(n, 0);
            // To a peer that waits, the round's baselines are signed first,
            // a BUSY between their chunks, as the peer's ops are read.
            let ops = match waits {
                true => {
                    sign(store, s, &round.1, n).await?;
                    busy(s, store.ops_since(id, &round.1, self::round())).await?
                }
                false => store.ops_since(id, &round.1, self::round()).await?,
            };
            // Read again: an import's baselines, signed just now, are in
            // no vector swapped before them (da-y8yde).
            let left = match ops.len() >= self::round() {
                true => ops
                    .len()
                    .max(ahead(&store.vector().await?, &round.1) as usize),
                false => ops.len(),
            };
            store.round(n, left);
            for (i, op) in ops.iter().enumerate() {
                put(s, op.as_bytes()).await?;
                *sent += 1;
                store.sent(i + 1, ops.len());
            }
            put(s, b"").await?;
            Ok(ops.len())
        };
        let recv = async {
            let mut lines = Vec::new();
            loop {
                let f = get(r, MAX_FRAME).await?;
                if f.is_empty() {
                    return Ok::<_, anyhow::Error>(lines);
                }
                if f != BUSY {
                    lines.push(String::from_utf8(f)?);
                    store.got(lines.len());
                }
            }
        };
        let (ops, lines) = futures_util::future::try_join(send, recv).await?;
        // Both sides read the same two counts against the same two caps. A
        // copy before da-ymgiv says round 0 and counts every round: so must
        // this side, or the two end the session at different rounds.
        let full = rep.round > 0 && (ops >= self::round() || lines.len() >= rep.round);
        // A round the peer filled has more behind it, as many as its vector
        // holds past this copy's (da-dwm2y): a big catch-up says how far it
        // is in all, not in this round. Never fewer than the round brought:
        // a peer signing an import's baselines as the rounds go sends ops
        // its vector did not yet hold.
        let more = rep.round > 0 && lines.len() >= rep.round;
        let left = match more {
            true => lines.len().max(ahead(&round.1, &round.0) as usize),
            false => lines.len(),
        };
        if more {
            store.more();
        }
        let got = rep.got;
        apply(store, s, &lines, rep, &mut size, (before, before + left)).await?;
        // Test knob: no end (the mutation control).
        let end = env::<u8>("DATUM_SYNC_TEST_END") != Some(0);
        // The peer filled its round, and under half of it was new to this
        // copy (`rep.got` counts what applied, the whole round by now).
        ends = end && rep.ends && more && 2 * (rep.got - got) < lines.len();
        before += lines.len();
        quiet += usize::from(!full);
        last = Some(round);
    }
    // A big history hashes for minutes on each side at once (da-0mwws).
    store.verifying();
    let (hash, now) = busy(s, async {
        let hash = store.verify(&rep.id).await?;
        Ok((hash, store.vector().await?))
    })
    .await?;
    let mine = last.map(|round| round.0).unwrap_or_default();
    let said = match now == mine {
        true => hash.clone(),
        false => format!("{MOVED}{hash}"),
    };
    // The acceptor answers only after it has read the dialer's hash, so
    // the dialer's close can never cut a frame still in flight.
    let theirs: String = if dialer {
        put_json(s, &said).await?;
        get_json(r, MAX_HELLO).await?
    } else {
        let h = get_json(r, MAX_HELLO).await?;
        put_json(s, &said).await?;
        h
    };
    let (they_moved, theirs) = theirs
        .strip_prefix(MOVED)
        .map_or((false, &theirs[..]), |h| (true, h));
    let moved = match now == mine {
        false => Some("this copy"),
        true => they_moved.then_some(rep.label.as_str()),
    };
    (rep.verified, rep.verify) = verdict(&rep.label, theirs == hash, &mine, &rep.vector, moved);
    Ok(())
}

/// How many ops `a` holds past `of`, by their vectors.
fn ahead(a: &Vector, of: &Vector) -> i64 {
    a.iter()
        .map(|(e, s)| (s - of.get(e).copied().unwrap_or(0)).max(0))
        .sum()
}

/// A session's verify, and its words. Hashes that differ are a MISMATCH only
/// over the same ops: with one copy ahead of the other they could not agree,
/// and it says who is ahead by how many ops instead. A reader ahead of a
/// member stays so, since it never relays (da-t3ey2). Nor is it one when
/// a copy, `moved`, changed after the vectors were read (da-o2arv).
fn verdict(
    label: &str,
    same: bool,
    mine: &Vector,
    theirs: &Vector,
    moved: Option<&str>,
) -> (Option<bool>, String) {
    let says = |who: &str, n: i64, of: &str| {
        let s = if n == 1 { "" } else { "s" };
        (n > 0).then(|| format!("{who} is {n} change{s} ahead of {of}"))
    };
    let apart: Vec<String> = [
        says("this copy", ahead(mine, theirs), label),
        says(label, ahead(theirs, mine), "this copy"),
    ]
    .into_iter()
    .flatten()
    .collect();
    match (apart.is_empty(), moved) {
        _ if same => (Some(true), "verify ok".into()),
        (false, _) => (None, format!("not verified: {}", apart.join(", "))),
        (true, Some(who)) => (None, format!("not verified: {who} changed as it verified")),
        (true, None) => (Some(false), "verify MISMATCH".into()),
    }
}

/// Record how a session with `id` ended. A peer that is neither a member
/// nor paired (a refused redemption) has no row to record it in. A dial
/// that reached no one ([`Failure::Unreachable`], all [`connect`] fails
/// with but another protocol) is offline: the peer is not up.
async fn record(store: &impl Store, id: &str, rep: &Result<Report>) {
    let (vector, verify, error, offline, met) = match rep {
        Ok(r) => (Some(&r.vector), r.verified, None, false, &r.met[..]),
        Err(e) => (
            e.downcast_ref::<Lost>().map(|Lost(r, _)| &r.vector),
            None,
            Some(e),
            failure(e) == Some(Failure::Unreachable),
            &[][..],
        ),
    };
    let _ = store
        .record_session(id, vector, verify, error, offline, met)
        .await;
}

/// What a handshake passes on of [`Store::known`]: failing to read it
/// never stops a sync.
async fn known(store: &impl Store) -> Vec<EndpointAddr> {
    let mut known = store.known().await.unwrap_or_default();
    known.truncate(MAX_KNOWN);
    known
}

/// How a redeemer joins: the invite's secret, the chain it says it is in —
/// its own when it rejoins that chain (da-j8c4l), none otherwise — and the
/// chain it leaves once admitted, if it leaves one (da-h7pnc).
pub struct Redeem<'a> {
    secret: &'a str,
    chain: String,
    leaves: Option<String>,
}

/// Dial a peer, presenting `invite` if this is its redemption, sync, and
/// record how it went.
pub async fn dial(
    store: &impl Store,
    ep: &Endpoint,
    addr: EndpointAddr,
    invite: Option<&Redeem<'_>>,
) -> Result<Report> {
    let id = addr.id.to_string();
    if store.dial_busy() {
        return Err(Failed(Failure::Refused(Why::Busy), format!("this copy {BUSY_WHY}")).into());
    }
    let rep = session(store, ep, addr, invite).await;
    // Busy is no session, and nothing failed (da-h31wt).
    let busy = rep.as_ref().err().and_then(failure) == Some(Failure::Refused(Why::Busy));
    if busy {
        return rep;
    }
    record(store, &id, &rep).await;
    rep
}

/// The reason a dialer closes with when its own handshake found it busy,
/// after the acceptor welcomed it: the acceptor records nothing (da-h31wt).
const CLOSED_BUSY: &[u8] = b"busy";

/// Whether `e` is the peer closing with [`CLOSED_BUSY`].
fn closed_busy(e: &anyhow::Error) -> bool {
    e.chain().any(|e| {
        matches!(e.downcast_ref(), Some(ConnectionError::ApplicationClosed(c)) if c.reason == CLOSED_BUSY)
    })
}

/// Why a copy applying another session's ops refuses one more.
const BUSY_WHY: &str = "is busy applying another sync's changes; it syncs again once done";

/// Why one of two sessions between the same copies at once gives way.
const CARRIED_WHY: &str = "is in a session with this copy already";

async fn session(
    store: &impl Store,
    ep: &Endpoint,
    addr: EndpointAddr,
    invite: Option<&Redeem<'_>>,
) -> Result<Report> {
    ensure!(addr.id != ep.id(), COPY);
    let id = addr.id.to_string();
    let conn = connect(ep, addr, ALPN).await?;
    let (mut s, mut r) = conn.open_bi().await?;
    let left = invite.and_then(|r| r.leaves.clone());
    let holds = match invite {
        Some(_) => store.holds().await?,
        None => 0,
    };
    let hello = Hello {
        label: store.label(),
        addr: ep.addr(),
        invite: invite.map(|r| r.secret.to_string()),
        chain: invite.map(|r| r.chain.clone()),
        holds,
        proves: invite.is_none(),
        typed: true,
        busy: true,
        known: match invite {
            Some(_) => Vec::new(),
            None => known(store).await,
        },
        round: round(),
        waits: true,
        ends: true,
        version: store.version(),
    };
    put_json(&mut s, &hello).await?;
    let mut reply = get_json(&mut r, MAX_HELLO).await?;
    if hello.proves && matches!(reply, Reply::Prove) {
        put_json(&mut s, &store.records().await?).await?;
        reply = get_json(&mut r, MAX_HELLO).await?;
    }
    let carried;
    let (label, added, met, round, waits, ends) = match reply {
        Reply::Welcome {
            label,
            addr,
            known,
            round,
            waits,
            ends,
            version,
        } => {
            // Two copies that dialed each other at once (da-vj3ly): the
            // lower endpoint id's session carries both ways, and the other
            // copy closes its own as busy, which neither side records.
            // Past here this one carries, and the peer's dial, if it comes
            // only now, is refused as busy (da-mghh3).
            if invite.is_none() && store.serves(&id) && ep.id().to_string() > id {
                conn.close(0u32.into(), CLOSED_BUSY);
                return Err(closed_carried(&label));
            }
            carried = Served::new(store, &id, true);
            // Its own handshake may refuse too (a paused copy): only then
            // does it leave. Given the invite, it marks where this copy was
            // admitted, so a join cut short before any op applies resumes
            // there (da-38eye).
            let admitted = async {
                let added = store
                    .handshake(
                        &id,
                        &label,
                        &addr,
                        invite.map(|r| r.secret),
                        None,
                        0,
                        &version,
                        None,
                        true,
                    )
                    .await?;
                if left.is_some() {
                    store.leave(false).await?;
                }
                Ok::<_, anyhow::Error>(added)
            };
            match admitted.await {
                Ok(added) => (label, added, known, round, waits, ends),
                Err(e) => {
                    let busy = e
                        .downcast_ref::<Refusal>()
                        .is_some_and(|r| r.0 == Why::Busy);
                    conn.close(0u32.into(), if busy { CLOSED_BUSY } else { b"refused" });
                    return Err(e);
                }
            }
        }
        Reply::Refused(why) => {
            conn.close(0u32.into(), b"refused");
            return Err(refused(why, None));
        }
        Reply::Refusal { why, kind } => {
            conn.close(0u32.into(), b"refused");
            return Err(refused(why, Some(kind)));
        }
        Reply::Prove => bail!("{} asked for what it was not offered", short(&id)),
    };
    let mut rep = Report::new(label, id, added, invite.is_some(), met);
    rep.round = round;
    rep.waits = waits;
    rep.ends = ends;
    let done = exchange(store, &mut s, &mut r, true, &mut rep).await;
    drop(carried);
    if let (Some(old), Ok(Some((new, _)))) = (left, store.chain().await) {
        rep.left = Some((old, new));
    }
    rep.route = route(&conn);
    conn.close(0u32.into(), b"done");
    match done {
        Ok(()) => Ok(rep),
        Err(e) => Err(Lost(rep, e).into()),
    }
}

/// The path a connection sends on: direct to an address, or relayed.
fn route(conn: &Connection) -> Option<String> {
    let paths = conn.paths();
    let path = paths.iter().find(|p| p.is_selected())?;
    Some(match path.remote_addr() {
        TransportAddr::Ip(a) => format!("direct {a}"),
        TransportAddr::Relay(url) => format!("relayed via {url}"),
        other => other.to_string(),
    })
}

/// What a `sync diff` with one peer found.
pub struct Diffed {
    /// How many entries were compared: those either copy holds.
    pub entries: usize,
    /// Per entry that differs, what this copy holds and what the peer does
    /// ([`Store::held`]), in one order.
    pub mine: Vec<serde_json::Value>,
    pub theirs: Vec<serde_json::Value>,
}

/// Ask the peer at `addr` what it holds where it differs from this copy
/// (da-n0ruf). The peer admits a member as it does for a sync; nothing is
/// applied on either side.
pub async fn diff(store: &impl Store, ep: &Endpoint, addr: EndpointAddr) -> Result<Diffed> {
    ensure!(addr.id != ep.id(), COPY);
    let id = addr.id.to_string();
    let conn = connect(ep, addr, DIFF).await?;
    let (mut s, mut r) = conn.open_bi().await?;
    let hello = Hello {
        label: store.label(),
        addr: ep.addr(),
        invite: None,
        chain: None,
        holds: 0,
        proves: false,
        known: Vec::new(),
        typed: true,
        busy: true,
        round: 0,
        waits: false,
        ends: false,
        version: store.version(),
    };
    put_json(&mut s, &hello).await?;
    match get_json(&mut r, MAX_HELLO).await? {
        Reply::Welcome { .. } => {}
        Reply::Refused(why) => return Err(refused(why, None)),
        Reply::Refusal { why, kind } => return Err(refused(why, Some(kind))),
        Reply::Prove => bail!("{} asked for what it was not offered", short(&id)),
    }
    let (hash, mine) = store.digest(&id).await?;
    put_json(&mut s, &hash).await?;
    let theirs: Option<Digest> = get_json(&mut r, MAX_FRAME).await?;
    let theirs = theirs.as_ref().unwrap_or(&mine);
    let all: std::collections::BTreeSet<&String> = mine.keys().chain(theirs.keys()).collect();
    let differs = |h: &&&String| mine.get(**h) != theirs.get(**h);
    let hids: Vec<String> = all.iter().filter(differs).map(|h| (*h).clone()).collect();
    put_json(&mut s, &hids).await?;
    let theirs = get_json(&mut r, MAX_FRAME).await?;
    conn.close(0u32.into(), b"done");
    Ok(Diffed {
        entries: all.len(),
        mine: store.held(&id, &hids).await?,
        theirs,
    })
}

/// Connect to `addr` over `alpn`, or say why not.
async fn connect(ep: &Endpoint, addr: EndpointAddr, alpn: &[u8]) -> Result<Connection> {
    let id = addr.id.to_string();
    within(async { Ok(ep.connect(addr, alpn).await?) })
        .await
        .map_err(|e| match other_protocol(&e) {
            true => Failed(Failure::Protocol, format!("{}: {OTHER}", short(&id))).into(),
            false => unreachable(ep, &id, e),
        })
}

/// What two copies that speak different ALPNs are told, on either side.
const OTHER: &str =
    "that copy speaks another version of datum sync than this one: update datum on one of them";

/// TLS's `no_application_protocol` alert: the other copy offered or served
/// no ALPN this one speaks.
fn other_protocol(e: &anyhow::Error) -> bool {
    let alert = TransportErrorCode::crypto(120);
    e.chain().any(|e| match e.downcast_ref() {
        Some(ConnectionError::ConnectionClosed(c)) => c.error_code == alert,
        Some(ConnectionError::TransportError(t)) => t.code == alert,
        _ => false,
    })
}

/// Why a dial reached no one, and what to do about it (da-shfv4): this
/// copy's own relay is down, or the other copy is not running. Either way it
/// is [`Failure::Unreachable`], which `redeem` reads as «try the next address».
fn unreachable(ep: &Endpoint, id: &str, e: anyhow::Error) -> anyhow::Error {
    let relays = ep.home_relay_status().get();
    let why = if !relays.is_empty() && !relays.iter().any(|r| r.is_connected()) {
        format!(
            "could not reach {}: this copy is not connected to its relay ({}); is it online?",
            short(id),
            relays[0].url()
        )
    } else {
        format!(
            "could not reach {}: {e}; is the other copy running? A terminal syncs while \
             `datum sync run` or its daemon runs (`datum sync daemon status`), datum.day while \
             a tab is open on the database",
            short(id)
        )
    };
    Failed(Failure::Unreachable, why).into()
}

/// An incoming connection, or why it failed.
async fn accepted(incoming: Incoming) -> Result<Connection> {
    incoming.await.map_err(|e| match anyhow!(e) {
        e if other_protocol(&e) => anyhow!("a copy dialed this one, and {OTHER}"),
        e => e,
    })
}

/// Answer one incoming connection: a peer, or the redemption of an invite.
pub async fn serve(store: &impl Store, ep: &Endpoint, incoming: Incoming) -> Result<Report> {
    let conn = accepted(incoming).await?;
    let id = conn.remote_id().to_string();
    // Taken with the mark, so the peer's own dial past its handshake
    // (`carries`) and this session (`serves`) never both go on (da-mghh3).
    let carried = store.carries(&id);
    let _served = Served::new(store, &id, false);
    let (mut s, mut r) = within(async { Ok(conn.accept_bi().await?) }).await?;
    let hello: Hello = get_json(&mut r, MAX_HELLO).await?;
    // A diff admits no one: it answers a member this copy holds, redeems
    // no invite, and asks for no records to admit one it has not heard of.
    let diff = conn.alpn() == DIFF;
    let mut records: Option<Vec<String>> = None;
    let verdict = loop {
        let verdict = match conn.remote_id() == ep.id() {
            true => Err((Why::Other, anyhow!(COPY))),
            false if !diff && store.applying() => Err((Why::Busy, anyhow!(BUSY_WHY))),
            false if !diff && carried && hello.busy => Err((Why::Busy, anyhow!(CARRIED_WHY))),
            false => store
                .handshake(
                    &id,
                    &hello.label,
                    &hello.addr,
                    hello.invite.as_deref().filter(|_| !diff),
                    hello.chain.as_deref(),
                    hello.holds,
                    &hello.version,
                    records.as_deref(),
                    false,
                )
                .await
                .map_err(|e| (e.downcast_ref::<Refusal>().map_or(Why::Other, |r| r.0), e)),
        };
        // A member this copy has not heard of shows that it is one, once.
        match verdict {
            Err((Why::NotMember, _)) if hello.proves && !diff && records.is_none() => {
                put_json(&mut s, &Reply::Prove).await?;
                records = Some(get_json(&mut r, MAX_RECORDS).await?);
            }
            verdict => break verdict,
        }
    };
    let known = match verdict {
        Ok(added) => !added,
        Err((kind, e)) => {
            let reply = Reply::refusal(&hello, kind, format!("{e:#}"));
            put_json(&mut s, &reply).await?;
            let _ = timeout(Duration::from_secs(5), conn.closed()).await;
            // The store's error stays its cause, for a caller to read.
            let why = format!("refused {} ({})", hello.label, short(&id));
            return Err(e.context(Failed(Failure::Refused(kind), why)));
        }
    };
    let welcome = Reply::Welcome {
        label: store.label(),
        addr: ep.addr(),
        known: self::known(store).await,
        round: round(),
        waits: true,
        ends: true,
        version: store.version(),
    };
    put_json(&mut s, &welcome).await?;
    let redeemed = hello.invite.is_some() && !diff;
    let mut rep = Report::new(hello.label, id.clone(), !known, redeemed, hello.known);
    rep.round = hello.round;
    rep.waits = hello.waits;
    rep.ends = hello.ends;
    if diff {
        let theirs: String = get_json(&mut r, MAX_HELLO).await?;
        let (hash, digest) = store.digest(&id).await?;
        put_json(&mut s, &(theirs != hash).then_some(digest)).await?;
        let hids: Vec<String> = get_json(&mut r, MAX_FRAME).await?;
        put_json(&mut s, &store.held(&id, &hids).await?).await?;
        let _ = timeout(Duration::from_secs(5), conn.closed()).await;
        rep.diffed = true;
        return Ok(rep);
    }
    let done = match exchange(store, &mut s, &mut r, false, &mut rep).await {
        Ok(()) => {
            let _ = timeout(Duration::from_secs(5), conn.closed()).await;
            Ok(rep)
        }
        Err(e) if closed_busy(&e) => {
            let why = format!("{}: {e:#}", rep.label);
            return Err(Failed(Failure::Refused(Why::Busy), why).into());
        }
        Err(e) => Err(Lost(rep, e).into()),
    };
    record(store, &id, &done).await;
    done
}

/// A session [`serve`] is serving, or one [`session`] dialed past its
/// handshake (`dialed`), said to its store as it begins and ends.
struct Served<'a, S: Store>(&'a S, String, bool);

impl<'a, S: Store> Served<'a, S> {
    fn new(store: &'a S, peer: &str, dialed: bool) -> Self {
        let served = Served(store, peer.to_string(), dialed);
        served.mark(true);
        served
    }

    fn mark(&self, on: bool) {
        match self.2 {
            true => self.0.carrying(&self.1, on),
            false => self.0.serving(&self.1, on),
        }
    }
}

impl<S: Store> Drop for Served<'_, S> {
    fn drop(&mut self) {
        self.mark(false);
    }
}

/// Redeem `ticket`'s invite and sync, with the first of its addresses that
/// admits it. A copy invited back into its own chain — a revoked device —
/// rejoins it, as a new device joins (da-j8c4l). A copy in another chain is
/// refused before anything is sent: its ops would flow one way into the
/// other chain (da-mepav) — unless they are its own alone, when it leaves
/// its chain once admitted (da-h7pnc).
pub async fn redeem(store: &impl Store, ep: &Endpoint, ticket: &Ticket) -> Result<Report> {
    let (chain, leaves) = match store.chain().await? {
        None => (String::new(), None),
        Some((id, _)) if ticket.about.chain.as_ref() == Some(&id) => (id, None),
        Some((id, n)) if store.leave(true).await.is_err() => bail!(
            "this copy is in sync chain {id} ({n} members); a copy belongs to one chain. \
             To join another, `datum sync chain leave` first (every entry is kept), or use \
             a copy that isn't in one"
        ),
        Some((id, _)) => (String::new(), Some(id)),
    };
    let invite = Redeem {
        secret: &ticket.secret,
        chain,
        leaves,
    };
    let mut rep = Err(anyhow!("the ticket names no address"));
    // A member rejoining is named among its chain's members (da-j8c4l).
    for addr in ticket.addrs.iter().filter(|a| a.id != ep.id()) {
        rep = dial(store, ep, addr.clone(), Some(&invite)).await;
        // Unreachable, or a member not yet holding the invite: the next
        // address may admit. Any other refusal stops here, a paused inviter
        // too, and so does another protocol: going on would report whatever
        // the last address said, often «could not reach», in place of the
        // words that say what to do.
        let next = rep.as_ref().err().and_then(failure);
        if !matches!(
            next,
            Some(Failure::Unreachable | Failure::Refused(Why::UnknownInvite | Why::Busy))
        ) {
            break;
        }
    }
    // An inviter stops serving its invite once it expires, so a late
    // ticket reaches no one; this clock says why. Only then: a clock that
    // is wrong must not refuse an invite still served (da-l9kdr).
    let late = ticket.about.expires.is_some_and(|at| at <= unix_now());
    match rep {
        Err(e) if late && failure(&e) == Some(Failure::Unreachable) => {
            let why = "invite expired: ask the inviting device for a new one";
            Err(e.context(Failed(Failure::Late, why.into())))
        }
        rep => rep,
    }
}

// ─── The endpoint ───────────────────────────────────────────────────────────

/// How the endpoint reaches its peers.
pub enum Net {
    /// n0's relays and discovery.
    N0,
    /// This one relay (a URL), and nothing else to find peers by.
    Relay(String),
    /// This one address, with no relay and no discovery (a LAN, the tests).
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    Local(std::net::SocketAddr),
}

/// The endpoint whose key is `secret`, the db's endpoint key.
pub async fn bind(secret: [u8; 32], net: &Net) -> Result<Endpoint> {
    let builder = match net {
        Net::N0 => Endpoint::builder(presets::N0),
        Net::Relay(url) => Endpoint::builder(presets::Minimal).relay_mode(RelayMode::custom([url
            .parse()
            .context("not a relay URL")?])),
        #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
        Net::Local(addr) => Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .clear_ip_transports()
            .bind_addr(*addr)?,
    };
    let ep = builder
        .secret_key(SecretKey::from_bytes(&secret))
        .alpns(vec![ALPN.to_vec(), DIFF.to_vec()])
        .bind()
        .await?;
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    let relayed = !matches!(net, Net::Local(_));
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    let relayed = true;
    if relayed {
        // The address a ticket or a peer is handed should name the relay.
        let _ = timeout(Duration::from_secs(10), ep.online()).await;
    }
    Ok(ep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// Whether this version reads a frame the previous one wrote, and writes
    /// it back with every field that one reads.
    type Reads = fn(&Value) -> Result<(), String>;

    fn reads<T: Serialize + DeserializeOwned>(old: &Value) -> Result<(), String> {
        let t: T = serde_json::from_value(old.clone()).map_err(|e| format!("unread: {e}"))?;
        let new = serde_json::to_value(t).map_err(|e| e.to_string())?;
        match lacks(&new, old, "", true) {
            None => Ok(()),
            Some(at) => Err(format!(
                "written back without {at} as the previous version has it"
            )),
        }
    }

    /// Where `new` lacks something of `old`: a key or an element, or with
    /// `values` a value it holds.
    fn lacks(new: &Value, old: &Value, at: &str, values: bool) -> Option<String> {
        match (new, old) {
            (Value::Object(n), Value::Object(o)) => o.iter().find_map(|(k, v)| {
                let at = format!("{at}/{k}");
                n.get(k)
                    .map_or(Some(at.clone()), |n| lacks(n, v, &at, values))
            }),
            (Value::Array(n), Value::Array(o)) if !values || n.len() == o.len() => {
                o.iter().enumerate().find_map(|(i, o)| {
                    let at = format!("{at}/{i}");
                    n.get(i)
                        .map_or(Some(at.clone()), |n| lacks(n, o, &at, values))
                })
            }
            _ => (values && new != old).then(|| format!("{at} = {old}")),
        }
    }

    fn kind<T: Serialize + DeserializeOwned>(
        name: &'static str,
        t: T,
    ) -> (&'static str, Value, Reads) {
        (name, serde_json::to_value(t).unwrap(), reads::<T>)
    }

    /// One frame of each kind, as this version writes it, every field set.
    fn kinds() -> Vec<(&'static str, Value, Reads)> {
        let addr = EndpointAddr::new(SecretKey::from_bytes(&[1; 32]).public())
            .with_relay_url("https://relay.example/".parse().unwrap())
            .with_ip_addr("192.0.2.1:4433".parse().unwrap());
        let version = Version {
            datum: "3.0.0-beta (440b271)".into(),
            schema: 57,
        };
        let line = r#"{"op":"set","endpoint":"e","seq":1}"#.to_string();
        let vector: Vector = [("e".to_string(), 1)].into();
        let digest: Digest = [("h".to_string(), "ab12".to_string())].into();
        vec![
            kind(
                "hello",
                Hello {
                    label: "laptop".into(),
                    addr: addr.clone(),
                    invite: Some("secret".into()),
                    chain: Some("chain".into()),
                    holds: 3,
                    proves: true,
                    typed: true,
                    busy: true,
                    known: vec![addr.clone()],
                    round: ROUND,
                    waits: true,
                    ends: true,
                    version: version.clone(),
                },
            ),
            kind(
                "welcome",
                Reply::Welcome {
                    label: "phone".into(),
                    addr: addr.clone(),
                    known: vec![addr.clone()],
                    round: ROUND,
                    waits: true,
                    ends: true,
                    version,
                },
            ),
            kind("refused", Reply::Refused("unknown invite".into())),
            kind(
                "refusal",
                Reply::Refusal {
                    why: "invite already used".into(),
                    kind: Why::InviteUsed,
                },
            ),
            kind("prove", Reply::Prove),
            kind(
                "why",
                [
                    Why::UnknownInvite,
                    Why::InviteUsed,
                    Why::InviteExpired,
                    Why::Paused,
                    Why::NotMember,
                    Why::Busy,
                    Why::Other,
                ],
            ),
            kind("records", vec![line]),
            kind("vector", vector),
            kind("hash", "ab12".to_string()),
            kind("digest", Some(digest)),
            kind("hids", vec!["h".to_string()]),
            kind(
                "ticket",
                Ticket {
                    addrs: vec![addr],
                    secret: "secret".into(),
                    about: About {
                        from: Some("laptop".into()),
                        chain: Some("chain".into()),
                        expires: Some(1_700_000_000),
                    },
                },
            ),
        ]
    }

    const FRAMES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/frames.json");

    fn recorded(now: &[(&'static str, Value, Reads)]) -> BTreeMap<String, Value> {
        now.iter()
            .map(|(k, v, _)| (k.to_string(), v.clone()))
            .collect()
    }

    /// What the recorded frames `old` say of this version's `now`: a kind it
    /// no longer reads or writes back, or else a kind or a key path it writes
    /// that `old` lacks, which a later release could drop unseen (da-igio7).
    fn drift(old: &BTreeMap<String, Value>, now: &[(&str, Value, Reads)]) -> Result<(), String> {
        let broken: Vec<String> = old
            .iter()
            .filter_map(|(name, frame)| match now.iter().find(|k| k.0 == name) {
                None => Some(format!("{name}: no longer written")),
                Some((_, _, reads)) => reads(frame).err().map(|e| format!("{name}: {e}")),
            })
            .collect();
        if !broken.is_empty() {
            return Err(format!(
                "a frame is no longer additive, so a copy one version apart cannot sync with \
                 this one; make it additive, or bump ALPN on purpose and record the frames \
                 again (the crate doc, «Versions»):\n{}",
                broken.join("\n")
            ));
        }
        let gained: Vec<String> = now
            .iter()
            .filter_map(|(name, frame, _)| match old.get(*name) {
                None => Some(format!("{name}: a new kind")),
                Some(was) => lacks(was, frame, "", false).map(|at| format!("{name}: {at}")),
            })
            .collect();
        if !gained.is_empty() {
            return Err(format!(
                "a frame gained a field: record the frames again \
                 (DATUM_SYNC_RECORD_FRAMES=1; the diff only adds):\n{}",
                gained.join("\n")
            ));
        }
        Ok(())
    }

    /// A copy one version apart reads every frame this one writes, and this
    /// one every frame it writes (da-2bnr6): the previous version's frames,
    /// kept in `tests/frames.json`, read here, and come back with every field
    /// they had. A kind there that this version no longer writes fails too,
    /// and so does a kind or a field it writes that the file lacks.
    #[test]
    fn frames_of_the_previous_version_read_both_ways() {
        let now = kinds();
        if std::env::var_os("DATUM_SYNC_RECORD_FRAMES").is_some() {
            let text = serde_json::to_string_pretty(&recorded(&now)).unwrap();
            std::fs::write(FRAMES, text + "\n").unwrap();
            return;
        }
        let old = serde_json::from_str(&std::fs::read_to_string(FRAMES).unwrap()).unwrap();
        if let Err(e) = drift(&old, &now) {
            panic!("{e}");
        }
    }

    /// A field this version writes and the recorded frames lack fails until
    /// they are recorded again, and recording them only adds (da-igio7).
    #[test]
    fn a_field_the_frames_lack_fails_until_recorded() {
        #[derive(Serialize, Deserialize)]
        struct Gained {
            #[serde(flatten)]
            hello: Hello,
            #[serde(default)]
            gained: bool,
        }
        let old = serde_json::from_str(&std::fs::read_to_string(FRAMES).unwrap()).unwrap();
        let mut now = kinds();
        let at = now.iter().position(|k| k.0 == "hello").unwrap();
        let hello = serde_json::from_value(now[at].1.clone()).unwrap();
        now[at] = kind(
            "hello",
            Gained {
                hello,
                gained: true,
            },
        );
        let e = drift(&old, &now).unwrap_err();
        assert!(
            e.contains("a frame gained a field") && e.contains("hello: /gained"),
            "{e}"
        );
        let again = recorded(&now);
        for (name, was) in &old {
            assert_eq!(lacks(&again[name], was, "", true), None, "{name}");
        }
        drift(&again, &now).unwrap();
    }

    /// Two copies that speak different ALPNs are told so in a sentence, on
    /// the side that dials and the side that answers (da-2bnr6).
    #[cfg(not(target_family = "wasm"))]
    #[tokio::test]
    async fn another_protocol_is_said_in_words() {
        let net = Net::Local("127.0.0.1:0".parse().unwrap());
        let this = bind([1; 32], &net).await.unwrap();
        let other = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
            .unwrap()
            .secret_key(SecretKey::from_bytes(&[2; 32]))
            .alpns(vec![b"datum/sync/3".to_vec()])
            .bind()
            .await
            .unwrap();
        let answering = async { accepted(other.accept().await.unwrap()).await };
        let (dialed, answered) = tokio::join!(connect(&this, other.addr(), ALPN), answering);
        let e = dialed.err().unwrap();
        assert_eq!(failure(&e), Some(Failure::Protocol));
        let id = other.id().to_string();
        assert_eq!(format!("{e:#}"), format!("{}: {OTHER}", short(&id)));
        let e = answered.err().unwrap();
        assert_eq!(
            format!("{e:#}"),
            format!("a copy dialed this one, and {OTHER}")
        );
    }

    /// A copy one version apart keeps reading a refusal (da-b6xh6): the
    /// frames below are what a copy before it sends and reads.
    #[test]
    fn a_refusal_reads_across_one_version() {
        let addr = EndpointAddr::new(SecretKey::from_bytes(&[1; 32]).public());
        // An older dialer says nothing of `typed`, and is refused in words.
        let old: Hello = serde_json::from_value(json!({
            "label": "old", "addr": addr, "invite": "s", "chain": "", "holds": 0
        }))
        .unwrap();
        let reply = Reply::refusal(&old, Why::InviteUsed, "invite already used".into());
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            json!({"Refused": "invite already used"})
        );
        // A dialer that says so is refused with the kind.
        let typed = Hello { typed: true, ..old };
        let reply = Reply::refusal(&typed, Why::InviteUsed, "invite already used".into());
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            json!({"Refusal": {"why": "invite already used", "kind": "invite-used"}})
        );
        // One that says nothing of `proves`, a copy before da-2u95o, is
        // never asked for its records and reads no kind it does not know.
        assert!(!typed.proves);
        let said = |hello: &Hello| {
            serde_json::to_value(Reply::refusal(hello, Why::NotMember, "no".into())).unwrap()
        };
        assert_eq!(said(&typed)["Refusal"]["kind"], "other");
        let proves = Hello {
            proves: true,
            ..typed
        };
        assert_eq!(said(&proves)["Refusal"]["kind"], "not-member");
        // Nor one that says nothing of `busy`, a copy before da-h31wt.
        let said = |hello: &Hello| {
            serde_json::to_value(Reply::refusal(hello, Why::Busy, "busy".into())).unwrap()
        };
        assert_eq!(said(&proves)["Refusal"]["kind"], "other");
        let busy = Hello {
            busy: true,
            ..proves
        };
        assert_eq!(said(&busy)["Refusal"]["kind"], "busy");
        // An older acceptor's refusal reads, and «unknown invite» still
        // sends a redeemer on to the next address.
        for (words, kind) in [
            ("unknown invite", Why::UnknownInvite),
            ("Error: unknown invite", Why::UnknownInvite),
            ("invite already used", Why::Other),
        ] {
            let Reply::Refused(why) = serde_json::from_value(json!({ "Refused": words })).unwrap()
            else {
                panic!("{words}: not a refusal");
            };
            let e = refused(why, None);
            assert_eq!(failure(&e), Some(Failure::Refused(kind)), "{words}");
            assert_eq!(e.to_string(), format!("refused: {words}"));
        }
    }

    /// A copy before da-3zqyv says nothing of what it runs, and its Hello
    /// and Welcome still read; a newer copy says both beside the old fields.
    #[test]
    fn a_handshake_without_a_version_reads() {
        let addr = EndpointAddr::new(SecretKey::from_bytes(&[1; 32]).public());
        let old: Hello = serde_json::from_value(json!({
            "label": "old", "addr": addr, "invite": null, "chain": null, "holds": 0, "typed": true
        }))
        .unwrap();
        assert_eq!(old.version, Version::default());
        let welcome = json!({"Welcome": {"label": "old", "addr": addr}});
        let Reply::Welcome { version, .. } = serde_json::from_value(welcome).unwrap() else {
            panic!("not a welcome");
        };
        assert_eq!(version, Version::default());
        let new = Hello {
            version: Version {
                datum: "3.0.900-beta (abc1234)".into(),
                schema: 57,
            },
            ..old
        };
        let said = serde_json::to_value(&new).unwrap();
        assert_eq!(said["datum"], "3.0.900-beta (abc1234)");
        assert_eq!(said["schema"], 57);
        assert_eq!(said["label"], "old");
    }

    /// A verify between copies that hold different ops is no verdict, and
    /// names each one ahead (da-t3ey2); over the same ops it is one.
    #[test]
    fn a_verify_is_a_mismatch_only_over_the_same_ops() {
        let v = |seqs: &[(&str, i64)]| -> Vector {
            seqs.iter().map(|(e, s)| (e.to_string(), *s)).collect()
        };
        let (mine, theirs) = (v(&[("a", 3), ("b", 1)]), v(&[("b", 2), ("c", 0)]));
        assert_eq!(
            verdict("B", false, &mine, &theirs, None),
            (
                None,
                "not verified: this copy is 3 changes ahead of B, \
                 B is 1 change ahead of this copy"
                    .into()
            )
        );
        assert_eq!(
            verdict("B", false, &mine, &mine, None),
            (Some(false), "verify MISMATCH".into())
        );
        assert_eq!(
            verdict("B", true, &mine, &theirs, None),
            (Some(true), "verify ok".into())
        );
        // A copy that changed under the verify: hashes that differ are no
        // verdict either (da-o2arv), and ones that agree still are.
        assert_eq!(
            verdict("B", false, &mine, &mine, Some("B")),
            (None, "not verified: B changed as it verified".into())
        );
        assert_eq!(
            verdict("B", true, &mine, &mine, Some("this copy")),
            (Some(true), "verify ok".into())
        );
    }

    /// A dial gave way to a session between the two copies whether it
    /// closed itself or the peer refused it, by the words the peer sends;
    /// a refusal as busy applying is no such dial (da-mghh3).
    #[test]
    fn a_dial_gave_way_closed_or_refused_and_not_when_busy_applying() {
        assert!(gave_way(&closed_carried("phone")));
        let sent = |why: &str| format!("{:#}", anyhow!(why.to_string()));
        assert!(gave_way(&refused(sent(CARRIED_WHY), Some(Why::Busy))));
        assert!(!gave_way(&refused(sent(BUSY_WHY), Some(Why::Busy))));
        assert!(!gave_way(&refused(sent(CARRIED_WHY), Some(Why::Other))));
    }

    /// A session waits 60 s on a silent peer, and only a test says less:
    /// the environment natively, [`IDLE_S`] in datum.day (da-eqjri).
    #[test]
    fn idle_is_a_minute_unless_a_test_says() {
        assert_eq!(env::<u64>("DATUM_SYNC_TIMEOUT_S"), None, "set for this run");
        assert_eq!(idle(), Duration::from_secs(60));
        assert_eq!(idle_of(0), Duration::from_secs(60));
        assert_eq!(idle_of(4), Duration::from_secs(4));
    }
}
