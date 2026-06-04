//! `datum sync install-daemon` — write + enable the per-platform user
//! service unit that launches `datum-sync-agent` on login.
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

/// Render the textual unit file for `platform`, pointing the ExecStart
/// at `binary`. Pure function; tests rely on it.
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
         ExecStart={exec}\n\
         Restart=on-failure\n\
         RestartSec=5s\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exec = binary.display(),
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

/// Where the unit lives on disk for `platform`.
pub fn unit_path(platform: Platform) -> Result<PathBuf> {
    match platform {
        Platform::SystemdUser => {
            let base = if let Ok(v) = std::env::var("XDG_CONFIG_HOME") {
                PathBuf::from(v)
            } else {
                let home = std::env::var("HOME").context("HOME not set")?;
                PathBuf::from(home).join(".config")
            };
            Ok(base.join("systemd/user/datum-sync-agent.service"))
        }
        Platform::LaunchdUser => {
            let home = std::env::var("HOME").context("HOME not set")?;
            Ok(PathBuf::from(home).join("Library/LaunchAgents/com.datum.sync-agent.plist"))
        }
    }
}

/// Locate the `datum-sync-agent` binary. Order:
/// 1. `DATUM_SYNC_AGENT_BIN` env override (for tests + packaging),
/// 2. sibling of the current exe (the common case — `cargo install`
///    drops both binaries in the same `bin/` dir),
/// 3. bare name (relies on `$PATH`).
pub fn resolve_agent_binary() -> PathBuf {
    if let Ok(v) = std::env::var("DATUM_SYNC_AGENT_BIN") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            let candidate = dir.join("datum-sync-agent");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from("datum-sync-agent")
}

/// Refuse to install a unit pointing at a binary that can't launch:
/// systemd requires absolute `ExecStart` paths, and a PATH-relative
/// launchd agent is just as fragile. Called before writing the unit;
/// `--show` skips it so the template stays inspectable.
fn validate_agent_binary(binary: &Path) -> Result<()> {
    if binary.is_absolute() && binary.exists() {
        return Ok(());
    }
    // For the bare-name fallback, report the sibling path we actually probed.
    let looked_at = if binary.is_absolute() {
        binary.to_path_buf()
    } else {
        std::env::current_exe()
            .ok()
            .and_then(|me| me.parent().map(|d| d.join("datum-sync-agent")))
            .unwrap_or_else(|| binary.to_path_buf())
    };
    anyhow::bail!(
        "datum-sync-agent binary not found (looked at {}); build/install it \
         or set DATUM_SYNC_AGENT_BIN=<absolute path>",
        looked_at.display(),
    );
}

/// CLI entry point for `datum sync install-daemon`.
pub fn run(show: bool, uninstall: bool) -> Result<()> {
    let platform = Platform::detect()?;
    let binary = resolve_agent_binary();

    if uninstall {
        let path = do_uninstall(platform)?;
        println!("✓ Uninstalled {}", path.display());
        return Ok(());
    }

    if show {
        println!(
            "# {} — {}",
            path_label(platform),
            unit_path(platform)?.display()
        );
        print!("{}", render_unit(platform, &binary));
        return Ok(());
    }

    validate_agent_binary(&binary)?;
    let path = do_install(platform, &binary)?;
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

fn do_install(platform: Platform, binary: &Path) -> Result<PathBuf> {
    let path = unit_path(platform)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create unit dir {parent:?}"))?;
    }
    let body = render_unit(platform, binary);
    // Write-then-rename so a crash mid-write never leaves a truncated unit.
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body).with_context(|| format!("write unit {tmp:?}"))?;
    fs::rename(&tmp, &path).with_context(|| format!("install unit {path:?}"))?;
    enable_unit(platform)?;
    Ok(path)
}

fn do_uninstall(platform: Platform) -> Result<PathBuf> {
    let path = unit_path(platform)?;
    let _ = disable_unit(platform);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("remove unit {path:?}"))?;
    }
    Ok(path)
}

fn enable_unit(platform: Platform) -> Result<()> {
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
            let p = unit_path(platform)?;
            let domain = launchd_domain();
            let label = format!("{domain}/com.datum.sync-agent");
            // Re-install: bootstrap fails on an already-loaded service.
            let _ = run_cmd("launchctl", &["bootout", &label]);
            run_cmd("launchctl", &["enable", &label])?;
            run_cmd("launchctl", &["bootstrap", &domain, &p.to_string_lossy()])?;
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

    /// Serializes tests that mutate process env vars (XDG_CONFIG_HOME, HOME,
    /// DATUM_SYNC_AGENT_BIN) — plain `cargo test` runs tests as threads in
    /// one process. nextest isolates per-process, so this is belt-and-braces.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn systemd_unit_contains_exec_and_install_section() {
        let binary = PathBuf::from("/usr/local/bin/datum-sync-agent");
        let body = render_unit(Platform::SystemdUser, &binary);
        assert!(body.contains("ExecStart=/usr/local/bin/datum-sync-agent"));
        assert!(body.contains("[Install]"));
        assert!(body.contains("WantedBy=default.target"));
        assert!(body.contains("Restart=on-failure"));
    }

    #[test]
    fn launchd_plist_contains_exec_and_label() {
        let binary = PathBuf::from("/opt/datum/datum-sync-agent");
        let body = render_unit(Platform::LaunchdUser, &binary);
        assert!(body.contains("com.datum.sync-agent"));
        assert!(body.contains("/opt/datum/datum-sync-agent"));
        assert!(body.contains("RunAtLoad"));
    }

    #[test]
    fn validate_rejects_bare_name_and_missing_paths() {
        // Bare name (the PATH fallback) — non-absolute, must be rejected.
        let err = validate_agent_binary(Path::new("datum-sync-agent")).unwrap_err();
        assert!(err.to_string().contains("DATUM_SYNC_AGENT_BIN"));
        // Absolute but nonexistent (bad env override) — also rejected,
        // and the message names the path we looked at.
        let err = validate_agent_binary(Path::new("/nonexistent/datum-sync-agent")).unwrap_err();
        assert!(err.to_string().contains("/nonexistent/datum-sync-agent"));
        assert!(err.to_string().contains("DATUM_SYNC_AGENT_BIN"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn install_without_agent_binary_errors_and_writes_no_unit() {
        // No env override, no sibling binary next to the test executable →
        // run(false, false) must bail directively BEFORE writing the unit
        // (and before invoking systemctl/launchctl).
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev_bin = std::env::var("DATUM_SYNC_AGENT_BIN").ok();
        let prev_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        let prev_home = std::env::var("HOME").ok();
        std::env::remove_var("DATUM_SYNC_AGENT_BIN");
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        std::env::set_var("HOME", tmp.path());

        let err = run(false, false).unwrap_err();

        let unit = tmp.path().join("systemd/user/datum-sync-agent.service");
        let plist = tmp
            .path()
            .join("Library/LaunchAgents/com.datum.sync-agent.plist");
        let unit_written = unit.exists();
        let plist_written = plist.exists();

        // Restore env before asserting so a failure doesn't leak a deleted
        // tempdir path into sibling tests (plain `cargo test` shares the
        // process).
        let restore = |key: &str, prev: Option<String>| match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        };
        restore("DATUM_SYNC_AGENT_BIN", prev_bin);
        restore("XDG_CONFIG_HOME", prev_xdg);
        restore("HOME", prev_home);

        assert!(err.to_string().contains("DATUM_SYNC_AGENT_BIN"));
        assert!(err.to_string().contains("not found"));
        assert!(!unit_written, "unit file must not be written: {unit:?}");
        assert!(!plist_written, "plist must not be written: {plist:?}");
    }

    #[test]
    fn systemd_unit_path_honors_xdg_config_home() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-cfg-install-test");
        let p = unit_path(Platform::SystemdUser).unwrap();
        assert_eq!(
            p,
            PathBuf::from("/tmp/xdg-cfg-install-test/systemd/user/datum-sync-agent.service")
        );
        match prev {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}
