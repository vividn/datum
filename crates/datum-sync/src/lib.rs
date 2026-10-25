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

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs::{File, TryLockError};
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
    dial, failure, gave_way, redeem, serve, unix_now, About, Applied, Diffed, Digest, Endpoint,
    EndpointAddr, Failure, Lost, Net, Refusal, Report, Store, Ticket, Vector, Version, Why,
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

/// The host this copy runs on, which its label is assembled from (da-xijlw).
pub fn host() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most `buf.len()` bytes into `buf`.
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return "unknown".into();
    }
    let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

/// The user it runs as: `USER`, else `LOGNAME`.
pub fn user() -> Option<String> {
    let user = std::env::var("USER").or_else(|_| std::env::var("LOGNAME"));
    user.ok().filter(|u| !u.is_empty())
}

/// How this copy names itself before it describes itself (da-ey3hz):
/// `<user>@<host>`, so two accounts on one host differ.
pub fn label() -> String {
    chain::assembled(user().as_deref(), &host(), None)
}

/// Write this copy's `describe` (da-xijlw): `label`, or with none one
/// assembled from its user, host and database's name.
pub fn describe_self(db: &Db, label: Option<&str>, kind: &str) -> Result<()> {
    Ok(db.sync_describe_self(label, kind, user().as_deref(), &host())?)
}

/// A copy applying another session's ops refused this one.
const BUSY: Failure = Failure::Refused(Why::Busy);

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
    /// The sessions served now (da-ymgiv), and those it dialed past their
    /// handshake (da-mghh3).
    RefCell<(Open, Open)>,
);

impl<'a> DbStore<'a> {
    fn new(db: &'a Db) -> Self {
        DbStore(db, None, Cell::new(None), None, Default::default())
    }

    fn tell(&self, f: impl FnOnce(&mut Told)) {
        if let Some(told) = self.1 {
            f(&mut told.lock().unwrap());
        }
    }
}

/// Per peer, how many sessions with it are open (da-ymgiv): a peer is held
/// until the last of them ends, however many began.
#[derive(Default)]
struct Open(BTreeMap<String, usize>);

impl Open {
    fn open(&mut self, peer: &str) {
        *self.0.entry(peer.to_string()).or_default() += 1;
    }

    fn close(&mut self, peer: &str) {
        if let Some(n) = self.0.get_mut(peer) {
            *n -= 1;
            if *n == 0 {
                self.0.remove(peer);
            }
        }
    }

    fn has(&self, peer: &str) -> bool {
        self.0.contains_key(peer)
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
    /// The round sending, the ops sent in those before it, and about how
    /// many it sends in all (da-ija0z): a round's alone read as the whole.
    round: (usize, usize, usize),
    got: usize,
    applied: (usize, usize),
    /// The round reading what it sends, and the baselines it signed so far,
    /// until it sends (da-la01w).
    preparing: Option<(usize, usize)>,
    /// A round received was full: the ops applied are one of several
    /// rounds', and their line names which (da-y8yde).
    more: bool,
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
            round: (0, 0, 0),
            got: 0,
            applied: (0, 0),
            preparing: None,
            more: false,
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
        (
            self.sent,
            self.round,
            self.got,
            self.applied,
            self.preparing,
            self.more,
            self.verifying,
        ) = Default::default();
    }

    /// Say what the session is doing, if it last did [`WAITING`] ago.
    fn say(&mut self) {
        if Instant::now() < self.said + WAITING {
            return;
        }
        self.said = Instant::now();
        let ((sent, of), (done, all)) = (self.sent, self.applied);
        // A session of more than one round says which, and how far in all.
        let (n, before, total) = self.round;
        let (round, in_all) = match n > 1 || total > of {
            true => (
                format!("round {n}: "),
                format!(", {} of about {total} in all", before + sent),
            ),
            false => Default::default(),
        };
        // What it receives names the round once the peer filled one.
        let taking = match n > 1 || self.more {
            true => format!("round {n}: "),
            false => String::new(),
        };
        // A copy sending nothing says what it applied till the next round's
        // ops come, and then what it has of them, from none: never a bare
        // «syncing X» (da-impts).
        let how = if self.verifying {
            ": verifying that both copies agree".into()
        } else if let Some((n, signed)) = self.preparing {
            let signed = match signed {
                0 => String::new(),
                _ => format!(", {signed} baselines signed"),
            };
            format!(": preparing round {n}: reading what to send{signed}")
        } else if done < all || (all > 0 && of == 0) {
            format!(": {taking}{done} of {all} ops applied")
        } else if sent < of {
            format!(": {round}{sent} of {of} ops sent{in_all}")
        } else if all == 0 && (self.got > 0 || n > 0 && of == 0) {
            format!(": {taking}{} ops received", self.got)
        } else if of > 0 {
            format!(": {round}{of} ops sent{in_all}, waiting for it to apply them")
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

    async fn ops_since(&self, peer: &str, since: &Vector, limit: usize) -> Result<Vec<String>> {
        let ops = self
            .0
            .sync_ops_round(since, false, Some(peer), Some(limit))?;
        // Test knob: a slow read, which the peer waits out between rounds.
        let delay = Duration::from_millis(env("DATUM_SYNC_TEST_SEND_DELAY_MS").unwrap_or(0));
        std::thread::sleep(delay * ops.len() as u32);
        Ok(ops)
    }

    async fn sign(&self, since: &Vector, n: usize) -> Result<usize> {
        Ok(self.0.sync_sign_baselines(since, Some(n))?)
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

    fn applying(&self) -> bool {
        let gate = apply_gate(self.0).and_then(|p| File::open(p).ok());
        gate.is_some_and(|f| matches!(f.try_lock_shared(), Err(TryLockError::WouldBlock)))
    }

    /// A lock on `<db>.applying`, so the sessions of every process take
    /// turns. Where the filesystem has no locks, none is held.
    async fn apply_gate(&self) -> Result<impl Sized> {
        let Some(path) = apply_gate(self.0) else {
            return Ok(None);
        };
        let gate = File::options().create(true).append(true).open(&path)?;
        loop {
            match gate.try_lock() {
                Ok(()) => return Ok(Some(gate)),
                Err(TryLockError::WouldBlock) => {
                    tokio::time::sleep(Duration::from_millis(100)).await
                }
                Err(TryLockError::Error(_)) => return Ok(None),
            }
        }
    }

    fn round(&self, n: usize, left: usize) {
        self.tell(|t| {
            let before = t.round.1 + t.sent.1;
            t.round = (n, before, t.round.2.max(before + left));
            // The round's received ops count from none (da-impts).
            (t.sent, t.got, t.applied, t.preparing) = Default::default();
        });
    }

    fn preparing(&self, n: usize, signed: usize) {
        self.tell(|t| t.preparing = Some((n, signed)));
    }

    fn more(&self) {
        self.tell(|t| t.more = true);
    }

    fn sent(&self, done: usize, of: usize) {
        self.tell(|t| t.sent = (done, of));
    }

    fn got(&self, n: usize) {
        self.tell(|t| t.got = n);
    }

    fn serving(&self, peer: &str, on: bool) {
        let served = &mut self.4.borrow_mut().0;
        match on {
            true => served.open(peer),
            false => served.close(peer),
        }
    }

    fn serves(&self, peer: &str) -> bool {
        self.4.borrow().0.has(peer)
    }

    fn carrying(&self, peer: &str, on: bool) {
        let carried = &mut self.4.borrow_mut().1;
        match on {
            true => carried.open(peer),
            false => carried.close(peer),
        }
    }

    fn carries(&self, peer: &str) -> bool {
        self.4.borrow().1.has(peer)
    }

    fn verifying(&self) {
        self.tell(|t| t.verifying = true);
    }

    /// Hashed on a connection of its own, off the session's thread, which
    /// sends meanwhile: a big history hashes for minutes (da-0mwws). It
    /// writes, to keep the summed hash's parts (da-9ln3s).
    async fn verify(&self, peer: &str, sums: bool) -> Result<String> {
        let (me, peer) = (self.0.sync_endpoint_id()?, peer.to_string());
        let hash = move |db: &Db| -> Result<String> {
            // Test knob: a slow verify, holding its thread as a big one does.
            let delay = env("DATUM_SYNC_TEST_VERIFY_DELAY_S").unwrap_or(0);
            std::thread::sleep(Duration::from_secs(delay));
            Ok(match sums {
                true => db.sync_state_hash_as(Some(&me), Some(&peer))?,
                false => db.sync_state_hash_ordered_as(Some(&me), Some(&peer))?.0,
            })
        };
        match self.0.path().and_then(|p| p.to_str()).map(String::from) {
            Some(path) => {
                tokio::task::spawn_blocking(move || hash(&Db::open_unmigrated(&path)?)).await?
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
        error: Option<&anyhow::Error>,
        offline: bool,
        met: &[EndpointAddr],
    ) -> Result<()> {
        // A session that gave way to a write on this file failed nobody:
        // `run` tries again at its next tick (da-h31wt).
        if error.is_some_and(|e| datum_core::db::busy(e.as_ref())) {
            return Ok(());
        }
        let error = error.map(|e| format!("{e:#}"));
        self.0
            .sync_record_session(peer, vector, verify, error.as_deref(), offline)?;
        if met.is_empty() {
            return Ok(());
        }
        Ok(self.0.sync_introduce(&serde_json::to_string(met)?)?)
    }

    async fn known(&self) -> Result<Vec<EndpointAddr>> {
        known(self.0)
    }
}

/// The file whose lock is `db`'s one applying session (da-h31wt); none for
/// a database with no file.
fn apply_gate(db: &Db) -> Option<std::path::PathBuf> {
    let mut path = db.path()?.as_os_str().to_owned();
    path.push(".applying");
    Some(path.into())
}

/// A handshake's error, with its kind where it is a [`Refusal`] the wire
/// sends (da-b6xh6).
fn refusal(e: DatumError) -> anyhow::Error {
    // Another process's write held the file past `run`'s YIELD: try again,
    // no failure (da-h31wt). The lock stays its cause, which `run` reads.
    if datum_core::db::busy(&e) {
        return anyhow::Error::from(e).context(Refusal(Why::Busy, "a write holds its file".into()));
    }
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
/// it is redeemed (or expires). `expires_at` is when, on the reader's clock.
/// `datum sync chain invite` has written the invite into the chain, so any
/// member can admit its redeemer: the ticket lists them all.
pub fn offer(db: &Db, secret: &str, expires: Duration, expires_at: &str, qr: bool) -> Result<()> {
    let secret = secret.to_string();
    runtime()?.block_on(async {
        let ep = bind(db).await?;
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
        if qr {
            println!("or open the link in a browser, or scan this code with a phone's camera:");
            let code = qrcode::QrCode::new(link.as_bytes())?;
            println!(
                "{}",
                code.render::<qrcode::render::unicode::Dense1x2>().build()
            );
        } else {
            println!("or open the link in a browser.\n");
        }
        let deadline = tokio::time::sleep(expires);
        tokio::pin!(deadline);
        // Every session at once, each saying how far it is: the daemon steps
        // aside for the invite, so this endpoint is the copy's, and served
        // only the redeemer, every other member dialing it through a long
        // join reached no one (da-dwm2y). It expires with none open.
        let mut sessions = FuturesUnordered::new();
        let result = loop {
            tokio::select! {
                _ = &mut deadline, if sessions.is_empty() => break Err(anyhow!("the invite expired unredeemed")),
                incoming = ep.accept() => {
                    let Some(incoming) = incoming else { break Ok(()) };
                    let ep = &ep;
                    sessions.push(async move {
                        let told = Told::new(String::new());
                        let store = DbStore(db, Some(&told), Cell::new(None), None, Default::default());
                        telling(&told, serve(&store, ep, incoming)).await
                    });
                }
                Some(served) = sessions.next() => match served {
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
        };
        // The others still open end as they would have.
        while let Some(served) = sessions.next().await {
            match served {
                Ok(rep) => print_added(&rep),
                Err(e) => eprintln!("{e:#}"),
            }
        }
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
        let store = DbStore(
            db,
            Some(&told),
            Cell::new(Some(admitted)),
            name,
            Default::default(),
        );
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
            describe_self(db, name, "cli")?;
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
        Some(Failure::Refused(Why::Busy)) => format!("connect again once {from} is done"),
        Some(Failure::Refused(Why::Other | Why::NotMember)) => {
            "`datum sync status` shows where this copy stands".into()
        }
        Some(Failure::Unreachable | Failure::TimedOut) => format!(
            "{from} did not answer: open datum.day on that database (or run \
             `datum sync chain invite` there) and connect again, or ask it for a fresh invite"
        ),
        // Its own words say it already.
        Some(Failure::Late | Failure::Protocol) | None => return e,
    };
    anyhow!("{e:#}. Next: {next}")
}

/// Where the `sync run` serving a database takes asks to sync now
/// (da-tltjj): a socket beside it. A database with no file has none.
fn asks(db: &Db) -> Option<std::path::PathBuf> {
    let path = db.conn().path().filter(|p| !p.is_empty())?;
    let mut path = std::fs::canonicalize(path).ok()?.into_os_string();
    path.push(".sync");
    Some(path.into())
}

/// What `datum sync` says of one session with `p`, which took `took`
/// (da-a4fxr): whether it failed, its line, and the line `-v` adds. One
/// it never reached is offline, with when it was last seen, its words in
/// the `-v` line, and no failure (da-9d6uy, vividn: «offline isn't a
/// failure»); nor is a busy one, which syncs with this copy once done
/// (da-h31wt). Neither synced, which [`ended`] counts.
fn said(p: &Peer, rep: &Result<Report>, took: Duration) -> (bool, String, String) {
    let offline = rep
        .as_ref()
        .is_err_and(|e| failure(e) == Some(Failure::Unreachable));
    let line = match rep {
        Ok(rep) => rep.to_string(),
        Err(_) if offline => match &p.last_seen {
            Some(at) => format!(
                "{}: offline, last seen {}",
                p.label,
                datum_core::time::on_readers_clock(at, "%H:%M")
            ),
            None => format!("{}: offline, never seen", p.label),
        },
        Err(e) => format!("{}: {e:#}", p.label),
    };
    let failed = !offline && rep.as_ref().is_err_and(|e| failure(e) != Some(BUSY));
    // A dial that never connected (offline, or refused at the handshake)
    // has no route to say.
    let route = match rep {
        Ok(rep) => rep.route.clone(),
        Err(e) => e.downcast_ref::<Lost>().and_then(|l| l.0.route.clone()),
    };
    let why = rep.as_ref().err().filter(|_| offline);
    let more: Vec<String> = route
        .into_iter()
        .chain([format!("{took:.1?}")])
        .chain(why.map(|e| format!("{e:#}")))
        .collect();
    (failed, line, format!("  {}", more.join(" · ")))
}

/// One session's line, as `sync run` answers an ask (after a line of how
/// many sessions it opened): whether it synced, and what `datum sync`
/// prints of it, `-v`'s line too.
fn answer(p: &Peer, rep: &Result<Report>, took: Duration) -> String {
    let (failed, line, verbose) = said(p, rep, took);
    let synced = rep.is_ok();
    serde_json::json!({ "ok": !failed, "synced": synced, "line": line, "verbose": verbose })
        .to_string()
        + "\n"
}

/// How `datum sync` ends, of `of` sessions: an error when one failed, or
/// when none synced — every member offline or busy (da-9d6uy, vividn: «a bare
/// `datum sync` should fail if no sync is successful»).
fn ended(failed: usize, synced: usize, of: usize) -> Result<()> {
    ensure!(failed == 0, "{failed} of {of} peer(s) failed");
    ensure!(synced > 0, "none of {of} peer(s) synced");
    Ok(())
}

/// After the dials: one of them may have passed its address on.
fn print_no_address(db: &Db) -> Result<()> {
    let status = db.sync_status()?;
    for p in &status {
        if let Some(why) = p.no_address(&status) {
            println!("{}: {why}", p.label);
        }
    }
    Ok(())
}

/// `datum sync`: sync with every member once — whom first, then each as
/// its session ends ([`said`]); `verbose` adds the route, the time and an
/// offline one's words. While a `sync run` serves this copy it asks that
/// to, and prints what its sessions did: a second set of sessions would
/// double the traffic, and bind the endpoint key twice (da-tltjj).
pub fn now_all(db: &Db, verbose: bool) -> Result<()> {
    let peers = synced_peers(db)?;
    if peers.is_empty() {
        println!("no sync peers: `datum sync chain invite` makes an invite");
        return Ok(());
    }
    let names: Vec<&str> = peers.iter().map(|p| p.label.as_str()).collect();
    println!("syncing with {}", names.join(", "));
    if let Some(daemon) = asks(db).and_then(|p| std::os::unix::net::UnixStream::connect(p).ok()) {
        use std::io::BufRead;
        eprintln!("this copy's sync daemon is syncing it");
        let mut lines = std::io::BufReader::new(daemon).lines();
        let stopped = "this copy's sync daemon stopped before it finished: `datum sync` again";
        let garbled =
            "this copy's sync daemon answered what this datum cannot read: is it another build?";
        let mut next = || lines.next().context(stopped)?.context(stopped);
        let of: usize = next()?.parse().context(garbled)?;
        let (mut failed, mut synced) = (0, 0);
        for _ in 0..of {
            let answer: serde_json::Value = serde_json::from_str(&next()?).context(garbled)?;
            let (ok, line) = (answer["ok"].as_bool(), answer["line"].as_str());
            let (ok, line) = ok.zip(line).context(garbled)?;
            failed += usize::from(!ok);
            // A daemon of a build before da-9d6uy says only `ok`.
            synced += usize::from(answer["synced"].as_bool().unwrap_or(ok));
            println!("{line}");
            if let Some(more) = answer["verbose"].as_str().filter(|_| verbose) {
                println!("{more}");
            }
        }
        print_no_address(db)?;
        return ended(failed, synced, of);
    }
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let store = &DbStore::new(db);
        let ep = &ep;
        // Each said as its session ends, not after the slowest (da-a4fxr).
        let mut dials: FuturesUnordered<_> = peers
            .iter()
            .map(|p| async move {
                // The whole dial, connecting included.
                let start = Instant::now();
                let rep = async { dial(store, ep, addr(p)?, None).await }.await;
                (p, rep, start.elapsed())
            })
            .collect();
        let (mut failed, mut synced) = (0, 0);
        while let Some((p, rep, took)) = dials.next().await {
            let (lost, line, more) = said(p, &rep, took);
            failed += usize::from(lost);
            synced += usize::from(rep.is_ok());
            println!("{line}");
            if verbose {
                println!("{more}");
            }
        }
        print_no_address(db)?;
        ep.close().await;
        ended(failed, synced, peers.len())
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

/// A push to `p`: its session, tagged with the peer. `asked`, a `datum sync`
/// waiting on it, is told its line as it ends.
fn push<'a>(
    store: &'a DbStore<'a>,
    ep: &'a Endpoint,
    p: Peer,
    asked: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) -> Session<'a> {
    Box::pin(async move {
        let start = Instant::now();
        let rep = loop {
            let rep = async { dial(store, ep, addr(&p)?, None).await }.await;
            // Asked, a dial given way to a session between the two copies
            // (da-vj3ly, da-mghh3) waits that session out and dials again,
            // so `datum sync` says this one verified, not that one runs
            // (da-221fe). Refused, that session may be over by now.
            let busy = rep.as_ref().is_err_and(|e| failure(e) == Some(BUSY));
            let gave = rep.as_ref().is_err_and(gave_way);
            if asked.is_none() || !busy || !(gave || store.serves(&p.id)) {
                break rep;
            }
            while store.serves(&p.id) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        if let Some(asked) = asked {
            let _ = asked.send(answer(&p, &rep, start.elapsed()));
        }
        (Some((p.id, p.label)), rep)
    })
}

/// `sync run`'s socket for [`asks`], gone with it. One left by a run that
/// was killed refuses a connection, and is replaced.
struct Asks(tokio::net::UnixListener, std::path::PathBuf);

impl Asks {
    /// Where it cannot listen, `datum sync` dials as it would without it.
    fn listen(db: &Db) -> Option<Self> {
        let path = asks(db)?;
        let _ = std::fs::remove_file(&path);
        match tokio::net::UnixListener::bind(&path) {
            Ok(listener) => Some(Asks(listener, path)),
            Err(e) => {
                eprintln!(
                    "{} not taking asks to sync at {}: {e}",
                    now(),
                    path.display()
                );
                None
            }
        }
    }
}

impl Drop for Asks {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.1);
    }
}

/// The next ask, or never without a socket.
async fn ask(
    asks: &Option<Asks>,
) -> std::io::Result<(tokio::net::UnixStream, tokio::net::unix::SocketAddr)> {
    match asks {
        Some(asks) => asks.0.accept().await,
        None => std::future::pending().await,
    }
}

/// How long `sync run` waits for a write lock another connection holds,
/// against every other connection's 5 s (da-lk1s2). Nobody waits on a
/// background apply, so it is the one that gives way: a person's write
/// waits out the sync, and the sync tries again at its next tick.
const YIELD: Duration = Duration::from_millis(250);

/// `datum sync run`: stay up — serve peers, and push to every peer each
/// time this copy's vector moves (a write here, or ops from another peer).
/// A push waits for an earlier one to the same peer to end: its rounds go
/// on while the vectors move, so it carries what is written meanwhile, and
/// a second beside it sends the peer the same ops again (da-ymgiv). A
/// failed push is tried again after [`backoff`], whether or not anything
/// new was written, and the gap grows with each failure until the peer
/// reaches this copy or a push reaches it.
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
        let asks = Asks::listen(db);
        eprintln!("datum sync run: {} is up; Ctrl-C to stop", short(&ep.id().to_string()));
        let mut sessions: FuturesUnordered<Session> = FuturesUnordered::new();
        // Per peer, the vector last pushed to it, and when to retry a failed
        // push with the gap it waits.
        let mut pushed: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
        let mut retry: BTreeMap<String, (Instant, Duration)> = BTreeMap::new();
        let mut busy: BTreeMap<String, Instant> = BTreeMap::new();
        // The pushes open: a tick's and an ask's can be open to one peer.
        let mut open = Open::default();
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
                // `datum sync` asks to sync now: every member, backing off
                // or not, each session's line answered as it ends.
                Ok((asked, _)) = ask(&asks) => {
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                    tokio::spawn(async move {
                        use tokio::io::AsyncWriteExt;
                        let mut asked = asked;
                        while let Some(line) = rx.recv().await {
                            if asked.write_all(line.as_bytes()).await.is_err() {
                                return;
                            }
                        }
                    });
                    let peers = match db.sync_paused()? {
                        Some(_) => Vec::new(),
                        None => synced_peers(db)?,
                    };
                    let _ = tx.send(format!("{}\n", peers.len()));
                    let vector = db.sync_vector()?;
                    for p in peers {
                        pushed.insert(p.id.clone(), vector.clone());
                        open.open(&p.id);
                        sessions.push(push(store, &ep, p, Some(tx.clone())));
                    }
                }
                Some(incoming) = ep.accept() => {
                    let ep = &ep;
                    sessions.push(Box::pin(async move {
                        (None, serve(store, ep, incoming).await)
                    }));
                }
                Some((peer, rep)) = sessions.next() => {
                    if let Some((id, _)) = &peer {
                        open.close(id);
                    }
                    match (rep, peer) {
                        (Ok(rep), _) => {
                            // A peer that reached us, or we it, is up again.
                            retry.remove(&rep.id);
                            println!("{} {rep}", now());
                        }
                        // A write held this copy's file past YIELD: every peer
                        // not backing off for being unreachable is tried again
                        // at the next tick. sync_net's
                        // `run_gives_way_to_a_write_holding_its_file_and_tries_again_at_once`.
                        // A peer's file held is its refusal as busy, the next
                        // arm: the peer gives way in its own `run` and pushes
                        // (`a_peer_held_by_a_write_comes_back_for_the_change_itself`).
                        (Err(e), _) if datum_core::db::busy(e.as_ref()) => {
                            eprintln!("{} gave way to a write: {e:#}", now());
                            pushed.clear();
                        }
                        // A copy applying refused it, here or there: no failure.
                        // The one applying pushes once done; a push is tried
                        // again after the first gap, which does not grow
                        // (da-h31wt).
                        (Err(e), peer) if failure(&e) == Some(BUSY) => {
                            if let Some((id, _)) = peer {
                                pushed.remove(&id);
                                // No wait while a session served to it
                                // carries this copy's ops: it pushes once
                                // that ends (da-vj3ly).
                                if !store.serves(&id) {
                                    busy.insert(id, Instant::now() + backoff(None));
                                }
                            }
                        }
                        // A session lost past the handshake names its peer.
                        (Err(e), None) => match e.downcast_ref::<Lost>() {
                            Some(Lost(rep, _)) => eprintln!("{} {}: {e:#}", now(), rep.label),
                            None => eprintln!("{} incoming: {e:#}", now()),
                        },
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
                            if d.wiped > 0 {
                                println!("{} dropped {} queued change(s) made before the sync chain's purge", now(), d.wiped);
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
                    // handshake refuses, and nothing is pushed. Applying, it
                    // waits: each push would bring the same ops (da-h31wt).
                    let peers = match db.sync_paused()? {
                        Some(_) => Vec::new(),
                        None if store.applying() => Vec::new(),
                        None => synced_peers(db)?,
                    };
                    let vector = db.sync_vector()?;
                    for p in peers {
                        let waiting = retry.get(&p.id).is_some_and(|r| r.0 > Instant::now())
                            || busy.get(&p.id).is_some_and(|at| *at > Instant::now());
                        // Nor while serving it: that session carries it too.
                        let held = open.has(&p.id) || store.serves(&p.id);
                        if waiting || held || pushed.get(&p.id) == Some(&vector) {
                            continue;
                        }
                        pushed.insert(p.id.clone(), vector.clone());
                        open.open(&p.id);
                        sessions.push(push(store, &ep, p, None));
                    }
                }
            }
        }
        ep.close().await;
        Ok(())
    })
}

/// What [`run_aside`]'s stop says: step aside.
#[derive(Debug)]
struct Aside;

impl std::fmt::Display for Aside {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("stepping aside")
    }
}

impl std::error::Error for Aside {}

/// [`run`] in the foreground, stepping aside while `aside`: a `connect` or
/// an `invite` at work on the database, which the daemon's run stops for
/// (da-x5ynh). It unbinds, waits for it to end and binds again, so no two
/// processes hold one endpoint key. Ctrl-C ends it waiting too.
pub fn run_aside(db: &Db, aside: impl Fn() -> bool) -> Result<()> {
    loop {
        if aside() {
            eprintln!(
                "{} a `datum sync chain connect` or `invite` is at work on this database: stepping aside until it ends",
                now()
            );
            let ended = runtime()?.block_on(async {
                let ctrl_c = tokio::signal::ctrl_c();
                tokio::pin!(ctrl_c);
                let mut tick = tokio::time::interval(Duration::from_millis(500));
                loop {
                    tokio::select! {
                        _ = &mut ctrl_c => return true,
                        _ = tick.tick() => if !aside() { return false },
                    }
                }
            });
            if ended {
                return Ok(());
            }
        }
        match run(db, || if aside() { Err(Aside.into()) } else { Ok(()) }) {
            Err(e) if e.is::<Aside>() => {}
            done => return done,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tick's push and an ask's open to one peer at once (da-ymgiv): the
    /// peer stays held until both have ended, not only the first.
    #[test]
    fn a_peer_with_two_sessions_open_is_held_until_both_end() {
        let mut open = Open::default();
        open.open("p");
        open.open("p");
        open.close("p");
        assert!(open.has("p"), "one session still runs");
        open.close("p");
        assert!(!open.has("p"));
        open.close("p");
        assert!(!open.has("p"), "a close with none open holds nothing");
    }

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
