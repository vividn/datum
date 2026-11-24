//! datum follows no symbolic link at the files it keeps beside a database
//! (da-bv58i): a link at `<db>.writing` made a write overwrite the database it
//! named, and one at `.spool` copied it out into `.spool.rejected`. Each is
//! planted here at another database, and the command says the link by name
//! and leaves that database as it was, byte for byte. The database path itself
//! is followed unless `DATUM_DB_NOFOLLOW=1`.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        let env = Self {
            tmp: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(env.home()).unwrap();
        env
    }

    fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }

    /// A database of its own, `<name>/<name>.db`, holding one entry.
    fn db(&self, name: &str) -> PathBuf {
        let dir = self.tmp.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join(format!("{name}.db"));
        let out = self.run(&db, &["moment", "a", "-y", "-t", "10:00"], &[], "");
        assert!(out.status.success(), "{}", stderr(&out));
        db
    }

    fn run(&self, db: &Path, args: &[&str], env: &[(&str, &str)], stdin: &str) -> Output {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", db)
            .env("DATUM_HOME", self.home())
            .env("DATUM_CONFIG_HOME", self.home())
            .env("NO_COLOR", "1")
            .envs(env.iter().copied())
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn beside(db: &Path, ext: &str) -> PathBuf {
    let mut p = db.as_os_str().to_owned();
    p.push(ext);
    p.into()
}

/// A write through `k.db` with a link at each of its sidecars to `j.db` names
/// the link, and `j.db` is unchanged. A link at `.writing`, `.spool` or
/// `.spool.lock` refuses the write. One at `.spool.rejected` or `.spool.torn`
/// does not: the line that would have been set aside through it waits in the
/// spool, as a line that cannot land does, and the write queues behind it.
#[test]
fn a_link_at_a_sidecar_is_named_and_never_followed() {
    let env = Env::new();
    let j = env.db("j");
    let before = std::fs::read(&j).unwrap();
    for (side, spooled) in [
        (".writing", None),
        (".spool", None),
        (".spool.lock", None),
        // A line in the middle that does not read goes to `.rejected`, the
        // last one cut off to `.torn`.
        (
            ".spool.rejected",
            Some("{\"id\":\"x\"\n{\"id\":\"y\",\"at\":\"2026-01-01T00:00:00Z\",\"ops\":[]}\n"),
        ),
        (".spool.torn", Some("{\"id\":\"x\"")),
    ] {
        let k = env.db(&format!("k{}", side.replace('.', "_")));
        if let Some(lines) = spooled {
            std::fs::write(beside(&k, ".spool"), lines).unwrap();
        }
        let link = beside(&k, side);
        std::os::unix::fs::symlink(&j, &link).unwrap();
        // `script --atomic` always queues, so it takes the spool's lock and
        // writes the spool whether or not the write lock is free. Behind a
        // line that waits, a plain write queues too.
        let out = match spooled {
            None => env.run(&k, &["script", "--atomic"], &[], "moment b -y -t 11:00\n"),
            Some(_) => env.run(&k, &["moment", "b", "-y", "-t", "11:00"], &[], ""),
        };
        let err = stderr(&out);
        assert_eq!(out.status.success(), spooled.is_some(), "{side}: {err}");
        assert!(
            err.contains(&format!("{} is a symbolic link", link.display())),
            "{side}: {err}"
        );
        assert!(
            std::fs::symlink_metadata(&link).unwrap().is_symlink(),
            "{side}"
        );
        assert!(std::fs::read(&j).unwrap() == before, "{side}: j.db changed");
        // The line that did not read is still queued, and the write after it.
        if spooled.is_some() {
            let spool = std::fs::read_to_string(beside(&k, ".spool")).unwrap();
            assert!(spool.starts_with("{\"id\":\"x\""), "{side}: {spool}");
            assert!(spool.contains("\"field\":\"b\""), "{side}: {spool}");
        }
    }
}

/// `DATUM_DB_NOFOLLOW=1` refuses a database that is a link, naming it, for a
/// write and a read alike; without it the link is followed, as before.
#[test]
fn the_opt_in_refuses_a_linked_database() {
    let env = Env::new();
    let j = env.db("j");
    let link = env.tmp.path().join("linked.db");
    std::os::unix::fs::symlink(&j, &link).unwrap();
    let before = std::fs::read(&j).unwrap();
    let opt_in = [("DATUM_DB_NOFOLLOW", "1")];
    for args in [&["moment", "b", "-y", "-t", "11:00"][..], &["tail"]] {
        let out = env.run(&link, args, &opt_in, "");
        let err = stderr(&out);
        assert!(!out.status.success(), "{args:?}: {err}");
        assert!(
            err.contains(&format!("{} is a symbolic link", link.display())),
            "{args:?}: {err}"
        );
    }
    assert!(std::fs::read(&j).unwrap() == before, "j.db changed");

    let out = env.run(&link, &["moment", "b", "-y", "-t", "11:00"], &[], "");
    assert!(out.status.success(), "{}", stderr(&out));
    let tail = env.run(&j, &["tail"], &opt_in, "");
    assert!(
        String::from_utf8_lossy(&tail.stdout).contains(" b "),
        "{}",
        String::from_utf8_lossy(&tail.stdout)
    );
}

/// `sync chain reenter` opens the database it re-enters as every other open
/// does (da-j18ms): under `DATUM_DB_NOFOLLOW=1` a link is refused, naming it,
/// and the file it names is left byte for byte. Without the opt-in the link is
/// followed to the file, which, at this datum's schema, has nothing to
/// re-enter.
#[test]
fn the_opt_in_refuses_a_linked_database_to_reenter() {
    let env = Env::new();
    let j = env.db("j");
    let link = env.tmp.path().join("linked.db");
    std::os::unix::fs::symlink(&j, &link).unwrap();
    let before = std::fs::read(&j).unwrap();
    let reenter = ["sync", "chain", "reenter"];

    let out = env.run(&link, &reenter, &[("DATUM_DB_NOFOLLOW", "1")], "");
    let err = stderr(&out);
    assert!(!out.status.success(), "{err}");
    assert!(
        err.contains(&format!("{} is a symbolic link", link.display())),
        "{err}"
    );
    assert!(std::fs::read(&j).unwrap() == before, "j.db changed");

    let out = env.run(&link, &reenter, &[], "");
    assert!(
        stderr(&out).contains("nothing to re-enter"),
        "{}",
        stderr(&out)
    );
}
