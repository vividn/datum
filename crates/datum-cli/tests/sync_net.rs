//! Easy Sync over iroh (da-rugg8): two copies of a database, each with its own
//! device, join one sync chain with a one-time invite (da-6177s) and sync over
//! QUIC — the real binary,
//! hermetic: `DATUM_SYNC_BIND` gives each copy a fixed localhost address, no
//! relay and no discovery.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use std::time::{Duration, Instant};

use datum_core::migrations::CURRENT_SCHEMA_VERSION;
use tempfile::TempDir;

mod admission;
mod catch_up;
mod closing;
mod diff;
mod first_sync;
mod giving_up;
mod identity;
mod invite;
mod purge;
mod readers;
mod run;
mod schema;

/// A copy's UDP port, and the claim that holds it for as long as the copy
/// lives (da-nuxm6). A port read off a probe bound to `:0` is free the moment
/// the probe drops, and any bind on the box to `:0` — another test's probe, a
/// DNS lookup — could be given it before the copy's `datum` binds it, which
/// then dies with «Address already in use». So the port is below Linux's
/// ephemeral range (32768–), where no `:0` bind lands, and a TCP listener on
/// the same number keeps it from the other copies: `datum` binds UDP.
pub fn claim_port() -> (u16, TcpListener) {
    loop {
        let port = 20_000 + (RandomState::new().hash_one(0) % 12_768) as u16;
        if let Ok(claim) = TcpListener::bind(("127.0.0.1", port)) {
            if UdpSocket::bind(("127.0.0.1", port)).is_ok() {
                return (port, claim);
            }
        }
    }
}

/// One copy: its own file, `DATUM_HOME` (so device and endpoint key) and port.
struct Copy {
    tmp: TempDir,
    port: u16,
    _claim: TcpListener,
    /// Set on every `datum` this copy runs.
    env: Vec<(&'static str, &'static str)>,
    /// Its database's file name.
    file: &'static str,
}

impl Copy {
    fn new() -> Self {
        let (port, _claim) = claim_port();
        // A user of its own, so its default label is (da-ey3hz): the
        // tests read a label in one copy and look for it in another.
        let user: &'static str = Box::leak(format!("u{port}").into_boxed_str());
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
            port,
            _claim,
            env: vec![("USER", user)],
            file: "data.db",
        }
    }

    /// A copy run by `user`, of a database in a file named `file`.
    fn of(user: &'static str, file: &'static str) -> Self {
        let mut copy = Self::new();
        copy.env = vec![("USER", user)];
        copy.file = file;
        copy
    }

    fn db(&self) -> PathBuf {
        self.tmp.path().join(self.file)
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", self.db())
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .env("DATUM_SYNC_BIND", format!("127.0.0.1:{}", self.port))
            // Never a service in the real home (da-vd2sn).
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .envs(self.env.iter().copied())
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }

    fn run_in(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = self.cmd(args).spawn().expect("spawn datum");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in(args, "")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`datum {}` failed: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// A command that must fail; its stdout and stderr together.
    fn fails(&self, args: &[&str], stdin: &str) -> String {
        let out = self.run_in(args, stdin);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !out.status.success(),
            "`datum {}` succeeded: {text}",
            args.join(" ")
        );
        text
    }

    /// A long-running `datum` (invite, run), killed when dropped.
    fn serve(&self, args: &[&str]) -> Serving {
        Serving::of(&mut self.cmd(args))
    }

    /// `sync chain invite` serving, and its ticket.
    fn invite(&self, args: &[&str]) -> (Serving, String) {
        let mut s = self.serve(&[&["sync", "chain", "invite"], args].concat());
        let line = s.lines_until("datum sync chain join ");
        let ticket = line.trim().rsplit(' ').next().unwrap();
        let ticket = ticket.trim_matches('\'').to_string();
        (s, ticket)
    }

    fn hash(&self) -> String {
        self.ok(&["sync", "pipe", "verify"]).trim().to_string()
    }

    fn id(&self) -> String {
        self.query("SELECT endpoint_id FROM sync_local")
    }

    fn query(&self, sql: &str) -> String {
        let db = rusqlite::Connection::open(self.db()).unwrap();
        // rusqlite's own 5 s ran out on a loaded runner while a copy's run
        // was killed and restarted (CI 6621: a plain SQLITE_BUSY, extended
        // code 5). A held write lock does not hold a WAL reader, so what
        // did is open (da-hd9sx).
        db.busy_timeout(Duration::from_secs(30)).unwrap();
        db.query_row(sql, [], |r| {
            let v: rusqlite::types::Value = r.get(0)?;
            Ok(match v {
                rusqlite::types::Value::Text(s) => s,
                rusqlite::types::Value::Integer(n) => n.to_string(),
                other => format!("{other:?}"),
            })
        })
        .unwrap()
    }

    fn has(&self, field: &str) -> bool {
        self.query(&format!(
            "SELECT COUNT(*) FROM entries WHERE field = '{field}'"
        )) != "0"
    }
}

struct Serving {
    child: Child,
    out: BufReader<ChildStdout>,
}

impl Serving {
    fn of(cmd: &mut Command) -> Self {
        let mut child = cmd.spawn().expect("spawn datum");
        let out = BufReader::new(child.stdout.take().unwrap());
        Serving { child, out }
    }

    /// Read stdout up to the line containing `needle`; that line.
    fn lines_until(&mut self, needle: &str) -> String {
        let mut line = String::new();
        loop {
            line.clear();
            if self.out.read_line(&mut line).unwrap() == 0 {
                // Its stderr is the reason it exited (da-nuxm6).
                let mut err = String::new();
                if let Some(mut e) = self.child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                let status = self.child.wait().unwrap();
                panic!("stdout ended before {needle:?} ({status}): {err}");
            }
            if line.contains(needle) {
                return line;
            }
        }
    }

    /// Wait for it to exit: whether it succeeded, and the rest of stdout.
    fn finish(&mut self) -> (bool, String) {
        let mut rest = String::new();
        self.out.read_to_string(&mut rest).unwrap();
        (self.child.wait().unwrap().success(), rest)
    }
}

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `a` and `b`, in the open chain `a`'s invite started.
fn paired() -> (Copy, Copy) {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&[]);
    b.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    (a, b)
}

pub fn eventually(what: &str, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{what} within 10s"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// What sync holds in `copy`'s file: its ops, entries and peer rows.
fn held(copy: &Copy) -> String {
    copy.query(
        "SELECT json_array(
           (SELECT json_group_array(body) FROM ops),
           (SELECT json_group_array(hid || modify_time) FROM entries),
           (SELECT json_group_array(json_array(last_seen, last_vector, last_verify_at,
              last_verify_match, last_error, last_error_at)) FROM sync_peers))",
    )
}

/// Each line `stream` prints, as it prints it.
fn lines(stream: impl Read + Send + 'static) -> std::sync::mpsc::Receiver<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        BufReader::new(stream)
            .lines()
            .map_while(Result::ok)
            .try_for_each(|l| tx.send(l))
    });
    rx
}

/// A child killed when dropped, its streams the test's to take.
struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn concurrent_edits_across_the_link_file_a_conflict_on_both_sides() {
    let (a, b) = paired();
    a.ok(&["occur", "coffee", "cups=2"]);
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["sync"]);
    let hid = b.query("SELECT hid FROM entries WHERE field = 'coffee'");
    drop(_a_runs);
    a.ok(&["update", &hid, "cups=4"]);
    b.ok(&["update", &hid, "cups=5"]);

    let _a_runs = a.serve(&["sync", "run"]);
    let now = b.ok(&["sync"]);
    assert!(now.contains("conflicts 1, verify ok"), "{now}");
    assert_eq!(a.hash(), b.hash());
    for copy in [&a, &b] {
        let conflicts = copy.ok(&["conflicts"]);
        assert!(conflicts.contains("data.cups"), "{conflicts}");
    }
    // Each side is named by its device's label in the chain, and renamed
    // with it, on both copies (da-05rkv).
    drop(_a_runs);
    a.ok(&["sync", "chain", "rename", "alpha"]);
    b.ok(&["sync", "chain", "rename", "beta"]);
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["sync"]);
    let column = |c: &Copy| {
        c.query(&format!(
            "SELECT conflicts FROM entries WHERE hid = '{hid}'"
        ))
    };
    assert_eq!(column(&a), column(&b));
    let sides: serde_json::Value = serde_json::from_str(&column(&a)).unwrap();
    let mut devices: Vec<&str> = sides["data.cups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["device"].as_str().unwrap())
        .collect();
    devices.sort();
    assert_eq!(devices, ["alpha", "beta"]);
}

fn member_of_list(list: &str, id: &str) -> serde_json::Value {
    let list: serde_json::Value = serde_json::from_str(list).unwrap();
    list["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["endpoint"] == id)
        .unwrap()
        .clone()
}

fn member_label(of: &Copy, id: &str) -> String {
    let list = of.ok(&["sync", "chain", "list", "--json"]);
    member_of_list(&list, id)["label"]
        .as_str()
        .unwrap()
        .to_string()
}

/// A first sync whose apply outlasts the progress timeout (da-039jx): b applies
/// a's 300-odd ops at 40ms each, 12s, on a 6s timeout, sending a `BUSY`
/// between chunks paced to a sixteenth of it unless `keepalive` is off. The
/// timeout is the most one chunk may hold b's thread, and on a loaded box one
/// was held past 3s, about where a WAL checkpoint falls in these ops (da-qwa1c).
/// The join's output, and whether the invite exited 0 and what it printed.
fn slow_first_sync(keepalive: bool) -> (Copy, Copy, Output, (bool, String)) {
    let (mut a, mut b) = (Copy::new(), Copy::new());
    for i in 0..300 {
        a.ok(&["occur", "step", &format!("n={i}")]);
    }
    for c in [&mut a, &mut b] {
        c.env = vec![
            ("DATUM_SYNC_TIMEOUT_S", "6"),
            ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "40"),
            (
                "DATUM_SYNC_TEST_KEEPALIVE",
                if keepalive { "1" } else { "0" },
            ),
        ];
    }
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.run(&["sync", "chain", "join", &ticket]);
    let served = inviting.finish();
    (a, b, joined, served)
}

/// A `datum` whose stdout and stderr go to `log` in the copy's directory,
/// killed when dropped.
struct Logged(Child);

impl Drop for Logged {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn logged(copy: &Copy, args: &[&str], log: &str) -> Logged {
    let at = copy.tmp.path().join(log);
    let f = std::fs::File::options()
        .create(true)
        .append(true)
        .open(at)
        .unwrap();
    let mut c = copy.cmd(args);
    c.stdout(f.try_clone().unwrap()).stderr(f);
    Logged(c.spawn().expect("spawn datum"))
}

fn log(copy: &Copy, log: &str) -> String {
    std::fs::read_to_string(copy.tmp.path().join(log)).unwrap_or_default()
}

/// `n` entries imported into `copy` at once, as vividn's re-import was: no op
/// covers them until a serve signs their baselines.
fn import(copy: &Copy, n: usize) {
    import_as(copy, "step", n)
}

/// [`import`]'s entries, of `field`.
fn import_as(copy: &Copy, field: &str, n: usize) {
    let docs: Vec<String> = (0..n)
        .map(|i| {
            let at = format!("2024-01-{:02}T{:02}:{:02}:{:02}.000Z", 1 + i / 86400, i / 3600 % 24, i / 60 % 60, i % 60);
            format!(
                r#"{{"_id":"{field}:{i}","data":{{"field":"{field}","occurTime":{{"utc":"{at}"}},"n":{i}}},"meta":{{"createTime":{{"utc":"{at}"}}}}}}"#
            )
        })
        .collect();
    let file = copy.tmp.path().join("import.json");
    let backup = format!(
        r#"{{"backupTime":"2024-02-01T00:00:00Z","docs":[{}]}}"#,
        docs.join(",")
    );
    std::fs::write(&file, backup).unwrap();
    copy.ok(&["db", "v2import", "--from", file.to_str().unwrap()]);
}

fn until(what: &str, limit: Duration, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < limit, "{what} within {limit:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `of`'s entry in `copy`'s `sync <what> --json` (`what` may be two words).
fn entry_of(copy: &Copy, what: &str, of: &Copy) -> serde_json::Value {
    let argv: Vec<&str> = ["sync"]
        .into_iter()
        .chain(what.split(' '))
        .chain(["--json"])
        .collect();
    let all: serde_json::Value = serde_json::from_str(&copy.ok(&argv)).unwrap();
    let id = of.id();
    let hit = all["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["endpoint"] == id.as_str());
    hit.cloned().unwrap_or_else(|| panic!("{id} not in {all}"))
}

/// `a`, `b` and `c` in one chain: `a` admitted `b`, logged an entry, then
/// admitted `c` by an invite with `caps`, and never ran again, so `b` has
/// not heard that `c` joined. `c` knows where `b` is, as `a` last saw it:
/// what a handshake passes on (da-fly3f). A dial that reaches no one fails
/// in 3s.
fn admitted_unheard(caps: &[&str]) -> (Copy, Copy, Copy) {
    let (a, mut b) = paired();
    a.ok(&["occur", "from-a"]);
    let mut c = Copy::new();
    let (mut inviting, ticket) = a.invite(caps);
    c.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    let at = format!(
        "SELECT addrs FROM sync_peers WHERE endpoint_id = '{}'",
        b.id()
    );
    let told = "INSERT OR REPLACE INTO sync_peers (endpoint_id, label, addrs, added_at)
                VALUES (?1, 'b', ?2, '2026-10-09T00:00:00Z')";
    let sql = rusqlite::Connection::open(c.db()).unwrap();
    sql.execute(told, [b.id(), a.query(&at)]).unwrap();
    b.env = vec![("DATUM_SYNC_DIAL_TIMEOUT_S", "3")];
    c.env = vec![("DATUM_SYNC_DIAL_TIMEOUT_S", "3")];
    (a, b, c)
}
