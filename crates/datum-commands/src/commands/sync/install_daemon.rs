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
            Ok(PathBuf::from(home)
                .join("Library/LaunchAgents/com.datum.sync-agent.plist"))
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
        println!("# {} — {}", path_label(platform), unit_path(platform)?.display());
        print!("{}", render_unit(platform, &binary));
        return Ok(());
    }

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
        fs::create_dir_all(parent)
            .with_context(|| format!("create unit dir {parent:?}"))?;
    }
    let body = render_unit(platform, binary);
    fs::write(&path, body)
        .with_context(|| format!("write unit {path:?}"))?;
    enable_unit(platform)?;
    Ok(path)
}

fn do_uninstall(platform: Platform) -> Result<PathBuf> {
    let path = unit_path(platform)?;
    let _ = disable_unit(platform);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("remove unit {path:?}"))?;
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
            let p = unit_path(platform)?;
            run_cmd("launchctl", &["load", "-w", &p.to_string_lossy()])?;
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
            let p = unit_path(platform)?;
            run_cmd("launchctl", &["unload", "-w", &p.to_string_lossy()])?;
        }
    }
    Ok(())
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
    fn systemd_unit_path_honors_xdg_config_home() {
        let prev = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-cfg-install-test");
        let p = unit_path(Platform::SystemdUser).unwrap();
        assert_eq!(
            p,
            PathBuf::from(
                "/tmp/xdg-cfg-install-test/systemd/user/datum-sync-agent.service"
            )
        );
        match prev {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}
