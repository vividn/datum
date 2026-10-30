//! Who a copy admits: an invite's redeemer with the inviter off (da-odwc7), members
//! one inviter admitted (da-fly3f), and those only their inviter has heard of
//! (da-2u95o, da-91wab).

use super::*;

/// c's dial to a reaches no one in 3s, and b admits it. The dial's limit,
/// not the session's: b's handshake, the one stretch before its `Welcome`
/// with nothing sent, took under 200ms on two loaded cores, and a CI runner
/// starved it past 3s (da-c5b2b, as da-hd9sx's and da-3njqh's; da-c6v27).
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
    c.env = vec![("DATUM_SYNC_DIAL_TIMEOUT_S", "3")];
    c.ok(&["occur", "from-c"]);
    let joined = c.ok(&["sync", "chain", "join", &ticket]);
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
    d.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);

    // Only d fetches the next invite before a goes away; b is behind, and
    // not up: d, told where b is (da-fly3f), dials it too and reaches no one.
    let (inviting, ticket) = a.invite(&[]);
    d.env = vec![("DATUM_SYNC_DIAL_TIMEOUT_S", "3")];
    let synced = d.ok(&["sync"]);
    assert!(synced.contains("verify ok"), "{synced}");
    drop(inviting);
    let (_b_runs, _d_runs) = (b.serve(&["sync", "run"]), d.serve(&["sync", "run"]));
    let mut c = Copy::new();
    c.env = vec![("DATUM_SYNC_DIAL_TIMEOUT_S", "3")];
    let joined = c.ok(&["sync", "chain", "join", &ticket]);
    assert!(joined.contains("verify ok"), "{joined}");
    // c joined through b, so d hears of c only from b's `sync run` pushing
    // to it afterwards.
    eventually("d's chain list naming c", || {
        let members = d.ok(&["sync", "chain", "list"]);
        members.contains(&c.id()[..10]) || {
            eprintln!("{members}");
            false
        }
    });
}

// ─── Members one inviter admitted (da-fly3f) ────────────────────────────────

/// `a`, `b` and `c` in one chain, `b` and `c` both admitted by `a`, which
/// has gone away: `b` heard of `c` from `a` first. A dial that reaches no
/// one fails in 3s.
fn admitted_by_one_inviter() -> (Copy, Copy, Copy) {
    let (a, mut b) = paired();
    let mut c = Copy::new();
    let (mut inviting, ticket) = a.invite(&[]);
    c.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    let (serving, _) = a.invite(&[]);
    b.ok(&["sync"]);
    drop(serving);
    b.env = vec![("DATUM_SYNC_DIAL_TIMEOUT_S", "3")];
    c.env = vec![("DATUM_SYNC_DIAL_TIMEOUT_S", "3")];
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
        let out = y.ok(&["sync"]);
        assert!(out.contains("verify ok"), "{out}");
        assert!(out.contains(": offline, last seen "), "a is gone: {out}");
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

// ─── A member only its inviter has heard of (da-2u95o) ──────────────────────

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
    let out = c.ok(&["sync"]);
    assert!(out.contains("verify ok"), "{out}");
    assert!(out.contains(": offline, last seen "), "a is gone: {out}");
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
    b.ok(&["sync", "chain", "expel", &a.id()[..10]]);
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
/// `r`'s sync says that one offline.
fn reader_syncs_unheard(b: &Copy, r: &Copy) -> (String, String, serde_json::Value) {
    let mut b_runs = b.serve(&["sync", "run"]);
    let out = r.ok(&["sync"]);
    assert!(out.contains(": offline, last seen "), "a is gone: {out}");
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
