//! `datum backup --auto | --no-auto | status` (da-tsna7; `backup daemon
//! install | uninstall | status` before it, da-gbac0; vividn, 2026-11-02:
//! «let's have a backup daemon that can be installed. Please systemd and not
//! cron»): the rotation `backup --to DIR` keeps, a step every D by a systemd
//! user timer and the oneshot service it starts. The install records D and
//! the cap in DIR, where each step reads them, so the step names neither. A daemon is a database and a destination: several destinations
//! are several daemons, a local rotation every 15m and a daily one to an
//! external drive, say. DIR defaults to the data directory's `backups/`, and
//! the rotation keeps each database in its own `DIR/<label>/`.
//!
//! `--db a,b,c` installs, removes or shows one daemon per database, each
//! what that `--db` alone makes; `--all`, one whose run backs up every
//! database datum knows at that run (da-bw7ur). Each timer fires at a fixed
//! random offset, so daemons installed together do not run together.
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

use super::backup_restore::derive_source_db;
use super::backup_rotation::{
    ago, canon, label, newest, rename_rotation, rotation_name, set_schedule, Schedule,
};
use super::sync_daemon::{ctl, linger_note, quoted, ENV};
use crate::args::BackupDaemonAct;
use crate::config;

const PREFIX: &str = "datum-backup-";
/// The lines in each service naming its database's file and its destination.
const DB_KEY: &str = "X-Datum-Db=";
const TO_KEY: &str = "X-Datum-To=";
/// The line in the service of the daemon that backs up every database.
const ALL_LINE: &str = "X-Datum-All=yes";
const NOT_ALL: &str = "--all or --db, not both: --all backs up every database datum knows";
const NO_SYSTEMD: &str = "no systemd here, and a backup on a schedule is a systemd user timer: \
     `datum backup --to DIR --auto` sets the rotation up in DIR, and `datum backup --to DIR` \
     takes its step, from what starts jobs here";

pub fn daemon(act: BackupDaemonAct, cli_db: Option<&str>) -> Result<()> {
    match act {
        BackupDaemonAct::Install {
            to,
            every,
            slots,
            all: true,
        } => {
            ensure!(cli_db.is_none(), NOT_ALL);
            install(to, every, slots, None, true)
        }
        BackupDaemonAct::Install {
            to, every, slots, ..
        } => {
            let dbs = each(cli_db)?;
            // Every one there before any is installed, so a list never
            // stops half way for a database that is not.
            for db in &dbs {
                let path = config::locate_db(*db).context("no data directory")?;
                ensure!(
                    path.exists(),
                    "no database at {}: nothing to back up",
                    path.display()
                );
            }
            (dbs.into_iter()).try_for_each(|db| install(to.clone(), every, slots, db, false))
        }
        BackupDaemonAct::Uninstall { to, all } => {
            ensure!(!all || cli_db.is_none(), NOT_ALL);
            let to = to.map(|t| canon(Path::new(&t)));
            let whose = match all {
                true => vec![None],
                false => (each(cli_db)?.into_iter())
                    .map(|db| {
                        Ok(Some(canon(
                            &config::locate_db(db).context("no data directory")?,
                        )))
                    })
                    .collect::<Result<_>>()?,
            };
            let units = units()?;
            for db in whose {
                let theirs: Vec<_> = (units.iter())
                    .filter(|(_, b)| ours(b, db.as_deref()))
                    .filter(|(_, b)| to.as_deref().is_none_or(|t| key(b, TO_KEY) == Some(t)))
                    .collect();
                if theirs.is_empty() {
                    let into = to
                        .as_ref()
                        .map_or(String::new(), |t| format!(" into {}", t.display()));
                    let what = db.map_or("every database".into(), |d| d.display().to_string());
                    println!("no backup daemon backs up {what}{into}");
                }
                theirs
                    .iter()
                    .try_for_each(|(unit, body)| remove(unit, body))?;
            }
            Ok(())
        }
        BackupDaemonAct::Status => {
            let mut units = units()?;
            // `--db`, one or a list, shows those databases' and every database's.
            if cli_db.is_some() {
                let dbs: Vec<PathBuf> = (each(cli_db)?.into_iter())
                    .filter_map(|db| config::locate_db(db).map(|p| canon(&p)))
                    .collect();
                units.retain(|(_, b)| ours(b, None) || dbs.iter().any(|d| ours(b, Some(d))));
            }
            if units.is_empty() {
                println!(
                    "none installed: `datum backup --auto` backs a database up every 30m, \
                     `--every DUR` every DUR"
                );
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
                if body.lines().any(|l| l == ALL_LINE) {
                    for (name, path) in config::known_dbs() {
                        let to = key(body, TO_KEY).unwrap_or(Path::new(""));
                        match (path.exists(), newest(to, &name).map(|(at, _)| at)) {
                            (false, _) => {}
                            (true, None) => println!("  {name}: no backup yet"),
                            (true, Some(at)) => println!(
                                "  {name}: newest backup {} ago",
                                ago((chrono::Utc::now() - at).num_seconds())
                            ),
                        }
                    }
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

/// The databases `--db` names, one or a list (da-bw7ur); with none, the
/// one datum opens by default.
fn each(cli_db: Option<&str>) -> Result<Vec<Option<&str>>> {
    match cli_db {
        None => Ok(vec![None]),
        Some(v) => Ok(config::db_list(v)?.into_iter().map(Some).collect()),
    }
}

/// Whether the service is the daemon of the database at `db`, or, with
/// `None`, of every database.
fn ours(body: &str, db: Option<&Path>) -> bool {
    match db {
        None => body.lines().any(|l| l == ALL_LINE),
        Some(db) => key(body, DB_KEY) == Some(db),
    }
}

/// The timer's random delay: a tenth of the period, so it stays well below
/// it, and five minutes at most, so an hourly backup is not late by six.
/// Fixed per unit (`FixedRandomDelay`), so each daemon runs at its own
/// offset and two installed together do not run together (da-bw7ur).
fn delay(every: i64) -> i64 {
    (every / 10).min(300)
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

/// The unit `install` writes for a database, or with `all` every one, into
/// `to` (DIR, by default the data directory's `backups/`): its name, the
/// database's file, the name its step backs it up by, DIR, and what it does,
/// in words. `make` makes DIR first, so it is named as it will be.
struct Planned {
    name: String,
    db: Option<PathBuf>,
    source: String,
    to: PathBuf,
    what: String,
}

fn planned(
    to: Option<String>,
    every: i64,
    cli_db: Option<&str>,
    all: bool,
    make: bool,
) -> Result<Planned> {
    let (db, source) = match all {
        true => (None, "every database".to_string()),
        false => {
            let db = canon(&config::locate_db(cli_db).context("no data directory")?);
            let source = rotation_name(cli_db, &db);
            (Some(db), source)
        }
    };
    let to = match to {
        Some(to) => PathBuf::from(to),
        None => config::data_dir_path()
            .context("no data directory")?
            .join("backups"),
    };
    if make {
        std::fs::create_dir_all(&to).with_context(|| format!("creating {}", to.display()))?;
    }
    let to = canon(&to);
    // The destination's hash keeps two destinations' daemons apart.
    let hash = format!("{:x}", Sha256::digest(to.display().to_string()));
    let named = match all {
        true => "all".into(),
        false => label(&source),
    };
    let what = format!(
        "datum db backup of {source} into {}, every {}",
        to.display(),
        ago(every)
    );
    Ok(Planned {
        name: format!("{PREFIX}{named}-{}", &hash[..8]),
        db,
        source,
        to,
        what,
    })
}

/// `backup --dry-run`'s word (da-tsna7): the timer `--auto` would install,
/// its DIR, and what it would do; nothing is written.
pub(crate) fn would_install(
    to: Option<String>,
    every: i64,
    slots: Option<u32>,
    cli_db: Option<&str>,
    all: bool,
) -> Result<(String, String, String)> {
    ensure!(!all || cli_db.is_none(), NOT_ALL);
    dir()?;
    let p = planned(to, every, cli_db, all, false)?;
    let what = match slots {
        Some(n) => format!("{}, {n} slots at most", p.what),
        None => p.what,
    };
    Ok((p.name, p.to.display().to_string(), what))
}

/// The line a service runs.
fn exec_start(body: &str) -> &str {
    body.lines()
        .find_map(|l| l.strip_prefix("ExecStart="))
        .unwrap_or("")
}

fn install(
    to: Option<String>,
    every: i64,
    slots: Option<u32>,
    cli_db: Option<&str>,
    all: bool,
) -> Result<()> {
    let p = planned(to, every, cli_db, all, true)?;
    let (unit, to, what) = (&p.name, &p.to, &p.what);
    // The step reads its period and cap from DIR, under the name it backs
    // the database up by.
    let schedule = Schedule {
        every_secs: every,
        slots,
    };
    let step_db = (!all).then(|| derive_source_db(Some(&p.source)));
    // With no systemd (macOS) the rotation is set up and no timer is: what
    // starts jobs there takes the step.
    let Ok(dir) = dir() else {
        set_schedule(to, step_db.as_deref(), schedule)?;
        println!(
            "no systemd here, so no timer: the rotation in {} is set up, every {}; run \
             `datum{} backup --to \"{}\"` every {} from what starts jobs here",
            to.display(),
            ago(every),
            match all {
                true => String::new(),
                false => format!(" --db \"{}\"", p.source),
            },
            to.display(),
            ago(every)
        );
        return Ok(());
    };
    // Every database's daemon and one database's into one DIR would keep
    // that database's rotation at two rhythms at once.
    if let Some((other, _)) = (units()?.iter())
        .find(|(_, b)| key(b, TO_KEY) == Some(to.as_path()) && ours(b, None) != all)
    {
        bail!(
            "{other} backs up {} into {} already: back up into another DIR, or uninstall it first",
            match all {
                true => "a database",
                false => "every database",
            },
            to.display()
        );
    }
    let service = dir.join(format!("{unit}.service"));
    // A database named `all` and every database share a unit name: neither
    // takes the other's.
    let before = std::fs::read_to_string(&service).ok();
    if let Some(other) = before.as_ref().filter(|b| !ours(b, p.db.as_deref())) {
        let by = match key(other, DB_KEY) {
            Some(o) => format!("--db {}", o.display()),
            None => "--all".into(),
        };
        bail!(
            "{unit} backs up {} into {} already: `datum {by} backup --no-auto --to {}` frees the name",
            key(other, DB_KEY).map_or("every database".into(), |o| o.display().to_string()),
            to.display(),
            to.display()
        );
    }
    // A unit from before da-tsna7 ran `--to DIR --every D`, which now
    // installs, so its timer would take no backup again: run by systemd, it
    // is rewritten here and takes its step too. The timer is enabled and
    // restarted as any install's, so whichever unit ran it, the result is an
    // install's (a restart from inside the service starts no second run).
    let running_old = std::env::var_os("INVOCATION_ID").is_some()
        && before.is_some_and(|b| exec_start(&b).contains(" --every "));
    set_schedule(to, step_db.as_deref(), schedule)?;
    let exe = std::env::current_exe().context("finding this datum binary")?;
    let which = match all {
        true => "db backup --all".into(),
        false => format!("--db \"{}\" db backup", quoted(&p.source)),
    };
    let run = format!(
        "\"{}\" {which} --to \"{}\"",
        quoted(&exe.display().to_string()),
        quoted(&to.display().to_string()),
    );
    let env: String = ENV
        .iter()
        .filter_map(|k| Some((k, std::env::var(k).ok().filter(|v| !v.is_empty())?)))
        .map(|(k, v)| format!("Environment=\"{k}={}\"\n", quoted(&v)))
        .collect();
    let described = format!("[Unit]\nDescription={}\n", what.replace('%', "%%"));
    let whose = match &p.db {
        Some(db) => format!("{DB_KEY}{}", db.display()),
        None => ALL_LINE.into(),
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    std::fs::write(
        &service,
        format!(
            "{described}{whose}\n{TO_KEY}{}\n\n[Service]\nType=oneshot\nExecStart={run}\n{env}",
            to.display()
        ),
    )
    .with_context(|| format!("writing {}", service.display()))?;
    // Fired as it starts, so at install and at each login, then every D
    // after the last run began, each at the unit's own fixed delay. systemd
    // counts that delay into the wait from one run to the next, so the timer
    // waits D less the largest delay it may draw: runs stand at most D apart,
    // and no period of the rotation goes untaken.
    let timer = format!("{unit}.timer");
    let delay = delay(every);
    std::fs::write(
        dir.join(&timer),
        format!(
            "{described}\n[Timer]\nOnActiveSec=0\nOnUnitActiveSec={}s\n\
             RandomizedDelaySec={delay}s\nFixedRandomDelay=yes\nAccuracySec=1s\n\n\
             [Install]\nWantedBy=timers.target\n",
            every - delay
        ),
    )
    .with_context(|| format!("writing {timer}"))?;
    ctl("systemctl", &["--user", "daemon-reload"])?;
    let stepped = match running_old {
        false => None,
        true => {
            let mut step = Command::new(&exe);
            match all {
                true => step.args(["db", "backup", "--all"]),
                false => step.args(["--db", &p.source, "db", "backup"]),
            };
            let ran = (step.arg("--to").arg(to).status()).context("taking the step")?;
            println!("rewrote {unit} as it runs, its step `datum backup --to DIR` reading DIR");
            Some(ran)
        }
    };
    ctl("systemctl", &["--user", "enable", &timer])?;
    ctl("systemctl", &["--user", "restart", &timer])
        .with_context(|| format!("wrote {unit}, but could not start it"))?;
    println!(
        "installed the systemd user timer {timer} ({}): {what}{}, the first {}",
        dir.display(),
        slots.map_or(String::new(), |n| format!(", {n} slots at most")),
        match delay {
            0 => "now".into(),
            d => format!("within {}", ago(d)),
        }
    );
    linger_note();
    if let Some(ran) = stepped {
        ensure!(ran.success(), "the backup into {} failed", to.display());
    }
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
pub(super) fn timer_stopped(unit: &str) -> bool {
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

/// The name datum knows the database at `db` by, as the daemon of every
/// database backs it up under.
pub fn known_as(db: &Path) -> Option<String> {
    let db = canon(db);
    (config::known_dbs().into_iter()).find_map(|(n, p)| (canon(&p) == db).then_some(n))
}

/// `db rename`'s part (da-a0vw0): each daemon of the database that was at
/// `from` made the daemon of the one at `to`, by `new`, keeping its
/// destination, its period and whether its timer is enabled; and at each
/// destination it, or the daemon of every database, names, the rotation
/// moved with it. `was` is the name it was known by. What it did, a line each.
pub fn renamed(from: &Path, to: &Path, new: &str, was: Option<&str>) -> Result<Vec<String>> {
    let Ok(dir) = dir() else {
        return Ok(Vec::new());
    };
    let (from, to) = (canon(from), canon(to));
    // The name an install by `new` gives, as `install` reads it.
    let source = config::active_db_name(Some(new)).unwrap_or_else(|| to.display().to_string());
    let (units, mut said, mut each) = (units()?, Vec::new(), Vec::new());
    // Each of its daemons read, and every conflict found, before any is
    // touched: a rename refused leaves every daemon as it was.
    for (unit, body) in &units {
        let Some(dest) = key(body, TO_KEY).filter(|_| ours(body, Some(&from))) else {
            continue;
        };
        let into = format!(" into {}, every ", dest.display());
        let old = (body.lines())
            .find_map(|l| l.strip_prefix("Description=datum db backup of "))
            .and_then(|d| Some(d.replace("%%", "%").split_once(&into)?.0.to_string()))
            .with_context(|| format!("{unit} does not say which database it backs up"))?;
        let hash = unit.rsplit_once('-').map_or("", |(_, h)| h);
        let moved = format!("{PREFIX}{}-{hash}", label(&source));
        // Another database's daemon into the same destination keeps its name.
        if let Some(other) = std::fs::read_to_string(dir.join(format!("{moved}.service")))
            .ok()
            .filter(|b| moved != *unit && !ours(b, Some(&to)))
        {
            bail!(
                "{moved} backs up {} into {} already, so every backup daemon of {old} \
                 still backs it up: uninstall one, then install it again",
                key(&other, DB_KEY).map_or("every database".into(), |p| p.display().to_string()),
                dest.display()
            );
        }
        each.push((unit.as_str(), body.as_str(), dest.to_path_buf(), old, moved));
    }
    for body in units.iter().map(|(_, b)| b).filter(|b| ours(b, None)) {
        if let (Some(dest), Some(was), Some(now)) = (key(body, TO_KEY), was, known_as(&to)) {
            said.extend(rename_rotation(dest, was, &now)?);
        }
    }
    for (unit, body, dest, old, moved) in each {
        let swap = |text: &str| {
            let of = |s: &str| format!("of {} into", s.replace('%', "%%"));
            let by = |s: &str| format!("--db \"{}\" db backup", quoted(s));
            let at = |p: &Path| format!("\n{DB_KEY}{}\n", p.display());
            (text.replacen(&of(&old), &of(&source), 1))
                .replacen(&by(&old), &by(&source), 1)
                .replacen(&at(&from), &at(&to), 1)
        };
        let timer = std::fs::read_to_string(dir.join(format!("{unit}.timer")))
            .with_context(|| format!("reading {unit}.timer"))?;
        let enabled = Command::new("systemctl")
            .args(["--user", "is-enabled", "--quiet", &format!("{unit}.timer")])
            .status()
            .is_ok_and(|s| s.success());
        if moved != unit {
            let _ = ctl(
                "systemctl",
                &["--user", "disable", "--now", &format!("{unit}.timer")],
            );
        }
        for (kind, text) in [("service", body), ("timer", &timer)] {
            let path = dir.join(format!("{moved}.{kind}"));
            std::fs::write(&path, swap(text))
                .with_context(|| format!("writing {}", path.display()))?;
            if moved != unit {
                std::fs::remove_file(dir.join(format!("{unit}.{kind}")))?;
            }
        }
        let _ = ctl("systemctl", &["--user", "daemon-reload"]);
        if moved != unit {
            let _ = ctl(
                "systemctl",
                &["--user", "reset-failed", &format!("{unit}.service")],
            );
            if enabled {
                let timer = format!("{moved}.timer");
                ctl("systemctl", &["--user", "enable", &timer])?;
                ctl("systemctl", &["--user", "restart", &timer])?;
            }
            said.push(format!("backup daemon: {unit} → {moved}"));
        }
        said.extend(rename_rotation(&dest, &old, &source)?);
    }
    Ok(said)
}

/// `datum check`'s word on each backup daemon of the database at `db`, by
/// its destination: its timer not running, or its last run failed.
pub fn findings(db: &Path) -> Vec<(&'static str, String)> {
    let db = canon(db);
    let mut found = Vec::new();
    let units = units().unwrap_or_default();
    let name = known_as(&db);
    for (unit, body) in units
        .iter()
        .filter(|(_, b)| ours(b, Some(&db)) || name.is_some() && ours(b, None))
    {
        if timer_stopped(unit) {
            found.push(("backup_daemon_missing", stopped(unit, body)));
        }
        match last_run(unit).get("Result").map(String::as_str) {
            None | Some("" | "success") => {}
            // Every database's run failed for some: for this one, if its
            // newest backup there is older than the run's period.
            Some(result) if ours(body, None) => {
                let to = key(body, TO_KEY).unwrap_or(Path::new(""));
                let name = name.as_deref().unwrap_or_default();
                let at = newest(to, name);
                // Runs stand at most a period apart: older, the last missed it.
                if at.is_none_or(|(at, every)| (chrono::Utc::now() - at).num_seconds() > every) {
                    let had = at.map_or("none there yet".into(), |(at, _)| {
                        format!("its newest is {}", at.to_rfc3339())
                    });
                    found.push((
                        "backup_daemon_failed",
                        format!(
                            "the backup of {name} into {} failed ({result}), {had}: `journalctl --user -u {unit}` says why",
                            to.display()
                        ),
                    ));
                }
            }
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

/// `datum install`'s offer (da-tsna7): one timer backing up every database
/// every 30m into the data directory's `backups/`, asked by `ask`, `answer`
/// its `--yes` or `--no`. With no one to ask, none: no rotation starts that
/// no one chose. One installed already is said, and nothing is asked; with no
/// systemd nothing is offered.
pub fn offer(answer: Option<bool>, ask: impl FnOnce(&str) -> Result<Option<bool>>) -> Result<()> {
    let Some(said) = said() else {
        return Ok(());
    };
    if !units()?.is_empty() {
        println!("{said}");
        return Ok(());
    }
    let to = config::data_dir_path()
        .context("no data directory")?
        .join("backups");
    let every = datum_parse::cli::BACKUP_EVERY_SECS;
    let question = format!(
        "Back up every database every {}, into {}?",
        ago(every),
        to.display()
    );
    match answer.map_or_else(|| ask(&question), |a| Ok(Some(a)))? {
        Some(true) => install(None, every, None, None, true),
        _ => {
            println!("{said}");
            Ok(())
        }
    }
}

/// `datum install`'s line: which are installed, or how to install one.
pub fn said() -> Option<String> {
    let units = units().ok()?;
    Some(match units.is_empty() {
        true => "backups on a schedule: off; `datum backup --auto` turns them on".into(),
        false => format!(
            "backups on a schedule: {} (`datum backup status`)",
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
