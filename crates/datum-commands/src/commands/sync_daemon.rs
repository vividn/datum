//! `datum sync daemon install | uninstall | status` (da-vd2sn): keep every
//! chained database syncing in the background — a systemd user unit on
//! Linux, a launchd agent on macOS — running the hidden `sync daemon run`,
//! which is `sync run` for each of them in one process. The first `chain
//! invite` or `chain connect` of a copy in no chain installs it, unless
//! `DATUM_SYNC_NO_DAEMON=1`: background work starts when the person opts
//! into the feature it serves, never at CLI install time. A `connect`
//! installs it once admitted, so a first sync cut short leaves it, and the
//! daemon finishes that sync (da-qr7gz). Linger is left alone: a user
//! service stops at logout, and `status` says so.

use std::collections::BTreeSet;
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use datum_core::db::Db;
use datum_core::error::DatumError;

use crate::args::SyncDaemonAct;
use crate::config;

/// Set (to anything but `0`), no chain command installs the daemon.
const NO_DAEMON: &str = "DATUM_SYNC_NO_DAEMON";
const UNIT: &str = "datum-sync.service";
const LABEL: &str = "day.datum.sync";
/// Where datum keeps its files, handed from the installing shell to the
/// service, which does not inherit it.
const ENV: [&str; 4] = [
    "DATUM_HOME",
    "DATUM_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
];
/// How often `run` looks for a database newly in a chain.
const LOOK: Duration = Duration::from_secs(60);
const AT_LOGIN: &str =
    "no systemd or launchd here: run `datum sync daemon run` at login to keep syncing";

#[derive(Clone, Copy)]
enum Manager {
    Systemd,
    Launchd,
}

impl Manager {
    fn here() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::Launchd)
        } else if cfg!(target_os = "linux") {
            Some(Self::Systemd)
        } else {
            None
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Systemd => "systemd user service datum-sync",
            Self::Launchd => "launchd agent day.datum.sync",
        }
    }

    fn path(self) -> Result<PathBuf> {
        Ok(match self {
            Self::Systemd => config::xdg_config_dir()
                .context("HOME is not set")?
                .join("systemd/user")
                .join(UNIT),
            Self::Launchd => std::env::var_os("HOME")
                .map(PathBuf::from)
                .context("HOME is not set")?
                .join(format!("Library/LaunchAgents/{LABEL}.plist")),
        })
    }

    fn body(self, exe: &Path) -> String {
        let env: Vec<(&str, String)> = ENV
            .iter()
            .filter_map(|k| Some((*k, std::env::var(k).ok().filter(|v| !v.is_empty())?)))
            .collect();
        match self {
            Self::Systemd => format!(
                "[Unit]\nDescription=datum sync: every chained database\n\n\
                 [Service]\nExecStart=\"{}\" sync daemon run\n{}Restart=on-failure\nRestartSec=30\n\n\
                 [Install]\nWantedBy=default.target\n",
                quoted(&exe.display().to_string()),
                env.iter()
                    .map(|(k, v)| format!("Environment=\"{k}={}\"\n", quoted(v)))
                    .collect::<String>()
            ),
            Self::Launchd => format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\">\n<dict>\n\
                 <key>Label</key><string>{LABEL}</string>\n\
                 <key>ProgramArguments</key><array><string>{}</string><string>sync</string><string>daemon</string><string>run</string></array>\n\
                 <key>EnvironmentVariables</key><dict>{}</dict>\n\
                 <key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n\
                 </dict>\n</plist>\n",
                xml(&exe.display().to_string()),
                env.iter()
                    .map(|(k, v)| format!("<key>{k}</key><string>{}</string>", xml(v)))
                    .collect::<String>()
            ),
        }
    }

    /// Write the unit or agent and (re)start it, so a second install runs
    /// this binary.
    fn install(self) -> Result<PathBuf> {
        let path = self.path()?;
        let exe = std::env::current_exe().context("finding this datum binary")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(&path, self.body(&exe))
            .with_context(|| format!("writing {}", path.display()))?;
        let p = path.to_string_lossy();
        match self {
            Self::Systemd => {
                ctl("systemctl", &["--user", "daemon-reload"])?;
                ctl("systemctl", &["--user", "enable", UNIT])?;
                ctl("systemctl", &["--user", "restart", UNIT])
            }
            Self::Launchd => {
                let _ = ctl("launchctl", &["unload", &p]);
                ctl("launchctl", &["load", "-w", &p])
            }
        }
        .with_context(|| format!("wrote {}, but could not start it", path.display()))?;
        Ok(path)
    }

    fn running(self) -> bool {
        match self {
            Self::Systemd => ctl("systemctl", &["--user", "is-active", "--quiet", UNIT]),
            Self::Launchd => ctl("launchctl", &["list", LABEL]),
        }
        .is_ok()
    }
}

/// Run a service manager's command; its stderr is the error.
fn ctl(program: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("running {program}"))?;
    if !out.status.success() {
        bail!(
            "`{program} {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// A value inside a systemd unit's double quotes: `%` would be a specifier.
fn quoted(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn daemon(act: SyncDaemonAct) -> Result<()> {
    let manager = Manager::here();
    match act {
        SyncDaemonAct::Run => return run(),
        SyncDaemonAct::Install => {
            let m = manager.context(AT_LOGIN)?;
            let path = m.install()?;
            println!(
                "installed the {} ({}): sync runs in the background for every chained database",
                m.name(),
                path.display()
            );
        }
        SyncDaemonAct::Uninstall => {
            let m = manager.context(AT_LOGIN)?;
            let path = m.path()?;
            if !path.exists() {
                println!("the sync daemon is not installed");
                return Ok(());
            }
            let p = path.to_string_lossy();
            // Stopping may fail (no user bus): the file goes regardless.
            let stopped = match m {
                Manager::Systemd => ctl("systemctl", &["--user", "disable", "--now", UNIT]),
                Manager::Launchd => ctl("launchctl", &["unload", "-w", &p]),
            };
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            if let Manager::Systemd = m {
                let _ = ctl("systemctl", &["--user", "daemon-reload"]);
            }
            println!("removed the {} ({})", m.name(), path.display());
            if let Err(e) = stopped {
                eprintln!("warning: it may still be running: {e:#}");
            }
        }
        SyncDaemonAct::Status => status(manager)?,
    }
    Ok(())
}

fn status(manager: Option<Manager>) -> Result<()> {
    let Some(m) = manager else {
        println!("{AT_LOGIN}");
        return Ok(());
    };
    let path = m.path()?;
    if !path.exists() {
        println!("not installed: `datum sync daemon install` syncs every chained database in the background");
        return Ok(());
    }
    println!("installed: the {} ({})", m.name(), path.display());
    match m.running() {
        true => println!("running"),
        false => println!("not running: `datum sync daemon install` starts it again"),
    }
    let chained: Vec<String> = chained().into_iter().map(|(name, _)| name).collect();
    println!(
        "{} chained database(s){}{}",
        chained.len(),
        if chained.is_empty() { "" } else { ": " },
        chained.join(", ")
    );
    if let (Manager::Systemd, Ok(user)) = (m, std::env::var("USER")) {
        let linger = Command::new("loginctl")
            .args(["show-user", &user, "--property=Linger", "--value"])
            .output();
        if linger.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "no") {
            println!("it stops when you log out: `loginctl enable-linger` keeps it running");
        }
    }
    Ok(())
}

fn opted_out() -> bool {
    std::env::var(NO_DAEMON).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Whether `sync status` should say a chained copy has no daemon: one could
/// be installed here, none is, and `DATUM_SYNC_NO_DAEMON` is not why.
pub fn missing() -> bool {
    !opted_out()
        && Manager::here()
            .and_then(|m| m.path().ok())
            .is_some_and(|p| !p.exists())
}

/// A `connect` at work on a database (da-qr7gz): a lock on a file beside it,
/// held for the command's life and so let go when it is killed. The daemon,
/// installed once the copy is admitted, leaves the database alone until
/// then: it would bind the copy's endpoint key in a second process, and a
/// relay sends a key's traffic to whichever bound it last.
pub struct FirstSync {
    _lock: File,
    path: PathBuf,
}

impl Drop for FirstSync {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn first_sync_lock(db: &Path) -> PathBuf {
    let mut path = std::fs::canonicalize(db)
        .unwrap_or_else(|_| db.into())
        .into_os_string();
    path.push(".joining");
    path.into()
}

/// Hold `db`'s [`FirstSync`]. A database with no file has none. A second
/// `connect` on the database waits here until the first ends; where the
/// filesystem has no locks, the `connect` goes on without one.
pub fn first_sync(db: &Db) -> Result<Option<FirstSync>> {
    let Some(path) = db.conn().path().filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    let path = first_sync_lock(Path::new(path));
    let lock = File::options()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    let _ = lock.lock();
    Ok(Some(FirstSync { _lock: lock, path }))
}

/// Whether a `connect` holds `db`'s [`FirstSync`].
fn in_first_sync(db: &Path) -> bool {
    File::open(first_sync_lock(db))
        .is_ok_and(|f| matches!(f.try_lock_shared(), Err(TryLockError::WouldBlock)))
}

/// The first time this copy starts or joins a chain: install the daemon,
/// and say so — unless it is installed already, or `DATUM_SYNC_NO_DAEMON`.
/// Never fails the chain command it follows.
pub fn auto_install() {
    if opted_out() {
        return;
    }
    let Some(m) = Manager::here() else {
        eprintln!("{AT_LOGIN}");
        return;
    };
    if m.path().map_or(true, |p| p.exists()) {
        return;
    }
    match m.install() {
        Ok(_) => eprintln!(
            "sync now runs in the background ({}); `datum sync pause` to pause, `datum sync daemon uninstall` to remove",
            m.name()
        ),
        Err(e) => eprintln!("warning: background sync not installed: {e:#}"),
    }
}

/// Every database datum knows of that is in a sync chain, or was admitted
/// to one and has its first sync to finish (da-qr7gz), read without touching
/// it. Not one a `connect` is still at work on.
fn chained() -> Vec<(String, PathBuf)> {
    config::known_dbs()
        .into_iter()
        .filter(|(_, path)| {
            !in_first_sync(path)
                && Db::open_read_only(&path.to_string_lossy()).is_ok_and(|db| {
                    db.sync_chain().is_ok_and(|c| c.exists())
                        || db.sync_joining().is_ok_and(|j| !j.is_empty())
                })
        })
        .collect()
}

/// `datum sync daemon run`: `sync run` for every chained database, each
/// on a thread of its own, looking for a newly chained one every [`LOOK`].
/// One that fails is tried again at the next look; Ctrl-C ends them all, and
/// so does one whose file a newer datum migrated under it, with an error.
fn run() -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut running = BTreeSet::new();
    let mut first = true;
    loop {
        for (_, path) in chained() {
            if !running.insert(path.clone()) {
                continue;
            }
            eprintln!("{}: syncing", path.display());
            let tx = tx.clone();
            std::thread::spawn(move || {
                // The client source is per thread, as `dispatch` sets it.
                datum_core::db::set_client_source(|| config::client_id().ok());
                // A panic is a failure like any other: retried at the next look.
                // `Ok(Err)`: it opened and ran, and then stopped.
                let done = std::panic::catch_unwind(|| {
                    let db = Db::open(&path.to_string_lossy())?;
                    anyhow::Ok(datum_sync::run(&db))
                })
                .unwrap_or_else(|_| Err(anyhow::anyhow!("sync panicked")));
                let _ = tx.send((path, done));
            });
        }
        if std::mem::take(&mut first) && running.is_empty() {
            eprintln!("no chained database yet: looking again every minute");
        }
        loop {
            match rx.recv_timeout(LOOK) {
                Ok((_, Ok(Ok(())))) => return Ok(()),
                // A newer datum migrated its file under it (da-1tnup): this
                // build is the wrong one, and the unit starts the installed.
                Ok((path, Ok(Err(e))))
                    if matches!(e.downcast_ref(), Some(DatumError::IncompatibleSchema(_))) =>
                {
                    return Err(e.context(path.display().to_string()));
                }
                Ok((path, Ok(Err(e)) | Err(e))) => {
                    eprintln!("{}: {e:#}", path.display());
                    running.remove(&path);
                }
                Err(_) => break,
            }
        }
    }
}
