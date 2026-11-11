//! A passkey's account (da-nf1p4): datum.day hands each command the PRF
//! output as `DATUM_ACCOUNT` while signed in. A chain started so has the
//! account key among its admins, which signs its acts after; and a device
//! signed in to it admits itself with an invite that key signed.

use super::*;

/// The PRF output whose account key `chain.rs`'s derivation test names.
const PRF: &str = "0707070707070707070707070707070707070707070707070707070707070707";
const KEY: &str = "ed25519:f71518e0672bdcd79e5cb2e5a1143eaaa5699998a08f28b2fa10716998e761ef";

fn signed_in(c: &Copy, args: &[&str]) -> String {
    signed_in_as(PRF, c, args)
}

fn signed_in_as(prf: &str, c: &Copy, args: &[&str]) -> String {
    let out = c
        .cmd(args)
        .env("DATUM_ACCOUNT", prf)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn a_chain_made_signed_in_is_the_accounts_and_its_devices_admit_themselves() {
    let (a, b) = (Copy::new(), Copy::new());
    assert_eq!(json(&signed_in(&a, &["sync", "account"]))["key"], KEY);
    signed_in(&a, &["sync", "new-invite"]);
    let list = json(&a.ok(&["sync", "chain", "list", "--json"]));
    let admins: Vec<_> = list["admins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| &a["key"])
        .collect();
    assert_eq!(admins, [KEY]);
    // Locked by it: signed in, an invite is signed; signed out, it asks an admin.
    signed_in(&a, &["sync", "new-invite"]);
    assert!(a
        .fails(&["sync", "new-invite"], "")
        .contains("Approve this"));

    // b, in no chain, signs in and redeems its own invite with a, which never invited it.
    let chain = list["chain"].as_str().unwrap();
    let own = json(&signed_in(&b, &["sync", "account", "--invite", chain]));
    let secret = own["secret"].as_str().unwrap();
    let hello = ["sync", "handshake", &b.id(), "--label", "b", "--addr", "{}"];
    a.ok(&[&hello[..], &["--invite", secret, "--chain", ""]].concat());
    let list = json(&a.ok(&["sync", "chain", "list", "--json"]));
    let joined = list["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["label"] == "b");
    assert_eq!(
        joined.unwrap()["via"],
        format!("invite {}", own["id"].as_str().unwrap())
    );

    // Signed in to another account, the invite it brings admits no one.
    let c = Copy::new();
    let other = json(&signed_in_as(
        &"08".repeat(32),
        &c,
        &["sync", "account", "--invite", chain],
    ));
    let hello = ["sync", "handshake", &c.id(), "--label", "c", "--addr", "{}"];
    let secret = other["secret"].as_str().unwrap();
    let why = a.fails(
        &[&hello[..], &["--invite", secret, "--chain", ""]].concat(),
        "",
    );
    assert!(why.contains("the invite c brought"), "{why}");
    assert!(c.fails(&["sync", "account"], "").contains("not signed in"));
    // Set but broken, it is refused, never read as signed out.
    let broken = c
        .cmd(&["sync", "new-invite"])
        .env("DATUM_ACCOUNT", "0707")
        .output()
        .unwrap();
    assert!(!broken.status.success());
    assert!(String::from_utf8_lossy(&broken.stderr).contains("DATUM_ACCOUNT is not a PRF output"));
}
