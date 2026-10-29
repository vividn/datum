//! A read of a database this process cannot write opens it read-only, and
//! writes nothing anywhere (da-uxfj3, vividn's town-data recipe 2): a town
//! database is one writer's, read by everyone. The real binary, run against
//! a database 0444 in a 0555 directory, which needs a non-root runner: root
//! writes through both modes.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime};

use tempfile::TempDir;

const KIT: &str = r#"
[kit]
name = "readkit"

[[view]]
name = "howmany"
sql = "SELECT COUNT(*) AS n FROM entries"

[[command]]
name = "howmany"
view = "howmany"
"#;

struct Env {
    tmp: TempDir,
}

impl Env {
    /// A database of a few entries, an alias and a kit, in `town/`.
    fn new() -> Self {
        let env = Env {
            tmp: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir(env.dir()).unwrap();
        let kit = env.tmp.path().join("readkit.toml");
        std::fs::write(&kit, KIT).unwrap();
        for line in [
            &["switch", "mood", "happy", "-t", "-60"][..],
            &["start", "work", "-t", "-30"],
            &["occur", "coffee"],
            &["alias", "set", "vbp", "note word pos=verb"],
            &["kit", "install", "--yes", kit.to_str().unwrap()],
        ] {
            env.ok(line);
        }
        env
    }

    fn dir(&self) -> PathBuf {
        self.tmp.path().join("town")
    }

    fn db(&self) -> PathBuf {
        self.dir().join("town.db")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", self.db())
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .args(args);
        c
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn err(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "`datum {}` read", args.join(" "));
        String::from_utf8(out.stderr).unwrap()
    }

    /// Every file in `town/`: its bytes and modification time.
    fn listing(&self) -> BTreeMap<String, (Vec<u8>, SystemTime)> {
        std::fs::read_dir(self.dir())
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                let m = e.metadata().unwrap();
                let bytes = std::fs::read(e.path()).unwrap_or_default();
                let name = e.file_name().into_string().unwrap();
                (name, (bytes, m.modified().unwrap()))
            })
            .collect()
    }

    /// Make `files` in `town/` 0444 and `town/` itself 0555, until the
    /// guard drops.
    fn read_only(&self, files: &[&str]) -> Writable {
        for f in files {
            chmod(&self.dir().join(f), 0o444);
        }
        chmod(&self.dir(), 0o555);
        Writable(self.dir())
    }
}

fn chmod(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// Gives `town/` back to its owner, so the tempdir can go.
struct Writable(PathBuf);

impl Drop for Writable {
    fn drop(&mut self) {
        chmod(&self.0, 0o755);
        for e in std::fs::read_dir(&self.0).unwrap() {
            chmod(&e.unwrap().path(), 0o644);
        }
    }
}

/// Root writes a 0444 file in a 0555 directory: these tests need a runner
/// that is not.
fn root() -> bool {
    // SAFETY: reads this process's effective uid.
    let root = unsafe { libc::geteuid() } == 0;
    if root {
        eprintln!("skipped: run as root, which writes through a 0444 file's mode");
    }
    root
}

/// Every read command, against a database it cannot write, reads it, and
/// the directory's listing and every file's bytes and mtime are as they
/// were: in a directory it cannot write, and in one it can, where SQLite
/// would make a `-wal` and a `-shm` for it.
#[test]
fn every_read_reads_a_db_it_cannot_write_and_writes_nothing() {
    if root() {
        return;
    }
    let env = Env::new();
    let backup = env.tmp.path().join("backup.jsonl");
    let owners = env.ok(&["howmany"]);
    let reads = [
        &["tail"][..],
        &["head"],
        &["now"],
        &["get", "mood"],
        &["history", "mood"],
        &["why", "mood"],
        &["grep", "happy"],
        &["check"],
        &["count", "mood"],
        &["stats"],
        &["stats", "mood"],
        &["ids"],
        &["fields"],
        &["states", "mood"],
        &["dayview"],
        &["nowview"],
        &["view", "howmany"],
        &["howmany"],
        &["kit", "list"],
        &["kit", "show", "readkit"],
        &["setup"],
        &["setup", "mood"],
        &["alias", "list"],
        &["alias", "show", "vbp"],
        &["sync", "status"],
        &["db", "backup", "--overwrite", backup.to_str().unwrap()],
    ];
    for dir in [0o555, 0o755] {
        let _guard = env.read_only(&["town.db"]);
        chmod(&env.dir(), dir);
        let before = env.listing();
        assert_eq!(before.keys().collect::<Vec<_>>(), ["town.db"]);
        for read in reads {
            env.ok(read);
            assert!(
                env.listing() == before,
                "`datum {}` changed the directory, {dir:o}",
                read.join(" ")
            );
        }
        assert_eq!(env.ok(&["howmany"]), owners);
        assert!(env.ok(&["tail"]).contains("happy"));
    }
}

/// No migration runs on a reader's open: a schema older than this datum's
/// is refused, naming both, and left as it was.
#[test]
fn a_reader_refuses_an_older_schema_naming_both_versions() {
    if root() {
        return;
    }
    let env = Env::new();
    let current = datum_core::migrations::CURRENT_SCHEMA_VERSION;
    let conn = rusqlite::Connection::open(env.db()).unwrap();
    conn.execute(
        "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
        [(current - 1).to_string()],
    )
    .unwrap();
    drop(conn);
    let _guard = env.read_only(&["town.db"]);
    let before = env.listing();
    let err = env.err(&["tail"]);
    for said in [
        format!("schema v{}", current - 1),
        format!("reads v{current}"),
        "its owner opens it once to upgrade".to_string(),
    ] {
        assert!(err.contains(&said), "{err}");
    }
    assert!(
        env.listing() == before,
        "the refused read changed the directory"
    );
}

/// While its writer holds a write transaction open, a reader reads through
/// the writer's `-wal` and `-shm`, read-only to it as well, and gets what
/// was committed; a `-shm` it cannot read is refused as the writer's.
#[test]
fn a_reader_reads_through_its_writers_open_session() {
    if root() {
        return;
    }
    let env = Env::new();
    let writer = rusqlite::Connection::open(env.db()).unwrap();
    writer
        .execute_batch(
            "BEGIN IMMEDIATE;
             UPDATE entries SET state = '\"sad\"' WHERE field = 'mood';",
        )
        .unwrap();
    let guard = env.read_only(&["town.db", "town.db-wal", "town.db-shm"]);
    let before = env.listing();
    let tail = env.ok(&["tail"]);
    assert!(tail.contains("happy") && !tail.contains("sad"), "{tail}");
    assert!(env.listing() == before, "the read changed the directory");

    chmod(&env.dir().join("town.db-shm"), 0o000);
    let err = env.err(&["tail"]);
    assert!(err.contains("open in its writer's datum"), "{err}");
    drop(guard);
    writer.execute_batch("ROLLBACK").unwrap();
}

/// A watch reads a checkpointed file `immutable`, which SQLite trusts not
/// to change: once its owner writes, the watch says so and ends, rather than
/// show a file it no longer reads.
#[test]
fn a_readers_watch_ends_once_its_owner_writes() {
    if root() {
        return;
    }
    let env = Env::new();
    let guard = env.read_only(&["town.db"]);
    let mut watch = env
        .command(&["dayview", "-w"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Read;
    // Its first render, then the rest drained, so the pipe stays open.
    let mut out = watch.stdout.take().unwrap();
    out.read_exact(&mut [0u8; 1]).unwrap();
    std::thread::spawn(move || std::io::copy(&mut out, &mut std::io::sink()));
    // The owner writes, past a coarse clock's tick.
    std::thread::sleep(Duration::from_millis(20));
    drop(guard);
    env.ok(&["occur", "coffee"]);
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = watch.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            watch.kill().unwrap();
            panic!("the watch went on reading a file that changed");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    watch
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(!status.success());
    assert!(stderr.contains("the database changed"), "{stderr}");
}

/// A reader takes no spool lock, and a spool it cannot read is said, not
/// read as empty: what the owner queued is unknown to it, not absent.
#[test]
fn a_reader_says_so_when_it_cannot_read_the_spool() {
    if root() {
        return;
    }
    let env = Env::new();
    std::fs::write(env.dir().join("town.db.spool"), "{}\n").unwrap();
    chmod(&env.dir().join("town.db.spool"), 0o000);
    let _guard = env.read_only(&["town.db"]);
    let err = env.err(&["check", "spool"]);
    assert!(
        err.contains("cannot read") && err.contains("town.db.spool"),
        "{err}"
    );
}
