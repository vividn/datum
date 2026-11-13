//! A standing admission (da-zkwiu): a device's fresh copies are each
//! admitted by its key, each replacing the last, until it is revoked or
//! expires.

use super::*;
use datum_core::db::chain;

const DEVICE: [u8; 32] = [3; 32];

/// The device key `DEVICE` signs as, in hex.
fn device_key() -> String {
    chain::bind_device(&DEVICE, "", "").0
}

/// `rec` written on `copy` as `sync approve` writes it, `code` its signature.
fn approve(copy: &Copy, mut rec: serde_json::Value, code: &str) -> Output {
    rec["chain"] = chain_of(copy).into();
    rec["created"] = "2026-11-12T00:00:00Z".into();
    let code = match code {
        "" => "unsigned".to_string(),
        key => chain::sign_ed25519(
            &rec,
            &chain::read_admin_secret(&std::fs::read_to_string(key).unwrap()).unwrap(),
        ),
    };
    copy.run(
        &["sync", "approve", &chain::approve_link(&rec)],
        &format!("{code}\n"),
    )
}

fn standing(expires: Option<&str>) -> serde_json::Value {
    serde_json::json!({"type": "standing", "device": device_key(), "caps": {},
        "label": "agent", "expires": expires})
}

/// `signer`'s own claim to `member`'s chain's standing admission by
/// `device`, as the proof a dial carries (da-tkbtu).
fn proof(member: &Copy, device: &[u8; 32], signer: &Copy) -> String {
    let secret: String = device.iter().map(|b| format!("{b:02x}")).collect();
    let chain = chain_of(member);
    let args = ["sync", "pipe", "standing", "--chain", &chain];
    signer.ok_in(&args, &secret).trim().to_string()
}

/// `copy`'s hello to `member` carrying `device`'s proof for `signer`.
fn hello(member: &Copy, copy: &Copy, device: &[u8; 32], signer: &Copy) -> Output {
    hello_with(member, copy, &proof(member, device, signer))
}

fn hello_with(member: &Copy, copy: &Copy, proof: &str) -> Output {
    let id = copy.id();
    let args = ["sync", "handshake", &id, "--label", "agent", "--addr", "{}"];
    member.run(
        &[&args[..], &["--invite", proof, "--chain", ""]].concat(),
        "",
    )
}

/// How many ops of `copy`'s `member` holds: none after a refused dial
/// (da-mmcbl), or its claim would travel on with the chain.
fn held_of(member: &Copy, copy: &Copy) -> String {
    let id = copy.id();
    member.query(&format!("SELECT COUNT(*) FROM ops WHERE endpoint = '{id}'"))
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Two fresh copies of one device are admitted by its standing record, the
/// second's admit revoking the first, whose ops stand; a hello with another
/// device's proof, another copy's admit, or one altered is refused; every copy
/// folds the same membership; and an expel of the device's copy revokes the
/// record, after which no copy of it is admitted.
#[test]
fn a_devices_fresh_copies_are_admitted_each_replacing_the_last_until_revoked() {
    let (a, b) = chained();
    let out = approve(&a, standing(None), "");
    assert!(out.status.success(), "{}", stderr(&out));

    let first = Copy::new();
    let out = hello(&a, &first, &DEVICE, &first);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(a.send(&first).0);
    first.ok(&["occur", "tea"]);
    assert!(first.send(&a).0);
    assert_eq!(member_of(&a, &first.id())["device"], device_key());
    // A dial again is in already, and its admit, synced later, changes nothing.
    let admitted = member_of(&a, &first.id());
    let out = hello(&a, &first, &DEVICE, &first);
    assert!(out.status.success(), "{}", stderr(&out));
    let (sent, said) = first.send(&a);
    assert!(sent, "{said}");
    assert_eq!(member_of(&a, &first.id()), admitted);

    let second = Copy::new();
    let out = hello(&a, &second, &DEVICE, &second);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(a.send(&second).0);
    assert!(!member_of(&a, &first.id())["revoked"].is_null());
    assert!(member_of(&a, &second.id())["revoked"].is_null());
    assert!(second.has("tea"), "the first copy's ops stand");
    second.ok(&["occur", "coffee"]);
    assert!(second.send(&a).0 && a.has("coffee"));

    let (stranger, other) = (Copy::new(), Copy::new());
    // Another copy's own claim, and the stranger's own with its time changed.
    let theirs = proof(&a, &DEVICE, &other);
    let altered = proof(&a, &DEVICE, &stranger).replace("\"created\":\"2", "\"created\":\"1");
    assert!(altered.contains("\"created\":\"1"), "{altered}");
    for (given, why) in [
        (
            proof(&a, &[4u8; 32], &stranger),
            "no standing admission for this device",
        ),
        (theirs, "carries no standing claim of its own"),
        (altered, "standing claim: bad signature"),
    ] {
        let out = hello_with(&a, &stranger, &given);
        assert!(!out.status.success());
        assert!(stderr(&out).contains(why), "{}", stderr(&out));
    }

    assert!(a.send(&b).0);
    for id in [first.id(), second.id()] {
        assert_eq!(member_of(&a, &id), member_of(&b, &id));
    }

    // Retired, first's own claim admits it again by neither route
    // (da-tkbtu): the fold refuses it, and as a record it is never read.
    let again = proof(&b, &DEVICE, &first);
    let out = hello_with(&b, &first, &again);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("in the chain already"),
        "{}",
        stderr(&out)
    );
    let id = first.id();
    let records = [
        "sync",
        "handshake",
        &id,
        "--label",
        "agent",
        "--addr",
        "{}",
        "--records",
    ];
    let refused = b.fails(&records, again.strip_prefix(chain::STANDING).unwrap());
    assert!(
        refused.contains("not a member of this sync chain"),
        "{refused}"
    );
    assert!(!member_of(&b, &id)["revoked"].is_null());

    // Both copies are labelled `agent`: the label names the live one (da-vffi4).
    let out = a.ok(&["sync", "chain", "expel", "agent"]);
    assert!(out.contains("standing admission is revoked"), "{out}");
    assert!(!member_of(&a, &second.id())["revoked"].is_null());
    let third = Copy::new();
    let out = hello(&a, &third, &DEVICE, &third);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("standing admission was revoked"),
        "{}",
        stderr(&out)
    );
    assert_eq!(held_of(&a, &third), "0", "a refused claim is not held");
}

/// A code no copy has restored yet is expelled by its label or device key:
/// its standing admission alone is revoked, and no copy of it is admitted
/// (da-vffi4).
#[test]
fn an_unused_code_is_expelled_by_its_label_or_device() {
    let (a, _b) = chained();
    let out = approve(&a, standing(None), "");
    assert!(out.status.success(), "{}", stderr(&out));
    let said = a.fails(&["sync", "chain", "expel", "agent", "--unjoin"], "");
    assert!(said.contains("no copy has restored yet"), "{said}");
    let out = a.ok(&["sync", "chain", "expel", &device_key()[..8]]);
    assert!(out.contains("its code admits no copy"), "{out}");
    let said = a.fails(&["sync", "chain", "expel", "agent"], "");
    assert!(said.contains("names 0 members"), "{said}");
    let copy = Copy::new();
    let out = hello(&a, &copy, &DEVICE, &copy);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("standing admission was revoked"),
        "{}",
        stderr(&out)
    );
}

/// In a locked chain a standing record takes an admin's signature, and one
/// past its `expires` admits no one.
#[test]
fn a_standing_admission_takes_an_admin_and_expires() {
    let (a, _b, admin) = locked();
    let out = approve(&a, standing(None), "");
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("not signed by an admin"),
        "{}",
        stderr(&out)
    );

    let out = approve(&a, standing(Some("2026-01-01T00:00:00Z")), &admin);
    assert!(out.status.success(), "{}", stderr(&out));
    let copy = Copy::new();
    let out = hello(&a, &copy, &DEVICE, &copy);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("standing admission expired (2026-01-01T00:00:00Z)"),
        "{}",
        stderr(&out)
    );
    assert_eq!(held_of(&a, &copy), "0", "a refused claim is not held");

    let out = approve(&a, standing(Some("2099-01-01T00:00:00Z")), &admin);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = hello(&a, &copy, &DEVICE, &copy);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_ne!(held_of(&a, &copy), "0", "an admitted claim is held");
}
