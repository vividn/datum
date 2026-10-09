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

use tempfile::TempDir;

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
}

impl Copy {
    fn new() -> Self {
        let (port, _claim) = claim_port();
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
            port,
            _claim,
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
        let line = s.lines_until("datum sync chain connect ");
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
    b.ok(&["sync", "chain", "connect", &ticket]);
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

/// An invite prints a datum.day join link (da-l9kdr), which `connect`
/// takes whole, as it takes the bare ticket the link's fragment holds.
#[test]
fn connect_takes_the_join_link_or_its_bare_ticket() {
    let (a, b, c) = (Copy::new(), Copy::new(), Copy::new());
    let (mut inviting, link) = a.invite(&[]);
    assert!(link.starts_with("https://datum.day/sync/join#"), "{link}");
    assert!(b
        .ok(&["sync", "chain", "connect", &link])
        .contains("verify ok"));
    assert!(inviting.finish().0);

    let (mut inviting, link) = a.invite(&[]);
    let bare = link.rsplit('#').next().unwrap();
    assert!(c
        .ok(&["sync", "chain", "connect", bare])
        .contains("verify ok"));
    assert!(inviting.finish().0);
    assert_eq!(a.hash(), c.hash());
}

/// The connect line an invite prints, pasted into a shell, hands `connect` the
/// whole link as one argument (da-sin5b). Unquoted, zsh with `extendedglob`
/// refused it — the `#` is a glob there, «no matches found» — and sh took it.
#[test]
fn the_printed_connect_line_pastes_into_a_shell() {
    let a = Copy::new();
    let mut inviting = a.serve(&["sync", "chain", "invite"]);
    let line = inviting.lines_until("datum sync chain connect ");
    let link = line.trim().rsplit(' ').next().unwrap().trim_matches('\'');
    assert!(link.starts_with("https://datum.day/sync/join#"), "{line}");
    let args = line.trim().strip_prefix("datum ").unwrap();
    for shell in [
        &["sh", "-c"][..],
        &["zsh", "-f", "-o", "extendedglob", "-c"],
    ] {
        let Ok(out) = Command::new(shell[0])
            .args(&shell[1..])
            .arg(format!("printf '%s\\n' {args}"))
            .output()
        else {
            eprintln!("{} is not installed: not tried", shell[0]);
            continue;
        };
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{}: {}",
            shell[0],
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            said.lines().collect::<Vec<_>>(),
            ["sync", "chain", "connect", link],
            "{}",
            shell[0]
        );
    }
}

/// A member that is not running is named, with what runs it (da-shfv4):
/// «timed out» alone said neither which side nor what to do.
#[test]
fn a_member_not_running_is_named_with_what_runs_it() {
    let (_a, mut b) = paired();
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "2")];
    let said = b.fails(&["sync"], "");
    for words in [
        "could not reach",
        "is the other copy running?",
        "`datum sync daemon status`",
        "a tab is open on the database",
    ] {
        assert!(said.contains(words), "{words}: {said}");
    }
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
    // Why, and what to do next (da-0tawk).
    assert!(
        used.contains("Next: ask") && used.contains("for a fresh invite"),
        "{used}"
    );
    // A member presenting it is a redeemer like any (da-j8c4l): refused.
    let again = b.fails(&["sync", "chain", "connect", &ticket], "");
    assert!(again.contains("invite already used"), "{again}");
    let expired = c.fails(&["sync", "chain", "connect", &stale], "");
    assert!(expired.contains("invite expired"), "{expired}");
    assert!(expired.contains("for a fresh invite"), "{expired}");
    assert!(!a.ok(&["sync", "chain", "list"]).contains(&c.id()[..10]));
}

/// `connect` says at once whom it waits for and until when, says every few
/// seconds that it still is, and a join nobody answers ends saying why and
/// what to do next (da-0tawk): vividn's terminal sat silent for a minute.
#[test]
fn connect_says_what_it_waits_for_and_why_it_ended_unanswered() {
    let (a, mut b) = (Copy::new(), Copy::new());
    let (inviting, ticket) = a.invite(&[]);
    // Nobody serves the invite now.
    drop(inviting);
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "6")];
    let out = b.fails(&["sync", "chain", "connect", &ticket], "");
    let lines: Vec<&str> = out.lines().collect();
    let waiting = lines.iter().position(|l| {
        l.contains("'s sync chain: waiting for it to answer (the invite holds until ")
    });
    let still = lines
        .iter()
        .position(|l| l.starts_with("still waiting for "));
    let ended = lines.iter().position(|l| l.contains("did not answer"));
    assert!(
        matches!((waiting, still, ended), (Some(w), Some(s), Some(e)) if w < s && s < e),
        "{out}"
    );
    assert!(out.contains("datum.day open on that database"), "{out}");
    assert!(out.contains("or ask it for a fresh invite"), "{out}");
}

/// A ticket past its time that nobody answers says the invite expired, in
/// its own words and with no «did not answer» after them: the failure is
/// its own kind, though an unreachable one lies under it (da-b6xh6).
#[test]
fn a_late_ticket_nobody_answers_says_it_expired() {
    let (a, mut c) = (Copy::new(), Copy::new());
    let (mut expiring, stale) = a.invite(&["--expires", "1s"]);
    assert!(!expiring.finish().0);
    c.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    let late = c.fails(&["sync", "chain", "connect", &stale], "");
    // As before da-b6xh6: the expiry, caused by the leg that was down.
    assert!(
        late.contains("invite expired: ask the inviting device for a new one\n\nCaused by:")
            && late.contains("could not reach "),
        "{late}"
    );
    assert!(!late.contains("Next:"), "{late}");
}

/// A refusal reaches the redeemer as a kind beside its words, and the next
/// step follows the kind (da-b6xh6): an invite the inviter does not hold asks
/// for a fresh one, and an inviter whose sync is paused is named as the copy
/// to resume, its words («paused on this copy») being the inviter's.
#[test]
fn a_refusal_names_its_next_step_by_its_kind() {
    let (a, c) = (Copy::new(), Copy::new());
    let (_inviting, ticket) = a.invite(&[]);
    let (head, code) = ticket.rsplit_once('#').unwrap();
    let b32 = data_encoding::BASE32_NOPAD;
    let mut t: serde_json::Value =
        serde_json::from_slice(&b32.decode(code.to_uppercase().as_bytes()).unwrap()).unwrap();
    t["secret"] = "not-an-invite-this-chain-holds".into();
    let forged = format!("{head}#{}", b32.encode(&serde_json::to_vec(&t).unwrap()));
    let unknown = c.fails(&["sync", "chain", "connect", &forged], "");
    assert!(unknown.contains("refused: unknown invite"), "{unknown}");
    assert!(unknown.contains("Next: ask "), "{unknown}");
    assert!(unknown.contains(" for a fresh invite"), "{unknown}");

    a.ok(&["sync", "pause"]);
    let paused = c.fails(&["sync", "chain", "connect", &ticket], "");
    assert!(
        paused.contains("refused: sync is paused on this copy"),
        "{paused}"
    );
    assert!(paused.contains("Next: sync is paused on "), "{paused}");
    assert!(
        paused.contains(", not on this copy: resume it there"),
        "{paused}"
    );
}

/// A stranger that dials without an invite is refused by the store's
/// handshake (`sync_chain`'s `a_non_members_act_is_refused_open_chain_or_not`);
/// here, the copy of this key, refused before it dials. In the inviter's
/// chain, it would rejoin (da-j8c4l); the handshake's own refusal of the key
/// is `sync_chain`'s, where a hello can be forced.
#[test]
fn a_copy_of_this_key_is_refused() {
    let (a, _b) = paired();
    let (_inviting, ticket) = a.invite(&[]);
    let mut own = a.cmd(&["sync", "chain", "connect", &ticket]);
    let (port, _claim) = claim_port();
    own.env("DATUM_SYNC_BIND", format!("127.0.0.1:{port}"));
    let out = own.output().unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("is a copy of this database"), "{text}");
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
             To join another, `datum sync chain leave` first (every entry is kept), or use \
             a copy that isn't in one"
        )),
        "{refused}"
    );
    assert_eq!(a.query("SELECT COUNT(*) FROM ops"), ops);
    assert!(!c.has("secret"));
    assert!(!c.ok(&["sync", "chain", "list"]).contains(&a.id()[..10]));
}

/// Two copies that each turned sync on — a chain of one apiece, each with an
/// entry — join seamlessly (da-h7pnc): the redeemer leaves its own chain,
/// and its entries come along into the other.
#[test]
fn a_chain_of_one_joins_another_chain_its_entries_with_it() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee"]);
    b.ok(&["occur", "tea"]);
    drop(b.invite(&[]));
    let b_chain =
        b.query("SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain'");
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.ok(&["sync", "chain", "connect", &ticket]);
    let a_chain =
        a.query("SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain'");
    assert!(
        joined.contains(&format!(
            "this copy's own sync chain {b_chain} (just itself) is replaced by sync chain {a_chain}"
        )),
        "{joined}"
    );
    assert!(joined.contains("verify ok"), "{joined}");
    assert!(inviting.finish().0);
    assert!(a.has("tea") && b.has("coffee"));
    assert_eq!(a.hash(), b.hash());
    let list = b.ok(&["sync", "chain", "list", "--json"]);
    assert!(
        list.contains(&a_chain) && !list.contains(&b_chain),
        "{list}"
    );
    for copy in [&a, &b] {
        let list = copy.ok(&["sync", "chain", "list"]);
        assert!(
            list.contains(&a.id()[..10]) && list.contains(&b.id()[..10]),
            "{list}"
        );
    }
}

/// A chain of one leaves only once its own handshake admits the inviter too:
/// paused, it refuses the session after the welcome, and keeps its chain.
#[test]
fn a_paused_chain_of_one_refused_at_its_own_handshake_keeps_its_chain() {
    let (a, b) = (Copy::new(), Copy::new());
    b.ok(&["occur", "tea"]);
    drop(b.invite(&[]));
    b.ok(&["sync", "pause"]);
    let ops = b.query("SELECT COUNT(*) FROM ops");
    let (_inviting, ticket) = a.invite(&[]);
    let refused = b.fails(&["sync", "chain", "connect", &ticket], "");
    assert!(refused.contains("paused"), "{refused}");
    // Its OWN pause: no next step pointing at the inviter (da-b6xh6).
    assert!(!refused.contains("Next:"), "{refused}");
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), ops);
    assert!(!a.has("tea"));
}

/// A chain of one still holding a revoked member's data is refused another
/// chain's invite, as a bigger chain is (da-mepav): that data is not its own.
#[test]
fn a_chain_of_one_holding_a_revoked_members_data_is_refused() {
    let (a, b) = paired();
    {
        let _a_runs = a.serve(&["sync", "run"]);
        b.ok(&["occur", "walk"]);
        b.ok(&["sync"]);
    }
    assert!(a.has("walk"));
    a.ok(&["sync", "chain", "revoke", &b.id()]);
    let c = Copy::new();
    let (_inviting, ticket) = c.invite(&[]);
    let refused = a.fails(&["sync", "chain", "connect", &ticket], "");
    assert!(
        refused.contains("(1 members); a copy belongs to one chain"),
        "{refused}"
    );
    assert!(!c.has("walk"));
}

/// `db purge` destroys sync too (da-8v7ua, vividn's reproduction): a copy
/// that synced, revoked the other and was refused `chain leave --alone` keeps
/// nothing after a purge — no op, no chain, no key — and its next invite
/// starts a new chain as a new endpoint.
#[test]
fn a_purge_drops_the_ops_the_chain_and_the_endpoint_key() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "coffee"]);
    let (mut inviting, ticket) = a.invite(&[]);
    assert!(b
        .ok(&["sync", "chain", "connect", &ticket])
        .contains("verify ok"));
    assert!(inviting.finish().0);
    b.ok(&["sync", "chain", "revoke", &a.id()]);
    let held = b.fails(&["sync", "chain", "leave", "--alone"], "");
    assert!(held.contains("holds other copies' data"), "{held}");
    let (old_id, old_chain) = (b.id(), b.query(CHAIN));

    let purged = b.ok(&["db", "purge", "-y"]);
    for table in ["entries", "ops", "sync_local", "sync_peers", "sync_notices"] {
        assert!(purged.contains(table), "{table} not dropped: {purged}");
    }
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
    assert_eq!(b.query("SELECT COUNT(*) FROM sync_local"), "0");
    let list = b.fails(&["sync", "chain", "list"], "");
    assert!(list.contains("in no sync chain"), "{list}");

    let (_inviting, _) = b.invite(&[]);
    assert_ne!(b.id(), old_id);
    assert_ne!(b.query(CHAIN), old_chain);
}

const CHAIN: &str =
    "SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1";

/// A copy purged while `sync run` serves it (the daemon's loop) stops
/// serving the old key, and a member of the old chain that dials it gets
/// nothing and sends nothing (da-8v7ua).
#[test]
fn a_purged_copy_serves_the_old_chain_nothing() {
    let (a, b) = paired();
    let mut b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "tea"]);
    a.ok(&["sync"]);
    assert!(b.has("tea"));

    b.ok(&["db", "purge", "-y"]);
    let start = Instant::now();
    while b_runs.child.try_wait().unwrap().is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "run still serving the old key"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let (ok, _) = b_runs.finish();
    let mut why = String::new();
    b_runs
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut why)
        .unwrap();
    assert!(!ok && why.contains("endpoint key changed"), "{why}");

    let _b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "walk"]);
    let entries = a.query("SELECT COUNT(*) FROM entries");
    assert!(!a.run(&["sync"]).status.success());
    assert!(!b.has("tea") && !b.has("walk"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
    assert_eq!(a.query("SELECT COUNT(*) FROM entries"), entries);
}

/// A purge leaves nothing of the database, as deleting its file would
/// (da-nc8i1), and with `sync run` holding the file open: no page of what it
/// held in the file or its `-wal`, no row but a new copy's own, and none of
/// its pre-migration snapshots, each a whole copy of an earlier state, which
/// the summary names. Another database's snapshot beside it stays.
#[test]
fn a_purge_leaves_nothing_of_the_database_in_its_file_or_beside_it() {
    const MARK: &str = "zebra-marker-77";
    let (a, b) = paired();
    let _b_runs = b.serve(&["sync", "run"]);
    // Long enough to fill more pages than the new schema takes back: a page
    // it reuses is zeroed whether or not the file is rewritten.
    a.ok(&["occur", "tea", &format!("note={}", MARK.repeat(8000))]);
    a.ok(&["sync"]);
    assert!(b.has("tea"));
    let old_id = b.id();
    let own = b
        .tmp
        .path()
        .join("data.db.pre-v1-to-v2-20260101T000000Z.bak");
    std::fs::write(&own, MARK).unwrap();
    std::fs::write(
        b.tmp
            .path()
            .join("other.db.pre-v1-to-v2-20260101T000000Z.bak"),
        MARK,
    )
    .unwrap();
    // The files beside the database that hold the marker, by name.
    let marked = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(b.tmp.path())
            .unwrap()
            .flatten()
            .filter(|e| {
                std::fs::read(e.path())
                    .is_ok_and(|bytes| bytes.windows(MARK.len()).any(|w| w == MARK.as_bytes()))
            })
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    };
    let before = marked();
    assert!(
        before.iter().any(|n| n == "data.db" || n == "data.db-wal") && before.len() >= 3,
        "{before:?}"
    );

    let purged = b.ok(&["db", "purge", "-y"]);
    assert!(
        purged.contains(&format!("removed: {}", own.display())) && !purged.contains("other.db"),
        "{purged}"
    );
    assert_eq!(marked(), ["other.db.pre-v1-to-v2-20260101T000000Z.bak"]);
    // `_meta` is the new schema's stamp, and `sync_local` the key `sync run`
    // mints for the new copy before it stops.
    let tables = b.query(
        "SELECT group_concat(name) FROM sqlite_master WHERE type = 'table' \
         AND name NOT IN ('_meta', 'sync_local')",
    );
    for table in tables.split(',') {
        let rows = b.query(&format!("SELECT COUNT(*) FROM \"{table}\""));
        assert_eq!(rows, "0", "{table}");
    }
    assert_eq!(
        b.query(&format!(
            "SELECT COUNT(*) FROM sync_local WHERE endpoint_id = '{old_id}'"
        )),
        "0"
    );
}

/// A revoked device rejoins by a fresh invite to its own chain (da-j8c4l),
/// as a new device joins: what each holds, both will — the entry it logged
/// while out among them — and it stays in that chain, with its endpoint.
#[test]
fn a_revoked_device_rejoins_by_a_fresh_invite_and_data_goes_both_ways() {
    let (a, b) = paired();
    let chain = a
        .query("SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1");
    let b_id = b.id();
    a.ok(&["sync", "chain", "revoke", &b_id]);
    b.ok(&["occur", "out"]);
    a.ok(&["occur", "meanwhile"]);
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.ok(&["sync", "chain", "connect", &ticket]);
    assert!(joined.contains("verify ok"), "{joined}");
    assert!(!joined.contains("is replaced by"), "{joined}");
    assert!(inviting.finish().0);
    assert!(a.has("out") && b.has("meanwhile"));
    assert_eq!(a.hash(), b.hash());
    assert_eq!(b.id(), b_id);
    let list = b.ok(&["sync", "chain", "list", "--json"]);
    assert!(list.contains(&chain), "{list}");
    let list = a.ok(&["sync", "chain", "list"]);
    assert!(
        !list.contains("REVOKED") && list.contains(", again "),
        "{list}"
    );
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

/// A copy behind a member's schema syncs on and warns to upgrade (da-3zqyv):
/// once on stderr, naming the member and both versions, and for as long as
/// it lasts in `sync status` and `datum check`; the newer copy says nothing.
#[test]
fn a_copy_behind_a_members_schema_syncs_and_warns_to_upgrade() {
    let (a, mut b) = paired();
    b.env.push(("DATUM_SYNC_TEST_SCHEMA", "999"));
    let mine = format!(
        "(schema {})",
        datum_core::migrations::CURRENT_SCHEMA_VERSION
    );
    let b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "walk"]);
    let first = a.run(&["sync"]);
    let (out, err) = (
        String::from_utf8(first.stdout).unwrap(),
        String::from_utf8(first.stderr).unwrap(),
    );
    assert!(out.contains("verify ok"), "{out}{err}");
    assert!(b.has("walk"));
    let label = member_label(&a, &b.id());
    for said in [&format!("⚠ {label} runs datum 3.0."), "(schema 999)", &mine] {
        assert!(err.contains(said), "{said} in: {err}");
    }
    assert!(err.contains("`git pull && scripts/install.sh`"), "{err}");
    let again = a.run(&["sync"]);
    assert!(again.status.success());
    let again = String::from_utf8(again.stderr).unwrap();
    assert!(!again.contains("schema 999"), "warned once: {again}");
    for (what, args) in [("status", &["sync", "status"][..]), ("check", &["check"])] {
        let shown = String::from_utf8(a.run(args).stdout).unwrap();
        assert!(shown.contains("(schema 999)"), "{what}: {shown}");
    }
    let list = a.ok(&["sync", "chain", "list", "--json"]);
    assert_eq!(member_of_list(&list, &b.id())["schema"], 999);

    // The newer copy dials the older one and warns of nothing.
    drop(b_runs);
    let _a_runs = a.serve(&["sync", "run"]);
    a.ok(&["occur", "swim"]);
    let newer = b.run(&["sync"]);
    let err = String::from_utf8(newer.stderr).unwrap();
    assert!(newer.status.success() && b.has("swim"), "{err}");
    assert!(!err.contains("runs datum"), "{err}");
    assert!(!b.ok(&["sync", "status"]).contains("runs datum"));
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
    // The redeemer is told it was admitted, and what finishes it (da-0tawk):
    // cut short before the chain's records, it is only joining (da-idw54).
    assert!(
        text.contains("this copy was admitted to its sync chain")
            && text.contains("`datum sync` finishes it"),
        "{text}"
    );
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

/// A first sync says how far it is on both sides (da-nyg6u): vividn's joiner
/// went silent for a minute and then printed ten lines that all said «87s»,
/// and his inviter said nothing until it was done. b applies a's 300-odd ops
/// at 40ms each: 12s, in chunks that each hold its thread for 2.5s.
#[test]
fn a_first_sync_says_how_far_it_is_on_both_sides() {
    let (a, mut b) = (Copy::new(), Copy::new());
    for i in 0..300 {
        a.ok(&["occur", "step", &format!("n={i}")]);
    }
    b.env = vec![("DATUM_SYNC_TEST_APPLY_DELAY_MS", "40")];
    let (mut inviting, ticket) = a.invite(&[]);
    let mut joining = b.cmd(&["sync", "chain", "connect", &ticket]);
    let mut joining = joining.spawn().expect("spawn datum");

    // The inviter names the joiner while the joiner is still at it.
    let mut said = BufReader::new(inviting.child.stderr.take().unwrap());
    let mut named = String::new();
    while !named.contains(" is joining: sending") && said.read_line(&mut named).unwrap() > 0 {}
    assert!(named.contains(" is joining: sending"), "{named}");
    assert!(joining.try_wait().unwrap().is_none(), "the join had ended");

    let joined = joining.wait_with_output().unwrap();
    let (out, err) = (
        String::from_utf8_lossy(&joined.stdout),
        String::from_utf8_lossy(&joined.stderr),
    );
    assert!(
        joined.status.success() && out.contains("verify ok"),
        "{out}{err}"
    );
    // The joiner says how many ops are applied, and each line it says is an
    // interval on from the last: none held back, then said together.
    let at: Vec<u64> = err
        .lines()
        .filter_map(|l| l.strip_suffix("s)")?.rsplit_once('(')?.1.parse().ok())
        .collect();
    assert!(
        at.len() > 1 && at.windows(2).all(|w| w[0] + 5 <= w[1]),
        "{err}"
    );
    assert!(err.contains(" ops applied ("), "{err}");

    let (ok, served) = inviting.finish();
    assert!(ok && served.contains("verify ok"), "{served}");
    let mut rest = String::new();
    said.read_to_string(&mut rest).unwrap();
    assert!(
        rest.contains(" ops sent, waiting for it to apply them ("),
        "{rest}"
    );
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
    let mut d = Copy::new();
    let (mut inviting, ticket) = a.invite(&[]);
    d.ok(&["sync", "chain", "connect", &ticket]);
    assert!(inviting.finish().0);

    // Only d fetches the next invite before a goes away; b is behind, and
    // not up: d, told where b is (da-fly3f), dials it too and reaches no one.
    let (inviting, ticket) = a.invite(&[]);
    d.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    let synced = d.fails(&["sync"], "");
    assert!(synced.contains("verify ok"), "{synced}");
    drop(inviting);
    let (_b_runs, _d_runs) = (b.serve(&["sync", "run"]), d.serve(&["sync", "run"]));
    let mut c = Copy::new();
    c.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    let joined = c.ok(&["sync", "chain", "connect", &ticket]);
    assert!(joined.contains("verify ok"), "{joined}");
    assert!(d.ok(&["sync", "chain", "list"]).contains(&c.id()[..10]));
}

// ─── Members one inviter admitted (da-fly3f) ────────────────────────────────

/// `a`, `b` and `c` in one chain, `b` and `c` both admitted by `a`, which
/// has gone away: `b` heard of `c` from `a` first. A dial that reaches no
/// one fails in 3s.
fn admitted_by_one_inviter() -> (Copy, Copy, Copy) {
    let (a, mut b) = paired();
    let mut c = Copy::new();
    let (mut inviting, ticket) = a.invite(&[]);
    c.ok(&["sync", "chain", "connect", &ticket]);
    assert!(inviting.finish().0);
    let (serving, _) = a.invite(&[]);
    b.ok(&["sync"]);
    drop(serving);
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    c.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    (a, b, c)
}

/// Two members one inviter admitted have never dialed each other: each
/// learns where the other is from the inviter's handshake, so with the
/// inviter gone they sync directly, and verify.
#[test]
fn two_members_one_inviter_admitted_sync_with_each_other_directly() {
    let (a, b, c) = admitted_by_one_inviter();
    for (x, y) in [(&b, &c), (&c, &b)] {
        let row = entry_of(x, "status", y);
        assert_eq!(row["state"], "never", "{row}");
        assert!(row["addr"].is_string() && row["to_send"] != 0, "{row}");
    }
    b.ok(&["occur", "from-b"]);
    c.ok(&["occur", "from-c"]);
    // Each dials the other in turn; an invite being offered serves members.
    for (x, y) in [(&b, &c), (&c, &b)] {
        let (_serving, _) = x.invite(&[]);
        let out = y.fails(&["sync"], "");
        assert!(out.contains("verify ok"), "{out}");
        assert!(out.contains("1 of 2 peer(s) failed"), "a is gone: {out}");
    }
    assert!(b.has("from-c") && c.has("from-b") && !a.has("from-b"));
    assert_eq!(b.hash(), c.hash());
    for (x, y) in [(&b, &c), (&c, &b)] {
        let row = entry_of(x, "status", y);
        assert_eq!(
            (row["state"].as_str(), row["to_send"].as_i64()),
            (Some("synced"), Some(0)),
            "{row}"
        );
    }
}

/// A member this copy has no address for, as a copy before da-fly3f leaves
/// every member it did not meet: `sync` and `sync status` say so, and count
/// nothing as waiting for it. An address a member passes on is kept for it,
/// and for no one else: not over one a handshake gave, not for this copy,
/// not for a copy outside the chain.
#[test]
fn a_member_with_no_address_is_named_and_an_address_passed_on_is_kept() {
    let (a, b, c) = admitted_by_one_inviter();
    let sql = rusqlite::Connection::open(b.db()).unwrap();
    let forget = "DELETE FROM sync_peers WHERE endpoint_id = ?1";
    assert_eq!(sql.execute(forget, [c.id()]).unwrap(), 1);
    let none = format!(
        "no address for it yet: it syncs through {}",
        member_label(&b, &a.id())
    );
    let row = entry_of(&b, "status", &c);
    assert_eq!(
        (row["state"].as_str(), row["to_send"].as_i64()),
        (Some("never"), Some(0)),
        "{row}"
    );
    assert!(row["addr"].is_null(), "{row}");
    let status = b.ok(&["sync", "status"]);
    let line = status.lines().find(|l| l.contains(&c.id()[..10])).unwrap();
    assert!(
        line.contains("to send 0") && line.ends_with(&none),
        "{line}"
    );
    let out = b.fails(&["sync"], "");
    assert!(out.contains(&format!(": {none}\n")), "{out}");

    let at = |id: &str| serde_json::json!({ "id": id, "addrs": [] });
    let stranger = "ab".repeat(32);
    let (mine, seen, lacked) = (b.id(), a.id(), c.id());
    let met = serde_json::json!([at(&mine), at(&seen), at(&lacked), at(&stranger)]).to_string();
    let before = entry_of(&b, "status", &a)["addr"].clone();
    b.ok(&["sync", "record-session", &seen, "--met", &met]);
    assert_eq!(entry_of(&b, "status", &a)["addr"], before);
    let row = entry_of(&b, "status", &c);
    assert_eq!(row["addr"], at(&lacked).to_string(), "{row}");
    assert_eq!(
        (row["state"].as_str(), row["to_send"] != 0),
        (Some("never"), true)
    );
    let rows = b.query("SELECT COUNT(*) FROM sync_peers");
    assert_eq!(rows, "2", "a and c alone");
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
        // A refusal of no kind of its own (da-b6xh6).
        assert!(
            refused.contains("Next: `datum sync status` shows where this copy stands"),
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

// ─── A member only its inviter has heard of (da-2u95o) ──────────────────────

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
    c.ok(&["sync", "chain", "connect", &ticket]);
    assert!(inviting.finish().0);
    let at = format!(
        "SELECT addrs FROM sync_peers WHERE endpoint_id = '{}'",
        b.id()
    );
    let told = "INSERT OR REPLACE INTO sync_peers (endpoint_id, label, addrs, added_at)
                VALUES (?1, 'b', ?2, '2026-10-09T00:00:00Z')";
    let sql = rusqlite::Connection::open(c.db()).unwrap();
    sql.execute(told, [b.id(), a.query(&at)]).unwrap();
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    c.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    (a, b, c)
}

/// With the inviter gone for good, the member that has not heard of the
/// newer one still syncs with it: asked, the newer one shows the chain's
/// records, which make it a member. The session then brings every op the
/// older lacks — the inviter's entry, the admission past it — and both
/// verify.
#[test]
fn a_member_only_its_inviter_heard_of_shows_its_admission_and_syncs() {
    let (a, b, c) = admitted_unheard(&[]);
    let heard = |of: &Copy| b.ok(&["sync", "chain", "list"]).contains(&of.id()[..10]);
    assert!(!heard(&c));
    b.ok(&["occur", "from-b"]);
    c.ok(&["occur", "from-c"]);
    let mut b_runs = b.serve(&["sync", "run"]);
    let out = c.fails(&["sync"], "");
    assert!(out.contains("verify ok"), "{out}");
    assert!(out.contains("1 of 2 peer(s) failed"), "a is gone: {out}");
    let served = b_runs.lines_until(" got ");
    assert!(served.contains("verify ok"), "{served}");
    assert!(heard(&c) && b.has("from-a") && b.has("from-c") && c.has("from-b"));
    assert!(!a.has("from-c"));
    assert_eq!(b.hash(), c.hash());
}

/// A reader shows the same records and stays refused: it brings none of the
/// inviter's ops, so the copy that has not heard of it could neither keep
/// its admission nor learn its filter. The refusal names both copies and
/// says what would fix it, on both ends, and nothing of the reader's is kept.
#[test]
fn a_reader_only_its_inviter_heard_of_is_refused_with_what_would_fix_it() {
    let (_a, b, c) = admitted_unheard(&["--reader"]);
    let mut b_runs = b.cmd(&["sync", "run"]);
    let mut b_runs = Serving::of(b_runs.stderr(Stdio::null()));
    let out = c.fails(&["sync"], "");
    let (me, it) = (member_label(&b, &b.id()), member_label(&c, &c.id()));
    let fix = format!(
        "refused: not a member of this sync chain as {me} holds it: if {it} has joined, sync \
         {me} with the copy that invited it"
    );
    assert!(out.contains(&fix), "{out}");
    assert!(!out.contains("verify"), "{out}");
    drop(b_runs.child.kill());
    assert_eq!(b_runs.finish().1, "", "b served no session");
    let kept = format!("SELECT COUNT(*) FROM ops WHERE endpoint = '{}'", c.id());
    assert_eq!(b.query(&kept), "0");
}
