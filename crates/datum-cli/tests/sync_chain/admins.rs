//! Admin keys: adding and removing them, a signature refused, a passkey
//! approving by link, and removing the last.

use super::*;

#[test]
fn an_added_admin_works_and_a_removed_one_is_refused_and_the_last_needs_a_flag() {
    let (a, _b, first) = locked();
    let second = a.key("second.key");
    std::fs::write(&second, "11".repeat(32)).unwrap();
    a.ok(&[
        "sync",
        "admin",
        "add",
        &second,
        "--label",
        "second",
        "--admin-key",
        &first,
    ]);
    // A key file's label is its name: its path would sync to every member.
    let before = a.ok(&["sync", "chain", "list"]);
    assert!(
        before.contains("admin.key") && !before.contains(&first),
        "{before}"
    );
    a.ok(&[
        "sync",
        "admin",
        "remove",
        "admin.key",
        "--admin-key",
        &second,
    ]);

    let old = a.fails(&["sync", "new-invite", "--admin-key", &first], "");
    assert!(old.contains("not signed by an admin"), "{old}");
    a.ok(&["sync", "new-invite", "--admin-key", &second]);
    let last = a.fails(
        &["sync", "admin", "remove", "second", "--admin-key", &second],
        "",
    );
    assert!(
        last.contains("this is the last admin: removing it UNLOCKS the chain")
            && last.contains("Pass --allow-unlock"),
        "{last}"
    );
    let members = a.ok(&["sync", "chain", "list"]);
    assert!(
        members.contains("second") && !members.contains("admin.key"),
        "{members}"
    );
}

#[test]
fn an_admin_act_without_a_valid_signature_is_refused_by_name() {
    let (a, b, _) = locked();
    let out = a.fails(&["sync", "chain", "expel", "b"], "not-a-signature\n");
    assert!(out.contains("https://datum.day/sync/approve#"), "{out}");
    assert!(out.contains("not signed by an admin"), "{out}");
    assert!(member_of(&a, &b.id())["revoked"].is_null());
}

/// Run an admin command that asks for a passkey: read the approve link it
/// prints, sign the record in its fragment, paste the code back.
fn approve(copy: &Copy, args: &[&str]) -> Output {
    let mut child = copy.cmd(args).spawn().unwrap();
    let mut err = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    while !line.contains("/sync/approve#") {
        line.clear();
        assert!(err.read_line(&mut line).unwrap() > 0, "no approve link");
    }
    let record = BASE64URL_NOPAD
        .decode(line.trim().rsplit('#').next().unwrap().as_bytes())
        .unwrap();
    let code = approval_code(&Sha256::digest(&record));
    writeln!(child.stdin.take().unwrap(), "{code}").unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_passkey_admin_approves_by_link_and_pasted_code() {
    let a = Copy::new();
    a.ok(&["sync", "new-invite"]);
    // Adding the first admin to an open chain asks for no approval.
    a.ok(&["sync", "admin", "add", &passkey_json()]);
    let out = approve(&a, &["sync", "new-invite"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let secret = json(&String::from_utf8_lossy(&out.stdout))["secret"]
        .as_str()
        .unwrap()
        .to_string();
    let x = Copy::new().id();
    a.ok(&[
        "sync",
        "handshake",
        &x,
        "--label",
        "x",
        "--addr",
        "{}",
        "--invite",
        &secret,
        "--chain",
        "",
    ]);
    let members = a.ok(&["sync", "chain", "list"]);
    assert!(
        members.contains("phone  es256") && members.contains("x  "),
        "{members}"
    );
}

/// datum.day's way (da-bjxts): the act stops at the approve link, having
/// said an invite's secret already, and `sync approve <link>` writes the
/// record signed with the code — which the chain checks, refusing a bad one.
#[test]
fn sync_approve_writes_the_record_of_a_run_that_stopped_at_its_link() {
    let a = Copy::new();
    a.ok(&["sync", "new-invite"]);
    a.ok(&["sync", "admin", "add", &passkey_json()]);
    let out = a.run(&["sync", "new-invite"], "");
    assert!(!out.status.success());
    let secret = json(&String::from_utf8_lossy(&out.stdout))["secret"]
        .as_str()
        .unwrap()
        .to_string();
    let err = String::from_utf8_lossy(&out.stderr);
    let link = err
        .split_whitespace()
        .find(|w| w.contains("/sync/approve#"))
        .unwrap();
    let record = BASE64URL_NOPAD
        .decode(link.rsplit('#').next().unwrap().as_bytes())
        .unwrap();
    let wrong = approval_code(&Sha256::digest(b"another record"));
    let out = a.fails(&["sync", "approve", link], &wrong);
    assert!(out.contains("the sync chain refuses it"), "{out}");
    let said = a.ok_in(
        &["sync", "approve", link],
        &approval_code(&Sha256::digest(&record)),
    );
    assert_eq!(said.trim(), "approved: invite");
    let x = Copy::new().id();
    let invite = ["--invite", &secret, "--chain", ""];
    a.ok(&[
        &["sync", "handshake", &x, "--label", "x", "--addr", "{}"][..],
        &invite,
    ]
    .concat());
    assert!(a.ok(&["sync", "chain", "list"]).contains("x  "));
}

/// Removing the last admin reopens the chain, but only on purpose (vividn,
/// da-aam46): without `--allow-unlock` it is refused and names the flag;
/// with it the chain is open, a member's invite goes through, and every
/// copy says so until it is acknowledged.
#[test]
fn removing_the_last_admin_unlocks_the_chain_only_on_purpose() {
    let (a, b, admin) = locked();
    let remove = [
        "sync",
        "admin",
        "remove",
        "admin.key",
        "--admin-key",
        &admin,
    ];
    let refused = a.fails(&remove, "");
    assert!(refused.contains("Pass --allow-unlock"), "{refused}");
    // Locked: b's invite asks for an admin's approval.
    assert!(b
        .fails(&["sync", "new-invite"], "\n")
        .contains("no approval code"));

    let said = a.ok(&[&remove[..], &["--allow-unlock"]].concat());
    assert!(said.contains("the chain is unlocked"), "{said}");
    assert!(a.send(&b).0);
    let unlocked = "sync chain unlocked by admin.key at ";
    let announced = b.stderr_of(&["tail"]);
    assert!(
        announced.contains(unlocked) && announced.contains("`datum sync chain ack unlock`"),
        "{announced}"
    );
    assert!(!b.stderr_of(&["tail"]).contains(unlocked), "announced once");
    assert!(json(&a.ok(&["sync", "admin", "list", "--json"]))["admins"]
        .as_array()
        .unwrap()
        .is_empty());
    b.ok(&["sync", "new-invite"]);
    // `ack unlock` names the unlocks alone, never a device labelled `unlock`.
    admit(&a, &Copy::new(), "unlock", None);

    for copy in [&a, &b] {
        let status = copy.ok(&["sync", "status"]);
        assert!(status.contains(unlocked), "{status}");
        let all = json(&copy.ok(&["sync", "status", "--json"]));
        assert_eq!(all["open"], true);
        assert_eq!(all["unlocked"][0]["by"], "admin.key");
        let check = copy.ok(&["check", "sync"]);
        let head = "the sync chain was unlocked: other devices may have relied on the lock; `datum sync chain ack unlock` to acknowledge:\n  ⚠ sync chain unlocked by admin.key";
        assert!(check.contains(head), "{check}");
        copy.ok(&["sync", "chain", "ack", "unlock"]);
        assert!(!copy.ok(&["sync", "status"]).contains(unlocked));
    }
    let check = a.ok(&["check", "sync"]);
    assert!(
        check.contains("⚠ another device joined: unlock") && !check.contains(unlocked),
        "{check}"
    );
}
