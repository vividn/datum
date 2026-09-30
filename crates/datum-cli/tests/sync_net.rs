//! Easy Sync over iroh (da-rugg8): two copies of a database, each with its own
//! device, join one sync chain with a one-time invite (da-6177s) and sync over
//! QUIC — the real binary,
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
        let ticket = s
            .lines_until("datum sync chain connect ")
            .rsplit(' ')
            .next()
            .unwrap()
            .trim()
            .to_string();
        (s, ticket)
    }

    fn hash(&self) -> String {
        self.ok(&["sync", "pipe", "verify"]).trim().to_string()
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

/// `a` and `b`, in the open chain `a`'s invite started.
fn paired() -> (Copy, Copy) {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&[]);
    b.ok(&["sync", "chain", "connect", &ticket]);
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
    let joined = b.ok(&["sync", "chain", "connect", &ticket]);
    assert!(joined.contains("added sync peer"), "{joined}");
    assert!(joined.contains("verify ok"), "{joined}");
    let (ok, served) = inviting.finish();
    assert!(ok, "invite exits once redeemed: {served}");
    assert!(served.contains("verify ok"), "{served}");

    assert_eq!(a.hash(), b.hash());
    assert!(a.has("tea") && b.has("coffee"));
    assert!(a.ok(&["sync", "chain", "list"]).contains(&b.id()[..10]));
    assert!(b.ok(&["sync", "chain", "list"]).contains(&a.id()[..10]));
}

#[test]
fn an_invite_is_one_time_and_expires() {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&[]);
    // The ticket from stdin works as well as the argument.
    b.run_in(&["sync", "chain", "connect"], &ticket);
    assert!(inviting.finish().0);

    let (mut expiring, stale) = a.invite(&["--expires", "1s"]);
    let (ok, _) = expiring.finish();
    assert!(!ok, "an invite unredeemed when it expires fails");

    let _serving = a.serve(&["sync", "run"]);
    let c = Copy::new();
    let used = c.fails(&["sync", "chain", "connect", &ticket], "");
    assert!(used.contains("invite already used"), "{used}");
    // A member presenting it is refused before it dials: it is in a chain.
    let again = b.fails(&["sync", "chain", "connect", &ticket], "");
    assert!(again.contains("a copy belongs to one chain"), "{again}");
    let expired = c.fails(&["sync", "chain", "connect", &stale], "");
    assert!(expired.contains("invite expired"), "{expired}");
    assert!(!a.ok(&["sync", "chain", "list"]).contains(&c.id()[..10]));
}

/// A stranger that dials without an invite is refused by the store's
/// handshake (`sync_chain`'s `a_non_members_act_is_refused_open_chain_or_not`);
/// here, the copy of this key. It is in the inviter's chain, so it is refused
/// before it dials (da-mepav); the handshake's own refusal of the key is
/// `sync_chain`'s, where a hello can be forced.
#[test]
fn a_copy_of_this_key_is_refused() {
    let (a, _b) = paired();
    let (_inviting, ticket) = a.invite(&[]);
    let mut own = a.cmd(&["sync", "chain", "connect", &ticket]);
    own.env("DATUM_SYNC_BIND", format!("127.0.0.1:{}", Copy::new().port));
    let out = own.output().unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("a copy belongs to one chain"), "{text}");
}

/// A copy belongs to one chain (da-mepav): one already in a chain that tries
/// another chain's ticket is refused by name before anything is written or
/// sent — no admit in that chain, none of its ops there, no record of its
/// own. A copy in no chain still joins (`paired`).
#[test]
fn a_copy_in_a_chain_is_refused_another_chains_invite() {
    let (a, _b) = paired();
    a.ok(&["occur", "secret"]);
    let c = Copy::new();
    let (_inviting, ticket) = c.invite(&[]);
    let ops = a.query("SELECT COUNT(*) FROM ops");
    let refused = a.fails(&["sync", "chain", "connect", &ticket], "");
    let chain = a
        .query("SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1");
    assert!(
        refused.contains(&format!(
            "this copy is in sync chain {chain} (2 members); a copy belongs to one chain. \
             To join another, use a copy that isn't in one"
        )),
        "{refused}"
    );
    assert_eq!(a.query("SELECT COUNT(*) FROM ops"), ops);
    assert!(!c.has("secret"));
    assert!(!c.ok(&["sync", "chain", "list"]).contains(&a.id()[..10]));
}

#[test]
fn run_serves_members_and_pushes_new_ops() {
    let (a, b) = paired();
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["occur", "walk"]);
    let now = b.ok(&["sync"]);
    // The walk, the spec its first log declares, and the describe b wrote
    // of itself once admitted.
    assert!(
        now.contains("sent 3, got 0, conflicts 0, verify ok"),
        "{now}"
    );
    assert!(a.has("walk"));

    let _b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "swim"]);
    eventually("a's write reaching b", || b.has("swim"));
    eventually("equal hashes", || a.hash() == b.hash());
}

/// `sync daemon run` (da-vd2sn) finds a chained database datum knows of —
/// here by the registry — and serves it as `sync run` does.
#[test]
fn daemon_run_serves_every_chained_database() {
    let (a, b) = paired();
    let config = b.tmp.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let registry = format!("[db.data]\npath = {:?}\n", b.db().display().to_string());
    std::fs::write(config.join("dbs.toml"), registry).unwrap();
    let mut daemon = b.cmd(&["sync", "daemon", "run"]);
    let _b_runs = Serving::of(daemon.stderr(Stdio::inherit()));
    a.ok(&["occur", "swim"]);
    eventually("a's sync reaching b's daemon", || {
        a.run(&["sync"]).status.success() && b.has("swim")
    });
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
        assert!(conflicts.contains("sync_conflict"), "{conflicts}");
    }
}

#[test]
fn a_new_member_is_announced_once() {
    let (a, b) = paired();
    for copy in [&a, &b] {
        let first = String::from_utf8(copy.run(&["tail"]).stderr).unwrap();
        assert!(first.contains("⚠ new sync endpoint"), "{first}");
        assert!(
            first.contains("`datum sync chain list` to review"),
            "{first}"
        );
        let second = String::from_utf8(copy.run(&["tail"]).stderr).unwrap();
        assert!(!second.contains("new sync"), "{second}");
    }
}

/// A first sync whose apply outlasts the progress timeout (da-039jx): b applies
/// a's 300-odd ops at 20ms each, 6s, on a 3s timeout, sending a `BUSY` every
/// 64 ops (1.3s) unless `keepalive` is off. The connect's output, and whether
/// the invite exited 0 and what it printed.
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
    let joined = b.run(&["sync", "chain", "connect", &ticket]);
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
fn without_keepalives_it_fails_says_what_landed_and_the_invite_still_exits() {
    let (a, b, joined, (ok, served)) = slow_first_sync(false);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&joined.stdout),
        String::from_utf8_lossy(&joined.stderr)
    );
    assert!(!joined.status.success(), "{text}");
    assert!(text.contains("applied 30"), "{text}");
    assert!(text.contains("paired with"), "{text}");
    // The admission held although the session failed: the invite exits 0.
    assert!(ok, "{served}");
    assert!(
        served.contains("paired with") && served.contains("run `datum sync`"),
        "{served}"
    );

    // And it is a pairing: the next sync finishes what the first began.
    let (mut a, mut b) = (a, b);
    a.env.clear();
    b.env.clear();
    let _a_runs = a.serve(&["sync", "run"]);
    let now = b.ok(&["sync"]);
    assert!(now.contains("verify ok"), "{now}");
    assert_eq!(a.hash(), b.hash());
}

// ─── The sync chain (da-odwc7) ──────────────────────────────────────────────

#[test]
fn a_member_admits_an_invites_redeemer_while_the_inviter_is_offline() {
    let (a, b) = paired();
    a.ok(&["occur", "from-a"]);
    let (inviting, ticket) = a.invite(&[]);
    // b fetches the invite, then a goes away.
    b.ok(&["sync"]);
    drop(inviting);
    let _b_runs = b.serve(&["sync", "run"]);

    let mut c = Copy::new();
    c.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    c.ok(&["occur", "from-c"]);
    let joined = c.ok(&["sync", "chain", "connect", &ticket]);
    assert!(joined.contains("verify ok"), "{joined}");
    assert!(c.has("from-a") && b.has("from-c"));
    let members = b.ok(&["sync", "chain", "list"]);
    assert!(members.matches("via invite").count() == 2, "{members}");
    assert!(members.contains(&c.id()[..10]), "{members}");
}

#[test]
fn a_member_that_has_not_seen_the_invite_passes_the_redeemer_on() {
    let (a, b) = paired();
    // d joins a second later, so a's ticket lists b before d.
    std::thread::sleep(Duration::from_millis(1100));
    let d = Copy::new();
    let (mut inviting, ticket) = a.invite(&[]);
    d.ok(&["sync", "chain", "connect", &ticket]);
    assert!(inviting.finish().0);

    // Only d fetches the next invite before a goes away; b is behind.
    let (inviting, ticket) = a.invite(&[]);
    d.ok(&["sync"]);
    drop(inviting);
    let (_b_runs, _d_runs) = (b.serve(&["sync", "run"]), d.serve(&["sync", "run"]));
    let mut c = Copy::new();
    c.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    let joined = c.ok(&["sync", "chain", "connect", &ticket]);
    assert!(joined.contains("verify ok"), "{joined}");
    assert!(d.ok(&["sync", "chain", "list"]).contains(&c.id()[..10]));
}

// ─── A device's identity, and each session recorded (da-rcb5m) ──────────────

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

#[test]
fn a_redeemer_describes_itself_and_every_session_is_recorded_for_status() {
    let (a, mut b) = paired();
    // The redemption was a session on both sides; b then described itself,
    // which a has not had yet.
    assert_eq!(entry_of(&a, "status", &b)["state"], "synced");
    let behind = entry_of(&b, "status", &a);
    assert_eq!(
        (behind["state"].as_str(), behind["to_send"].as_i64()),
        (Some("pending"), Some(1)),
        "{behind}"
    );
    assert!(entry_of(&a, "chain list", &b)["client"].is_null());

    // a serves (an invite being offered serves members too), b dials.
    let (serving, _) = a.invite(&[]);
    b.ok(&["sync"]);
    let described = entry_of(&a, "chain list", &b);
    assert_eq!(
        described["client"],
        b.query("SELECT client_id FROM sync_local").as_str(),
        "{described}"
    );
    assert_eq!(described["kind"], "cli");
    for (x, y) in [(&a, &b), (&b, &a)] {
        let row = entry_of(x, "status", y);
        assert_eq!(
            (row["state"].as_str(), row["to_send"].as_i64()),
            (Some("synced"), Some(0)),
            "{row}"
        );
        assert!(row["last_verify"]["at"].is_string(), "{row}");
    }

    // a gone, b's session fails, and says why in the words bare `sync` printed.
    drop(serving);
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    let failed = b.fails(&["sync"], "");
    let row = entry_of(&b, "status", &a);
    assert_eq!(row["state"], "error", "{row}");
    let why = row["error"].as_str().unwrap();
    assert!(
        failed.contains(why) && row["error_at"].is_string(),
        "{why} / {failed}"
    );
}

/// A reader writes nothing, so a copy holding entries of its own is refused
/// a reader invite, filtered or not, by count (da-tti3w): it is in no chain
/// after and its entries are as they were, and the invite, unused, admits a
/// fresh copy. A copy holding entries takes a member invite as before
/// (`an_invite_pairs_two_copies_and_their_data_converges`).
#[test]
fn a_copy_holding_entries_is_refused_a_reader_invite() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    for caps in [&["--reader"][..], &["--fields", "coffee"]] {
        let r = Copy::new();
        r.ok(&["occur", "tea"]);
        r.ok(&["occur", "tea", "-t", "-5"]);
        let (mut inviting, ticket) = a.invite(caps);
        let refused = r.fails(&["sync", "chain", "connect", &ticket], "");
        assert!(
            refused.contains("holds 2 entries of its own; a reader writes nothing"),
            "{refused}"
        );
        assert_eq!(
            r.query("SELECT COUNT(*) FROM ops WHERE hid = '~chain'"),
            "0"
        );
        assert_eq!(r.ok(&["count"]).trim(), "2");
        assert!(!r.has("coffee"));

        let fresh = Copy::new();
        let joined = fresh.ok(&["sync", "chain", "connect", &ticket]);
        assert!(joined.contains("verify ok"), "{joined}");
        assert!(fresh.has("coffee"));
        assert!(inviting.finish().0);
    }
}

/// A filtered reader over iroh (da-u81u1): the member serving it knows who
/// it is and sends its fields alone — more than one apply chunk of them —
/// the verify over that set agrees, and a widened filter brings the rest
/// on the next session, in step again.
#[test]
fn a_filtered_reader_over_the_link_gets_its_fields_and_verifies() {
    let (a, r) = (Copy::new(), Copy::new());
    for m in 1..=70 {
        a.ok(&["occur", "coffee", "-t", &format!("-{m}")]);
    }
    a.ok(&["occur", "tea", "-t", "-100"]);
    let (mut inviting, ticket) = a.invite(&["--fields", "coffee"]);
    let joined = r.ok(&["sync", "chain", "connect", &ticket]);
    assert!(joined.contains("verify ok"), "{joined}");
    assert!(inviting.finish().0);
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'coffee'"),
        "70"
    );
    assert!(!r.has("tea"));

    let who = r.id()[..10].to_string();
    a.ok(&[
        "sync",
        "chain",
        "role",
        &who,
        "reader",
        "--fields",
        "coffee,tea",
    ]);
    let _a_runs = a.serve(&["sync", "run"]);
    let now = r.ok(&["sync"]);
    assert!(now.contains("verify ok"), "{now}");
    assert!(r.has("tea"));
}
