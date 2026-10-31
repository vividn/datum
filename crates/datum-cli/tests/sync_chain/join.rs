//! A join cut short, and resumed (da-38eye, da-idw54).

use super::*;

/// A join cut short (da-idw54): `a` held entries before its chain began, so
/// a first sync carries its chain records — `b`'s admission among them —
/// after them. `b`, holding part of the history, is in no chain yet: it
/// names `a` as the copy it is joining through, and takes a session with it
/// either way round, which is how it resumes. Before any of `a`'s ops, and
/// for a copy whose ops it does not hold, the address book grants nothing.
#[test]
fn a_join_cut_short_names_the_copy_it_joins_through_and_resumes_with_it() {
    let (a, b) = (Copy::new(), Copy::new());
    for t in ["0800", "0900", "1000"] {
        a.ok(&["occur", "tea", "-t", t]);
    }
    let out = redeem(&a, &b, "b", None, &["--chain", ""]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // b's side of the handshake: it dialed a, and was welcomed.
    let hello = ["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"];
    b.ok(&[&hello[..], &["--dialed"]].concat());
    assert!(b
        .fails(&["sync", "chain", "list"], "")
        .contains("in no sync chain"));

    let ops = a.ops_for(&b);
    assert!(
        ops.lines()
            .take(2)
            .all(|l| !l.contains(r#""kind":"chain""#)),
        "the history comes first"
    );
    b.ok_in(
        &["sync", "pipe", "apply"],
        &ops.lines().take(2).collect::<Vec<_>>().join("\n"),
    );
    let list = json(&b.ok(&["sync", "chain", "list", "--json"]));
    assert!(list["chain"].is_null(), "{list}");
    assert_eq!(list["joining"][0]["endpoint"], a.id().as_str(), "{list}");
    let text = b.ok(&["sync", "chain", "list"]);
    assert!(text.contains("joining a sync chain through a ("), "{text}");
    assert!(b
        .ok(&["sync", "status"])
        .contains("`datum sync` resumes it"));
    let status = json(&b.ok(&["sync", "status", "--json"]));
    assert!(status["chain"].is_null(), "{status}");
    assert_eq!(
        status["members"][0]["endpoint"],
        a.id().as_str(),
        "{status}"
    );
    assert_eq!(status["members"][0]["addr"], "{}", "{status}");
    // a dials b to push: taken. A stranger: refused.
    assert!(b.ok(&hello).contains("known"));
    let stranger = Copy::new().id();
    let refused = b.fails(
        &[
            "sync",
            "handshake",
            &stranger,
            "--label",
            "s",
            "--addr",
            "{}",
        ],
        "",
    );
    assert!(
        refused.contains("not a member of this sync chain"),
        "{refused}"
    );

    // The rest arrives: b is in a's chain, and joining nothing.
    assert!(a.send(&b).0);
    let theirs = json(&a.ok(&["sync", "chain", "list", "--json"]))["chain"].clone();
    let list = json(&b.ok(&["sync", "chain", "list", "--json"]));
    assert_eq!(list["chain"], theirs, "{list}");
    assert!(list.get("joining").is_none(), "{list}");
    assert_eq!(a.hash(), b.hash());
}

/// A join cut short before any op applied (da-38eye): `b` holds nothing of
/// `a`'s, but its own side of the handshake said it redeemed an invite
/// there, so it names `a` as the copy it is joining through and takes a
/// session with it — `a` has already used the invite, so nothing else could.
#[test]
fn a_join_cut_short_before_any_op_resumes_where_its_invite_was_redeemed() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea", "-t", "0800"]);
    let out = redeem(&a, &b, "b", None, &["--chain", ""]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let hello = ["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"];
    b.ok(&[&hello[..], &["--dialed", "--invite", "secret"]].concat());
    let list = json(&b.ok(&["sync", "chain", "list", "--json"]));
    assert!(list["chain"].is_null(), "{list}");
    assert_eq!(list["joining"][0]["endpoint"], a.id().as_str(), "{list}");
    let status = json(&b.ok(&["sync", "status", "--json"]));
    assert_eq!(
        status["members"][0]["endpoint"],
        a.id().as_str(),
        "{status}"
    );
    assert!(b.ok(&hello).contains("known"));

    assert!(a.send(&b).0);
    let list = json(&b.ok(&["sync", "chain", "list", "--json"]));
    assert!(!list["chain"].is_null(), "{list}");
    assert!(list.get("joining").is_none(), "{list}");
    assert_eq!(a.hash(), b.hash());
}

/// A copy cut short like that which starts a chain of its own gives the
/// join up: left later, it is joining nothing (da-38eye).
#[test]
fn a_copy_that_starts_its_own_chain_gives_up_the_join_it_was_making() {
    let (a, b) = (Copy::new(), Copy::new());
    let out = redeem(&a, &b, "b", None, &["--chain", ""]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let hello = ["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"];
    b.ok(&[&hello[..], &["--dialed", "--invite", "secret"]].concat());
    b.ok(&["sync", "new-invite"]);
    b.ok(&["sync", "chain", "leave"]);
    assert!(b
        .fails(&["sync", "chain", "list"], "")
        .contains("in no sync chain"));
}
