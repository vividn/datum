//! `datum sync daemon install | uninstall | status` (da-vd2sn): keep every
//! chained database syncing in the background — a systemd user unit on
//! Linux, a launchd agent on macOS — running the hidden `sync daemon run`,
//! which is `sync run` for each of them in one process. The first `chain
//! invite` or `chain join` of a copy in no chain installs it, unless
//! `DATUM_SYNC_NO_DAEMON=1`: background work starts when the person opts
//! into the feature it serves, never at CLI install time. A `join`
//! installs it once admitted, so a first sync cut short leaves it, and the
//! daemon finishes that sync (da-qr7gz). Linger is left alone: a user
//! service stops at logout, and `status` says so.

use std::collections::BTreeSet;
use std::ffi::CString;
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
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
pub(super) const ENV: [&str; 4] = [
    "DATUM_HOME",
    "DATUM_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CONFIG_HOME",
];
/// How often `run` looks for a database newly in a chain.
const LOOK: Duration = Duration::from_secs(60);
/// How soon `run` first looks again at a database that refused its look.
const LOOK_AGAIN: Duration = Duration::from_secs(1);
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
        let exe = unit_exe()?;
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
pub(super) fn ctl(program: &str, args: &[&str]) -> Result<()> {
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

/// The path a daemon's unit names this datum by (da-tmmzh): the one it was
/// run by, as typed (a relative one read from the working directory) or
/// found on PATH, where that reaches this binary through a link, so the
/// unit follows the link to whatever version an installer puts behind it
/// (`bin/datum → versions/<v>`, which keeps two versions; Homebrew's Cellar
/// after `brew cleanup`), and only where no one but this user and root can
/// change what it reaches (`trusted`, da-d1uhd); otherwise the binary itself.
/// Refused where the binary is gone, removed or replaced as it ran: a unit
/// naming it would never start.
pub(super) fn unit_exe() -> Result<PathBuf> {
    Ok(chosen()?.0)
}

/// `unit_exe`'s path, and the link this datum was run by where it was
/// refused, with why.
fn chosen() -> Result<(PathBuf, Option<(PathBuf, String)>)> {
    let exe = std::env::current_exe().context("finding this datum binary")?;
    let this = std::fs::canonicalize(&exe).with_context(|| {
        format!(
            "this datum ({}) was removed or replaced as it ran, so a unit naming it would never \
             start: run the datum installed now",
            exe.display()
        )
    })?;
    let run_by = std::env::args_os().next().map(PathBuf::from).and_then(|a| {
        match a.as_os_str().as_encoded_bytes().contains(&b'/') {
            true => Some(a),
            false => std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|d| d.join(&a))
                .find(|p| p.is_file()),
        }
    });
    let run_by = (run_by.and_then(|p| std::path::absolute(p).ok()))
        .filter(|p| std::fs::canonicalize(p).is_ok_and(|c| c == this));
    // SAFETY: getuid cannot fail.
    let uid = unsafe { libc::getuid() };
    let private = |g| private_group(g, uid);
    Ok(
        match run_by.map(|p| (trusted(&p, &this, uid, &private), p)) {
            Some((Ok(()), p)) => (p, None),
            Some((Err(why), p)) => (exe, Some((p, why))),
            None => (exe, None),
        },
    )
}

/// Whether no one but `uid` and root can change what `link` reaches
/// (da-d1uhd): each link followed from it to `this`, and every directory
/// above each, as typed and as resolved, is owned by `uid` or root, and no
/// directory is writable by group or other unless it is sticky (`/tmp`),
/// where no one else can move what is ours, or only by a group `private`
/// says holds no one but `uid` and with no ACL, whose mask the group bits
/// would be (da-y30od: Debian's umask 002 makes `~/.local/bin` 0775).
/// Else why not.
fn trusted(
    link: &Path,
    this: &Path,
    uid: u32,
    private: &dyn Fn(u32) -> bool,
) -> std::result::Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let mut hop = link.to_path_buf();
    // `this`'s own directories go unchecked: a unit naming the file is what
    // it falls back to. A hop is checked before it is followed, so one that
    // is no link (`bin/../v1/datum`, through a linked directory) is checked
    // too. 40 hops is the kernel's own bound (a link changed into a loop
    // after `chosen` resolved it).
    for _ in 0..40 {
        if hop == this {
            return Ok(());
        }
        let dir = hop.parent().unwrap_or(Path::new("/")).to_path_buf();
        let real = std::fs::canonicalize(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for p in hop.ancestors().chain(real.ancestors()) {
            let m = std::fs::symlink_metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
            if m.uid() != uid && m.uid() != 0 {
                return Err(format!("{} belongs to another user", p.display()));
            }
            // The group's write counts unless the group is this user's alone
            // and no ACL makes the bits a mask.
            let group = m.mode() & 0o020 != 0 && !(private(m.gid()) && !has_acl(p));
            if !m.is_symlink() && (m.mode() & 0o002 != 0 || group) && m.mode() & 0o1000 == 0 {
                return Err(format!("{} is writable by group or other", p.display()));
            }
        }
        match std::fs::read_link(&hop) {
            Ok(to) => hop = dir.join(to),
            Err(_) => return Ok(()),
        }
    }
    Err(format!("{} goes through too many links", link.display()))
}

/// Whether group `gid` can hold no one but `uid` (da-y30od), by `alone`
/// over every account NSS enumerates and the group's members. It proves only
/// that this user is enumerable: an account NSS does not list whose primary
/// gid is this one (a misconfigured gid collision) is missed, and so is a
/// group password in gshadow, which lets a non-member in by `newgrp`: no
/// process but root's can read gshadow. One scan at a time: `getpwent` and
/// `getgrent` share state, and a daemon's threads each join grants.
fn private_group(gid: u32, uid: u32) -> bool {
    use std::ffi::CStr;
    static NSS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _held = NSS.lock().unwrap_or_else(|e| e.into_inner());
    let mut accounts = Vec::new();
    // SAFETY: under `NSS`, each entry is copied before the next call reuses
    // its buffer.
    let members = unsafe {
        libc::setpwent();
        loop {
            let p = libc::getpwent();
            if p.is_null() {
                break;
            }
            let name = CStr::from_ptr((*p).pw_name).to_owned();
            accounts.push(((*p).pw_uid, (*p).pw_gid, name));
        }
        libc::endpwent();
        let mut groups = Vec::new();
        libc::setgrent();
        loop {
            let g = libc::getgrent();
            if g.is_null() {
                break;
            }
            let (mut m, mut members) = ((*g).gr_mem, Vec::new());
            while !(*m).is_null() {
                members.push(CStr::from_ptr(*m).to_owned());
                m = m.add(1);
            }
            groups.push(((*g).gr_gid, members));
        }
        libc::endgrent();
        members_of(gid, groups)
    };
    members.is_some_and(|members| alone(gid, uid, &accounts, &members))
}

/// The members of every one of `groups`' lines with `gid`, or none where no
/// line has it: a second line with the gid (`groupadd -o`, an NSS source
/// merging groups) can add a member `getgrgid`, which returns the first,
/// never shows, and login gives them the gid all the same (da-txvrk).
fn members_of(gid: u32, groups: Vec<(u32, Vec<CString>)>) -> Option<Vec<CString>> {
    let mut lines = groups.into_iter().filter(|g| g.0 == gid).peekable();
    lines.peek()?;
    Some(lines.flat_map(|g| g.1).collect())
}

/// Whether `accounts` (uid, primary gid, name) and `members`, group `gid`'s
/// list, leave it no one but `uid`: `uid` is among them with `gid` its
/// primary, no other account has it as primary, and no other name is a
/// member. Where NSS does not enumerate this user (sssd and the like), the
/// scan cannot be complete, so it is not.
fn alone(gid: u32, uid: u32, accounts: &[(u32, u32, CString)], members: &[CString]) -> bool {
    let Some((_, primary, name)) = accounts.iter().find(|a| a.0 == uid) else {
        return false;
    };
    *primary == gid
        && accounts.iter().all(|a| a.0 == uid || a.1 != gid)
        && members.iter().all(|m| m == name)
}

/// Whether `p` may carry a POSIX ACL: anything but its absence (or a file
/// system with none) is taken as one.
fn has_acl(p: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(path) = CString::new(p.as_os_str().as_bytes()) else {
            return true;
        };
        let name = c"system.posix_acl_access";
        // SAFETY: both are NUL-terminated; a null buffer of 0 asks the size.
        let n = unsafe { libc::lgetxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
        let e = std::io::Error::last_os_error().raw_os_error();
        !(n < 0 && matches!(e, Some(libc::ENODATA) | Some(libc::ENOTSUP)))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = p;
        true
    }
}

/// What a status says of the path a unit names this datum by (da-d1uhd):
/// a link or the file, as the unit says, and for the file, the link this
/// datum was run by that a unit would not name as the files stand now, and
/// why. A binary gone is `binary_word`'s, said before this.
pub(super) fn chosen_word(body: &str) -> Option<String> {
    let named = named_exe(body)?;
    let shown = named.display();
    if std::fs::canonicalize(&named).ok()? != named {
        return Some(format!("it runs datum by the link {shown}"));
    }
    Some(match chosen().ok().and_then(|c| c.1) {
        Some((link, why)) => format!(
            "it runs the file {shown}, not the link {} this datum was run by: {why}",
            link.display()
        ),
        None => format!("it runs the file {shown}"),
    })
}

/// The binary a unit's or agent's text names: its `ExecStart=`'s quoted
/// first word, or its `ProgramArguments`' first string.
pub(super) fn named_exe(body: &str) -> Option<PathBuf> {
    if let Some(line) = body.lines().find_map(|l| l.strip_prefix("ExecStart=\"")) {
        let (mut s, mut chars) = (String::new(), line.chars());
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(s.replace("%%", "%").into()),
                '\\' => s.extend(chars.next()),
                c => s.push(c),
            }
        }
        return None;
    }
    let s = (body
        .split("<key>ProgramArguments</key><array><string>")
        .nth(1))?
    .split("</string>")
    .next()?;
    Some(
        (s.replace("&lt;", "<").replace("&gt;", ">"))
            .replace("&amp;", "&")
            .into(),
    )
}

/// Whether the binary a unit names is `exe`'s file, read through any link:
/// `None` where it is missing, `Some(false)` where the text names none.
fn names(body: &str, exe: &Path) -> Option<bool> {
    let Some(named) = named_exe(body) else {
        return Some(false);
    };
    let at = std::fs::canonicalize(named).ok()?;
    Some(std::fs::canonicalize(exe).is_ok_and(|e| e == at))
}

/// What a status says of the binary a unit names (da-tmmzh), where it is
/// missing or is not this datum; `fix` the command that installs it again.
/// Missing is said even where this datum cannot find itself.
pub(super) fn binary_word(body: &str, fix: &str) -> Option<String> {
    let named = named_exe(body)?;
    let shown = named.display();
    if std::fs::canonicalize(&named).is_err() {
        return Some(format!(
            "its binary {shown} is missing, so it cannot start: {fix} installs it onto this datum"
        ));
    }
    let this = std::env::current_exe().ok()?;
    (names(body, &this) == Some(false)).then(|| {
        format!(
            "it runs {shown}, which is not this datum ({}): {fix} installs it onto this one",
            this.display()
        )
    })
}

/// A value inside a systemd unit's double quotes: `%` would be a specifier.
pub(super) fn quoted(s: &str) -> String {
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
        println!(
            "not installed: `datum sync --auto` syncs every chained database in the background"
        );
        return Ok(());
    }
    println!("installed: the {} ({})", m.name(), path.display());
    let body = std::fs::read_to_string(&path).unwrap_or_default();
    let word = binary_word(&body, "`datum sync --auto`");
    if let Some(word) = word.or_else(|| chosen_word(&body)) {
        println!("{word}");
    }
    match m.running() {
        true => println!("running"),
        false => println!("not running: `datum sync --auto` starts it again"),
    }
    let chained: Vec<String> = chained().into_iter().map(|(name, _)| name).collect();
    println!(
        "{} chained database(s){}{}",
        chained.len(),
        if chained.is_empty() { "" } else { ": " },
        chained.join(", ")
    );
    if let Manager::Systemd = m {
        linger_note();
    }
    Ok(())
}

/// A user service stops at logout, unless the user lingers: say so.
pub(super) fn linger_note() {
    let Ok(user) = std::env::var("USER") else {
        return;
    };
    let linger = Command::new("loginctl")
        .args(["show-user", &user, "--property=Linger", "--value"])
        .output();
    if linger.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "no") {
        println!("it stops when you log out: `loginctl enable-linger` keeps it running");
    }
}

/// `datum install`'s part (da-uphgn): install the daemon again onto this
/// binary when one is installed; install it when a database syncs, unless
/// `DATUM_SYNC_NO_DAEMON`, or when `force`. What it did, if anything.
pub fn ensure(force: bool) -> Result<Option<String>> {
    let Some(m) = Manager::here() else {
        return match force {
            true => Err(anyhow::anyhow!(AT_LOGIN)),
            false => Ok(None),
        };
    };
    if !force && !m.path()?.exists() && (opted_out() || chained().is_empty()) {
        return Ok(None);
    }
    let path = m.install()?;
    Ok(Some(format!(
        "sync daemon: the {} ({}) runs this datum",
        m.name(),
        path.display()
    )))
}

/// Restart an installed daemon, so it runs the binary now at its path:
/// whether there was one.
pub fn restart() -> Result<bool> {
    let Some(m) = Manager::here() else {
        return Ok(false);
    };
    let path = m.path()?;
    if !path.exists() {
        return Ok(false);
    }
    match m {
        Manager::Systemd => ctl("systemctl", &["--user", "restart", UNIT])?,
        Manager::Launchd => {
            let p = path.to_string_lossy();
            let _ = ctl("launchctl", &["unload", &p]);
            ctl("launchctl", &["load", "-w", &p])?
        }
    }
    Ok(true)
}

/// What `datum check` says of the daemon for the binary `exe` (da-uphgn):
/// none installed while the database `syncs`, one set to run another
/// binary, or one running an older binary than the one now at `exe`,
/// which a replace leaves until it restarts. That last is read while the
/// daemon runs, and on systemd alone: a stopped daemon starts on what is
/// at `exe`. Each is a category and its message.
pub fn findings(exe: &Path, syncs: bool) -> Vec<(&'static str, String)> {
    let Some(m) = Manager::here() else {
        return Vec::new();
    };
    let Some(body) = m.path().ok().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return match syncs && !opted_out() {
            true => vec![(
                "install_daemon_missing",
                "this database syncs, and no sync daemon is installed: `datum install` installs it"
                    .into(),
            )],
            false => Vec::new(),
        };
    };
    let shown = exe.display().to_string();
    let word = match names(&body, exe) {
        Some(true) => None,
        None => Some(format!(
            "the sync daemon's binary is missing, so it cannot start: `datum install` points it at {shown}"
        )),
        Some(false) => Some(format!(
            "the sync daemon runs another datum than {shown}: `datum install` points it here"
        )),
    };
    if let Some(word) = word {
        return vec![("install_daemon_binary", word)];
    }
    match running_exe(m) {
        Some(running) if running != exe => vec![(
            "install_daemon_stale",
            format!(
                "the sync daemon runs an older datum than the one at {shown}: `datum install` restarts it"
            ),
        )],
        _ => Vec::new(),
    }
}

/// The binary the running daemon was started from, where the system says:
/// a replaced one reads as `<path> (deleted)`.
fn running_exe(m: Manager) -> Option<PathBuf> {
    let Manager::Systemd = m else {
        return None;
    };
    let out = Command::new("systemctl")
        .args(["--user", "show", "--property=MainPID", "--value", UNIT])
        .output()
        .ok()?;
    // 0: not running.
    let pid = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|&pid| pid > 0)?;
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

fn opted_out() -> bool {
    std::env::var(NO_DAEMON).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Whether a daemon is installed here, which keeps a pending join trying
/// (da-a2lxa).
pub fn installed() -> bool {
    Manager::here()
        .and_then(|m| m.path().ok())
        .is_some_and(|p| p.exists())
}

/// Whether `sync status` should say a chained copy has no daemon: one could
/// be installed here, none is, and `DATUM_SYNC_NO_DAEMON` is not why.
pub fn missing() -> bool {
    !opted_out()
        && Manager::here()
            .and_then(|m| m.path().ok())
            .is_some_and(|p| !p.exists())
}

/// A `join` or an `invite` at work on a database (da-qr7gz, da-apd52): a
/// lock on a file beside it, held for the command's life and so let go when
/// it is killed. The daemon leaves the database alone until then: it would
/// bind the copy's endpoint key in a second process, and a relay sends a
/// key's traffic to whichever bound it last.
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
/// `join` or `invite` on the database is refused: waiting, it would wait
/// out the first, an invite its whole expiry. Where the filesystem has no
/// locks, it goes on without one.
pub fn first_sync(db: &Db) -> Result<Option<FirstSync>> {
    let Some(path) = db.conn().path().filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    hold(Path::new(path)).map(Some)
}

/// The lock [`first_sync`] holds, on the database at `db`: a `db rename`
/// holds it too, so the daemon lets go of the file it moves.
pub(super) fn hold(db: &Path) -> Result<FirstSync> {
    let path = first_sync_lock(db);
    let lock = File::options()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("creating {}", path.display()))?;
    if let Err(TryLockError::WouldBlock) = lock.try_lock() {
        bail!(BUSY);
    }
    Ok(FirstSync { _lock: lock, path })
}

/// Why a second `join` or `invite` on a database is refused.
const BUSY: &str = "a `datum sync chain join` or `invite` is at work on this database: \
     let it end, or stop it, first";

/// Why the daemon stopped serving a database it had taken up.
const ASIDE: &str = "a `datum sync chain join` or `invite` is at work on it: \
     stepping aside until it ends";

/// Whether a `join` or an `invite` holds `db`'s [`FirstSync`].
pub(super) fn in_first_sync(db: &Path) -> bool {
    File::open(first_sync_lock(db))
        .is_ok_and(|f| matches!(f.try_lock_shared(), Err(TryLockError::WouldBlock)))
}

/// The first time this copy starts or joins a chain: keep `db` where the
/// daemon looks, and install the daemon, and say so — unless it is
/// installed already, or `DATUM_SYNC_NO_DAEMON`. Never fails the chain
/// command it follows.
pub fn auto_install(db: &Db) {
    if let Some(path) = db.conn().path().filter(|p| !p.is_empty()) {
        if let Err(e) = config::ensure_known(Path::new(path)) {
            eprintln!("warning: background sync will not find this database: {e:#}");
        }
    }
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
            "sync now runs in the background ({}); `datum sync pause` to pause, `datum sync --no-auto` to remove",
            m.name()
        ),
        Err(e) => eprintln!("warning: background sync not installed: {e:#}"),
    }
}

/// Every database datum knows of that is in a sync chain, or was admitted
/// to one and has its first sync to finish (da-qr7gz), read without touching
/// it. Not one a `join` or an `invite` is still at work on.
fn chained() -> Vec<(String, PathBuf)> {
    looked().0
}

/// [`chained`], and whether a database that is there refused the look: a
/// writer opening or closing it has a `-wal` and no `-shm` for an instant,
/// which says nothing of its chain (da-mtsgn).
fn looked() -> (Vec<(String, PathBuf)>, bool) {
    let mut refused = false;
    let found = config::known_dbs()
        .into_iter()
        .filter(|(_, path)| {
            if in_first_sync(path) {
                return false;
            }
            match Db::open_read_only(&path.to_string_lossy()) {
                Ok(db) => {
                    db.sync_chain().is_ok_and(|c| c.exists())
                        || db.sync_joining().is_ok_and(|j| !j.is_empty())
                        || db
                            .sync_join_pending()
                            .is_ok_and(|j| j.is_some_and(|j| j.refused.is_none()))
                }
                Err(_) => {
                    refused |= path.exists();
                    false
                }
            }
        })
        .collect();
    (found, refused)
}

/// How long until the next look: [`LOOK`], or while a database refuses the
/// look [`LOOK_AGAIN`] doubling up to it, since a `-wal` a crash left alone
/// refuses every look until something opens the file.
fn next_look(refused: bool, again: &mut Duration) -> Duration {
    if !refused {
        *again = LOOK_AGAIN;
        return LOOK;
    }
    let wait = *again;
    *again = (wait * 2).min(LOOK);
    wait
}

/// `datum sync daemon run`: `sync run` for every chained database, each
/// on a thread of its own, looking for a newly chained one every [`LOOK`],
/// or sooner while a database refuses the look ([`next_look`]).
/// One that fails is tried again at the next look; Ctrl-C ends them all, and
/// so does one whose file a newer datum migrated under it, with an error.
fn run() -> Result<()> {
    if let Err(e) = super::sync::kept() {
        eprintln!("⚠ the account kept here: {e:#}");
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let mut running = BTreeSet::new();
    let mut first = true;
    let mut again = LOOK_AGAIN;
    loop {
        let (found, refused) = looked();
        for (_, path) in found {
            if !running.insert(path.clone()) {
                continue;
            }
            eprintln!("{}: syncing", path.display());
            let tx = tx.clone();
            std::thread::spawn(move || {
                // The client source is per thread, as `dispatch` sets it.
                datum_core::db::set_client_source(|| config::client_id().ok());
                datum_core::db::set_device_source(|| config::device_key().ok());
                // A panic is a failure like any other: retried at the next look.
                // `Ok(Err)`: it opened and ran, and then stopped.
                let done = std::panic::catch_unwind(|| {
                    let db = Db::open(&path.to_string_lossy())?;
                    super::sync::bind_device(&db);
                    // A `join` or an `invite` taking it up meanwhile has it
                    // to itself (da-8sah2): this endpoint would serve its
                    // joiner too. Retried at a look that finds it free.
                    let grants = super::sync_grant::Watch::default();
                    let aside = || {
                        grants.tick(&db);
                        super::sync_upgrade::follow(&db, true);
                        ensure!(!in_first_sync(&path), ASIDE);
                        Ok(())
                    };
                    let up = |at| super::sync_grant::say_where(path.clone(), at);
                    anyhow::Ok(datum_sync::run(&db, up, aside))
                })
                .unwrap_or_else(|_| Err(anyhow::anyhow!("sync panicked")));
                let _ = tx.send((path, done));
            });
        }
        if std::mem::take(&mut first) && running.is_empty() && !refused {
            eprintln!("no chained database yet: looking again every minute");
        }
        let wait = next_look(refused, &mut again);
        loop {
            match rx.recv_timeout(wait) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A database that goes on refusing is looked at less and less often,
    /// never less often than any other, and one look it answers starts over.
    #[test]
    fn a_look_refused_again_and_again_backs_off_to_the_minute() {
        let mut again = LOOK_AGAIN;
        let secs: Vec<u64> = (0..8)
            .map(|_| next_look(true, &mut again).as_secs())
            .collect();
        assert_eq!(secs, [1, 2, 4, 8, 16, 32, 60, 60]);
        assert_eq!(next_look(false, &mut again), LOOK);
        assert_eq!(next_look(true, &mut again), LOOK_AGAIN);
    }

    /// A link is trusted in a directory of this user's at 0755, and not
    /// where group or other may write it, or another non-root user owns it
    /// (da-d1uhd).
    #[test]
    fn a_link_is_trusted_only_where_no_one_else_can_change_it() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        // Made under the umask: 0755, so only `bin` decides.
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let (bin, file) = (tmp.path().join("bin"), tmp.path().join("datum"));
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(&file, "").unwrap();
        let link = bin.join("datum");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let this = std::fs::canonicalize(&file).unwrap();
        let mode = |m| std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(m)).unwrap();
        // SAFETY: getuid cannot fail.
        let me = unsafe { libc::getuid() };

        mode(0o755);
        assert_eq!(trusted(&link, &this, me, &|_| false), Ok(()));
        for m in [0o775, 0o757] {
            mode(m);
            let why = trusted(&link, &this, me, &|_| false).unwrap_err();
            assert_eq!(
                why,
                format!("{} is writable by group or other", bin.display())
            );
        }
        mode(0o755);

        // `bin/datum → ../versions/datum`, `versions` a link to a directory
        // others may write: the hop through it, which is no link, is
        // checked as it is resolved.
        let shared = tmp.path().join("shared");
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o775)).unwrap();
        std::fs::write(shared.join("datum"), "").unwrap();
        let file = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(shared.join("datum"), file).unwrap();
        std::os::unix::fs::symlink(&shared, tmp.path().join("versions")).unwrap();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("../versions/datum", &link).unwrap();
        let this = std::fs::canonicalize(&link).unwrap();
        let why = trusted(&link, &this, me, &|_| false).unwrap_err();
        let shared = std::fs::canonicalize(&shared).unwrap();
        assert_eq!(
            why,
            format!("{} is writable by group or other", shared.display())
        );

        // Owned by another non-root user, last since as root it chowns
        // `bin`: as root, chown it; otherwise this user's tree is another
        // user's to a reader of another uid.
        let reader = match me {
            0 => {
                std::os::unix::fs::chown(&bin, Some(65534), None).unwrap();
                0
            }
            me => me + 1,
        };
        let why = trusted(&link, &this, reader, &|_| false).unwrap_err();
        assert!(why.ends_with("belongs to another user"), "{why}");
    }

    /// A link in a 0775 directory of this user's private group is trusted,
    /// and not where the group holds another, nor where the group bits are
    /// an ACL's mask granting another user write (da-y30od).
    #[test]
    fn a_private_group_may_write_where_no_acl_lets_another() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let tmp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let (bin, file) = (tmp.path().join("bin"), tmp.path().join("datum"));
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(&file, "").unwrap();
        let link = bin.join("datum");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let this = std::fs::canonicalize(&file).unwrap();
        // SAFETY: getuid cannot fail.
        let me = unsafe { libc::getuid() };
        let gid = std::fs::metadata(&bin).unwrap().gid();
        let ours = move |g| g == gid;

        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o775)).unwrap();
        assert_eq!(trusted(&link, &this, me, &ours), Ok(()));
        let why = trusted(&link, &this, me, &|_| false).unwrap_err();
        assert_eq!(
            why,
            format!("{} is writable by group or other", bin.display())
        );
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o757)).unwrap();
        assert!(trusted(&link, &this, me, &ours).is_err());

        #[cfg(target_os = "linux")]
        {
            // 0755 and `user:nobody:rwx`, as `setfacl -m` writes it: the mode
            // shows the mask, 0775.
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            let mut acl = 2u32.to_le_bytes().to_vec();
            for (tag, perm, id) in [
                (1u16, 7u16, u32::MAX),
                (2, 7, 65534),
                (4, 5, u32::MAX),
                (0x10, 7, u32::MAX),
                (0x20, 5, u32::MAX),
            ] {
                acl.extend(
                    tag.to_le_bytes()
                        .iter()
                        .chain(&perm.to_le_bytes())
                        .chain(&id.to_le_bytes()),
                );
            }
            let path = CString::new(bin.to_str().unwrap()).unwrap();
            let name = c"system.posix_acl_access";
            // SAFETY: NUL-terminated names, `acl` read for its length.
            let rc = unsafe {
                libc::lsetxattr(
                    path.as_ptr(),
                    name.as_ptr(),
                    acl.as_ptr().cast(),
                    acl.len(),
                    0,
                )
            };
            if rc != 0 {
                eprintln!(
                    "no ACL on this file system ({}): its case is untested",
                    std::io::Error::last_os_error()
                );
                return;
            }
            assert_eq!(std::fs::metadata(&bin).unwrap().mode() & 0o777, 0o775);
            let why = trusted(&link, &this, me, &ours).unwrap_err();
            assert_eq!(
                why,
                format!("{} is writable by group or other", bin.display())
            );
        }
    }

    /// A group is private only where the accounts and members fed say no
    /// one but this user can be in it (da-y30od).
    #[test]
    fn a_group_is_private_only_where_no_other_account_can_be_in_it() {
        let n = |s: &str| CString::new(s).unwrap();
        let me = (1001, 1001, n("me"));
        let root = (0, 0, n("root"));
        let alone = |accounts: &[(u32, u32, CString)], members: &[CString]| {
            alone(1001, 1001, accounts, members)
        };
        assert!(alone(&[root.clone(), me.clone()], &[]));
        assert!(alone(&[root.clone(), me.clone()], &[n("me")]));
        let other = (1002, 1001, n("other"));
        assert!(!alone(&[me.clone(), other], &[]), "another's primary");
        assert!(
            !alone(std::slice::from_ref(&me), &[n("me"), n("other")]),
            "another member"
        );
        assert!(!alone(&[root], &[]), "this user not enumerated");
        assert!(
            !alone(&[(1001, 100, n("me"))], &[]),
            "another primary of mine"
        );
    }

    /// Every line with the gid gives its members, a second line's too, and
    /// no line with it gives none (da-txvrk).
    #[test]
    fn a_groups_members_are_every_line_with_its_gid() {
        let n = |s: &str| CString::new(s).unwrap();
        let groups = || {
            vec![
                (1001, vec![n("me")]),
                (1002, vec![n("third")]),
                (1001, vec![n("other")]),
            ]
        };
        assert_eq!(members_of(1001, groups()), Some(vec![n("me"), n("other")]));
        assert_eq!(members_of(1003, groups()), None);
        assert_eq!(members_of(1002, vec![(1002, vec![])]), Some(vec![]));
    }
}
