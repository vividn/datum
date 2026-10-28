//! `datum sync grant` (da-n0uj1.4): a device in one chain with the granter
//! joins another with no ticket carried by hand, its invite sealed to it in a
//! record of the chain the two share. Installs of the real binary, each with
//! databases by name in its own `DATUM_HOME`, each database on a localhost
//! port of its own (`DATUM_SYNC_BIND`), as in `sync_net`.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::TempDir;

use crate::sync_net::claim_port;

/// One install: a data directory, so one device key, and its databases.
struct Install {
    tmp: TempDir,
    user: &'static str,
    ports: std::cell::RefCell<Vec<(&'static str, u16, TcpListener)>>,
}

impl Install {
    fn new(user: &'static str) -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
            user,
            ports: Default::default(),
        }
    }

    /// The port database `db` binds, claimed on first use.
    fn port(&self, db: &'static str) -> u16 {
        let mut ports = self.ports.borrow_mut();
        if let Some((_, p, _)) = ports.iter().find(|(d, _, _)| *d == db) {
            return *p;
        }
        let (port, claim) = claim_port();
        ports.push((db, port, claim));
        port
    }

    fn cmd(&self, db: &'static str, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env_remove("DATUM_DB")
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .env("USER", self.user)
            .env("DATUM_SYNC_NO_DAEMON", "1")
            // A member not running is waited for this long, not a minute.
            .env("DATUM_SYNC_TIMEOUT_S", "5")
            .env("DATUM_SYNC_BIND", format!("127.0.0.1:{}", self.port(db)))
            .args(["--db", db])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }

    fn ok(&self, db: &'static str, args: &[&str]) -> String {
        let out = self.cmd(db, args).output().unwrap();
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.status.success(),
            "`datum --db {db} {}`: {said}",
            args.join(" ")
        );
        said
    }

    fn json(&self, db: &'static str, args: &[&str]) -> Value {
        let out = self.cmd(db, args).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn device(&self) -> String {
        let status = self.json("home", &["sync", "status", "--json"]);
        status["this"]["device"].as_str().unwrap().to_string()
    }

    /// `sync run` on `db`, up, its stderr read on a thread.
    fn run(&self, db: &'static str) -> Running {
        let mut child = self.cmd(db, &["sync", "run"]).spawn().unwrap();
        let err = BufReader::new(child.stderr.take().unwrap());
        let (tx, log) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            err.lines()
                .map_while(Result::ok)
                .try_for_each(|l| tx.send(l))
        });
        let running = Running { child, log };
        running.says("is up");
        running
    }

    /// A query on database `db`'s file, or none where there is no file.
    fn query(&self, db: &str, sql: &str) -> Option<String> {
        let path = self.tmp.path().join("home").join(format!("{db}.db"));
        if !path.exists() {
            return None;
        }
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.query_row(sql, [], |r| r.get::<_, String>(0)).ok()
    }

    fn has(&self, db: &str, field: &str) -> bool {
        let n = self.query(
            db,
            &format!("SELECT COUNT(*) || '' FROM entries WHERE field = '{field}'"),
        );
        n.is_some_and(|n| n != "0")
    }
}

struct Running {
    child: Child,
    log: Receiver<String>,
}

impl Running {
    #[track_caller]
    fn says(&self, what: &str) -> String {
        loop {
            let Ok(line) = self.log.recv_timeout(Duration::from_secs(20)) else {
                panic!("the run saying {what:?} within 20s");
            };
            if line.contains(what) {
                return line;
            }
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn eventually(what: &str, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "{what} within 20s"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `g`'s `home` chain, every other install in it by an invite of its own
/// and synced since, and `g`'s run serving it.
fn home_chain(g: &Install, others: &[&Install]) -> Running {
    g.ok("home", &["occur", "start"]);
    for o in others {
        let mut inviting = g.cmd("home", &["sync", "chain", "invite"]).spawn().unwrap();
        let mut out = BufReader::new(inviting.stdout.take().unwrap()).lines();
        let line = out
            .find_map(|l| l.ok().filter(|l| l.contains("datum sync chain connect ")))
            .expect("the invite's connect line");
        let ticket = line.trim().rsplit(' ').next().unwrap().trim_matches('\'');
        o.ok("home", &["sync", "chain", "connect", ticket]);
        assert!(inviting.wait().unwrap().success());
    }
    // Each says what it is, its device with it, on the sync after its join.
    let run = g.run("home");
    for o in others {
        o.ok("home", &["sync"]);
    }
    run
}

/// The cell `device` has in the column of chain `chain`, as `g`'s grid says.
fn cell(g: &Install, chain: &str, device: &str) -> Option<String> {
    let grid = g.json("work", &["sync", "grid", "--json"]);
    let row = grid["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["device"] == device)?
        .clone();
    row["cells"][chain]["state"].as_str().map(String::from)
}

/// A device online joins as the grant reaches it, one offline as it next
/// syncs, a reader as a reader, and a third device in the carrier chain
/// holds the grants and can read neither.
#[test]
fn a_grant_joins_a_device_that_shares_a_chain_online_or_later() {
    let g = Install::new("gina");
    let (t, o, z) = (
        Install::new("tom"),
        Install::new("olga"),
        Install::new("zed"),
    );
    let _home = home_chain(&g, &[&t, &o, &z]);
    g.ok("work", &["occur", "coffee"]);
    g.ok("work", &["sync", "new-invite"]);
    let work = g.json("work", &["sync", "status", "--json"]);
    let chain = work["chain"].as_str().unwrap().to_string();
    let _work = g.run("work");

    // Online: its run takes the grant up as it lands, and joins.
    let tom = t.run("home");
    let said = g.ok("work", &["sync", "grant", &t.device()[..12], "member"]);
    assert!(said.contains("joins as «work»"), "{said}");
    tom.says("joined «work»");
    eventually("tom's work holding gina's coffee", || {
        t.has("work", "coffee")
    });
    // Its device reaches gina with its describe, on its next sync.
    t.ok("work", &["sync"]);
    assert_eq!(cell(&g, &chain, &t.device()).as_deref(), Some("member"));

    // Offline when granted: the grant waits in the chain they share.
    let said = g.ok(
        "work",
        &["sync", "grant", &o.device(), "reader", "--as", "gina"],
    );
    assert!(said.contains("joins as «gina»"), "{said}");
    assert!(o.query("gina", "SELECT 'x'").is_none());
    let synced = o.ok("home", &["sync"]);
    assert!(synced.contains("joined «gina»"), "{synced}");
    assert!(o.has("gina", "coffee"));
    o.ok("gina", &["sync"]);
    // Joined by it, it is done: the next sync joins nothing.
    let again = o.ok("home", &["sync"]);
    assert!(!again.contains("joining"), "{again}");
    eventually("olga a reader of work", || {
        cell(&g, &chain, &o.device()).as_deref() == Some("reader")
    });

    // A third member holds both grants, sealed: no chain id, no ticket, in
    // the record or the line that wrote it.
    z.ok("home", &["sync"]);
    let records = z
        .query(
            "home",
            "SELECT group_concat(body, char(10)) FROM ops WHERE hid = '~chain'",
        )
        .unwrap();
    let grants: Vec<&str> = records
        .lines()
        .filter(|l| l.contains("\"grant\""))
        .collect();
    assert_eq!(grants.len(), 2, "{records}");
    for g in grants {
        assert!(!g.contains(&chain) && !g.contains("sync/join"), "{g}");
    }
    assert!(z.query("work", "SELECT 'x'").is_none() && z.query("gina", "SELECT 'x'").is_none());

    // `expel` takes a device too, resolved to its endpoint in this chain.
    g.ok("work", &["sync", "chain", "expel", &t.device()[..12]]);
    assert_eq!(cell(&g, &chain, &t.device()).as_deref(), Some("expelled"));

    // Expelled, it is never told, and still holds the chain: granted again,
    // it is admitted again, its copy the same.
    g.ok("work", &["sync", "grant", &t.device(), "member"]);
    tom.says("joined «work»");
    eventually("tom a member of work again", || {
        cell(&g, &chain, &t.device()).as_deref() == Some("member")
    });
    assert!(t.query("work-2", "SELECT 'x'").is_none());
}

/// Names are not identities: a device holding a database of the grant's
/// name, with data of its own, keeps it and joins under another name. An
/// expired grant joins nothing, and the grant again does.
#[test]
fn a_grant_never_takes_a_database_of_its_name_and_an_expired_one_is_granted_again() {
    let g = Install::new("gina");
    let t = Install::new("tom");
    let _home = home_chain(&g, &[&t]);
    g.ok("work", &["occur", "coffee"]);
    g.ok("work", &["sync", "new-invite"]);
    let _work = g.run("work");
    t.ok("work", &["occur", "tea"]);

    g.ok(
        "work",
        &["sync", "grant", &t.device(), "member", "--expires", "1s"],
    );
    std::thread::sleep(Duration::from_millis(1500));
    let synced = t.ok("home", &["sync"]);
    assert!(
        synced.contains("expired before this device synced"),
        "{synced}"
    );
    assert!(t.query("work-2", "SELECT 'x'").is_none());

    g.ok("work", &["sync", "grant", &t.device(), "member"]);
    let synced = t.ok("home", &["sync"]);
    assert!(synced.contains("joined «work-2»"), "{synced}");
    assert!(t.has("work-2", "coffee") && !t.has("work-2", "tea"));
    assert!(t.has("work", "tea") && !t.has("work", "coffee"));
    let own = t.json("work", &["sync", "status", "--json"]);
    assert!(own["chain"].is_null(), "{own}");
}
