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

/// The key file `name` in `i`'s directory, its path.
fn key(i: &Install, name: &str) -> String {
    i.tmp.path().join(name).to_string_lossy().into_owned()
}

/// `db`'s chain records of type `kind`, as `i` holds them.
fn records(i: &Install, db: &str, kind: &str) -> usize {
    let n = i.query(
        db,
        &format!(
            "SELECT COUNT(*) || '' FROM ops WHERE hid = '~chain' AND body LIKE '%\"type\":\"{kind}\"%'"
        ),
    );
    n.map_or(0, |n| n.parse().unwrap())
}

/// `grant --like` into a database in no chain (da-n0uj1.5): it starts one
/// whose admins are the source's, signed by one of them, and grants each
/// device the role it has there.
#[test]
fn a_new_database_copies_its_sources_admins_and_each_devices_role() {
    let g = Install::new("gina");
    let (t, o) = (Install::new("tom"), Install::new("olga"));
    let _home = home_chain(&g, &[&t, &o]);
    let admin = key(&g, "home.key");
    g.ok("home", &["sync", "admin", "add", &admin]);
    g.ok(
        "home",
        &[
            "sync",
            "chain",
            "role",
            &o.device()[..12],
            "reader",
            "--admin-key",
            &admin,
        ],
    );
    g.ok("work", &["occur", "coffee"]);

    let said = g.ok(
        "work",
        &["sync", "grant", "--like", "home", "--admin-key", &admin],
    );
    assert!(said.contains("started sync chain"), "{said}");
    // The same keys under the same labels, each added when its chain began.
    let keys = |db| {
        let admins = g.json(db, &["sync", "admin", "list", "--json"])["admins"].clone();
        let pair = |a: &Value| (a["key"].to_string(), a["label"].to_string());
        admins
            .as_array()
            .unwrap()
            .iter()
            .map(pair)
            .collect::<Vec<_>>()
    };
    assert_eq!(keys("work"), keys("home"));
    let chain = g.json("work", &["sync", "status", "--json"])["chain"]
        .as_str()
        .unwrap()
        .to_string();

    let _work = g.run("work");
    for i in [&t, &o] {
        let synced = i.ok("home", &["sync"]);
        assert!(synced.contains("joined «work»"), "{synced}");
        assert!(i.has("work", "coffee"));
        i.ok("work", &["sync"]);
    }
    eventually("tom a member and olga a reader of work", || {
        cell(&g, &chain, &t.device()).as_deref() == Some("member")
            && cell(&g, &chain, &o.device()).as_deref() == Some("reader")
    });
}

/// `grant --like` into a database in a chain of its own: only its admin
/// copies, `--devices` takes a subset, a device it holds is not granted
/// again, and the chain is never a second one.
#[test]
fn an_existing_chain_takes_the_devices_it_lacks_from_its_admin_alone() {
    let g = Install::new("gina");
    let (t, o) = (Install::new("tom"), Install::new("olga"));
    let _home = home_chain(&g, &[&t, &o]);
    let (home_key, work_key) = (key(&g, "home.key"), key(&g, "work.key"));
    let like = |i: &Install, db, more: &[&str]| {
        let args = [&["sync", "grant", "--like", "home"][..], more].concat();
        i.cmd(db, &args).output().unwrap()
    };
    let refused = |out: std::process::Output, why: &str| {
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && err.contains(why), "{err}");
    };
    g.ok("home", &["sync", "admin", "add", &home_key]);
    // A reader of home's coffee alone: its fields are no other database's.
    let olga = &o.device()[..12];
    g.ok(
        "home",
        &[
            "sync",
            "chain",
            "role",
            olga,
            "reader",
            "--fields",
            "coffee",
            "--admin-key",
            &home_key,
        ],
    );
    o.ok("home", &["sync"]);
    // It carries no grant in home: refused before its database is in a chain.
    refused(like(&o, "notes", &[]), "carries no grant");
    assert_eq!(records(&o, "notes", "genesis"), 0);

    g.ok("work", &["occur", "coffee"]);
    g.ok("work", &["sync", "new-invite"]);
    g.ok("work", &["sync", "admin", "add", &work_key]);
    let chain = g.json("work", &["sync", "status", "--json"])["chain"]
        .as_str()
        .unwrap()
        .to_string();
    let held = records(&g, "work", "invite");

    // Home's admin is not work's: refused, nothing written.
    refused(
        like(&g, "work", &["--admin-key", &home_key]),
        "no admin key of this database",
    );
    assert_eq!(records(&g, "work", "invite"), held);
    assert_eq!(records(&g, "work", "grant"), 0);
    assert_eq!(records(&g, "home", "grant"), 0);
    // This device is not one to grant.
    let mine = g.device();
    refused(
        like(&g, "work", &["--devices", &mine, "--admin-key", &work_key]),
        "no device of home's sync chain:",
    );

    // Its admin, one device of two.
    let tom = &t.device()[..12];
    let said = g.ok(
        "work",
        &[
            "sync",
            "grant",
            "--like",
            "home",
            "--devices",
            tom,
            "--admin-key",
            &work_key,
        ],
    );
    assert!(!said.contains("started"), "{said}");
    assert_eq!(records(&g, "home", "grant"), 1);
    let _work = g.run("work");
    let synced = t.ok("home", &["sync"]);
    assert!(synced.contains("joined «work»"), "{synced}");
    t.ok("work", &["sync"]);
    let synced = o.ok("home", &["sync"]);
    assert!(!synced.contains("joining"), "{synced}");
    assert!(o.query("work", "SELECT 'x'").is_none());

    // Every device: the one it holds is said and left, and so is the
    // filtered reader, granted nothing.
    eventually("tom a member of work", || {
        cell(&g, &chain, &t.device()).as_deref() == Some("member")
    });
    let all = ["sync", "grant", "--like", "home", "--admin-key", &work_key];
    let said = g.ok("work", &all);
    assert!(said.contains("member here already"), "{said}");
    assert!(said.contains("a reader of some of home's fields"), "{said}");
    assert_eq!(records(&g, "home", "grant"), 1);

    // Unfiltered, it is copied as the reader it is.
    g.ok(
        "home",
        &[
            "sync",
            "chain",
            "role",
            olga,
            "reader",
            "--admin-key",
            &home_key,
        ],
    );
    g.ok("work", &all);
    assert_eq!(records(&g, "home", "grant"), 2);
    let synced = o.ok("home", &["sync"]);
    assert!(synced.contains("joined «work»"), "{synced}");
    o.ok("work", &["sync"]);
    eventually("olga a reader of work", || {
        cell(&g, &chain, &o.device()).as_deref() == Some("reader")
    });
    assert_eq!(records(&g, "work", "genesis"), 1);
    let theirs = o.json("work", &["sync", "status", "--json"]);
    assert_eq!(theirs["chain"].as_str(), Some(chain.as_str()));
}

/// The challenge a passkey signs for a batch approve link (da-7bjai): the
/// digest of the list of each record's digest, as the page computes it.
fn batch_challenge(link: &str) -> (Vec<String>, Vec<u8>) {
    use data_encoding::{BASE64URL_NOPAD, HEXLOWER};
    use sha2::{Digest, Sha256};
    let fragment = link.trim().rsplit('#').next().unwrap();
    let texts: Vec<String> =
        serde_json::from_slice(&BASE64URL_NOPAD.decode(fragment.as_bytes()).unwrap()).unwrap();
    let digests: Vec<String> = texts
        .iter()
        .map(|t| HEXLOWER.encode(&Sha256::digest(t.as_bytes())))
        .collect();
    let list = serde_json::to_string(&digests).unwrap();
    (texts, Sha256::digest(list.as_bytes()).to_vec())
}

fn batch_link(texts: &[String]) -> String {
    let fragment =
        data_encoding::BASE64URL_NOPAD.encode(serde_json::to_string(texts).unwrap().as_bytes());
    format!("https://datum.day/sync/approve#{fragment}")
}

/// `grant --like` from a passkey-locked chain (da-7bjai): the genesis and an
/// invite per device are approved at once, one link and one code, and all
/// three devices join. The code over that batch approves no other: a batch
/// with one invite altered is refused, nothing written.
#[test]
fn a_passkey_approves_a_grant_like_of_three_devices_once() {
    use std::io::{Read, Write};
    let g = Install::new("gina");
    let (t, o, z) = (
        Install::new("tom"),
        Install::new("olga"),
        Install::new("zed"),
    );
    let _home = home_chain(&g, &[&t, &o, &z]);
    g.ok(
        "home",
        &["sync", "admin", "add", &crate::sync_chain::passkey_json()],
    );
    g.ok("work", &["occur", "coffee"]);

    let mut copying = g
        .cmd("work", &["sync", "grant", "--like", "home"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut err = BufReader::new(copying.stderr.take().unwrap());
    let (mut asked, mut line) = (String::new(), String::new());
    while !line.contains("/sync/approve#") {
        asked.push_str(&line);
        line.clear();
        assert!(
            err.read_line(&mut line).unwrap() > 0,
            "no approve link: {asked}"
        );
    }
    assert!(asked.contains("these 4, at once"), "{asked}");
    let (texts, challenge) = batch_challenge(&line);
    let records: Vec<Value> = texts
        .iter()
        .map(|t| serde_json::from_str(t).unwrap())
        .collect();
    let kinds: Vec<&str> = records
        .iter()
        .map(|r| r["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["genesis", "invite", "invite", "invite"]);
    let code = crate::sync_chain::approval_code(&challenge);

    // One invite altered: the code approves the batch it was made for alone.
    let mut altered = records.clone();
    altered[3]["caps"] = serde_json::json!({"role": "reader"});
    let texts2: Vec<String> = altered.iter().map(|r| r.to_string()).collect();
    let mut approving = g
        .cmd("notes", &["sync", "approve", &batch_link(&texts2)])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(approving.stdin.take().unwrap(), "{code}").unwrap();
    let out = approving.wait_with_output().unwrap();
    let why = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && why.contains("not signed by an admin"),
        "{why}"
    );
    assert_eq!(records_of(&g, "notes"), 0);

    writeln!(copying.stdin.take().unwrap(), "{code}").unwrap();
    let mut rest = String::new();
    err.read_to_string(&mut rest).unwrap();
    let out = copying.wait_with_output().unwrap();
    assert!(out.status.success(), "{rest}");
    assert!(
        !rest.contains("/sync/approve#"),
        "a second approval asked: {rest}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("started sync chain"));

    let _work = g.run("work");
    for i in [&t, &o, &z] {
        let synced = i.ok("home", &["sync"]);
        assert!(synced.contains("joined «work»"), "{synced}");
        assert!(i.has("work", "coffee"));
    }
}

/// Every chain record `i`'s `db` holds.
fn records_of(i: &Install, db: &str) -> usize {
    let n = i.query(db, "SELECT COUNT(*) || '' FROM ops WHERE hid = '~chain'");
    n.map_or(0, |n| n.parse().unwrap())
}

/// datum.day's CLI makes no ticket (da-n0uj1.7): a grant seals the one its
/// sync worker made for an invite written first, refused unless it is that
/// invite's, of that role; a `--like` `--defer`red starts the chain, writes an
/// invite per device, and leaves each grant to the page, naming them.
#[test]
fn a_grant_takes_a_ticket_made_elsewhere_and_a_deferred_grant_like_leaves_its_grants() {
    use data_encoding::BASE32_NOPAD;
    let g = Install::new("gina");
    let t = Install::new("tom");
    let _home = home_chain(&g, &[&t]);
    g.ok("work", &["occur", "coffee"]);
    let invite = g.json("work", &["sync", "new-invite"]);
    let (secret, id) = (
        invite["secret"].as_str().unwrap(),
        invite["id"].as_str().unwrap(),
    );
    // Where work is dialed, as its own invite's ticket says, with another secret.
    let mut inviting = g.cmd("work", &["sync", "chain", "invite"]).spawn().unwrap();
    let line = BufReader::new(inviting.stdout.take().unwrap())
        .lines()
        .find_map(|l| l.ok().filter(|l| l.contains("sync/join#")))
        .unwrap();
    let _ = inviting.kill();
    let _ = inviting.wait();
    let theirs = line
        .rsplit('#')
        .next()
        .unwrap()
        .trim_matches(|c| c == '\'' || c == ' ');
    let addrs = serde_json::from_slice::<Value>(
        &BASE32_NOPAD
            .decode(theirs.to_uppercase().as_bytes())
            .unwrap(),
    )
    .unwrap()["addrs"]
        .clone();
    let ticket = |secret: &str| {
        BASE32_NOPAD.encode(
            &serde_json::to_vec(&serde_json::json!({"addrs": addrs, "secret": secret})).unwrap(),
        )
    };
    let tom = &t.device()[..12];
    let grant = |role, ticket: &str| {
        g.cmd(
            "work",
            &[
                "sync", "grant", tom, role, "--ticket", ticket, "--invite", id,
            ],
        )
        .output()
        .unwrap()
    };
    for (out, why) in [
        (
            grant("member", &ticket("not-its-secret")),
            "no ticket of invite",
        ),
        (
            grant("reader", &ticket(secret)),
            "a member's, not a reader's",
        ),
        (grant("member", "not a ticket"), "--ticket is not a ticket"),
    ] {
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && err.contains(why), "{err}");
    }
    assert_eq!(records(&g, "home", "grant"), 0);

    let _work = g.run("work");
    let out = grant("member", &ticket(secret));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let synced = t.ok("home", &["sync"]);
    assert!(synced.contains("joined «work»"), "{synced}");
    assert!(t.has("work", "coffee"));

    let out = g
        .cmd("notes", &["sync", "grant", "--like", "home", "--defer"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("started sync chain"));
    let left: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(left["grants"].as_array().unwrap().len(), 1, "{left}");
    assert_eq!(left["grants"][0]["device"], t.device().as_str());
    assert_eq!(left["grants"][0]["role"], "member");
    // Its invite written with the genesis, approved with it at once (da-7bjai).
    let made = &left["grants"][0]["invite"];
    assert!(
        made["secret"].is_string() && made["expires"].is_string(),
        "{left}"
    );
    assert_eq!(records(&g, "notes", "genesis"), 1);
    assert_eq!(records(&g, "notes", "invite"), 1);
    let held = g.query(
        "notes",
        "SELECT group_concat(body) FROM ops WHERE hid = '~chain'",
    );
    assert!(held.unwrap().contains(made["id"].as_str().unwrap()));
    assert_eq!(records(&g, "home", "grant"), 1);
}

/// `sync chain wipe` (da-n0uj1.6): a device online deletes its copy as the
/// order reaches it, carried by the dial of a member's run; one offline is
/// expelled at once, says it is asked, and deletes its copy as it next
/// dials a member. Each then shows as wiped; this copy is never wiped.
#[test]
fn a_wiped_device_deletes_its_copy_online_at_once_and_offline_once_back() {
    let g = Install::new("gina");
    let (t, o) = (Install::new("tom"), Install::new("olga"));
    let home = home_chain(&g, &[&t, &o]);
    let admin = key(&g, "home.key");
    g.ok("home", &["sync", "admin", "add", &admin]);
    t.ok("home", &["occur", "tea"]);
    o.ok("home", &["occur", "mate"]);
    o.ok("home", &["sync"]);
    let tom = t.run("home");
    let chain = g.json("home", &["sync", "status", "--json"])["chain"]
        .as_str()
        .unwrap()
        .to_string();
    let cell = |i: &Install| {
        let grid = g.json("home", &["sync", "grid", "--json"]);
        let rows = grid["devices"].as_array().unwrap().clone();
        let row = rows
            .into_iter()
            .find(|r| r["device"] == i.device().as_str());
        row.and_then(|r| r["cells"][&chain]["state"].as_str().map(String::from))
    };
    let wipe = |who: &str| {
        let args = ["sync", "chain", "wipe", who, "--yes", "--admin-key", &admin];
        g.cmd("home", &args).output().unwrap()
    };

    let own = wipe(&g.device()[..12]);
    let said = String::from_utf8_lossy(&own.stderr);
    assert!(
        !own.status.success() && said.contains("is this copy"),
        "{said}"
    );

    // Online: gina's run carries the order as it dials tom, at once.
    let said = String::from_utf8_lossy(&wipe(&t.device()[..12]).stdout).into_owned();
    assert!(said.contains("cannot be made to delete anything"), "{said}");
    assert!(
        said.contains("since it last synced goes with its copy"),
        "{said}"
    );
    // Its run says so whichever way the order came: carried by gina's
    // dial, or with the refusal of its own if the two dialed at once.
    tom.says("deleted its copy of this database");
    // Its run stops: the copy it served is another now, in no chain.
    let tom = std::cell::RefCell::new(tom);
    eventually("tom's run stopping", || {
        tom.borrow_mut().child.try_wait().unwrap().is_some()
    });
    assert_eq!(
        t.query("home", "SELECT COUNT(*) || '' FROM entries")
            .as_deref(),
        Some("0")
    );
    assert_eq!(
        t.query("home", "SELECT COUNT(*) || '' FROM ops").as_deref(),
        Some("0")
    );
    eventually("tom wiped", || cell(&t).as_deref() == Some("wiped"));

    // Offline: expelled at once, its copy kept until it comes back. What
    // it wrote since it last synced, before the order too, goes with it:
    // it obeys at the handshake, before any op moves.
    o.ok("home", &["occur", "yerba"]);
    wipe(&o.device()[..12]);
    assert_eq!(cell(&o).as_deref(), Some("wiping"));
    assert!(o.has("home", "mate") && o.has("home", "yerba"));
    let back = o.cmd("home", &["sync"]).output().unwrap();
    let said = String::from_utf8_lossy(&back.stdout);
    assert!(said.contains("deleted its copy of this database"), "{said}");
    assert!(!o.has("home", "mate") && !o.has("home", "start"));
    let own = o.json("home", &["sync", "status", "--json"]);
    assert!(own["chain"].is_null(), "{own}");
    eventually("olga wiped", || cell(&o).as_deref() == Some("wiped"));
    // What each synced stays on gina's copy; what olga had not is nowhere.
    assert!(g.has("home", "tea") && g.has("home", "mate"));
    assert!(!g.has("home", "yerba") && !o.has("home", "yerba"));
    drop(home);
}
