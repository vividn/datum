//! da-7r1v4: every command that changes something takes `--dry-run`, prints
//! what it would do, and writes nothing — no row, no sync op, no file. A
//! command with no dry run of its own runs on a copy of the database in
//! memory, its action word DRYRUN; one that only reads refuses the flag.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};

/// `datum <args>` with its database, home and config inside `dir`.
fn datum(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.join("t.db"))
        .env("HOME", dir)
        .env("XDG_CONFIG_HOME", dir.join(".config"))
        .env("XDG_DATA_HOME", dir.join(".local/share"))
        .env("NO_COLOR", "1")
        .env("EDITOR", "true")
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}

/// Every file under `dir`, by path, with its bytes: the database, its sync
/// log (the `ops` table is in it), anything made beside it, the config.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(at: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(at).unwrap() {
            let p = e.unwrap().path();
            match p.is_dir() {
                true => walk(&p, into),
                false => {
                    into.insert(p.display().to_string(), std::fs::read(&p).unwrap());
                }
            }
        }
    }
    let mut all = BTreeMap::new();
    walk(dir, &mut all);
    all
}

fn seeded() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for line in [
        &["moment", "coffee"][..],
        &["change", "mood", "happy"],
        &["on", "work"],
        &["alias", "set", "cf", "moment coffee"],
    ] {
        let out = datum(dir.path(), line);
        assert!(out.status.success(), "{line:?}: {out:?}");
    }
    dir
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn every_write_dry_runs_and_writes_nothing() {
    let dir = seeded();
    let before = files(dir.path());
    for line in [
        // run on a copy
        &["retime", "coffee", "-t", "-5"][..],
        &["undo"],
        &["revert", "coffee"],
        &["ignore", "coffee"],
        &["setup", "coffee", "--kind", "on"],
        &["alias", "set", "cg", "moment coffee"],
        &["alias", "remove", "cf"],
        &["check", "--fix"],
        &["db", "config", "auto_setup", "false"],
        &["kit", "uninstall", "nothing"],
        &["flag", "coffee"],
        &["edit", "coffee"],
        // their own
        &["moment", "tea"],
        &["change", "mood", "sad"],
        &["off", "work"],
        &["update", "coffee", "x=1"],
        &["delete", "coffee"],
        &["rename", "coffee", "tea"],
    ] {
        let mut args = line.to_vec();
        args.push("--dry-run");
        let out = datum(dir.path(), &args);
        assert!(out.status.success(), "{args:?}: {}", text(&out));
        assert!(
            text(&out).contains("[dry-run]"),
            "{args:?} says it is a dry run: {}",
            text(&out)
        );
        assert!(files(dir.path()) == before, "{args:?} wrote something");
    }
    // The control: the same comparison sees a real write.
    let out = datum(dir.path(), &["retime", "coffee", "-t", "-5"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        files(dir.path()) != before,
        "a real retime changes the file"
    );
}

#[test]
fn a_dry_run_echoes_dryrun_as_its_action() {
    let dir = seeded();
    let out = datum(
        dir.path(),
        &["retime", "coffee", "-t", "-5", "--dry-run", "--json"],
    );
    assert!(out.status.success(), "{}", text(&out));
    let echo: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(echo["docs"][0]["action"], "DRYRUN", "{echo}");
    assert_eq!(echo["docs"][0]["field"], "coffee", "{echo}");
}

#[test]
fn a_dry_run_makes_no_database() {
    let dir = tempfile::tempdir().unwrap();
    let new = dir.path().join("new.db");
    for line in [
        &["moment", "coffee"][..],
        &["setup", "coffee", "--kind", "on"],
    ] {
        let mut args = vec!["--db", new.to_str().unwrap()];
        args.extend(line);
        args.push("--dry-run");
        let out = datum(dir.path(), &args);
        assert!(out.status.success(), "{args:?}: {}", text(&out));
        assert!(!new.exists(), "{args:?} made the database");
    }
}

#[test]
fn a_read_refuses_a_dry_run() {
    let dir = seeded();
    let before = files(dir.path());
    for (line, words) in [
        (&["tail"][..], "tail"),
        (&["get", "coffee"], "get"),
        (&["stats", "coffee"], "stats"),
        (&["sync", "status"], "sync status"),
        (&["alias", "list"], "alias list"),
    ] {
        let mut args = line.to_vec();
        args.push("--dry-run");
        let out = datum(dir.path(), &args);
        assert!(!out.status.success(), "{args:?}: {}", text(&out));
        let said = format!("nothing to dry-run: {words} writes nothing");
        assert!(text(&out).contains(&said), "{args:?}: {}", text(&out));
    }
    // One that writes outside the database says it has none.
    let out = datum(dir.path(), &["alias", "install", "zsh", "--dry-run"]);
    assert!(!out.status.success());
    assert!(
        text(&out).contains("alias install has no --dry-run"),
        "{}",
        text(&out)
    );
    assert!(files(dir.path()) == before, "a refusal wrote something");
}

/// A dry run says what the real run would: a line that would change nothing
/// keeps NODIFF, and only a write is DRYRUN.
#[test]
fn a_dry_run_that_would_change_nothing_says_so() {
    let dir = seeded();
    let action = |args: &[&str]| {
        let out = datum(dir.path(), args);
        assert!(out.status.success(), "{args:?}: {}", text(&out));
        let echo: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        echo["docs"][0]["action"].as_str().unwrap().to_string()
    };
    let same = ["setup", "coffee", "--kind", "moment", "--json"];
    assert_eq!(action(&same), "NODIFF");
    assert_eq!(action(&[&same[..], &["--dry-run"]].concat()), "NODIFF");
    let other = ["setup", "coffee", "--kind", "on", "--json", "--dry-run"];
    assert_eq!(action(&other), "DRYRUN");
}

/// A database that is there and will not open fails a dry run as it fails
/// the real run, and is not read as an empty one.
#[cfg(unix)]
#[test]
fn a_dry_run_of_a_database_that_will_not_open_fails() {
    use std::os::unix::fs::PermissionsExt;
    let dir = seeded();
    let db = dir.path().join("t.db");
    std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&db).is_ok() {
        return; // root reads it anyway
    }
    let line = ["setup", "tea", "--kind", "on"];
    let real = datum(dir.path(), &line);
    let dry = datum(dir.path(), &[&line[..], &["--dry-run"]].concat());
    std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!real.status.success(), "{}", text(&real));
    assert!(
        !dry.status.success(),
        "the dry run read it as empty: {}",
        text(&dry)
    );
    assert!(text(&dry).contains("unable to open"), "{}", text(&dry));
}

/// datum.day's sync store asks `sync --as <ENDPOINT> chain leave --alone
/// --dry-run` before a join (`crates/datum-sync-wire/src/web.rs`): `--as`
/// keeps leave's own dry run, which says it may and writes nothing.
#[test]
fn a_dry_run_of_its_own_holds_under_sync_as() {
    let dir = seeded();
    let out = datum(dir.path(), &["sync", "new-invite"]);
    assert!(out.status.success(), "{}", text(&out));
    let list = datum(dir.path(), &["sync", "chain", "list", "--json"]);
    let list: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let me = list["members"][0]["endpoint"].as_str().unwrap().to_string();
    let before = files(dir.path());
    let leave = ["chain", "leave", "--alone", "--dry-run"];
    let out = datum(dir.path(), &[&["sync", "--as", &me][..], &leave].concat());
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(files(dir.path()), before, "the dry run wrote");
}
