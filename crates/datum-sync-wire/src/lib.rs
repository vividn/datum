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
//!   came — until a round in which neither vector moved. Last the verify
//!   hashes, the dialer's first, and the dialer closes.
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
//!   of a big history takes minutes (da-0mwws).

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::pin;
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use data_encoding::BASE32_NOPAD;
use futures_util::future::{select, Either};
use iroh::endpoint::{presets, Incoming, RecvStream, SendStream};
pub use iroh::{Endpoint, EndpointAddr};
use iroh::{RelayMode, SecretKey, Watcher};
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
/// A session in which both sides keep writing still ends.
const MAX_ROUNDS: usize = 16;
/// Ops per apply — the first, where a store [`yields`](Store::yields).
const CHUNK: usize = 64;
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
    async fn ops_since(&self, peer: &str, since: &Vector) -> Result<Vec<String>>;
    async fn apply(&self, ops: &[String]) -> Result<Applied>;
    /// Whether [`apply`](Store::apply) awaits work done off this thread,
    /// so that a session can send meanwhile: a browser's CLI process.
    fn yields(&self) -> bool {
        false
    }
    /// `done` of a session's `of` received ops are applied.
    fn progress(&self, _done: usize, _of: usize) {}
    /// `done` of the `of` ops a round sends are on the wire.
    fn sent(&self, _done: usize, _of: usize) {}
    /// `n` of the ops a round receives have come.
    fn got(&self, _n: usize) {}
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
    /// member this copy has not itself seen (da-fly3f).
    async fn record_session(
        &self,
        peer: &str,
        vector: Option<&Vector>,
        verify: Option<bool>,
        error: Option<&str>,
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

/// How long a session may go with no frame moving either way
/// (`DATUM_SYNC_TIMEOUT_S`, for the tests).
fn idle() -> Duration {
    Duration::from_secs(env("DATUM_SYNC_TIMEOUT_S").unwrap_or(60))
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
    /// Where the dialer last saw the chain's members (da-fly3f); a redeemer,
    /// not yet one of them, says none.
    #[serde(default)]
    known: Vec<EndpointAddr>,
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
    /// The chain of one this copy left to redeem the invite, and the chain
    /// it joined (da-h7pnc).
    pub left: Option<(String, String)>,
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

/// Apply `ops` in chunks, and between chunks send a [`BUSY`]: a [`CHUNK`]
/// at a time, or where the store [`yields`](Store::yields), twice the last
/// up to [`MAX_CHUNK`], each run [`busy`].
async fn apply(
    store: &impl Store,
    s: &mut SendStream,
    ops: &[String],
    rep: &mut Report,
) -> Result<()> {
    // Test knob: the keepalives off (the mutation control).
    let keepalive = env::<u8>("DATUM_SYNC_TEST_KEEPALIVE") != Some(0);
    let (mut done, mut size) = (0, CHUNK);
    while done < ops.len() {
        if done > 0 && keepalive {
            put(s, BUSY).await?;
        }
        let chunk = &ops[done..ops.len().min(done + size)];
        let applied = match store.yields() {
            false => store.apply(chunk).await?,
            true => {
                size = (size * 2).min(MAX_CHUNK);
                busy(s, store.apply(chunk)).await?
            }
        };
        done += chunk.len();
        store.progress(done, ops.len());
        for why in &applied.refused {
            eprintln!("refused from {}: {why}", rep.label);
        }
        rep.got += applied.applied;
        rep.conflicts += applied.conflicts;
    }
    Ok(())
}

/// Swap ops until neither side's vector moves, then compare state hashes.
async fn exchange(
    store: &impl Store,
    s: &mut SendStream,
    r: &mut RecvStream,
    dialer: bool,
    rep: &mut Report,
) -> Result<()> {
    let mut last = None;
    for _ in 0..MAX_ROUNDS {
        let mine = store.vector().await?;
        put_json(s, &mine).await?;
        let theirs: Vector = get_json(r, MAX_FRAME).await?;
        rep.vector = theirs.clone();
        let round = (mine, theirs);
        if last.as_ref() == Some(&round) {
            break;
        }
        let ops = store.ops_since(&rep.id, &round.1).await?;
        let send = async {
            for (i, op) in ops.iter().enumerate() {
                put(s, op.as_bytes()).await?;
                store.sent(i + 1, ops.len());
            }
            put(s, b"").await
        };
        let recv = async {
            let mut lines = Vec::new();
            loop {
                let f = get(r, MAX_FRAME).await?;
                if f.is_empty() {
                    return Ok::<_, anyhow::Error>(lines);
                }
                lines.push(String::from_utf8(f)?);
                store.got(lines.len());
            }
        };
        let ((), lines) = futures_util::future::try_join(send, recv).await?;
        rep.sent += ops.len();
        apply(store, s, &lines, rep).await?;
        last = Some(round);
    }
    // A big history hashes for minutes on each side at once (da-0mwws).
    store.verifying();
    let hash = busy(s, store.verify(&rep.id)).await?;
    // The acceptor answers only after it has read the dialer's hash, so
    // the dialer's close can never cut a frame still in flight.
    let theirs: String = if dialer {
        put_json(s, &hash).await?;
        get_json(r, MAX_HELLO).await?
    } else {
        let h = get_json(r, MAX_HELLO).await?;
        put_json(s, &hash).await?;
        h
    };
    let mine = last.map(|round| round.0).unwrap_or_default();
    (rep.verified, rep.verify) = verdict(&rep.label, theirs == hash, &mine, &rep.vector);
    Ok(())
}

/// A session's verify, and its words. Hashes that differ are a MISMATCH only
/// over the same ops: with one copy ahead of the other they could not agree,
/// and it says who is ahead by how many ops instead. A reader ahead of a
/// member stays so, since it never relays (da-t3ey2).
fn verdict(label: &str, same: bool, mine: &Vector, theirs: &Vector) -> (Option<bool>, String) {
    let ahead = |a: &Vector, of: &Vector| -> i64 {
        a.iter()
            .map(|(e, s)| (s - of.get(e).copied().unwrap_or(0)).max(0))
            .sum()
    };
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
    match apart.is_empty() {
        _ if same => (Some(true), "verify ok".into()),
        true => (Some(false), "verify MISMATCH".into()),
        false => (None, format!("not verified: {}", apart.join(", "))),
    }
}

/// Record how a session with `id` ended. A peer that is neither a member
/// nor paired (a refused redemption) has no row to record it in.
async fn record(store: &impl Store, id: &str, rep: &Result<Report>) {
    let (vector, verify, error, met) = match rep {
        Ok(r) => (Some(&r.vector), r.verified, None, &r.met[..]),
        Err(e) => (
            e.downcast_ref::<Lost>().map(|Lost(r, _)| &r.vector),
            None,
            Some(format!("{e:#}")),
            &[][..],
        ),
    };
    let _ = store
        .record_session(id, vector, verify, error.as_deref(), met)
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
    let rep = session(store, ep, addr, invite).await;
    record(store, &id, &rep).await;
    rep
}

async fn session(
    store: &impl Store,
    ep: &Endpoint,
    addr: EndpointAddr,
    invite: Option<&Redeem<'_>>,
) -> Result<Report> {
    ensure!(addr.id != ep.id(), COPY);
    let id = addr.id.to_string();
    let conn = within(async { Ok(ep.connect(addr, ALPN).await?) })
        .await
        .map_err(|e| unreachable(ep, &id, e))?;
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
        known: match invite {
            Some(_) => Vec::new(),
            None => known(store).await,
        },
        version: store.version(),
    };
    put_json(&mut s, &hello).await?;
    let mut reply = get_json(&mut r, MAX_HELLO).await?;
    if hello.proves && matches!(reply, Reply::Prove) {
        put_json(&mut s, &store.records().await?).await?;
        reply = get_json(&mut r, MAX_HELLO).await?;
    }
    let (label, added, met) = match reply {
        Reply::Welcome {
            label,
            addr,
            known,
            version,
        } => {
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
                Ok(added)
            };
            match admitted.await {
                Ok(added) => (label, added, known),
                Err(e) => {
                    conn.close(0u32.into(), b"refused");
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
    let done = exchange(store, &mut s, &mut r, true, &mut rep).await;
    if let (Some(old), Ok(Some((new, _)))) = (left, store.chain().await) {
        rep.left = Some((old, new));
    }
    conn.close(0u32.into(), b"done");
    match done {
        Ok(()) => Ok(rep),
        Err(e) => Err(Lost(rep, e).into()),
    }
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
    let conn = within(async { Ok(ep.connect(addr, DIFF).await?) })
        .await
        .map_err(|e| unreachable(ep, &id, e))?;
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

/// Answer one incoming connection: a peer, or the redemption of an invite.
pub async fn serve(store: &impl Store, ep: &Endpoint, incoming: Incoming) -> Result<Report> {
    let conn = incoming.await?;
    let id = conn.remote_id().to_string();
    let (mut s, mut r) = within(async { Ok(conn.accept_bi().await?) }).await?;
    let hello: Hello = get_json(&mut r, MAX_HELLO).await?;
    // A diff admits no one: it answers a member this copy holds, redeems
    // no invite, and asks for no records to admit one it has not heard of.
    let diff = conn.alpn() == DIFF;
    let mut records: Option<Vec<String>> = None;
    let verdict = loop {
        let verdict = match conn.remote_id() == ep.id() {
            true => Err((Why::Other, COPY.to_string())),
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
                .map_err(|e| {
                    (
                        e.downcast_ref::<Refusal>().map_or(Why::Other, |r| r.0),
                        format!("{e:#}"),
                    )
                }),
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
        Err((kind, why)) => {
            let reply = Reply::refusal(&hello, kind, why.clone());
            put_json(&mut s, &reply).await?;
            let _ = timeout(Duration::from_secs(5), conn.closed()).await;
            bail!("refused {} ({}): {why}", hello.label, short(&id));
        }
    };
    let welcome = Reply::Welcome {
        label: store.label(),
        addr: ep.addr(),
        known: self::known(store).await,
        version: store.version(),
    };
    put_json(&mut s, &welcome).await?;
    let redeemed = hello.invite.is_some() && !diff;
    let mut rep = Report::new(hello.label, id.clone(), !known, redeemed, hello.known);
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
        Err(e) => Err(Lost(rep, e).into()),
    };
    record(store, &id, &done).await;
    done
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
    for addr in &ticket.addrs {
        rep = dial(store, ep, addr.clone(), Some(&invite)).await;
        // Unreachable, or a member not yet holding the invite: the next
        // address may admit. Any other refusal stops here, a paused inviter
        // too: going on would report whatever the last address said, often
        // «could not reach», in place of the refusal that says what to do.
        let next = rep.as_ref().err().and_then(failure);
        if !matches!(
            next,
            Some(Failure::Unreachable | Failure::Refused(Why::UnknownInvite))
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
    use serde_json::json;

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
            verdict("B", false, &mine, &theirs),
            (
                None,
                "not verified: this copy is 3 changes ahead of B, \
                 B is 1 change ahead of this copy"
                    .into()
            )
        );
        assert_eq!(
            verdict("B", false, &mine, &mine),
            (Some(false), "verify MISMATCH".into())
        );
        assert_eq!(
            verdict("B", true, &mine, &theirs),
            (Some(true), "verify ok".into())
        );
    }
}
