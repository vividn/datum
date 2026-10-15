//! Easy Sync over iroh, natively (da-rugg8): `datum-sync-wire`'s session
//! over a [`Store`] on `Db`, and what only a native copy does — serve an
//! invite, `sync run`'s daemon, the QR, the host's name. The protocol, the
//! endpoint and the pairing rules are the wire crate's and datum-core's, so
//! datum.day's copy runs the same ones (da-6kpj5).
//!
//! - **Net.** n0's relays and discovery; `DATUM_SYNC_RELAY=<url>` one relay
//!   and nothing else (a private relay, the tests' local one);
//!   `DATUM_SYNC_BIND=<ip:port>` that one address with no relay and no
//!   discovery (a LAN, the tests).
//! - **Peers.** A peer is a member of this copy's sync chain (da-odwc7,
//!   da-6177s), and `sync_peers` only remembers where members were last
//!   seen: any member admits the redeemer of the chain's invite, and only
//!   unrevoked members are served or dialed (`Db::sync_handshake`). Anyone
//!   else is refused at the handshake, and so is this copy's own key.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, ensure, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use datum_core::db::peers::Peer;
use datum_core::db::{chain, Db};
use datum_core::error::{DatumError, Refusal as CoreRefusal};
use datum_core::migrations::CURRENT_SCHEMA_VERSION;
use datum_parse::time::Zone;
use datum_sync_wire::{
    dial, failure, redeem, serve, unix_now, About, Applied, Diffed, Digest, Endpoint, EndpointAddr,
    Failure, Lost, Net, Refusal, Report, Store, Ticket, Vector, Version, Why,
};
use futures_util::stream::{FuturesUnordered, StreamExt};
use iroh::Watcher;
use tokio::time::Instant;

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn short(id: &str) -> &str {
    &id[..id.len().min(10)]
}

/// How this copy names itself by default (da-ey3hz): `<user>@<host>`, the
/// user from `USER` (else `LOGNAME`), so two accounts on one host differ.
pub fn label() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most `buf.len()` bytes into `buf`.
    let host = if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0 {
        let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..n]).into_owned()
    } else {
        "unknown".into()
    };
    match std::env::var("USER").or_else(|_| std::env::var("LOGNAME")) {
        Ok(user) if !user.is_empty() => chain::fit(&format!("{user}@{host}"), chain::MAX_LABEL),
        _ => chain::fit(&host, chain::MAX_LABEL),
    }
}

/// The label this copy takes in its chain unless told one: [`label`], and
/// where another member already carries that, the database's name after it,
/// then a short id too. A label is printed on every member for the chain's
/// life, so it grows only where two would read alike (da-ey3hz).
fn named(db: &Db) -> Result<String> {
    // The chain as its inviter held it: a member whose describe had not
    // reached the inviter carries its Hello's label here, so two copies
    // joining one after the other can both take the same longer label.
    let base = label();
    let with_db = match db.path().and_then(|p| p.file_stem()) {
        Some(name) => format!("{base} ({})", name.to_string_lossy()),
        None => base.clone(),
    };
    Ok(db.sync_untaken_label(&[base, chain::fit(&with_db, chain::MAX_LABEL)])?)
}

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

/// The session runs on the calling thread, and blocks it while it applies;
/// iroh's own tasks run on a worker, so frames still go out meanwhile, and so
/// does what says how far the session is ([`telling`]).
fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?)
}

// ─── The store ──────────────────────────────────────────────────────────────

/// A session's store: this copy's `Db`, on the session's own thread, where
/// a command that says how its one session goes hears of it, what to do
/// the first time a peer this copy dialed welcomes it, and the label a
/// `connect --label` introduces it by.
struct DbStore<'a>(
    &'a Db,
    Option<&'a Mutex<Told>>,
    Cell<Option<&'a dyn Fn()>>,
    Option<&'a str>,
);

impl<'a> DbStore<'a> {
    fn new(db: &'a Db) -> Self {
        DbStore(db, None, Cell::new(None), None)
    }

    fn tell(&self, f: impl FnOnce(&mut Told)) {
        if let Some(told) = self.1 {
            f(&mut told.lock().unwrap());
        }
    }
}

/// How often a session says what it is doing.
const WAITING: Duration = Duration::from_secs(5);

/// What `connect` and `offer` say of their session as it goes (da-nyg6u):
/// whom it is with, and how far its ops are.
struct Told {
    began: Instant,
    said: Instant,
    /// «still waiting for X», and from the handshake «syncing X»; empty
    /// says nothing.
    whom: String,
    /// Ops sent and of how many, ops received, ops applied and of how many.
    sent: (usize, usize),
    got: usize,
    applied: (usize, usize),
    /// Every op is applied, and the two copies hash what they hold.
    verifying: bool,
}

impl Told {
    fn new(whom: String) -> Arc<Mutex<Self>> {
        let now = Instant::now();
        Arc::new(Mutex::new(Told {
            began: now,
            said: now,
            whom,
            sent: (0, 0),
            got: 0,
            applied: (0, 0),
            verifying: false,
        }))
    }

    /// The session is with `label` from here. One this copy `serves` starts
    /// its clock, and the redeemer of an invite is named at once.
    fn met(&mut self, label: &str, serves: bool, redeems: bool) {
        if serves {
            (self.began, self.said) = (Instant::now(), Instant::now());
        }
        if serves && redeems {
            eprintln!("{label} is joining: sending…");
        }
        self.whom = format!("syncing {label}");
        (self.sent, self.got, self.applied, self.verifying) = Default::default();
    }

    /// Say what the session is doing, if it last did [`WAITING`] ago.
    fn say(&mut self) {
        if Instant::now() < self.said + WAITING {
            return;
        }
        self.said = Instant::now();
        let ((sent, of), (done, all)) = (self.sent, self.applied);
        let how = if self.verifying {
            ": verifying that both copies agree".into()
        } else if done < all {
            format!(": {done} of {all} ops applied")
        } else if sent < of {
            format!(": {sent} of {of} ops sent")
        } else if all == 0 && self.got > 0 {
            format!(": {} ops received", self.got)
        } else if of > 0 {
            format!(": {of} ops sent, waiting for it to apply them")
        } else {
            String::new()
        };
        if !self.whom.is_empty() {
            let s = self.began.elapsed().as_secs();
            eprintln!("{}{how} ({s}s)", self.whom);
        }
    }
}

/// Run `session`, saying what it is doing each [`WAITING`] — from a worker:
/// the session holds its own thread while it applies, and a timer beside it
/// there said nothing for a minute, then every line it had missed at once.
async fn telling<T>(told: &Arc<Mutex<Told>>, session: impl Future<Output = T>) -> T {
    let told = told.clone();
    let saying = tokio::spawn(async move {
        loop {
            let due = told.lock().unwrap().said + WAITING;
            tokio::time::sleep_until(due).await;
            told.lock().unwrap().say();
        }
    });
    let out = session.await;
    saying.abort();
    out
}

impl Store for DbStore<'_> {
    /// The label it joins as, else the one its chain knows it by, so a peer
    /// names it as `chain list` does.
    fn label(&self) -> String {
        let db = self.0;
        let mine = || {
            Some(
                db.sync_chain()
                    .ok()?
                    .member(&db.sync_endpoint_id().ok()?)?
                    .label
                    .clone(),
            )
        };
        self.3.map(String::from).or_else(mine).unwrap_or_else(label)
    }

    async fn chain(&self) -> Result<Option<(String, usize)>> {
        let c = self.0.sync_chain()?;
        let members = c.members.iter().filter(|m| m.revoked.is_none()).count();
        Ok(c.exists().then_some((c.id, members)))
    }

    async fn leave(&self, dry_run: bool) -> Result<()> {
        Ok(self.0.sync_chain_leave_alone(dry_run)?)
    }

    async fn vector(&self) -> Result<Vector> {
        Ok(self.0.sync_vector()?)
    }

    async fn ops_since(&self, peer: &str, since: &Vector) -> Result<Vec<String>> {
        Ok(self.0.sync_ops_since(since, false, Some(peer))?)
    }

    async fn apply(&self, ops: &[String]) -> Result<Applied> {
        // Test knob: a slow apply. The delay BLOCKS the thread, as a slow
        // `sync_apply` does: an async sleep would yield, and the test would
        // pass on the runtime da-039jx replaced.
        let delay = Duration::from_millis(env("DATUM_SYNC_TEST_APPLY_DELAY_MS").unwrap_or(0));
        std::thread::sleep(delay * ops.len() as u32);
        let s = self.0.sync_apply(&ops.join("\n"))?;
        Ok(Applied {
            applied: s.applied,
            conflicts: s.conflicts,
            refused: s.refused,
        })
    }

    fn progress(&self, done: usize, of: usize) {
        self.tell(|t| t.applied = (done, of));
    }

    fn sent(&self, done: usize, of: usize) {
        self.tell(|t| t.sent = (done, of));
    }

    fn got(&self, n: usize) {
        self.tell(|t| t.got = n);
    }

    fn verifying(&self) {
        self.tell(|t| t.verifying = true);
    }

    /// Hashed on a connection of its own, off the session's thread, which
    /// sends meanwhile: a big history hashes for minutes (da-0mwws).
    async fn verify(&self, peer: &str) -> Result<String> {
        let (me, peer) = (self.0.sync_endpoint_id()?, peer.to_string());
        let hash = move |db: &Db| -> Result<String> {
            // Test knob: a slow verify, holding its thread as a big one does.
            let delay = env("DATUM_SYNC_TEST_VERIFY_DELAY_S").unwrap_or(0);
            std::thread::sleep(Duration::from_secs(delay));
            Ok(db.sync_state_hash_as(Some(&me), Some(&peer))?.0)
        };
        match self.0.path().and_then(|p| p.to_str()).map(String::from) {
            Some(path) => {
                tokio::task::spawn_blocking(move || hash(&Db::open_read_only(&path)?)).await?
            }
            None => hash(self.0),
        }
    }

    async fn digest(&self, peer: &str) -> Result<(String, Digest)> {
        Ok(self.0.sync_digest(Some(peer))?)
    }

    async fn held(&self, peer: &str, hids: &[String]) -> Result<Vec<serde_json::Value>> {
        Ok(self.0.sync_held(Some(peer), hids)?)
    }

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
    ) -> Result<bool> {
        let addr = serde_json::to_string(addr)?;
        let records = records.map(|r| r.join("\n"));
        let added = self
            .0
            .sync_handshake(
                peer,
                label,
                &addr,
                invite,
                chain,
                holds,
                records.as_deref(),
                dialed,
            )
            .map_err(refusal)?;
        self.tell(|t| t.met(label, !dialed, invite.is_some()));
        if dialed {
            if let Some(welcomed) = self.2.take() {
                welcomed();
            }
        }
        // A copy before da-3zqyv says nothing; recording never stops a sync.
        if version.schema > 0 {
            let said = self
                .0
                .sync_record_version(peer, &version.datum, version.schema);
            if let Ok(Some(w)) = said {
                eprintln!("⚠ {w}");
            }
        }
        Ok(added)
    }

    async fn records(&self) -> Result<Vec<String>> {
        Ok(self.0.sync_chain_records()?)
    }

    fn version(&self) -> Version {
        Version {
            datum: datum_core::build().into(),
            // Test knob: a copy that says it is newer than it is.
            schema: env("DATUM_SYNC_TEST_SCHEMA").unwrap_or(CURRENT_SCHEMA_VERSION),
        }
    }

    async fn holds(&self) -> Result<u64> {
        Ok(self.0.sync_holds()?)
    }

    async fn record_session(
        &self,
        peer: &str,
        vector: Option<&Vector>,
        verify: Option<bool>,
        error: Option<&str>,
        met: &[EndpointAddr],
    ) -> Result<()> {
        self.0.sync_record_session(peer, vector, verify, error)?;
        if met.is_empty() {
            return Ok(());
        }
        Ok(self.0.sync_introduce(&serde_json::to_string(met)?)?)
    }

    async fn known(&self) -> Result<Vec<EndpointAddr>> {
        known(self.0)
    }
}

/// A handshake's error, with its kind where it is a [`Refusal`] the wire
/// sends (da-b6xh6).
fn refusal(e: DatumError) -> anyhow::Error {
    let DatumError::Refused(kind, why) = e else {
        return e.into();
    };
    let kind = match kind {
        CoreRefusal::UnknownInvite => Why::UnknownInvite,
        CoreRefusal::InviteUsed => Why::InviteUsed,
        CoreRefusal::InviteExpired => Why::InviteExpired,
        CoreRefusal::Paused => Why::Paused,
        CoreRefusal::NotMember => Why::NotMember,
    };
    Refusal(kind, why).into()
}

fn addr(p: &Peer) -> Result<EndpointAddr> {
    Ok(serde_json::from_str(&p.addrs)?)
}

/// The peers to sync with: in a chain, those still members, and whose
/// address this copy knows.
fn synced_peers(db: &Db) -> Result<Vec<Peer>> {
    Ok(db
        .sync_peers()?
        .into_iter()
        .filter(|p| p.synced && addr(p).is_ok())
        .collect())
}

/// Where this copy last saw them: what a ticket lists, and a handshake
/// passes on.
fn known(db: &Db) -> Result<Vec<EndpointAddr>> {
    Ok(synced_peers(db)?
        .iter()
        .filter_map(|p| addr(p).ok())
        .collect())
}

/// Say so if `e` is a session that redeemed an invite and then failed: the
/// two copies are paired, and only the first sync is unfinished — `then`.
fn paired_unfinished(e: &anyhow::Error, then: &str) -> bool {
    match e.downcast_ref::<Lost>() {
        Some(Lost(rep, _)) if rep.redeemed => {
            println!("paired with {}; {then}", rep.label);
            true
        }
        _ => false,
    }
}

// ─── The endpoint ───────────────────────────────────────────────────────────

async fn bind(db: &Db) -> Result<Endpoint> {
    let net = match (
        std::env::var("DATUM_SYNC_BIND"),
        std::env::var("DATUM_SYNC_RELAY"),
    ) {
        (Ok(addr), _) => Net::Local(addr.parse::<SocketAddr>().context("DATUM_SYNC_BIND")?),
        (_, Ok(url)) => Net::Relay(url),
        _ => Net::N0,
    };
    let ep = datum_sync_wire::bind(db.sync_endpoint_secret()?, &net).await?;
    // Its sessions are this endpoint's: on a file purged or rekeyed under
    // them they stop, and write nothing into it (da-otz0d). Until the
    // command ends, which unbinds (`commands::sync`).
    db.sync_session_as(Some(&ep.id().to_string()));
    Ok(ep)
}

fn print_added(rep: &Report) {
    if let Some((old, new)) = &rep.left {
        println!("this copy's own sync chain {old} (just itself) is replaced by sync chain {new}");
    }
    if rep.added {
        println!("added sync peer {} ({})", rep.label, short(&rep.id));
    }
    println!("{rep}");
}

// ─── Commands ───────────────────────────────────────────────────────────────

/// Print how to redeem the invite whose secret is `secret`, and serve until
/// it is redeemed (or expires). `datum sync chain invite` has written the invite
/// into the chain, so any member can admit its redeemer: the ticket lists
/// them all.
pub fn offer(db: &Db, secret: &str, expires: Duration, expires_at: &str, qr: bool) -> Result<()> {
    let secret = secret.to_string();
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let told = Told::new(String::new());
        let store = DbStore(db, Some(&told), Cell::new(None), None);
        let mut addrs = vec![ep.addr()];
        addrs.extend(known(db)?);
        let chain = db.sync_chain()?;
        let from = chain
            .member(&db.sync_endpoint_id()?)
            .map(|m| m.label.clone());
        let about = About {
            from,
            chain: Some(chain.id),
            expires: Some(unix_now() + expires.as_secs()),
        };
        let link = Ticket {
            addrs,
            secret,
            about,
        }
        .link()?;
        println!("One-time invite, until {expires_at}. On the other copy run:\n");
        // Quoted: zsh's `extendedglob` reads the `#` as a glob (da-sin5b).
        // The link is a fixed prefix and base32, so it holds no `'`.
        println!("  datum sync chain connect '{link}'\n");
        println!("or open the link in a browser.\n");
        if qr {
            let code = qrcode::QrCode::new(link.as_bytes())?;
            println!(
                "{}",
                code.render::<qrcode::render::unicode::Dense1x2>().build()
            );
        }
        let deadline = tokio::time::sleep(expires);
        tokio::pin!(deadline);
        let result = loop {
            tokio::select! {
                _ = &mut deadline => break Err(anyhow!("the invite expired unredeemed")),
                incoming = ep.accept() => {
                    let Some(incoming) = incoming else { break Ok(()) };
                    match telling(&told, serve(&store, &ep, incoming)).await {
                        Ok(rep) => {
                            print_added(&rep);
                            if rep.redeemed { break Ok(()) }
                        }
                        Err(e) => {
                            eprintln!("{e:#}");
                            if paired_unfinished(&e, UNFINISHED) { break Ok(()) }
                        }
                    }
                }
            }
        };
        ep.close().await;
        result
    })
}

const UNFINISHED: &str = "the first sync did not finish: run `datum sync`";

/// `datum sync chain connect <ticket>`: redeem an invite and sync, with the
/// first of the ticket's addresses that admits it. It says at once whom it
/// waits for and until when, then every [`WAITING`] that it still is, and
/// once it is answered how far the sync is; a join that fails ends saying why
/// and what to do next (da-0tawk). `admitted` runs once it is welcomed, a
/// member from then on, before its first sync (da-qr7gz). `name`: the label
/// it joins as (`--label`), else [`named`].
pub fn connect(db: &Db, ticket: &str, name: Option<&str>, admitted: &dyn Fn()) -> Result<()> {
    let t = Ticket::decode(ticket)?;
    let from = t.about.from.as_deref().unwrap_or("the inviting device");
    let until = t
        .about
        .expires
        .and_then(|at| DateTime::from_timestamp(at as i64, 0))
        .map(|at| {
            format!(
                " (the invite holds until {})",
                Zone::host().local_naive(at).format("%H:%M")
            )
        });
    eprintln!(
        "joining {from}'s sync chain: waiting for it to answer{}\n\
         it answers while it is online: datum.day open on that database, its \
         sync daemon, or `datum sync chain invite` still running there",
        until.as_deref().unwrap_or_default()
    );
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let told = Told::new(format!("still waiting for {from}"));
        let store = DbStore(db, Some(&told), Cell::new(Some(admitted)), name);
        let rep = telling(&told, redeem(&store, &ep, &t)).await;
        ep.close().await;
        // Admitted, it says what it is; its peers hear on the next sync. A
        // copy refused (one already in a chain) writes nothing.
        let joined =
            "this copy was admitted to its sync chain, but the first sync did not finish: \
                      `datum sync` finishes it";
        let admitted = rep
            .as_ref()
            .map_or_else(|e| paired_unfinished(e, joined), |_| true);
        let me = db.sync_endpoint_id()?;
        if admitted && db.sync_chain()?.member(&me).is_some() {
            let name = name.map_or_else(|| named(db), |n| Ok(n.to_string()))?;
            db.sync_describe_self(&name, "cli")?;
        }
        let rep = match rep {
            Err(e) if !admitted => return Err(next_step(e, from)),
            rep => rep?,
        };
        print_added(&rep);
        Ok(())
    })
}

/// A join that failed, as one line: why, and what to do next — by the
/// failure's kind, never its words (da-b6xh6).
fn next_step(e: anyhow::Error, from: &str) -> anyhow::Error {
    let next = match failure(&e) {
        Some(Failure::Refused(Why::UnknownInvite | Why::InviteUsed | Why::InviteExpired)) => {
            format!("ask {from} for a fresh invite")
        }
        // Its words say «this copy», read on the redeemer's terminal.
        Some(Failure::Refused(Why::Paused)) => format!(
            "sync is paused on {from}, not on this copy: resume it there (`datum sync \
             resume`, or tap «sync paused» on datum.day) and connect again"
        ),
        Some(Failure::Refused(Why::Other | Why::NotMember)) => {
            "`datum sync status` shows where this copy stands".into()
        }
        Some(Failure::Unreachable | Failure::TimedOut) => format!(
            "{from} did not answer: open datum.day on that database (or run \
             `datum sync chain invite` there) and connect again, or ask it for a fresh invite"
        ),
        // Its own words say it already.
        Some(Failure::Late) | None => return e,
    };
    anyhow!("{e:#}. Next: {next}")
}

/// `datum sync`: sync with every member once.
pub fn now_all(db: &Db) -> Result<()> {
    let peers = synced_peers(db)?;
    if peers.is_empty() {
        println!("no sync peers: `datum sync chain invite` makes an invite");
        return Ok(());
    }
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let store = DbStore::new(db);
        let dials = peers
            .iter()
            .map(|p| async { dial(&store, &ep, addr(p)?, None).await });
        let mut failed = 0;
        for (p, rep) in peers
            .iter()
            .zip(futures_util::future::join_all(dials).await)
        {
            match rep {
                Ok(rep) => println!("{rep}"),
                Err(e) => {
                    failed += 1;
                    println!("{}: {e:#}", p.label);
                }
            }
        }
        // After the dials: one of them may have passed its address on.
        let status = db.sync_status()?;
        for p in &status {
            if let Some(why) = p.no_address(&status) {
                println!("{}: {why}", p.label);
            }
        }
        ep.close().await;
        ensure!(failed == 0, "{failed} of {} peer(s) failed", peers.len());
        Ok(())
    })
}

/// `datum sync diff <WHO>`: what this copy and the member `id` hold
/// differently (da-n0ruf). It asks the member, which must be up.
pub fn diff(db: &Db, id: &str) -> Result<Diffed> {
    let peer = synced_peers(db)?.into_iter().find(|p| p.id == id);
    let peer = peer.context("this copy knows no address for it: `datum sync` with it first")?;
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let found = datum_sync_wire::diff(&DbStore::new(db), &ep, addr(&peer)?).await;
        ep.close().await;
        found
    })
}

/// A session and, for a push, the peer's id and label.
type Session<'a> = Pin<Box<dyn Future<Output = (Option<(String, String)>, Result<Report>)> + 'a>>;

/// How long a peer a push failed to reach is left before the next try:
/// [`RETRY`] after the first failure, twice the last gap after each one
/// after it, up to [`RETRY_CAP`] times it (da-u1ul1) —
/// `DATUM_SYNC_TEST_RETRY_MS` in place of [`RETRY`], for the tests.
fn backoff(last: Option<Duration>) -> Duration {
    let first = env("DATUM_SYNC_TEST_RETRY_MS").map_or(RETRY, Duration::from_millis);
    last.map_or(first, |prev| (prev * 2).min(first * RETRY_CAP))
}

const RETRY: Duration = Duration::from_secs(60);
const RETRY_CAP: u32 = 30;

/// A gap as the log says it: whole minutes as such.
fn gap(d: Duration) -> String {
    match d.as_secs() {
        s if s >= 60 && s % 60 == 0 => format!("{} min", s / 60),
        _ => format!("{d:?}"),
    }
}

/// How long `sync run` waits for a write lock another connection holds,
/// against every other connection's 5 s (da-lk1s2). Nobody waits on a
/// background apply, so it is the one that gives way: a person's write
/// waits out the sync, and the sync tries again at its next tick.
const YIELD: Duration = Duration::from_millis(250);

/// `datum sync run`: stay up — serve peers, and push to every peer each
/// time this copy's vector moves (a write here, or ops from another peer).
/// A push does not wait for an earlier one to the same peer: a dial to a
/// peer that is down takes 30s to fail. A failed push is tried again after
/// [`backoff`], whether or not anything new was written, and the gap grows
/// with each failure until the peer reaches this copy or a push reaches it.
/// It dials every member once it is up, and again when its address changes
/// (its network came back, or moved), so a member that backed off from it
/// hears at once that it is up (da-u1ul1).
///
/// It stops, with an error, once the file's endpoint key is no longer the
/// one it bound: a `db purge` or `sync chain rekey` made it another copy
/// (da-8v7ua), and the daemon starts it again only if that copy is chained.
/// And once the file's schema is not this build's: a newer datum migrated it
/// (da-1tnup). A session open then stops as it does ([`bind`]). And with
/// the error `stop` gives, which it asks every tick: the daemon's, once a
/// `connect` or an `invite` takes the database (da-8sah2).
pub fn run(db: &Db, stop: impl Fn() -> Result<()>) -> Result<()> {
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        db.conn().busy_timeout(YIELD)?;
        // Its own holds of the write lock past a second, with what they
        // wrote: what a person's write waited on (da-hyem1).
        let long = Duration::from_millis(env("DATUM_SYNC_LONG_HOLD_MS").unwrap_or(1000));
        db.on_long_hold(long, |held, what| {
            eprintln!("{} held the write lock {:.1} s: {what}", now(), held.as_secs_f64())
        });
        let store = &DbStore::new(db);
        eprintln!("datum sync run: {} is up; Ctrl-C to stop", short(&ep.id().to_string()));
        let mut sessions: FuturesUnordered<Session> = FuturesUnordered::new();
        // Per peer, the vector last pushed to it, and when to retry a failed
        // push with the gap it waits.
        let mut pushed: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
        let mut retry: BTreeMap<String, (Instant, Duration)> = BTreeMap::new();
        let (mut mine, mut seen) = (ep.watch_addr(), ep.addr());
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        loop {
            tokio::select! {
                _ = &mut ctrl_c => break,
                Ok(at) = mine.updated() => {
                    if std::mem::replace(&mut seen, at.clone()) == at {
                        continue;
                    }
                    eprintln!("{} this copy's address changed: dialing every member", now());
                    pushed.clear();
                    // Due now, each keeping its gap: the address moves when
                    // the network goes down too, and a peer down all along
                    // fails again at the gap it had reached.
                    for (at, _) in retry.values_mut() {
                        *at = Instant::now();
                    }
                }
                Some(incoming) = ep.accept() => {
                    let ep = &ep;
                    sessions.push(Box::pin(async move {
                        (None, serve(store, ep, incoming).await)
                    }));
                }
                Some((peer, rep)) = sessions.next() => {
                    match (rep, peer) {
                        (Ok(rep), _) => {
                            // A peer that reached us, or we it, is up again.
                            retry.remove(&rep.id);
                            println!("{} {rep}", now());
                        }
                        // A write held this copy's file past YIELD, or the
                        // peer's: every peer not backing off for being
                        // unreachable is tried again at the next tick. Matched
                        // on the text, which sync_net's test pins:
                        // `run_gives_way_to_a_write_holding_its_file_and_tries_again_at_once`.
                        (Err(e), _) if format!("{e:#}").contains("database is locked") => {
                            eprintln!("{} gave way to a write: {e:#}", now());
                            pushed.clear();
                        }
                        (Err(e), None) => eprintln!("{} incoming: {e:#}", now()),
                        (Err(e), Some((id, label))) => {
                            let wait = backoff(retry.get(&id).map(|r| r.1));
                            eprintln!("{} {label}: {e:#}; next try in {}", now(), gap(wait));
                            pushed.remove(&id);
                            retry.insert(id, (Instant::now() + wait, wait));
                        }
                    }
                }
                _ = tick.tick() => {
                    if let Err(e) = db.sync_endpoint_secret().map_err(Into::into).and_then(|_| stop()) {
                        ep.close().await;
                        return Err(e);
                    }
                    // What a command spooled while another held the lock (da-9q3fr).
                    match db.drain_spool() {
                        Ok(d) => {
                            if d.landed > 0 {
                                println!("{} wrote {} queued change(s)", now(), d.landed);
                            }
                            if let Some(e) = d.stuck {
                                eprintln!("{} queued changes wait: {e:#}", now());
                            }
                            for p in d.set_aside {
                                eprintln!("{} a queued change could not be read: set aside in {}", now(), p.display());
                            }
                        }
                        Err(e) => eprintln!("{} queued changes wait: {e:#}", now()),
                    }
                    // Paused (da-wlbyx), it idles: an incoming session's
                    // handshake refuses, and nothing is pushed.
                    let peers = match db.sync_paused()? {
                        Some(_) => Vec::new(),
                        None => synced_peers(db)?,
                    };
                    let vector = db.sync_vector()?;
                    for p in peers {
                        let waiting = retry.get(&p.id).is_some_and(|r| r.0 > Instant::now());
                        if waiting || pushed.get(&p.id) == Some(&vector) {
                            continue;
                        }
                        pushed.insert(p.id.clone(), vector.clone());
                        let ep = &ep;
                        sessions.push(Box::pin(async move {
                            let rep = async { dial(store, ep, addr(&p)?, None).await };
                            (Some((p.id.clone(), p.label.clone())), rep.await)
                        }));
                    }
                }
            }
        }
        ep.close().await;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// datum.day's store reads the kind the CLI prints (core's name) as the
    /// wire's: each kind is named alike in both (da-b6xh6).
    #[test]
    fn a_refusals_kind_is_named_alike_in_core_and_on_the_wire() {
        use CoreRefusal::*;
        for kind in [UnknownInvite, InviteUsed, InviteExpired, Paused, NotMember] {
            let e = refusal(DatumError::Refused(kind, String::new()));
            let wire = e.downcast_ref::<Refusal>().unwrap().0;
            assert_eq!(serde_json::json!(kind), serde_json::json!(wire));
        }
    }

    /// A push that fails waits a minute, then twice as long after each
    /// failure, never more than half an hour (da-u1ul1).
    #[test]
    fn a_failed_push_waits_twice_as_long_each_time_up_to_half_an_hour() {
        let mut last = None;
        let minutes: Vec<String> = (0..8)
            .map(|_| {
                last = Some(backoff(last));
                gap(last.unwrap())
            })
            .collect();
        let want = ["1", "2", "4", "8", "16", "30", "30", "30"].map(|m| format!("{m} min"));
        assert_eq!(minutes, want);
    }
}
