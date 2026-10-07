//! `datum sync daemon` (da-vd2sn): the systemd user unit written to a
//! throwaway home, `systemctl` and `loginctl` faked on PATH so nothing
//! reaches a real service manager. Linux only: that is the unit's platform.
#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let log = tmp.path().join("ctl.log");
        for (name, body) in [
            (
                "systemctl",
                format!("echo \"systemctl $*\" >> {}", log.display()),
            ),
            ("loginctl", "echo no".to_string()),
            ("cargo", "true".to_string()),
        ] {
            let path = bin.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let (port, _claim) = claim_port();
        Self { tmp, port, _claim }
    }

    fn path(&self, p: &str) -> PathBuf {
        self.tmp.path().join(p)
    }

    fn service(&self) -> PathBuf {
        self.path("config/systemd/user/datum-sync.service")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.path("ctl.log")).unwrap_or_default()
    }

    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut c = Command::new(program);
        let path = format!(
            "{}:{}",
            self.path("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        c.env("PATH", path)
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_DATA_HOME", self.path("xdgdata"))
            .env("DATUM_HOME", self.path("data"))
            .env("DATUM_CONFIG_HOME", self.path("dc%onf"))
            .env("DATUM_DB", self.path("data/data.db"))
            .env("DATUM_COLOR", "0")
            .env("DATUM_SYNC_BIND", format!("127.0.0.1:{}", self.port))
            .env("USER", "tester")
            .env_remove("DATUM_SYNC_NO_DAEMON");
        c
    }

    fn datum(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut c = self.command(env!("CARGO_BIN_EXE_datum"));
        c.args(args).envs(env.iter().copied()).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.datum(args, &[]);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    /// A first invite, served for a second and expiring: its stderr.
    fn invite(&self, env: &[(&str, &str)]) -> String {
        let out = self.datum(&["sync", "chain", "invite", "--expires", "1s"], env);
        String::from_utf8(out.stderr).unwrap()
    }
}

#[test]
fn install_status_and_uninstall() {
    let h = Home::new();
    assert!(h
        .ok(&["sync", "daemon", "status"])
        .contains("not installed"));

    let said = h.ok(&["sync", "daemon", "install"]);
    assert!(said.contains("systemd user service datum-sync"), "{said}");
    assert!(said.contains(&h.service().display().to_string()), "{said}");
    let service = std::fs::read_to_string(h.service()).unwrap();
    let exe = Path::new(env!("CARGO_BIN_EXE_datum")).display().to_string();
    assert!(
        service.contains(&format!("ExecStart=\"{exe}\" sync daemon run")),
        "{service}"
    );
    for (var, dir) in [
        ("DATUM_HOME", "data"),
        ("DATUM_CONFIG_HOME", "dc%onf"),
        ("XDG_DATA_HOME", "xdgdata"),
        ("XDG_CONFIG_HOME", "config"),
    ] {
        // systemd would read a lone `%` as a specifier.
        let dir = h.path(dir).display().to_string().replace('%', "%%");
        let line = format!("Environment=\"{var}={dir}\"");
        assert!(service.contains(&line), "{line}: {service}");
    }
    for call in [
        "--user daemon-reload",
        "--user enable datum-sync.service",
        "--user restart datum-sync.service",
    ] {
        assert!(h.log().contains(call), "{call}: {}", h.log());
    }

    let status = h.ok(&["sync", "daemon", "status"]);
    for line in [
        "installed: the systemd user service datum-sync",
        "running",
        "0 chained database(s)",
        "`loginctl enable-linger`",
    ] {
        assert!(status.contains(line), "{line}: {status}");
    }

    let said = h.ok(&["sync", "daemon", "uninstall"]);
    assert!(said.contains("removed the systemd user service"), "{said}");
    // None of them opened the active database, so none created it.
    assert!(!h.path("data/data.db").exists());
    assert!(!h.service().exists());
    assert!(h.log().contains("--user disable --now datum-sync.service"));
    assert!(h
        .ok(&["sync", "daemon", "status"])
        .contains("not installed"));
    assert!(h
        .ok(&["sync", "daemon", "uninstall"])
        .contains("not installed"));
}

#[test]
fn the_first_invite_installs_it_and_says_so() {
    let h = Home::new();
    let said = h.invite(&[]);
    assert!(
        said.contains("sync now runs in the background (systemd user service datum-sync)"),
        "{said}"
    );
    assert!(h.service().exists());
    let status = h.ok(&["sync", "daemon", "status"]);
    assert!(status.contains("1 chained database(s): data"), "{status}");

    // Not the first: a copy already in a chain leaves an uninstall alone.
    h.ok(&["sync", "daemon", "uninstall"]);
    assert!(!h.invite(&[]).contains("background"));
    assert!(!h.service().exists());
}

#[test]
fn no_daemon_skips_the_install() {
    let h = Home::new();
    assert!(!h
        .invite(&[("DATUM_SYNC_NO_DAEMON", "1")])
        .contains("background"));
    assert!(!h.service().exists());
    assert!(h.log().is_empty(), "{}", h.log());
}

#[test]
fn install_sh_restarts_an_installed_daemon() {
    let h = Home::new();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/install.sh");
    let install = || {
        let out = h
            .command("bash")
            .arg(&script)
            .arg("--no-completions")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    };
    install();
    assert!(!h.log().contains("restart"), "{}", h.log());

    std::fs::create_dir_all(h.service().parent().unwrap()).unwrap();
    std::fs::write(h.service(), "").unwrap();
    install();
    assert!(
        h.log()
            .contains("systemctl --user restart datum-sync.service"),
        "{}",
        h.log()
    );
}

/// A join admitted whose first sync then fails is still a member, and a
/// member the daemon does not serve is one the others time out on (da-shfv4).
/// The first sync fails as in `sync_net`'s `slow_first_sync`: the joiner
/// applies 300 ops at 20 ms each with no keepalives, on a 3 s timeout.
#[test]
fn a_join_whose_first_sync_fails_still_installs_it() {
    use std::io::{BufRead, BufReader};
    let (a, b) = (Home::new(), Home::new());
    for i in 0..300 {
        a.ok(&["occur", "step", &format!("n={i}")]);
    }
    let slow = [
        ("DATUM_SYNC_TIMEOUT_S", "3"),
        ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "20"),
        ("DATUM_SYNC_TEST_KEEPALIVE", "0"),
    ];
    let mut inviting = a
        .command(env!("CARGO_BIN_EXE_datum"))
        .args(["sync", "chain", "invite"])
        .envs(slow)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let ticket = BufReader::new(inviting.stdout.take().unwrap())
        .lines()
        .map_while(Result::ok)
        .find_map(|l| {
            Some(
                l.split("datum sync chain connect ")
                    .nth(1)?
                    .trim()
                    .trim_matches('\'')
                    .to_string(),
            )
        })
        .unwrap();
    let joined = b.datum(&["sync", "chain", "connect", &ticket], &slow);
    let _ = inviting.kill();
    let _ = inviting.wait();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&joined.stdout),
        String::from_utf8_lossy(&joined.stderr)
    );
    assert!(!joined.status.success(), "{said}");
    assert!(said.contains("paired with"), "{said}");
    assert!(said.contains("sync now runs in the background"), "{said}");
    assert!(b.service().exists());
}
