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

/// A join names its device and its person (da-xza37), as a first invite
/// does: `--label` and `--user` describe it, the username is kept for the install, and a
/// second database's join there describes it by that username unasked.
#[test]
fn a_join_describes_its_copy_by_the_label_and_user_given_and_keeps_the_user() {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&["--user", "al"]);
    b.ok(&[
        "sync", "chain", "join", "--label", "phone", "--user", "bea", &ticket,
    ]);
    assert!(inviting.finish().0);
    // The first invite describes the copy it starts the chain on by its own.
    let list = a.ok(&["sync", "chain", "list", "--json"]);
    assert_eq!(member_of_list(&list, &a.id())["user"].as_str(), Some("al"));
    // Its peers hear of its describe at the next sync, as of any.
    let list = b.ok(&["sync", "chain", "list", "--json"]);
    let me = member_of_list(&list, &b.id());
    assert_eq!(
        (me["label"].as_str(), me["user"].as_str()),
        (Some("phone"), Some("bea"))
    );
    let other = Copy::new();
    let (mut inviting, ticket) = other.invite(&[]);
    let work = b.tmp.path().join("work.db");
    let work = work.to_str().unwrap();
    b.ok(&["--db", work, "sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    let id = b.ok(&["--db", work, "sync", "status", "--json"]);
    let id: serde_json::Value = serde_json::from_str(&id).unwrap();
    let id = id["this"]["endpoint"].as_str().unwrap().to_string();
    // What work's chain says of it, after a sync there (its peer is offline:
    // only the describe a sync starts with counts).
    let work_user = || {
        b.run(&["--db", work, "sync"]);
        let list = b.ok(&["--db", work, "sync", "chain", "list", "--json"]);
        member_of_list(&list, &id)["user"]
            .as_str()
            .map(String::from)
    };
    assert_eq!(work_user().as_deref(), Some("bea"));
    // `rename --user` changes it, keeping the label; empty forgets it.
    b.ok(&["sync", "chain", "rename", "--user", "b"]);
    let list = b.ok(&["sync", "chain", "list", "--json"]);
    let me = member_of_list(&list, &b.id());
    assert_eq!(
        (me["label"].as_str(), me["user"].as_str()),
        (Some("phone"), Some("b"))
    );
    // The install's other chained database takes it at its next sync.
    assert_eq!(work_user().as_deref(), Some("b"));
    b.ok(&["sync", "chain", "rename", "--user", ""]);
    let list = b.ok(&["sync", "chain", "list", "--json"]);
    let os_user = b.env[0].1;
    assert_eq!(
        member_of_list(&list, &b.id())["user"].as_str(),
        Some(os_user)
    );
    // Forgotten, too: it goes back to the user it runs as.
    assert_eq!(work_user().as_deref(), Some(os_user));
}

/// Piped, a join asks nothing: its label is assembled and its user is the
/// one it runs as, as before (da-xza37).
#[test]
fn a_piped_join_asks_nothing_and_keeps_todays_label_and_user() {
    let (a, b) = (Copy::new(), Copy::new());
    let (mut inviting, ticket) = a.invite(&[]);
    let out = b.run(&["sync", "chain", "join", &ticket]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(inviting.finish().0);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        !said.contains("This device's name") && !said.contains("username"),
        "{said}"
    );
    let list = b.ok(&["sync", "chain", "list", "--json"]);
    let me = member_of_list(&list, &b.id());
    let user = b.env[0].1;
    let label = me["label"].as_str().unwrap();
    assert!(label.starts_with(&format!("{user}@")), "{label}");
    assert_eq!(me["user"].as_str(), Some(user));
}
