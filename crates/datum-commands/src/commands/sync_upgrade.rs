//! A sync chain's upgrades (da-w8zp5; vividn, 2026-11-04: «devices should be
//! able to update each other, but the sync chain should be smart», «floors
//! only», «whole chain», and of `chain upgrade`: «sends an upgrade signal
//! across the whole chain and gets everyone on the latest version (even not
//! crossing any sync floors)»).
//!
//! - `datum sync pin <VERSION> | unpin`: the chain's `pin` record, which no
//!   member's upgrade goes past (`install::upgrade` reads it), so its floor
//!   cannot rise above it. A pin moves no copy back.
//! - `datum sync chain upgrade`: this copy upgrades, then writes the chain's
//!   `upgrade` record, an ask every member follows as it syncs.
//! - [`follow`]: after a sync, a copy upgrades when a member asked and it is
//!   behind the version asked, or a member's sync floor is above its schema.
//!   Between floors nothing moves. It runs `datum upgrade`, which fetches and
//!   verifies the release itself, never anything a peer sent; a datum
//!   `datum install` did not set up (a checkout's build) says so instead.

use anyhow::{ensure, Result};
use chrono::Utc;
use datum_core::db::chain::{self, at_least, held_at, past_pin, Chain};
use datum_core::db::Db;
use datum_core::migrations::said_build;
use serde_json::{json, Value};

/// `datum sync pin <VERSION>`, or with none `unpin`.
pub fn pin(db: &Db, version: Option<String>, admin_key: Option<&str>) -> Result<()> {
    let chain = db.sync_chain()?;
    ensure!(chain.exists(), super::sync::NO_CHAIN);
    if version.is_none() && chain.pin.is_none() {
        println!("sync chain {} is not pinned", chain.id);
        return Ok(());
    }
    let record = json!({
        "type": "pin", "chain": chain.id, "version": version.as_deref().unwrap_or(""),
        "created": Utc::now().to_rfc3339(),
    });
    // In a locked chain an admin signs it, or `sign` refuses: the fold takes
    // a pin from no one else.
    super::sync::sign(db, record, &chain, admin_key)?;
    let Some(v) = version else {
        println!(
            "unpinned sync chain {}: its members upgrade as before",
            chain.id
        );
        return Ok(());
    };
    println!(
        "pinned sync chain {} at datum {v}: no member upgrades past it",
        chain.id
    );
    for (label, datum) in builds(db, &db.sync_chain()?)? {
        if datum.as_deref().is_some_and(|d| past_pin(d, &v)) {
            println!(
                "  {label} runs datum {}, past the pin: it stays there, as a pin moves no copy back",
                datum.unwrap_or_default()
            );
        }
    }
    Ok(())
}

/// Each live member's label and the build it last said it runs, this copy's
/// its own.
fn builds(db: &Db, chain: &Chain) -> Result<Vec<(String, Option<String>)>> {
    let me = db.sync_endpoint_id()?;
    let versions = db.sync_versions()?;
    Ok(chain
        .members
        .iter()
        .filter(|m| m.revoked.is_none())
        .map(|m| {
            let datum = match m.endpoint == me {
                true => Some(said_build()),
                false => versions.get(&m.endpoint).map(|(d, _)| d.clone()),
            };
            (m.label.clone(), datum.filter(|d| !d.is_empty()))
        })
        .collect())
}

pub(crate) fn label(chain: &Chain, endpoint: &str) -> String {
    chain.member(endpoint).map_or_else(
        || endpoint[..10.min(endpoint.len())].into(),
        |m| m.label.clone(),
    )
}

/// What `sync status` says of the pin and the last ask, as lines and as
/// `--json`'s `pin` and `asked`: who has followed the ask, by the build each
/// member last said it runs, as far as the pin.
pub fn shown(db: &Db, chain: &Chain) -> Result<(Vec<String>, Value)> {
    let at = |hlc: &str| on_clock(chain::hlc_time(hlc));
    let mut lines = Vec::new();
    let pin = chain.pin.as_ref().map(|p| {
        let v = p.version.clone().unwrap_or_default();
        lines.push(format!(
            "pinned at datum {v} by {}, {}: no member upgrades past it — `datum sync unpin`",
            label(chain, &p.by),
            at(&p.hlc)
        ));
        json!({"version": v, "by": label(chain, &p.by), "at": chain::hlc_time(&p.hlc)})
    });
    let asked = match &chain.upgrade {
        None => Value::Null,
        Some(a) => {
            let pin = pin.as_ref().and_then(|p| p["version"].as_str());
            let (mut followed, mut behind, mut not_yet) = (Vec::new(), Vec::new(), Vec::new());
            for (who, datum) in builds(db, chain)? {
                let done = match (&a.version, &datum) {
                    (_, None) => false,
                    (_, Some(d)) if pin.is_some_and(|p| held_at(d, p)) => true,
                    (Some(want), Some(d)) => at_least(d, want),
                    (None, Some(_)) => false,
                };
                let said = datum.as_deref().unwrap_or("not heard from");
                match done {
                    true => followed.push(who),
                    false => {
                        not_yet.push(format!("{who} ({said})"));
                        behind.push(json!({"label": who, "datum": said}));
                    }
                }
            }
            let to = a
                .version
                .as_deref()
                .map_or(String::new(), |v| format!(" to datum {v}"));
            lines.push(format!(
                "{} asked every member to upgrade{to}, {}: followed by {}; not yet {}",
                label(chain, &a.by),
                at(&a.hlc),
                none_if_empty(&followed),
                none_if_empty(&not_yet),
            ));
            json!({
                "by": label(chain, &a.by), "at": chain::hlc_time(&a.hlc), "datum": a.version,
                "followed": followed, "behind": behind,
            })
        }
    };
    Ok((lines, json!({"pin": pin, "asked": asked})))
}

fn none_if_empty(names: &[String]) -> String {
    match names.is_empty() {
        true => "none".into(),
        false => names.join(", "),
    }
}

fn on_clock(at: &str) -> String {
    datum_core::time::on_readers_clock(at, "%H:%M")
}

#[cfg(feature = "net")]
pub use net::{ask, asked, follow, runs};

#[cfg(feature = "net")]
mod net {
    use std::collections::BTreeMap;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::commands::install;

    /// `datum sync chain upgrade`: this copy upgrades as far as the pin, then
    /// asks every member to, naming the version it now runs, and syncs the ask.
    pub fn ask(db: &Db) -> Result<()> {
        ensure!(db.sync_chain()?.exists(), super::super::sync::NO_CHAIN);
        let datum = match install::managed() {
            Ok(exe) => {
                let ran = Command::new(&exe).arg("upgrade").status()?;
                ensure!(
                    ran.success(),
                    "this copy did not upgrade, so nothing was asked"
                );
                Some(runs(&exe)?)
            }
            Err(why) => {
                eprintln!("warning: this copy does not upgrade itself: {why}");
                None
            }
        };
        asked(db, datum)
    }

    /// The version the datum at `exe` says it is.
    pub fn runs(exe: &std::path::Path) -> Result<String> {
        let said = Command::new(exe).arg("--version").output()?;
        let said = String::from_utf8_lossy(&said.stdout);
        Ok(said.trim().trim_start_matches("datum ").to_string())
    }

    /// The ask itself, naming `datum`, the version this copy now runs, and
    /// its sync: [`ask`] once this copy has upgraded, and `datum upgrade
    /// --force-upgrade` (da-kf328).
    pub fn asked(db: &Db, datum: Option<String>) -> Result<()> {
        let chain = db.sync_chain()?;
        ensure!(chain.exists(), super::super::sync::NO_CHAIN);
        let record = json!({
            "type": "upgrade", "chain": chain.id, "datum": datum,
            "created": Utc::now().to_rfc3339(),
        });
        db.sync_chain_write(record, true)?;
        let pin = chain.pin.as_ref().and_then(|p| p.version.as_deref());
        println!(
            "asked every member of sync chain {} to upgrade{}{}: each fetches and verifies the \
             release itself as it next syncs; `datum sync status` shows who has followed",
            chain.id,
            datum.map_or(String::new(), |d| format!(" to datum {d}")),
            pin.map_or(String::new(), |p| format!(
                ", as far as the chain's pin, datum {p}"
            )),
        );
        if let Err(e) = datum_sync::now_all(db, false) {
            eprintln!("warning: {e:#}: the ask goes out as this copy next syncs");
        }
        Ok(())
    }

    /// Why this copy should upgrade now, if it should, with the key it is
    /// tried once under: a member's ask it is behind, or a member's floor
    /// above its schema.
    fn reason(db: &Db, chain: &Chain) -> Result<Option<(String, String)>> {
        let me = db.sync_endpoint_id()?;
        if let Some(a) = chain.upgrade.as_ref().filter(|a| a.by != me) {
            let behind = a
                .version
                .as_deref()
                .is_none_or(|v| !at_least(&said_build(), v));
            if behind {
                let words = format!("{} asked every member to upgrade", label(chain, &a.by));
                return Ok(Some((format!("ask {}", a.hlc), words)));
            }
        }
        let apart = db
            .sync_floors_apart()?
            .into_iter()
            .find(|(_, a)| a.this_updates);
        if let Some((ep, a)) = apart {
            return Ok(Some((
                format!("floor {}", a.floor),
                a.words(&label(chain, &ep)),
            )));
        }
        // A member that went past the pin with `datum upgrade --leave-behind`
        // (da-kf328), told to a copy held there, once per build it says: a
        // prompt alone, so a copy below the pin is not moved by it.
        let Some(pin) = chain.pin.as_ref().and_then(|p| p.version.as_deref()) else {
            return Ok(None);
        };
        let build = said_build();
        if !held_at(&build, pin) {
            return Ok(None);
        }
        let versions = db.sync_versions()?;
        Ok(chain
            .members
            .iter()
            .filter(|m| m.revoked.is_none() && m.endpoint != me)
            .find_map(|m| {
                let (d, _) = versions.get(&m.endpoint)?;
                (!d.is_empty() && past_pin(d, pin) && !at_least(&build, d)).then(|| {
                    let words = format!("{} upgraded past the pin to datum {d}", m.label);
                    (format!("left {} {d}", m.endpoint), words)
                })
            }))
    }

    /// The reason last acted on, `_meta` `upgrade_tried`: its key, and
    /// whether it is done or failed within the hour, so not tried again yet.
    /// An ask naming no version (its asker a checkout's build) is done once
    /// `datum upgrade` succeeds, the newest release then being this copy's.
    fn tried(db: &Db, key: &str) -> bool {
        let got: Option<String> = db
            .conn()
            .query_row(
                "SELECT value FROM _meta WHERE key = 'upgrade_tried'",
                [],
                |r| r.get(0),
            )
            .ok();
        let Some(v) = got.and_then(|g| serde_json::from_str::<Value>(&g).ok()) else {
            return false;
        };
        let recent = v["at"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .is_some_and(|t| Utc::now().signed_duration_since(t) < chrono::Duration::hours(1));
        v["key"] == key && (v["ok"] == true || recent)
    }

    fn try_as(db: &Db, key: &str, ok: bool) -> Result<()> {
        let v = json!({"key": key, "ok": ok, "at": Utc::now().to_rfc3339()}).to_string();
        db.conn().execute(
            "INSERT INTO _meta (key, value) VALUES ('upgrade_tried', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [v],
        )?;
        Ok(())
    }

    /// After a sync: upgrade where the chain asks it, as far as its pin, once
    /// per reason (and again an hour after one failed). A datum `datum install`
    /// did not set up says the chain has moved on instead. `background`, a
    /// `sync run`'s, which ticks twice a second: it looks as a chain record
    /// comes, else once a minute, and the upgrade runs on a thread of its
    /// own, logged to stderr.
    pub fn follow(db: &Db, background: bool) {
        static LOOKED: Mutex<BTreeMap<String, (Instant, i64)>> = Mutex::new(BTreeMap::new());
        if background {
            let path = db.conn().path().unwrap_or_default().to_string();
            let records = crate::commands::sync_grant::records(db);
            let mut looked = LOOKED.lock().unwrap();
            let fresh =
                |(at, n): &(Instant, i64)| *n == records && at.elapsed() < Duration::from_secs(60);
            if looked.get(&path).is_some_and(fresh) {
                return;
            }
            looked.insert(path, (Instant::now(), records));
        }
        if let Err(e) = follow_or_fail(db, background) {
            eprintln!("⚠ upgrading as the sync chain asks: {e:#}");
        }
    }

    fn follow_or_fail(db: &Db, background: bool) -> Result<()> {
        let chain = db.sync_chain_served()?;
        let Some((key, why)) = reason(db, &chain)? else {
            return Ok(());
        };
        // The pin is part of the key, so a change of pin, an unpin
        // among them, tries the reason again.
        let pin = chain.pin.as_ref().and_then(|p| p.version.as_deref());
        let key = format!("{key} pin {}", pin.unwrap_or("none"));
        if tried(db, &key) {
            return Ok(());
        }
        let build = said_build();
        if let Some(p) = pin {
            if held_at(&build, p) {
                try_as(db, &key, true)?;
                match past_pin(&build, p) {
                    // Ahead of the pin: no upgrade would move it, pinned or not.
                    true => eprintln!(
                        "⚠ {why}, and this copy runs datum {build}, past the sync chain's pin \
                         (datum {p}): it stays where it is"
                    ),
                    false => eprintln!(
                        "⚠ {why}, and this copy runs datum {build}, held at the sync chain's pin \
                         (datum {p}): `datum upgrade --leave-behind` upgrades it past the pin, \
                         `datum sync unpin` lets it upgrade"
                    ),
                }
                return Ok(());
            }
        }
        let exe = match install::managed() {
            Ok(exe) => exe,
            Err(not) => {
                // Below a floor it cannot sync, so it says so again hourly, and
                // upgrades itself once `datum install` sets it up; an ask, once.
                try_as(db, &key, !key.starts_with("floor"))?;
                eprintln!("⚠ {why}: the sync chain has moved on, but {not}");
                return Ok(());
            }
        };
        try_as(db, &key, false)?;
        let path = db.conn().path().unwrap_or_default().to_string();
        // The child says what it did on this process's own stdout and stderr,
        // so a daemon's journal keeps it even where the restart that ends the
        // upgrade takes the child with it (systemd's cgroup: its work is done
        // by then). In a process group of its own: launchd kills a job's whole
        // group as it unloads it, which would take the child before its
        // `launchctl load` puts the daemon back (vremya's reading of
        // launchd.plist(5) on #1759; nothing here runs launchd).
        let run = move || {
            let ran = Command::new(&exe)
                .arg("upgrade")
                .stdin(Stdio::null())
                .process_group(0)
                .status();
            match ran {
                Ok(s) if s.success() => (true, None),
                Ok(s) => (false, Some(format!("it exited with {s}"))),
                Err(e) => (false, Some(e.to_string())),
            }
        };
        let failed = {
            let why = why.clone();
            move |e: String| format!("⚠ {why}, but the upgrade failed: {e}; tried again in an hour")
        };
        if !background {
            println!("{why}: upgrading");
            let (ok, e) = run();
            if let Some(e) = e {
                eprintln!("{}", failed(e));
            }
            return try_as(db, &key, ok);
        }
        std::thread::spawn(move || {
            eprintln!("{} {why}: upgrading", Utc::now().to_rfc3339());
            let (ok, e) = run();
            if let Some(e) = e {
                eprintln!("{} {}", Utc::now().to_rfc3339(), failed(e));
            }
            // Unrecorded, the reason is tried again at the next look.
            if let Err(e) = Db::open(&path)
                .map_err(Into::into)
                .and_then(|db| try_as(&db, &key, ok))
            {
                eprintln!("⚠ {path}: could not record the upgrade's outcome: {e:#}");
            }
        });
        Ok(())
    }
}
