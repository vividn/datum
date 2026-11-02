//! `datum db backup daemon install | uninstall | status` (da-gbac0; vividn,
//! 2026-11-02: «let's have a backup daemon that can be installed. Please
//! systemd and not cron»): the rotation `db backup --to DIR --every D`
//! keeps, run every D by a systemd user timer and the oneshot service it
//! starts. A daemon is a database and a destination: several destinations
//! are several daemons, a local rotation every 15m and a daily one to an
//! external drive, say. DIR defaults to the data directory's `backups/`, and
//! the rotation keeps each database in its own `DIR/<label>/`.
//!
//! A timer and not a long-running service, as the sync daemon is: a backup
//! is a job that ends, so nothing stays resident between runs, and systemd
//! keeps how the last run went, which `status` and `datum check` read. With
//! no systemd (macOS) it installs nothing and says so: never cron.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};

use super::backup_rotation::{ago, label};
use super::sync_daemon::{ctl, linger_note, quoted, ENV};
use crate::args::BackupDaemonAct;
use crate::config;

const PREFIX: &str = "datum-backup-";
/// The lines in each service naming its database's file and its destination.
const DB_KEY: &str = "X-Datum-Db=";
const TO_KEY: &str = "X-Datum-To=";
const NO_SYSTEMD: &str = "no systemd here, and `datum db backup daemon` installs a systemd \
     user timer: run `datum db backup --to DIR --every DUR` every DUR from what starts jobs here";

pub fn daemon(act: BackupDaemonAct, cli_db: Option<&str>) -> Result<()> {
    match act {
        BackupDaemonAct::Install { to, every, slots } => install(to, every, slots, cli_db),
        BackupDaemonAct::Uninstall { to } => {
            let db = canon(&config::locate_db(cli_db).context("no data directory")?);
            let to = to.map(|t| canon(Path::new(&t)));
            let theirs: Vec<_> = (units()?.into_iter())
                .filter(|(_, b)| key(b, DB_KEY) == Some(db.as_path()))
                .filter(|(_, b)| to.as_deref().is_none_or(|t| key(b, TO_KEY) == Some(t)))
                .collect();
            if theirs.is_empty() {
                let into = to.map_or(String::new(), |t| format!(" into {}", t.display()));
                println!("no backup daemon backs up {}{into}", db.display());
            }
            theirs
                .iter()
                .try_for_each(|(unit, body)| remove(unit, body))
        }
        BackupDaemonAct::Status => {
            let units = units()?;
            if units.is_empty() {
                println!("none installed: `datum db backup daemon install --every DUR` backs a database up every DUR");
            }
            for (unit, body) in &units {
                let what = body.lines().find_map(|l| l.strip_prefix("Description="));
                println!("{unit}: {}", what.unwrap_or("?").replace("%%", "%"));
                let run = last_run(unit);
                let at = run.get("ExecMainExitTimestamp").filter(|t| !t.is_empty());
                match (at, run.get("Result").map(String::as_str)) {
                    (None, _) => println!("  not run yet"),
                    (Some(at), Some("success")) => println!("  last ran {at}: succeeded"),
                    (Some(at), result) => {
                        println!("  last ran {at}: failed ({})", result.unwrap_or("?"))
                    }
                }
                if timer_stopped(unit) {
                    println!("  {}", stopped(unit, body));
                }
            }
            if !units.is_empty() {
                linger_note();
            }
            Ok(())
        }
    }
}

/// Where the units go: systemd's user directory, on Linux alone.
fn dir() -> Result<PathBuf> {
    ensure!(cfg!(target_os = "linux"), NO_SYSTEMD);
    Ok(config::xdg_config_dir()
        .context("HOME is not set")?
        .join("systemd/user"))
}

/// A file or directory as the units name it: a symlink's target. One gone
/// since is named by its directory's, so it still matches.
fn canon(p: &Path) -> PathBuf {
    let in_dir = || {
        let dir = std::fs::canonicalize(p.parent()?.to_str().filter(|d| !d.is_empty())?).ok()?;
        Some(dir.join(p.file_name()?))
    };
    (std::fs::canonicalize(p).ok())
        .or_else(in_dir)
        .or_else(|| std::path::absolute(p).ok())
        .unwrap_or_else(|| p.into())
}

/// A path a service names on its `X-Datum-…=` line.
fn key<'a>(body: &'a str, k: &str) -> Option<&'a Path> {
    body.lines().find_map(|l| l.strip_prefix(k)).map(Path::new)
}

/// Every backup daemon installed: its unit's name, and its service's text.
fn units() -> Result<Vec<(String, String)>> {
    let Ok(read) = std::fs::read_dir(dir()?) else {
        return Ok(Vec::new());
    };
    let mut found: Vec<_> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let unit = name.strip_suffix(".service")?.strip_prefix(PREFIX)?;
            Some((
                format!("{PREFIX}{unit}"),
                std::fs::read_to_string(e.path()).ok()?,
            ))
        })
        .collect();
    found.sort();
    Ok(found)
}

fn install(to: Option<String>, every: i64, slots: Option<u32>, cli_db: Option<&str>) -> Result<()> {
    let dir = dir()?;
    let db = config::locate_db(cli_db).context("no data directory")?;
    ensure!(
        db.exists(),
        "no database at {}: nothing to back up",
        db.display()
    );
    let db = canon(&db);
    // The name a run by hand gives, so it keeps the rotation it began.
    let source = config::active_db_name(cli_db).unwrap_or_else(|| db.display().to_string());
    let to = match to {
        Some(to) => PathBuf::from(to),
        None => config::data_dir_path()
            .context("no data directory")?
            .join("backups"),
    };
    std::fs::create_dir_all(&to).with_context(|| format!("creating {}", to.display()))?;
    let to = canon(&to);
    // The destination's hash keeps two destinations' daemons apart.
    let hash = format!("{:x}", Sha256::digest(to.display().to_string()));
    let unit = format!("{PREFIX}{}-{}", label(&source), &hash[..8]);
    let service = dir.join(format!("{unit}.service"));
    if let Some(other) = std::fs::read_to_string(&service)
        .ok()
        .and_then(|b| key(&b, DB_KEY).filter(|o| *o != db).map(Path::to_path_buf))
    {
        bail!(
            "{unit} backs up {} into {} already: `datum --db {} db backup daemon uninstall --to {}` frees the name",
            other.display(),
            to.display(),
            other.display(),
            to.display()
        );
    }
    let exe = std::env::current_exe().context("finding this datum binary")?;
    let mut run = format!(
        "\"{}\" --db \"{}\" db backup --to \"{}\" --every {every}s",
        quoted(&exe.display().to_string()),
        quoted(&source),
        quoted(&to.display().to_string()),
    );
    if let Some(n) = slots {
        run += &format!(" --slots {n}");
    }
    let env: String = ENV
        .iter()
        .filter_map(|k| Some((k, std::env::var(k).ok().filter(|v| !v.is_empty())?)))
        .map(|(k, v)| format!("Environment=\"{k}={}\"\n", quoted(&v)))
        .collect();
    let what = format!(
        "datum db backup of {source} into {}, every {}",
        to.display(),
        ago(every)
    );
    let described = format!("[Unit]\nDescription={}\n", what.replace('%', "%%"));
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    std::fs::write(
        &service,
        format!(
            "{described}{DB_KEY}{}\n{TO_KEY}{}\n\n[Service]\nType=oneshot\nExecStart={run}\n{env}",
            db.display(),
            to.display()
        ),
    )
    .with_context(|| format!("writing {}", service.display()))?;
    // Fired as it starts, so at install and at each login, then every D
    // after the last run began.
    let timer = format!("{unit}.timer");
    std::fs::write(
        dir.join(&timer),
        format!(
            "{described}\n[Timer]\nOnActiveSec=0\nOnUnitActiveSec={every}s\nAccuracySec=1s\n\n\
             [Install]\nWantedBy=timers.target\n"
        ),
    )
    .with_context(|| format!("writing {timer}"))?;
    ctl("systemctl", &["--user", "daemon-reload"])?;
    ctl("systemctl", &["--user", "enable", &timer])?;
    ctl("systemctl", &["--user", "restart", &timer])
        .with_context(|| format!("wrote {unit}, but could not start it"))?;
    println!(
        "installed the systemd user timer {timer} ({}): {what}, the first now",
        dir.display()
    );
    linger_note();
    Ok(())
}

/// Stop and remove a backup daemon, naming it.
fn remove(unit: &str, body: &str) -> Result<()> {
    let dir = dir()?;
    // Stopping may fail (no user bus): the files go regardless.
    let stopped = ctl(
        "systemctl",
        &["--user", "disable", "--now", &format!("{unit}.timer")],
    );
    for kind in ["timer", "service"] {
        let path = dir.join(format!("{unit}.{kind}"));
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e).with_context(|| format!("removing {}", path.display()))
            }
            _ => {}
        }
    }
    let _ = ctl("systemctl", &["--user", "daemon-reload"]);
    // A failed last run would stay listed as a failed unit, file or none.
    let _ = ctl(
        "systemctl",
        &["--user", "reset-failed", &format!("{unit}.service")],
    );
    println!(
        "removed the systemd user timer {unit}.timer, into {}",
        into(body)
    );
    if let Err(e) = stopped {
        eprintln!("warning: it may still be running: {e:#}");
    }
    Ok(())
}

/// The destination a service names.
fn into(body: &str) -> String {
    key(body, TO_KEY).map_or("?".into(), |t| t.display().to_string())
}

/// Whether the unit's timer is not running: `is-active` says 3, «not
/// running», or 4, «no such unit» (systemd never loaded it). Any other
/// failure is systemctl not reaching the user bus, which says nothing of it.
fn timer_stopped(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", &format!("{unit}.timer")])
        .status()
        .is_ok_and(|s| matches!(s.code(), Some(3 | 4)))
}

fn stopped(unit: &str, body: &str) -> String {
    format!(
        "the backup daemon into {} is not running: `systemctl --user enable --now {unit}.timer` starts it",
        into(body)
    )
}

/// How the service's last run went, as systemd keeps it: `Result` and
/// `ExecMainExitTimestamp`, empty before the first.
fn last_run(unit: &str) -> BTreeMap<String, String> {
    let out = Command::new("systemctl")
        .args(["--user", "show", "--property=Result,ExecMainExitTimestamp"])
        .arg(format!("{unit}.service"))
        .output();
    let text = out.map_or(String::new(), |o| {
        String::from_utf8_lossy(&o.stdout).into_owned()
    });
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.trim().to_string()))
        .collect()
}

/// `datum check`'s word on each backup daemon of the database at `db`, by
/// its destination: its timer not running, or its last run failed.
pub fn findings(db: &Path) -> Vec<(&'static str, String)> {
    let db = canon(db);
    let mut found = Vec::new();
    let units = units().unwrap_or_default();
    for (unit, body) in units
        .iter()
        .filter(|(_, b)| key(b, DB_KEY) == Some(db.as_path()))
    {
        if timer_stopped(unit) {
            found.push(("backup_daemon_missing", stopped(unit, body)));
        }
        match last_run(unit).get("Result").map(String::as_str) {
            None | Some("" | "success") => {}
            Some(result) => found.push((
                "backup_daemon_failed",
                format!(
                    "the last backup into {} failed ({result}): `journalctl --user -u {unit}` says why",
                    into(body)
                ),
            )),
        }
    }
    found
}

/// `datum install`'s line: which are installed, or how to install one.
pub fn said() -> Option<String> {
    let units = units().ok()?;
    Some(match units.is_empty() {
        true => "backups on a schedule: `datum db backup daemon install --every 1h`".into(),
        false => format!(
            "backup daemon: {} (`datum db backup daemon status`)",
            units
                .iter()
                .map(|(u, _)| &u[..])
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

/// `datum uninstall`'s part: every backup daemon goes.
pub fn uninstall_all() -> Result<()> {
    match dir() {
        Ok(_) => units()?
            .iter()
            .try_for_each(|(unit, body)| remove(unit, body)),
        Err(_) => Ok(()),
    }
}
