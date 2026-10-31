//! A reader: it receives, writes nothing and relays nothing.

use super::*;

/// `copy` redeeming a reader's invite of `member`'s (da-9yvb6), and sent the
/// chain: whether that applied cleanly, and what it said.
fn admit_reader(
    member: &Copy,
    copy: &Copy,
    label: &str,
    admin_key: Option<&str>,
) -> (bool, String) {
    let mut args = vec!["sync", "new-invite", "--reader"];
    args.extend(admin_key.map(|k| ["--admin-key", k]).into_iter().flatten());
    let invite = json(&member.ok(&args));
    let secret = invite["secret"].as_str().unwrap();
    let id = copy.id();
    let hello = ["sync", "handshake", &id, "--label", label, "--addr", "{}"];
    member.ok(&[&hello[..], &["--invite", secret, "--chain", ""]].concat());
    member.send(copy)
}

const READ_ONLY: &str = "this copy is a reader in its sync chain: it receives, and can't write";

/// A reader receives every write; its copy refuses its own, and its chain
/// acts but a describe of itself; it serves its own ops alone, so nothing
/// reaches another member through it; `chain list` and `status` say so.
#[test]
fn a_reader_receives_every_write_writes_nothing_and_relays_nothing() {
    let (a, b) = chained();
    let r = Copy::new();
    a.ok(&["occur", "tea"]);
    assert!(admit_reader(&a, &r, "r", None).0);
    assert!(r.has("tea"));
    let out = r.fails(&["occur", "coffee"], "");
    assert!(out.contains(READ_ONLY), "{out}");
    assert!(!r.has("coffee"));
    let out = r.fails(&["sync", "new-invite"], "");
    assert!(
        out.contains("its writer is a reader in the sync chain"),
        "{out}"
    );
    b.ok(&["occur", "juice"]);
    assert!(b.send(&a).0 && a.send(&r).0);
    assert!(r.has("juice"));
    assert_eq!(a.hash(), r.hash());

    // It describes itself, and serves that alone: b, which has not heard
    // of r from a, holds r's describe until a's records reach it.
    r.ok(&["sync", "chain", "rename", "phone"]);
    let served = r.ok(&["sync", "pipe", "ops"]);
    assert!(!served.is_empty());
    assert!(
        served.lines().all(|l| json(l)["endpoint"] == r.id()),
        "{served}"
    );
    let (ok, text) = r.send(&b);
    assert!(ok && text.contains("held 1"), "{text}");
    assert!(a.send(&b).0);
    assert_eq!(member_of(&b, &r.id())["label"], "phone");

    assert_eq!(member_of(&b, &r.id())["role"], "reader");
    assert_eq!(member_of(&b, &a.id())["role"], "member");
    assert!(b.ok(&["sync", "chain", "list"]).contains("phone  reader"));
    assert_eq!(
        json(&r.ok(&["sync", "status", "--json"]))["this"]["reader"],
        true
    );
    let status = json(&a.ok(&["sync", "status", "--json"]));
    let of = |id: &str| {
        status["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["endpoint"] == id)
            .unwrap()["reader"]
            .clone()
    };
    assert_eq!((of(&r.id()), of(&b.id())), (true.into(), false.into()));
}

/// A reader's entry op, and a forged invite or revoke, are refused by the
/// fold on every copy, open chain or locked.
#[test]
fn a_readers_entry_op_and_forged_acts_are_refused_open_chain_or_locked() {
    for lock in [false, true] {
        let (a, b) = chained();
        let admin = a.key("admin.key");
        if lock {
            a.ok(&["sync", "admin", "add", &admin]);
        }
        let key = lock.then_some(admin.as_str());
        let chain_id = a.query(
            "SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1",
        );
        let r = Copy::new();
        assert!(admit_reader(&a, &r, "r", key).0);
        for record in [
            serde_json::json!({"type": "revoke", "endpoint": b.id()}),
            serde_json::json!({"type": "invite", "invite_id": "x", "secret_hash": "h",
                "expires": "2099-01-01T00:00:00Z", "caps": {}}),
        ] {
            let mut record = record;
            record["chain"] = chain_id.clone().into();
            record["created"] = "2026-09-29T00:00:00Z".into();
            let out = a.run(&["sync", "pipe", "apply"], &forged(&r, &a, record));
            let text = String::from_utf8_lossy(&out.stderr);
            assert!(!out.status.success(), "{text}");
            assert!(
                text.contains("its writer is a reader in the sync chain"),
                "{text}"
            );
        }
        assert!(member_of(&a, &b.id())["revoked"].is_null());

        // An entry op, made before it joined: refused on every copy, its
        // own included, which still serves it for each copy to refuse.
        let r = Copy::new();
        r.ok(&["occur", "before"]);
        let (ok, text) = admit_reader(&a, &r, "r2", key);
        assert!(!ok && text.contains("a reader in the sync chain"), "{text}");
        assert!(!r.has("before"));
        let (ok, text) = r.send(&a);
        assert!(!ok && text.contains("a reader in the sync chain"), "{text}");
        assert!(!a.has("before"));
    }
}

/// A reader's status counts what it will send a member: its own ops alone,
/// never another member's, which it does not relay, nor an entry no op
/// covers, which it signs no baseline for (da-b2krf).
#[test]
fn a_readers_status_counts_its_own_ops_alone_as_to_send() {
    let (a, b) = chained();
    let r = Copy::new();
    a.ok(&["occur", "tea"]);
    assert!(admit_reader(&a, &r, "r", None).0);
    b.ok(&["occur", "juice"]);
    assert!(b.send(&a).0 && a.send(&r).0);
    // r has an address for each: with none, nothing waits whoever asks
    // (da-fly3f).
    for (m, label) in [(&a, "a"), (&b, "b")] {
        let hello = ["sync", "handshake", &m.id(), "--label", label];
        r.ok(&[&hello[..], &["--addr", "{}", "--dialed"]].concat());
    }
    // r holds a's and b's ops, and has no vector of b's: none are r's to send.
    assert!(json(&r.ok(&["sync", "pipe", "vector"]))[b.id()].as_i64() > Some(0));
    assert_eq!(status_of(&r, &b)["to_send"], 0);
    assert_eq!(status_of(&r, &a)["to_send"], 0);
    let hid = r.query("SELECT MIN(hid) FROM entries WHERE field = 'tea'");
    r.exec(&format!("DELETE FROM ops WHERE hid = '{hid}'"));
    assert_eq!(status_of(&r, &b)["to_send"], 0);
    // Its describe of itself is its own.
    r.ok(&["sync", "chain", "rename", "phone"]);
    assert_eq!(status_of(&r, &b)["to_send"], 1);
    assert_eq!(status_of(&r, &a)["to_send"], 1);
    // A member still counts all it holds.
    assert!(status_of(&a, &b)["to_send"].as_i64() > Some(1));
}

/// Roles change at a record's HLC, either way: a reader made a member
/// writes; a member made a reader keeps what it wrote before, and what it
/// wrote after is refused everywhere, its own copy included once it hears.
#[test]
fn a_reader_made_a_member_writes_and_a_member_made_a_reader_does_not() {
    let (a, b) = chained();
    let r = Copy::new();
    assert!(admit_reader(&a, &r, "r", None).0);
    assert!(a
        .ok(&["sync", "chain", "role", "r", "member"])
        .contains("r is a member"));
    assert!(a.send(&r).0);
    r.ok(&["occur", "promoted"]);
    assert!(r.send(&a).0 && a.has("promoted"));

    b.ok(&["occur", "before"]);
    assert!(b.send(&a).0);
    a.ok(&["sync", "chain", "role", "b", "reader"]);
    let again = a.fails(&["sync", "chain", "role", "b", "reader"], "");
    assert!(again.contains("b is already a reader"), "{again}");
    b.ok(&["occur", "after"]);
    let (ok, text) = b.send(&a);
    assert!(!ok && text.contains("a reader in the sync chain"), "{text}");
    assert!(a.has("before") && !a.has("after"));
    let (ok, text) = a.send(&b);
    assert!(!ok && text.contains("a reader in the sync chain"), "{text}");
    assert!(b.has("before") && !b.has("after"));
    let out = b.fails(&["occur", "more"], "");
    assert!(out.contains(READ_ONLY), "{out}");
    assert_eq!(a.hash(), b.hash());
}

/// Demoted and promoted back: what it wrote while a reader stays refused,
/// and what it writes after applies on every copy, a third that never met
/// it included — its refused op does not hold back the ones after it.
#[test]
fn a_member_made_a_reader_and_a_member_again_writes_on_every_copy() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    a.ok(&["sync", "chain", "role", "b", "reader"]);
    b.ok(&["occur", "after"]);
    let (ok, text) = b.send(&a);
    assert!(!ok && text.contains("a reader in the sync chain"), "{text}");
    a.ok(&["sync", "chain", "role", "b", "member"]);
    let _ = a.send(&b);
    b.ok(&["occur", "again"]);
    let (ok, text) = b.send(&a);
    assert!(ok, "{text}");
    assert!(a.has("again") && !a.has("after"));
    let _ = a.send(&c);
    assert!(c.has("again") && !c.has("after"));
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), c.hash());
}
