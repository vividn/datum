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

use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, ensure, Context, Result};
use chrono::{SecondsFormat, Utc};
use datum_core::db::peers::Peer;
use datum_core::db::{chain, Db};
use datum_sync_wire::{
    dial, redeem, serve, Applied, Endpoint, EndpointAddr, Lost, Net, Report, Store, Ticket, Vector,
};
use futures_util::stream::{FuturesUnordered, StreamExt};

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn short(id: &str) -> &str {
    &id[..id.len().min(10)]
}

/// How this copy names itself to a peer: the host and the client type.
pub fn label() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most `buf.len()` bytes into `buf`.
    let host = if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0 {
        let n = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..n]).into_owned()
    } else {
        "unknown".into()
    };
    // Within the chain's label rules, whatever the host is called.
    let host: String = host
        .chars()
        .filter(|c| chain::label_problem(&c.to_string()).is_none())
        .take(chain::MAX_LABEL - " (cli)".len())
        .collect();
    format!("{host} (cli)")
}

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

/// The session runs on the calling thread, and blocks it while it applies;
/// iroh's own tasks run on a worker, so frames still go out meanwhile.
fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?)
}

// ─── The store ──────────────────────────────────────────────────────────────

/// A session's store: this copy's `Db`, on the session's own thread.
struct DbStore<'a>(&'a Db);

impl Store for DbStore<'_> {
    fn label(&self) -> String {
        label()
    }

    async fn chain(&self) -> Result<Option<(String, usize)>> {
        let c = self.0.sync_chain()?;
        let members = c.members.iter().filter(|m| m.revoked.is_none()).count();
        Ok(c.exists().then_some((c.id, members)))
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

    async fn verify(&self, peer: &str) -> Result<String> {
        Ok(self.0.sync_state_hash(Some(peer))?.0)
    }

    async fn handshake(
        &self,
        peer: &str,
        label: &str,
        addr: &EndpointAddr,
        invite: Option<&str>,
        chain: Option<&str>,
        holds: u64,
        dialed: bool,
    ) -> Result<bool> {
        let addr = serde_json::to_string(addr)?;
        Ok(self
            .0
            .sync_handshake(peer, label, &addr, invite, chain, holds, dialed)?)
    }

    async fn holds(&self) -> Result<u64> {
        Ok(self.0.count_entries_filtered(None, None)? as u64)
    }

    async fn record_session(
        &self,
        peer: &str,
        vector: Option<&Vector>,
        verify: Option<bool>,
        error: Option<&str>,
    ) -> Result<()> {
        Ok(self.0.sync_record_session(peer, vector, verify, error)?)
    }
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

/// Say so if `e` is a session that redeemed an invite and then failed: the
/// two copies are paired, and only the first sync is unfinished.
fn paired_unfinished(e: &anyhow::Error) -> bool {
    match e.downcast_ref::<Lost>() {
        Some(Lost(rep, _)) if rep.redeemed => {
            println!(
                "paired with {}; the first sync did not finish: run `datum sync`",
                rep.label
            );
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
    datum_sync_wire::bind(db.sync_endpoint_secret()?, &net).await
}

fn print_added(rep: &Report) {
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
        let store = DbStore(db);
        let mut addrs = vec![ep.addr()];
        addrs.extend(synced_peers(db)?.iter().filter_map(|p| addr(p).ok()));
        let ticket = Ticket { addrs, secret }.encode()?;
        println!("One-time invite, until {expires_at}. On the other copy run:\n");
        println!("  datum sync chain connect {ticket}\n");
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
                    match serve(&store, &ep, incoming).await {
                        Ok(rep) => {
                            print_added(&rep);
                            if rep.redeemed { break Ok(()) }
                        }
                        Err(e) => {
                            eprintln!("{e:#}");
                            if paired_unfinished(&e) { break Ok(()) }
                        }
                    }
                }
            }
        };
        ep.close().await;
        result
    })
}

/// `datum sync chain connect <ticket>`: redeem an invite and sync, with the first
/// of the ticket's addresses that admits it.
pub fn connect(db: &Db, ticket: &str) -> Result<()> {
    let t = Ticket::decode(ticket)?;
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let rep = redeem(&DbStore(db), &ep, &t).await;
        ep.close().await;
        // Admitted, it says what it is; its peers hear on the next sync. A
        // copy refused (one already in a chain) writes nothing.
        let admitted = rep.as_ref().map_or_else(paired_unfinished, |_| true);
        let me = db.sync_endpoint_id()?;
        if admitted && db.sync_chain()?.member(&me).is_some() {
            db.sync_describe_self(&label(), "cli")?;
        }
        print_added(&rep?);
        Ok(())
    })
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
        let store = DbStore(db);
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
        ep.close().await;
        ensure!(failed == 0, "{failed} of {} peer(s) failed", peers.len());
        Ok(())
    })
}

/// A session and, for a push, the peer's id and label.
type Session<'a> = Pin<Box<dyn Future<Output = (Option<(String, String)>, Result<Report>)> + 'a>>;

/// How long a peer a push failed to reach is left before the next try.
const RETRY: Duration = Duration::from_secs(30);

/// `datum sync run`: stay up — serve peers, and push to every peer each
/// time this copy's vector moves (a write here, or ops from another peer).
/// A push does not wait for an earlier one to the same peer: a dial to a
/// peer that is down takes 30s to fail. A failed push is tried again after
/// [`RETRY`], whether or not anything new was written.
pub fn run(db: &Db) -> Result<()> {
    runtime()?.block_on(async {
        let ep = bind(db).await?;
        let store = &DbStore(db);
        eprintln!("datum sync run: {} is up; Ctrl-C to stop", short(&ep.id().to_string()));
        let mut sessions: FuturesUnordered<Session> = FuturesUnordered::new();
        // Per peer, the vector last pushed to it, and when to retry a failed push.
        let mut pushed: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
        let mut retry: BTreeMap<String, tokio::time::Instant> = BTreeMap::new();
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        loop {
            tokio::select! {
                _ = &mut ctrl_c => break,
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
                        (Err(e), None) => eprintln!("{} incoming: {e:#}", now()),
                        (Err(e), Some((id, label))) => {
                            eprintln!("{} {label}: {e:#}", now());
                            pushed.remove(&id);
                            retry.insert(id, tokio::time::Instant::now() + RETRY);
                        }
                    }
                }
                _ = tick.tick() => {
                    // Paused (da-wlbyx), it idles: an incoming session's
                    // handshake refuses, and nothing is pushed.
                    let peers = match db.sync_paused()? {
                        Some(_) => Vec::new(),
                        None => synced_peers(db)?,
                    };
                    let vector = db.sync_vector()?;
                    for p in peers {
                        let waiting = retry.get(&p.id).is_some_and(|t| *t > tokio::time::Instant::now());
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
