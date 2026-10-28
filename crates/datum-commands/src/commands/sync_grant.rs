//! `datum sync grant` (da-n0uj1.4): a device joins this database's chain
//! with no ticket carried by hand. The granter writes an invite here, as
//! `chain invite` does, and its join link, sealed to the device's key, in a
//! `grant` record of a chain the two share already: the CARRIER, whose
//! members relay it like any record, so it waits there for a device that is
//! offline. The device, syncing the carrier, opens it and redeems the link
//! as `chain connect` does: the admission is today's, every check with it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use datum_core::db::chain::{self, Chain, Member, Role};
use datum_core::db::Db;
use serde_json::{json, Value};

use super::sync_grid::{chains, device_label};
use crate::config;

/// Live in its chain, reader or not.
fn live(m: &Member) -> bool {
    m.revoked.is_none()
}

/// The device `who` names among the members of every chain this install
/// holds: its key, at least [`chain::MIN_PREFIX`] characters of it, or its
/// label as `sync grid` shows it. Its key and that label.
fn device(columns: &[(String, PathBuf, Chain)], who: &str) -> Result<(String, String)> {
    let mut hits: Vec<(String, String)> = Vec::new();
    let members = columns.iter().flat_map(|(_, _, c)| &c.members);
    for m in members {
        let Some(d) = &m.device else { continue };
        let label = device_label(m);
        let named =
            d == who || label == who || who.len() >= chain::MIN_PREFIX && d.starts_with(who);
        if named && hits.iter().all(|(h, _)| h != d) {
            hits.push((d.clone(), label));
        }
    }
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => bail!(
            "{who} names no device in this install's sync chains: `datum sync grid` lists them"
        ),
        n => bail!("{who} names {n} devices: give more of its key (`datum sync grid`)"),
    }
}

/// `datum --db X sync grant <DEVICE> member|reader [--as NAME]`.
pub fn grant(
    db: &Db,
    who: &str,
    reader: bool,
    name: Option<&str>,
    expires: Duration,
    admin_key: Option<&str>,
) -> Result<()> {
    let columns = chains(db)?;
    let here = db.sync_chain()?;
    let Some((db_name, _, _)) = columns.first().filter(|(_, _, c)| c.id == here.id) else {
        bail!("this copy is in no sync chain: `datum sync chain invite` starts one");
    };
    if let Some(n) = name {
        ensure!(
            !n.is_empty() && !config::looks_like_path(n),
            "--as is a database's name, not a path: {n}"
        );
    }
    let (device, label) = device(&columns, who)?;
    ensure!(
        datum_core::db::device_id().as_ref() != Some(&device),
        "{label} is this device: its databases are here already"
    );
    if let Some(m) = here
        .members
        .iter()
        .find(|m| m.device.as_ref() == Some(&device) && live(m))
    {
        bail!(
            "{label} is a {} of this sync chain already: `datum sync chain role` changes that",
            m.role().as_str()
        );
    }
    // The carrier: a chain the device is live in, and this install's copy
    // in it a live member that writes.
    let carrier = columns.iter().skip(1).find_map(|(name, path, c)| {
        let holds = c
            .members
            .iter()
            .any(|m| m.device.as_ref() == Some(&device) && live(m));
        let copy = holds
            .then(|| Db::open(&path.to_string_lossy()).ok())
            .flatten()?;
        let me = c.member(&copy.sync_endpoint_id().ok()?)?;
        (live(me) && me.role() == Role::Member).then_some((name, c, copy))
    });
    let Some((via, carrier, copy)) = carrier else {
        bail!(
            "{label} shares no sync chain where this install is a member (a reader sends no \
             grant): `datum sync chain invite` makes a ticket to carry to it instead"
        );
    };
    let caps = super::sync::caps(db, reader, None)?;
    let (secret, expires_at, invite) =
        super::sync::invite(db, expires, admin_key, caps, None, false)?;
    let role = if reader { "reader" } else { "member" };
    let as_name = name.unwrap_or(db_name);
    let payload = json!({
        "ticket": datum_sync::grant_link(db, &secret, expires)?,
        "chain": here.id, "invite": invite, "db": as_name, "role": role, "expires": expires_at,
        "from": here.member(&db.sync_endpoint_id()?).map(|m| m.label.clone()),
    });
    let (eph, sealed) = chain::seal_grant(&device, &payload).map_err(anyhow::Error::msg)?;
    copy.sync_chain_write(
        json!({
            "type": "grant", "chain": carrier.id, "to": device, "eph": eph, "sealed": sealed,
            "created": Utc::now().to_rfc3339(),
        }),
        true,
    )?;
    println!(
        "granted {label} {role} of this database: the invite rides sealed to it in {via}'s sync \
         chain, and {label} joins as «{as_name}» when it next syncs {via} there, until {}",
        datum_core::time::on_readers_clock(&expires_at, "%Y-%m-%d %H:%M")
    );
    Ok(())
}

/// A grant to this device, opened: the name it joins under, its join link
/// and who granted it.
struct Granted {
    name: String,
    ticket: String,
    from: String,
}

/// The grants to this device that `db`'s chain holds that no database here
/// was admitted by: one live in their chain whose latest admission came
/// through the grant's invite. An expelled copy is never told so, and holds
/// its chain as a member, so a grant made since admits it again. The newest
/// per chain. One expired is left, and said only when `say_expired`, as it
/// arrives: a chain record stays for good.
fn pending(db: &Db, say_expired: bool) -> Result<Vec<Granted>> {
    let grants = db.sync_chain()?.grants;
    if grants.is_empty() {
        return Ok(Vec::new());
    }
    // Each chain a copy here is live in, and the `via` it came in by.
    let mut came: Vec<(String, String)> = Vec::new();
    for (_, path, c) in chains(db)? {
        let me = Db::open_read_only(&path.to_string_lossy())?.sync_endpoint_id()?;
        if let Some(m) = c.member(&me).filter(|m| live(m)) {
            came.push((c.id.clone(), m.via.clone()));
        }
    }
    let mut newest: Vec<Value> = Vec::new();
    for p in grants.iter().filter_map(chain::open_grant) {
        let via = format!("invite {}", p["invite"].as_str().unwrap_or_default());
        if !came.iter().any(|(c, v)| p["chain"] == *c && *v == via) {
            newest.retain(|q| q["chain"] != p["chain"]);
            newest.push(p);
        }
    }
    let mut out = Vec::new();
    for p in newest {
        let s = |k: &str| p[k].as_str().unwrap_or_default().to_string();
        let from = p["from"].as_str().unwrap_or("a device").to_string();
        let expired = DateTime::parse_from_rfc3339(&s("expires")).is_ok_and(|t| t < Utc::now());
        if expired {
            if !say_expired {
                continue;
            }
            eprintln!(
                "⚠ {from}'s grant of «{}» expired before this device synced: ask it for another",
                s("db")
            );
            continue;
        }
        out.push(Granted {
            name: free_name(&s("db"), &s("chain"))?,
            ticket: s("ticket"),
            from,
        });
    }
    Ok(out)
}

/// The name a grant of `chain` joins under here: `want`, or `want-2`, `-3`…
/// where a database of that name holds data or another chain (it is never
/// touched), or another join. A database already in `chain` is its own: a
/// join there cut short.
fn free_name(want: &str, chain: &str) -> Result<String> {
    ensure!(
        !want.is_empty() && !config::looks_like_path(want),
        "a grant names no database: {want:?}"
    );
    for n in 1.. {
        let name = match n {
            1 => want.to_string(),
            n => format!("{want}-{n}"),
        };
        let path = config::locate_db(Some(&name)).context("Cannot determine data directory")?;
        if !path.exists() {
            return Ok(name);
        }
        let there = Db::open_read_only(&path.to_string_lossy())?;
        let c = there.sync_chain()?;
        let empty = !c.exists() && there.sync_holds()? == 0 && there.sync_joining()?.is_empty();
        if c.id == chain || empty {
            return Ok(name);
        }
    }
    unreachable!()
}

/// Join `g` as `chain connect` does, in a process of its own, with this
/// one's environment, so its data directory and registry: the ticket on its
/// stdin, which no other user reads. A bound address (the tests') gets a
/// port of its own: the carrier's run may hold this one.
fn join(g: &Granted, quiet: bool) -> Result<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut c = Command::new(std::env::current_exe()?);
    c.args(["--db", &g.name, "sync", "chain", "connect"])
        .stdin(Stdio::piped());
    if let Ok(at) = std::env::var("DATUM_SYNC_BIND") {
        let ip = at
            .parse::<std::net::SocketAddr>()
            .context("DATUM_SYNC_BIND")?
            .ip();
        c.env(
            "DATUM_SYNC_BIND",
            std::net::SocketAddr::new(ip, 0).to_string(),
        );
    }
    if quiet {
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    let mut child = c.spawn()?;
    child
        .stdin
        .take()
        .context("no stdin")?
        .write_all(g.ticket.as_bytes())?;
    let out = child.wait_with_output()?;
    let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
    ensure!(out.status.success(), "joining «{}» failed: {said}", g.name);
    Ok(format!("joined «{}», granted by {}", g.name, g.from))
}

/// The chain records `db` holds: a grant's arrival moves it.
pub fn records(db: &Db) -> i64 {
    let count = db.conn().query_row(
        "SELECT COUNT(*) FROM ops WHERE hid = ?1 AND applied = 1",
        [chain::CHAIN_HID],
        |r| r.get::<_, i64>(0),
    );
    count.unwrap_or(0)
}

/// After a bare `datum sync` of `db`, which held `before` chain records:
/// join every grant pending, saying so as it goes. A join that fails is
/// said, and tried again at the next sync: the sync itself did not fail.
pub fn redeem(db: &Db, before: i64) -> Result<()> {
    for g in pending(db, records(db) != before)? {
        println!("joining «{}»: {} granted it to this device", g.name, g.from);
        match join(&g, false) {
            Ok(said) => println!("{said}"),
            Err(e) => eprintln!("⚠ {e:#}; `datum sync` tries again"),
        }
    }
    Ok(())
}

/// A `sync run`'s eye on its grants: on a tick after a chain record came,
/// or once a failed join's wait is out (a minute, doubling to half an hour),
/// it joins those pending, one at a time, on a thread of its own, so the run
/// serves on meanwhile.
#[derive(Default, Clone)]
pub struct Watch(Arc<Mutex<WatchState>>);

#[derive(Default)]
struct WatchState {
    seen: i64,
    /// When to try a failed join again, and the wait it waited.
    retry: Option<(Instant, Duration)>,
    joining: bool,
}

impl Watch {
    pub fn tick(&self, db: &Db) {
        let count = records(db);
        let mut s = self.0.lock().unwrap();
        let due = s.retry.is_some_and(|(at, _)| at <= Instant::now());
        if s.joining || (count == s.seen && !due) {
            return;
        }
        let arrived = count != s.seen;
        s.seen = count;
        let grants = match pending(db, arrived) {
            Ok(g) if !g.is_empty() => g,
            Ok(_) => return,
            Err(e) => return eprintln!("grants: {e:#}"),
        };
        s.joining = true;
        let watch = self.0.clone();
        std::thread::spawn(move || {
            let mut failed = false;
            for g in grants {
                match join(&g, true) {
                    Ok(said) => eprintln!("{said}"),
                    Err(e) => {
                        eprintln!("{e:#}; trying again later");
                        failed = true;
                    }
                }
            }
            let mut s = watch.lock().unwrap();
            s.joining = false;
            let wait = s.retry.map_or(Duration::from_secs(60), |(_, w)| {
                (w * 2).min(Duration::from_secs(1800))
            });
            s.retry = failed.then(|| (Instant::now() + wait, wait));
        });
    }
}
