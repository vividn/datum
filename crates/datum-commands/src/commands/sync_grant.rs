//! `datum sync grant` (da-n0uj1.4): a device joins this database's chain
//! with no ticket carried by hand. The granter writes an invite here, as
//! `chain invite` does, and its join link, sealed to the device's key, in a
//! `grant` record of a chain the two share already: the CARRIER, whose
//! members relay it like any record, so it waits there for a device that is
//! offline. The device, syncing the carrier, opens it and redeems the link
//! as `chain connect` does: the admission is today's, every check with it.
//!
//! datum.day's CLI makes no ticket, having no network (da-n0uj1.7): there
//! the grant seals the one its sync worker made for an invite written
//! first (`--ticket`, `--invite`), and `--like` makes none but leaves them
//! to the page (`--defer`). The rest is the same in either build.

use std::path::PathBuf;
#[cfg(feature = "net")]
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(feature = "net")]
use std::time::Instant;

use anyhow::{bail, ensure, Context, Result};
#[cfg(feature = "net")]
use chrono::DateTime;
use chrono::Utc;
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

/// `datum --db X sync grant <DEVICE> member|reader [--as NAME]`. `given`:
/// the ticket made elsewhere for an invite already here, and that invite.
pub fn grant(
    db: &Db,
    who: &str,
    reader: bool,
    name: Option<&str>,
    expires: Duration,
    admin_key: Option<&str>,
    given: Option<(&str, &str)>,
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
    let role = if reader { "reader" } else { "member" };
    let (ticket, expires_at, invite) = match given {
        Some((ticket, id)) => {
            let secret = chain::ticket_secret(ticket)
                .context("--ticket is not a ticket: base32 of its JSON, a secret in it")?;
            let Some((r, filtered, until)) = here.open_invite(id, &secret) else {
                bail!("--ticket is no ticket of invite {id}, an unused invite of this sync chain");
            };
            ensure!(
                (r == Role::Reader) == reader,
                "invite {id} is a {}'s, not a {role}'s",
                r.as_str()
            );
            ensure!(
                !filtered,
                "invite {id} is a filtered reader's: a grant is of all of this database"
            );
            (ticket.to_string(), until.to_string(), id.to_string())
        }
        #[cfg(feature = "net")]
        None => {
            let caps = super::sync::caps(db, reader, None)?;
            let (secret, until, id) =
                super::sync::invite(db, expires, admin_key, caps, None, false)?;
            (datum_sync::grant_link(db, &secret, expires)?, until, id)
        }
        #[cfg(not(feature = "net"))]
        None => {
            let _ = (expires, admin_key);
            bail!("this build makes no ticket: datum.day's sync widget grants a device")
        }
    };
    let as_name = name.unwrap_or(db_name);
    let payload = json!({
        "ticket": ticket,
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

/// `datum --db X sync grant --like SRC [--devices D,…]` (da-n0uj1.5, da-pvax0):
/// X takes SRC's devices, a grant each with the role it has in SRC. X in no
/// chain starts one whose admins are SRC's (vividn: «if db2 is really new,
/// then can auto setup admin to be the same as db1»), signed by one of them;
/// X in a chain takes grants for the devices it lacks, signed by its own
/// admin. It never starts a second chain. `defer`: no grant is made,
/// and the ones left are printed as JSON, `{"grants": [{device, label,
/// role}]}`, what was said going to stderr.
pub fn grant_like(
    db: &Db,
    like: &str,
    devices: Option<&[String]>,
    expires: Duration,
    admin_key: Option<&str>,
    defer: bool,
) -> Result<()> {
    ensure!(
        defer || cfg!(feature = "net"),
        "this build makes no ticket: datum.day's sync widget grants a database's devices"
    );
    let say = |line: String| match defer {
        true => eprintln!("{line}"),
        false => println!("{line}"),
    };
    let path = config::locate_db(Some(like)).context("Cannot determine data directory")?;
    ensure!(path.exists(), "no database {like}");
    let canon = |p: &std::path::Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    ensure!(
        db.path().map(canon) != Some(canon(&path)),
        "{like} is this database"
    );
    let src_db = Db::open_read_only(&path.to_string_lossy())?;
    let src = src_db.sync_chain()?;
    ensure!(
        src.exists() && src.dissolved().is_none(),
        "{like} is in no sync chain: it has no devices to grant"
    );
    // A grant rides SRC's chain to its device: so before anything is
    // written, this install writes there.
    let src_me = src_db.sync_endpoint_id()?;
    ensure!(
        src.member(&src_me)
            .is_some_and(|m| live(m) && m.role() == Role::Member),
        "this install's {like} writes nothing to its sync chain (a reader, or expelled): \
         it carries no grant to {like}'s devices"
    );
    let here = db.sync_chain()?;
    let starts = !here.exists() || here.dissolved().is_some();
    ensure!(
        starts || here.id != src.id,
        "this database is in {like}'s sync chain already"
    );
    if !starts {
        let me = here.member(&db.sync_endpoint_id()?);
        ensure!(
            me.is_some_and(|m| live(m) && m.role() == Role::Member),
            "this copy writes nothing to its sync chain (a reader, or expelled): it grants no devices"
        );
    }
    // Whose admin signs: this chain's, or the one a new chain takes its admins from.
    let (signer, whose) = match starts {
        true => (&src, like),
        false => (&here, "this database"),
    };
    if let Some(k) = admin_key.filter(|_| !signer.is_open()) {
        let key = chain::ed25519_admin_key(&super::sync::key_file(k, false)?);
        ensure!(
            signer.admins.iter().any(|a| a.key == key),
            "{k} is no admin key of {whose}'s sync chain: only its admin grants devices into it"
        );
    }

    // Each device live in SRC, once, but this one; an endpoint bound to no
    // device key (a copy before da-n0uj1.2) is counted, and left.
    let mine = datum_core::db::device_id();
    let mut picked: Vec<&Member> = Vec::new();
    let mut unbound = 0;
    for m in &src.members {
        if !live(m) || m.left.is_some() || m.endpoint == src_me {
            continue;
        }
        match &m.device {
            None => unbound += 1,
            Some(d) if Some(d) == mine.as_ref() => {}
            Some(_) if picked.iter().any(|p| p.device == m.device) => {}
            Some(_) => picked.push(m),
        }
    }
    if let Some(names) = devices {
        let column = [(like.to_string(), path.clone(), src_db.sync_chain()?)];
        let mut wanted = Vec::new();
        for n in names {
            let (d, label) = device(&column, n)?;
            ensure!(
                picked.iter().any(|m| m.device.as_ref() == Some(&d)),
                "{label} is no device of {like}'s sync chain: expelled, gone, or this one"
            );
            wanted.push(d);
        }
        picked.retain(|m| m.device.as_ref().is_some_and(|d| wanted.contains(d)));
        // The devices named are the grants: the unbound were not asked for.
        unbound = 0;
    }

    // The genesis, if any, and an invite per device to grant, approved at
    // once (da-7bjai): a passkey is asked one approval for them all.
    let mut records = Vec::new();
    let chain = match starts {
        true => {
            let admins: Vec<Value> = src
                .admins
                .iter()
                .map(|a| json!({"key": a.key, "label": a.label}))
                .collect();
            let genesis = json!({
                "type": "genesis", "chain": ulid::Ulid::new().to_string().to_lowercase(),
                "admins": admins, "label": super::sync::copy_label(),
                "created": Utc::now().to_rfc3339(),
            });
            let id = genesis["chain"].as_str().unwrap_or_default().to_string();
            records.push(genesis);
            id
        }
        false => here.id.clone(),
    };
    let mut notes = Vec::new();
    let mut invited = Vec::new();
    for m in picked {
        let d = m.device.as_deref().unwrap_or_default();
        let label = device_label(m);
        let had = here
            .members
            .iter()
            .find(|h| !starts && h.device.as_deref() == Some(d) && live(h));
        if let Some(had) = had {
            notes.push(format!("{label} is a {} here already", had.role().as_str()));
        } else if m.filter.is_some() {
            notes.push(format!(
                "{label} is a reader of some of {like}'s fields, which are not this database's: \
                 `datum sync grant {} reader` makes it a reader of all of it",
                &d[..12.min(d.len())]
            ));
        } else {
            let reader = m.role() == Role::Reader;
            let caps = super::sync::caps(db, reader, None)?;
            let (secret, until, id, record) = super::sync::invite_record(&chain, expires, caps);
            records.push(record);
            invited.push((d.to_string(), label, reader, secret, until, id));
        }
    }
    // Before the approval, so a run that stops at its link (datum.day's,
    // which approves with `sync approve`) has handed every secret over.
    if defer {
        let grants: Vec<Value> = invited
            .iter()
            .map(|(device, label, reader, secret, expires, id)| {
                json!({
                    "device": device, "label": label,
                    "role": if *reader { "reader" } else { "member" },
                    "invite": {"secret": secret, "expires": expires, "id": id},
                })
            })
            .collect();
        println!("{}", json!({ "grants": grants }));
    }
    if starts && here.dissolved().is_some() {
        db.sync_chain_leave()?;
    }
    super::sync::sign_all(db, records, signer, admin_key)?;
    if starts {
        if let Err(e) = super::sync::describe_self(db, None, super::sync::kind()) {
            eprintln!("⚠ this copy could not describe itself ({e}); `datum sync chain rename <LABEL>` tries again");
        }
        #[cfg(feature = "net")]
        super::sync_daemon::auto_install(db);
        let admins = match src.admins.len() {
            0 => format!("open, as {like}'s is"),
            n => format!("its admins {like}'s {n}"),
        };
        say(format!("started sync chain {chain}: {admins}"));
    }
    notes.into_iter().for_each(say);
    #[cfg(feature = "net")]
    if !defer {
        for (device, _, reader, secret, _, id) in &invited {
            let ticket = datum_sync::grant_link(db, secret, expires)?;
            let given = Some((ticket.as_str(), id.as_str()));
            grant(db, device, *reader, None, expires, admin_key, given)?;
        }
    }
    if unbound > 0 {
        say(format!(
            "{unbound} member(s) of {like} bound to no device key (an older datum's copy) \
             are not granted: `datum sync chain invite` makes a ticket to carry to each"
        ));
    }
    Ok(())
}

#[cfg(feature = "net")]
/// A grant to this device, opened: the name it joins under, its join link
/// and who granted it.
struct Granted {
    name: String,
    ticket: String,
    from: String,
}

#[cfg(feature = "net")]
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

#[cfg(feature = "net")]
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

#[cfg(feature = "net")]
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

#[cfg(feature = "net")]
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

#[cfg(feature = "net")]
/// A `sync run`'s eye on its grants: on a tick after a chain record came,
/// or once a failed join's wait is out (a minute, doubling to half an hour),
/// it joins those pending, one at a time, on a thread of its own, so the run
/// serves on meanwhile.
#[derive(Default, Clone)]
pub struct Watch(Arc<Mutex<WatchState>>);

#[cfg(feature = "net")]
#[derive(Default)]
struct WatchState {
    seen: i64,
    /// When to try a failed join again, and the wait it waited.
    retry: Option<(Instant, Duration)>,
    joining: bool,
}

#[cfg(feature = "net")]
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
