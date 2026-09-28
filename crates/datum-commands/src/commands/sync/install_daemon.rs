//! `datum sync install-daemon` — write + enable the per-platform user
//! service unit that launches the sync daemon on login.
//!
//! Platforms:
//!
//! | OS     | Framework | Unit path                                                     |
//! |--------|-----------|---------------------------------------------------------------|
//! | Linux  | systemd   | `$XDG_CONFIG_HOME/systemd/user/datum-sync-agent.service`      |
//! | macOS  | launchd   | `$HOME/Library/LaunchAgents/com.datum.sync-agent.plist`       |
//! | Other  | —         | bail with "unsupported platform; file a bead"                 |
//!
//! The unit is intentionally dumb: exec the binary on login, restart on
//! failure. All configuration stays in `datumrc.toml`.
//!
//! What it execs is `datum sync agent` — the daemon hosted inside the
//! `datum` binary — NOT a separately-installed `datum-sync-agent`. A
//! second binary sits outside whatever upgrades `datum`, so it silently
//! goes stale: it starts, finds nothing to do under an older config
//! shape, and idles forever with no error (da-ge3e). One binary cannot
//! drift from itself.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    SystemdUser,
    LaunchdUser,
}

impl Platform {
    pub fn detect() -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            Ok(Platform::SystemdUser)
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Platform::LaunchdUser)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            anyhow::bail!(
                "install-daemon not supported on this OS yet — file a bead \
                 if you need Windows / BSD support"
            );
        }
    }
}

/// Arguments appended to the `datum` binary in the unit's exec line.
const AGENT_ARGV: [&str; 2] = ["sync", "agent"];

/// Render the textual unit file for `platform`, pointing the ExecStart
/// at `datum sync agent`. Pure function; tests rely on it.
pub fn render_unit(platform: Platform, binary: &Path) -> String {
    match platform {
        Platform::SystemdUser => render_systemd(binary),
        Platform::LaunchdUser => render_launchd(binary),
    }
}

fn render_systemd(binary: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=datum sync daemon (push-on-write + periodic pull)\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         ExecStart=\"{exec}\" {args}\n\
         Restart=on-failure\n\
         RestartSec=5s\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exec = binary.display(),
        args = AGENT_ARGV.join(" "),
    )
}

fn render_launchd(binary: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.datum.sync-agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exec}</string>
        <string>sync</string>
        <string>agent</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
</dict>
</plist>
"#,
        exec = binary.display(),
    )
}

/// The directories a unit path is resolved against. The CLI reads them
/// from the environment; a test passes a tempdir instead of setting `HOME`,
/// which every other test in the process would see (da-25oa).
pub struct UserDirs {
    pub home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
}

impl UserDirs {
    pub fn from_env() -> Self {
        UserDirs {
            home: std::env::var_os("HOME").map(PathBuf::from),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        }
    }
}

/// Where the unit lives on disk for `platform`.
pub fn unit_path(platform: Platform, dirs: &UserDirs) -> Result<PathBuf> {
    let home = || dirs.home.clone().context("HOME not set");
    match platform {
        Platform::SystemdUser => {
            let base = match &dirs.xdg_config_home {
                Some(v) => v.clone(),
                None => home()?.join(".config"),
            };
            Ok(base.join("systemd/user/datum-sync-agent.service"))
        }
        Platform::LaunchdUser => {
            Ok(home()?.join("Library/LaunchAgents/com.datum.sync-agent.plist"))
        }
    }
}

/// Locate the `datum` binary the unit should exec. Order:
/// 1. `DATUM_BIN` env override (for tests + packaging),
/// 2. the currently-running executable — i.e. the very `datum` the user
///    typed `install-daemon` into, which is the one their upgrade path
///    maintains.
///
/// There is deliberately no `$PATH` fallback: systemd needs an absolute
/// `ExecStart`, and a unit that resolves through `$PATH` picks up a
/// different binary than the shell does.
pub fn resolve_agent_binary() -> PathBuf {
    if let Ok(v) = std::env::var("DATUM_BIN") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("datum"))
}

/// Refuse to install a unit pointing at a binary that can't launch:
/// systemd requires absolute `ExecStart` paths, and a PATH-relative
/// launchd agent is just as fragile. Called before writing the unit;
/// `--show` skips it so the template stays inspectable.
fn validate_agent_binary(binary: &Path) -> Result<()> {
    if binary.is_absolute() && binary.exists() {
        return Ok(());
    }
    anyhow::bail!(
        "datum binary not found at an absolute path (looked at {}); \
         set DATUM_BIN=<absolute path to datum>",
        binary.display(),
    );
}

/// CLI entry point for `datum sync install-daemon`.
pub fn run(show: bool, uninstall: bool, dirs: &UserDirs) -> Result<()> {
    let platform = Platform::detect()?;
    let binary = resolve_agent_binary();

    if uninstall {
        let path = do_uninstall(platform, dirs)?;
        println!("✓ Uninstalled {}", path.display());
        return Ok(());
    }

    if show {
        println!(
            "# {} — {}",
            path_label(platform),
            unit_path(platform, dirs)?.display()
        );
        print!("{}", render_unit(platform, &binary));
        return Ok(());
    }

    validate_agent_binary(&binary)?;
    let path = do_install(platform, &binary, dirs)?;
    println!("✓ Installed unit at {}", path.display());
    println!(
        "✓ Daemon enabled; the agent will auto-start next login. Run \
         `datum sync status` to confirm it's alive."
    );
    Ok(())
}

fn path_label(platform: Platform) -> &'static str {
    match platform {
        Platform::SystemdUser => "systemd user unit",
        Platform::LaunchdUser => "launchd user plist",
    }
}

fn do_install(platform: Platform, binary: &Path, dirs: &UserDirs) -> Result<PathBuf> {
    let path = unit_path(platform, dirs)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create unit dir {parent:?}"))?;
    }
    let body = render_unit(platform, binary);
    // Write-then-rename so a crash mid-write never leaves a truncated unit.
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body).with_context(|| format!("write unit {tmp:?}"))?;
    fs::rename(&tmp, &path).with_context(|| format!("install unit {path:?}"))?;
    enable_unit(platform, &path)?;
    Ok(path)
}

fn do_uninstall(platform: Platform, dirs: &UserDirs) -> Result<PathBuf> {
    let path = unit_path(platform, dirs)?;
    let _ = disable_unit(platform);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("remove unit {path:?}"))?;
    }
    Ok(path)
}

fn enable_unit(platform: Platform, path: &Path) -> Result<()> {
    match platform {
        Platform::SystemdUser => {
            run_cmd("systemctl", &["--user", "daemon-reload"])?;
            run_cmd(
                "systemctl",
                &["--user", "enable", "--now", "datum-sync-agent.service"],
            )?;
        }
        Platform::LaunchdUser => {
            // Modern spelling — `load -w` is deprecated. enable clears any
            // persisted disabled flag (what `-w` did), bootstrap loads now.
            let domain = launchd_domain();
            let label = format!("{domain}/com.datum.sync-agent");
            // Re-install: bootstrap fails on an already-loaded service.
            let _ = run_cmd("launchctl", &["bootout", &label]);
            run_cmd("launchctl", &["enable", &label])?;
            run_cmd(
                "launchctl",
                &["bootstrap", &domain, &path.to_string_lossy()],
            )?;
        }
    }
    Ok(())
}

fn disable_unit(platform: Platform) -> Result<()> {
    match platform {
        Platform::SystemdUser => {
            run_cmd(
                "systemctl",
                &["--user", "disable", "--now", "datum-sync-agent.service"],
            )?;
        }
        Platform::LaunchdUser => {
            let label = format!("{}/com.datum.sync-agent", launchd_domain());
            run_cmd("launchctl", &["bootout", &label])?;
        }
    }
    Ok(())
}

/// launchd GUI domain target for the current user, e.g. `gui/501`.
fn launchd_domain() -> String {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        format!("gui/{uid}")
    }
    #[cfg(not(unix))]
    {
        unreachable!("launchd path is only reachable on macOS")
    }
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("spawn {cmd}"))?;
    if !status.success() {
        anyhow::bail!("{cmd} {args:?} exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Tests that mutate process env vars (DATUM_BIN) serialize on the shared `env_lock` — plain
    // `cargo test` runs tests as threads in one process. nextest isolates
    // per-process, so this is belt-and-braces.
    use datum_test_util::{env_lock, ScopedEnv};

    /// The unit must exec `datum sync agent`, not a standalone
    /// `datum-sync-agent`: a second binary lives outside whatever
    /// upgrades `datum` and goes stale unnoticed (da-ge3e).
    #[test]
    fn systemd_unit_execs_datum_sync_agent() {
        let binary = PathBuf::from("/usr/local/bin/datum");
        let body = render_unit(Platform::SystemdUser, &binary);
        assert!(
            body.contains("ExecStart=\"/usr/local/bin/datum\" sync agent"),
            "unit must exec the datum binary's own agent subcommand:\n{body}"
        );
        assert!(
            !body.contains("/datum-sync-agent\n") && !body.contains("/datum-sync-agent "),
            "unit must not point at a separately-installed agent binary:\n{body}"
        );
        assert!(body.contains("[Install]"));
        assert!(body.contains("WantedBy=default.target"));
        assert!(body.contains("Restart=on-failure"));
    }

    /// A `datum` living under a path with a space must still start:
    /// systemd splits ExecStart on whitespace, so the executable is
    /// quoted (suspect E of da-ge3e).
    #[test]
    fn systemd_exec_path_with_a_space_is_quoted() {
        let binary = PathBuf::from("/home/vividn/My Tools/datum");
        let body = render_unit(Platform::SystemdUser, &binary);
        assert!(
            body.contains("ExecStart=\"/home/vividn/My Tools/datum\" sync agent"),
            "a path with a space must stay one argument:\n{body}"
        );
    }

    #[test]
    fn launchd_plist_contains_exec_and_label() {
        let binary = PathBuf::from("/opt/datum/datum");
        let body = render_unit(Platform::LaunchdUser, &binary);
        assert!(body.contains("com.datum.sync-agent"));
        assert!(body.contains("<string>/opt/datum/datum</string>"));
        assert!(body.contains("<string>sync</string>"));
        assert!(body.contains("<string>agent</string>"));
        assert!(body.contains("RunAtLoad"));
    }

    #[test]
    fn validate_rejects_bare_name_and_missing_paths() {
        // Bare name (no absolute path) — systemd can't exec it.
        let err = validate_agent_binary(Path::new("datum")).unwrap_err();
        assert!(err.to_string().contains("DATUM_BIN"));
        // Absolute but nonexistent (bad env override) — also rejected,
        // and the message names the path we looked at.
        let err = validate_agent_binary(Path::new("/nonexistent/datum")).unwrap_err();
        assert!(err.to_string().contains("/nonexistent/datum"));
        assert!(err.to_string().contains("DATUM_BIN"));
    }

    /// With no override the unit points at the running executable —
    /// the `datum` the user actually typed the command into, which is
    /// the one their upgrade path keeps current.
    #[test]
    fn resolve_defaults_to_the_running_binary() {
        let _guard = env_lock();
        let _bin = ScopedEnv::remove("DATUM_BIN");
        assert_eq!(resolve_agent_binary(), std::env::current_exe().unwrap());

        let _set = ScopedEnv::set("DATUM_BIN", "/opt/pkg/datum");
        assert_eq!(resolve_agent_binary(), PathBuf::from("/opt/pkg/datum"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn install_with_unusable_binary_errors_and_writes_no_unit() {
        // A DATUM_BIN pointing nowhere → run(false, false) must bail
        // directively BEFORE writing the unit (and before invoking
        // systemctl/launchctl).
        let _guard = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        // ScopedEnv restores on drop even if an assertion panics, so a
        // failure doesn't leak a deleted tempdir path into sibling tests
        // (plain `cargo test` shares the process).
        let _bin = ScopedEnv::set("DATUM_BIN", "/nonexistent/datum");
        let dirs = UserDirs {
            home: Some(tmp.path().to_path_buf()),
            xdg_config_home: Some(tmp.path().to_path_buf()),
        };

        let err = run(false, false, &dirs).unwrap_err();

        let unit = tmp.path().join("systemd/user/datum-sync-agent.service");
        let plist = tmp
            .path()
            .join("Library/LaunchAgents/com.datum.sync-agent.plist");
        let unit_written = unit.exists();
        let plist_written = plist.exists();

        assert!(err.to_string().contains("DATUM_BIN"));
        assert!(err.to_string().contains("not found"));
        assert!(!unit_written, "unit file must not be written: {unit:?}");
        assert!(!plist_written, "plist must not be written: {plist:?}");
    }

    #[test]
    fn systemd_unit_path_honors_xdg_config_home() {
        let dirs = UserDirs {
            home: Some(PathBuf::from("/home/someone")),
            xdg_config_home: Some(PathBuf::from("/tmp/xdg-cfg-install-test")),
        };
        let p = unit_path(Platform::SystemdUser, &dirs).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/tmp/xdg-cfg-install-test/systemd/user/datum-sync-agent.service")
        );
    }
}
