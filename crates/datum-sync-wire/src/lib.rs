//! Easy Sync's session protocol (da-rugg8, split from storage in da-6kpj5):
//! two copies of a database sync over a QUIC connection. The merge is
//! datum-core's; this crate moves the ops, over a [`Store`] that holds them
//! — `datum-sync`'s over `Db` natively, the CLI's in datum.day's sync worker
//! ([`web`]). It builds for wasm32-unknown-unknown, where iroh is relay-only.
//!
//! - **Endpoint.** The iroh endpoint's key IS the db's endpoint key, so the
//!   iroh endpoint id is the `endpoint` every op of this copy carries. ALPN
//!   `datum/sync/2`, and a [`Net`]: n0's relays and discovery, one relay of
//!   the caller's (`DATUM_SYNC_RELAY`, the tests' local one), or natively
//!   one bound address with neither (`DATUM_SYNC_BIND`).
//! - **Wire.** One bidirectional stream of frames, each a big-endian u32
//!   length and its bytes. The dialer sends a [`Hello`], the acceptor a
//!   [`Reply`], each after the store's [`Store::handshake`] has admitted the
//!   other. Then rounds: each side sends its version vector, then the ops
//!   the other lacks, one frame each, and an empty frame, and applies what
//!   came — until a round in which neither vector moved. Last the verify
//!   hashes, the dialer's first, and the dialer closes.
//! - **Record.** Every session this copy dials, and every one it serves past
//!   the handshake, is recorded as it ends — the peer's vector, the verify,
//!   or the error it failed with — for `datum sync status` (da-rcb5m).
//! - **Progress.** A session fails only when no frame has moved either way
//!   for [`idle`] (da-039jx). A long apply is chunked, and between chunks
//!   the side applying sends a [`BUSY`] frame, which a reader skips: a
//!   first sync of 32k ops takes minutes and is not a dead peer. A store
//!   whose apply runs elsewhere ([`Store::yields`], a browser's CLI) is sent
//!   BUSYs while it runs too, and its chunks double (da-89ujs): each is a
//!   process that commits, and a commit rewrites every index page the chunk
//!   touched, however few ops it held.

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
const MAX_FRAME: usize = 64 << 20;
const MAX_HELLO: usize = 64 << 10;
/// A session in which both sides keep writing still ends.
const MAX_ROUNDS: usize = 16;
/// Ops per apply — the first, where a store [`yields`](Store::yields).
const CHUNK: usize = 64;
/// The most ops a doubling chunk grows to: one CLI process applying 8,192
/// ops peaked at 108 MiB, and a command typed meanwhile waits for it.
const MAX_CHUNK: usize = 8192;
/// datum-core's `peers::COPY`, which this crate cannot see.
const COPY: &str =
    "this peer is a copy of this database (same endpoint key); run `datum sync chain rekey` on one of them";

pub type Vector = BTreeMap<String, i64>;

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
    /// The state hash, over what a session with `peer` shares.
    async fn verify(&self, peer: &str) -> Result<String>;
    /// `peer` introduced itself as `label` at `addr`: record it, and when
    /// it `dialed` this copy, admit it (as a peer, or by `invite`) or fail
    /// saying why not; a redeemer names its own `chain`, and how many
    /// entries it `holds`. When this copy dialed it, `invite` is the one
    /// it just redeemed there. Whether it is new to this copy comes back.
    #[allow(clippy::too_many_arguments)]
    async fn handshake(
        &self,
        peer: &str,
        label: &str,
        addr: &EndpointAddr,
        invite: Option<&str>,
        chain: Option<&str>,
        holds: u64,
        dialed: bool,
    ) -> Result<bool>;
    /// How many entries this copy holds, as `datum count` counts them (life
    /// data: no spec, layout or alias): a reader invite refuses a redeemer
    /// holding any, which a reader's fold would hide (da-tti3w).
    async fn holds(&self) -> Result<u64>;
    async fn record_session(
        &self,
        peer: &str,
        vector: Option<&Vector>,
        verify: Option<bool>,
        error: Option<&str>,
    ) -> Result<()>;
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

#[derive(Serialize, Deserialize)]
struct Hello {
    label: String,
    addr: EndpointAddr,
    invite: Option<String>,
    /// A redeemer's own chain, empty for none (da-mepav).
    chain: Option<String>,
    /// A redeemer's entries, as `datum count` counts them (da-tti3w).
    #[serde(default)]
    holds: u64,
    /// The dialer reads a [`Reply::Refusal`]; one that does not say so, a
    /// copy before da-b6xh6, is refused the old way.
    #[serde(default)]
    typed: bool,
}

#[derive(Serialize, Deserialize)]
enum Reply {
    Welcome {
        label: String,
        addr: EndpointAddr,
    },
    /// A refusal's words alone: to and from a copy before da-b6xh6.
    Refused(String),
    Refusal {
        why: String,
        kind: Why,
    },
}

impl Reply {
    /// Refuse a dialer with `kind` and `why`, in the form it reads.
    fn refusal(typed: bool, kind: Why, why: String) -> Self {
        match typed {
            true => Reply::Refusal { why, kind },
            false => Reply::Refused(why),
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
    pub verified: bool,
    /// The peer's version vector, as it last sent it.
    pub vector: Vector,
    /// The chain of one this copy left to redeem the invite, and the chain
    /// it joined (da-h7pnc).
    pub left: Option<(String, String)>,
}

impl Report {
    fn new(label: String, id: String, added: bool, redeemed: bool) -> Self {
        Report {
            label,
            id,
            added,
            redeemed,
            sent: 0,
            got: 0,
            conflicts: 0,
            verified: false,
            vector: BTreeMap::new(),
            left: None,
        }
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: sent {}, got {}, conflicts {}, verify {}",
            self.label,
            self.sent,
            self.got,
            self.conflicts,
            if self.verified { "ok" } else { "MISMATCH" }
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

/// Apply `ops` in chunks, and between chunks send a [`BUSY`]: a [`CHUNK`]
/// at a time, or where the store [`yields`](Store::yields), twice the last
/// up to [`MAX_CHUNK`], with a BUSY every quarter of [`idle`] while each runs.
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
                // A BUSY is sent outside the race: one dropped half-sent
                // would cut the stream's framing.
                let mut work = pin!(store.apply(chunk));
                loop {
                    match select(work.as_mut(), pin!(sleep(idle() / 4))).await {
                        Either::Left((applied, _)) => break applied?,
                        Either::Right(_) if keepalive => put(s, BUSY).await?,
                        Either::Right(_) => {}
                    }
                }
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
            for op in &ops {
                put(s, op.as_bytes()).await?;
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
            }
        };
        let ((), lines) = futures_util::future::try_join(send, recv).await?;
        rep.sent += ops.len();
        apply(store, s, &lines, rep).await?;
        last = Some(round);
    }
    let hash = store.verify(&rep.id).await?;
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
    rep.verified = theirs == hash;
    Ok(())
}

/// Record how a session with `id` ended. A peer that is neither a member
/// nor paired (a refused redemption) has no row to record it in.
async fn record(store: &impl Store, id: &str, rep: &Result<Report>) {
    let (vector, verify, error) = match rep {
        Ok(r) => (Some(&r.vector), Some(r.verified), None),
        Err(e) => (
            e.downcast_ref::<Lost>().map(|Lost(r, _)| &r.vector),
            None,
            Some(format!("{e:#}")),
        ),
    };
    let _ = store
        .record_session(id, vector, verify, error.as_deref())
        .await;
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
        typed: true,
    };
    put_json(&mut s, &hello).await?;
    let (label, added) = match get_json(&mut r, MAX_HELLO).await? {
        Reply::Welcome { label, addr } => {
            // Its own handshake may refuse too (a paused copy): only then
            // does it leave. Given the invite, it marks where this copy was
            // admitted, so a join cut short before any op applies resumes
            // there (da-38eye).
            let admitted = async {
                let added = store
                    .handshake(&id, &label, &addr, invite.map(|r| r.secret), None, 0, true)
                    .await?;
                if left.is_some() {
                    store.leave(false).await?;
                }
                Ok(added)
            };
            match admitted.await {
                Ok(added) => (label, added),
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
    };
    let mut rep = Report::new(label, id, added, invite.is_some());
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
    let verdict = match conn.remote_id() == ep.id() {
        true => Err((Why::Other, COPY.to_string())),
        false => store
            .handshake(
                &id,
                &hello.label,
                &hello.addr,
                hello.invite.as_deref(),
                hello.chain.as_deref(),
                hello.holds,
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
    let known = match verdict {
        Ok(added) => !added,
        Err((kind, why)) => {
            let reply = Reply::refusal(hello.typed, kind, why.clone());
            put_json(&mut s, &reply).await?;
            let _ = timeout(Duration::from_secs(5), conn.closed()).await;
            bail!("refused {} ({}): {why}", hello.label, short(&id));
        }
    };
    let welcome = Reply::Welcome {
        label: store.label(),
        addr: ep.addr(),
    };
    put_json(&mut s, &welcome).await?;
    let mut rep = Report::new(hello.label, id.clone(), !known, hello.invite.is_some());
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
        .alpns(vec![ALPN.to_vec()])
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
        let reply = Reply::refusal(old.typed, Why::InviteUsed, "invite already used".into());
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            json!({"Refused": "invite already used"})
        );
        // A dialer that says so is refused with the kind.
        let reply = Reply::refusal(true, Why::InviteUsed, "invite already used".into());
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            json!({"Refusal": {"why": "invite already used", "kind": "invite-used"}})
        );
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
}
