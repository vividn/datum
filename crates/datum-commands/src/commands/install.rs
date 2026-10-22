//! `datum install | uninstall | upgrade` (da-uphgn; vividn, 2026-10-21: «yes
//! sounds good re install proposal»): with prebuilt binaries the binary is
//! its own installer. `install` sets up tab completion, and the sync daemon
//! when a database syncs, and records which binary it set up; `uninstall`
//! undoes it and keeps the record; `upgrade` replaces that binary with the
//! newest release. `datum check`'s install section reads the same record
//! ([`findings`]).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, ensure, Context, Result};
use datum_parse::cli::{InstallArgs, Shell};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{alias, completions, sync_daemon};
use crate::args::SyncDaemonAct;
use crate::config;

/// The release `datum.day/install.sh` installs from; `DATUM_RELEASE_URL`
/// names another, as it does there.
const RELEASE: &str = "https://github.com/vividn/datum/releases/download/latest";

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
    let path = record_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    // The shell is recorded even when its script could not be written, so
    // `datum check` says it is missing until it is not.
    let record = Installed {
        exe: exe.clone(),
        completions: shell
            .filter(|_| !args.no_completions)
            .map(|s| s.name().into()),
    };
    std::fs::write(&path, toml::to_string(&record)?)
        .with_context(|| format!("writing {}", path.display()))?;
    println!("installed {}", exe.display());
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

/// The install root cargo put `exe` in, if it did: cargo keeps
/// `.crates.toml` there, beside `bin/`.
fn cargo_root(exe: &Path) -> Option<&Path> {
    let root = exe.parent()?.parent()?;
    root.join(".crates.toml").exists().then_some(root)
}

fn remove_binary(exe: &Path) -> Result<()> {
    match cargo_root(exe) {
        Some(root) => {
            let ok = Command::new("cargo")
                .args(["uninstall", "--root"])
                .arg(root)
                .arg("datum-cli")
                .status()
                .context("running cargo uninstall")?
                .success();
            ensure!(ok, "`cargo uninstall` failed");
        }
        None => std::fs::remove_file(exe).with_context(|| format!("removing {}", exe.display()))?,
    }
    println!("removed {}", exe.display());
    Ok(())
}

pub fn upgrade(check: bool) -> Result<()> {
    let exe = this_exe()?;
    if cargo_root(&exe).is_some() {
        bail!(
            "{} was built by cargo from a checkout: upgrade it with `git pull && scripts/install.sh`",
            exe.display()
        );
    }
    match installed() {
        Some(i) if i.exe == exe => {}
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
    let (url, want) = newest()?;
    if sha256(&exe)? == want {
        println!("datum is up to date");
        return Ok(());
    }
    if check {
        println!("a newer datum is released: `datum upgrade` installs it");
        return Ok(());
    }
    // Beside the binary, so the rename that puts it in place is atomic.
    let part = exe.with_file_name(".datum-upgrade");
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
        std::fs::rename(&part, &exe).with_context(|| format!("replacing {}", exe.display()))?;
        Ok(String::from_utf8_lossy(&version.stdout).trim().to_string())
    })();
    if swapped.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    println!("upgraded {} to {}", exe.display(), swapped?);
    match sync_daemon::restart() {
        Ok(true) => println!("restarted the sync daemon onto it"),
        Ok(false) => {}
        Err(e) => eprintln!("warning: the sync daemon still runs the old datum: {e:#}"),
    }
    Ok(())
}

/// This platform's binary in the newest release, and its SHA256SUMS line.
fn newest() -> Result<(String, String)> {
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
    let release = std::env::var("DATUM_RELEASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| RELEASE.into());
    let sums = curl(&format!("{release}/SHA256SUMS"), None)?;
    let want = String::from_utf8_lossy(&sums)
        .lines()
        .find_map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            // `*` marks a sum `sha256sum -b` made.
            [sum, name] if name.trim_start_matches('*') == asset => Some(sum.to_lowercase()),
            _ => None,
        })
        .with_context(|| format!("SHA256SUMS lists no {asset}"))?;
    ensure!(
        want.len() == 64 && want.bytes().all(|b| b.is_ascii_hexdigit()),
        "SHA256SUMS's line for {asset} holds no sha256: {want}"
    );
    Ok((format!("{release}/{asset}"), want))
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
        match newest().and_then(|(_, want)| Ok(sha256(&record.exe)? != want)) {
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
