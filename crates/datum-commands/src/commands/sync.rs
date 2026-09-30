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
use serde_json::{json, Value};

use crate::args::{ShowFlag, SyncAdminAct, SyncCommand, SyncFilter};
use crate::commands::check::{Issue, Severity};

const NO_CHAIN: &str = "this copy is in no sync chain: `datum sync chain invite` on this device starts one, or `datum sync chain connect <TICKET>` joins one with a ticket from another";

/// What an open chain says beside its members.
const OPEN: &str =
    "no admin: any device can add devices — `datum sync admin add` to protect the chain";

pub fn sync(cmd: SyncCommand, db: &Db) -> Result<()> {
    match cmd {
        SyncCommand::Now => {
            if !db.sync_chain()?.exists() {
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
        SyncCommand::Members { show } => members(db, show == ShowFlag::Json)?,
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
        SyncCommand::Status { show } => status(db, show == ShowFlag::Json)?,
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
        SyncCommand::RecordSession {
            endpoint,
            vector,
            verify,
            error,
        } => {
            let vector: Option<BTreeMap<String, i64>> = vector
                .map(|v| serde_json::from_str(&v))
                .transpose()
                .context("--vector is not a version vector")?;
            db.sync_record_session(&endpoint, vector.as_ref(), verify, error.as_deref())?;
        }
        SyncCommand::Handshake {
            endpoint,
            label,
            addr,
            invite,
            chain,
            dialed,
        } => {
            let (invite, chain) = (invite.as_deref(), chain.as_deref());
            let added = db.sync_handshake(&endpoint, &label, &addr, invite, chain, dialed)?;
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
            invite(db, expires, admin_key.as_deref(), caps, true)?;
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
        SyncCommand::Ack { who } => {
            ensure!(db.sync_ack(&who)? > 0, "nothing to acknowledge for {who}");
            println!("acknowledged {who}");
        }
        #[cfg(feature = "net")]
        SyncCommand::Invite {
            reader,
            filter,
            qr,
            expires_secs,
            admin_key,
        } => {
            let expires = std::time::Duration::from_secs(expires_secs);
            let starts = !db.sync_chain()?.exists();
            let caps = caps(db, reader, filter)?;
            if let Some(f) = caps.get("fields") {
                eprintln!("the redeemer is a reader sent only {f}, each with its spec");
            }
            let (secret, expires_at) = invite(db, expires, admin_key.as_deref(), caps, false)?;
            if starts {
                super::sync_daemon::auto_install();
            }
            datum_sync::offer(db, &secret, expires, &expires_at, qr)?
        }
        #[cfg(feature = "net")]
        SyncCommand::Connect { ticket } => {
            let ticket = match ticket {
                Some(t) => t,
                None => {
                    let mut t = String::new();
                    std::io::stdin().read_to_string(&mut t)?;
                    t
                }
            };
            let joins = !db.sync_chain()?.exists();
            datum_sync::connect(db, &ticket)?;
            if joins && db.sync_chain()?.exists() {
                super::sync_daemon::auto_install();
            }
        }
        #[cfg(feature = "net")]
        SyncCommand::Run => datum_sync::run(db)?,
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
/// not. `print`: say the secret on
/// stdout before the record is signed, so a run that stops at an approve
/// link has handed it over (datum.day signs with `sync approve`, da-bjxts).
fn invite(
    db: &Db,
    expires: std::time::Duration,
    admin_key: Option<&str>,
    caps: Value,
    print: bool,
) -> Result<(String, String)> {
    let mut chain = db.sync_chain()?;
    if !chain.exists() {
        let genesis = json!({
            "type": "genesis", "chain": ulid::Ulid::new().to_string().to_lowercase(),
            "admins": [], "label": copy_label(), "created": chrono::Utc::now().to_rfc3339(),
        });
        db.sync_chain_write(genesis, true)?;
        // The chain stands without it: a failure here is no reason to fail.
        if let Err(e) = db.sync_describe_self(&copy_label(), kind()) {
            eprintln!("⚠ this copy could not describe itself ({e}); `datum sync chain rename <LABEL>` tries again");
        }
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
fn members(db: &Db, json: bool) -> Result<()> {
    let chain = db.sync_chain()?;
    ensure!(chain.exists(), NO_CHAIN);
    let mine = db.sync_endpoint_id()?;
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
                    "role": m.role().as_str(), "this": m.endpoint == mine,
                    "filter": filter_json(chain.filter(&m.endpoint)),
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
    for m in &chain.members {
        let revoked = m.revoked.as_deref().map(chain::hlc_time);
        println!(
            "  {}  {}{}  {}{}  admitted {} via {}{}{}",
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
            m.via,
            revoked
                .map(|r| format!("  REVOKED {r}"))
                .unwrap_or_default(),
            if m.endpoint == mine {
                "  (this copy)"
            } else {
                ""
            },
        );
    }
    Ok(())
}

/// `datum sync status`: per member, how this copy stands with it.
fn status(db: &Db, json: bool) -> Result<()> {
    let chain = db.sync_chain()?;
    let mine = db.sync_endpoint_id()?;
    let label = chain
        .member(&mine)
        .map_or_else(copy_label, |m| m.label.clone());
    let peers = db.sync_status()?;
    let paused = db.sync_paused()?;
    let notices = db.sync_notices_unacked()?;
    let unlocks: Vec<_> = notices.iter().filter(|n| n.kind == "unlock").collect();
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
                    "kind": p.kind, "revoked": p.revoked, "reader": p.reader,
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
            "open": chain.exists() && chain.is_open(), "unlocked": unlocked,
            "this": {
                "endpoint": mine, "client": db.sync_client_id()?, "label": label,
                "reader": chain.member(&mine).is_some_and(|m| m.role() == chain::Role::Reader),
                "filter": filter_json(chain.filter(&mine)),
            },
            "members": members,
            "notices": notices.iter().map(|n| json!({
                "subject": n.subject, "kind": n.kind, "label": n.label, "at": n.at, "text": n.to_string(),
            })).collect::<Vec<_>>(),
        });
        println!("{out}");
        return Ok(());
    }
    if !chain.exists() {
        println!("{NO_CHAIN}");
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
    if chain.is_open() {
        println!("{OPEN}");
    }
    for n in &unlocks {
        println!("⚠ {n} — {UNLOCK_HINT}");
    }
    let width = peers
        .iter()
        .map(|p| p.label.chars().count())
        .max()
        .unwrap_or(0);
    for p in &peers {
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
            if p.revoked { "  REVOKED" } else { "" },
            if p.reader { "  reader" } else { "" },
            p.filter
                .as_ref()
                .map(|f| format!(" ({})", f.describe()))
                .unwrap_or_default(),
            p.error
                .as_deref()
                .map(|e| format!("  {e}"))
                .unwrap_or_default(),
        );
    }
    Ok(())
}

/// The warnings (da-odwc7): once, on the first command after the chain
/// admits an endpoint or an admin, since each can read and write
/// everything, or is unlocked. Each also stays in `datum check` until
/// `datum sync chain ack`.
pub fn warn_new_members(db: &Db) {
    for n in db.sync_notices_unannounced().unwrap_or_default() {
        match n.kind.as_str() {
            "unlock" => eprintln!("⚠ {n} — {UNLOCK_HINT}"),
            _ => eprintln!(
                "⚠ {n} — `datum sync chain list` to review, `datum sync chain ack` to acknowledge"
            ),
        }
    }
}

/// What an unlock notice asks of the person reading it.
const UNLOCK_HINT: &str =
    "other devices may have relied on the lock; `datum sync chain ack unlock` to acknowledge";

/// The sync chain's new endpoints and admins (da-odwc7), and its unlocks
/// (da-aam46), until `datum sync chain ack` says they are known.
pub fn notice_issues(db: &Db) -> Result<Vec<Issue>> {
    let (unlocks, new): (Vec<_>, Vec<_>) = db
        .sync_notices_unacked()?
        .into_iter()
        .partition(|n| n.kind == "unlock");
    let issue = |category, head: String, notices: &[Notice]| Issue {
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
    Ok(out)
}
