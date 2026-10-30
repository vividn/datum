//! An invite and its redeeming: who joins, what they are told, and who is refused.

use super::*;

#[test]
fn an_invite_pairs_two_copies_and_their_data_converges() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    b.ok(&["occur", "tea"]);
    let (mut inviting, ticket) = a.invite(&["--expires", "5m"]);
    let joined = b.ok(&["sync", "chain", "join", &ticket]);
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

/// An invite prints a datum.day join link (da-l9kdr), which `join`
/// takes whole, as it takes the bare ticket the link's fragment holds.
#[test]
fn join_takes_the_join_link_or_its_bare_ticket() {
    let (a, b, c) = (Copy::new(), Copy::new(), Copy::new());
    let (mut inviting, link) = a.invite(&[]);
    assert!(link.starts_with("https://datum.day/sync/join#"), "{link}");
    assert!(b
        .ok(&["sync", "chain", "join", &link])
        .contains("verify ok"));
    assert!(inviting.finish().0);

    let (mut inviting, link) = a.invite(&[]);
    let bare = link.rsplit('#').next().unwrap();
    assert!(c.ok(&["sync", "chain", "join", bare]).contains("verify ok"));
    assert!(inviting.finish().0);
    assert_eq!(a.hash(), c.hash());
}

/// An invite names its expiry on the reader's clock, as `sync status` does,
/// not the stored instant (da-4cwn8).
#[test]
fn an_invite_names_its_expiry_on_the_readers_clock() {
    let a = Copy::new();
    // The clock alone: a date before it, on another day, is the helper's
    // rule, and a midnight between the reads would make it either.
    let kathmandu = |at: chrono::DateTime<chrono::Utc>| {
        let east = chrono::FixedOffset::east_opt(5 * 3600 + 45 * 60).unwrap();
        format!("{}.", at.with_timezone(&east).format("%H:%M"))
    };
    let ten = chrono::Duration::minutes(10);
    let before = kathmandu(chrono::Utc::now() + ten);
    let mut inviting = Serving::of(
        a.cmd(&["sync", "chain", "invite", "--expires", "10m"])
            .env("TZ", "Asia/Kathmandu"),
    );
    let line = inviting.lines_until("One-time invite");
    let after = kathmandu(chrono::Utc::now() + ten);
    assert!(
        (line.contains(&before) || line.contains(&after)) && !line.contains('Z'),
        "{before} or {after} in {line}"
    );
}

/// `--qr` says what its code is for on the line before it: a newcomer told to
/// type it sees a scan, not only a browser (da-ju9hy).
#[test]
fn an_invites_qr_code_says_to_scan_it() {
    let a = Copy::new();
    let mut inviting = a.serve(&["sync", "chain", "invite", "--qr"]);
    let caption = inviting.lines_until("or open the link in a browser");
    assert!(
        caption.contains("scan this code with a phone's camera:"),
        "{caption}"
    );
    // Under it the code's quiet zone, blank, then the code itself.
    let mut code = String::new();
    while code.trim().is_empty() {
        code.clear();
        inviting.out.read_line(&mut code).unwrap();
    }
    assert!(code.contains(['█', '▀', '▄']), "{code:?}");
}

/// The join line an invite prints, pasted into a shell, hands `join` the
/// whole link as one argument (da-sin5b). Unquoted, zsh with `extendedglob`
/// refused it — the `#` is a glob there, «no matches found» — and sh took it.
#[test]
fn the_printed_join_line_pastes_into_a_shell() {
    let a = Copy::new();
    let mut inviting = a.serve(&["sync", "chain", "invite"]);
    let line = inviting.lines_until("datum sync chain join ");
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
            ["sync", "chain", "join", link],
            "{}",
            shell[0]
        );
    }
}

/// A member that is not running is named, with what runs it (da-shfv4):
/// «timed out» alone said neither which side nor what to do. Bare `sync`
/// says it offline; `-v` says the words (da-a4fxr). Its only member, it
/// synced with none, which fails (da-9d6uy).
#[test]
fn a_member_not_running_is_named_with_what_runs_it() {
    let (_a, mut b) = paired();
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "2")];
    let said = b.fails(&["sync", "-v"], "");
    assert!(said.contains("none of 1 peer(s) synced"), "{said}");
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
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "2"), ("TZ", "Etc/GMT-2")];
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
    // On the reader's clock, as the notices are (da-jb4ln).
    let seen = chrono::DateTime::parse_from_rfc3339(row["last_seen"].as_str().unwrap()).unwrap();
    let seen = seen.with_timezone(&chrono::FixedOffset::east_opt(2 * 3600).unwrap());
    let said = line(&b);
    assert!(
        said.contains(" offline ") && said.contains(&seen.format("%H:%M").to_string()),
        "{said}"
    );
    assert!(!said.contains("could not reach"), "{said}");

    a.ok(&["sync", "chain", "expel", &b.id()]);
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
    c.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    let (up, down) = (member_label(&a, &b.id()), member_label(&a, &c.id()));
    let _b_runs = b.serve(&["sync", "run"]);
    a.env.push(("DATUM_SYNC_DIAL_TIMEOUT_S", "3"));
    // The last seen on the reader's clock, not the stored instant (da-49tq4).
    a.env.push(("TZ", "Asia/Kathmandu"));

    let sync = |args: &[&str]| {
        let mut run = Killed(a.cmd(args).spawn().unwrap());
        let out = lines(run.0.stdout.take().unwrap());
        let said: Vec<(String, Instant)> = out.iter().map(|l| (l, Instant::now())).collect();
        assert!(run.0.wait().unwrap().success(), "{said:?}");
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
    let seen = entry_of(&a, "status", &c);
    let east = chrono::FixedOffset::east_opt(5 * 3600 + 45 * 60).unwrap();
    let seen = chrono::DateTime::parse_from_rfc3339(seen["last_seen"].as_str().unwrap())
        .unwrap()
        .with_timezone(&east);
    let today = chrono::Utc::now().with_timezone(&east).date_naive();
    let seen = seen
        .format(if seen.date_naive() == today {
            "%H:%M"
        } else {
            "%Y-%m-%d %H:%M"
        })
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
    b.run_in(&["sync", "chain", "join"], &ticket);
    assert!(inviting.finish().0);

    let (mut expiring, stale) = a.invite(&["--expires", "1s"]);
    let (ok, _) = expiring.finish();
    assert!(!ok, "an invite unredeemed when it expires fails");

    let _serving = a.serve(&["sync", "run"]);
    let c = Copy::new();
    let used = c.fails(&["sync", "chain", "join", &ticket], "");
    assert!(used.contains("invite already used"), "{used}");
    // Why, and what to do next (da-0tawk).
    assert!(
        used.contains("Next: ask") && used.contains("for a fresh invite"),
        "{used}"
    );
    // A member presenting it is a redeemer like any (da-j8c4l): refused.
    let again = b.fails(&["sync", "chain", "join", &ticket], "");
    assert!(again.contains("invite already used"), "{again}");
    let expired = c.fails(&["sync", "chain", "join", &stale], "");
    assert!(expired.contains("invite expired"), "{expired}");
    assert!(expired.contains("for a fresh invite"), "{expired}");
    assert!(!a.ok(&["sync", "chain", "list"]).contains(&c.id()[..10]));
}

/// `join` says at once whom it waits for and until when, says every few
/// seconds that it still is, and a join nobody answers ends saying why and
/// what to do next (da-0tawk): vividn's terminal sat silent for a minute.
#[test]
fn join_says_what_it_waits_for_and_why_it_ended_unanswered() {
    let (a, mut b) = (Copy::new(), Copy::new());
    let (inviting, ticket) = a.invite(&[]);
    // Nobody serves the invite now.
    drop(inviting);
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "6")];
    let out = b.fails(&["sync", "chain", "join", &ticket], "");
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
    let late = c.fails(&["sync", "chain", "join", &stale], "");
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
    let unknown = c.fails(&["sync", "chain", "join", &forged], "");
    assert!(unknown.contains("refused: unknown invite"), "{unknown}");
    assert!(unknown.contains("Next: ask "), "{unknown}");
    assert!(unknown.contains(" for a fresh invite"), "{unknown}");

    a.ok(&["sync", "pause"]);
    let paused = c.fails(&["sync", "chain", "join", &ticket], "");
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
    let mut own = a.cmd(&["sync", "chain", "join", &ticket]);
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
    let refused = a.fails(&["sync", "chain", "join", &ticket], "");
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
    let joined = b.ok(&["sync", "chain", "join", &ticket]);
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
    let refused = b.fails(&["sync", "chain", "join", &ticket], "");
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
    a.ok(&["sync", "chain", "expel", &b.id()]);
    let c = Copy::new();
    let (_inviting, ticket) = c.invite(&[]);
    let refused = a.fails(&["sync", "chain", "join", &ticket], "");
    assert!(
        refused.contains("(1 members); a copy belongs to one chain"),
        "{refused}"
    );
    assert!(!c.has("walk"));
}
