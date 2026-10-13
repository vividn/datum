//! `datum sync` — Easy Sync. The merge is `datum_core::db::sync` (da-k329q),
//! and here is the pipe it runs over: ops as JSON lines on stdout (`pipe
//! ops`) and stdin (`pipe apply`). Pairing and syncing over iroh is `datum_sync`
//! (da-rugg8), behind the `net` feature. The sync chain's admin acts
//! (da-odwc7) are signed here, with a key file or by a passkey on datum.day
//! — once the chain has an admin; an open one's are the members' own
//! (da-6177s).

use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{bail, ensure, Context, Result};
use datum_core::db::chain::{self, Chain, Notice};
use datum_core::db::Db;
use datum_core::error::DatumError;
use datum_core::migrations::CURRENT_SCHEMA_VERSION;
use serde_json::{json, Value};

use crate::args::{ShowFlag, SyncAdminAct, SyncCommand, SyncFilter};
use crate::commands::check::{Domain, Issue, Severity};

const NO_CHAIN: &str = "this copy is in no sync chain: `datum sync chain invite` on this device starts one, or `datum sync chain connect <TICKET>` joins one with a ticket from another";

/// What a copy joining a chain through `label` (`endpoint`) says of it.
fn joining(label: &str, endpoint: &str) -> String {
    format!(
        "joining a sync chain through {label} ({}): its first sync did not finish — `datum sync` resumes it",
        &endpoint[..10.min(endpoint.len())]
    )
}

/// What an open chain says beside its members.
const OPEN: &str =
    "no admin: any device can add devices — `datum sync admin add` to protect the chain";

/// `datum sync …`. A session it ran bound this thread to the endpoint it
/// began as, and that ends with the command (da-otz0d): the next one here, a
/// REPL's next line, may purge or rekey the copy.
pub fn sync(cmd: SyncCommand, db: &Db) -> Result<()> {
    let done = command(cmd, db);
    db.sync_session_as(None);
    done
}

fn command(cmd: SyncCommand, db: &Db) -> Result<()> {
    match cmd {
        SyncCommand::As(endpoint, cmd) => {
            db.sync_session_as(Some(&endpoint));
            command(*cmd, db)?
        }
        SyncCommand::Now => {
            db.sync_refuse_debug()?;
            if !db.sync_chain()?.exists() && db.sync_joining()?.is_empty() {
                println!("{NO_CHAIN}");
                return Ok(());
            }
            if let Some(at) = db.sync_paused()? {
                bail!("sync is paused on this copy since {at}: `datum sync resume` to sync again");
            }
            #[cfg(feature = "net")]
            let synced = datum_sync::now_all(db);
            #[cfg(not(feature = "net"))]
            let synced: Result<()> = Err(anyhow::anyhow!(NO_NET));
            if let Some(line) = summary(db)? {
                println!("{line}");
            }
            synced?
        }
        SyncCommand::Pause => {
            ensure!(db.sync_chain()?.exists(), NO_CHAIN);
            match db.sync_set_paused(true)? {
                Some(at) => println!("sync was already paused, since {at}"),
                None => println!(
                    "sync paused: this copy neither sends nor receives until `datum sync resume`"
                ),
            }
        }
        SyncCommand::Resume => match db.sync_set_paused(false)? {
            Some(at) => println!("sync resumed (paused since {at})"),
            None => println!("sync was not paused"),
        },
        SyncCommand::Vector => {
            println!("{}", serde_json::to_string(&db.sync_vector()?)?);
        }
        SyncCommand::Ops {
            since,
            overlap,
            peer,
        } => {
            let since: BTreeMap<String, i64> = match since {
                Some(v) => serde_json::from_str(&v)
                    .with_context(|| format!("--since is not a version vector: {v}"))?,
                None => BTreeMap::new(),
            };
            for line in db.sync_ops_since(&since, overlap, peer.as_deref())? {
                println!("{line}");
            }
        }
        SyncCommand::Records => {
            for line in db.sync_chain_records()? {
                println!("{line}");
            }
        }
        SyncCommand::Apply { show } => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            let s = db.sync_apply(&input)?;
            // A session's apply (datum.day's transport): the refusals are
            // the summary's, as the native session reports them, not a failure.
            if show == ShowFlag::Json {
                let out = json!({
                    "applied": s.applied, "already_had": s.already_had, "held": s.held,
                    "conflicts": s.conflicts, "refused": s.refused,
                });
                println!("{out}");
                return Ok(());
            }
            for why in &s.refused {
                eprintln!("refused: {why}");
            }
            println!(
                "applied {} · already had {} · held {} · conflicts {}",
                s.applied, s.already_had, s.held, s.conflicts
            );
            if !s.refused.is_empty() {
                anyhow::bail!("{} op(s) refused", s.refused.len());
            }
        }
        SyncCommand::Leave {
            alone,
            dry_run,
            force,
        } => {
            let chain = db.sync_chain()?;
            ensure!(chain.exists(), NO_CHAIN);
            let mut told = None;
            match alone {
                true => db.sync_chain_leave_alone(dry_run)?,
                false if dry_run => {}
                false => {
                    if !force {
                        told = depart(db, &chain.id, "datum sync chain leave")?;
                    }
                    db.sync_chain_leave()?
                }
            }
            if !dry_run {
                println!(
                    "left sync chain {}: this copy is in none, its entries kept",
                    chain.id
                );
                if let Some(told) = told {
                    println!(
                        "its departure is with {}: every member learns of it through them",
                        told.join(", ")
                    );
                }
                if !alone {
                    println!(
                        "new endpoint key {}: an invite from this copy starts a new chain",
                        db.sync_endpoint_id()?
                    );
                }
            }
        }
        SyncCommand::Rekey => {
            println!("new endpoint key {}", db.sync_rekey()?);
            println!(
                "the new key is no member of this copy's sync chain: join it again with an invite"
            );
        }
        SyncCommand::Verify { verbose, peer } => {
            let (hash, per_column) = db.sync_state_hash(peer.as_deref())?;
            println!("{hash}");
            if verbose {
                for (column, hash) in per_column {
                    println!("  {column:<24} {hash}");
                }
            }
        }
        SyncCommand::Digest { peer } => {
            println!("{}", json!(db.sync_digest(peer.as_deref())?));
        }
        SyncCommand::Held { peer } => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            let hids: Vec<String> =
                serde_json::from_str(&input).context("stdin is not a JSON array of hids")?;
            println!("{}", json!(db.sync_held(peer.as_deref(), &hids)?));
        }
        SyncCommand::Refold { show } => super::sync_diagnose::refold(db, show == ShowFlag::Json)?,
        SyncCommand::Admin {
            act,
            who,
            label,
            admin_key,
            allow_unlock,
        } => {
            let chain = db.sync_chain()?;
            ensure!(chain.exists(), NO_CHAIN);
            let (mut record, said) = match act {
                SyncAdminAct::Revoke => {
                    let m = member(&chain, &who)?;
                    (
                        json!({"type": "revoke", "endpoint": m}),
                        format!("revoked {who}"),
                    )
                }
                SyncAdminAct::Revert { yes, show } => {
                    let m = member(&chain, &who)?;
                    let purged = chain.member(&m).and_then(|m| m.purged.as_deref());
                    let p = db.sync_purge_preview(&m)?;
                    if show == ShowFlag::Json && !yes {
                        let label = chain.member(&m).map(|m| m.label.as_str());
                        let out = json!({
                            "endpoint": m, "label": label, "created": p.created,
                            "edited": p.edited, "deleted": p.deleted, "defined": p.defined, "first": p.first,
                            "last": p.last, "purged": purged.map(chain::hlc_time),
                        });
                        println!("{out}");
                        return Ok(());
                    }
                    ensure!(
                        purged.is_none(),
                        "{who} is reverted already: a fresh invite brings it back, its ops with it"
                    );
                    let what = revert_words(&who, &m, &p);
                    if !yes {
                        println!(
                            "{what}\nnothing changed: `datum sync chain revert {who} --yes` does it"
                        );
                        return Ok(());
                    }
                    (
                        json!({"type": "purge", "endpoint": m}),
                        format!("reverted: {what}\na fresh invite brings it back, its ops with it"),
                    )
                }
                SyncAdminAct::Role { reader, filter } => {
                    let m = member(&chain, &who)?;
                    let role = if reader { "reader" } else { "member" };
                    let filter = resolve(db, filter)?;
                    // Nothing to sign: in a locked chain, a passkey tap for nothing.
                    let now = chain
                        .member(&m)
                        .map(|m| (m.role().as_str(), m.filter.clone()));
                    let was =
                        |f: &chain::Filter| filter.as_ref().is_some_and(|g| g.fields == f.fields);
                    ensure!(
                        !now.as_ref().is_some_and(
                            |(r, f)| *r == role && f.as_ref().map_or(filter.is_none(), was)
                        ),
                        "{who} is already a {role}{}",
                        filter
                            .as_ref()
                            .map(|f| format!(" of those fields ({})", f.describe()))
                            .unwrap_or_default()
                    );
                    let mut r = json!({"type": "role", "endpoint": m, "role": role});
                    let said = match &filter {
                        Some(f) => {
                            r["fields"] = json!(f.fields);
                            r["of"] = f.of.into();
                            format!("{who} is a reader, {}", f.describe())
                        }
                        None => format!("{who} is a {role}"),
                    };
                    (r, said)
                }
                SyncAdminAct::AddAdmin => {
                    let (key, l) = match who.trim_start().starts_with('{') {
                        true => passkey(&who)?,
                        false => (
                            chain::ed25519_admin_key(&key_file(&who, true)?),
                            file_label(&who),
                        ),
                    };
                    let label = checked(label.unwrap_or(l))?;
                    let r = json!({"type": "add_admin", "key": key, "label": label});
                    (r, format!("added admin {label}"))
                }
                SyncAdminAct::RemoveAdmin => {
                    let hits: Vec<_> = chain
                        .admins
                        .iter()
                        .filter(|a| names(&who, &a.label, &a.key))
                        .collect();
                    let [a] = hits.as_slice() else {
                        bail!("{who} names {} admins of this chain", hits.len());
                    };
                    let mut r = json!({"type": "remove_admin", "key": a.key});
                    if chain.admins.len() > 1 {
                        (r, format!("removed admin {}", a.label))
                    } else {
                        ensure!(
                            allow_unlock,
                            "this is the last admin: removing it UNLOCKS the chain, so any device can add, revoke or rename devices. Pass --allow-unlock to do it"
                        );
                        r["unlock"] = true.into();
                        (r, format!("removed admin {}: the chain is unlocked, so any device can add, revoke or rename devices", a.label))
                    }
                }
            };
            record["chain"] = chain.id.clone().into();
            record["created"] = chrono::Utc::now().to_rfc3339().into();
            sign(db, record, &chain, admin_key.as_deref())?;
            println!("{said}");
        }
        SyncCommand::Members { show, show_revoked } => {
            members(db, show == ShowFlag::Json, show_revoked)?
        }
        SyncCommand::AdminList { show } => admins(db, show == ShowFlag::Json)?,
        SyncCommand::Rename {
            who,
            label,
            admin_key,
        } => {
            let label = checked(label)?;
            let chain = db.sync_chain()?;
            ensure!(chain.exists(), NO_CHAIN);
            let mine = db.sync_endpoint_id()?;
            let target = match &who {
                Some(who) => member(&chain, who)?,
                None => mine.clone(),
            };
            let m = chain.member(&target).context("this copy is not a member")?;
            if target == mine {
                db.sync_describe_self(&label, m.kind.as_deref().unwrap_or(kind()))?;
            } else {
                let record = json!({
                    "type": "describe", "chain": chain.id, "endpoint": target,
                    "client": m.client, "label": label, "kind": m.kind,
                    "created": chrono::Utc::now().to_rfc3339(),
                });
                sign(db, record, &chain, admin_key.as_deref())?;
            }
            println!("renamed {} to {label}", m.label);
        }
        SyncCommand::Status { show, show_revoked } => {
            status(db, show == ShowFlag::Json, show_revoked)?
        }
        // The page's network worker runs the transport outside the CLI, and
        // the iroh key IS this secret. Handing it over widens nothing: the
        // page can already read the whole db file, `sync_local` included
        // (vividn, 2026-09-29, da-rcb5m). Hidden from help and completions.
        SyncCommand::EndpointKey => {
            let key = db.sync_endpoint_secret()?;
            println!(
                "{}",
                key.iter().map(|b| format!("{b:02x}")).collect::<String>()
            );
        }
        SyncCommand::Holds => println!("{}", db.sync_holds()?),
        SyncCommand::RecordSession {
            endpoint,
            vector,
            verify,
            error,
            met,
        } => {
            let vector: Option<BTreeMap<String, i64>> = vector
                .map(|v| serde_json::from_str(&v))
                .transpose()
                .context("--vector is not a version vector")?;
            db.sync_record_session(&endpoint, vector.as_ref(), verify, error.as_deref())?;
            if let Some(met) = met {
                db.sync_introduce(&met)?;
            }
        }
        SyncCommand::Handshake {
            endpoint,
            label,
            addr,
            invite,
            chain,
            holds,
            version,
            records,
            dialed,
        } => {
            let (invite, chain) = (invite.as_deref(), chain.as_deref());
            let mut shown = String::new();
            if records {
                std::io::stdin().read_to_string(&mut shown)?;
            }
            let records = records.then_some(shown.as_str());
            // A refusal's kind on stdout, for datum.day's store (da-b6xh6).
            let added = db
                .sync_handshake(
                    &endpoint, &label, &addr, invite, chain, holds, records, dialed,
                )
                .inspect_err(|e| {
                    if let DatumError::Refused(kind, _) = e {
                        println!("{}", json!(kind));
                    }
                })?;
            // Recording never stops a sync (da-3zqyv).
            if let Some((datum, schema)) = version {
                if let Ok(Some(w)) = db.sync_record_version(&endpoint, &datum, schema) {
                    eprintln!("⚠ {w}");
                }
            }
            println!("{}", if added { "added" } else { "known" });
        }
        SyncCommand::NewInvite {
            reader,
            filter,
            expires_secs,
            admin_key,
        } => {
            let expires = std::time::Duration::from_secs(expires_secs);
            let caps = caps(db, reader, filter)?;
            invite(db, expires, admin_key.as_deref(), caps, None, true)?;
        }
        SyncCommand::Approve { link } => {
            let chain = db.sync_chain()?;
            ensure!(chain.exists(), NO_CHAIN);
            let mut record = chain::approve_record(&link)
                .context("not an approve link: https://datum.day/sync/approve#<record>")?;
            let mut code = String::new();
            std::io::stdin().read_line(&mut code)?;
            ensure!(!code.trim().is_empty(), "no approval code");
            record["sig"] = code.trim().into();
            let what = record["type"].as_str().unwrap_or("record").to_string();
            db.sync_chain_write(record, true)?;
            println!("approved: {what}");
        }
        SyncCommand::Qr { text } => {
            let code = qrcode::QrCode::new(text.as_bytes())?;
            println!("{}", code.render::<qrcode::render::svg::Color>().build());
        }
        SyncCommand::Ack { who: Some(who), .. } => {
            ensure!(db.sync_ack(&who)? > 0, "nothing to acknowledge for {who}");
            println!("acknowledged {who}");
        }
        // `--all`: every notice pending, each named, so none goes unseen.
        SyncCommand::Ack { all: true, .. } => {
            let notices = db.sync_notices_unacked()?;
            if notices.is_empty() {
                println!("nothing to acknowledge");
            }
            for n in notices {
                db.sync_ack(n.who())?;
                println!("acknowledged {n}");
            }
        }
        // Bare: the one notice pending, or the list to name one from.
        SyncCommand::Ack { who: None, .. } => match db.sync_notices_unacked()?.as_slice() {
            [] => bail!("nothing to acknowledge"),
            [n] => {
                db.sync_ack(n.who())?;
                println!("acknowledged {n}");
            }
            many => bail!(
                "{} to acknowledge; name one, or `datum sync chain ack --all`:\n{}",
                many.len(),
                many.iter()
                    .map(|n| format!("  ⚠ {n} — `datum sync chain ack {}`", n.who()))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        },
        #[cfg(feature = "net")]
        SyncCommand::Invite {
            reader,
            filter,
            qr,
            expires_secs,
            label,
            admin_key,
        } => {
            let label = label.map(checked).transpose()?;
            let expires = std::time::Duration::from_secs(expires_secs);
            let starts = !db.sync_chain()?.exists();
            // The daemon leaves the copy to the invite: a joiner it admitted
            // would never be seen here, which waits out the expiry (da-apd52).
            let _held = super::sync_daemon::first_sync(db)?;
            let caps = caps(db, reader, filter)?;
            if let Some(f) = caps.get("fields") {
                eprintln!("the redeemer is a reader sent only {f}, each with its spec");
            }
            let (secret, expires_at) = invite(
                db,
                expires,
                admin_key.as_deref(),
                caps,
                label.as_deref(),
                false,
            )?;
            if starts {
                super::sync_daemon::auto_install(db);
            }
            datum_sync::offer(db, &secret, expires, &expires_at, qr)?
        }
        #[cfg(feature = "net")]
        SyncCommand::Connect { ticket, label } => {
            let label = label.map(checked).transpose()?;
            db.sync_refuse_debug()?;
            let ticket = match ticket {
                Some(t) => t,
                None => {
                    let mut t = String::new();
                    std::io::stdin().read_to_string(&mut t)?;
                    t
                }
            };
            let joins = !db.sync_chain()?.exists();
            let _first = super::sync_daemon::first_sync(db)?;
            // Admitted, it is a member whether or not its first sync ends:
            // unserved, every other copy times out on it (da-shfv4, da-qr7gz).
            datum_sync::connect(db, &ticket, label.as_deref(), &|| {
                if joins {
                    super::sync_daemon::auto_install(db);
                }
            })?;
        }
        #[cfg(feature = "net")]
        SyncCommand::Diff { who, show } => {
            let chain = db.sync_chain()?;
            ensure!(chain.exists(), NO_CHAIN);
            let m = member(&chain, &who)?;
            ensure!(m != db.sync_endpoint_id()?, "{who} is this copy");
            let label = chain.member(&m).map_or(who.as_str(), |m| &m.label);
            super::sync_diagnose::diff(db, &m, label, show == ShowFlag::Json)?
        }
        #[cfg(feature = "net")]
        SyncCommand::Run => {
            db.sync_refuse_debug()?;
            datum_sync::run(db, || Ok(()))?
        }
        #[cfg(feature = "net")]
        SyncCommand::Daemon(act) => super::sync_daemon::daemon(act)?,
        #[cfg(not(feature = "net"))]
        _ => bail!(NO_NET),
    }
    Ok(())
}

#[cfg(not(feature = "net"))]
const NO_NET: &str = "peer-to-peer sync is not in this build (no networking)";

/// Bare `datum sync`'s one line: how the members stand after it (none
/// when there are none: the sync has said so).
fn summary(db: &Db) -> Result<Option<String>> {
    let peers: Vec<_> = db
        .sync_status()?
        .into_iter()
        .filter(|p| !p.revoked)
        .collect();
    let states: Vec<String> = ["synced", "pending", "error", "never"]
        .iter()
        .map(|s| (s, peers.iter().filter(|p| p.state == *s).count()))
        .filter(|(_, n)| *n > 0)
        .map(|(s, n)| format!("{n} {s}"))
        .collect();
    Ok((!states.is_empty())
        .then(|| format!("{} · `datum sync status` for more", states.join(" · "))))
}

/// A reader's filter as the chain records it (da-u81u1): its fields — a
/// kit's, those it holds specs for here — and how many fields the database
/// has, those named included.
fn resolve(db: &Db, filter: Option<SyncFilter>) -> Result<Option<chain::Filter>> {
    let fields: std::collections::BTreeSet<String> = match filter {
        None => return Ok(None),
        Some(SyncFilter::Fields(f)) => f.into_iter().collect(),
        Some(SyncFilter::Kit(k)) => {
            let specs = db.load_specs_by_kit(&k)?;
            ensure!(!specs.is_empty(), "kit {k} has no fields in this database");
            specs.into_iter().map(|s| s.field).collect()
        }
    };
    let mut all: std::collections::BTreeSet<String> = db.distinct_fields()?.into_iter().collect();
    all.extend(fields.iter().cloned());
    Ok(Some(chain::Filter {
        fields,
        of: all.len() as i64,
    }))
}

/// An invite's `caps`: a member's `{}`, a reader's role, and its filter.
fn caps(db: &Db, reader: bool, filter: Option<SyncFilter>) -> Result<Value> {
    let mut caps = if reader {
        json!({"role": "reader"})
    } else {
        json!({})
    };
    if let Some(f) = resolve(db, filter)? {
        caps["fields"] = json!(f.fields);
        caps["of"] = f.of.into();
    }
    Ok(caps)
}

/// Write an invite any member admits the redeemer of, starting an open
/// chain with this copy its first member when it is in none: its secret,
/// which only the ticket carries, and when it expires. `caps`: what the
/// redeemer is admitted as — a reader (da-9yvb6), filtered (da-u81u1) or
/// not. `label`: what this copy is named, in a chain it starts or the one it
/// is in (da-ey3hz). `print`: say the secret on
/// stdout before the record is signed, so a run that stops at an approve
/// link has handed it over (datum.day signs with `sync approve`, da-bjxts).
fn invite(
    db: &Db,
    expires: std::time::Duration,
    admin_key: Option<&str>,
    caps: Value,
    label: Option<&str>,
    print: bool,
) -> Result<(String, String)> {
    let mut chain = db.sync_chain()?;
    let starts = !chain.exists();
    let name = label.map_or_else(copy_label, String::from);
    // The chain stands without it: a failure here is no reason to fail.
    let describe = || {
        if let Err(e) = db.sync_describe_self(&name, kind()) {
            eprintln!("⚠ this copy could not describe itself ({e}); `datum sync chain rename <LABEL>` tries again");
        }
    };
    if starts {
        let genesis = json!({
            "type": "genesis", "chain": ulid::Ulid::new().to_string().to_lowercase(),
            "admins": [], "label": name, "created": chrono::Utc::now().to_rfc3339(),
        });
        db.sync_chain_write(genesis, true)?;
        describe();
        chain = db.sync_chain()?;
        eprintln!(
            "started sync chain {}; this copy is its first member",
            chain.id
        );
    }
    let (secret, expires_at) = datum_core::db::peers::invite_secret(expires);
    if print {
        println!("{}", json!({"secret": secret, "expires": expires_at}));
    }
    let record = json!({
        "type": "invite", "chain": chain.id,
        "invite_id": ulid::Ulid::new().to_string().to_lowercase(),
        "secret_hash": chain::secret_hash(&secret), "expires": expires_at,
        "caps": caps,
        "created": chrono::Utc::now().to_rfc3339(),
    });
    sign(db, record, &chain, admin_key)?;
    // In a chain it was in, only once the invite is signed: a refused
    // invite renames nothing.
    if !starts && label.is_some() {
        describe();
    }
    Ok((secret, expires_at))
}

/// How this copy names itself in the chain.
fn copy_label() -> String {
    #[cfg(feature = "net")]
    return datum_sync::label();
    #[cfg(not(feature = "net"))]
    "datum.day".into()
}

/// `label`, or why no member or admin may carry it — before anything is
/// signed; the chain refuses it too.
fn checked(label: String) -> Result<String> {
    match chain::label_problem(&label) {
        Some(why) => bail!(why),
        None => Ok(label),
    }
}

/// How output names a client (da-z99wr): the label of the chain member it
/// runs as, read now so a rename relabels every row it wrote, else `<kind>
/// <short id>`, whose kind this copy knows for its own client alone. An
/// entry written with `-M` has no client, and says so.
pub(crate) fn client_labeller(db: &Db) -> Result<impl Fn(&str) -> String> {
    let chain = db.sync_chain()?;
    let mine = crate::config::client_id().ok();
    Ok(move |id: &str| {
        if id.is_empty() {
            return "no client".to_string();
        }
        let member = chain
            .members
            .iter()
            .rev()
            .find(|m| m.client.as_deref() == Some(id));
        member.map(|m| m.label.clone()).unwrap_or_else(|| {
            let kind = if mine.as_deref() == Some(id) {
                kind()
            } else {
                "client"
            };
            format!("{kind} {}", id.chars().take(8).collect::<String>())
        })
    })
}

/// What this build is to its peers: the native CLI, or datum.day's, whose
/// wasm build has no `net` feature.
fn kind() -> &'static str {
    if cfg!(feature = "net") {
        "cli"
    } else {
        "web"
    }
}

/// `who` names `id`: as its label, or as at least [`chain::MIN_PREFIX`]
/// characters of it.
fn names(who: &str, label: &str, id: &str) -> bool {
    label == who || who.len() >= chain::MIN_PREFIX && id.starts_with(who)
}

/// What reverting `who` sets aside, in words (da-ry11c): a verb agrees with
/// its count, and a count of zero says nothing (da-sivjb).
fn revert_words(who: &str, endpoint: &str, p: &chain::PurgePreview) -> String {
    let minute = |t: &Option<String>| t.as_deref().map(|t| t.chars().take(16).collect::<String>());
    let id = &endpoint[..10];
    let many = |n: usize, one: &str, more: &str| match n {
        1 => format!("1 {one}"),
        n => format!("{n} {more}"),
    };
    let go = |n: usize| if n == 1 { "goes" } else { "go" };
    let defs = many(p.defined, "definition", "definitions");
    let (Some(first), Some(last)) = (minute(&p.first), minute(&p.last)) else {
        return match p.defined {
            0 => format!("{who} ({id}) signed no change to an entry: reverting it only revokes it"),
            n => format!(
                "{who} ({id}) signed no change to an entry, only {defs}: reverting it revokes \
                 it and sets {} aside",
                if n == 1 { "that" } else { "them" }
            ),
        };
    };
    let mut what = vec![];
    if p.created > 0 {
        let n = p.created;
        what.push(format!(
            "{} it created {}",
            many(n, "entry", "entries"),
            go(n)
        ));
    }
    let back: Vec<_> = [(p.edited, "edited"), (p.deleted, "deleted")]
        .into_iter()
        .filter(|&(n, _)| n > 0)
        .enumerate()
        .map(|(i, (n, did))| match i == 0 && what.is_empty() {
            true => format!("{} it {did}", many(n, "entry", "entries")),
            false => format!("{n} it {did}"),
        })
        .collect();
    if !back.is_empty() {
        let n = p.edited + p.deleted;
        what.push(format!(
            "{} {} back to the state without it",
            back.join(" and "),
            go(n)
        ));
    }
    if p.defined > 0 {
        what.push(format!("{defs} it signed {}", go(p.defined)));
    }
    format!(
        "{who} ({id}) — every op it signed, {first} to {last} UTC: {}; it is revoked",
        what.join(", ")
    )
}

/// The chain member `who` names, by label or a prefix of its endpoint id.
fn member(chain: &Chain, who: &str) -> Result<String> {
    let hits: Vec<_> = chain
        .members
        .iter()
        .filter(|m| names(who, &m.label, &m.endpoint))
        .collect();
    match hits.as_slice() {
        [m] => Ok(m.endpoint.clone()),
        _ => bail!("{who} names {} members of this chain", hits.len()),
    }
}

/// A passkey's admin key and label, from `{"alg", "publicKey", "label"}`.
fn passkey(json: &str) -> Result<(String, String)> {
    let v: Value = serde_json::from_str(json).context("the passkey is not JSON")?;
    let key = chain::passkey_admin_key(&v).map_err(anyhow::Error::msg)?;
    let label = checked(v["label"].as_str().unwrap_or("passkey").to_string())?;
    Ok((key, label))
}

/// A key file's admin label: its name, not its path, which every member
/// would otherwise hold.
fn file_label(path: &str) -> String {
    let name = std::path::Path::new(path).file_name();
    name.map_or(path.into(), |n| n.to_string_lossy().into_owned())
}

/// The Ed25519 secret in a key file; with `create`, a new one written there
/// first when there is none.
fn key_file(path: &str, create: bool) -> Result<[u8; 32]> {
    if create && !std::path::Path::new(path).exists() {
        let mut f = std::fs::OpenOptions::new();
        f.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut f, 0o600);
        std::io::Write::write_all(&mut f.open(path)?, chain::new_admin_secret().as_bytes())?;
        eprintln!("wrote a new admin key to {path}: whoever holds it administers the chain");
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("admin key {path}"))?;
    chain::read_admin_secret(&text).with_context(|| format!("{path} is not an admin key"))
}

/// Sign a chain record and write it: with the key file, or by sending it
/// to datum.day to be approved with a passkey and reading the approval code
/// back — or, at a terminal, an empty line once datum.day has written it
/// into a copy of this database: a sync brings it here (da-bjxts). An open
/// chain's record goes unsigned: the fold asks no `sig` of it while the
/// chain has no admin (`Chain::step`), and this copy's endpoint key signs
/// its op like any other.
fn sign(db: &Db, mut record: Value, chain: &Chain, admin_key: Option<&str>) -> Result<()> {
    if !chain.is_open() {
        let sig = match admin_key {
            Some(path) => chain::sign_ed25519(&record, &key_file(path, false)?),
            None => {
                eprintln!(
                    "Approve this with your passkey:\n\n  {}\n\nthen paste the approval code here, or press Enter once a browser holding this database has approved it:",
                    chain::approve_link(&record)
                );
                let mut code = String::new();
                std::io::stdin().read_line(&mut code)?;
                if code.trim().is_empty() && std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                    #[cfg(feature = "net")]
                    datum_sync::now_all(db)?;
                    ensure!(
                        db.sync_chain()?.holds(&record),
                        "not approved yet: no approval code, and no sync brought it"
                    );
                    return Ok(());
                }
                ensure!(!code.trim().is_empty(), "no approval code");
                code.trim().to_string()
            }
        };
        record["sig"] = sig.into();
    }
    Ok(db.sync_chain_write(record, true)?)
}

/// The chain's admins, as `chain list --json` and `admin list --json` have
/// them.
fn admins_json(chain: &Chain) -> Vec<Value> {
    chain
        .admins
        .iter()
        .map(|a| json!({"key": a.key, "label": a.label, "added": chain::hlc_time(&a.added)}))
        .collect()
}

/// The chain's admins, one line each, or what an open chain says.
fn print_admins(chain: &Chain) {
    if chain.is_open() {
        println!("  {OPEN}");
    }
    for a in &chain.admins {
        let kind = a.key.split(':').next().unwrap_or_default();
        let key: String = a.key.chars().take(24).collect();
        println!(
            "  {}  {kind}  {key}…  added {}",
            a.label,
            chain::hlc_time(&a.added)
        );
    }
}

/// `datum sync admin list`.
fn admins(db: &Db, json: bool) -> Result<()> {
    let chain = db.sync_chain()?;
    ensure!(chain.exists(), NO_CHAIN);
    if json {
        println!(
            "{}",
            json!({"chain": chain.id, "admins": admins_json(&chain)})
        );
        return Ok(());
    }
    println!("sync chain {}\nadmins:", chain.id);
    print_admins(&chain);
    Ok(())
}

/// A filter as `--json` has it: `{fields, of}`, or null.
fn filter_json(f: Option<&chain::Filter>) -> Value {
    f.map_or(Value::Null, |f| json!({"fields": f.fields, "of": f.of}))
}

/// `datum sync chain list`.
fn members(db: &Db, json: bool, show_revoked: bool) -> Result<()> {
    let chain = db.sync_chain()?;
    // A copy joining a chain is in none yet, and says through which copies
    // (da-idw54): datum.day reads this to resume the join.
    let through = db.sync_joining()?;
    if !chain.exists() && !through.is_empty() {
        match json {
            true => println!(
                "{}",
                json!({"chain": null, "admins": [], "members": [], "joining": through
                    .iter()
                    .map(|p| json!({"endpoint": p.id, "label": p.label}))
                    .collect::<Vec<_>>()})
            ),
            false => through
                .iter()
                .for_each(|p| println!("{}", joining(&p.label, &p.id))),
        }
        return Ok(());
    }
    ensure!(chain.exists(), NO_CHAIN);
    let mine = db.sync_endpoint_id()?;
    // What each runs: this copy's own, the others' as they last said (da-3zqyv).
    let mut versions = db.sync_versions()?;
    versions.insert(
        mine.clone(),
        (datum_core::build().into(), CURRENT_SCHEMA_VERSION),
    );
    if json {
        let admins = admins_json(&chain);
        let members: Vec<Value> = chain
            .members
            .iter()
            .map(|m| {
                json!({
                    "endpoint": m.endpoint, "label": m.label, "client": m.client,
                    "kind": m.kind, "admitted": chain::hlc_time(&m.admitted), "via": m.via,
                    "revoked": m.revoked.as_deref().map(chain::hlc_time),
                    "purged": m.purged.as_deref().map(chain::hlc_time),
                    "left": m.left.as_deref().map(chain::hlc_time),
                    "rejoined": m.rejoined.as_deref().map(chain::hlc_time),
                    "role": m.role().as_str(), "this": m.endpoint == mine,
                    "filter": filter_json(chain.filter(&m.endpoint)),
                    "datum": versions.get(&m.endpoint).map(|v| &v.0),
                    "schema": versions.get(&m.endpoint).map(|v| v.1),
                })
            })
            .collect();
        let out = json!({"chain": chain.id, "admins": admins, "members": members});
        println!("{out}");
        return Ok(());
    }
    println!("sync chain {}\nadmins:", chain.id);
    print_admins(&chain);
    println!("endpoints:");
    // This copy's own row stays even when revoked: it says why it is out.
    let (shown, hidden): (Vec<_>, Vec<_>) = chain
        .members
        .iter()
        .partition(|m| show_revoked || m.revoked.is_none() || m.endpoint == mine);
    for m in shown {
        let revoked = m.revoked.as_deref().map(chain::hlc_time);
        let rejoined = m.rejoined.as_deref().map(chain::hlc_time);
        println!(
            "  {}  {}{}  {}{}  admitted {}{} via {}{}{}{}",
            m.label,
            m.role().as_str(),
            chain
                .filter(&m.endpoint)
                .map(|f| format!(" ({})", f.describe()))
                .unwrap_or_default(),
            &m.endpoint[..10],
            m.client
                .as_deref()
                .map(|c| format!("  {} on {c}", m.kind.as_deref().unwrap_or("?")))
                .unwrap_or_default(),
            chain::hlc_time(&m.admitted),
            rejoined.map(|r| format!(", again {r}")).unwrap_or_default(),
            m.via,
            versions
                .get(&m.endpoint)
                .map(|(d, s)| format!("  datum {d} (schema {s})"))
                .unwrap_or_default(),
            match (m.purged.as_deref(), m.left.as_deref()) {
                (Some(p), _) => format!("  REVERTED {}", chain::hlc_time(p)),
                (_, Some(l)) => format!("  LEFT {}", chain::hlc_time(l)),
                _ => revoked
                    .map(|r| format!("  REVOKED {r}"))
                    .unwrap_or_default(),
            },
            if m.endpoint == mine {
                "  (this copy)"
            } else {
                ""
            },
        );
    }
    print_hidden(hidden.len());
    Ok(())
}

/// Tell the chain this copy leaves (da-dsob1): sign its departure, and
/// sync it to every member unless one holds it already. The members that
/// hold it; none, with no one to tell. Refused while none does: the copy
/// stays in its chain, its departure signed, until a sync delivers it.
/// `again` is the command that finishes it.
pub(crate) fn depart(db: &Db, chain: &str, again: &str) -> Result<Option<Vec<String>>> {
    let Some(told) = db.sync_depart()? else {
        return Ok(None);
    };
    // Signed, and held by no member yet: sync with every one, then ask
    // again. Each it reaches, or fails to, is a line of its own; a sync that
    // failed whole (no endpoint, say) is said with the refusal.
    #[cfg(feature = "net")]
    let (told, synced) = match told.is_empty() {
        true => {
            let synced = datum_sync::now_all(db);
            // The sessions bound this thread to the endpoint key they began
            // with (da-otz0d), and the leave mints a new one.
            db.sync_session_as(None);
            (db.sync_depart()?.unwrap_or_default(), synced)
        }
        false => (told, Ok(())),
    };
    #[cfg(not(feature = "net"))]
    let synced: Result<()> = Ok(());
    ensure!(
        !told.is_empty(),
        "no member holds this copy's departure from sync chain {chain} yet{}: it stays in the \
         chain, writing nothing, until a sync delivers it. `{again}` again once a member is \
         up, or `--force` to leave now, telling no one",
        synced
            .err()
            .map(|e| format!(" ({e:#})"))
            .unwrap_or_default()
    );
    Ok(Some(told))
}

/// The one line for the revoked members a human listing leaves out
/// (da-73gyz). A revert revokes too, and so does a leave (da-dsob1), so the
/// line says «former», which names all three. `status` counts only the two
/// it can show: a member that left is no copy to stand with, and has no row
/// there even with `--show-revoked`.
fn print_hidden(n: usize) {
    if n > 0 {
        let s = if n == 1 { "" } else { "s" };
        println!("  {n} former member{s} hidden — `--show-revoked`");
    }
}

/// `datum sync status`: per member, how this copy stands with it.
fn status(db: &Db, json: bool, show_revoked: bool) -> Result<()> {
    let chain = db.sync_chain()?;
    let mine = db.sync_endpoint_id()?;
    let label = chain
        .member(&mine)
        .map_or_else(copy_label, |m| m.label.clone());
    let peers = db.sync_status()?;
    let paused = db.sync_paused()?;
    let notices = db.sync_notices_unacked()?;
    let unlocks: Vec<_> = notices.iter().filter(|n| n.kind == "unlock").collect();
    let clock = db.sync_clock_waits()?;
    let upgrade = db.sync_upgrade_warnings()?;
    let unsent = db.sync_unsent()?;
    if json {
        let path = db.conn().path().unwrap_or_default();
        let name = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy());
        let members: Vec<Value> = peers
            .iter()
            .map(|p| {
                json!({
                    "endpoint": p.endpoint, "client": p.client, "label": p.label,
                    "kind": p.kind, "revoked": p.revoked, "purged": p.purged, "reader": p.reader,
                    "filter": filter_json(p.filter.as_ref()), "state": p.state,
                    "to_send": p.to_send, "last_seen": p.last_seen,
                    "last_verify": p.last_verify.as_ref().map(|(at, m)| json!({"at": at, "match": m})),
                    "error": p.error, "error_at": p.error_at, "addr": p.addr,
                })
            })
            .collect();
        let unlocked: Vec<Value> = unlocks
            .iter()
            .map(|n| json!({"by": n.label, "at": n.at}))
            .collect();
        let out = json!({
            "db": name, "chain": chain.exists().then_some(&chain.id), "paused": paused,
            "debug": db.debug_copy()?,
            "open": chain.exists() && chain.is_open(), "unlocked": unlocked,
            "this": {
                "endpoint": mine, "client": db.sync_client_id()?, "label": label,
                "datum": datum_core::build(), "schema": CURRENT_SCHEMA_VERSION,
                "reader": chain.member(&mine).is_some_and(|m| m.role() == chain::Role::Reader),
                "revoked": chain.member(&mine).and_then(|m| m.revoked.as_deref()).map(chain::hlc_time),
                "filter": filter_json(chain.filter(&mine)),
            },
            "members": members,
            "notices": notices.iter().map(|n| json!({
                "subject": n.subject, "kind": n.kind, "label": n.label, "at": n.at, "text": n.to_string(),
            })).collect::<Vec<_>>(),
            "clock": clock.iter().map(|w| json!({
                "endpoint": w.endpoint, "label": w.label, "ops": w.ops,
                "ahead_ms": w.ahead_ms, "text": w.to_string(),
            })).collect::<Vec<_>>(),
            "upgrade": upgrade, "unsent": unsent,
        });
        println!("{out}");
        return Ok(());
    }
    // Before the chain: the pipe syncs copies in none.
    for w in &clock {
        println!("⚠ {w}");
    }
    if let Err(why) = db.sync_refuse_debug() {
        println!("{why}");
        return Ok(());
    }
    if !chain.exists() {
        match peers.is_empty() {
            true => println!("{NO_CHAIN}"),
            false => peers
                .iter()
                .for_each(|p| println!("{}", joining(&p.label, &p.endpoint))),
        }
        return Ok(());
    }
    if let Some(at) = &paused {
        println!("⚠ paused since {at}: this copy neither sends nor receives — `datum sync resume`");
    }
    println!(
        "sync chain {} · this copy {label} ({})",
        chain.id,
        &mine[..10]
    );
    // An empty field here reads as «not shared», not «nothing logged».
    if let Some(f) = chain.filter(&mine) {
        let fields: Vec<&str> = f.fields.iter().map(String::as_str).collect();
        println!("a reader, {}: {}", f.describe(), fields.join(", "));
    }
    match chain.member(&mine).map(|m| (&m.left, &m.revoked)) {
        Some((Some(at), _)) => println!(
            "⚠ this copy signed its departure at {}: `datum sync chain leave` finishes once a \
             member holds it",
            chain::hlc_time(at)
        ),
        Some((_, Some(at))) => println!(
            "⚠ this copy was revoked at {}: it syncs again once a member's invite admits it, \
             or `datum sync chain leave` keeps every entry and starts afresh",
            chain::hlc_time(at)
        ),
        _ => {}
    }
    if chain.is_open() {
        println!("{OPEN}");
    }
    #[cfg(feature = "net")]
    if super::sync_daemon::missing() {
        println!(
            "⚠ no sync daemon: this copy syncs only when `datum sync` is run — \
             `datum sync daemon install` keeps it syncing in the background"
        );
    }
    // What `datum check` shows of sync too, from the same source (da-nq07y).
    for w in upgrade.iter().chain(&unsent) {
        println!("⚠ {w}");
    }
    for n in &notices {
        println!("{}", notice_line(n));
    }
    let (shown, hidden): (Vec<_>, Vec<_>) = peers.iter().partition(|p| show_revoked || !p.revoked);
    let width = shown
        .iter()
        .map(|p| p.label.chars().count())
        .max()
        .unwrap_or(0);
    for p in shown {
        let when = match p.state {
            "error" => p.error_at.clone(),
            "never" => None,
            _ => p
                .last_verify
                .as_ref()
                .map(|(at, _)| at.clone())
                .or(p.last_seen.clone()),
        };
        println!(
            "  {:<width$}  {}  {:<7}  to send {:<5}  {}{}{}{}{}",
            p.label,
            &p.endpoint[..10],
            p.state,
            p.to_send,
            when.as_deref().unwrap_or("-"),
            match (p.purged, p.revoked) {
                (true, _) => "  REVERTED",
                (_, true) => "  REVOKED",
                _ => "",
            },
            if p.reader { "  reader" } else { "" },
            p.filter
                .as_ref()
                .map(|f| format!(" ({})", f.describe()))
                .unwrap_or_default(),
            p.error
                .clone()
                .or_else(|| p.no_address(&peers))
                .map(|e| format!("  {e}"))
                .unwrap_or_default(),
        );
    }
    print_hidden(hidden.len());
    Ok(())
}

/// The warnings (da-odwc7): once, on the first command after the chain
/// admits an endpoint or an admin, since each can read and write
/// everything, or is unlocked. Each also stays in `datum check` and
/// `datum sync status` until `datum sync chain ack`. `before` is what was unannounced when the
/// command began, so a session never tells its own admissions to nobody,
/// and this runs after the command's output, which would push the warning
/// off the screen: a command that died first leaves it to the next
/// (da-36xnc).
pub fn warn_new_members(db: &Db, before: &[Notice]) {
    if before.is_empty() {
        return;
    }
    for n in db.sync_notices_unannounced().unwrap_or_default() {
        if !before.iter().any(|b| b.subject == n.subject) {
            continue;
        }
        eprintln!("{}", notice_line(&n));
        let _ = db.sync_notice_announced(&n);
    }
}

/// A notice as a warning, with what clears it.
fn notice_line(n: &Notice) -> String {
    match n.kind.as_str() {
        "unlock" => format!("⚠ {n} — {UNLOCK_HINT}"),
        _ => format!(
            "⚠ {n} — `datum sync chain list` to review, `datum sync chain ack {}` to acknowledge",
            n.who()
        ),
    }
}

/// What an unlock notice asks of the person reading it.
const UNLOCK_HINT: &str =
    "other devices may have relied on the lock; `datum sync chain ack unlock` to acknowledge";

/// The sync chain's new endpoints and admins (da-odwc7), and its unlocks
/// (da-aam46), until `datum sync chain ack` says they are known; a member's
/// newer schema; and changes no other device has. `sync status` shows each
/// from the same source (da-nq07y).
pub fn notice_issues(db: &Db) -> Result<Vec<Issue>> {
    let (unlocks, new): (Vec<_>, Vec<_>) = db
        .sync_notices_unacked()?
        .into_iter()
        .partition(|n| n.kind == "unlock");
    let issue = |category, head: String, notices: &[Notice]| Issue {
        domain: Domain::Sync,
        severity: Severity::Warning,
        category,
        message: format!(
            "{head}:\n{}",
            notices
                .iter()
                .map(|n| format!("  ⚠ {n}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        count: notices.len(),
        example: None,
        entries: Vec::new(),
        rendered_block: String::new(),
    };
    let mut out = Vec::new();
    if !new.is_empty() {
        let head = format!(
            "{} new to the sync chain; `datum sync chain ack <id>` once you know {}",
            new.len(),
            if new.len() == 1 { "it" } else { "them" },
        );
        out.push(issue("sync_new_member", head, &new));
    }
    if !unlocks.is_empty() {
        let head = format!("the sync chain was unlocked: {UNLOCK_HINT}");
        out.push(issue("sync_unlocked", head, &unlocks));
    }
    // A member runs a newer schema than this copy (da-3zqyv), and changes
    // made here no other device has (da-u10rl).
    let upgrade = db.sync_upgrade_warnings()?.into_iter();
    let unsent = db.sync_unsent()?.into_iter();
    let warnings = upgrade.map(|w| ("sync_upgrade", w));
    for (category, w) in warnings.chain(unsent.map(|w| ("sync_unsent", w))) {
        out.push(Issue {
            domain: Domain::Sync,
            severity: Severity::Warning,
            category,
            message: format!("⚠ {w}"),
            count: 1,
            example: None,
            entries: Vec::new(),
            rendered_block: String::new(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session's endpoint binds its command alone (da-otz0d): the next on
    /// this thread, a REPL's next line, gives the copy a new key.
    #[test]
    fn a_sessions_endpoint_is_bound_for_its_command_alone() {
        let db = Db::open_in_memory().unwrap();
        let id = db.sync_endpoint_id().unwrap();
        let vector = SyncCommand::As(id.clone(), Box::new(SyncCommand::Vector));
        sync(vector, &db).unwrap();
        assert_ne!(db.sync_rekey().unwrap(), id);
    }

    /// A verb agrees with its count, a count of zero says nothing, and the
    /// definitions a device signed beside its entries are named (da-sivjb).
    #[test]
    fn revert_words_agree_with_their_counts() {
        let words = |created, edited, deleted, defined| {
            let at = Some("2026-10-07T10:00:00Z".to_string());
            let p = chain::PurgePreview {
                created,
                edited,
                deleted,
                defined,
                first: at.clone(),
                last: at,
            };
            let w = revert_words("b", "0123456789abcdef", &p);
            w[w.find("UTC: ").unwrap() + 5..].to_string()
        };
        let back = "back to the state without it; it is revoked";
        assert_eq!(
            words(1, 1, 1, 0),
            format!("1 entry it created goes, 1 it edited and 1 it deleted go {back}")
        );
        assert_eq!(words(2, 0, 0, 0), "2 entries it created go; it is revoked");
        assert_eq!(words(0, 1, 0, 0), format!("1 entry it edited goes {back}"));
        assert_eq!(words(0, 0, 3, 0), format!("3 entries it deleted go {back}"));
        assert_eq!(
            words(1, 0, 1, 0),
            format!("1 entry it created goes, 1 it deleted goes {back}")
        );
        assert_eq!(
            words(1, 0, 0, 2),
            "1 entry it created goes, 2 definitions it signed go; it is revoked"
        );
        assert_eq!(
            words(0, 1, 0, 1),
            format!("1 entry it edited goes back to the state without it, 1 definition it signed goes; it is revoked")
        );
    }
}
