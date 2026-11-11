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

/// `args` signed in, `stdin` on its stdin: datum.day hands a backup so.
fn signed_in_with(c: &Copy, args: &[&str], stdin: &str) -> String {
    let mut child = c.cmd(args).env("DATUM_ACCOUNT", PRF).spawn().unwrap();
    let mut pipe = child.stdin.take().unwrap();
    pipe.write_all(stdin.as_bytes()).unwrap();
    drop(pipe);
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
    String::from_utf8(out.stdout).unwrap()
}

/// The account's chain is the account's to restore and purge (da-t1492):
/// signed in, a hard restore of every copy names no order for a passkey to
/// approve and runs with the backup on stdin, as datum.day hands it, and a
/// purge of every copy runs with no approval code. Signed out, the dry run
/// names the order an admin must approve.
#[test]
fn the_account_signs_the_restore_and_the_purge_of_its_chain() {
    let a = Copy::new();
    signed_in(&a, &["sync", "new-invite"]);
    a.ok(&["occur", "tea"]);
    let bytes = a.ok(&["db", "backup", "-"]);
    a.ok(&["occur", "poison"]);
    let piped = ["db", "restore", "-", "--hard", "--chain"];
    let dry = [&piped[..], &["--dry-run", "--json"]].concat();
    assert!(json(&a.ok_in(&dry, &bytes))["approve"].is_string());
    assert!(json(&signed_in_with(&a, &dry, &bytes))["approve"].is_null());
    signed_in_with(&a, &[&piped[..], &["--yes"]].concat(), &bytes);
    let count = |f: &str| a.ok(&["count", f]).trim().to_string();
    assert_eq!([count("tea"), count("poison")], ["1", "0"]);

    signed_in(&a, &["db", "purge", "--chain", "--yes"]);
    assert_eq!(count("tea"), "0");
}

/// What `datum sync login` keeps (da-mzqs4), `account` in the data
/// directory, signs the install in with no `DATUM_ACCOUNT`: `sync status`
/// names the account, and the page's `--seal` of a login link is to the
/// device key and nonce the link names. `sync logout` forgets it.
#[test]
fn a_kept_account_signs_in_until_logout_forgets_it() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["moment", "tea"]);
    b.ok(&["moment", "tea"]);
    let status = |c: &Copy| json(&c.ok(&["sync", "status", "--json"]))["this"].clone();
    assert_eq!(status(&a)["account"], serde_json::Value::Null);
    let home = a.tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    // One that does not read is no account, and `status` says why.
    std::fs::write(home.join("account"), "0707").unwrap();
    assert_eq!(status(&a)["account"], serde_json::Value::Null);
    assert!(a
        .ok(&["sync", "status"])
        .contains("⚠ signed in to no account:"));
    std::fs::write(home.join("account"), PRF).unwrap();
    assert_eq!(status(&a)["account"], KEY);
    assert!(a
        .ok(&["sync", "status"])
        .contains(&format!("signed in to account {KEY}")));
    assert_eq!(json(&a.ok(&["sync", "account"]))["key"], KEY);

    let device = status(&b)["device"].as_str().unwrap().to_string();
    let login = serde_json::json!({"addr": {"id": device, "addrs": []}, "nonce": "n1"});
    let link = format!(
        "https://datum.day/sync/login#{}",
        data_encoding::BASE32_NOPAD.encode(login.to_string().as_bytes())
    );
    let sealed = json(&a.ok(&["sync", "account", "--seal", &link]));
    assert_eq!(sealed["to"], device.as_str());
    let message = json(sealed["message"].as_str().unwrap());
    assert!(
        message["eph"].is_string() && message["sealed"].is_string(),
        "{message}"
    );
    let why = a.fails(
        &[
            "sync",
            "account",
            "--seal",
            "https://datum.day/sync/login#nope",
        ],
        "",
    );
    assert!(why.contains("not a datum login link"), "{why}");

    assert!(a.ok(&["sync", "logout"]).contains("signed out"));
    assert!(!home.join("account").exists());
    assert_eq!(status(&a)["account"], serde_json::Value::Null);
    assert!(a.fails(&["sync", "account"], "").contains("not signed in"));
    assert!(a.ok(&["sync", "logout"]).contains("not signed in"));
}
