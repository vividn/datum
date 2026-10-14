//! `sync chain purge --data | --all` (da-qzrj9): the chain-wide hard purge,
//! carried out by each copy as it comes to hold the order.

use super::*;

/// What `copy` holds of data: entries, entry ops with a body, history rows.
fn data_of(copy: &Copy) -> [String; 3] {
    [
        "SELECT COUNT(*) FROM entries",
        "SELECT COUNT(*) FROM ops WHERE hid != '~chain' AND body != ''",
        "SELECT COUNT(*) FROM entry_history",
    ]
    .map(|sql| copy.query(sql))
}

const EMPTY: [&str; 3] = ["0", "0", "0"];

/// Each member's endpoint and whether it is revoked, as `copy` holds them.
fn members_of(copy: &Copy) -> Vec<(String, bool)> {
    let list = json(&copy.ok(&["sync", "chain", "list", "--json"]));
    let members = list["members"].as_array().unwrap().iter();
    members
        .map(|m| {
            (
                m["endpoint"].as_str().unwrap().into(),
                m["revoked"].is_string(),
            )
        })
        .collect()
}

/// `sync chain purge --data` (da-qzrj9, vividn 10-12): a signed order each
/// copy carries out as it comes to hold it. a orders it and b, online,
/// empties at its next sync. c was offline: it missed a's last entry and
/// logged one of its own, and empties when it next connects — to b, which
/// carried the order out, so the seqs b took of a's ops reach c as stubs it
/// would otherwise wait on forever. The confirmation said who learns when;
/// d, revoked, is never told. The members stay, a's backup restores what
/// went, and what is written afterwards syncs as ever.
#[test]
fn a_data_purge_empties_every_copy_as_it_connects_and_the_chain_syncs_on() {
    let (a, b) = chained();
    let (c, d) = (Copy::new(), Copy::new());
    admit(&a, &c, "c", None);
    admit(&a, &d, "d", None);
    a.ok(&["occur", "tea"]);
    for other in [&b, &c, &d] {
        assert!(a.send(other).0);
    }
    a.ok(&["sync", "chain", "revoke", &d.id()[..10]]);
    a.ok(&["sync", "record-session", &b.id(), "--verify", "match"]);
    // As if b had admitted c: a never met it.
    a.exec(&format!(
        "DELETE FROM sync_peers WHERE endpoint_id = '{}'",
        c.id()
    ));
    a.ok(&["occur", "coffee"]);
    assert!(a.send(&b).0);
    c.ok(&["occur", "juice"]);
    let members = members_of(&a);

    // Without --yes: who learns of it when, and nothing changes.
    let said = a.ok(&["sync", "chain", "purge", "--data"]);
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    for line in [
        format!("b ({}) last synced with this copy {today}T", &b.id()[..10]),
        format!(
            "c ({}) has not synced with this copy: it purges when it next connects",
            &c.id()[..10]
        ),
        format!(
            "d ({}) is revoked: it is never told, and keeps what it holds",
            &d.id()[..10]
        ),
        "nothing changed: `datum sync chain purge --data --yes` does it".into(),
    ] {
        assert!(said.contains(&line), "{line}\n{said}");
    }
    assert!(a.has("tea") && a.has("coffee"));

    let done = a.ok(&["sync", "chain", "purge", "--data", "--yes"]);
    assert_eq!(data_of(&a), EMPTY, "{done}");
    // A whole earlier copy beside b's file goes with what it holds.
    let snapshot = b
        .tmp
        .path()
        .join("data.db.pre-v1-to-v2-20260101T000000Z.bak");
    std::fs::write(&snapshot, "tea").unwrap();
    assert!(a.send(&b).0);
    assert_eq!(data_of(&b), EMPTY);
    assert!(!snapshot.exists());
    // c's entry, made before the order, is taken as it reaches b.
    assert!(c.send(&b).0);
    assert_eq!(data_of(&b), EMPTY);
    let (ok, text) = b.send(&c);
    assert!(ok, "{text}");
    assert_eq!(data_of(&c), EMPTY, "c's own entry goes too");
    for (from, to) in [(&c, &a), (&c, &b), (&a, &c)] {
        let (ok, text) = from.send(to);
        assert!(ok, "{text}");
    }
    for copy in [&a, &b, &c] {
        assert_eq!(members_of(copy), members);
        assert_eq!(data_of(copy), EMPTY);
    }

    // Afterwards a new entry on any copy syncs to the others.
    c.ok(&["occur", "later"]);
    for (from, to) in [(&c, &b), (&b, &a)] {
        assert!(from.send(to).0);
    }
    assert!(a.has("later") && b.has("later"));
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), c.hash());

    // d is refused a session, so never told, and keeps what it holds.
    let hello = ["sync", "handshake", &d.id(), "--label", "d", "--addr", "{}"];
    a.fails(&hello, "");
    assert!(d.has("tea"));

    // a's backup, taken before it purged, restores what went.
    let backup = done.rsplit("backup: ").next().unwrap().trim();
    let fresh = Copy::new();
    fresh.ok(&["db", "restore", backup, "--yes"]);
    assert!(fresh.has("tea") && fresh.has("coffee") && !fresh.has("juice"));
}

/// `sync chain purge --all` (da-qzrj9): the data goes and the chain
/// dissolves — every copy reads «in no sync chain», the offline one too once
/// it connects, here to b, which passes the order on. What one writes
/// afterwards reaches no other, the fold takes no record after it, and an
/// invite starts a new chain with a new key.
#[test]
fn an_all_purge_dissolves_the_chain_on_every_copy_as_it_connects() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    a.ok(&["occur", "tea"]);
    for other in [&b, &c] {
        assert!(a.send(other).0);
    }
    let said = a.ok(&["sync", "chain", "purge", "--all"]);
    assert!(said.contains("the chain dissolves"), "{said}");
    a.ok(&["sync", "chain", "purge", "--all", "--yes"]);
    assert!(b.has("tea") && c.has("tea"));
    assert!(a.send(&b).0);
    assert!(b.send(&c).0);
    for copy in [&a, &b, &c] {
        assert_eq!(data_of(copy), EMPTY);
        for cmd in [
            &["sync", "chain", "list"][..],
            &["sync", "admin", "list"],
            &["sync", "status"],
        ] {
            let out = copy.ok(cmd);
            assert!(out.contains("this copy is in no sync chain"), "{out}");
        }
        for cmd in [
            &["sync", "chain", "list", "--json"][..],
            &["sync", "admin", "list", "--json"],
        ] {
            let out = json(&copy.ok(cmd));
            assert!(
                out["chain"].is_null() && out["dissolved"].is_string(),
                "{out}"
            );
            assert_eq!(out["admins"], serde_json::json!([]), "{out}");
        }
        let status = json(&copy.ok(&["sync", "status", "--json"]));
        assert!(status["chain"].is_null() && status["dissolved"].is_string());
        assert_eq!(
            (&status["open"], &status["members"]),
            (&serde_json::json!(false), &serde_json::json!([]))
        );
    }

    a.ok(&["occur", "later"]);
    assert!(a.send(&b).0);
    assert!(!b.has("later"));
    let refused = a.fails(&["sync", "chain", "revoke", &b.id()[..10]], "");
    assert!(refused.contains("dissolved by a purge"), "{refused}");

    let old = a.id();
    a.ok(&["sync", "new-invite"]);
    assert_ne!(a.id(), old);
    assert!(a.ok(&["sync", "chain", "list"]).contains("sync chain"));
    assert!(a.has("later"));
    // A dissolved copy is in no chain to `db purge` either: nothing to
    // leave, nothing to rejoin.
    c.ok(&["db", "purge", "--yes"]);
}

/// In a locked chain a purge is an admin act (da-qzrj9): any member orders
/// it with the admin key, and none without.
#[test]
fn in_a_locked_chain_a_purge_takes_the_admin_key() {
    let (a, b, admin) = locked();
    b.ok(&["occur", "tea"]);
    let refused = b.fails(&["sync", "chain", "purge", "--data", "--yes"], "");
    assert!(refused.contains("no approval code"), "{refused}");
    assert!(b.has("tea"));
    // Refused, it leaves no backup of a purge that never happened.
    let beside = std::fs::read_dir(b.tmp.path()).unwrap();
    let names: Vec<_> = beside.map(|f| f.unwrap().file_name()).collect();
    assert!(
        !names
            .iter()
            .any(|n| n.to_string_lossy().contains(".purge-")),
        "{names:?}"
    );
    b.ok(&[
        "sync",
        "chain",
        "purge",
        "--data",
        "--yes",
        "--admin-key",
        &admin,
    ]);
    assert_eq!(data_of(&b), EMPTY);
    assert!(b.send(&a).0);
    assert_eq!(data_of(&a), EMPTY);
}

/// A filtered reader's holes lie below what it holds, by design: a purge
/// empties it all the same, with no stub sent or waited on, and what is
/// written afterwards reaches it (da-qzrj9).
#[test]
fn a_filtered_reader_with_holes_is_purged_and_reads_on() {
    let (a, _b) = chained();
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    for field in ["coffee", "tea", "coffee", "tea", "coffee"] {
        a.ok(&["occur", field]);
    }
    assert!(feed(&a, &r).0);
    let held = |sql: &str| r.query(&format!("SELECT COUNT(*) FROM ops WHERE {sql}"));
    let gaps = "seq < (SELECT MAX(seq) FROM ops o WHERE o.endpoint = ops.endpoint)";
    assert_ne!(held(&format!("hid != '~chain' AND {gaps}")), "0");

    a.ok(&["sync", "chain", "purge", "--data", "--yes"]);
    a.ok(&["occur", "tea"]);
    a.ok(&["occur", "coffee", "note=after"]);
    assert!(feed(&a, &r).0);
    let rows = |c: &Copy, of: &str| {
        c.query(&format!(
            "SELECT group_concat(slug, ' ') FROM (SELECT slug FROM entries {of} ORDER BY slug)"
        ))
    };
    let coffee = "WHERE field = 'coffee' OR slug = 'spec:coffee'";
    assert_eq!(rows(&a, coffee).split(' ').count(), 2);
    assert_eq!(rows(&r, ""), rows(&a, coffee));
    assert!(!holds_any(&r, "tea"));
    assert_eq!(held("applied = 0"), "0");
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));

    // Its filter changing reads past the stubs it holds, and keeps them.
    let stubs = held("applied = 3");
    assert_ne!(stubs, "0");
    a.ok(&[
        "sync",
        "chain",
        "role",
        "r",
        "reader",
        "--fields",
        "coffee,tea",
    ]);
    let (ok, text) = feed(&a, &r);
    assert!(ok, "{text}");
    assert!(holds_any(&r, "tea"));
    assert_eq!(held("applied = 3"), stubs);
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
}
