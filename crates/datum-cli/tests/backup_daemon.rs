//! `datum backup --auto` and `db backup daemon` (da-gbac0, da-tsna7): the
//! systemd user timers and services
//! written to a throwaway home, `systemctl` faked on PATH so nothing reaches
//! a real service manager. The fake says a timer is not loaded (exit 4)
//! while its name is a line of the file `inactive`, not enabled while it is
//! one of `disabled`, cannot reach the user
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
             *is-enabled*) grep -qxF \"$4\" {disabled} 2>/dev/null && exit 1; exit 0 ;;\n\
             *show*) echo \"Result=$(cat {result} 2>/dev/null || echo success)\"; \
                     echo \"ExecMainExitTimestamp=$(cat {ran} 2>/dev/null)\" ;;\n\
             esac\n",
            log = p("ctl.log"),
            inactive = p("inactive"),
            disabled = p("disabled"),
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

    /// A rotation's period and cap as DIR records them for the step: every
    /// database's at DIR's top, or the one database's in its subdirectory.
    fn schedule(&self, service: &str) -> (i64, Option<u64>) {
        let to = (service.lines())
            .find_map(|l| l.strip_prefix("X-Datum-To="))
            .unwrap();
        let read = |p: PathBuf| -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
        };
        let rotation = match service.contains("X-Datum-All=yes") {
            true => read(PathBuf::from(to).join("index.json"))["all"].clone(),
            false => {
                let dbs = read(PathBuf::from(to).join("data/index.json"))["dbs"].clone();
                dbs.as_object().unwrap().values().next().unwrap().clone()
            }
        };
        (
            rotation["every_secs"].as_i64().unwrap(),
            rotation["slots"].as_u64(),
        )
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
            "ExecStart=\"{}\" --db \"{}\" db backup --to \"{bk}\"\n",
            exe.display(),
            db.display(),
        )),
        "{service}"
    );
    assert!(service.contains("Type=oneshot"), "{service}");
    // The step names no period: DIR records it (da-tsna7).
    assert_eq!(h.schedule(&service), (900, None));
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
        service.contains(&format!("--to \"{backups}\"\n")),
        "{service}"
    );
    assert_eq!(h.schedule(&service), (3600, None));
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
    assert_eq!(h.schedule(&service), (3600, Some(8)));
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
        let period = h.schedule(service).0;
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
        &["--db=a,b", "db", "backup", "--to", "bk"],
        &["--db=a,b", "backup", "--auto", "--dry-run"],
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

/// `db rename` takes the backup daemons with it (da-a0vw0): each unit backs
/// up the new name, its timer enabled again under its new name, and each
/// destination's rotation, the `--all` daemon's too, goes on under NEW/.
#[test]
fn rename_takes_the_daemons_and_their_rotations() {
    let h = Home::new();
    let (here, drive, every) = (h.dir("here"), h.dir("drive"), h.dir("every"));
    h.install(&["--to", &here, "--every", "15m"]);
    h.install(&["--to", &drive, "--every", "1d"]);
    h.install(&["--all", "--to", &every, "--every", "1h"]);
    for (_, service, _) in h.units() {
        assert!(h.run_service(&service).status.success());
    }
    let old: Vec<_> = h.units().into_iter().map(|(u, _, _)| u).collect();
    // A timer someone disabled stays disabled under its new name.
    let (off, _, _) = h.unit_into(&drive);
    std::fs::write(h.path("disabled"), format!("{off}.timer\n")).unwrap();

    let said = h.ok(&["db", "rename", "fresh"]);
    let fresh = std::fs::canonicalize(h.path("data/fresh.db")).unwrap();
    for to in [&here, &drive] {
        let (unit, service, timer) = h.unit_into(to);
        assert!(unit.starts_with("datum-backup-fresh-"), "{unit}");
        assert!(!old.contains(&unit), "{unit}");
        assert!(service.contains("--db \"fresh\" db backup"), "{service}");
        assert!(
            service.contains(&format!("X-Datum-Db={}\n", fresh.display())),
            "{service}"
        );
        assert!(timer.contains("Description=datum db backup of fresh into"));
        assert!(said.contains(&format!("{to}/data → {to}/fresh")), "{said}");
        let log = std::fs::read_to_string(h.path("ctl.log")).unwrap();
        let enabled = log.contains(&format!("enable {unit}.timer"));
        assert_eq!(enabled, to == &here, "{log}");
    }
    assert!(
        said.contains(&format!("{every}/data → {every}/fresh")),
        "{said}"
    );
    assert_eq!(h.units().len(), 3);
    let check =
        || String::from_utf8(h.datum(&["--db", "fresh", "check", "install"]).stdout).unwrap();
    assert!(!check().contains("backup_daemon"), "{}", check());

    // A run backs up the renamed database, not a new empty one by the old
    // name, and goes on with the rotation it began: slot 0 holds the period.
    for (_, service, _) in h.units() {
        let ran = h.run_service(&service);
        assert!(ran.status.success(), "{ran:?}");
        let out = String::from_utf8_lossy(&ran.stdout);
        assert!(out.contains("Slot 0 holds this period"), "{out}");
    }
    assert!(!h.path("data/data.db").exists());
    for to in ["here", "drive", "every"] {
        assert!(!h.path(&format!("{to}/data")).exists(), "{to}");
        let index = std::fs::read_to_string(h.path(&format!("{to}/fresh/index.json"))).unwrap();
        let index: serde_json::Value = serde_json::from_str(&index).unwrap();
        let rotation = &index["dbs"]["fresh"]["backups"];
        assert_eq!(index["dbs"].as_object().unwrap().len(), 1, "{index}");
        assert!(rotation[0]["slot"] == 0, "{index}");
    }
    // `check` of the new name reads its daemons as its own.
    std::fs::write(h.path("result"), "exit-code\n").unwrap();
    let check = check();
    assert!(check.contains(&format!("into {here} failed")), "{check}");
    assert!(check.contains(&format!("into {drive} failed")), "{check}");
    std::fs::remove_file(h.path("result")).unwrap();
    let status = h.ok(&["db", "backup", "daemon", "status"]);
    assert!(status.contains("  fresh: newest backup"), "{status}");

    // Where NEW/ is there already, nothing moves, and the echo says so.
    std::fs::create_dir(h.path("here/again")).unwrap();
    let said = h.ok(&["--db", "fresh", "db", "rename", "again"]);
    assert!(
        said.contains(&format!("kept {here}/fresh, as {here}/again is there")),
        "{said}"
    );
    assert!(h.path("here/fresh/index.json").is_file());
    assert!(h.path("drive/again/index.json").is_file());
}

/// A daemon of the new name another database holds refuses the rename's
/// part before any daemon is touched (da-a0vw0).
#[test]
fn rename_onto_another_databases_unit_touches_no_daemon() {
    let h = Home::new();
    let (here, drive) = (h.dir("here"), h.dir("drive"));
    h.install(&["--to", &here, "--every", "1h"]);
    h.install(&["--to", &drive, "--every", "1h"]);
    // A path whose stem is `fresh`, backed up into drive: its unit is the
    // name the drive daemon of `fresh` would take.
    let path = h.path("elsewhere/fresh.db").display().to_string();
    h.ok(&["--db", &path, "moment", "x"]);
    h.ok(&[
        "--db", &path, "db", "backup", "daemon", "install", "--to", &drive, "--every", "1d",
    ]);
    let before = h.units();
    let out = h.datum(&["db", "rename", "fresh"]);
    assert!(!out.status.success(), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("uninstall one, then install it again"),
        "{err}"
    );
    assert_eq!(h.units(), before);
}

/// da-tsna7: a backup to a file is one file, and starts nothing: no unit,
/// no call to systemctl, no rotation (vividn, 2026-11-04: «a user just
/// trying to make a manual backup doesn't get a hanoi setup going»).
#[test]
fn a_backup_to_a_file_installs_nothing() {
    let h = Home::new();
    let out = h.path("out.jsonl.gz").display().to_string();
    for words in [
        &["backup", &out][..],
        &["db", "backup", &out, "--overwrite"],
    ] {
        h.ok(words);
        assert!(h.path("out.jsonl.gz").is_file());
    }
    assert!(h.units().is_empty());
    assert!(!h.path("ctl.log").exists());
    assert!(!h.path("data/backups").exists());
}

/// `--auto` is a timer every 30m, `--every D` one every D and `--auto` too,
/// `--no-auto` takes it away, and `status` is the one place to look.
#[test]
fn auto_installs_every_30m_every_sets_it_and_no_auto_removes_it() {
    let h = Home::new();
    let bk = h.dir("bk");
    let said = h.ok(&["backup", "--to", &bk, "--auto"]);
    assert!(said.contains("every 30m"), "{said}");
    let (unit, service, timer) = h.unit_into(&bk);
    assert_eq!(h.schedule(&service), (1800, None));
    assert!(timer.contains("OnUnitActiveSec=1620s\n"), "{timer}");
    // --every sets the period, and the same DIR's timer is replaced.
    h.ok(&["backup", "--to", &bk, "--every", "1h", "--slots", "6"]);
    let (again, service, timer) = h.unit_into(&bk);
    assert_eq!(again, unit);
    assert_eq!(h.schedule(&service), (3600, Some(6)));
    assert!(timer.contains("OnUnitActiveSec=3300s\n"), "{timer}");
    // No --to: the data directory's backups/.
    h.ok(&["backup", "--auto"]);
    let backups = h.dir("data/backups");
    assert_eq!(h.schedule(&h.unit_into(&backups).1), (1800, None));

    let status = h.ok(&["backup", "status"]);
    assert_eq!(status, h.ok(&["db", "backup", "daemon", "status"]));
    assert!(status.contains(&format!("into {bk}, every 1h")), "{status}");
    assert!(
        status.contains(&format!("into {backups}, every 30m")),
        "{status}"
    );

    let said = h.ok(&["backup", "--to", &bk, "--no-auto"]);
    assert!(said.contains(&format!("{unit}.timer, into {bk}")), "{said}");
    assert_eq!(h.units().len(), 1);
    h.ok(&["backup", "--no-auto"]);
    assert!(h.units().is_empty());
    assert!(h.ok(&["backup", "status"]).contains("none installed"));
}

/// The timer's step is `backup --to DIR`, no --every: DIR's index records
/// the period and the cap, and a step by hand into a DIR holding no rotation
/// is refused and writes nothing. Its slot restores like any backup, and the
/// index keys it by the database it opened (da-d5cmj, da-o6fvk).
#[test]
fn the_step_reads_its_period_from_dir() {
    let h = Home::new();
    h.ok(&["--db", "fitness", "moment", "pushups"]);
    let empty = h.dir("empty");
    let out = h.datum(&["--db", "fitness", "backup", "--to", &empty]);
    assert!(!out.status.success(), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("holds no rotation of fitness"), "{err}");
    assert!(err.contains("--auto"), "{err}");
    assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);

    let bk = h.dir("bk");
    h.ok(&["--db", "fitness", "backup", "--to", &bk, "--every", "1h"]);
    let (_, service, _) = h.unit_into(&bk);
    let exec = service
        .lines()
        .find(|l| l.starts_with("ExecStart="))
        .unwrap();
    assert!(
        exec.ends_with(&format!("--db \"fitness\" db backup --to \"{bk}\"")),
        "{exec}"
    );
    let ran = h.run_service(&service);
    assert!(ran.status.success(), "{ran:?}");
    let took: serde_json::Value =
        serde_json::from_str(&h.ok(&["--db", "fitness", "backup", "--to", &bk, "--json"])).unwrap();
    // The run above took slot 0, and this one is in its period.
    assert_eq!(took["slot"], serde_json::Value::Null, "{took}");

    let listed: serde_json::Value =
        serde_json::from_str(&h.ok(&["backup", "--list", &bk, "--json"])).unwrap();
    assert_eq!(listed["backups"][0]["source_db"], "fitness");
    assert_eq!(listed["rotations"]["fitness"]["every_secs"], 3600);
    let file = listed["backups"][0]["file"].as_str().unwrap();
    assert!(file.starts_with("fitness/"), "{file}");
    let restored: serde_json::Value = serde_json::from_str(&h.ok(&[
        "--db",
        "fresh",
        "db",
        "restore",
        &format!("{bk}/{file}"),
        "--json",
    ]))
    .unwrap();
    assert_eq!(restored["inserted"], listed["backups"][0]["entry_count"]);
}

/// A unit installed before the step read DIR runs `--to DIR --every D`,
/// which now installs a timer and would take no backup again. Run by
/// systemd it rewrites itself to the step, takes the step, and leaves its
/// timer enabled and running, as any install does.
#[test]
fn a_unit_from_before_rewrites_itself_as_it_runs() {
    let h = Home::new();
    let bk = h.dir("bk");
    h.install(&["--to", &bk, "--every", "1h"]);
    let (unit, service, _) = h.unit_into(&bk);
    let old = service.replace(
        &format!("--to \"{bk}\"\n"),
        &format!("--to \"{bk}\" --every 3600s\n"),
    );
    assert_ne!(old, service);
    let file = h.path(&format!("config/systemd/user/{unit}.service"));
    std::fs::write(&file, &old).unwrap();
    std::fs::remove_file(h.path("ctl.log")).unwrap();

    let line = old
        .lines()
        .find_map(|l| l.strip_prefix("ExecStart="))
        .unwrap();
    let ran = h
        .command("sh")
        .args(["-c", line])
        .env("INVOCATION_ID", "0123")
        .output()
        .unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), service);
    let index = std::fs::read_to_string(h.path("bk/data/index.json")).unwrap();
    assert!(index.contains("\"slot\": 0"), "{index}");
    assert!(
        String::from_utf8_lossy(&ran.stdout).contains("rewrote"),
        "{ran:?}"
    );
    let log = std::fs::read_to_string(h.path("ctl.log")).unwrap();
    assert!(log.contains(&format!("restart {unit}.timer")), "{log}");
    // Run by hand, the same line installs, and takes no step.
    std::fs::remove_file(h.path("ctl.log")).unwrap();
    std::fs::write(&file, &old).unwrap();
    let ran = h.command("sh").args(["-c", line]).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert!(
        !String::from_utf8_lossy(&ran.stdout).contains("rewrote"),
        "{ran:?}"
    );
    let log = std::fs::read_to_string(h.path("ctl.log")).unwrap();
    assert!(log.contains(&format!("restart {unit}.timer")), "{log}");
}

/// `--dry-run` (and `--plan`, its older name) says what `--auto` would set
/// up and, with `--span`, what the rotation would hold, writing nothing.
#[test]
fn dry_run_says_what_it_would_set_up_and_writes_nothing() {
    let h = Home::new();
    let bk = h.path("bk").display().to_string();
    let dry = ["backup", "--to", &bk, "--every", "1h", "--slots", "4"];
    let said = h.ok(&[&dry[..], &["--dry-run", "--span", "7d"]].concat());
    assert!(
        said.contains("would install the systemd user timer datum-backup-data-"),
        "{said}"
    );
    assert!(
        said.contains(&format!("into {bk}, every 1h, 4 slots at most")),
        "{said}"
    );
    assert!(said.contains("after 7d: 5 files"), "{said}");
    assert_eq!(
        said,
        h.ok(&[&dry[..], &["--plan", "--span", "7d"]].concat())
    );
    let doc: serde_json::Value =
        serde_json::from_str(&h.ok(&[&dry[..], &["--dry-run", "--json"]].concat())).unwrap();
    assert!(
        doc["timer"]
            .as_str()
            .unwrap()
            .starts_with("datum-backup-data-"),
        "{doc}"
    );
    assert_eq!(
        (doc["every_secs"].as_i64(), doc["slots"].as_i64()),
        (Some(3600), Some(4))
    );
    // --auto alone would be every 30m.
    let said = h.ok(&["backup", "--to", &bk, "--auto", "--dry-run"]);
    assert!(said.contains("every 30m"), "{said}");
    assert!(h.units().is_empty());
    assert!(!h.path("bk").exists());
    assert!(!h.path("ctl.log").exists());
}
