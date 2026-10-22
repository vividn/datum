//! `datum install | uninstall | upgrade` and `datum check`'s install section
//! (da-uphgn), in a throwaway home: the binary is a copy of its own,
//! `systemctl` is faked on PATH, and the release is a directory read by
//! `file://`. Linux only: the daemon's unit is systemd's.
#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::sync_net::claim_port;

struct Home {
    tmp: TempDir,
    port: u16,
    _claim: std::net::TcpListener,
}

impl Home {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let fake = tmp.path().join("fake");
        std::fs::create_dir(&fake).unwrap();
        let log = tmp.path().join("ctl.log").display().to_string();
        for (name, body) in [
            ("systemctl", format!("echo \"systemctl $*\" >> {log}")),
            (
                "cargo",
                format!("echo \"cargo $*\" >> {log}; rm \"$3/bin/datum\""),
            ),
        ] {
            let path = fake.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let exe = tmp.path().join("bin/datum");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::copy(env!("CARGO_BIN_EXE_datum"), &exe).unwrap();
        let (port, _claim) = claim_port();
        let home = Self { tmp, port, _claim };
        home.ok(&["occur", "coffee"]);
        home
    }

    fn path(&self, p: &str) -> PathBuf {
        self.tmp.path().join(p)
    }

    fn exe(&self) -> PathBuf {
        self.path("bin/datum")
    }

    fn completion(&self) -> PathBuf {
        self.path("xdgdata/bash-completion/completions/datum")
    }

    fn service(&self) -> PathBuf {
        self.path("config/systemd/user/datum-sync.service")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.path("ctl.log")).unwrap_or_default()
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let path = format!(
            "{}:{}",
            self.path("fake").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(self.exe())
            .args(args)
            .env("PATH", path)
            .env("HOME", self.path("home"))
            .env("SHELL", "/bin/bash")
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_DATA_HOME", self.path("xdgdata"))
            .env("DATUM_HOME", self.path("data"))
            .env("DATUM_CONFIG_HOME", self.path("dconf"))
            .env("DATUM_DB", self.path("data/data.db"))
            .env(
                "DATUM_RELEASE_URL",
                format!("file://{}", self.path("release").display()),
            )
            .env("DATUM_SYNC_BIND", format!("127.0.0.1:{}", self.port))
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .env("NO_COLOR", "1")
            .env_remove("DATUM_CONFIG")
            .env_remove("ZDOTDIR")
            .envs(env.iter().copied())
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        self.ok_with(args, &[])
    }

    fn ok_with(&self, args: &[&str], env: &[(&str, &str)]) -> String {
        let out = self.run_with(args, env);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    fn err(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stderr).unwrap()
    }

    /// The install section's categories, or `None` when it did not run.
    fn install_findings(&self) -> Option<Vec<String>> {
        let report: serde_json::Value =
            serde_json::from_str(&self.ok(&["check", "--json"])).unwrap();
        let ran = report["examined"]["checkers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "install");
        ran.then(|| {
            report["issues"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["domain"] == "install")
                .map(|i| i["category"].as_str().unwrap().to_string())
                .collect()
        })
    }

    /// A release holding `binary` as this platform's, with `sum` its line
    /// in SHA256SUMS (`None`: its real one).
    fn release(&self, binary: &[u8], sum: Option<&str>) {
        let dir = self.path("release");
        std::fs::create_dir_all(&dir).unwrap();
        let asset = format!("datum-{}-{}", std::env::consts::ARCH, std::env::consts::OS);
        std::fs::write(dir.join(&asset), binary).unwrap();
        let sum = sum.map_or_else(|| format!("{:x}", Sha256::digest(binary)), String::from);
        std::fs::write(dir.join("SHA256SUMS"), format!("{sum}  {asset}\n")).unwrap();
    }
}

#[test]
fn install_then_uninstall_keeps_the_record() {
    let h = Home::new();
    let said = h.ok(&["install"]);
    assert!(h.completion().exists(), "{said}");
    assert!(!h.service().exists(), "no database syncs: {said}");
    assert!(
        said.contains("datum alias install bash >> ~/.bashrc"),
        "{said}"
    );
    assert!(h.path("data/installed.toml").exists());

    h.ok(&["install", "--daemon"]);
    let unit = std::fs::read_to_string(h.service()).unwrap();
    assert!(
        unit.contains(&format!("ExecStart=\"{}\"", h.exe().display())),
        "{unit}"
    );

    // Every alias loop goes: another database's, and the first build's one
    // line (6627637cf) too.
    let rc = h.path("home/.zshrc");
    let one_line = "for __datum_a in asleep find; do command -v \"$__datum_a\" >/dev/null 2>&1 || \
         alias \"$__datum_a=\"'datum '\"$__datum_a\"; done; unset __datum_a\n";
    let loops = h.ok(&["alias", "install", "zsh"])
        + one_line
        + &h.ok(&["--db", "other", "alias", "install", "zsh"]);
    std::fs::create_dir_all(rc.parent().unwrap()).unwrap();
    // The last loop's newline trimmed, as an editor may.
    std::fs::write(&rc, format!("export KEPT=1\n{}", loops.trim_end())).unwrap();

    let said = h.ok(&["uninstall"]);
    assert_eq!(std::fs::read_to_string(&rc).unwrap(), "export KEPT=1\n");
    assert!(!h.completion().exists(), "{said}");
    assert!(!h.service().exists(), "{said}");
    assert!(!h.exe().exists(), "{said}");
    assert!(!h.path("data/installed.toml").exists(), "{said}");
    assert!(h.path("data/data.db").exists(), "{said}");
    assert!(
        said.contains(&format!("data    {}", h.path("data").display())),
        "{said}"
    );
}

/// A datum a package manager put in place is its to remove.
#[test]
fn uninstall_leaves_a_binary_install_did_not_set_up() {
    let h = Home::new();
    // And another binary's record is that binary's: it stays.
    let record = h.path("data/installed.toml");
    std::fs::write(&record, "exe = \"/opt/other/datum\"\n").unwrap();
    let said = h.ok(&["uninstall"]);
    assert!(h.exe().exists(), "{said}");
    assert!(said.contains("did not set it up"), "{said}");
    assert!(record.exists(), "{said}");
}

/// The daemon comes with `datum install` once a database syncs, unless
/// DATUM_SYNC_NO_DAEMON, and installing it again restarts it onto the
/// binary, as install.sh used to.
#[test]
fn install_sets_up_the_daemon_when_a_database_syncs() {
    let h = Home::new();
    h.run(&["sync", "chain", "invite", "--expires", "1s"]);
    h.ok(&["install", "--no-completions"]);
    assert!(!h.service().exists(), "opted out");
    h.ok_with(
        &["install", "--no-completions"],
        &[("DATUM_SYNC_NO_DAEMON", "0")],
    );
    assert!(h.service().exists());
    assert!(!h.completion().exists());
    assert!(
        h.log()
            .contains("systemctl --user restart datum-sync.service"),
        "{}",
        h.log()
    );
}

#[test]
fn check_says_what_install_set_up_and_is_gone() {
    let h = Home::new();
    assert_eq!(h.install_findings(), None, "not installed: no section");
    h.ok(&["install", "--daemon"]);
    assert_eq!(h.install_findings(), Some(vec![]));

    std::fs::remove_file(h.completion()).unwrap();
    let unit = std::fs::read_to_string(h.service()).unwrap();
    let other = unit.replace(&h.exe().display().to_string(), "/opt/old/datum");
    std::fs::write(h.service(), other).unwrap();
    assert_eq!(
        h.install_findings(),
        Some(vec![
            "install_completion_missing".to_string(),
            "install_daemon_binary".to_string()
        ])
    );

    // Opted out of completion, none is missed.
    h.ok(&["install", "--no-completions"]);
    assert_eq!(h.install_findings(), Some(vec![]));

    // The shell it was installed for is the one checked, not $SHELL's.
    h.ok(&["install", "--shell", "fish"]);
    assert_eq!(h.install_findings(), Some(vec![]));
    std::fs::remove_file(h.path("config/fish/completions/datum.fish")).unwrap();
    assert_eq!(
        h.install_findings(),
        Some(vec!["install_completion_missing".to_string()])
    );
}

#[test]
fn update_checks_are_opt_in() {
    let h = Home::new();
    h.ok(&["install", "--no-completions"]);
    h.release(b"#!/bin/sh\necho datum 9.9.9\n", None);
    assert_eq!(h.install_findings(), Some(vec![]), "off by default");
    std::fs::create_dir_all(h.path("dconf")).unwrap();
    std::fs::write(h.path("dconf/datumrc.toml"), "update_check = true\n").unwrap();
    assert_eq!(
        h.install_findings(),
        Some(vec!["install_update".to_string()])
    );
    h.release(&std::fs::read(h.exe()).unwrap(), None);
    assert_eq!(h.install_findings(), Some(vec![]), "up to date");
}

#[test]
fn upgrade_refuses_a_mismatch_and_swaps_in_a_good_release() {
    let h = Home::new();
    let stub = b"#!/bin/sh\necho datum 9.9.9\n";
    h.release(stub, None);
    let refused = h.err(&["upgrade"]);
    assert!(
        refused.contains("not set up by `datum install`"),
        "{refused}"
    );

    h.ok(&["install", "--daemon"]);
    let before = std::fs::read(h.exe()).unwrap();
    h.release(&before, None);
    assert!(h.ok(&["upgrade"]).contains("up to date"));

    h.release(stub, Some(&"0".repeat(64)));
    let refused = h.err(&["upgrade"]);
    assert!(refused.contains("does not match SHA256SUMS"), "{refused}");
    assert_eq!(std::fs::read(h.exe()).unwrap(), before);
    assert!(!h.path("bin/.datum-upgrade").exists());

    h.release(b"not a program", None);
    let refused = h.err(&["upgrade"]);
    assert!(refused.contains("does not run here"), "{refused}");
    assert_eq!(std::fs::read(h.exe()).unwrap(), before);
    assert!(!h.path("bin/.datum-upgrade").exists());

    h.release(stub, None);
    assert!(h.ok(&["upgrade", "--check"]).contains("a newer datum"));
    assert_eq!(std::fs::read(h.exe()).unwrap(), before);

    let said = h.ok(&["upgrade"]);
    assert!(said.contains("datum 9.9.9"), "{said}");
    assert_eq!(std::fs::read(h.exe()).unwrap(), stub);
    assert_eq!(
        h.log().matches("restart datum-sync.service").count(),
        2,
        "install, then upgrade: {}",
        h.log()
    );
}

/// A datum cargo installed upgrades from its checkout, and uninstalls by
/// cargo, so cargo's own record of it goes too.
#[test]
fn a_cargo_install_upgrades_from_its_checkout() {
    let h = Home::new();
    std::fs::write(h.path(".crates.toml"), "").unwrap();
    h.ok(&["install", "--no-completions"]);
    let refused = h.err(&["upgrade"]);
    assert!(
        refused.contains("git pull && scripts/install.sh"),
        "{refused}"
    );
    h.ok(&["uninstall"]);
    assert!(!h.exe().exists());
    let cargo = format!(
        "cargo uninstall --root {} datum-cli",
        h.tmp.path().display()
    );
    assert!(h.log().contains(&cargo), "{}", h.log());
}
