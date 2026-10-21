//! `datum sync daemon` (da-vd2sn): the systemd user unit written to a
//! throwaway home, `systemctl` and `loginctl` faked on PATH so nothing
//! reaches a real service manager. Linux only: that is the unit's platform.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Output, Stdio};

use tempfile::TempDir;

use crate::sync_net::{claim_port, eventually};

/// A long-running `datum`, killed when dropped.
struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

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
            .env("NO_COLOR", "1")
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

    fn spawn(&self, args: &[&str], env: &[(&str, &str)]) -> Killed {
        let mut c = self.command(env!("CARGO_BIN_EXE_datum"));
        c.args(args).envs(env.iter().copied());
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
        Killed(c.spawn().unwrap())
    }

    /// An invite serving, its ticket, and its stdout: kept open until the
    /// join is done, since the invite prints past the ticket, and a closed
    /// pipe ends it before it serves (exit 141, da-aa2hw).
    fn inviting(&self, env: &[(&str, &str)]) -> (Killed, BufReader<ChildStdout>, String) {
        let mut inviting = self.spawn(&["sync", "chain", "invite"], env);
        let mut out = BufReader::new(inviting.0.stdout.take().unwrap());
        let ticket = (&mut out)
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
        (inviting, out, ticket)
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

/// `scripts/uninstall.sh` undoes what install.sh and the README install —
/// the daemon, completion, the alias loop, the binary — and keeps the
/// record; a second run has nothing to do (da-dshno). The faked `cargo`
/// removes the binary as cargo's own uninstall would.
#[test]
fn uninstall_sh_removes_datum_and_keeps_the_record() {
    let h = Home::new();
    let root = h.path("cargo");
    let installed = root.join("bin/datum");
    std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_datum"), &installed).unwrap();
    std::fs::write(
        h.path("bin/cargo"),
        format!(
            "#!/bin/sh\necho \"cargo $*\" >> {}\nrm \"$3/bin/datum\"\n",
            h.path("ctl.log").display()
        ),
    )
    .unwrap();
    let run = |program: &Path, args: &[&str]| {
        let out = h
            .command(program)
            .args(args)
            .env("CARGO_HOME", &root)
            .env("SHELL", "/bin/bash")
            .env_remove("CARGO_INSTALL_ROOT")
            .env_remove("ZDOTDIR")
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };

    run(&installed, &["occur", "coffee"]);
    run(&installed, &["completions", "install"]);
    run(&installed, &["sync", "daemon", "install"]);
    let rc = h.path("home/.zshrc");
    let loop_ = run(&installed, &["alias", "install", "zsh"]);
    std::fs::create_dir_all(rc.parent().unwrap()).unwrap();
    std::fs::write(&rc, format!("export KEPT=1\n{loop_}")).unwrap();
    let completion = h.path("xdgdata/bash-completion/completions/datum");
    assert!(completion.exists() && h.service().exists());

    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/uninstall.sh");
    let said = run(Path::new("bash"), &[script.to_str().unwrap()]);
    assert!(!installed.exists(), "{said}");
    assert!(!completion.exists(), "{said}");
    assert!(!h.service().exists(), "{said}");
    assert_eq!(std::fs::read_to_string(&rc).unwrap(), "export KEPT=1\n");
    let uninstall = format!("cargo uninstall --root {} datum-cli", root.display());
    assert!(h.log().contains(&uninstall), "{}", h.log());
    assert!(
        said.contains(&format!("data    {}", h.path("data").display())),
        "{said}"
    );
    assert!(h.ok(&["tail"]).contains("coffee"));

    let again = run(Path::new("bash"), &[script.to_str().unwrap()]);
    assert!(again.contains("nothing to uninstall"), "{again}");
}

/// A join admitted whose first sync then fails is still a member, and a
/// member the daemon does not serve is one the others time out on (da-shfv4).
/// The first sync fails as in `sync_net`'s `slow_first_sync`: the joiner
/// applies 300 ops at 20 ms each with no keepalives, on a 3 s timeout.
#[test]
fn a_join_whose_first_sync_fails_still_installs_it() {
    let (a, b) = (Home::new(), Home::new());
    for i in 0..300 {
        a.ok(&["occur", "step", &format!("n={i}")]);
    }
    let slow = [
        ("DATUM_SYNC_TIMEOUT_S", "3"),
        ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "20"),
        ("DATUM_SYNC_TEST_KEEPALIVE", "0"),
    ];
    let (inviting, _out, ticket) = a.inviting(&slow);
    let joined = b.datum(&["sync", "chain", "connect", &ticket], &slow);
    drop(inviting);
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

/// `connect` installs the daemon once it is admitted, not after its first
/// sync (da-qr7gz): killed while that sync runs it has left the daemon, which
/// kept off the database for as long as the `connect` lived, and finishes
/// the join. The joiner applies 300 ops at 20 ms each: six seconds to be
/// killed in.
#[test]
fn a_join_killed_at_its_first_sync_left_the_daemon_which_finishes_it() {
    let (a, b) = (Home::new(), Home::new());
    for i in 0..300 {
        a.ok(&["occur", "step", &format!("n={i}")]);
    }
    let (inviting, _out, ticket) = a.inviting(&[]);
    let slow = [("DATUM_SYNC_TEST_APPLY_DELAY_MS", "20")];
    let mut joining = b.spawn(&["sync", "chain", "connect", &ticket], &slow);
    // Read no further, and kept open: a closed stderr would end the join.
    let mut err = BufReader::new(joining.0.stderr.take().unwrap());
    let said = (&mut err)
        .lines()
        .map_while(Result::ok)
        .any(|l| l.contains("sync now runs in the background"));
    assert!(said, "the join ended without installing the daemon");
    // Said with the first sync still to finish, and the daemon leaves the
    // database to the `connect` meanwhile.
    let status = b.ok(&["sync", "status"]);
    assert!(status.contains("joining a sync chain through"), "{status}");
    let daemon = b.ok(&["sync", "daemon", "status"]);
    assert!(daemon.contains("0 chained database(s)"), "{daemon}");

    drop(joining);
    // Killed, it left its lock's file behind, let go: the daemon reads past it.
    assert!(b.path("data/data.db.joining").exists());
    assert!(b.service().exists());
    assert!(b
        .log()
        .contains("systemctl --user restart datum-sync.service"));
    let status = b.ok(&["sync", "status"]);
    assert!(status.contains("joining a sync chain through"), "{status}");
    let daemon = b.ok(&["sync", "daemon", "status"]);
    assert!(daemon.contains("1 chained database(s): data"), "{daemon}");

    drop(inviting);
    let _a_runs = a.spawn(&["sync", "run"], &[]);
    let _b_runs = b.spawn(&["sync", "daemon", "run"], &[]);
    eventually("the daemon finishing the join", || {
        b.ok(&["sync", "status"]).contains("  synced  ")
    });
    assert!(b.ok(&["tail", "-n", "1"]).contains("step"));
}

/// An open invite holds the copy as `connect` does (da-apd52): a daemon
/// serving it too binds its endpoint key in a second process and may admit
/// the joiner itself, and the invite, having seen nothing, waits out its
/// expiry. Redeemed, the invite says so and exits, and the daemon takes the
/// database up; expired, as `the_first_invite_installs_it_and_says_so` reads.
#[test]
fn an_open_invite_keeps_the_daemon_off_the_copy() {
    let (a, b) = (Home::new(), Home::new());
    let (mut inviting, out, ticket) = a.inviting(&[]);
    let daemon = a.ok(&["sync", "daemon", "status"]);
    assert!(daemon.contains("0 chained database(s)"), "{daemon}");

    b.ok(&["sync", "chain", "connect", &ticket]);
    let said: Vec<String> = out.lines().map_while(Result::ok).collect();
    assert!(inviting.0.wait().unwrap().success(), "{said:?}");
    assert!(
        said.iter().any(|l| l.starts_with("added sync peer")),
        "{said:?}"
    );
    let daemon = a.ok(&["sync", "daemon", "status"]);
    assert!(daemon.contains("1 chained database(s): data"), "{daemon}");
}

/// An invite on a copy the daemon is ALREADY serving (da-8sah2): the lock
/// is read only when the daemon takes a database up, so the thread serving
/// it reads it too, and steps aside for the invite, which then admits the
/// joiner itself.
#[test]
fn an_invite_moves_the_daemon_serving_the_copy_aside() {
    let (a, b) = (Home::new(), Home::new());
    a.invite(&[("DATUM_SYNC_NO_DAEMON", "1")]);
    // On a port of its own: the invite binds the home's.
    let (port, _claim) = claim_port();
    let bind = format!("127.0.0.1:{port}");
    let mut daemon = a.spawn(&["sync", "daemon", "run"], &[("DATUM_SYNC_BIND", &bind)]);
    let err = BufReader::new(daemon.0.stderr.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        err.lines()
            .map_while(Result::ok)
            .try_for_each(|l| tx.send(l))
    });
    let says = |what: &str| {
        while !rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("the daemon saying {what:?} within 10s"))
            .contains(what)
        {}
    };
    says("is up");

    let (mut inviting, out, ticket) = a.inviting(&[]);
    says("stepping aside until it ends");
    b.ok(&["sync", "chain", "connect", &ticket]);
    let said: Vec<String> = out.lines().map_while(Result::ok).collect();
    assert!(inviting.0.wait().unwrap().success(), "{said:?}");
    assert!(
        said.iter().any(|l| l.starts_with("added sync peer")),
        "{said:?}"
    );
}

/// A foreground `sync run` steps aside for an invite as the daemon does
/// (da-x5ynh), unbound until the join ends, and is up again after it.
#[test]
fn an_invite_moves_a_foreground_run_aside_until_it_ends() {
    let (a, b) = (Home::new(), Home::new());
    a.invite(&[("DATUM_SYNC_NO_DAEMON", "1")]);
    let (port, _claim) = claim_port();
    let bind = format!("127.0.0.1:{port}");
    let mut run = a.spawn(&["sync", "run"], &[("DATUM_SYNC_BIND", &bind)]);
    let err = BufReader::new(run.0.stderr.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        err.lines()
            .map_while(Result::ok)
            .try_for_each(|l| tx.send(l))
    });
    let says = |what: &str| {
        while !rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("the run saying {what:?} within 10s"))
            .contains(what)
        {}
    };
    says("is up");

    let (mut inviting, out, ticket) = a.inviting(&[]);
    says("stepping aside until it ends");
    b.ok(&["sync", "chain", "connect", &ticket]);
    let said: Vec<String> = out.lines().map_while(Result::ok).collect();
    assert!(inviting.0.wait().unwrap().success(), "{said:?}");
    assert!(
        said.iter().any(|l| l.starts_with("added sync peer")),
        "{said:?}"
    );
    says("is up");
    assert!(run.0.try_wait().unwrap().is_none(), "the run ended");
}

#[test]
fn status_names_a_missing_daemon_on_a_chained_copy() {
    let h = Home::new();
    let missing = "no sync daemon";
    // A read creates no database (da-ov6x5): `db migrate` makes it.
    h.ok(&["db", "migrate"]);
    assert!(!h.ok(&["sync", "status"]).contains(missing));
    h.invite(&[("DATUM_SYNC_NO_DAEMON", "1")]);
    let status = h.ok(&["sync", "status"]);
    assert!(
        status.contains(missing) && status.contains("`datum sync daemon install`"),
        "{status}"
    );
    let opted_out = h.datum(&["sync", "status"], &[("DATUM_SYNC_NO_DAEMON", "1")]);
    assert!(!String::from_utf8_lossy(&opted_out.stdout).contains(missing));
    h.ok(&["sync", "daemon", "install"]);
    assert!(!h.ok(&["sync", "status"]).contains(missing));
}

/// A database opened by path, outside the data directory, is the daemon's
/// once chained: the first invite keeps it in the registry (da-zrl7n).
#[test]
fn a_db_chained_by_path_is_the_daemons() {
    let h = Home::new();
    let db = h.path("elsewhere/bypath.db");
    let db = db.to_str().unwrap();
    h.ok(&["--db", db, "occur", "coffee"]);
    h.datum(
        &["--db", db, "sync", "chain", "invite", "--expires", "1s"],
        &[("DATUM_SYNC_NO_DAEMON", "1")],
    );
    h.ok(&["sync", "daemon", "install"]);
    let daemon = h.ok(&["sync", "daemon", "status"]);
    assert!(
        daemon.contains(&format!("1 chained database(s): {db}")),
        "{daemon}"
    );
}
