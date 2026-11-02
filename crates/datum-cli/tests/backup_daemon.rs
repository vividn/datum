//! `datum db backup daemon` (da-gbac0): the systemd user timers and services
//! written to a throwaway home, `systemctl` faked on PATH so nothing reaches
//! a real service manager. The fake says a timer is not loaded (exit 4)
//! while its name is a line of the file `inactive`, cannot reach the user
//! bus (exit 1) while `nobus` exists, and gives every service's last result
//! as `result`'s text and its last run's time as `ran`'s.
#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

struct Home(TempDir);

impl Home {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let p = |s: &str| tmp.path().join(s).display().to_string();
        std::fs::create_dir(p("bin")).unwrap();
        let fake = format!(
            "#!/bin/sh\necho \"systemctl $*\" >> {log}\n\
             case \"$*\" in\n\
             *is-active*) [ -e {nobus} ] && exit 1; grep -qxF \"$4\" {inactive} 2>/dev/null && exit 4; exit 0 ;;\n\
             *show*) echo \"Result=$(cat {result} 2>/dev/null || echo success)\"; \
                     echo \"ExecMainExitTimestamp=$(cat {ran} 2>/dev/null)\" ;;\n\
             esac\n",
            log = p("ctl.log"),
            inactive = p("inactive"),
            nobus = p("nobus"),
            result = p("result"),
            ran = p("ran"),
        );
        let path = tmp.path().join("bin/systemctl");
        std::fs::write(&path, fake).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let home = Self(tmp);
        home.ok(&["moment", "coffee"]);
        home
    }

    fn path(&self, p: &str) -> PathBuf {
        self.0.path().join(p)
    }

    /// A directory made, as the units name it.
    fn dir(&self, p: &str) -> String {
        std::fs::create_dir_all(self.path(p)).unwrap();
        std::fs::canonicalize(self.path(p))
            .unwrap()
            .display()
            .to_string()
    }

    /// Every unit installed: its name, and its service's and timer's text.
    fn units(&self) -> Vec<(String, String, String)> {
        let dir = self.path("config/systemd/user");
        let mut found: Vec<_> = std::fs::read_dir(&dir)
            .map(|r| r.flatten().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let unit = name.strip_suffix(".service")?.to_string();
                let timer = std::fs::read_to_string(dir.join(format!("{unit}.timer"))).unwrap();
                Some((unit, std::fs::read_to_string(e.path()).unwrap(), timer))
            })
            .collect();
        found.sort();
        found
    }

    /// The one unit backing up into `to`.
    fn unit_into(&self, to: &str) -> (String, String, String) {
        let mut found: Vec<_> = (self.units().into_iter())
            .filter(|(_, s, _)| s.contains(&format!("X-Datum-To={to}\n")))
            .collect();
        assert_eq!(found.len(), 1, "{to}: {found:?}");
        found.remove(0)
    }

    fn datum(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_datum"))
            .args(args)
            .output()
            .unwrap()
    }

    /// The service's `ExecStart=` run by a shell, as systemd would.
    fn run_service(&self, service: &str) -> Output {
        let line = service.lines().find_map(|l| l.strip_prefix("ExecStart="));
        self.command("sh")
            .args(["-c", line.unwrap()])
            .output()
            .unwrap()
    }

    fn command(&self, program: &str) -> Command {
        let path = format!(
            "{}:{}",
            self.path("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new(program);
        command
            .env("PATH", path)
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("DATUM_HOME", self.path("data"))
            .env("DATUM_CONFIG_HOME", self.path("conf"))
            .env("DATUM_DB", self.path("data/data.db"))
            .env("NO_COLOR", "1")
            .env("USER", "tester");
        command
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.datum(args);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    fn install(&self, extra: &[&str]) -> String {
        let args = [&["db", "backup", "daemon", "install"][..], extra].concat();
        self.ok(&args)
    }

    /// `datum check install`'s categories and messages.
    fn check(&self) -> String {
        String::from_utf8(self.datum(&["check", "install"]).stdout).unwrap()
    }
}

#[test]
fn install_writes_the_command_and_its_period() {
    let h = Home::new();
    let bk = h.dir("bk");
    let db = std::fs::canonicalize(h.path("data/data.db")).unwrap();
    assert!(h
        .ok(&["db", "backup", "daemon", "status"])
        .contains("none installed"));

    h.install(&["--to", &bk, "--every", "15m"]);
    let (unit, service, timer) = h.unit_into(&bk);
    assert!(unit.starts_with("datum-backup-data-"), "{unit}");
    let exe = std::fs::canonicalize(env!("CARGO_BIN_EXE_datum")).unwrap();
    assert!(
        service.contains(&format!(
            "ExecStart=\"{}\" --db \"{}\" db backup --to \"{bk}\" --every 900s\n",
            exe.display(),
            db.display(),
        )),
        "{service}"
    );
    assert!(service.contains("Type=oneshot"), "{service}");
    // The delay a tenth of 15m, and the timer waits the period less it.
    assert!(timer.contains("OnUnitActiveSec=810s\n"), "{timer}");
    assert!(
        timer.contains("RandomizedDelaySec=90s\nFixedRandomDelay=yes\n"),
        "{timer}"
    );
    let log = std::fs::read_to_string(h.path("ctl.log")).unwrap();
    assert!(log.contains(&format!("enable {unit}.timer")), "{log}");
    assert!(log.contains(&format!("restart {unit}.timer")), "{log}");
}

/// With no --to, the data directory's backups/ (vividn, 2026-11-02: «yeah,
/// i think that is a sane default»).
#[test]
fn no_to_backs_up_into_the_data_directorys_backups() {
    let h = Home::new();
    h.install(&["--every", "1h"]);
    let backups = h.dir("data/backups");
    let (_, service, _) = h.unit_into(&backups);
    assert!(
        service.contains(&format!("--to \"{backups}\" --every 3600s")),
        "{service}"
    );
}

/// A database has a daemon per destination: a second adds one, a reinstall
/// into one replaces only it, and each is said and checked by its own.
#[test]
fn a_daemon_per_destination() {
    let h = Home::new();
    let (here, drive) = (h.dir("here"), h.dir("drive"));
    h.install(&["--to", &here, "--every", "15m"]);
    h.install(&["--to", &drive, "--every", "1d"]);
    assert_eq!(h.units().len(), 2);
    let (unit_here, _, _) = h.unit_into(&here);
    let (unit_drive, _, drive_timer) = h.unit_into(&drive);
    assert_ne!(unit_here, unit_drive);
    // A day's delay is capped at five minutes.
    assert!(drive_timer.contains("OnUnitActiveSec=86100s\nRandomizedDelaySec=300s\n"));

    // Again into one, with another period and a cap: that one changes alone.
    let (_, before_drive, _) = h.unit_into(&drive);
    h.install(&["--to", &here, "--every", "1h", "--slots", "8"]);
    assert_eq!(h.units().len(), 2);
    let (again, service, timer) = h.unit_into(&here);
    assert_eq!(again, unit_here);
    assert!(timer.contains("OnUnitActiveSec=3300s\n"), "{timer}");
    assert!(service.contains("--every 3600s --slots 8\n"), "{service}");
    assert_eq!(h.unit_into(&drive).1, before_drive);

    // Status: a line each, its destination and period, and its last run.
    let status = h.ok(&["db", "backup", "daemon", "status"]);
    assert!(
        status.contains(&format!("{unit_here}: datum db backup of ")),
        "{status}"
    );
    assert!(
        status.contains(&format!("into {here}, every 1h")),
        "{status}"
    );
    assert!(
        status.contains(&format!("into {drive}, every 24h")),
        "{status}"
    );
    assert_eq!(status.matches("not run yet").count(), 2, "{status}");
    std::fs::write(h.path("ran"), "Mon 2026-11-02 16:35:26 CET").unwrap();
    let status = h.ok(&["db", "backup", "daemon", "status"]);
    assert_eq!(
        status
            .matches("last ran Mon 2026-11-02 16:35:26 CET: succeeded")
            .count(),
        2,
        "{status}"
    );

    // Healthy, `check` says nothing; one stopped is named by its destination.
    assert!(!h.check().contains("backup_daemon"), "{}", h.check());
    std::fs::write(h.path("inactive"), format!("{unit_drive}.timer\n")).unwrap();
    let check = h.check();
    assert_eq!(check.matches("backup_daemon_missing").count(), 1, "{check}");
    assert!(
        check.contains(&format!("into {drive} is not running")),
        "{check}"
    );
    assert!(!check.contains(&format!("into {here} ")), "{check}");
    // A systemctl that cannot ask the user bus says nothing of the timers.
    std::fs::write(h.path("nobus"), "").unwrap();
    assert!(!h.check().contains("backup_daemon"), "{}", h.check());
    std::fs::remove_file(h.path("nobus")).unwrap();
    std::fs::remove_file(h.path("inactive")).unwrap();
    // A failed last run, each by its destination.
    std::fs::write(h.path("result"), "exit-code\n").unwrap();
    let check = h.check();
    assert_eq!(check.matches("backup_daemon_failed").count(), 2, "{check}");
    assert!(
        check.contains(&format!("into {drive} failed (exit-code)")),
        "{check}"
    );
    assert!(h
        .ok(&["db", "backup", "daemon", "status"])
        .contains("failed (exit-code)"));
    std::fs::remove_file(h.path("result")).unwrap();

    // --to removes that one; bare, the rest, naming each.
    let said = h.ok(&["db", "backup", "daemon", "uninstall", "--to", &drive]);
    assert!(
        said.contains(&format!("{unit_drive}.timer, into {drive}")),
        "{said}"
    );
    assert_eq!(h.units().len(), 1);
    let log = std::fs::read_to_string(h.path("ctl.log")).unwrap();
    // A failed last run is not left listed as a failed unit.
    assert!(
        log.contains(&format!("reset-failed {unit_drive}.service")),
        "{log}"
    );
    h.install(&["--to", &drive, "--every", "1d"]);
    let said = h.ok(&["db", "backup", "daemon", "uninstall"]);
    assert!(
        said.contains(&format!("into {here}")) && said.contains(&format!("into {drive}")),
        "{said}"
    );
    assert!(h.units().is_empty());
    assert!(h
        .ok(&["db", "backup", "daemon", "uninstall"])
        .contains("no backup daemon"));
}

/// Each database gets its own, and a unit name is never taken from another.
#[test]
fn two_databases_two_daemons() {
    let h = Home::new();
    let bk = h.dir("bk");
    h.install(&["--to", &bk, "--every", "1h"]);
    h.ok(&["--db", "other", "moment", "tea"]);
    let other = ["--db", "other", "db", "backup", "daemon"];
    h.ok(&[&other[..], &["install", "--to", &bk, "--every", "1d"]].concat());
    let service = (h.units().into_iter())
        .find(|(u, _, _)| u.starts_with("datum-backup-other-"))
        .unwrap()
        .1;
    // A named database is backed up by its name, as a run by hand is.
    assert!(service.contains("--db \"other\" db backup"), "{service}");
    // `other` uninstalls its own alone.
    h.ok(&[&other[..], &["uninstall"]].concat());
    assert_eq!(h.units().len(), 1);
    h.ok(&[&other[..], &["install", "--to", &bk, "--every", "1d"]].concat());

    // A database that is not there has nothing to back up.
    let path = h.path("elsewhere/other.db").display().to_string();
    let install = [
        "--db", &path, "db", "backup", "daemon", "install", "--to", &bk, "--every", "1h",
    ];
    let out = h.datum(&install);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no database at"),
        "{out:?}"
    );
    // A path whose stem is `other`, into the same destination, is another
    // database: the unit name `other` holds there is refused, not replaced.
    h.ok(&["--db", &path, "moment", "x"]);
    let out = h.datum(&install);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("backs up"),
        "{out:?}"
    );
}

/// A database deleted after its install, under a symlinked directory, is
/// still the one its unit names: uninstall finds it.
#[test]
fn uninstall_finds_a_deleted_database_under_a_symlink() {
    let h = Home::new();
    std::fs::create_dir(h.path("real")).unwrap();
    std::os::unix::fs::symlink(h.path("real"), h.path("link")).unwrap();
    let db = h.path("link/gone.db").display().to_string();
    let bk = h.dir("bk");
    h.ok(&["--db", &db, "moment", "x"]);
    h.ok(&[
        "--db", &db, "db", "backup", "daemon", "install", "--to", &bk, "--every", "1h",
    ]);
    std::fs::remove_file(h.path("real/gone.db")).unwrap();
    let said = h.ok(&["--db", &db, "db", "backup", "daemon", "uninstall"]);
    assert!(said.contains("removed"), "{said}");
    assert!(h.units().is_empty());
}

/// Every timer's run lands at a fixed random offset below its period, so
/// daemons installed together do not fire together (da-bw7ur).
#[test]
fn every_timer_has_a_fixed_random_delay_below_its_period() {
    let h = Home::new();
    for (i, every) in ["5s", "15m", "1h", "1d"].iter().enumerate() {
        h.install(&["--to", &h.dir(&format!("d{i}")), "--every", every]);
    }
    h.install(&["--all", "--to", &h.dir("all"), "--every", "2h"]);
    let units = h.units();
    assert_eq!(units.len(), 5);
    let secs = |text: &str, key: &str| -> i64 {
        let line = text.lines().find_map(|l| l.strip_prefix(key)).unwrap();
        line.trim_end_matches('s').parse().unwrap()
    };
    for (unit, service, timer) in &units {
        assert!(
            timer.contains("\nFixedRandomDelay=yes\n"),
            "{unit}: {timer}"
        );
        let period = secs(
            service
                .lines()
                .find(|l| l.starts_with("ExecStart="))
                .unwrap()
                .split(" --every ")
                .nth(1)
                .unwrap(),
            "",
        );
        let delay = secs(timer, "RandomizedDelaySec=");
        assert!(
            delay < period || period < 10 && delay == 0,
            "{unit}: {delay} of {period}"
        );
        // The wait and the most the delay can add are the period.
        assert_eq!(secs(timer, "OnUnitActiveSec=") + delay, period, "{unit}");
    }
}

/// `--db a,b` installs one daemon per database, each what its `--db` alone
/// makes, and each runs into its own subdirectory; uninstall and status take
/// the list too (da-bw7ur).
#[test]
fn a_list_installs_one_daemon_per_database() {
    let h = Home::new();
    let bk = h.dir("bk");
    h.ok(&["--db", "finance", "moment", "rent"]);
    h.ok(&["--db", "home", "moment", "plant"]);
    let list = ["--db", "finance,home", "db", "backup", "daemon"];
    let said = h.ok(&[&list[..], &["install", "--to", &bk, "--every", "1h"]].concat());
    assert_eq!(
        said.matches("installed the systemd user timer").count(),
        2,
        "{said}"
    );
    let units = h.units();
    assert_eq!(units.len(), 2, "{units:?}");
    for name in ["finance", "home"] {
        let (_, service, _) = (units.iter())
            .find(|(u, _, _)| u.starts_with(&format!("datum-backup-{name}-")))
            .unwrap();
        // Exactly the unit a single --db makes.
        assert!(
            service.contains(&format!("--db \"{name}\" db backup")),
            "{service}"
        );
        let ran = h.run_service(service);
        assert!(ran.status.success(), "{ran:?}");
        assert!(h.path(&format!("bk/{name}/index.json")).is_file(), "{name}");
    }
    assert!(!h.path("bk/data").exists());
    // Status shows the list's, and not the default database's.
    h.install(&["--to", &bk, "--every", "1d"]);
    let status = h.ok(&[&list[..], &["status"]].concat());
    assert_eq!(status.matches("datum db backup of").count(), 2, "{status}");
    assert!(!status.contains("datum-backup-data-"), "{status}");
    let said = h.ok(&[&list[..], &["uninstall"]].concat());
    assert_eq!(said.matches("removed").count(), 2, "{said}");
    assert_eq!(h.units().len(), 1);
    // An empty name in a list is refused, nothing installed.
    let out = h.datum(&[
        "--db", "finance,", "db", "backup", "daemon", "install", "--every", "1h",
    ]);
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(h.units().len(), 1);
    // One not there refuses the list before any is installed.
    let out = h.datum(&[
        "--db",
        "finance,nope",
        "db",
        "backup",
        "daemon",
        "install",
        "--to",
        &bk,
        "--every",
        "1h",
    ]);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no database at"),
        "{out:?}"
    );
    assert_eq!(h.units().len(), 1);
}

/// Any other command given a list refuses it, and makes no database.
#[test]
fn a_list_on_another_command_is_refused() {
    let h = Home::new();
    for args in [
        &["--db", "a,b", "moment", "x"][..],
        &["--db", "a,b", "tail"],
        &["--db=a,b", "db", "backup", "--to", "bk", "--every", "1h"],
    ] {
        let out = h.datum(args);
        assert!(!out.status.success(), "{args:?}: {out:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("takes one database"), "{args:?}: {err}");
    }
    let made: Vec<_> = std::fs::read_dir(h.path("data"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert!(
        !made.iter().any(|f| f.to_string_lossy().contains(',')),
        "{made:?}"
    );
}

/// `--all` is one daemon whose run backs up every database datum knows at
/// that run: one made after the install is taken, one deleted drops out,
/// and a run that fails for one names it, in status and check (da-bw7ur).
#[test]
fn all_backs_up_every_database_at_each_run() {
    let h = Home::new();
    let bk = h.dir("bk");
    h.install(&["--all", "--to", &bk, "--every", "1h"]);
    let units = h.units();
    assert_eq!(units.len(), 1);
    let (unit, service, _) = &units[0];
    assert!(unit.starts_with("datum-backup-all-"), "{unit}");
    assert!(service.contains(" db backup --all --to "), "{service}");
    assert!(!service.contains("X-Datum-Db="), "{service}");

    // Made after the install, and backed up by the next run.
    h.ok(&["--db", "later", "moment", "x"]);
    let ran = h.run_service(service);
    assert!(ran.status.success(), "{ran:?}");
    for name in ["data", "later"] {
        assert!(h.path(&format!("bk/{name}/index.json")).is_file(), "{name}");
    }
    let status = h.ok(&["db", "backup", "daemon", "status"]);
    assert!(status.contains("every database into"), "{status}");
    assert!(status.contains("  later: newest backup"), "{status}");

    // Deleted, it drops out: the run neither fails nor makes it again.
    std::fs::remove_file(h.path("data/later.db")).unwrap();
    std::fs::remove_dir_all(h.path("bk/later")).unwrap();
    let ran = h.run_service(service);
    assert!(ran.status.success(), "{ran:?}");
    assert!(!h.path("data/later.db").exists());
    assert!(!h.path("bk/later").exists());

    // One that cannot be read fails the run, which names it and takes the rest.
    std::fs::write(h.path("data/broken.db"), "not a database").unwrap();
    let ran = h.run_service(service);
    assert!(!ran.status.success(), "{ran:?}");
    let err = String::from_utf8_lossy(&ran.stderr);
    assert!(err.contains("no backup of broken ("), "{err}");
    assert!(!err.contains("data ("), "{err}");
    assert!(h.path("bk/data/index.json").is_file());
    // Check names a database the failed run missed (one made since), and
    // not one it took.
    std::fs::remove_file(h.path("data/broken.db")).unwrap();
    h.ok(&["--db", "since", "moment", "x"]);
    std::fs::write(h.path("result"), "exit-code\n").unwrap();
    let check = String::from_utf8(h.datum(&["--db", "since", "check", "install"]).stdout).unwrap();
    assert!(check.contains("the backup of since into"), "{check}");
    assert!(check.contains("none there yet"), "{check}");
    assert!(!h.check().contains("backup_daemon_failed"), "{}", h.check());
    std::fs::remove_file(h.path("result")).unwrap();

    // A database named `all` does not take every database's unit.
    h.ok(&["--db", "all", "moment", "x"]);
    let out = h.datum(&[
        "--db", "all", "db", "backup", "daemon", "install", "--to", &bk, "--every", "1h",
    ]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("every database"),
        "{out:?}"
    );

    // --all with --db names none, and one database's daemon is refused
    // into the DIR every database's backs up into, and the other way.
    let out = h.datum(&[
        "--db", "since", "db", "backup", "daemon", "install", "--all", "--every", "1h",
    ]);
    assert!(!out.status.success(), "{out:?}");
    let out = h.datum(&[
        "db", "backup", "daemon", "install", "--to", &bk, "--every", "1d",
    ]);
    assert!(!out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("every database into"),
        "{out:?}"
    );
    let other = h.dir("other");
    h.install(&["--to", &other, "--every", "1d"]);
    let out = h.datum(&[
        "db", "backup", "daemon", "install", "--all", "--to", &other, "--every", "1h",
    ]);
    assert!(!out.status.success(), "{out:?}");
    // Uninstall --all removes it alone.
    let said = h.ok(&["db", "backup", "daemon", "uninstall", "--all"]);
    assert!(said.contains(&format!("{unit}.timer")), "{said}");
    let left = h.units();
    assert_eq!(left.len(), 1);
    assert!(left[0].0.starts_with("datum-backup-data-"), "{left:?}");
}
