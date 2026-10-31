//! Who is in the chain: an open chain's invites and expels, the first admin
//! key locking it, and an expelled device coming back.

use super::*;

/// `invite --label` in a locked chain renames this copy only once the
/// invite is signed: one refused for want of an admin renames nothing
/// (da-ey3hz).
#[test]
fn a_refused_invite_in_a_locked_chain_renames_nothing() {
    let (a, _b, _) = locked();
    let before = member_of(&a, &a.id())["label"].clone();
    let why = a.fails(&["sync", "chain", "invite", "--label", "laptop"], "");
    assert!(why.contains("no approval code"), "{why}");
    assert_eq!(member_of(&a, &a.id())["label"], before);
    assert_ne!(before, "laptop");
}

#[test]
fn two_copies_with_no_admin_join_by_invite_and_join() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea"]);
    admit(&a, &b, "b", None);
    assert!(b.has("tea"));
    let members = json(&b.ok(&["sync", "chain", "list", "--json"]));
    assert_eq!(members["admins"], serde_json::json!([]), "{members}");
    let via: Vec<_> = members["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["via"].as_str().unwrap().split(' ').next().unwrap())
        .collect();
    assert_eq!(via, ["genesis", "invite"], "{members}");
    b.ok(&["occur", "coffee"]);
    assert!(b.send(&a).0);
    assert!(a.has("coffee"));
    assert_eq!(a.hash(), b.hash());
}

/// A copy that starts a chain holding a non-member's ops signs every entry
/// again as its own (da-1flih), so the chain never meets that endpoint's
/// ops: a joiner gets the founder's rows however its applies are cut, each
/// edit either made of the other's entry included.
#[test]
fn a_founder_signs_a_non_members_entries_again_as_its_own() {
    let (x, a) = (Copy::new(), Copy::new());
    x.ok(&["occur", "tea", "-t", "2026-10-04 08:00"]);
    let (ok, text) = x.send(&a);
    assert!(ok, "{text}");
    a.ok(&["occur", "coffee", "-t", "2026-10-04 09:00"]);
    assert!(a.send(&x).0);
    let hid = |f: &str| a.query(&format!("SELECT hid FROM entries WHERE field = '{f}'"));
    x.ok(&["update", &hid("coffee"), "cups=2"]);
    assert!(x.send(&a).0);
    a.ok(&["update", &hid("tea"), "cups=3"]);
    a.ok(&["sync", "new-invite"]);
    let x_id = x.id();
    assert_eq!(
        a.query(&format!(
            "SELECT COUNT(*) FROM ops WHERE endpoint = '{x_id}'"
        )),
        "0"
    );
    // Signed then, not at the next sync: an entry's history is its ops.
    assert_eq!(
        a.query("SELECT COUNT(*) FROM entries WHERE hid NOT IN (SELECT hid FROM ops)"),
        "0"
    );

    let lines = a.ok(&["sync", "pipe", "ops"]);
    let whole = Copy::new();
    whole.ok_in(&["sync", "pipe", "apply"], &lines);
    let split = Copy::new();
    for line in lines.lines() {
        split.ok_in(&["sync", "pipe", "apply"], &format!("{line}\n"));
    }
    assert!(whole.has("tea") && whole.has("coffee"));
    assert_eq!(whole.hash(), a.hash());
    assert_eq!(split.hash(), a.hash());
}

/// A non-member's chain act is refused, open chain or locked, and so is its
/// handshake without an invite.
#[test]
fn a_non_members_act_is_refused_open_chain_or_not() {
    for lock in [false, true] {
        let (a, b) = chained();
        if lock {
            a.ok(&["sync", "admin", "add", &a.key("admin.key")]);
        }
        let stranger = Copy::new();
        let chain_id = a.query(
            "SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1",
        );
        let revoke = serde_json::json!({
            "type": "revoke", "chain": chain_id, "endpoint": b.id(),
            "created": "2026-09-29T00:00:00Z",
        });
        let out = a.run(&["sync", "pipe", "apply"], &forged(&stranger, &a, revoke));
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{text}");
        assert!(
            text.contains("its writer is not a member of the sync chain"),
            "{text}"
        );
        assert!(member_of(&a, &b.id())["revoked"].is_null());

        let hello = a.fails(
            &[
                "sync",
                "handshake",
                &stranger.id(),
                "--label",
                "s",
                "--addr",
                "{}",
            ],
            "",
        );
        assert!(hello.contains("not a member of this sync chain"), "{hello}");
        let own = a.fails(
            &["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"],
            "",
        );
        assert!(own.contains("is a copy of this database"), "{own}");
        assert!(b
            .ok(&["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"])
            .contains("known"));
    }
}

#[test]
fn in_an_open_chain_a_member_invites_a_third_copy_and_revokes_one() {
    let (a, b) = chained();
    let c = Copy::new();
    // b, which did not start the chain, invites c; a hears of it from b.
    admit(&b, &c, "c", None);
    assert!(b.send(&a).0);
    assert!(member_of(&a, &c.id())["via"]
        .as_str()
        .unwrap()
        .starts_with("invite "));
    // c revokes a: no admin, no key, no prompt.
    assert!(c
        .ok(&["sync", "chain", "expel", &a.id()[..10]])
        .starts_with("expelled "));
    assert!(c.send(&b).0);
    assert!(member_of(&b, &a.id())["revoked"].is_string());
    a.ok(&["occur", "after"]);
    let (ok, text) = a.send(&b);
    assert!(
        !ok && text.contains("made after its endpoint was expelled"),
        "{text}"
    );
}

/// `chain list` and `status` leave a revoked or reverted member out and say
/// how many (da-73gyz); `--show-revoked` shows them, `--json` always carries them,
/// and a revoked copy still lists itself.
#[test]
fn chain_list_and_status_hide_revoked_members_and_say_how_many() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    // A member's row, not a ⚠ notice that names it.
    let shows = |args: &[&str], who: &Copy| {
        a.ok(args)
            .lines()
            .any(|l| !l.starts_with('⚠') && l.contains(&who.id()[..10]))
    };
    let count = |args: &[&str]| json(&a.ok(args))["members"].as_array().unwrap().len();

    a.ok(&["sync", "chain", "expel", "c"]);
    for (cmd, live) in [
        (&["sync", "chain", "list"][..], 3),
        (&["sync", "status"], 2),
    ] {
        let out = a.ok(cmd);
        assert!(
            out.contains("1 former member hidden — `--show-revoked`"),
            "{out}"
        );
        assert!(shows(cmd, &b) && !shows(cmd, &c), "{out}");
        assert!(shows(&[cmd, &["--show-revoked"]].concat(), &c));
        assert!(!a
            .ok(&[cmd, &["--show-revoked"]].concat())
            .contains("hidden"));
        assert_eq!(count(&[cmd, &["--json"]].concat()), live);
    }

    // A revert is hidden the same way, and the count is of both.
    a.ok(&["sync", "chain", "expel", "b", "--unjoin", "--yes"]);
    for cmd in [&["sync", "chain", "list"][..], &["sync", "status"]] {
        let out = a.ok(cmd);
        assert!(
            out.contains("2 former members hidden — `--show-revoked`"),
            "{out}"
        );
        assert!(!shows(cmd, &b) && !shows(cmd, &c), "{out}");
        assert!(a
            .ok(&[cmd, &["--show-revoked"]].concat())
            .contains("UNJOINED"));
    }
    assert!(shows(&["sync", "chain", "list"], &a));

    // c hears of its own revoke, and still lists itself.
    assert!(a.send(&c).0);
    let own = c.ok(&["sync", "chain", "list"]);
    assert!(
        own.contains("(this copy)") && own.contains("EXPELLED"),
        "{own}"
    );
}

#[test]
fn the_first_admin_add_locks_the_chain() {
    let (a, b) = chained();
    // Any member may lock it: b, which did not start the chain.
    let admin = b.key("admin.key");
    b.ok(&["sync", "admin", "add", &admin]);
    assert!(b.send(&a).0);

    let out = a.fails(&["sync", "new-invite"], "not-a-signature\n");
    assert!(out.contains("https://datum.day/sync/approve#"), "{out}");
    assert!(out.contains("not signed by an admin"), "{out}");
    let out = a.fails(&["sync", "chain", "expel", "b"], "");
    assert!(out.contains("no approval code"), "{out}");
    // A second key needs the first: first come, first served.
    let out = a.fails(&["sync", "admin", "add", &a.key("mine.key")], "\n");
    assert!(out.contains("no approval code"), "{out}");

    a.ok(&["sync", "new-invite", "--admin-key", &admin]);
    a.ok(&["sync", "chain", "expel", "b", "--admin-key", &admin]);
    let members = json(&a.ok(&["sync", "chain", "list", "--json"]));
    assert_eq!(members["admins"].as_array().unwrap().len(), 1, "{members}");
}

#[test]
fn a_revoke_refuses_what_the_endpoint_made_after_it_on_every_copy() {
    let (a, b, admin) = locked();
    let c = Copy::new();
    admit(&a, &c, "c", Some(&admin));
    b.ok(&["occur", "before"]);
    assert!(b.send(&a).0 && b.send(&c).0);

    // Three characters of an id name nothing: too easy to hit by accident.
    let b_id = b.id();
    let short = a.fails(
        &["sync", "chain", "expel", &b_id[..3], "--admin-key", &admin],
        "",
    );
    assert!(short.contains("names 0 members"), "{short}");
    a.ok(&["sync", "chain", "expel", "b", "--admin-key", &admin]);
    b.ok(&["occur", "after"]);
    // c takes b's op before it hears of the revoke…
    assert!(b.send(&c).0);
    assert!(c.has("after"));
    // …and drops it when it does.
    let (ok, text) = a.send(&c);
    assert!(
        !ok && text.contains("made after its endpoint was expelled"),
        "{text}"
    );
    assert!(!c.has("after") && c.has("before"));
    // a refuses it outright.
    let (ok, text) = b.send(&a);
    assert!(
        !ok && text.contains("made after its endpoint was expelled"),
        "{text}"
    );
    assert!(!a.has("after") && a.has("before"));
    assert_eq!(a.hash(), c.hash());
    let members = a.ok(&["sync", "chain", "list", "--show-revoked"]);
    assert!(members.contains("EXPELLED"), "{members}");
}

/// A revoked device rejoins by a fresh invite (da-j8c4l), as a new device
/// joins: its endpoint, label and first admission kept, its revoke cleared.
/// What it made while out — refused by every copy until then, and by its
/// own once it heard of its revoke — folds in everywhere, and what the
/// others made meanwhile reaches it. Every member is told it came back.
#[test]
fn a_revoked_device_rejoins_by_a_fresh_invite_with_what_it_made_while_out() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    b.ok(&["occur", "before"]);
    assert!(b.send(&a).0 && a.send(&c).0);
    let admitted = member_of(&a, &b.id())["admitted"].clone();

    a.ok(&["sync", "chain", "expel", "b"]);
    assert!(a.send(&b).0 && a.send(&c).0);
    b.ok(&["occur", "out"]);
    let (ok, text) = b.send(&a);
    assert!(
        !ok && text.contains("made after its endpoint was expelled"),
        "{text}"
    );
    assert!(!a.has("out"));
    a.ok(&["occur", "meanwhile"]);
    let status = b.ok(&["sync", "status"]);
    assert!(status.contains("this copy was expelled"), "{status}");

    let out = redeem(&a, &b, "b again", None, &["--chain", &chain_of(&a)]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(a.has("out"));
    assert!(a.send(&b).0 && b.send(&a).0 && a.send(&c).0);
    for copy in [&a, &b, &c] {
        assert!(copy.has("before") && copy.has("out") && copy.has("meanwhile"));
        let m = member_of(copy, &b.id());
        assert!(m["revoked"].is_null() && m["rejoined"].is_string(), "{m}");
        assert_eq!(
            (&m["label"], &m["admitted"]),
            (&"b".into(), &admitted),
            "{m}"
        );
    }
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), c.hash());
    let members = json(&c.ok(&["sync", "chain", "list", "--json"]))["members"].clone();
    assert_eq!(members.as_array().unwrap().len(), 3, "{members}");
    let notices = c.ok(&["sync", "status", "--json"]);
    assert!(notices.contains("b joined again, at "), "{notices}");
    // Its whole id names its admission and its rejoin: one endpoint.
    c.ok(&["sync", "chain", "ack", &b.id()]);
    let notices = json(&c.ok(&["sync", "status", "--json"]))["notices"].to_string();
    assert!(!notices.contains(&b.id()[..18]), "{notices}");
    assert!(!b.ok(&["sync", "status"]).contains("expelled"));
}
