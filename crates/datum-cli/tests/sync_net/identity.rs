//! A member's label, its announcement, and each session recorded for `sync status`
//! (da-rcb5m).

use super::*;

/// `invite --label` names the copy that starts the chain, `join --label`
/// the one that joins, and every member lists both; an invite from a copy
/// in the chain renames it (da-ey3hz).
#[test]
fn invite_and_join_name_their_copies_by_label() {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&["--label", "laptop"]);
    b.ok(&["sync", "chain", "join", "--label", "phone", &ticket]);
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
    for cmd in [&["invite"][..], &["join", "ticket"]] {
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
/// it and the database's name after it (`sync_chain/labels.rs` has a copy
/// alone, its host); ann's second
/// database tells her two copies apart by that name. Every copy here runs
/// on this one host; members on different hosts (a laptop and a phone) are
/// tested in chain.rs, `a_label_is_the_least_that_tells_the_chains_members_apart`.
#[test]
fn an_unset_label_is_assembled_alike_on_every_copy() {
    let a = Copy::of("ann", "data.db");
    let join = |copy: &Copy| {
        let (mut inviting, ticket) = a.invite(&[]);
        copy.ok(&["sync", "chain", "join", &ticket]);
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
        first.contains("not yours? `datum sync chain expel "),
        "{first}"
    );
}

// ─── A device's identity, and each session recorded (da-rcb5m) ──────────────

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
