//! `datum sync` — Easy Sync. The merge is `datum_core::db::sync` (da-k329q),
//! and here is the pipe it runs over: ops as JSON lines on stdout (`ops`)
//! and stdin (`apply`). Pairing and syncing over iroh is `datum_sync`
//! (da-rugg8), behind the `net` feature. The sync chain's admin acts
//! (da-odwc7) are signed here, with a key file or by a passkey on datum.day.

use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{bail, ensure, Context, Result};
use datum_core::db::chain::{self, Chain};
use datum_core::db::Db;
use serde_json::{json, Value};

use crate::args::{ShowFlag, SyncAdminAct, SyncCommand};
use crate::commands::check::{Issue, Severity};

const NO_CHAIN: &str = "this copy is in no sync chain: `datum sync init` starts one";

pub fn sync(cmd: SyncCommand, db: &Db) -> Result<()> {
    match cmd {
        SyncCommand::Vector => {
            println!("{}", serde_json::to_string(&db.sync_vector()?)?);
        }
        SyncCommand::Ops { since, overlap } => {
            let since: BTreeMap<String, i64> = match since {
                Some(v) => serde_json::from_str(&v)
                    .with_context(|| format!("--since is not a version vector: {v}"))?,
                None => BTreeMap::new(),
            };
            for line in db.sync_ops_since(&since, overlap)? {
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
            println!("peers paired with the old key must pair with this copy again");
        }
        SyncCommand::Verify { verbose } => {
            let (hash, per_column) = db.sync_state_hash()?;
            println!("{hash}");
            if verbose {
                for (column, hash) in per_column {
                    println!("  {column:<24} {hash}");
                }
            }
        }
        SyncCommand::Init {
            admin_key,
            admin_passkey,
        } => {
            ensure!(
                !db.sync_chain()?.exists(),
                "this copy is in a sync chain already"
            );
            let (key, label) = match (&admin_key, &admin_passkey) {
                (Some(path), _) => (
                    chain::ed25519_admin_key(&key_file(path, true)?),
                    checked(file_label(path))?,
                ),
                (_, Some(json)) => passkey(json)?,
                _ => bail!("sync init needs --admin-key or --admin-passkey"),
            };
            let id = ulid::Ulid::new().to_string().to_lowercase();
            let genesis = json!({
                "type": "genesis", "chain": id, "admins": [{"key": key, "label": label}],
                "label": copy_label(), "created": chrono::Utc::now().to_rfc3339(),
            });
            db.sync_chain_write(sign(genesis, admin_key.as_deref())?, true)?;
            // The chain stands without it: a failure here is no reason to say
            // the init failed and leave its genesis behind.
            if let Err(e) = db.sync_describe_self(&copy_label(), client()) {
                eprintln!("⚠ this copy could not describe itself ({e}); `datum sync rename <LABEL>` tries again");
            }
            println!("started sync chain {id}; this copy is its first member");
        }
        SyncCommand::Admin {
            act,
            who,
            label,
            admin_key,
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
                SyncAdminAct::AddEndpoint => {
                    let label = checked(label.unwrap_or_else(|| who.chars().take(10).collect()))?;
                    let r = json!({"type": "add_endpoint", "endpoint": who, "label": label});
                    (r, format!("admitted {label}"))
                }
                SyncAdminAct::AddAdmin => {
                    let (key, l) = match who.trim_start().starts_with('{') {
                        true => passkey(&who)?,
                        false => (
                            chain::ed25519_admin_key(&key_file(&who, false)?),
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
                    let r = json!({"type": "remove_admin", "key": a.key});
                    (r, format!("removed admin {}", a.label))
                }
            };
            record["chain"] = chain.id.clone().into();
            record["created"] = chrono::Utc::now().to_rfc3339().into();
            db.sync_chain_write(sign(record, admin_key.as_deref())?, true)?;
            println!("{said}");
        }
        SyncCommand::Members { show } => members(db, show == ShowFlag::Json)?,
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
                db.sync_describe_self(&label, m.client.as_deref().unwrap_or(client()))?;
            } else {
                let record = json!({
                    "type": "describe", "chain": chain.id, "endpoint": target,
                    "device": m.device, "label": label, "client": m.client,
                    "created": chrono::Utc::now().to_rfc3339(),
                });
                db.sync_chain_write(sign(record, admin_key.as_deref())?, true)?;
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
            dialed,
        } => {
            let added = db.sync_handshake(&endpoint, &label, &addr, invite.as_deref(), dialed)?;
            println!("{}", if added { "added" } else { "known" });
        }
        SyncCommand::NewInvite { expires_secs } => {
            ensure!(
                !db.sync_chain()?.exists(),
                "this copy is in a sync chain: `datum sync invite` makes an invite"
            );
            let expires = std::time::Duration::from_secs(expires_secs);
            let (secret, expires_at) = db.sync_new_invite(expires)?;
            println!("{}", json!({"secret": secret, "expires": expires_at}));
        }
        SyncCommand::Peers { show } => {
            let peers = db.sync_peers()?;
            if show == ShowFlag::Json {
                let peers: Vec<Value> = peers
                    .iter()
                    .map(|p| {
                        json!({
                            "endpoint": p.id, "label": p.label, "addr": p.addrs,
                            "added_at": p.added_at, "last_seen": p.last_seen, "synced": p.synced,
                        })
                    })
                    .collect();
                println!("{}", Value::from(peers));
                return Ok(());
            }
            if peers.is_empty() {
                println!("no sync peers");
            }
            for p in peers {
                println!(
                    "{}  {}  added {}  last seen {}",
                    p.label,
                    &p.id[..p.id.len().min(10)],
                    p.added_at,
                    p.last_seen.as_deref().unwrap_or("never")
                );
            }
        }
        SyncCommand::Forget { peer } => {
            let (label, id) = db.sync_forget(&peer)?;
            println!("forgot sync peer {label} ({})", &id[..id.len().min(10)]);
        }
        SyncCommand::Ack { who } => {
            ensure!(db.sync_ack(&who)? > 0, "nothing to acknowledge for {who}");
            println!("acknowledged {who}");
        }
        #[cfg(feature = "net")]
        SyncCommand::Invite {
            qr,
            expires_secs,
            admin_key,
        } => {
            let chain = db.sync_chain()?;
            ensure!(chain.exists(), NO_CHAIN);
            let expires = std::time::Duration::from_secs(expires_secs);
            let (secret, expires_at) = datum_core::db::peers::invite_secret(expires);
            let invite = json!({
                "type": "invite", "chain": chain.id,
                "invite_id": ulid::Ulid::new().to_string().to_lowercase(),
                "secret_hash": chain::secret_hash(&secret), "expires": expires_at, "caps": {},
                "created": chrono::Utc::now().to_rfc3339(),
            });
            db.sync_chain_write(sign(invite, admin_key.as_deref())?, true)?;
            datum_sync::offer(db, &secret, expires, &expires_at, qr)?
        }
        #[cfg(feature = "net")]
        SyncCommand::AddClient { qr, expires_secs } => {
            datum_sync::add_client(db, std::time::Duration::from_secs(expires_secs), qr)?
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
            datum_sync::connect(db, &ticket)?
        }
        #[cfg(feature = "net")]
        SyncCommand::Now => datum_sync::now_all(db)?,
        #[cfg(feature = "net")]
        SyncCommand::Run => datum_sync::run(db)?,
        #[cfg(not(feature = "net"))]
        _ => anyhow::bail!("peer-to-peer sync is not in this build (no networking)"),
    }
    Ok(())
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

/// What this build is to its peers: the native CLI, or datum.day's, whose
/// wasm build has no `net` feature.
fn client() -> &'static str {
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

/// Sign a chain record: with the key file, or by sending it to datum.day to
/// be approved with a passkey and reading the approval code back.
fn sign(mut record: Value, admin_key: Option<&str>) -> Result<Value> {
    let sig = match admin_key {
        Some(path) => chain::sign_ed25519(&record, &key_file(path, false)?),
        None => {
            eprintln!(
                "Approve this with your passkey:\n\n  {}\n\nthen paste the approval code here:",
                chain::approve_link(&record)
            );
            let mut code = String::new();
            std::io::stdin().read_line(&mut code)?;
            ensure!(!code.trim().is_empty(), "no approval code");
            code.trim().to_string()
        }
    };
    record["sig"] = sig.into();
    Ok(record)
}

/// `datum sync members`.
fn members(db: &Db, json: bool) -> Result<()> {
    let chain = db.sync_chain()?;
    ensure!(chain.exists(), NO_CHAIN);
    let mine = db.sync_endpoint_id()?;
    if json {
        let admins: Vec<Value> = chain
            .admins
            .iter()
            .map(|a| json!({"key": a.key, "label": a.label, "added": chain::hlc_time(&a.added)}))
            .collect();
        let members: Vec<Value> = chain
            .members
            .iter()
            .map(|m| {
                json!({
                    "endpoint": m.endpoint, "label": m.label, "device": m.device,
                    "client": m.client, "admitted": chain::hlc_time(&m.admitted), "via": m.via,
                    "revoked": m.revoked.as_deref().map(chain::hlc_time),
                    "this": m.endpoint == mine,
                })
            })
            .collect();
        let out = json!({"chain": chain.id, "admins": admins, "members": members});
        println!("{out}");
        return Ok(());
    }
    println!("sync chain {}\nadmins:", chain.id);
    for a in &chain.admins {
        let kind = a.key.split(':').next().unwrap_or_default();
        let key: String = a.key.chars().take(24).collect();
        println!(
            "  {}  {kind}  {key}…  added {}",
            a.label,
            chain::hlc_time(&a.added)
        );
    }
    println!("endpoints:");
    for m in &chain.members {
        let revoked = m.revoked.as_deref().map(chain::hlc_time);
        println!(
            "  {}  {}{}  admitted {} via {}{}{}",
            m.label,
            &m.endpoint[..10],
            m.device
                .as_deref()
                .map(|d| format!("  {} on {d}", m.client.as_deref().unwrap_or("?")))
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
    if json {
        let path = db.conn().path().unwrap_or_default();
        let name = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy());
        let members: Vec<Value> = peers
            .iter()
            .map(|p| {
                json!({
                    "endpoint": p.endpoint, "device": p.device, "label": p.label,
                    "client": p.client, "revoked": p.revoked, "state": p.state,
                    "to_send": p.to_send, "last_seen": p.last_seen,
                    "last_verify": p.last_verify.as_ref().map(|(at, m)| json!({"at": at, "match": m})),
                    "error": p.error, "error_at": p.error_at,
                })
            })
            .collect();
        let out = json!({
            "db": name, "chain": chain.exists().then_some(&chain.id),
            "this": {"endpoint": mine, "device": db.sync_device_id()?, "label": label},
            "members": members,
        });
        println!("{out}");
        return Ok(());
    }
    if !chain.exists() {
        println!("{NO_CHAIN}");
        return Ok(());
    }
    println!(
        "sync chain {} · this copy {label} ({})",
        chain.id,
        &mine[..10]
    );
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
            "  {:<width$}  {}  {:<7}  to send {:<5}  {}{}{}",
            p.label,
            &p.endpoint[..10],
            p.state,
            p.to_send,
            when.as_deref().unwrap_or("-"),
            if p.revoked { "  REVOKED" } else { "" },
            p.error
                .as_deref()
                .map(|e| format!("  {e}"))
                .unwrap_or_default(),
        );
    }
    Ok(())
}

/// The warnings (da-rugg8, da-odwc7): once, on the first command after a
/// peer is added or the chain admits an endpoint or an admin, since each can
/// read and write everything. A chain's line also stays in `datum check`
/// until `datum sync ack`.
pub fn warn_new_peers(db: &Db) {
    for n in db.sync_notices_unannounced().unwrap_or_default() {
        eprintln!("⚠ {n} — datum sync members to review, datum sync ack to acknowledge");
    }
    let Ok(mut stmt) = db.conn().prepare(
        "SELECT label, endpoint_id, added_at FROM sync_peers
         WHERE announced = 0 AND endpoint_id NOT IN (SELECT subject FROM sync_notices)",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    }) else {
        return;
    };
    let mut any = false;
    for (label, id, added) in rows.flatten() {
        any = true;
        eprintln!(
            "⚠ new sync peer {label} ({}) added {added} — datum sync peers to review",
            &id[..id.len().min(10)]
        );
    }
    if any {
        let _ = db.conn().execute(
            "UPDATE sync_peers SET announced = 1 WHERE announced = 0",
            [],
        );
    }
}

/// The sync chain's new endpoints and admins (da-odwc7), until `datum sync
/// ack` says they are known.
pub fn notice_issues(db: &Db) -> Result<Vec<Issue>> {
    let notices = db.sync_notices_unacked()?;
    if notices.is_empty() {
        return Ok(Vec::new());
    }
    let lines: Vec<String> = notices.iter().map(|n| format!("  ⚠ {n}")).collect();
    Ok(vec![Issue {
        severity: Severity::Warning,
        category: "sync_new_member",
        message: format!(
            "{} new to the sync chain; `datum sync ack <id>` once you know {}:\n{}",
            notices.len(),
            if notices.len() == 1 { "it" } else { "them" },
            lines.join("\n"),
        ),
        count: notices.len(),
        example: None,
        entries: Vec::new(),
        rendered_block: String::new(),
    }])
}
