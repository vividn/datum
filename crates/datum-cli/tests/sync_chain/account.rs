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

/// The account record (da-rnxp4): a device signed in publishes the
/// account's chains it is in, sealed under the account's key; another of
/// the account's devices reads which to join, into what database, through
/// which member; a third device publishing keeps what the record held; and
/// another account reads nothing of it.
#[test]
fn the_account_record_says_which_chains_a_new_device_joins_and_through_whom() {
    let (a, b) = (Copy::new(), Copy::new());
    // Signed in, with no chain of the account's: nothing to publish.
    assert!(json(&signed_in(&a, &["sync", "account", "--record"]))["body"].is_null());
    signed_in(&a, &["sync", "new-invite"]);
    let chain = json(&a.ok(&["sync", "chain", "list", "--json"]))["chain"].clone();
    let relay = "http://127.0.0.1:3340/";
    let record = ["sync", "account", "--record", "--relay", relay];
    let made = json(&signed_in(&a, &record));
    let name = made["name"].as_str().unwrap();
    assert_eq!(name.len(), 52, "pkarr's z-base-32 of the key");
    let body = made["body"].as_str().unwrap();

    // b reads it: the chain, the database a names it by, and a, by the relay.
    let read = json(&signed_in(&b, &["sync", "account", "--read", body]));
    let joins = read["joins"].as_array().unwrap();
    assert_eq!(joins.len(), 1, "{read}");
    assert_eq!(joins[0]["chain"], chain);
    assert_eq!(joins[0]["db"], "data");
    let member = json(joins[0]["members"][0].as_str().unwrap());
    assert_eq!(
        member,
        serde_json::json!({"id": a.id(), "addrs": [{"Relay": relay}]})
    );
    // a holds it: nothing to join.
    let held = json(&signed_in(&a, &["sync", "account", "--read", body]));
    assert_eq!(held["joins"], serde_json::json!([]));

    // b, in no chain yet, publishes what the record held.
    let kept = json(&signed_in(
        &b,
        &["sync", "account", "--record", "--with", body],
    ));
    let again = json(&signed_in(
        &b,
        &["sync", "account", "--read", kept["body"].as_str().unwrap()],
    ));
    assert_eq!(again["joins"][0]["chain"], chain);

    // Another account's devices read nothing of it.
    let why = b.fails(&["sync", "account", "--read", body], "");
    assert!(why.contains("not signed in"), "{why}");
    let other = b
        .cmd(&["sync", "account", "--read", body])
        .env("DATUM_ACCOUNT", "08".repeat(32))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&other.stderr).contains("not this account's record"));
}
