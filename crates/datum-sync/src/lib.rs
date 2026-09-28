//! Easy Sync over iroh (da-rugg8): two copies of a database sync over a QUIC
//! connection. The merge is datum-core's (`sync_vector`, `sync_ops_since`,
//! `sync_apply`, `sync_state_hash`); this crate moves the ops, and pairs
//! copies with one-time invites.
//!
//! - **Endpoint.** The iroh endpoint's key IS the db's endpoint key, so the
//!   iroh endpoint id is the `endpoint` every op of this copy carries. ALPN
//!   `datum/sync/1`, n0's relays and discovery. `DATUM_SYNC_BIND=<ip:port>`
//!   binds that one address with no relay and no discovery (a LAN, the tests).
//! - **Peers.** Until signed membership, trust is pairwise: a peer is whoever
//!   redeemed an invite from this copy, or whose invite it redeemed. Anyone
//!   else is refused at the handshake, and so is this copy's own key.
//! - **Wire.** One bidirectional stream of frames, each a big-endian u32
//!   length and its bytes. The dialer sends a [`Hello`], the acceptor a
//!   [`Reply`]. Then rounds: each side sends its version vector, then the ops
//!   the other lacks, one frame each, and an empty frame, and applies what
//!   came — until a round in which neither vector moved. Last the verify
//!   hashes, the dialer's first, and the dialer closes.

use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use chrono::{SecondsFormat, Utc};
use data_encoding::BASE32_NOPAD;
use datum_core::db::Db;
use futures_util::stream::{FuturesUnordered, StreamExt};
use iroh::endpoint::{presets, Incoming, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, RelayMode, SecretKey};
use rusqlite::{params, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ALPN: &[u8] = b"datum/sync/1";
const MAX_FRAME: usize = 64 << 20;
const MAX_HELLO: usize = 64 << 10;
/// A session in which both sides keep writing still ends.
const MAX_ROUNDS: usize = 16;
const SESSION_TIMEOUT: Duration = Duration::from_secs(300);
const COPY: &str = "this peer is a copy of this database (same endpoint key); run `datum sync rekey` on one of them";

// ─── Small things ───────────────────────────────────────────────────────────

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

fn short(id: &str) -> &str {
    &id[..id.len().min(10)]
}

/// How this copy names itself to a peer: the host and the client type.
fn label() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most `buf.len()` bytes into `buf`.
    let host = if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0 {
        let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..n]).into_owned()
    } else {
        "unknown".into()
    };
    format!("{host} (cli)")
}

async fn within<T>(limit: Duration, f: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(limit, f)
        .await
        .map_err(|_| anyhow!("timed out after {}s", limit.as_secs()))?
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

// ─── Peers and invites (local tables, never synced) ─────────────────────────

pub struct Peer {
    pub id: String,
    pub label: String,
    addrs: String,
    pub added_at: String,
    pub last_seen: Option<String>,
}

impl Peer {
    fn addr(&self) -> Result<EndpointAddr> {
        Ok(serde_json::from_str(&self.addrs)?)
    }
}

pub fn peers(db: &Db) -> Result<Vec<Peer>> {
    let mut stmt = db.conn().prepare(
        "SELECT endpoint_id, label, addrs, added_at, last_seen FROM sync_peers ORDER BY added_at",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Peer {
            id: r.get(0)?,
            label: r.get(1)?,
            addrs: r.get(2)?,
            added_at: r.get(3)?,
            last_seen: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn is_peer(db: &Db, id: &str) -> Result<bool> {
    Ok(db
        .conn()
        .query_row(
            "SELECT 1 FROM sync_peers WHERE endpoint_id = ?1",
            [id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Record `id` as a peer, or refresh its label, address and last seen.
fn record_peer(db: &Db, id: &str, label: &str, addr: &EndpointAddr) -> Result<()> {
    db.conn().execute(
        "INSERT INTO sync_peers (endpoint_id, label, addrs, added_at, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT (endpoint_id) DO UPDATE SET
           label = excluded.label, addrs = excluded.addrs, last_seen = excluded.last_seen",
        params![id, label, serde_json::to_string(addr)?, now()],
    )?;
    Ok(())
}

/// Mark the invite whose secret is `secret` used, if it is unused and
/// unexpired; otherwise say why not.
fn redeem(db: &Db, secret: &str) -> Result<(), String> {
    let hash = sha256_hex(secret);
    let row: Option<(String, Option<String>)> = db
        .conn()
        .query_row(
            "SELECT expires_at, used_at FROM sync_invites WHERE secret_hash = ?1",
            [&hash],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match row {
        None => Err("unknown invite".into()),
        Some((_, Some(used))) => Err(format!("invite already used ({used})")),
        Some((expires, None)) if expires <= now() => Err(format!("invite expired ({expires})")),
        Some(_) => db
            .conn()
            .execute(
                "UPDATE sync_invites SET used_at = ?2 WHERE secret_hash = ?1",
                params![hash, now()],
            )
            .map(drop)
            .map_err(|e| e.to_string()),
    }
}

// ─── Wire ───────────────────────────────────────────────────────────────────

/// A one-time invite: where to dial, and the secret to present.
#[derive(Serialize, Deserialize)]
struct Ticket {
    addr: EndpointAddr,
    secret: String,
}

impl Ticket {
    fn encode(&self) -> Result<String> {
        Ok(BASE32_NOPAD.encode(&serde_json::to_vec(self)?))
    }

    fn decode(s: &str) -> Result<Self> {
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
}

#[derive(Serialize, Deserialize)]
enum Reply {
    Welcome { label: String, addr: EndpointAddr },
    Refused(String),
}

async fn put(s: &mut SendStream, b: &[u8]) -> Result<()> {
    s.write_all(&(b.len() as u32).to_be_bytes()).await?;
    s.write_all(b).await?;
    Ok(())
}

async fn get(r: &mut RecvStream, max: usize) -> Result<Vec<u8>> {
    let mut n = [0u8; 4];
    r.read_exact(&mut n).await?;
    let n = u32::from_be_bytes(n) as usize;
    ensure!(n <= max, "frame of {n} bytes is over the limit");
    let mut b = vec![0u8; n];
    r.read_exact(&mut b).await?;
    Ok(b)
}

async fn put_json(s: &mut SendStream, v: &impl Serialize) -> Result<()> {
    put(s, &serde_json::to_vec(v)?).await
}

async fn get_json<T: DeserializeOwned>(r: &mut RecvStream, max: usize) -> Result<T> {
    Ok(serde_json::from_slice(&get(r, max).await?)?)
}

// ─── The session ────────────────────────────────────────────────────────────

/// One sync with one peer.
pub struct Report {
    pub label: String,
    pub id: String,
    /// The peer was added by this session.
    pub added: bool,
    pub sent: usize,
    pub got: usize,
    pub conflicts: usize,
    pub verified: bool,
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

/// Swap ops until neither side's vector moves, then compare state hashes.
async fn exchange(
    db: &Db,
    s: &mut SendStream,
    r: &mut RecvStream,
    dialer: bool,
    rep: &mut Report,
) -> Result<()> {
    let mut last = None;
    for _ in 0..MAX_ROUNDS {
        let mine = db.sync_vector()?;
        put_json(s, &mine).await?;
        let theirs: BTreeMap<String, i64> = get_json(r, MAX_FRAME).await?;
        let round = (mine, theirs);
        if last.as_ref() == Some(&round) {
            break;
        }
        let ops = db.sync_ops_since(&round.1)?;
        let send = async {
            for op in &ops {
                put(s, op.as_bytes()).await?;
            }
            put(s, b"").await
        };
        let recv = async {
            let mut lines = String::new();
            loop {
                let f = get(r, MAX_FRAME).await?;
                if f.is_empty() {
                    return Ok::<_, anyhow::Error>(lines);
                }
                lines.push_str(std::str::from_utf8(&f)?);
                lines.push('\n');
            }
        };
        let ((), lines) = tokio::try_join!(send, recv)?;
        let applied = db.sync_apply(&lines)?;
        for why in &applied.refused {
            eprintln!("refused from {}: {why}", rep.label);
        }
        rep.sent += ops.len();
        rep.got += applied.applied;
        rep.conflicts += applied.conflicts;
        last = Some(round);
    }
    let hash = db.sync_state_hash()?.0;
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

/// Dial a peer, presenting `invite` if this is its redemption, and sync.
async fn dial(db: &Db, ep: &Endpoint, addr: EndpointAddr, invite: Option<&str>) -> Result<Report> {
    ensure!(addr.id != ep.id(), COPY);
    let id = addr.id.to_string();
    let conn = ep
        .connect(addr, ALPN)
        .await
        .map_err(|e| anyhow!("could not reach {}: {e}", short(&id)))?;
    let (mut s, mut r) = conn.open_bi().await?;
    let hello = Hello {
        label: label(),
        addr: ep.addr(),
        invite: invite.map(str::to_string),
    };
    put_json(&mut s, &hello).await?;
    let (label, addr) = match get_json(&mut r, MAX_HELLO).await? {
        Reply::Welcome { label, addr } => (label, addr),
        Reply::Refused(why) => {
            conn.close(0u32.into(), b"refused");
            bail!("refused: {why}");
        }
    };
    let added = !is_peer(db, &id)?;
    record_peer(db, &id, &label, &addr)?;
    let mut rep = Report {
        label,
        id,
        added,
        sent: 0,
        got: 0,
        conflicts: 0,
        verified: false,
    };
    let done = exchange(db, &mut s, &mut r, true, &mut rep).await;
    conn.close(0u32.into(), b"done");
    done.map(|()| rep)
}

/// Answer one incoming connection: a peer, or the redemption of an invite.
async fn serve(db: &Db, ep: &Endpoint, incoming: Incoming) -> Result<Report> {
    let conn = incoming.await?;
    let id = conn.remote_id().to_string();
    let (mut s, mut r) = conn.accept_bi().await?;
    let hello: Hello = get_json(&mut r, MAX_HELLO).await?;
    // An invite presented is redeemed or refused, even by a peer: one use.
    let known = is_peer(db, &id)?;
    let verdict = match &hello.invite {
        _ if conn.remote_id() == ep.id() => Err(COPY.to_string()),
        Some(secret) => redeem(db, secret),
        None if known => Ok(()),
        None => Err("not a peer of this copy: redeem an invite from it first".into()),
    };
    if let Err(why) = verdict {
        put_json(&mut s, &Reply::Refused(why.clone())).await?;
        let _ = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;
        bail!("refused {} ({}): {why}", hello.label, short(&id));
    }
    record_peer(db, &id, &hello.label, &hello.addr)?;
    put_json(
        &mut s,
        &Reply::Welcome {
            label: label(),
            addr: ep.addr(),
        },
    )
    .await?;
    let mut rep = Report {
        label: hello.label,
        id,
        added: !known,
        sent: 0,
        got: 0,
        conflicts: 0,
        verified: false,
    };
    exchange(db, &mut s, &mut r, false, &mut rep).await?;
    let _ = tokio::time::timeout(Duration::from_secs(5), conn.closed()).await;
    Ok(rep)
}

// ─── The endpoint ───────────────────────────────────────────────────────────

async fn bind(db: &Db) -> Result<Endpoint> {
    let key = SecretKey::from_bytes(&db.sync_endpoint_secret()?);
    let local = std::env::var("DATUM_SYNC_BIND").ok();
    let builder = match &local {
        Some(addr) => Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .clear_ip_transports()
            .bind_addr(addr.parse::<SocketAddr>().context("DATUM_SYNC_BIND")?)?,
        None => Endpoint::builder(presets::N0),
    };
    let ep = builder
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    if local.is_none() {
        // The address a ticket or a peer is handed should name the relay.
        let _ = tokio::time::timeout(Duration::from_secs(10), ep.online()).await;
    }
    Ok(ep)
}

fn print_added(rep: &Report) {
    if rep.added {
        println!("added sync peer {} ({})", rep.label, short(&rep.id));
    }
    println!("{rep}");
}

// ─── Commands ───────────────────────────────────────────────────────────────

/// `datum sync add-client`: mint a one-time invite, print how to redeem it,
/// and serve until it is redeemed (or expires).
pub fn add_client(db: &Db, expires: Duration, qr: bool) -> Result<()> {
    let secret = hex(&rand::random::<[u8; 32]>());
    let expires_at = (Utc::now() + expires).to_rfc3339_opts(SecondsFormat::Secs, true);
    db.conn().execute(
        "INSERT INTO sync_invites (secret_hash, expires_at) VALUES (?1, ?2)",
        params![sha256_hex(&secret), expires_at],
    )?;
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let ticket = Ticket {
            addr: ep.addr(),
            secret,
        }
        .encode()?;
        println!("One-time invite, until {expires_at}. On the other copy run:\n");
        println!("  datum sync connect {ticket}\n");
        if qr {
            let code = qrcode::QrCode::new(ticket.as_bytes())?;
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
                    match within(SESSION_TIMEOUT, serve(db, &ep, incoming)).await {
                        Ok(rep) if rep.added => { print_added(&rep); break Ok(()) }
                        Ok(rep) => println!("{rep}"),
                        Err(e) => eprintln!("{e:#}"),
                    }
                }
            }
        };
        ep.close().await;
        result
    })
}

/// `datum sync connect <ticket>`: redeem an invite and sync.
pub fn connect(db: &Db, ticket: &str) -> Result<()> {
    let t = Ticket::decode(ticket)?;
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let rep = within(SESSION_TIMEOUT, dial(db, &ep, t.addr, Some(&t.secret))).await;
        ep.close().await;
        print_added(&rep?);
        Ok(())
    })
}

/// `datum sync now`: sync with every peer once.
pub fn now_all(db: &Db) -> Result<()> {
    let peers = peers(db)?;
    if peers.is_empty() {
        println!("no sync peers: `datum sync add-client` makes an invite");
        return Ok(());
    }
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let dials = peers
            .iter()
            .map(|p| async { within(SESSION_TIMEOUT, dial(db, &ep, p.addr()?, None)).await });
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
        ep.close().await;
        ensure!(failed == 0, "{failed} of {} peer(s) failed", peers.len());
        Ok(())
    })
}

type Session<'a> = Pin<Box<dyn Future<Output = (Option<String>, Result<Report>)> + 'a>>;

/// `datum sync run`: stay up — serve peers, and push to every peer each
/// time this copy's vector moves (a write here, or ops from another peer).
/// A push does not wait for an earlier one to the same peer: a dial to a
/// peer that is down takes 30s to fail.
pub fn run(db: &Db) -> Result<()> {
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        eprintln!("datum sync run: {} is up; Ctrl-C to stop", short(&ep.id().to_string()));
        let mut sessions: FuturesUnordered<Session> = FuturesUnordered::new();
        // Per peer, the vector last pushed to it.
        let mut pushed: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        loop {
            tokio::select! {
                _ = &mut ctrl_c => break,
                Some(incoming) = ep.accept() => {
                    let ep = &ep;
                    sessions.push(Box::pin(async move {
                        (None, within(SESSION_TIMEOUT, serve(db, ep, incoming)).await)
                    }));
                }
                Some((peer, rep)) = sessions.next() => {
                    match rep {
                        Ok(rep) => println!("{} {rep}", now()),
                        Err(e) => eprintln!("{} {}: {e:#}", now(), peer.as_deref().unwrap_or("incoming")),
                    }
                }
                _ = tick.tick() => {
                    let vector = db.sync_vector()?;
                    for p in peers(db)? {
                        if pushed.get(&p.id) == Some(&vector) {
                            continue;
                        }
                        pushed.insert(p.id.clone(), vector.clone());
                        let ep = &ep;
                        sessions.push(Box::pin(async move {
                            let rep = async { within(SESSION_TIMEOUT, dial(db, ep, p.addr()?, None)).await };
                            (Some(p.label.clone()), rep.await)
                        }));
                    }
                }
            }
        }
        ep.close().await;
        Ok(())
    })
}

/// `datum sync peers`.
pub fn list(db: &Db) -> Result<()> {
    let peers = peers(db)?;
    if peers.is_empty() {
        println!("no sync peers");
    }
    for p in peers {
        println!(
            "{}  {}  added {}  last seen {}",
            p.label,
            short(&p.id),
            p.added_at,
            p.last_seen.as_deref().unwrap_or("never")
        );
    }
    Ok(())
}

/// `datum sync forget <peer>`: by label, or by a prefix of its id.
pub fn forget(db: &Db, who: &str) -> Result<()> {
    let hits: Vec<Peer> = peers(db)?
        .into_iter()
        .filter(|p| p.label == who || p.id.starts_with(who))
        .collect();
    match hits.as_slice() {
        [p] => {
            db.conn()
                .execute("DELETE FROM sync_peers WHERE endpoint_id = ?1", [&p.id])?;
            println!("forgot sync peer {} ({})", p.label, short(&p.id));
            Ok(())
        }
        [] => bail!("no sync peer {who}"),
        _ => bail!("{who} names {} sync peers; give more of the id", hits.len()),
    }
}
