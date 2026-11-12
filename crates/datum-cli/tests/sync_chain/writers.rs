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
        serde_json::json!({"fields": [], "of": 1, "said": "filtered: 0 of 1 fields"})
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

/// A writer that signs a chain record itself, an invite, a revoke or a role
/// of its own, and pipes it into a member: refused on that member's fold,
/// open chain or locked, and the chain is as it was (da-2j7yr).
#[test]
fn a_writers_forged_acts_are_refused_by_a_member_open_chain_or_locked() {
    for lock in [false, true] {
        let (a, b) = chained();
        let admin = a.key("admin.key");
        if lock {
            a.ok(&["sync", "admin", "add", &admin]);
        }
        let key: &[&str] = if lock { &["--admin-key", &admin] } else { &[] };
        let w = Copy::new();
        admit_writer(&a, &w, key);
        let chain = || a.ok(&["sync", "chain", "list", "--json"]);
        let before = chain();
        for record in [
            serde_json::json!({"type": "invite", "invite_id": "x", "secret_hash": "h",
                "expires": "2099-01-01T00:00:00Z", "caps": {}}),
            serde_json::json!({"type": "revoke", "endpoint": b.id()}),
            serde_json::json!({"type": "role", "endpoint": w.id(), "role": "member"}),
        ] {
            let mut record = record;
            record["chain"] = chain_of(&a).into();
            record["created"] = "2026-09-29T00:00:00Z".into();
            let out = a.run(&["sync", "pipe", "apply"], &forged(&w, &a, record));
            let text = String::from_utf8_lossy(&out.stderr);
            assert!(!out.status.success(), "{text}");
            assert!(
                text.contains("its writer is a writer in the sync chain"),
                "{text}"
            );
        }
        assert_eq!(chain(), before);
        assert_eq!(member_of(&a, &w.id())["role"], "writer");
    }
}

/// A writer's bounds (da-mogvf, vividn, 2026-11-12: «Both limits»): it
/// writes only its fields, and their specs, no alias, and changes only the
/// entries it created. Its own copy refuses the rest as it is typed; an op
/// of it signed anyway is refused on a member's fold and named in `sync
/// status`. A member is unbounded.
#[test]
fn a_writer_writes_only_its_fields_and_changes_only_its_own_entries() {
    let (a, b) = chained();
    a.ok(&["occur", "mood", "-y", "-t", "9:00"]);
    a.ok(&["occur", "tea", "-y", "-t", "9:30"]);
    assert!(a.send(&b).0);
    let w = Copy::new();
    admit_writer(&a, &w, &["--fields", "mood"]);
    let hid = |c: &Copy, f: &str| c.query(&format!("SELECT hid FROM entries WHERE field = '{f}'"));
    let theirs = hid(&a, "mood");

    for (args, why) in [
        (
            &["occur", "tea", "-y", "-t", "10:00"][..],
            "this copy is a writer of «mood» in the sync chain, and this is «tea»",
        ),
        (
            &["update", &theirs, "x=1"],
            "this copy is a writer in the sync chain: it changes only the entries it created",
        ),
        (
            &["alias", "set", "m", "occur mood"],
            "no alias or other entry about the database",
        ),
    ] {
        let out = w.fails(args, "");
        assert!(out.contains(why), "{args:?}: {out}");
    }

    // Its own, in its fields, and the spec of a new field it writes: applied.
    w.ok(&["occur", "mood", "-y", "-t", "11:00"]);
    let mine = w.query(&format!(
        "SELECT hid FROM entries WHERE field = 'mood' AND hid != '{theirs}'"
    ));
    w.ok(&["update", &mine, "x=2"]);
    let (ok, text) = w.send(&a);
    assert!(ok, "{text}");
    assert_eq!(
        a.query(&format!(
            "SELECT json_extract(data, '$.x') FROM entries WHERE hid = '{mine}'"
        )),
        "2"
    );

    // Signed anyway: a tea entry of its own, and an edit of a's mood entry.
    let body = |sql: &str| json(&a.query(&format!("SELECT body FROM ops WHERE {sql}")));
    let mut tea = body(&format!("hid = '{}' AND kind = 'create'", hid(&a, "tea")));
    tea["hid"] = "wtea0000000000000000000000".into();
    tea["slug"] = "tea:2026-01-01T10:00:00.000Z".into();
    tea["effect"]["create"]["hid"] = tea["hid"].clone();
    tea["effect"]["create"]["slug"] = tea["slug"].clone();
    let out = a.run(&["sync", "pipe", "apply"], &signed_as(&w, &a, tea));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("a writer of «mood» in the sync chain, and this is «tea»"),
        "{text}"
    );

    let mut edit = body(&format!("hid = '{mine}' AND kind = 'update'"));
    edit["hid"] = theirs.clone().into();
    edit["slug"] = a
        .query(&format!("SELECT slug FROM entries WHERE hid = '{theirs}'"))
        .into();
    let out = a.run(&["sync", "pipe", "apply"], &signed_as(&w, &a, edit));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("it changes only the entries it created"),
        "{text}"
    );
    assert_eq!(
        a.query(&format!(
            "SELECT COUNT(*) FROM entries WHERE hid = 'wtea0000000000000000000000'
             OR (hid = '{theirs}' AND json_extract(data, '$.x') IS NOT NULL)"
        )),
        "0"
    );
    let notices = json(&a.ok(&["sync", "status", "--json"]))["notices"].to_string();
    assert!(
        notices.contains("changes from w refused (a writer of «mood» in the sync chain"),
        "{notices}"
    );

    // A member edits a's entry, and it reaches a.
    b.ok(&["update", &theirs, "x=3"]);
    assert!(b.send(&a).0);
    assert_eq!(
        a.query(&format!(
            "SELECT json_extract(data, '$.x') FROM entries WHERE hid = '{theirs}'"
        )),
        "3"
    );
}
