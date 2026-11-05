//! `datum install | uninstall | upgrade` (da-uphgn; vividn, 2026-10-21: «yes
//! sounds good re install proposal»): with prebuilt binaries the binary is
//! its own installer. `install` sets up tab completion, and the sync daemon
//! when a database syncs, and records which binary it set up; `uninstall`
//! undoes it and keeps the record; `upgrade` replaces that binary with the
//! newest release, or with `--to` a kept one (da-ec8y8). `datum check`'s
//! install section reads the same record
//! ([`findings`]).
//!
//! `upgrade --auto` installs a systemd user timer that runs `datum upgrade`
//! daily, on the backup daemon's pattern, and `install` offers it (da-f7ckm;
//! vividn, 2026-11-04: «offered»).

use std::io::IsTerminal;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, ensure, Context, Result};
use datum_parse::cli::{InstallArgs, Shell};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::question::{is_yes, Question};
use super::sync_daemon::{ctl, linger_note, quoted, ENV};
use super::{alias, backup_daemon, completions, sync_daemon};
use crate::args::SyncDaemonAct;
use crate::config;

/// The release `datum.day/install.sh` installs from; `DATUM_RELEASE_URL`
/// names another, as it does there. A kept release, `v<version>`, is beside
/// it (release/publish.sh).
const RELEASE: &str = "https://github.com/vividn/datum/releases/download/latest";

/// The daily upgrade's timer and service: not `datum-upgrade`, a name a
/// machine may already give a timer of its own.
const AUTO: &str = "datum-auto-upgrade";

/// The release `upgrade` reads: the latest, or the kept one of `to`.
fn release_url(to: Option<&str>) -> String {
    let latest = std::env::var("DATUM_RELEASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| RELEASE.into());
    match to {
        None => latest,
        Some(v) => {
            let latest = latest.trim_end_matches('/');
            let dir = latest.rsplit_once('/').map_or(latest, |(dir, _)| dir);
            format!("{dir}/v{v}")
        }
    }
}

/// The key a release's SHA256SUMS is signed with (da-0b55k). A binary
/// trusts this one alone, whatever serves the download.
const RELEASE_KEY: &str = include_str!("../../../../release/minisign.pub");

fn release_key() -> PublicKey {
    // A test build signs its releases with a throwaway key; a release build
    // has no such door.
    #[cfg(feature = "test-util")]
    if let Ok(key) = std::env::var("DATUM_TEST_RELEASE_KEY") {
        return PublicKey::from_base64(&key).expect("DATUM_TEST_RELEASE_KEY is a minisign key");
    }
    PublicKey::decode(RELEASE_KEY).expect("release/minisign.pub is a minisign key")
}

/// What `datum install` set up, kept beside the databases.
#[derive(Serialize, Deserialize)]
struct Installed {
    exe: PathBuf,
    /// The shell tab completion was installed for; none with
    /// `--no-completions`.
    completions: Option<String>,
}

fn record_path() -> Result<PathBuf> {
    Ok(config::data_dir_path()
        .context("HOME is not set")?
        .join("installed.toml"))
}

fn installed() -> Option<Installed> {
    toml::from_str(&std::fs::read_to_string(record_path().ok()?).ok()?).ok()
}

fn record(installed: &Installed) -> Result<()> {
    let path = record_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&path, toml::to_string(installed)?)
        .with_context(|| format!("writing {}", path.display()))
}

fn this_exe() -> Result<PathBuf> {
    std::env::current_exe().context("finding this datum binary")
}

pub fn install(args: InstallArgs) -> Result<()> {
    let exe = this_exe()?;
    let shell = args
        .shell
        .or_else(|| completions::ShellEnv::from_process()?.shell);
    if !args.no_completions {
        match completions::install_for(shell) {
            Ok(said) => print!("{said}"),
            Err(e) => eprintln!("warning: tab completion not installed: {e:#}"),
        }
    }
    match sync_daemon::ensure(args.daemon) {
        Ok(said) => said.iter().for_each(|s| println!("{s}")),
        Err(e) if args.daemon => return Err(e),
        Err(e) => eprintln!("warning: the sync daemon was not installed: {e:#}"),
    }
    if let Err(e) = backup_daemon::offer(args.auto_upgrade, |q| ask(q, "back up")) {
        eprintln!("warning: backups on a schedule were not turned on: {e:#}");
    }
    // The shell is recorded even when its script could not be written, so
    // `datum check` says it is missing until it is not.
    record(&Installed {
        exe: exe.clone(),
        completions: shell
            .filter(|_| !args.no_completions)
            .map(|s| s.name().into()),
    })?;
    println!("installed {}", exe.display());
    if let Err(e) = offer_auto(&exe, args.auto_upgrade) {
        eprintln!("warning: daily upgrades were not turned on: {e:#}");
    }
    let rc = match shell {
        Some(Shell::Zsh) => Some(("zsh", "${ZDOTDIR:-$HOME}/.zshrc")),
        Some(Shell::Bash) => Some(("bash", "~/.bashrc")),
        _ => None,
    };
    if let Some((shell, rc)) = rc {
        println!(
            "to run your aliases as bare words (`mood happy`): datum alias install {shell} >> {rc}"
        );
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let exe = this_exe()?;
    let step = |name: &str, done: Result<()>| {
        if let Err(e) = done {
            eprintln!("warning: removing the {name} failed: {e:#}; the other steps still run");
        }
    };
    step("sync daemon", sync_daemon::daemon(SyncDaemonAct::Uninstall));
    step("backup daemon", backup_daemon::uninstall_all());
    if auto_on() {
        step("daily upgrade timer", auto(false));
    }
    step(
        "tab completion",
        completions::uninstall_all().map(|s| print!("{s}")),
    );
    for shell in ["zsh", "bash"] {
        step("alias loop", alias::uninstall_every(shell));
    }
    // Only a binary `datum install` set up, or cargo installed: one a
    // package manager put in place is that package manager's to remove.
    let recorded = installed().is_some_and(|i| i.exe == exe);
    let ours = recorded || cargo_root(&exe).is_some();
    match ours {
        true => step("binary", remove_binary(&exe)),
        false => println!(
            "left {}: `datum install` did not set it up, so whatever installed it removes it",
            exe.display()
        ),
    }
    // Another binary's record stays: it is still installed.
    if let Some(path) = recorded.then(record_path).and_then(Result::ok) {
        let _ = std::fs::remove_file(path);
    }
    let shown = |p: Option<PathBuf>| p.map_or("?".into(), |p| p.display().to_string());
    let (data, conf) = (
        shown(config::data_dir_path()),
        shown(config::config_dir_path()),
    );
    println!(
        "\nKept your record:\n  data    {data}\n  config  {conf} (dbs.toml there may name databases elsewhere)\n\
         To remove it too, by hand: rm -r '{data}' '{conf}'"
    );
    Ok(())
}

/// The install root cargo put `exe` in, if it did: cargo lists what it
/// installed in `.crates.toml` there, beside `bin/`. A release datum
/// `upgrade` put in place of cargo's is no longer listed.
fn cargo_root(exe: &Path) -> Option<&Path> {
    let root = exe.parent()?.parent()?;
    let listed = std::fs::read_to_string(root.join(".crates.toml")).ok()?;
    listed
        .lines()
        .any(|l| l.starts_with("\"datum-cli "))
        .then_some(root)
}

fn cargo_uninstall(root: &Path) -> Result<()> {
    let ok = Command::new("cargo")
        .args(["uninstall", "--root"])
        .arg(root)
        .arg("datum-cli")
        .status()
        .context("running cargo uninstall")?
        .success();
    ensure!(ok, "`cargo uninstall` failed");
    Ok(())
}

fn remove_binary(exe: &Path) -> Result<()> {
    match cargo_root(exe) {
        Some(root) => cargo_uninstall(root)?,
        None => std::fs::remove_file(exe).with_context(|| format!("removing {}", exe.display()))?,
    }
    println!("removed {}", exe.display());
    Ok(())
}

pub fn upgrade(check: bool, to: Option<&str>) -> Result<()> {
    let exe = this_exe()?;
    // A datum cargo built from a checkout moves onto the release (da-1yc69;
    // vividn, 2026-11-04: «it uses the public key in the checkedout version,
    // and then verifies the binary and switches»): verified as any upgrade
    // is, and recorded as `datum install` records, so its next upgrade is
    // an ordinary one.
    let cargo = cargo_root(&exe).map(Path::to_path_buf);
    let recorded = upgradable(&exe, cargo.is_some())?;
    // `--check` too, so it never offers an upgrade the run would refuse.
    replaceable(&exe)?;
    if check {
        if let Some(said) = auto_said() {
            println!("{said}");
        }
    }
    // `<major>.<minor>` alone is that minor's newest build (vividn,
    // 2026-11-05: «you just get the latest compatible version this way»): its
    // kept `v<major>.<minor>` once the next minor has begun, the latest while
    // it is the current one; the version check below refuses a latest of
    // another minor.
    let minor_only = to.is_some_and(|v| v.matches('.').count() == 1);
    let kept = release_url(to);
    let found = newest(&kept).or_else(|e| {
        let missing = e
            .to_string()
            .starts_with(&format!("could not download {kept}/SHA256SUMS"));
        match minor_only && missing {
            true => newest(&release_url(None)).map_err(|_| e),
            false => Err(e),
        }
    });
    let (url, want) = found.map_err(|e| match to {
        Some(v) => e.context(format!(
            "datum {v} has no kept release to install: one is kept for each build that raises \
             the sync floor, and for the build before it"
        )),
        None => e,
    })?;
    if sha256(&exe)? == want {
        match to {
            Some(v) => println!("datum is {v} already"),
            None => println!("datum is up to date"),
        }
        return Ok(());
    }
    if cargo.is_some() {
        eprintln!(
            "warning: {} was built by cargo from a checkout: upgrading moves it onto the release's \
             builds, and `scripts/install.sh` in the checkout puts a checkout build back",
            exe.display()
        );
    }
    // With `--to`, a check downloads it too, to say it is that version: a
    // minor's newest build may be the latest, which says so only when run.
    if check && to.is_none() {
        println!("a newer datum is released: `datum upgrade` installs it");
        return Ok(());
    }
    // Beside the binary, so the rename that puts it in place is atomic.
    let part = exe.with_file_name(".datum-upgrade");
    // Set once cargo has removed the old datum: the release is then the
    // only one left, and a failed rename keeps it.
    let mut kept = false;
    let swapped = (|| {
        curl(&url, Some(&part))?;
        let got = sha256(&part)?;
        ensure!(
            got == want,
            "{url} does not match SHA256SUMS (sha256 {got}, expected {want}): nothing changed"
        );
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
        // A release can list a binary that does not run here: it is not
        // put in place of one that does.
        let version = Command::new(&part)
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .with_context(|| format!("{url} does not run here: nothing changed"))?;
        // A kept release is the version it is kept for, and a minor's
        // newest build is of that minor.
        let said = String::from_utf8_lossy(&version.stdout);
        if let Some(v) = to {
            let rest = said.strip_prefix(&format!("datum {v}")).unwrap_or("");
            let of_it = match minor_only {
                true => rest.starts_with('.'),
                false => rest.starts_with(['-', ' ']),
            };
            ensure!(
                of_it,
                "{url} says it is {}, not datum {v}: nothing changed",
                said.trim()
            );
        }
        if check {
            return Ok(said.trim().to_string());
        }
        // Through cargo, so its record no longer claims the binary. It
        // goes first, since cargo would delete a release renamed in before
        // it: so this path alone has a moment with no datum in place.
        if let Some(root) = &cargo {
            cargo_uninstall(root).context("nothing changed")?;
            kept = true;
        }
        std::fs::rename(&part, &exe).with_context(|| match kept {
            true => format!(
                "cargo uninstalled {exe} but the release could not take its place: it is kept, \
                 verified, at {part}, and `mv '{part}' '{exe}'` finishes the upgrade",
                exe = exe.display(),
                part = part.display()
            ),
            false => format!("replacing {}", exe.display()),
        })?;
        Ok(String::from_utf8_lossy(&version.stdout).trim().to_string())
    })();
    if (swapped.is_err() || check) && !kept {
        let _ = std::fs::remove_file(&part);
    }
    if let (true, Some(v)) = (check, to) {
        let said = swapped?;
        println!("{said} is released: `datum upgrade --to {v}` installs it");
        return Ok(());
    }
    println!("upgraded {} to {}", exe.display(), swapped?);
    if recorded.is_none() {
        record(&Installed {
            exe: exe.clone(),
            completions: None,
        })?;
    }
    match sync_daemon::restart() {
        Ok(true) => println!("restarted the sync daemon onto it"),
        Ok(false) => {}
        Err(e) => eprintln!("warning: the sync daemon still runs the old datum: {e:#}"),
    }
    Ok(())
}

/// What `datum install` recorded of `exe`, which `upgrade` upgrades; `None`
/// for a datum `cargo` built, which it moves onto the release. Any other
/// datum is refused.
fn upgradable(exe: &Path, cargo: bool) -> Result<Option<Installed>> {
    let recorded = installed();
    match &recorded {
        Some(i) if i.exe == exe => {}
        None if cargo => {}
        Some(i) => bail!(
            "this datum ({}) is not the one `datum install` set up ({}): upgrade that one",
            exe.display(),
            i.exe.display()
        ),
        None => bail!(
            "this datum ({}) was not set up by `datum install`, so upgrade leaves it alone: \
             run `datum install` first if it is yours to upgrade",
            exe.display()
        ),
    }
    Ok(recorded)
}

/// Refused where this user cannot replace `exe` (the binary a link names,
/// as `current_exe` resolves it): an upgrade renames the new one into its
/// directory. A link to a binary root keeps current, say, is root's to
/// upgrade, and is never replaced (da-f7ckm).
fn replaceable(exe: &Path) -> Result<()> {
    let dir = exe.parent().context("this datum is in no directory")?;
    let c = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes())?;
    // SAFETY: `c` is a NUL-terminated path that outlives the call.
    // The effective uid's access, which the rename will have.
    let ok =
        unsafe { libc::faccessat(libc::AT_FDCWD, c.as_ptr(), libc::W_OK, libc::AT_EACCESS) } == 0;
    ensure!(
        ok,
        "this datum ({}) is not yours to replace: you cannot write {}, so whoever put it there \
         upgrades it",
        exe.display(),
        dir.display()
    );
    Ok(())
}

/// Where the timer's units go: systemd's user directory, on Linux alone.
fn units_dir() -> Result<PathBuf> {
    ensure!(
        cfg!(target_os = "linux"),
        "no systemd here, and `datum upgrade --auto` installs a systemd user timer: \
         run `datum upgrade` daily from what starts jobs here"
    );
    Ok(config::xdg_config_dir()
        .context("HOME is not set")?
        .join("systemd/user"))
}

fn auto_on() -> bool {
    units_dir().is_ok_and(|d| d.join(format!("{AUTO}.timer")).exists())
}

/// `upgrade --check`'s word on the timer; none where there is no systemd.
fn auto_said() -> Option<String> {
    units_dir().ok()?;
    Some(match auto_on() {
        true if backup_daemon::timer_stopped(AUTO) => format!(
            "daily upgrades: installed but not running: \
             `systemctl --user enable --now {AUTO}.timer` starts them"
        ),
        true => {
            format!("daily upgrades: on ({AUTO}.timer); `datum upgrade --no-auto` turns them off")
        }
        false => "daily upgrades: off; `datum upgrade --auto` turns them on".into(),
    })
}

/// `upgrade --auto | --no-auto`: the daily upgrade timer installed, or removed.
pub fn auto(on: bool) -> Result<()> {
    let dir = units_dir()?;
    let timer = format!("{AUTO}.timer");
    if !on {
        if !dir.join(&timer).exists() {
            println!("no daily upgrade timer installed");
            return Ok(());
        }
        // Stopping may fail (no user bus): the files go regardless.
        let stopped = ctl("systemctl", &["--user", "disable", "--now", &timer]);
        for kind in ["timer", "service"] {
            match std::fs::remove_file(dir.join(format!("{AUTO}.{kind}"))) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("removing {AUTO}.{kind}"))
                }
                _ => {}
            }
        }
        let _ = ctl("systemctl", &["--user", "daemon-reload"]);
        let _ = ctl(
            "systemctl",
            &["--user", "reset-failed", &format!("{AUTO}.service")],
        );
        println!(
            "removed the systemd user timer {timer}: datum upgrades when you run `datum upgrade`"
        );
        if let Err(e) = stopped {
            eprintln!("warning: it may still be running: {e:#}");
        }
        return Ok(());
    }
    let exe = this_exe()?;
    ensure!(
        cargo_root(&exe).is_none(),
        "this datum ({}) was built by cargo from a checkout, and a daily upgrade would move it onto \
         the release's builds unasked: `datum upgrade` moves it once, saying so, and then \
         `datum upgrade --auto` upgrades it daily",
        exe.display()
    );
    upgradable(&exe, false)?;
    replaceable(&exe).context("so no daily upgrade timer for it")?;
    let env: String = (ENV.iter().chain(&["DATUM_RELEASE_URL"]))
        .filter_map(|k| Some((k, std::env::var(k).ok().filter(|v| !v.is_empty())?)))
        .map(|(k, v)| format!("Environment=\"{k}={}\"\n", quoted(&v)))
        .collect();
    let described = "[Unit]\nDescription=datum upgrade, daily\n";
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let service = dir.join(format!("{AUTO}.service"));
    std::fs::write(
        &service,
        format!(
            "{described}\n[Service]\nType=oneshot\nExecStart=\"{}\" upgrade\n{env}",
            quoted(&exe.display().to_string())
        ),
    )
    .with_context(|| format!("writing {}", service.display()))?;
    // A day missed while the machine was off runs once it is on, each at a
    // random hour's delay so installs do not all ask the release at once.
    std::fs::write(
        dir.join(&timer),
        format!(
            "{described}\n[Timer]\nOnCalendar=daily\nRandomizedDelaySec=1h\nPersistent=true\n\n\
             [Install]\nWantedBy=timers.target\n"
        ),
    )
    .with_context(|| format!("writing {timer}"))?;
    ctl("systemctl", &["--user", "daemon-reload"])
        .and_then(|()| ctl("systemctl", &["--user", "enable", "--now", &timer]))
        .with_context(|| {
            format!("wrote {timer}, but could not start it: `datum upgrade --no-auto` removes it")
        })?;
    println!(
        "installed the systemd user timer {timer} ({}): `datum upgrade` daily; \
         `datum upgrade --no-auto` removes it",
        dir.display()
    );
    linger_note();
    Ok(())
}

/// `datum install`'s offer of the daily upgrade, `answer` its `--yes` or
/// `--no`. A timer already on is put onto this binary unasked; a datum cargo
/// built, or a system with no systemd, is offered nothing.
fn offer_auto(exe: &Path, answer: Option<bool>) -> Result<()> {
    if units_dir().is_err()
        || cargo_root(exe).is_some()
        || replaceable(exe).is_err()
        || answer == Some(false)
    {
        return Ok(());
    }
    match answer {
        // `--yes`, or a timer already on, put onto this binary unasked.
        Some(_) => auto(true),
        None if auto_on()
            || ask(
                "Upgrade datum daily, as releases come out?",
                "upgrade daily",
            )?
            .unwrap_or(false) =>
        {
            auto(true)
        }
        None => {
            println!("daily upgrades: off; `datum upgrade --auto` turns them on");
            Ok(())
        }
    }
}

/// One of `datum install`'s offers, asked of a person at a terminal, yes the
/// default: on stdin, or on the terminal itself where stdin is install.sh's
/// own script (`curl … | sh`), never read. With no one to ask, `None`, which
/// each offer takes as no (vividn, 2026-11-05: «when run interactively, it
/// should ask for both. Otherwise I guess the default should be no to either
/// of those»).
fn ask(question: &str, label: &str) -> Result<Option<bool>> {
    let question = Question::yes_no(question, label, Some(true));
    let answer = match std::io::stdin().is_terminal() {
        true => question.ask()?,
        false => match (std::io::stderr().is_terminal())
            .then(|| std::fs::File::open("/dev/tty").ok())
            .flatten()
        {
            Some(tty) => question.ask_on(&mut std::io::BufReader::new(tty), true)?,
            None => return Ok(None),
        },
    };
    Ok(Some(is_yes(&answer)))
}

/// This platform's binary in `release`, and its SHA256SUMS line.
fn newest(release: &str) -> Result<(String, String)> {
    let os = match std::env::consts::OS {
        "linux" | "macos" => std::env::consts::OS,
        _ => "",
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" | "aarch64" => std::env::consts::ARCH,
        _ => "",
    };
    ensure!(
        !os.is_empty() && !arch.is_empty(),
        "no prebuilt datum for this platform: build it from a checkout"
    );
    let asset = format!("datum-{arch}-{os}");
    let sums = curl(&format!("{release}/SHA256SUMS"), None)?;
    // The build's own name, `datum-<arch>-<os>-<sha7>`, signed by that
    // build's `SHA256SUMS-<sha7>.minisig`: a release cut short, or a cache
    // serving the previous SHA256SUMS, gives the previous build's, still
    // there (release/publish.sh, da-mzhau). One from before build names
    // lists only `datum-<arch>-<os>`, read where no build name is, under
    // `SHA256SUMS.minisig`.
    let build = |name: &str| {
        name.strip_prefix(asset.as_str())
            .and_then(|b| b.strip_prefix('-'))
            .is_some_and(|b| !b.is_empty() && b.bytes().all(|c| c.is_ascii_hexdigit()))
    };
    let text = String::from_utf8_lossy(&sums);
    let line = |wanted: &dyn Fn(&str) -> bool| {
        text.lines()
            .find_map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
                // `*` marks a sum `sha256sum -b` made.
                [sum, name] if wanted(name.trim_start_matches('*')) => {
                    Some((sum.to_lowercase(), name.trim_start_matches('*').to_string()))
                }
                _ => None,
            })
    };
    let (want, name) = line(&build)
        .or_else(|| line(&|name| name == asset))
        .with_context(|| format!("SHA256SUMS lists no {asset}"))?;
    let signature = match name.strip_prefix(&format!("{asset}-")) {
        Some(b) => format!("SHA256SUMS-{b}.minisig"),
        None => "SHA256SUMS.minisig".into(),
    };
    let sig = curl(&format!("{release}/{signature}"), None)
        .with_context(|| format!("SHA256SUMS is not signed: the release has no {signature}"))?;
    Signature::decode(&String::from_utf8_lossy(&sig))
        .and_then(|sig| release_key().verify(&sums, &sig, false))
        .map_err(|e| {
            anyhow!("SHA256SUMS's signature does not verify with the key this datum was built with: {e}")
        })?;
    ensure!(
        want.len() == 64 && want.bytes().all(|b| b.is_ascii_hexdigit()),
        "SHA256SUMS's line for {name} holds no sha256: {want}"
    );
    Ok((format!("{release}/{name}"), want))
}

/// datum carries no HTTP client: `curl` downloads, to `out` or to memory.
fn curl(url: &str, out: Option<&Path>) -> Result<Vec<u8>> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--max-time", "600"]);
    if let Some(out) = out {
        cmd.arg("-o").arg(out);
    }
    let got = cmd.arg(url).output().context("running curl")?;
    ensure!(
        got.status.success(),
        "could not download {url}: {}",
        String::from_utf8_lossy(&got.stderr).trim()
    );
    Ok(got.stdout)
}

fn sha256(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// `datum check`'s install section: what `datum install` set up and is
/// gone, a sync daemon on another binary or none while the database
/// `syncs`, and, with `update_check = true` in datumrc.toml, a newer
/// release, asked of it on every run. Each is a category and its message;
/// `None`, the section not run, for a datum `datum install` did not set up.
pub fn findings(syncs: bool) -> Option<Vec<(&'static str, String)>> {
    let record = installed()?;
    let mut found = Vec::new();
    let shell = record.completions.as_deref().and_then(Shell::from_name);
    if let Some(shell) = shell.filter(|&s| completions::missing(s)) {
        found.push((
            "install_completion_missing",
            format!(
                "no {} tab completion installed: `datum install` puts it back",
                shell.name()
            ),
        ));
    }
    found.extend(sync_daemon::findings(&record.exe, syncs));
    let wanted = config::load_raw_config_or_default().is_ok_and(|c| c.update_check == Some(true));
    if wanted {
        match newest(&release_url(None)).and_then(|(_, want)| Ok(sha256(&record.exe)? != want)) {
            Ok(false) => {}
            Ok(true) => found.push((
                "install_update",
                "a newer datum is released: `datum upgrade` installs it".into(),
            )),
            Err(e) => found.push((
                "install_update_check",
                format!("could not check for a newer datum: {e:#}"),
            )),
        }
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    /// The release key is published beside the release too, and each copy
    /// is the one built in.
    #[test]
    fn the_release_key_is_quoted_where_it_is_published() {
        super::release_key();
        let line = super::RELEASE_KEY.lines().nth(1).unwrap();
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        for file in ["README.md", "web/static/install.sh", "web/static/agents.md"] {
            let text = std::fs::read_to_string(format!("{root}/{file}")).unwrap();
            assert!(text.contains(line), "{file} does not quote {line}");
        }
    }
}
