//! The handshake a served copy answers: an invite's id, a refusal's kind, and
//! who it admits (da-2bvxq).

use super::*;

/// `new-invite` prints the invite's id, which its redeemer's `via` names:
/// how datum.day's invite panel hears that its invite was used (da-2bvxq).
#[test]
fn new_invite_prints_the_id_its_redeemer_is_admitted_via() {
    let (a, b) = (Copy::new(), Copy::new());
    let invite = json(&a.ok(&["sync", "new-invite"]));
    let (secret, id) = (
        invite["secret"].as_str().unwrap(),
        invite["id"].as_str().unwrap(),
    );
    let hello = ["sync", "handshake", &b.id(), "--label", "b", "--addr", "{}"];
    let out = a.run(
        &[&hello[..], &["--invite", secret, "--chain", ""]].concat(),
        "",
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let list = json(&a.ok(&["sync", "chain", "list", "--json"]));
    let joined = list["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["label"] == "b")
        .unwrap();
    assert_eq!(joined["via"], format!("invite {id}"));
}

/// A refusal of a kind a redeemer acts on prints the kind on stdout, which
/// is how datum.day's store hears it (da-b6xh6); one of no kind prints none.
#[test]
fn a_handshake_refusal_prints_its_kind() {
    let a = Copy::new();
    let invite = json(&a.ok(&["sync", "new-invite"]));
    let secret = invite["secret"].as_str().unwrap();
    let hello = |c: &Copy, secret: &str| {
        let id = c.id();
        let args = ["sync", "handshake", &id, "--label", "c", "--addr", "{}"];
        a.run(
            &[&args[..], &["--invite", secret, "--chain", ""]].concat(),
            "",
        )
    };
    let (c, d) = (Copy::new(), Copy::new());
    assert!(hello(&c, secret).status.success());
    let used = hello(&d, secret);
    let unknown = hello(&d, "not-an-invite");
    let plain = |id: &str| {
        a.run(
            &["sync", "handshake", id, "--label", "d", "--addr", "{}"],
            "",
        )
    };
    let (stranger, own) = (plain(&d.id()), plain(&a.id()));
    a.ok(&["sync", "pause"]);
    let paused = hello(&d, secret);
    for (out, kind) in [
        (used, "\"invite-used\""),
        (unknown, "\"unknown-invite\""),
        (stranger, "\"not-member\""),
        (own, ""),
        (paused, "\"paused\""),
    ] {
        assert!(!out.status.success(), "{kind}");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), kind);
    }
}

/// A member admitted since this copy last synced is refused by name, with
/// what would fix it, and admitted once it shows its chain's records
/// (da-2u95o) — as datum.day's store does it: `sync pipe records` on the
/// one, `sync handshake --records` on the other. The admission is past an
/// entry of the inviter's this copy lacks, and nothing shown is kept: the
/// session brings it.
#[test]
fn a_handshake_admits_a_member_it_has_not_heard_of_by_its_records() {
    let (a, b) = chained();
    a.ok(&["occur", "tea"]);
    let c = Copy::new();
    admit(&a, &c, "c", None);
    let id = c.id();
    let hello = ["sync", "handshake", &id, "--label", "c", "--addr", "{}"];
    let refused = b.fails(&hello, "");
    let fix = "not a member of this sync chain as b holds it: if c has joined, sync b with \
               the copy that invited it";
    assert!(refused.contains(fix), "{refused}");
    let hello = [&hello[..], &["--records"]].concat();
    let held = b.ok(&["sync", "pipe", "records"]);
    assert!(b.fails(&hello, &held).contains(fix));

    let records = c.ok(&["sync", "pipe", "records"]);
    assert_eq!(b.ok_in(&hello, &records).trim(), "added");
    assert_eq!(b.ok(&["sync", "pipe", "records"]), held);
    assert!(c.send(&b).0);
    assert_eq!(member_of(&b, &id)["label"], "c");
    assert_eq!(b.ok(&hello[..7]).trim(), "known");
    assert!(b.has("tea"));
}

/// A copy belongs to one chain (da-mepav): a redeemer in another chain, or
/// one that does not say, is refused at the admitting member's handshake,
/// and nothing of it is admitted.
#[test]
fn a_redeemer_in_another_chain_or_not_saying_is_refused() {
    let (a, b) = chained();
    let theirs = json(&a.ok(&["sync", "chain", "list", "--json"]))["chain"]
        .as_str()
        .unwrap()
        .to_string();
    let other = Copy::new();
    other.ok(&["sync", "new-invite"]);
    let other_chain = json(&other.ok(&["sync", "chain", "list", "--json"]))["chain"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(theirs, other_chain);

    let refused = redeem(&other, &b, "b", None, &["--chain", &theirs]);
    let text = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "{text}");
    assert!(
        text.contains(&format!(
            "b is in another sync chain ({theirs}); a copy belongs to one chain"
        )),
        "{text}"
    );
    let silent = redeem(&other, &Copy::new(), "old", None, &[]);
    let text = String::from_utf8_lossy(&silent.stderr);
    assert!(!silent.status.success(), "{text}");
    assert!(
        text.contains("old does not say which sync chain it is in"),
        "{text}"
    );
    let members = json(&other.ok(&["sync", "chain", "list", "--json"]))["members"].clone();
    assert_eq!(members.as_array().unwrap().len(), 1, "{members}");

    // In no chain, it is admitted.
    admit(&other, &Copy::new(), "fresh", None);
}
