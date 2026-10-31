//! `sync chain leave`, and a copy of one chain refusing another's.

use super::*;

/// `sync chain leave` (da-h7pnc): a chain of one is left with every entry
/// kept, and those entries go to the next copy it syncs with.
#[test]
fn a_chain_of_one_is_left_with_its_entries() {
    let a = Copy::new();
    a.ok(&["occur", "tea"]);
    a.ok(&["sync", "new-invite"]);
    a.ok(&["sync", "chain", "leave", "--dry-run"]);
    assert_ne!(a.query("SELECT COUNT(*) FROM ops"), "0");
    let left = a.ok(&["sync", "chain", "leave"]);
    assert!(
        left.contains("this copy is in none, its entries kept"),
        "{left}"
    );
    assert_eq!(a.query("SELECT COUNT(*) FROM ops"), "0");
    assert!(a.has("tea"));
    let status = json(&a.ok(&["sync", "status", "--json"]));
    assert!(status["chain"].is_null(), "{status}");
    assert!(a
        .fails(&["sync", "chain", "leave"], "")
        .contains("in no sync chain"));
    // In no chain, it joins another, its entries with it.
    let b = Copy::new();
    b.ok(&["occur", "coffee"]);
    admit(&b, &a, "a", None);
    assert!(a.send(&b).0);
    assert!(a.has("coffee") && b.has("tea"));
    assert_eq!(a.hash(), b.hash());
}

/// The leave `join` makes on its own (`--alone`, da-h7pnc) is only a
/// copy's holding nothing but its own data: another member, or a revoked
/// one's data, refuses it, and nothing goes.
/// Being in no chain is a state, which `--json` answers as `sync status
/// --json` does, and writing nothing (da-y6oh6).
/// The bare forms still refuse, naming both ways out of it.
#[test]
fn a_chainless_copy_lists_its_chain_and_admins_as_json() {
    let a = Copy::new();
    a.ok(&["occur", "tea"]);
    let local = a.query("SELECT * FROM sync_local");
    let list = json(&a.ok(&["sync", "chain", "list", "--json"]));
    assert_eq!(
        list,
        serde_json::json!({"chain": null, "admins": [], "members": []})
    );
    let admins = json(&a.ok(&["sync", "admin", "list", "--json"]));
    assert_eq!(admins, serde_json::json!({"chain": null, "admins": []}));
    assert_eq!(a.query("SELECT * FROM sync_local"), local);
    for argv in [&["sync", "chain", "list"][..], &["sync", "admin", "list"]] {
        assert!(a.fails(argv, "").contains("in no sync chain"), "{argv:?}");
    }
}

#[test]
fn the_leave_join_makes_is_refused_a_copy_holding_others_data() {
    let (a, b) = chained();
    b.ok(&["occur", "tea"]);
    assert!(b.send(&a).0);
    let ops = a.query("SELECT COUNT(*) FROM ops");
    for argv in [
        &["sync", "chain", "leave", "--alone", "--dry-run"][..],
        &["sync", "chain", "leave", "--alone"],
    ] {
        let refused = a.fails(argv, "");
        assert!(refused.contains("has 1 other member(s)"), "{refused}");
        assert!(refused.contains("only a copy holding nothing but its own data"));
    }
    assert_eq!(a.query("SELECT COUNT(*) FROM ops"), ops);
    a.ok(&["sync", "chain", "expel", "b"]);
    let refused = a.fails(&["sync", "chain", "leave", "--alone"], "");
    assert!(refused.contains("holds other copies' data"), "{refused}");
    assert!(a.has("tea"));
}

/// `sync chain leave` works from any chain (da-j8c4l): from a chain of
/// three, the copy keeps every entry, its own and those it received, under
/// a new key with no peer; an invite from it starts a new chain with it the
/// first member; and a member of the old chain can't sync with it: each
/// one's handshake refuses the other (the pipe has none).
///
/// It tells the chain first (da-dsob1): it signs its departure, and leaves
/// once a member's last session ended holding it — refused till then, and
/// writing nothing. The member lists it as left and no more in its status,
/// and so does the third, which hears it from that member; its entries stay
/// on all three.
#[test]
fn leaving_a_chain_of_three_keeps_every_entry_and_starts_afresh() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    a.ok(&["occur", "coffee"]);
    b.ok(&["occur", "tea"]);
    assert!(a.send(&b).0 && b.send(&a).0 && a.send(&c).0);
    let old = chain_of(&b);
    let (old_id, entries) = (b.id(), b.query("SELECT COUNT(*) FROM entries"));
    b.ok(&["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"]);
    assert_ne!(b.query("SELECT COUNT(*) FROM sync_peers"), "0");

    let refused = b.fails(&["sync", "chain", "leave"], "");
    assert!(
        refused.contains("no member holds this copy's departure"),
        "{refused}"
    );
    let leave = "SELECT COUNT(*) FROM ops WHERE json_extract(body, '$.effect.type') = 'leave'";
    assert_eq!(b.query(leave), "1");
    let status = b.ok(&["sync", "status"]);
    assert!(
        status.contains("this copy signed its departure"),
        "{status}"
    );
    let write = b.fails(&["occur", "late"], "");
    assert!(write.contains("signed its departure"), "{write}");
    // A transport's session with a ends holding it.
    assert!(b.send(&a).0);
    let vector = a.ok(&["sync", "pipe", "vector"]);
    b.ok(&["sync", "record-session", &a.id(), "--vector", vector.trim()]);
    let left = b.ok(&["sync", "chain", "leave"]);
    assert!(left.contains(&format!("left sync chain {old}")), "{left}");
    assert!(left.contains("its departure is with "), "{left}");
    assert!(left.contains("new endpoint key"), "{left}");
    assert!(a.send(&c).0);
    for x in [&a, &c] {
        // A member that left is a former one (da-73gyz): hidden, counted.
        let list = x.ok(&["sync", "chain", "list"]);
        assert!(!list.contains(&old_id[..10]), "{list}");
        assert!(list.contains("1 former member hidden"), "{list}");
        let list = x.ok(&["sync", "chain", "list", "--show-revoked"]);
        let row = list.lines().find(|l| l.contains(&old_id[..10])).unwrap();
        assert!(row.contains("  LEFT "), "{list}");
        let status = json(&x.ok(&["sync", "status", "--json"]));
        assert!(!status["members"].to_string().contains(&old_id), "{status}");
        // `status` has no row for it to hide, so it counts none.
        for flag in [&[][..], &["--show-revoked"]] {
            let status = x.ok(&[&["sync", "status"][..], flag].concat());
            let mut rows = status.lines().filter(|l| !l.starts_with('⚠'));
            assert!(!rows.any(|l| l.contains(&old_id[..10])), "{status}");
            assert!(!status.contains("former member"), "{status}");
        }
        assert!(x.has("tea"));
    }
    assert_eq!(b.query("SELECT COUNT(*) FROM entries"), entries);
    assert!(b.has("coffee") && b.has("tea"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
    assert_eq!(b.query("SELECT COUNT(*) FROM sync_peers"), "0");
    assert_ne!(b.id(), old_id);

    // A new chain, b its first member, vouching for all it holds.
    b.ok(&["sync", "new-invite"]);
    let new = chain_of(&b);
    assert_ne!(new, old);
    let members = json(&b.ok(&["sync", "chain", "list", "--json"]))["members"].clone();
    assert_eq!(members.as_array().unwrap().len(), 1, "{members}");
    let d = Copy::new();
    admit(&b, &d, "d", None);
    assert!(d.has("coffee") && d.has("tea"));

    // The old chain's members and b refuse each other.
    let hello = ["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"];
    assert!(b
        .fails(&hello, "")
        .contains("not a member of this sync chain"));
    let hello = ["sync", "handshake", &b.id(), "--label", "b", "--addr", "{}"];
    assert!(a
        .fails(&hello, "")
        .contains("not a member of this sync chain"));
    assert_eq!(chain_of(&a), old);
}

/// A copy in a chain refuses the records of another (da-d8ort): the fold
/// takes the earliest genesis it holds, so an older chain's ops piped in
/// would make that chain this copy's, and refuse its own.
#[test]
fn an_older_chains_records_piped_in_are_refused() {
    let (a, _) = chained();
    a.ok(&["occur", "tea"]);
    let b = Copy::new();
    b.ok(&["occur", "coffee"]);
    b.ok(&["sync", "new-invite"]);
    let list = |x: &Copy| json(&x.ok(&["sync", "chain", "list", "--json"]));
    let (x, y) = (list(&a)["chain"].clone(), list(&b)["chain"].clone());
    assert_ne!(x, y);

    let (ok, out) = a.send(&b);
    assert!(
        !ok && out.contains(&format!(
            "a record of another sync chain ({})",
            x.as_str().unwrap()
        )),
        "{out}"
    );
    let after = list(&b);
    assert_eq!(after["chain"], y, "{after}");
    assert_eq!(after["members"].as_array().unwrap().len(), 1, "{after}");
    assert!(b.has("coffee") && !b.has("tea"));
}
