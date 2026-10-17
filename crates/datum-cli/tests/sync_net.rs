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
/// «timed out» alone said neither which side nor what to do. Bare `sync`
/// says it offline; `-v` says the words (da-a4fxr).
#[test]
fn a_member_not_running_is_named_with_what_runs_it() {
    let (_a, mut b) = paired();
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "2")];
    let said = b.fails(&["sync", "-v"], "");
    for words in [
        "could not reach",
        "is the other copy running?",
        "`datum sync daemon status`",
        "a tab is open on the database",
    ] {
        assert!(said.contains(words), "{words}: {said}");
    }
}

/// A member that never answers reads `offline` in `sync status`, with when
/// it was last seen and not the words, which are the same each time; one
/// that answers and then refuses reads `error`, with its words (da-ueb4p).
#[test]
fn a_member_that_never_answers_is_offline_and_one_that_refuses_is_an_error() {
    let (a, mut b) = paired();
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "2")];
    let line = |b: &Copy| {
        let status = b.ok(&["sync", "status"]);
        let id = a.id();
        let line = status.lines().find(|l| l.contains(&id[..10]));
        line.unwrap_or_else(|| panic!("{status}")).to_string()
    };
    b.fails(&["sync"], "");
    let row = entry_of(&b, "status", &a);
    assert_eq!(row["state"], "offline", "{row}");
    let error = row["error"].as_str().unwrap_or_default();
    assert!(error.contains("could not reach"), "{row}");
    assert!(row["error_at"].is_string(), "{row}");
    let seen = row["last_seen"].as_str().unwrap();
    let said = line(&b);
    assert!(said.contains(" offline ") && said.contains(seen), "{said}");
    assert!(!said.contains("could not reach"), "{said}");

    a.ok(&["sync", "chain", "revoke", &b.id()]);
    let _a_runs = a.serve(&["sync", "run"]);
    b.fails(&["sync"], "");
    let row = entry_of(&b, "status", &a);
    assert_eq!(row["state"], "error", "{row}");
    let error = row["error"].as_str().unwrap();
    assert!(error.contains("refused"), "{row}");
    let said = line(&b);
    assert!(said.contains(" error ") && said.contains(error), "{said}");
}

/// `datum sync` names whom it syncs with before it dials, says each member
/// as its session ends — the one up before the other's timeout — and one
/// it never reached as offline, with when it was last seen; `-v` adds the
/// route, the time and that one's words (da-a4fxr).
#[test]
fn sync_names_its_members_first_and_each_as_it_ends() {
    let (mut a, b) = paired();
    let c = Copy::new();
    let (mut inviting, ticket) = a.invite(&[]);
    c.ok(&["sync", "chain", "connect", &ticket]);
    assert!(inviting.finish().0);
    let (up, down) = (member_label(&a, &b.id()), member_label(&a, &c.id()));
    let _b_runs = b.serve(&["sync", "run"]);
    a.env.push(("DATUM_SYNC_TIMEOUT_S", "3"));

    let sync = |args: &[&str]| {
        let mut run = Killed(a.cmd(args).spawn().unwrap());
        let out = lines(run.0.stdout.take().unwrap());
        let said: Vec<(String, Instant)> = out.iter().map(|l| (l, Instant::now())).collect();
        assert!(!run.0.wait().unwrap().success(), "{said:?}");
        said
    };
    let said = sync(&["sync"]);
    let text: Vec<&str> = said.iter().map(|(l, _)| l.as_str()).collect();
    let first = text[0];
    assert!(first.starts_with("syncing with "), "{text:?}");
    assert!(first.contains(&up) && first.contains(&down), "{text:?}");
    let at = |text: &[&str], who: &str| {
        // A session's own line names the peer as its handshake did.
        let at = text.iter().position(|l| l.starts_with(who));
        at.unwrap_or_else(|| panic!("no line of {who}: {text:?}"))
    };
    let (i, j) = (at(&text, &up), at(&text, &down));
    assert!(text[i].contains(": sent "), "{text:?}");
    let gap = said[j].1 - said[i].1;
    assert!(
        gap > Duration::from_secs(1),
        "{up} said only {gap:?} before {down}: {text:?}"
    );
    let seen = entry_of(&a, "status", &c)["last_seen"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        text[j],
        format!("{down}: offline, last seen {seen}"),
        "{text:?}"
    );
    assert!(
        text.last().unwrap().contains("synced"),
        "the summary last: {text:?}"
    );

    let said = sync(&["sync", "-v"]);
    let text: Vec<&str> = said.iter().map(|(l, _)| l.as_str()).collect();
    let (i, j) = (at(&text, &up), at(&text, &down));
    // `Copy::cmd` binds every copy to 127.0.0.1 (`DATUM_SYNC_BIND`).
    let route = text[i + 1];
    assert!(
        route.starts_with("  direct 127.0.0.1:") && route.ends_with('s'),
        "{text:?}"
    );
    let why = text[j + 1];
    assert!(why.contains("s · could not reach"), "{text:?}");
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
/// here, the copy of this key, refused before it dials: the database's own
/// invite holds it (da-apd52). In the inviter's chain, it would rejoin
/// (da-j8c4l); the handshake's own refusal of the key is `sync_chain`'s,
/// where a hello can be forced.
#[test]
fn a_copy_of_this_key_is_refused() {
    let (a, _b) = paired();
    let (_inviting, ticket) = a.invite(&[]);
    let mut own = a.cmd(&["sync", "chain", "connect", &ticket]);
    let (port, _claim) = claim_port();
    own.env("DATUM_SYNC_BIND", format!("127.0.0.1:{port}"));
    let out = own.output().unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("`invite` is at work on this database"),
        "{text}"
    );
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

    let purged = b.ok(&["db", "purge", "--leave", "-y"]);
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

/// `db restore --hard --leave` takes the ops and the chain with the rows,
/// and plain `--hard` refuses a chained copy (da-95y4o):
/// ops kept for a row it dropped fold that row back on every peer, and the
/// two never verify. The copy is a new one, in no chain, and a fresh invite
/// brings it back: both verify ok, and the peer's data comes back to it.
#[test]
fn a_hard_restore_leaves_the_chain_and_rejoins_by_invite() {
    let (a, b) = paired();
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["occur", "tea"]);
    let backup = b.tmp.path().join("b.jsonl");
    b.ok(&["db", "backup", backup.to_str().unwrap()]);
    b.ok(&["occur", "juice"]);
    b.ok(&["sync"]);
    assert!(a.has("juice"));
    let (old_id, old_chain) = (b.id(), b.query(CHAIN));

    let backup = backup.to_str().unwrap();
    // Without --leave it is refused: the chain would keep what it drops.
    let ops = b.query("SELECT COUNT(*) FROM ops");
    let refused = b.fails(&["db", "restore", backup, "--hard", "--yes"], "");
    assert!(
        refused.contains(&format!("in sync chain {old_chain}")) && refused.contains("--leave"),
        "{refused}"
    );
    assert!(b.has("juice"));
    assert_eq!(
        (b.query("SELECT COUNT(*) FROM ops"), b.id()),
        (ops, old_id.clone())
    );

    let restored = b.ok(&["db", "restore", backup, "--hard", "--leave", "--yes"]);
    assert!(
        restored.contains(&format!("left sync chain {old_chain}")),
        "{restored}"
    );
    assert!(!b.has("juice"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
    let list = b.fails(&["sync", "chain", "list"], "");
    assert!(list.contains("in no sync chain"), "{list}");
    assert_ne!(b.id(), old_id);

    drop(_a_runs);
    let (mut inviting, ticket) = a.invite(&[]);
    assert!(b
        .ok(&["sync", "chain", "connect", &ticket])
        .contains("verify ok"));
    assert!(inviting.finish().0);
    assert_eq!(b.query(CHAIN), old_chain);
    assert!(b.has("juice"));
    let _a_runs = a.serve(&["sync", "run"]);
    assert!(b.ok(&["sync"]).contains("verify ok"));
    for x in [&a, &b] {
        x.ok(&["check", "sync"]);
    }
}

/// A `--hard --leave` no member can be told of is refused whole, naming `sync
/// chain leave`, whose `--force` tells no one; then it goes through
/// (da-95y4o). Restore's own `--force` is its conflict strategy, and
/// `--hard` refuses it.
#[test]
fn a_hard_restore_no_member_hears_of_is_refused_and_names_chain_leave() {
    let (a, b) = paired();
    b.ok(&["occur", "tea"]);
    let backup = b.tmp.path().join("b.jsonl");
    let backup = backup.to_str().unwrap();
    b.ok(&["db", "backup", backup]);
    b.ok(&["occur", "juice"]);
    drop(a);

    let refused = b.fails(&["db", "restore", backup, "--hard", "--leave", "--yes"], "");
    assert!(
        refused.contains("`datum sync chain leave` again") && refused.contains("`--force`"),
        "{refused}"
    );
    assert!(b.has("juice"));
    b.ok(&["sync", "chain", "leave", "--force"]);
    b.ok(&["db", "restore", backup, "--hard", "--yes"]);
    assert!(!b.has("juice"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
}

const CHAIN: &str =
    "SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1";

/// `db purge --rejoin` over iroh (da-qjxfx): the purge's own first sync
/// refetches everything from the member that serves, the copy keeps its
/// endpoint and its chain, and the next `datum sync` verifies with each.
#[test]
fn a_purge_with_rejoin_refetches_from_the_chain_and_verifies() {
    let (a, b) = paired();
    let c = Copy::new();
    let (mut inviting, ticket) = a.invite(&[]);
    c.ok(&["sync", "chain", "connect", &ticket]);
    assert!(inviting.finish().0);
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "coffee"]);
    // c is down: each sync fails it, and a's answer is enough.
    let _a_runs = a.serve(&["sync", "run"]);
    b.run(&["sync"]);
    c.run(&["sync"]);
    assert!(c.has("coffee"));
    let (id, chain) = (b.id(), b.query(CHAIN));

    let purged = b.ok(&["db", "purge", "--rejoin", "-y"]);
    assert!(
        purged.contains("verify ok") && !purged.contains("not refetched"),
        "{purged}"
    );
    assert!(b.has("tea") && b.has("coffee"));
    assert_eq!((b.id(), b.query(CHAIN)), (id, chain));
    b.ok(&["occur", "walk"]);
    let _c_runs = c.serve(&["sync", "run"]);
    let synced = b.ok(&["sync"]);
    assert_eq!(synced.matches("verify ok").count(), 2, "{synced}");
    for x in [&a, &b, &c] {
        assert!(x.has("walk"));
        x.ok(&["check", "sync"]);
    }
}

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

    b.ok(&["db", "purge", "--leave", "--force", "-y"]);
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

/// A purge that lands between an open session's rounds leaves the file as
/// empty as it left it (da-otz0d): the session stops at its next round, where
/// it took the peer's whole chain — genesis first, as a copy in no chain
/// does — and wrote the peer's row back. `b` applies slowly, so `a`, which
/// has applied its first round, waits a round on it: the purge lands there.
#[test]
fn a_purge_between_an_open_sessions_rounds_leaves_the_file_empty() {
    let (a, mut b) = paired();
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "juice"]);
    b.env = vec![("DATUM_SYNC_TEST_APPLY_DELAY_MS", "2000")];
    let _b_runs = b.serve(&["sync", "run"]);
    let a_syncs = a.cmd(&["sync"]).spawn().unwrap();
    eventually("a's first round", || a.has("juice"));
    a.ok(&["db", "purge", "--leave", "--force", "-y"]);

    let out = a_syncs.wait_with_output().unwrap();
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success() && said.contains("endpoint key changed"),
        "{said}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for table in ["ops", "entries", "sync_peers", "sync_local"] {
        let rows = a.query(&format!("SELECT COUNT(*) FROM {table}"));
        assert_eq!(rows, "0", "{table}");
    }
}

/// Stamp `copy`'s file one schema on, as a newer datum's migration leaves
/// it. What an older build then says of the two versions.
fn migrate_under(copy: &Copy) -> String {
    let now = CURRENT_SCHEMA_VERSION + 1;
    let stamped = rusqlite::Connection::open(copy.db())
        .unwrap()
        .execute(
            "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
            [now.to_string()],
        )
        .unwrap();
    assert_eq!(stamped, 1);
    format!("at schema v{now} and this datum writes v{CURRENT_SCHEMA_VERSION}")
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

/// A newer datum's migration that lands between an open session's rounds
/// stops the session, which names both versions and writes nothing more
/// into the file (da-1tnup): its rows would be the old shape. `b` applies
/// slowly, so `a` waits a round on it, as for the purge above.
#[test]
fn a_migration_between_an_open_sessions_rounds_stops_it_and_it_writes_nothing_more() {
    let (a, mut b) = paired();
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "juice"]);
    b.env = vec![("DATUM_SYNC_TEST_APPLY_DELAY_MS", "2000")];
    let _b_runs = b.serve(&["sync", "run"]);
    let a_syncs = a.cmd(&["sync"]).spawn().unwrap();
    eventually("a's first round", || a.has("juice"));
    let versions = migrate_under(&a);
    let before = held(&a);

    let out = a_syncs.wait_with_output().unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success() && said.contains(&versions), "{said}");
    assert_eq!(held(&a), before);
}

/// `sync run` and the daemon over it stop, failed and naming both versions,
/// once a newer datum has migrated the file (da-1tnup): the unit then starts
/// the installed binary. Neither writes into it again.
#[test]
fn a_migration_under_sync_run_or_the_daemon_stops_it() {
    for daemon in [false, true] {
        let (a, b) = paired();
        let config = b.tmp.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        let registry = format!("[db.data]\npath = {:?}\n", b.db().display().to_string());
        std::fs::write(config.join("dbs.toml"), registry).unwrap();
        let args: &[&str] = if daemon {
            &["sync", "daemon", "run"]
        } else {
            &["sync", "run"]
        };
        let mut b_runs = b.serve(args);
        a.ok(&["occur", "tea"]);
        eventually("a's sync reaching b", || {
            a.run(&["sync"]).status.success() && b.has("tea")
        });

        let versions = migrate_under(&b);
        let before = held(&b);
        let start = Instant::now();
        while b_runs.child.try_wait().unwrap().is_none() {
            assert!(start.elapsed() < Duration::from_secs(10), "still running");
            std::thread::sleep(Duration::from_millis(100));
        }
        let (ok, _) = b_runs.finish();
        let mut why = String::new();
        let mut stderr = b_runs.child.stderr.take().unwrap();
        stderr.read_to_string(&mut why).unwrap();
        assert!(!ok && why.contains(&versions), "daemon {daemon}: {why}");
        assert_eq!(held(&b), before, "daemon {daemon}");
    }
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

    let purged = b.ok(&["db", "purge", "--leave", "--force", "-y"]);
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

/// `datum sync` on a copy a `sync run` serves asks the run to sync
/// (da-tltjj): it binds nothing of its own — here the copy's one port, which
/// the run holds, so a bind would fail — and prints each member's session as
/// the run's, failing when one did. With the run killed, its socket left
/// beside the database, it dials as it always did.
#[test]
fn sync_asks_the_run_serving_its_copy_and_binds_nothing_of_its_own() {
    let (a, mut b) = paired();
    b.env.push(("DATUM_SYNC_TIMEOUT_S", "2"));
    let mut a_runs = a.serve(&["sync", "run"]);
    let b_runs = b.serve(&["sync", "run"]);
    let socket = PathBuf::from(format!("{}.sync", b.db().display()));
    eventually("b's run taking asks", || socket.exists());

    b.ok(&["occur", "walk"]);
    let out = b.run(&["sync"]);
    let (said, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{said}{err}");
    assert!(
        err.contains("this copy's sync daemon is syncing it"),
        "{err}"
    );
    assert!(said.contains("verify ok"), "{said}");
    assert!(a.has("walk"));

    // A member it cannot reach fails it, named offline, its words in `-v`'s
    // line, which the run sends too (da-a4fxr).
    a_runs.child.kill().unwrap();
    a_runs.child.wait().unwrap();
    let said = b.fails(&["sync", "-v"], "");
    assert!(
        said.contains("this copy's sync daemon is syncing it"),
        "{said}"
    );
    assert!(said.contains(": offline, last seen "), "{said}");
    assert!(said.contains("s · could not reach"), "{said}");
    assert!(said.contains("1 of 1 peer(s) failed"), "{said}");

    let _a_runs = a.serve(&["sync", "run"]);
    // Killed and waited for (`Serving`'s drop): SIGKILL runs no destructor,
    // so the socket stays, as a crashed daemon's would.
    drop(b_runs);
    assert!(socket.exists(), "a killed run leaves its socket");
    b.ok(&["occur", "swim"]);
    let out = b.run(&["sync"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("sync daemon"), "{err}");
    assert!(a.has("swim"));
}

/// An answer cut short — the run stopped with a session open — fails the
/// ask rather than reads as done, and one this build cannot read says so.
#[test]
fn an_answer_cut_short_or_unreadable_fails_the_ask() {
    let (_a, b) = paired();
    let socket = PathBuf::from(format!("{}.sync", b.db().display()));
    for (answer, says) in [
        (
            "2\n{\"ok\":true,\"line\":\"a: verify ok\"}\n",
            "stopped before it finished",
        ),
        ("two\n", "cannot read"),
        ("1\n{\"ok\":true}\n", "cannot read"),
    ] {
        let run = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let answering = std::thread::spawn(move || {
            run.accept()
                .unwrap()
                .0
                .write_all(answer.as_bytes())
                .unwrap();
        });
        let said = b.fails(&["sync"], "");
        answering.join().unwrap();
        assert!(said.contains(says), "{answer:?}: {said}");
        std::fs::remove_file(&socket).unwrap();
    }
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

/// `sync run` is the one that gives way (da-lk1s2): with a write holding its
/// file, it fails a session within its short busy timeout, where any other
/// connection waits out 5 s, and tries every peer again at its next tick
/// rather than after a failed push's backoff — quietly: the peer it refused
/// records no error (da-h31wt).
#[test]
fn run_gives_way_to_a_write_holding_its_file_and_tries_again_at_once() {
    let (a, b) = paired();
    let mut a_runs = a.serve(&["sync", "run"]);
    let (tx, rx) = std::sync::mpsc::channel();
    let err = BufReader::new(a_runs.child.stderr.take().unwrap());
    std::thread::spawn(move || {
        err.lines()
            .map_while(Result::ok)
            .try_for_each(|l| tx.send(l))
    });
    let until = |needle: &str| loop {
        let line = rx.recv_timeout(Duration::from_secs(10)).expect(needle);
        if line.contains(needle) {
            break;
        }
    };
    // Up before b first pushes, or that push is unreachable and waits a minute.
    until("is up");
    let _b_runs = b.serve(&["sync", "run"]);
    let write = rusqlite::Connection::open(a.db()).unwrap();
    write.execute_batch("BEGIN IMMEDIATE").unwrap();

    b.ok(&["occur", "walk"]);
    let start = Instant::now();
    until("gave way to a write");
    let took = start.elapsed();
    assert!(took < Duration::from_secs(4), "a gave way after {took:?}");
    assert!(!a.has("walk"));

    // Giving way failed nobody: b, refused while a's file is held, does
    // not say so in its status (da-h31wt).
    std::thread::sleep(Duration::from_millis(500));
    let said = b.query("SELECT COUNT(*) FROM sync_peers WHERE last_error IS NOT NULL");
    assert_eq!(said, "0", "{}", b.ok(&["sync", "status"]));

    write.execute_batch("COMMIT").unwrap();
    eventually("b's walk reaching a", || a.has("walk"));
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

/// A member that is down is tried again after a gap that doubles with each
/// failure up to thirty times the first, every failure after the first
/// writes nothing, and the member, up again, is heard and pushed to at once
/// — not at the next try (da-u1ul1). The first gap is 200 ms here, a minute for real.
#[test]
fn an_unreachable_member_is_tried_less_often_and_heard_at_once_when_it_is_back() {
    let (mut a, b) = paired();
    a.env.extend([
        ("DATUM_SYNC_TIMEOUT_S", "1"),
        ("DATUM_SYNC_TEST_RETRY_MS", "200"),
    ]);
    let mut a_runs = Killed(a.cmd(&["sync", "run"]).spawn().unwrap());
    let (err, out) = (
        lines(a_runs.0.stderr.take().unwrap()),
        lines(a_runs.0.stdout.take().unwrap()),
    );
    let mut gaps = Vec::new();
    let mut first = None;
    while gaps.len() < 6 {
        let line = err
            .recv_timeout(Duration::from_secs(10))
            .expect("a failure");
        if let Some((_, gap)) = line.split_once("; next try in ") {
            gaps.push(gap.to_string());
            first.get_or_insert_with(|| held(&a));
        }
    }
    assert_eq!(gaps, ["200ms", "400ms", "800ms", "1.6s", "3.2s", "6s"]);
    assert_eq!(Some(held(&a)), first, "a failure alike wrote");

    let start = Instant::now();
    let _b_runs = b.serve(&["sync", "run"]);
    let heard = out
        .recv_timeout(Duration::from_secs(10))
        .expect("b's session");
    assert!(heard.contains(": sent "), "{heard}");
    // And pushes to it again at once, its gap of 6 s forgotten.
    a.ok(&["occur", "tea"]);
    eventually("a's write reaching b", || b.has("tea"));
    let took = start.elapsed();
    assert!(took < Duration::from_secs(4), "a reached b after {took:?}");
}

/// `sync run` logs a hold of its write lock past `DATUM_SYNC_LONG_HOLD_MS`
/// (a second unless set) with what it wrote (da-hyem1): here every hold,
/// and the apply of b's walk among them.
#[test]
fn run_logs_its_long_holds_of_the_write_lock_with_what_they_wrote() {
    let (a, b) = paired();
    let mut a_runs = Serving::of(a.cmd(&["sync", "run"]).env("DATUM_SYNC_LONG_HOLD_MS", "0"));
    let err = BufReader::new(a_runs.child.stderr.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        err.lines()
            .map_while(Result::ok)
            .try_for_each(|l| tx.send(l))
    });
    let until = |needle: &str| loop {
        let line = rx.recv_timeout(Duration::from_secs(10)).expect(needle);
        if line.contains(needle) {
            break line;
        }
    };
    until("is up");
    b.ok(&["occur", "walk"]);
    b.ok(&["sync"]);
    let held = until("entries");
    assert!(held.contains(" held the write lock 0."), "{held}");
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
    // As a names b when it warns: b's describe, which can grow it, arrives after.
    let label = member_label(&a, &b.id());
    let first = a.run(&["sync"]);
    let (out, err) = (
        String::from_utf8(first.stdout).unwrap(),
        String::from_utf8(first.stderr).unwrap(),
    );
    assert!(out.contains("verify ok"), "{out}{err}");
    assert!(b.has("walk"));
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

/// `invite --label` names the copy that starts the chain, `connect --label`
/// the one that joins, and every member lists both; an invite from a copy
/// in the chain renames it (da-ey3hz).
#[test]
fn invite_and_connect_name_their_copies_by_label() {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&["--label", "laptop"]);
    b.ok(&["sync", "chain", "connect", "--label", "phone", &ticket]);
    assert!(inviting.finish().0);
    for copy in [&a, &b] {
        assert_eq!(member_label(copy, &a.id()), "laptop");
        assert_eq!(member_label(copy, &b.id()), "phone");
    }
    // In a chain it is in, the invite once signed renames it.
    let _inviting = b.invite(&["--label", "desk"]);
    assert_eq!(member_label(&b, &b.id()), "desk");
}

/// A label the chain refuses is refused at the flag, saying why, before
/// a chain is started or an invite redeemed.
#[test]
fn a_label_the_chain_refuses_is_refused_at_the_flag() {
    let a = Copy::new();
    for cmd in [&["invite"][..], &["connect", "ticket"]] {
        let argv = [&["sync", "chain"][..], cmd, &["--label", "a\u{1b}[2J"]].concat();
        let err = a.fails(&argv, "");
        assert!(
            err.contains("a label cannot contain a control character"),
            "{err}"
        );
        let long = "x".repeat(65);
        let err = a.fails(
            &[&["sync", "chain"][..], cmd, &["--label", &long]].concat(),
            "",
        );
        assert!(err.contains("a label is at most 64 characters"), "{err}");
    }
}

/// Unnamed, a copy's label is assembled from what it runs on (da-xijlw),
/// alike on every copy: beside a second user on its host, `user@` before
/// it and the database's name after it (`sync_chain.rs` has a copy alone,
/// its host); ann's second
/// database tells her two copies apart by that name. Every copy here runs
/// on this one host; members on different hosts (a laptop and a phone) are
/// tested in chain.rs, `a_label_is_the_least_that_tells_the_chains_members_apart`.
#[test]
fn an_unset_label_is_assembled_alike_on_every_copy() {
    let a = Copy::of("ann", "data.db");
    let join = |copy: &Copy| {
        let (mut inviting, ticket) = a.invite(&[]);
        copy.ok(&["sync", "chain", "connect", &ticket]);
        assert!(inviting.finish().0);
    };
    let (bob, work) = (Copy::of("bob", "data.db"), Copy::of("ann", "work.db"));
    join(&bob);
    let list = a.ok(&["sync", "chain", "list", "--json"]);
    let host = member_of_list(&list, &a.id())["host"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(member_label(&bob, &bob.id()), format!("bob@{host} (data)"));
    join(&work);
    {
        // Each describe reaches every copy.
        let _a_runs = a.serve(&["sync", "run"]);
        // A bare sync dials every member, and only a runs.
        for copy in [&bob, &work, &bob] {
            copy.run(&["sync"]);
        }
    }
    for copy in [&a, &bob, &work] {
        let labels = [&a, &bob, &work].map(|c| member_label(copy, &c.id()));
        let want = [
            "ann@{host} (data)",
            "bob@{host} (data)",
            "ann@{host} (work)",
        ];
        assert_eq!(labels, want.map(|w| w.replace("{host}", &host)));
    }
}

#[test]
fn a_new_member_is_announced_once() {
    let (a, b) = paired();
    // The inviter is told a device joined, and what revokes it; the joiner
    // only who was in the chain it joined, plainly (da-o9xdd).
    let told = [
        "⚠ another device joined: ",
        "you joined this sync chain; its members: ",
    ];
    for (copy, told) in [(&a, told[0]), (&b, told[1])] {
        let first = String::from_utf8(copy.run(&["tail"]).stderr).unwrap();
        assert!(first.starts_with(told), "{first}");
        assert_eq!(first.lines().count(), 1, "{first}");
        let second = String::from_utf8(copy.run(&["tail"]).stderr).unwrap();
        assert!(!second.contains(told), "{second}");
    }
    let first = a.ok(&["sync", "status"]);
    assert!(
        first.contains("not yours? `datum sync chain revoke "),
        "{first}"
    );
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

/// A first sync whose verify outlasts the progress timeout (da-0mwws): the
/// joiner's or the inviter's hash holds its thread for 6s on a 3s timeout,
/// and the peer, whose own is done, hears a `BUSY` every 0.75s unless
/// `keepalive` is off. Whether the join and the invite succeeded, and what
/// each printed on stdout and stderr.
fn slow_verify(joiner: bool, keepalive: bool) -> ((bool, String), (bool, String)) {
    let (mut a, mut b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea"]);
    for c in [&mut a, &mut b] {
        c.env = vec![
            ("DATUM_SYNC_TIMEOUT_S", "3"),
            (
                "DATUM_SYNC_TEST_KEEPALIVE",
                if keepalive { "1" } else { "0" },
            ),
        ];
    }
    let slow = if joiner { &mut b } else { &mut a };
    slow.env.push(("DATUM_SYNC_TEST_VERIFY_DELAY_S", "6"));
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.run(&["sync", "chain", "connect", &ticket]);
    let (served_ok, mut served) = inviting.finish();
    let err = inviting.child.stderr.as_mut().unwrap();
    err.read_to_string(&mut served).unwrap();
    let joined_text = format!(
        "{}{}",
        String::from_utf8_lossy(&joined.stdout),
        String::from_utf8_lossy(&joined.stderr)
    );
    ((joined.status.success(), joined_text), (served_ok, served))
}

#[test]
fn a_verify_longer_than_the_timeout_completes_on_keepalives_and_says_so() {
    for joiner in [true, false] {
        let ((ok, joined), (served_ok, served)) = slow_verify(joiner, true);
        assert!(ok && joined.contains("verify ok"), "{joined}");
        assert!(served_ok && served.contains("verify ok"), "{served}");
        let slow = if joiner { &joined } else { &served };
        assert!(
            slow.contains(": verifying that both copies agree ("),
            "{slow}"
        );
    }
}

#[test]
fn without_keepalives_a_slow_verify_times_out() {
    let ((ok, joined), (_, served)) = slow_verify(true, false);
    assert!(!ok && !joined.contains("verify ok"), "{joined}");
    assert!(served.contains("nothing moved for 3s"), "{served}");
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

/// A slow copy catching up from three members at once (da-h31wt): vividn's,
/// after a purge, lost every session but one to «connection lost: timed
/// out» or «database is locked». x applies at 20 ms an op, on a 3 s
/// timeout, while three members running `sync run` each bring it 100
/// entries, and a command is typed on it meanwhile: all four converge, and
/// none of them says a session failed.
#[test]
fn a_slow_copy_catches_up_from_three_members_at_once() {
    let mut x = Copy::new();
    let mut peers: Vec<Copy> = (0..3).map(|_| Copy::new()).collect();
    for p in &peers {
        let (mut inviting, ticket) = x.invite(&[]);
        p.ok(&["sync", "chain", "connect", &ticket]);
        assert!(inviting.finish().0);
    }
    for (i, p) in peers.iter().enumerate() {
        for n in 0..100 {
            p.ok(&["occur", &format!("p{i}"), &format!("n={n}")]);
        }
    }
    let said = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut runs = Vec::new();
    let mut up = |c: &Copy| {
        let mut child = Killed(c.cmd(&["sync", "run"]).spawn().unwrap());
        for rx in [
            lines(child.0.stdout.take().unwrap()),
            lines(child.0.stderr.take().unwrap()),
        ] {
            let (said, who) = (said.clone(), c.env[0].1);
            std::thread::spawn(move || {
                rx.iter()
                    .for_each(|l| said.lock().unwrap().push(format!("{who}: {l}")))
            });
        }
        runs.push(child);
    };
    let timeout = ("DATUM_SYNC_TIMEOUT_S", "3");
    x.env
        .extend([timeout, ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "20")]);
    up(&x);
    for p in &mut peers {
        p.env.extend([timeout, ("DATUM_SYNC_TEST_RETRY_MS", "200")]);
        up(p);
    }
    let count = || x.query("SELECT COUNT(*) FROM entries WHERE field LIKE 'p_'");
    let start = Instant::now();
    while count() == "0" {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "x applied nothing"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    x.ok(&["occur", "typed"]);
    assert_ne!(count(), "300", "the command was typed after the catch-up");
    while !peers.iter().all(|p| p.has("typed") && p.hash() == x.hash()) {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "converged: {:#?}",
            said.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    drop(runs);
    let said = said.lock().unwrap();
    let failed: Vec<&String> = said
        .iter()
        .filter(|l| {
            ["locked", "timed out", "connection lost", "failed"]
                .iter()
                .any(|w| l.contains(w))
        })
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}

// ─── A big catch-up, a round at a time (da-ymgiv) ───────────────────────────

/// The ops a round carries at most: the wire crate's `ROUND`.
const ROUND: usize = 8192;

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

/// The ops a copy's log says it sent `to` (a label, or part of one): «sent
/// N» of each session it finished, and «and sent N» of each one lost.
fn sent_in(log: &str, to: &str) -> usize {
    let mut n = 0;
    for line in log.lines().filter(|l| l.contains(to)) {
        for (i, _) in line.match_indices("sent ") {
            let before = &line[..i];
            if before.ends_with(": ") || before.ends_with("and ") {
                let digits: String = line[i + 5..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                n += digits.parse::<usize>().unwrap_or(0);
            }
        }
    }
    n
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

fn ops(copy: &Copy) -> usize {
    copy.query("SELECT COUNT(*) FROM ops WHERE applied > 0")
        .parse()
        .unwrap()
}

fn until(what: &str, limit: Duration, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < limit, "{what} within {limit:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// vividn's purge and re-import on a chain of four (da-ymgiv): each copy runs
/// `sync run`, 20,000 entries are imported on a, and d, a slow computer,
/// applies at 1ms an op and loses its sessions every 15s, its `sync run`
/// killed and started again. A round carries at most 1,000 ops here and
/// applies before the next, so a lost session wastes at most a round from
/// each sender, where it wasted all it had been sent and not yet applied
/// (staging: 80,535 sent for 20,003, 7 sessions lost); and no copy dials a
/// peer it is in a session with. Measured: 21,008 sent, 9 lost.
#[test]
fn a_big_import_reaches_a_slow_copy_without_sending_it_many_times() {
    let (mut a, mut b, mut c, mut d) = (Copy::new(), Copy::new(), Copy::new(), Copy::new());
    for joiner in [&b, &c, &d] {
        let (mut inviting, ticket) = a.invite(&[]);
        joiner.ok(&["sync", "chain", "connect", &ticket]);
        assert!(inviting.finish().0);
    }
    for copy in [&mut a, &mut b, &mut c, &mut d] {
        copy.env.push(("DATUM_SYNC_TEST_ROUND", "1000"));
    }
    d.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1"));
    let runs: Vec<Logged> = [&a, &b, &c]
        .into_iter()
        .map(|copy| logged(copy, &["sync", "run"], "run.log"))
        .collect();
    let mut slow = logged(&d, &["sync", "run"], "run.log");
    // Each has dialed the others it starts with.
    until("the copies settle", Duration::from_secs(60), || {
        [&a, &b, &c, &d]
            .iter()
            .all(|c| log(c, "run.log").matches("verify ok").count() >= 3)
    });
    let before = ops(&d);
    import(&a, 20_000);
    // A write moves a's vector, and a pushes: its serve signs the baselines.
    a.ok(&["occur", "tea"]);
    until("a signs its baselines", Duration::from_secs(60), || {
        ops(&a) > 20_000
    });
    let all = ops(&a);
    let (start, mut lost) = (Instant::now(), 0);
    while ops(&d) < all {
        assert!(
            start.elapsed() < Duration::from_secs(600),
            "d holds every op within 600s: {} of {all}",
            ops(&d)
        );
        if start.elapsed() > Duration::from_secs(15 * (lost + 1)) {
            drop(slow);
            slow = logged(&d, &["sync", "run"], "run.log");
            lost += 1;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let needed = all - before;
    // What a, b and c say they sent d, once their sessions with it end.
    let to = format!("u{}@", d.port);
    let sent = || -> Vec<usize> {
        [&a, &b, &c]
            .iter()
            .map(|c| sent_in(&log(c, "run.log"), &to))
            .collect()
    };
    let mut seen = sent();
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let now = sent();
        if now == seen && now.iter().sum::<usize>() >= needed {
            break;
        }
        seen = now;
    }
    let total: usize = seen.iter().sum();
    eprintln!(
        "d lacked {needed} ops, lost its sessions {lost} times and was sent {total} (a {}, b {}, c {})",
        seen[0], seen[1], seen[2]
    );
    assert!(lost > 0, "d's sessions were never cut");
    // At most a round from each of a, b and c is lost with each session,
    // and one more at the end: staging's 80,535 is well over it.
    let bound = needed + (lost as usize + 1) * 3 * 1000;
    assert!(
        total <= bound,
        "d lacked {needed} ops and was sent {total}, over {bound}"
    );
    // And every copy converges.
    let start = Instant::now();
    for (n, copy) in [("b", &b), ("c", &c)] {
        while ops(copy) < all {
            if start.elapsed() > Duration::from_secs(180) {
                eprintln!("{n}'s log:\n{}", log(copy, "run.log"));
                eprintln!("a's log:\n{}", log(&a, "run.log"));
                panic!("{n} holds every op within 180s: {} of {all}", ops(copy));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    eprintln!(
        "every copy holds every op {}s after d",
        start.elapsed().as_secs()
    );
    drop((runs, slow));
    for copy in [&b, &c, &d] {
        assert_eq!(copy.hash(), a.hash());
    }
}

/// A session cut mid-catch-up (da-ymgiv) loses at most the round it was
/// cut in: b, applying at 1ms an op, is killed once a round has applied, and
/// the next session is sent only what b still lacks.
#[test]
fn a_catch_up_cut_short_resumes_from_the_last_round_that_applied() {
    let (mut a, mut b) = paired();
    import(&a, 20_000);
    // Its baselines signed now: signing 20,000 sends nothing for longer
    // than the 5s below.
    a.ok(&["sync", "pipe", "ops", "--limit", "1"]);
    let before = ops(&b);
    // a hears the cut session is lost within 5s; b, which paces its apply
    // chunks by its own timeout, has the same one, or a can hear nothing
    // from it for longer than that between chunks.
    for copy in [&mut a, &mut b] {
        copy.env.push(("DATUM_SYNC_TIMEOUT_S", "5"));
    }
    b.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1"));
    let _serving = logged(&a, &["sync", "run"], "run.log");
    let cut = logged(&b, &["sync"], "cut.log");
    until("b applies a round", Duration::from_secs(120), || {
        ops(&b) > before + ROUND
    });
    drop(cut);
    let held = ops(&b);
    // a hears the cut session is lost before b dials again.
    until(
        "a says the session is lost",
        Duration::from_secs(30),
        || log(&a, "run.log").contains("then:"),
    );
    let to = format!("u{}@", b.port);
    let first = sent_in(&log(&a, "run.log"), &to);
    b.env
        .retain(|(k, _)| *k != "DATUM_SYNC_TEST_APPLY_DELAY_MS");
    b.ok(&["sync"]);
    let needed = ops(&b) - before;
    let rest = sent_in(&log(&a, "run.log"), &to) - first;
    eprintln!(
        "b lacked {needed}: the cut session sent {first} and applied {}, the next sent {rest}",
        held - before
    );
    assert_eq!(
        rest,
        needed - (held - before),
        "the next session sends only what b lacks"
    );
    assert!(
        first <= held - before + ROUND,
        "a cut session loses at most a round: sent {first}, applied {}",
        held - before
    );
    assert_eq!(a.hash(), b.hash());
}

/// Two copies that dial each other at once (da-vj3ly): both `sync run`s
/// start together, each holding 2,000 entries the other lacks, and each
/// dials the other as it comes up. The lower endpoint id's session carries
/// both ways and the other copy closes its own, so each op is sent once,
/// where the two sessions sent each twice.
#[test]
fn two_copies_dialing_each_other_at_once_send_each_op_once() {
    let (mut a, mut b) = paired();
    import_as(&a, "step", 2000);
    import_as(&b, "stair", 2000);
    for copy in [&mut a, &mut b] {
        // Its baselines signed now, before the runs start.
        copy.ok(&["sync", "pipe", "ops", "--limit", "1"]);
        // Each session applies for seconds, so the two overlap.
        copy.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1"));
    }
    let lacking = ops(&a) + ops(&b);
    let runs = [&a, &b].map(|copy| logged(copy, &["sync", "run"], "run.log"));
    until("both hold every op", Duration::from_secs(120), || {
        ops(&a) == ops(&b) && ops(&a) * 2 > lacking && a.hash() == b.hash()
    });
    let needed = 2 * ops(&a) - lacking;
    // Each has one peer: every session line is with the other.
    let sent = || sent_in(&log(&a, "run.log"), "") + sent_in(&log(&b, "run.log"), "");
    // Until every session has said what it sent, which it does as it ends:
    // the total unchanged for 6s, past either session's apply of 2,000 ops
    // at 1ms each, so a second session still open is not missed.
    let (start, mut seen, mut still) = (Instant::now(), sent(), 0);
    while still < 3 || seen < needed {
        std::thread::sleep(Duration::from_secs(2));
        let now = sent();
        still = if now == seen { still + 1 } else { 0 };
        seen = now;
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the sessions say they sent {needed} within 60s, not {seen}:\n{}\n{}",
            log(&a, "run.log"),
            log(&b, "run.log")
        );
    }
    drop(runs);
    eprintln!("the copies lacked {needed} ops between them and were sent {seen}");
    // Measured: exactly what was lacked; two sessions send twice that.
    assert!(
        seen <= needed + needed / 10,
        "the copies lacked {needed} ops and were sent {seen}:\n{}\n{}",
        log(&a, "run.log"),
        log(&b, "run.log")
    );
    assert_eq!(a.hash(), b.hash());
}

/// The first serve after a bulk import signs its baselines (da-f7mmn):
/// 10,000 here, about 5s in a debug build, on a 2s timeout. a, dialing
/// b's `sync run`, signs them in chunks of the clock's size with a `BUSY`
/// after each, unless `keepalive` is off; it sent nothing until all were
/// signed, and lost b. The sync's output.
fn first_sync_after_an_import(keepalive: bool) -> (Copy, Copy, Output) {
    let (mut a, mut b) = paired();
    import(&a, 10_000);
    for c in [&mut a, &mut b] {
        c.env = vec![
            ("DATUM_SYNC_TIMEOUT_S", "2"),
            (
                "DATUM_SYNC_TEST_KEEPALIVE",
                if keepalive { "1" } else { "0" },
            ),
        ];
    }
    let _serving = b.serve(&["sync", "run"]);
    let out = a.run(&["sync"]);
    (a, b, out)
}

#[test]
fn the_first_sync_after_an_import_keeps_its_peer_while_it_signs() {
    let (a, b, out) = first_sync_after_an_import(true);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("verify ok"), "{text}");
    assert!(ops(&b) > 10_000);
    assert_eq!(a.hash(), b.hash());
}

#[test]
fn without_keepalives_the_first_sync_after_an_import_loses_its_peer() {
    let (_a, _b, out) = first_sync_after_an_import(false);
    assert!(
        !out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
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
    let line = status
        .lines()
        .find(|l| !l.starts_with('⚠') && l.contains(&c.id()[..10]))
        .unwrap();
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

    // a gone, b's session fails, offline (da-ueb4p), and says why in the
    // words `sync -v` printed (da-a4fxr).
    drop(serving);
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "3")];
    let failed = b.fails(&["sync", "-v"], "");
    let row = entry_of(&b, "status", &a);
    assert_eq!(row["state"], "offline", "{row}");
    let why = row["error"].as_str().unwrap();
    assert!(
        failed.contains(why) && row["error_at"].is_string(),
        "{why} / {failed}"
    );
}

/// A reader ahead of the member it syncs with (da-t3ey2): r holds an entry
/// of a's that b, which knows r, lacks, and a reader never relays, so no
/// session between them can make them equal. Each says who is ahead and by
/// how much, and neither a MISMATCH, in its session or its status; the same
/// ops and different rows are still one.
#[test]
fn a_reader_ahead_of_a_member_says_so_and_the_same_ops_apart_are_a_mismatch() {
    let (a, mut b, r) = admitted_unheard(&["--reader"]);
    // A dial that reaches no one fails the command: what it printed.
    let sync = |c: &Copy| String::from_utf8(c.run(&["sync"]).stdout).unwrap();

    // b hears of r from a; r alone gets a's next entry; a goes.
    let a_runs = a.serve(&["sync", "run"]);
    let heard = sync(&b);
    assert!(heard.contains("verify ok"), "{heard}");
    a.ok(&["occur", "later"]);
    let got = sync(&r);
    assert!(got.contains("verify ok") && r.has("later"), "{got}");
    drop(a_runs);

    // b's run dials r in its turn, and reaches no one: its status would say
    // that, so not while this looks.
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "30")];
    let mut b_runs = b.serve(&["sync", "run"]);
    let dialed = sync(&r);
    let served = b_runs.lines_until(" got ");
    drop(b_runs);
    // As each names the other now: r's describe reached b in that session.
    let (b_label, r_label) = (member_label(&r, &b.id()), member_label(&b, &r.id()));
    b.env = r.env.clone();
    // The entry, and the spec its first log declared.
    let ahead = format!("not verified: this copy is 2 changes ahead of {b_label}");
    assert!(dialed.contains(&ahead), "{dialed}");
    let ahead = format!("not verified: {r_label} is 2 changes ahead of this copy");
    assert!(served.contains(&ahead), "{served}");
    for (x, y) in [(&b, &r), (&r, &b)] {
        let row = entry_of(x, "status", y);
        assert_eq!(
            (row["state"].as_str(), row["error"].as_str()),
            (Some("pending"), None),
            "{row}"
        );
    }

    // b catches up from a, so the two hold the same ops and verify; then r
    // loses a row.
    let a_runs = a.serve(&["sync", "run"]);
    assert!(sync(&b).contains("verify ok") && b.has("later"));
    drop(a_runs);
    let _b_runs = b.serve(&["sync", "run"]);
    let level = sync(&r);
    assert!(level.contains("verify ok"), "{level}");
    assert_eq!(entry_of(&r, "status", &b)["state"], "synced");
    rusqlite::Connection::open(r.db())
        .unwrap()
        .execute("DELETE FROM entries WHERE field = 'later'", [])
        .unwrap();
    let lost = sync(&r);
    assert!(lost.contains("verify MISMATCH"), "{lost}");
    assert_eq!(entry_of(&r, "status", &b)["error"], "verify MISMATCH");
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

/// What a copy holds is more than `count` counts: a flag marker and a meta
/// note are entries a reader could not send either, and `count`, as `tail`,
/// leaves them out (da-lkh1j).
#[test]
fn a_copy_holding_a_flag_and_a_meta_note_is_refused_a_reader_invite() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    let r = Copy::new();
    r.ok(&["flag", "tea", "come back to this"]);
    r.ok(&["meta", "tea", "rate it"]);
    assert_eq!(r.ok(&["count"]).trim(), "0");
    assert_eq!(r.ok(&["sync", "holds"]).trim(), "2");
    let (_inviting, ticket) = a.invite(&["--reader"]);
    let refused = r.fails(&["sync", "chain", "connect", &ticket], "");
    assert!(
        refused.contains("holds 2 entries of its own; a reader writes nothing"),
        "{refused}"
    );
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

/// `chain leave` tells the chain (da-dsob1), a reader's as a member's: the
/// leaver signs its departure and syncs it to the members it reaches, and a
/// member down then hears of it from one of those. Each lists it as left,
/// and no more in its status, which dials it no more; what it logged stays
/// on every copy. Reaching no one, it refuses, and stays.
#[test]
fn a_leave_reaches_every_member_through_those_it_reached() {
    for caps in [&[][..], &["--reader"]] {
        let (a, b, c) = admitted_unheard(caps);
        let member = caps.is_empty();
        if member {
            c.ok(&["occur", "from-c"]);
        }
        let id = c.id();
        let refused = c.fails(&["sync", "chain", "leave"], "");
        assert!(
            refused.contains("no member holds this copy's departure"),
            "{refused}"
        );
        // a up and b down: a holds it, and b hears it from a.
        let a_runs = a.serve(&["sync", "run"]);
        let left = c.ok(&["sync", "chain", "leave"]);
        assert!(left.contains("its departure is with "), "{left}");
        assert!(b.ok(&["sync"]).contains("verify ok"));
        drop(a_runs);
        for x in [&a, &b] {
            let list = x.ok(&["sync", "chain", "list", "--json"]);
            assert!(member_of_list(&list, &id)["left"].is_string(), "{list}");
            let status: serde_json::Value =
                serde_json::from_str(&x.ok(&["sync", "status", "--json"])).unwrap();
            assert!(!status["members"].to_string().contains(&id), "{status}");
            assert_eq!(x.has("from-c"), member);
        }
        assert_eq!(c.has("from-c"), member);
    }
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

/// One admitted by a copy this one holds as revoked shows records it does
/// not read, and stays refused: the refusal names both copies and says what
/// would fix it, on both ends, and nothing of the dialer's is kept.
#[test]
fn one_admitted_by_a_copy_held_as_revoked_is_refused_with_what_would_fix_it() {
    let (a, b, c) = admitted_unheard(&["--reader"]);
    b.ok(&["sync", "chain", "revoke", &a.id()[..10]]);
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

// ─── A reader only its inviter has heard of (da-91wab) ──────────────────────

/// `r` syncs with `b` serving, which had not heard of it; what each said,
/// and `r` as `b`'s `sync status` lists it after. The inviter is gone, so
/// `r`'s sync fails on that one.
fn reader_syncs_unheard(b: &Copy, r: &Copy) -> (String, String, serde_json::Value) {
    let mut b_runs = b.serve(&["sync", "run"]);
    let out = r.fails(&["sync"], "");
    assert!(out.contains("1 of 2 peer(s) failed"), "a is gone: {out}");
    let served = b_runs.lines_until(" got ");
    let status = b.ok(&["sync", "status", "--json"]);
    (out, served, member_of_list(&status, &r.id()))
}

/// With the inviter gone for good, a filtered reader still syncs with the
/// member that has not heard of it: it shows the chain's records, which the
/// member keeps, since a reader brings none of the inviter's ops they wait
/// on. The member reads its filter from them: the reader gets its fields
/// alone, both verify over them, and the member's `sync status` lists it.
/// The verify agrees because the inviter's entry the member lacks, `from-a`,
/// is outside the filter: inside it, this is the plain reader's case below.
#[test]
fn a_filtered_reader_only_its_inviter_heard_of_shows_its_admission_and_syncs() {
    let (_a, b, r) = admitted_unheard(&["--fields", "tea"]);
    b.ok(&["occur", "tea"]);
    b.ok(&["occur", "from-b"]);
    let (out, served, listed) = reader_syncs_unheard(&b, &r);
    assert!(out.contains("verify ok"), "{out}");
    assert!(served.contains("verify ok"), "{served}");
    assert!(r.has("tea") && !r.has("from-b") && !r.has("from-a"));
    assert_eq!(listed["reader"], true);
    assert_eq!(listed["filter"]["fields"], serde_json::json!(["tea"]));
    assert_eq!(listed["last_verify"]["match"], true);
}

/// A plain reader is served the same way, and gets all the member holds.
/// It holds the inviter's entry too, which the member lacks and no reader
/// relays, so there is no verify to give: each says the reader is ahead, as
/// for a reader the member has long known (da-t3ey2).
#[test]
fn a_plain_reader_only_its_inviter_heard_of_shows_its_admission_and_syncs() {
    let (_a, b, r) = admitted_unheard(&["--reader"]);
    b.ok(&["occur", "from-b"]);
    let (out, served, listed) = reader_syncs_unheard(&b, &r);
    assert!(r.has("from-b"), "{out}");
    assert_eq!(
        (listed["reader"].clone(), listed["filter"].clone()),
        (true.into(), serde_json::Value::Null)
    );
    assert!(r.has("from-a") && !b.has("from-a"));
    // The entry and its spec, which the member lacks; and the reader's
    // invite, its admission and its describe, which the member holds
    // waiting on those two.
    let ahead = "not verified: this copy is 5 changes ahead of ";
    assert!(out.contains(ahead), "{out}");
    assert!(
        served.contains(" is 5 changes ahead of this copy"),
        "{served}"
    );
    assert!(listed["last_verify"].is_null(), "{listed}");
}

/// `datum sync diff <WHO>` (da-n0ruf): two copies whose verify disagrees
/// find the entry and its column by asking the member, which says what it
/// holds and which ops; a pair that agrees says so and how much it compared;
/// and neither side is written. Only a member is answered.
#[test]
fn diff_names_the_entry_and_the_column_two_copies_hold_differently() {
    let (a, b) = paired();
    a.ok(&["occur", "coffee", "cups=2"]);
    let _a_runs = a.serve(&["sync", "run"]);
    let now = b.ok(&["sync"]);
    assert!(now.contains("verify ok"), "{now}");
    let who = &a.id()[..12];
    let label = member_label(&b, &a.id());
    let same = b.ok(&["sync", "diff", who]);
    assert!(
        same.starts_with(&format!("this copy and {label} hold the same data ("))
            && same.ends_with(" entries compared)\n"),
        "{same}"
    );
    let same: serde_json::Value =
        serde_json::from_str(&b.ok(&["sync", "diff", who, "--json"])).unwrap();
    assert_eq!(same["compared"], 2, "{same}");
    assert_eq!(same["differ"], serde_json::json!([]), "{same}");
    assert_eq!(same["peer"]["endpoint"], a.id().as_str(), "{same}");

    // b's row goes stale, as a fold that went wrong leaves it.
    let hid = b.query("SELECT hid FROM entries WHERE field = 'coffee'");
    let sql = |c: &Copy, sql: &str| {
        let db = rusqlite::Connection::open(c.db()).unwrap();
        db.execute_batch(sql).unwrap();
    };
    sql(
        &b,
        "UPDATE entries SET data = '{\"cups\":5}' WHERE field = 'coffee'",
    );
    let state = || {
        let ops = "SELECT COUNT(*) FROM ops";
        (a.hash(), b.hash(), a.query(ops), b.query(ops))
    };
    let before = state();
    assert_ne!(before.0, before.1);
    let said = b.ok(&["sync", "diff", who]);
    assert!(
        said.starts_with(&format!(
            "this copy and {label} hold 1 entry differently, of 2 entries compared:\n\ncoffee:"
        )),
        "{said}"
    );
    assert!(said.contains(&format!(" ({})\n", &hid[..5])), "{said}");
    assert!(
        said.contains(&format!("\n  data.cups: here 5, {label} 2\n")),
        "{said}"
    );
    assert!(said.contains("\n  both hold the same 1 op, "), "{said}");
    let json: serde_json::Value =
        serde_json::from_str(&b.ok(&["sync", "diff", who, "--json"])).unwrap();
    let differs = &json["differ"][0];
    assert_eq!(json["differ"].as_array().unwrap().len(), 1, "{json}");
    assert_eq!(differs["hid"], hid.as_str(), "{json}");
    assert_eq!(
        differs["columns"]["data.cups"],
        serde_json::json!({"here": 5, "there": 2}),
        "{json}"
    );
    assert_eq!(differs["there"]["ops"][0]["kind"], "create", "{json}");
    assert_eq!(state(), before, "a diff writes nothing on either side");

    // The row gone and its ops kept (da-95y4o), seen from beside it.
    sql(&b, "DELETE FROM entries WHERE field = 'coffee'");
    let said = b.ok(&["sync", "diff", who]);
    assert!(
        said.contains(&format!("\n  no row here; {label} has one\n")),
        "{said}"
    );
    assert!(said.contains("\n  both hold the same 1 op, "), "{said}");

    // An entry only one side has, with the op the other lacks.
    a.ok(&["occur", "tea"]);
    let said = b.ok(&["sync", "diff", who]);
    assert!(said.contains("\ntea:"), "{said}");
    assert!(said.contains("\n  ops only here: none\n"), "{said}");
    assert!(
        said.contains(&format!("\n  ops only on {label}: create by ")),
        "{said}"
    );

    // A copy that is no member is told nothing.
    a.ok(&["sync", "chain", "revoke", &b.id()[..12]]);
    let refused = b.fails(&["sync", "diff", who], "");
    assert!(refused.contains("refused: "), "{refused}");
}

/// A filtered reader's diff is over what it shares (da-n0ruf, da-u81u1):
/// an entry outside its filter is no difference, and asked for by hid the
/// member tells it neither the row nor its ops.
#[test]
fn a_filtered_readers_diff_is_over_its_fields_alone() {
    let (a, r) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee"]);
    a.ok(&["occur", "tea"]);
    let (mut inviting, ticket) = a.invite(&["--fields", "coffee"]);
    r.ok(&["sync", "chain", "connect", &ticket]);
    assert!(inviting.finish().0);
    assert!(!r.has("tea"));

    let _a_runs = a.serve(&["sync", "run"]);
    let same = r.ok(&["sync", "diff", &a.id()[..12]]);
    // The coffee and the spec its log declared.
    assert!(
        same.contains("hold the same data (2 entries compared)"),
        "{same}"
    );

    let tea = a.query("SELECT hid FROM entries WHERE field = 'tea'");
    let held = |peer: &[&str]| -> serde_json::Value {
        let args = [&["sync", "pipe", "held"], peer].concat();
        let out = a.run_in(&args, &format!("[\"{tea}\"]"));
        serde_json::from_slice(&out.stdout).unwrap()
    };
    let told = held(&["--peer", &r.id()]);
    assert_eq!(
        told,
        serde_json::json!([{"hid": tea, "row": null, "ops": []}])
    );
    // A member with no filter is told both.
    let told = held(&[]);
    assert_eq!(told[0]["row"]["field"], "tea", "{told}");
    assert_eq!(told[0]["ops"][0]["kind"], "create", "{told}");

    // The member asking the reader compares the same two entries: the tea
    // is no difference, so its hid is never sent to the reader either.
    drop(_a_runs);
    let _r_runs = r.serve(&["sync", "run"]);
    let same = a.ok(&["sync", "diff", &r.id()[..12]]);
    assert!(
        same.contains("hold the same data (2 entries compared)"),
        "{same}"
    );
}

/// A diff admits no one (da-n0ruf, da-2u95o): a member the answering copy
/// has not heard of is refused, where its sync would show the chain's
/// records and be admitted, and the answering copy still has not heard of
/// it afterwards. Once it has synced, its diff is answered.
#[test]
fn a_diff_from_a_member_not_heard_of_is_refused_and_admits_no_one() {
    let (_a, b, c) = admitted_unheard(&[]);
    let heard = || b.ok(&["sync", "chain", "list"]).contains(&c.id()[..10]);
    let _b_runs = b.serve(&["sync", "run"]);
    let who = &b.id()[..12];
    let out = c.fails(&["sync", "diff", who], "");
    assert!(
        out.contains("refused: not a member of this sync chain"),
        "{out}"
    );
    assert!(!heard(), "a diff admitted its dialer");

    // The inviter is gone, so the sync as a whole fails; the one with b verifies.
    let synced = c.fails(&["sync"], "");
    assert!(synced.contains("verify ok"), "{synced}");
    assert!(heard());
    let same = c.ok(&["sync", "diff", who]);
    assert!(same.contains("hold the same data ("), "{same}");
}
