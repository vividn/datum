//! An agent's code (da-bdyor): `datum sync chain agent` makes a device key
//! and a standing admission for it (da-zkwiu), and prints the code that
//! holds the key, the chain and where to dial; `datum sync restore <code>`,
//! in a sandbox with nothing kept, makes a fresh copy of that device, which
//! a member admits by its own claim (da-tkbtu), retiring the copy before.

use anyhow::{bail, ensure, Context, Result};
use datum_core::db::chain::{self, Role};
use datum_core::db::Db;
use serde_json::json;

use super::sync::{caps, checked, local, sign_all, NO_CHAIN};
use crate::args::SyncFilter;

/// What a restored copy's last line says, since its sandbox keeps nothing.
const AFTER: &str =
    "Run `datum sync` after writing: an entry not yet synced is lost with this sandbox";

/// Why an open chain makes no agent a member (ístina's F19), nor
/// promotes one to it (da-ebxg5).
pub(super) const AT_MOST_A_WRITER: &str = "in a chain with no admin, an agent can be at most a \
    writer: lock the chain first (`datum sync admin add …`, or sign in on datum.day)";

/// `datum sync chain agent`: the standing record, signed as an admin act,
/// and the agent's code; `--json` says it for datum.day, which makes the
/// code itself from the secret, its CLI having no network.
#[allow(clippy::too_many_arguments)]
pub(super) fn agent(
    db: &Db,
    label: String,
    level: &str,
    role: Role,
    filter: Option<SyncFilter>,
    expires_secs: Option<u64>,
    admin_key: Option<&str>,
    json_out: bool,
) -> Result<()> {
    let chain = db.sync_chain()?;
    ensure!(chain.exists() && chain.dissolved().is_none(), NO_CHAIN);
    // An open chain lets a member write its first `add_admin`, which locks
    // it: a member agent could lock the person out (ístina's F19).
    ensure!(
        !(chain.is_open() && role == Role::Member),
        "{AT_MOST_A_WRITER}, or give it `--level write` / `light --write`"
    );
    let label = checked(label.trim().into())?;
    ensure!(
        !label.is_empty(),
        "--label names the agent: it cannot be blank"
    );
    let filtered = filter.is_some();
    let caps = caps(db, role, filter)?;
    let secret = chain::new_admin_secret();
    let device = chain::read_admin_secret(&secret).context("a new device key")?;
    let device = chain::bind_device(&device, "", "").0;
    let expires = expires_secs.map(|s| chrono::Utc::now() + chrono::Duration::seconds(s as i64));
    // A standing admission makes its device an agent, never an admin
    // (`Chain.agents`, `Chain::agent_refuses`): one offered for a person's own
    // device would inherit that rule.
    let record = json!({
        "type": "standing", "chain": chain.id, "device": device, "caps": caps,
        "label": label, "expires": expires.map(|e| e.to_rfc3339()),
        "created": chrono::Utc::now().to_rfc3339(),
    });
    let fields: Option<Vec<String>> = caps.get("fields").map(|f| {
        let f = f.as_array().into_iter().flatten();
        f.filter_map(|f| f.as_str().map(Into::into)).collect()
    });
    let until = expires.map(|e| e.timestamp() as u64);
    #[cfg(feature = "net")]
    let code = Some(datum_sync::agent_code(
        db,
        &secret,
        &label,
        role.as_str(),
        fields.clone(),
        until,
    )?);
    #[cfg(not(feature = "net"))]
    let code: Option<String> = None;
    if json_out {
        let out = json!({
            "secret": secret, "device": device, "agent": label, "level": level,
            "role": role.as_str(), "fields": fields, "code": code,
            "expires": expires.map(|e| e.to_rfc3339()),
        });
        println!("{out}");
    }
    // Said first, as `sync new-invite` says its secret: datum.day signs a
    // locked chain's record by `sync approve`, from the approve link. An
    // approval refused leaves a secret no standing record names, which
    // admits no one.
    sign_all(db, vec![record], &chain, admin_key)?;
    if json_out {
        return Ok(());
    }
    let what = match chain::Filter::from_json(&caps).filter(|_| filtered) {
        Some(f) => format!("a {}, {}", role.as_str(), f.describe()),
        None => format!("a {}", role.as_str()),
    };
    println!("agent {label} ({level}): {what}");
    println!("Its code makes a fresh copy of it each time, retiring the copy before.");
    match expires {
        Some(e) => println!(
            "Anyone with this code is this agent until it expires or you remove it.\n\
             It expires {}; `datum sync chain expel {label}` removes it. The code:",
            local(&e.to_rfc3339())
        ),
        None => println!(
            "Anyone with this code is this agent until you remove it: it never expires \
             (--expires never).\n`datum sync chain expel {label}` removes it. The code:"
        ),
    }
    println!();
    println!(
        "{}",
        code.as_deref()
            .unwrap_or("(this build has no network, so no code)")
    );
    println!();
    println!("On the agent, with nothing kept: `datum sync restore '<CODE>'`, or DATUM_AGENT_CODE=<CODE> and `datum sync restore`");
    Ok(())
}

/// `datum sync restore [<CODE>]`: this install becomes the code's device,
/// and this database a fresh copy of the agent, joined by its standing
/// admission and synced at its level. A copy restored already syncs.
#[cfg(feature = "net")]
pub(super) fn restore(db: &Db, code: Option<String>) -> Result<()> {
    let env = std::env::var("DATUM_AGENT_CODE").ok();
    let code = code
        .or(env.filter(|c| !c.trim().is_empty()))
        .context("no code: `datum sync restore <CODE>`, or DATUM_AGENT_CODE in the env")?;
    let a = datum_sync::Agent::decode(&code)?;
    if let Some(at) = a
        .expires
        .filter(|at| *at as i64 <= chrono::Utc::now().timestamp())
    {
        let at = chrono::DateTime::from_timestamp(at as i64, 0).unwrap_or_default();
        bail!(
            "this agent's code expired at {}: restoring nothing. A new one is `datum sync chain agent` on a device in its chain",
            local(&at.to_rfc3339())
        );
    }
    // Every refusal before the device key is set, so a refused restore
    // leaves the install as it was; a live member of the code's chain, the
    // agent's copy or the person's own, only syncs.
    let chain = db.sync_chain()?;
    let me = db.sync_endpoint_id()?;
    if chain.exists() {
        ensure!(
            chain.id == a.chain && chain.dissolved().is_none(),
            "this database is in another sync chain: restore into a fresh one, `datum --db <NAME> sync restore`"
        );
        if chain.member(&me).is_some_and(|m| m.revoked.is_none()) {
            datum_sync::now_all(db, false)?;
            println!("{AFTER}");
            return Ok(());
        }
    }
    let device: String = a.device.iter().map(|b| format!("{b:02x}")).collect();
    crate::config::set_device_key(&device)?;
    let _first = super::sync_daemon::first_sync(db)?;
    let proof = db.sync_standing_claim(&a.device, &a.chain)?;
    let label = a.label.clone();
    // A restore installs no daemon, so nothing here keeps its join trying.
    datum_sync::connect(
        db,
        &a.link(proof)?,
        label.as_deref(),
        None,
        &|| false,
        &|| {},
    )?;
    if db.sync_chain()?.member(&me).is_some() {
        println!("{AFTER}");
    } else {
        println!("This sandbox keeps nothing: in a fresh one, restore the code again");
    }
    Ok(())
}
