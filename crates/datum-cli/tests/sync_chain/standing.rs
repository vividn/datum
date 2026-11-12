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

/// `copy`'s hello to `member` carrying `device`'s proof for `endpoint`.
fn hello(member: &Copy, copy: &Copy, device: &[u8; 32], endpoint: &str) -> Output {
    let proof = chain::standing_proof(device, &chain_of(member), endpoint);
    let id = copy.id();
    let args = ["sync", "handshake", &id, "--label", "agent", "--addr", "{}"];
    member.run(
        &[&args[..], &["--invite", &proof, "--chain", ""]].concat(),
        "",
    )
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Two fresh copies of one device are admitted by its standing record, the
/// second's admit revoking the first, whose ops stand; a hello with another
/// device's proof or a proof for another endpoint is refused; every copy
/// folds the same membership; and an expel of the device's copy revokes the
/// record, after which no copy of it is admitted.
#[test]
fn a_devices_fresh_copies_are_admitted_each_replacing_the_last_until_revoked() {
    let (a, b) = chained();
    let out = approve(&a, standing(None), "");
    assert!(out.status.success(), "{}", stderr(&out));

    let first = Copy::new();
    let out = hello(&a, &first, &DEVICE, &first.id());
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(a.send(&first).0);
    first.ok(&["occur", "tea"]);
    assert!(first.send(&a).0);
    assert_eq!(member_of(&a, &first.id())["device"], device_key());

    let second = Copy::new();
    let out = hello(&a, &second, &DEVICE, &second.id());
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(a.send(&second).0);
    assert!(!member_of(&a, &first.id())["revoked"].is_null());
    assert!(member_of(&a, &second.id())["revoked"].is_null());
    assert!(second.has("tea"), "the first copy's ops stand");
    second.ok(&["occur", "coffee"]);
    assert!(second.send(&a).0 && a.has("coffee"));

    let stranger = Copy::new();
    for (device, endpoint, why) in [
        (
            &[4u8; 32],
            stranger.id(),
            "no standing admission for this device",
        ),
        (&DEVICE, second.id(), "not signed for by its device's key"),
    ] {
        let out = hello(&a, &stranger, device, &endpoint);
        assert!(!out.status.success());
        assert!(stderr(&out).contains(why), "{}", stderr(&out));
    }

    assert!(a.send(&b).0);
    for id in [first.id(), second.id()] {
        assert_eq!(member_of(&a, &id), member_of(&b, &id));
    }

    let out = a.ok(&["sync", "chain", "expel", &second.id()]);
    assert!(out.contains("standing admission is revoked"), "{out}");
    let third = Copy::new();
    let out = hello(&a, &third, &DEVICE, &third.id());
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
    let out = hello(&a, &copy, &DEVICE, &copy.id());
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("standing admission expired (2026-01-01T00:00:00Z)"),
        "{}",
        stderr(&out)
    );

    let out = approve(&a, standing(Some("2099-01-01T00:00:00Z")), &admin);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = hello(&a, &copy, &DEVICE, &copy.id());
    assert!(out.status.success(), "{}", stderr(&out));
}
