//! Easy Sync over iroh (da-rugg8): two copies of a database, each with its own
//! device, pair with a one-time invite and sync over QUIC — the real binary,
//! hermetic: `DATUM_SYNC_BIND` gives each copy a fixed localhost address, no
//! relay and no discovery.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// One copy: its own file, `DATUM_HOME` (so device and endpoint key) and port.
struct Copy {
    tmp: TempDir,
    port: u16,
    /// Set on every `datum` this copy runs.
    env: Vec<(&'static str, &'static str)>,
}

impl Copy {
    fn new() -> Self {
        let port = std::net::UdpSocket::bind("127.0.0.1:0")
            .and_then(|s| s.local_addr())
            .expect("a free port")
            .port();
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
            port,
            env: Vec::new(),
        }
    }

    fn db(&self) -> PathBuf {
        self.tmp.path().join("data.db")
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", self.db())
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("DATUM_COLOR", "0")
            .env("DATUM_SYNC_BIND", format!("127.0.0.1:{}", self.port))
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

    /// A long-running `datum` (add-client, run), killed when dropped.
    fn serve(&self, args: &[&str]) -> Serving {
        let mut child = self.cmd(args).spawn().expect("spawn datum");
        let out = BufReader::new(child.stdout.take().unwrap());
        Serving { child, out }
    }

    /// `add-client` running, and its ticket.
    fn invite(&self, args: &[&str]) -> (Serving, String) {
        let mut s = self.serve(&[&["sync", "add-client"], args].concat());
        let ticket = s
            .lines_until("datum sync connect ")
            .rsplit(' ')
            .next()
            .unwrap()
            .trim()
            .to_string();
        (s, ticket)
    }

    fn hash(&self) -> String {
        self.ok(&["sync", "verify"]).trim().to_string()
    }

    fn id(&self) -> String {
        self.query("SELECT endpoint_id FROM sync_local")
    }

    fn query(&self, sql: &str) -> String {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .query_row(sql, [], |r| {
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
    /// Read stdout up to the line containing `needle`; that line.
    fn lines_until(&mut self, needle: &str) -> String {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.out.read_line(&mut line).unwrap();
            assert!(n > 0, "stdout ended before {needle:?}");
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

/// `a` and `b`, paired through an invite from `a`.
fn paired() -> (Copy, Copy) {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&[]);
    b.ok(&["sync", "connect", &ticket]);
    assert!(inviting.finish().0);
    (a, b)
}

fn eventually(what: &str, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{what} within 10s"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn an_invite_pairs_two_copies_and_their_data_converges() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    b.ok(&["occur", "tea"]);
    let (mut inviting, ticket) = a.invite(&["--expires", "5m"]);
    let joined = b.ok(&["sync", "connect", &ticket]);
    assert!(joined.contains("added sync peer"), "{joined}");
    assert!(joined.contains("verify ok"), "{joined}");
    let (ok, served) = inviting.finish();
    assert!(ok, "add-client exits once redeemed: {served}");
    assert!(served.contains("verify ok"), "{served}");

    assert_eq!(a.hash(), b.hash());
    assert!(a.has("tea") && b.has("coffee"));
    assert!(a.ok(&["sync", "peers"]).contains(&b.id()[..10]));
    assert!(b.ok(&["sync", "peers"]).contains(&a.id()[..10]));
}

#[test]
fn an_invite_is_one_time_and_expires() {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&[]);
    // The ticket from stdin works as well as the argument.
    b.run_in(&["sync", "connect"], &ticket);
    assert!(inviting.finish().0);

    let (mut expiring, stale) = a.invite(&["--expires", "1s"]);
    let (ok, _) = expiring.finish();
    assert!(!ok, "add-client fails when its invite expires unredeemed");

    let _serving = a.serve(&["sync", "run"]);
    let c = Copy::new();
    let used = c.fails(&["sync", "connect", &ticket], "");
    assert!(used.contains("invite already used"), "{used}");
    // A peer presenting a used invite is refused too: one use is one use.
    let again = b.fails(&["sync", "connect", &ticket], "");
    assert!(again.contains("invite already used"), "{again}");
    let expired = c.fails(&["sync", "connect", &stale], "");
    assert!(expired.contains("invite expired"), "{expired}");
    assert_eq!(c.ok(&["sync", "peers"]).trim(), "no sync peers");
}

#[test]
fn a_stranger_and_a_copy_of_this_key_are_refused_at_the_handshake() {
    let (a, b) = paired();
    a.ok(&["occur", "secret"]);
    let serving = a.serve(&["sync", "run"]);

    // c was never paired: it knows a's address (b's peer row), not an invite.
    let c = Copy::new();
    c.ok(&["occur", "intrusion"]);
    let row: [String; 4] = rusqlite::Connection::open(b.db())
        .unwrap()
        .query_row(
            "SELECT endpoint_id, label, addrs, added_at FROM sync_peers",
            [],
            |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
        )
        .unwrap();
    rusqlite::Connection::open(c.db())
        .unwrap()
        .execute(
            "INSERT INTO sync_peers (endpoint_id, label, addrs, added_at, announced)
             VALUES (?1, ?2, ?3, ?4, 1)",
            row,
        )
        .unwrap();
    let refused = c.fails(&["sync", "now"], "");
    assert!(refused.contains("not a peer of this copy"), "{refused}");
    assert!(!c.has("secret") && !a.has("intrusion"));

    // a dialing its own endpoint (a copy of this db) is refused by name.
    drop(serving);
    let (_inviting, ticket) = a.invite(&[]);
    let mut own = a.cmd(&["sync", "connect", &ticket]);
    own.env("DATUM_SYNC_BIND", format!("127.0.0.1:{}", Copy::new().port));
    let out = own.output().unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("is a copy of this database"), "{text}");
}

#[test]
fn run_serves_peers_and_pushes_new_ops() {
    let (a, b) = paired();
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["occur", "walk"]);
    let now = b.ok(&["sync", "now"]);
    // The walk and the spec its first log declares.
    assert!(
        now.contains("sent 2, got 0, conflicts 0, verify ok"),
        "{now}"
    );
    assert!(a.has("walk"));

    let _b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "swim"]);
    eventually("a's write reaching b", || b.has("swim"));
    eventually("equal hashes", || a.hash() == b.hash());
}

#[test]
fn concurrent_edits_across_the_link_file_a_conflict_on_both_sides() {
    let (a, b) = paired();
    a.ok(&["occur", "coffee", "cups=2"]);
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["sync", "now"]);
    let hid = b.query("SELECT hid FROM entries WHERE field = 'coffee'");
    drop(_a_runs);
    a.ok(&["update", &hid, "cups=4"]);
    b.ok(&["update", &hid, "cups=5"]);

    let _a_runs = a.serve(&["sync", "run"]);
    let now = b.ok(&["sync", "now"]);
    assert!(now.contains("conflicts 1, verify ok"), "{now}");
    assert_eq!(a.hash(), b.hash());
    for copy in [&a, &b] {
        let conflicts = copy.ok(&["conflicts"]);
        assert!(conflicts.contains("sync_conflict"), "{conflicts}");
    }
}

#[test]
fn a_new_peer_is_announced_once() {
    let (a, b) = paired();
    for copy in [&a, &b] {
        let first = String::from_utf8(copy.run(&["tail"]).stderr).unwrap();
        assert!(first.contains("⚠ new sync peer"), "{first}");
        assert!(first.contains("datum sync peers to review"), "{first}");
        let second = String::from_utf8(copy.run(&["tail"]).stderr).unwrap();
        assert!(!second.contains("new sync peer"), "{second}");
    }
}

#[test]
fn forget_takes_a_label_or_an_id_prefix_and_refuses_an_ambiguous_one() {
    let c = Copy::new();
    assert_eq!(c.ok(&["sync", "peers"]).trim(), "no sync peers");
    let conn = rusqlite::Connection::open(c.db()).unwrap();
    for id in ["aaaa1111", "bbbb2222"] {
        conn.execute(
            "INSERT INTO sync_peers (endpoint_id, label, addrs, added_at, announced)
             VALUES (?1, 'laptop (cli)', '{}', '2026-09-29T00:00:00Z', 1)",
            [id],
        )
        .unwrap();
    }
    let both = c.fails(&["sync", "forget", "laptop (cli)"], "");
    assert!(both.contains("names 2 sync peers"), "{both}");
    assert!(c
        .ok(&["sync", "forget", "bbbb"])
        .contains("forgot sync peer laptop (cli) (bbbb2222)"));
    assert!(c
        .ok(&["sync", "forget", "laptop (cli)"])
        .contains("(aaaa1111)"));
    assert_eq!(c.ok(&["sync", "peers"]).trim(), "no sync peers");
}

/// A first sync whose apply outlasts the progress timeout (da-039jx): b applies
/// a's 300-odd ops at 20ms each, 6s, on a 3s timeout, sending a `BUSY` every
/// 64 ops (1.3s) unless `keepalive` is off. The connect's output, and whether
/// add-client exited 0 and what it printed.
fn slow_first_sync(keepalive: bool) -> (Copy, Copy, Output, (bool, String)) {
    let (mut a, mut b) = (Copy::new(), Copy::new());
    for i in 0..300 {
        a.ok(&["occur", "step", &format!("n={i}")]);
    }
    for c in [&mut a, &mut b] {
        c.env = vec![
            ("DATUM_SYNC_TIMEOUT_S", "3"),
            ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "20"),
            (
                "DATUM_SYNC_TEST_KEEPALIVE",
                if keepalive { "1" } else { "0" },
            ),
        ];
    }
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.run(&["sync", "connect", &ticket]);
    let served = inviting.finish();
    (a, b, joined, served)
}

#[test]
fn a_first_sync_longer_than_the_timeout_completes_on_keepalives() {
    let (a, b, joined, (ok, served)) = slow_first_sync(true);
    let text = String::from_utf8_lossy(&joined.stdout);
    assert!(
        joined.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&joined.stderr)
    );
    assert!(text.contains("verify ok"), "{text}");
    assert!(ok && served.contains("verify ok"), "{served}");
    assert_eq!(a.hash(), b.hash());
}

#[test]
fn without_keepalives_it_fails_says_what_landed_and_add_client_still_exits() {
    let (a, b, joined, (ok, served)) = slow_first_sync(false);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&joined.stdout),
        String::from_utf8_lossy(&joined.stderr)
    );
    assert!(!joined.status.success(), "{text}");
    assert!(text.contains("applied 30"), "{text}");
    assert!(text.contains("paired with"), "{text}");
    // The pairing held although the session failed: add-client exits 0.
    assert!(ok, "{served}");
    assert!(
        served.contains("paired with") && served.contains("datum sync now"),
        "{served}"
    );

    // And it is a pairing: the next sync finishes what the first began.
    let (mut a, mut b) = (a, b);
    a.env.clear();
    b.env.clear();
    let _a_runs = a.serve(&["sync", "run"]);
    let now = b.ok(&["sync", "now"]);
    assert!(now.contains("verify ok"), "{now}");
    assert_eq!(a.hash(), b.hash());
}
