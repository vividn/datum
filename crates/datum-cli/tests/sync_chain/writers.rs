//! A writer (da-o5o6m): it writes entries as a member does, and serves as a
//! reader does, its own ops alone. Only a member relays.

use super::*;

/// `copy` redeeming a writer's invite of `member`'s, with `filter`'s flags,
/// and fed what a session would send it.
fn admit_writer(member: &Copy, copy: &Copy, filter: &[&str]) {
    let args = [&["sync", "new-invite", "--writer"][..], filter].concat();
    let invite = json(&member.ok(&args));
    let secret = invite["secret"].as_str().unwrap();
    let id = copy.id();
    let hello = ["sync", "handshake", &id, "--label", "w", "--addr", "{}"];
    member.ok(&[&hello[..], &["--invite", secret, "--chain", ""]].concat());
    let (ok, text) = feed(member, copy);
    assert!(ok, "{text}");
}

/// Every op `copy` serves from empty is its own.
fn serves_only_its_own(copy: &Copy) {
    let served = copy.ok(&["sync", "pipe", "ops"]);
    assert!(!served.is_empty());
    assert!(
        served.lines().all(|l| json(l)["endpoint"] == copy.id()),
        "{served}"
    );
}

/// A writer's entries reach every member and stay there; it writes no
/// chain record; the chain, `list` and `status` name it; made a reader, it
/// writes nothing.
#[test]
fn a_writers_entries_reach_every_member_and_stay() {
    let (a, b) = chained();
    a.ok(&["occur", "tea", "-t", "-3"]);
    assert!(a.send(&b).0);
    let w = Copy::new();
    admit_writer(&a, &w, &[]);
    assert!(w.has("tea"), "unfiltered, it receives everything");
    w.ok(&["occur", "coffee", "-t", "-2"]);
    let out = w.fails(&["sync", "new-invite"], "");
    assert!(
        out.contains(
            "its writer is a writer in the sync chain: it writes entries, and no chain records"
        ),
        "{out}"
    );

    let (ok, text) = w.send(&a);
    assert!(ok, "{text}");
    assert!(a.send(&b).0);
    for copy in [&a, &b] {
        assert!(copy.has("coffee"));
    }
    // Still there after another round each way: nothing refused it.
    b.ok(&["occur", "juice", "-t", "-1"]);
    assert!(b.send(&a).0 && a.send(&b).0 && a.send(&w).0 && w.send(&a).0);
    for copy in [&a, &b, &w] {
        assert!(copy.has("coffee") && copy.has("juice"));
    }
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), w.hash());

    assert_eq!(member_of(&b, &w.id())["role"], "writer");
    assert!(a.ok(&["sync", "chain", "list"]).contains("w  writer"));
    assert_eq!(
        json(&w.ok(&["sync", "status", "--json"]))["this"]["role"],
        "writer"
    );
    let status = json(&a.ok(&["sync", "status", "--json"]));
    let of = |id: &str| {
        let members = status["members"].as_array().unwrap();
        members.iter().find(|m| m["endpoint"] == id).unwrap()["role"].clone()
    };
    assert_eq!(
        (of(&w.id()), of(&b.id())),
        ("writer".into(), "member".into())
    );

    // `role <WHO> reader`: it writes nothing more.
    a.ok(&["sync", "chain", "role", "w", "reader"]);
    assert!(a.send(&w).0);
    let out = w.fails(&["occur", "cocoa"], "");
    assert!(
        out.contains("this copy is a reader in its sync chain"),
        "{out}"
    );
    a.ok(&["sync", "chain", "role", "w", "writer"]);
    assert!(a.send(&w).0);
    w.ok(&["occur", "cocoa"]);
}

/// A member syncing with a writer receives only the writer's own ops:
/// another member's write the writer holds reaches it from that member, or
/// one that relays, never through the writer.
#[test]
fn a_member_syncing_with_a_writer_receives_only_its_own_ops() {
    let (a, b) = chained();
    let w = Copy::new();
    admit_writer(&a, &w, &[]);
    a.ok(&["occur", "tea", "-t", "-2"]);
    assert!(a.send(&w).0);
    assert!(w.has("tea"));
    w.ok(&["occur", "coffee", "-t", "-1"]);
    serves_only_its_own(&w);

    // b has heard of neither tea nor w: from w it gets coffee alone, held
    // until a's records admit w.
    let (ok, text) = w.send(&b);
    assert!(ok && text.contains("held"), "{text}");
    assert!(!b.has("tea"));
    assert!(a.send(&b).0);
    assert!(b.has("coffee") && b.has("tea"));
    assert!(w.send(&a).0);
    assert_eq!(a.hash(), b.hash());
}

/// A writer with an empty filter holds the chain's records and its own
/// entries, and nothing of the members'; what it writes reaches them.
#[test]
fn a_writer_with_an_empty_filter_holds_only_the_chain_and_its_own_entries() {
    let (a, b) = chained();
    a.ok(&["setup", "tea", "--verb", "occur"]);
    a.ok(&["occur", "tea", "-t", "-3"]);
    let w = Copy::new();
    admit_writer(&a, &w, &["--fields", ""]);
    assert!(!holds_any(&w, "tea"));
    assert_ne!(
        w.query("SELECT COUNT(*) FROM ops WHERE hid = '~chain'"),
        "0"
    );
    assert_eq!(member_of(&w, &b.id())["role"], "member");
    assert_eq!(
        member_of(&a, &w.id())["filter"],
        serde_json::json!({"fields": [], "of": 1})
    );

    w.ok(&["occur", "coffee", "-t", "-2"]);
    assert!(w.send(&a).0 && a.send(&b).0);
    assert!(a.has("coffee") && b.has("coffee"));
    a.ok(&["occur", "tea", "-t", "-1"]);
    b.ok(&["occur", "juice"]);
    assert!(b.send(&a).0);
    let (ok, text) = feed(&a, &w);
    assert!(ok, "{text}");
    assert!(feed(&b, &w).0);
    assert!(!holds_any(&w, "tea") && !holds_any(&w, "juice"));
    assert!(w.has("coffee"));
    let theirs = format!(
        "SELECT COUNT(*) FROM ops WHERE hid != '~chain' AND endpoint != '{}'",
        w.id()
    );
    assert_eq!(w.query(&theirs), "0");
    assert_eq!(verify_with(&a, &w), verify_with(&w, &a));
}
