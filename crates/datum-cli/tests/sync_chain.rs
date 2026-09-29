//! The sync chain (da-odwc7): signed membership, checked by every copy — the
//! real binary, ops piped between copies as in `sync_ops`. A chain starts
//! open, every member an admin (da-6177s); the first admin key locks it —
//! an Ed25519 key file, and in one test a software passkey.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Output, Stdio};

use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

struct Copy {
    tmp: TempDir,
}

impl Copy {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", self.tmp.path().join("data.db"))
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("DATUM_COLOR", "0")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }

    fn run(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = self.cmd(args).spawn().expect("spawn datum");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn ok_in(&self, args: &[&str], stdin: &str) -> String {
        let out = self.run(args, stdin);
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        self.ok_in(args, "")
    }

    /// A command that must fail; its stdout and stderr together.
    fn fails(&self, args: &[&str], stdin: &str) -> String {
        let out = self.run(args, stdin);
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !out.status.success(),
            "`datum {}` succeeded: {text}",
            args.join(" ")
        );
        text
    }

    /// What `other` lacks of this copy's ops.
    fn ops_for(&self, other: &Copy) -> String {
        let vector = other.ok(&["sync", "vector"]);
        self.ok(&["sync", "ops", "--since", vector.trim()])
    }

    /// Send `other` what it lacks: whether it applied cleanly, and its output.
    fn send(&self, other: &Copy) -> (bool, String) {
        let out = other.run(&["sync", "apply"], &self.ops_for(other));
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), text)
    }

    fn id(&self) -> String {
        self.ok(&["sync", "ops"]); // mints the key
        self.query("SELECT endpoint_id FROM sync_local")
    }

    fn key(&self, name: &str) -> String {
        self.tmp.path().join(name).display().to_string()
    }

    fn hash(&self) -> String {
        self.ok(&["sync", "verify"]).trim().to_string()
    }

    fn has(&self, field: &str) -> bool {
        self.query(&format!(
            "SELECT COUNT(*) FROM entries WHERE field = '{field}' AND deleted_at IS NULL"
        )) != "0"
    }

    fn query(&self, sql: &str) -> String {
        rusqlite::Connection::open(self.tmp.path().join("data.db"))
            .unwrap()
            .query_row(sql, [], |r| {
                let v: rusqlite::types::Value = r.get(0)?;
                Ok(match v {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    other => format!("{other:?}"),
                })
            })
            .unwrap()
    }

    fn stderr_of(&self, args: &[&str]) -> String {
        String::from_utf8(self.run(args, "").stderr).unwrap()
    }
}

/// `copy` admitted to `member`'s chain as `label`, as a served handshake
/// admits the redeemer of an invite — `new-invite` starting an open chain
/// when `member` is in none — and sent the chain.
fn admit(member: &Copy, copy: &Copy, label: &str, admin_key: Option<&str>) {
    let mut args = vec!["sync", "new-invite"];
    args.extend(admin_key.map(|k| ["--admin-key", k]).into_iter().flatten());
    let invite = json(&member.ok(&args));
    let secret = invite["secret"].as_str().unwrap();
    let id = copy.id();
    let hello = ["sync", "handshake", &id, "--label", label, "--addr", "{}"];
    member.ok(&[&hello[..], &["--invite", secret]].concat());
    assert!(member.send(copy).0);
}

/// `a`, holding an open chain, and `b`, invited into it.
fn chained() -> (Copy, Copy) {
    let (a, b) = (Copy::new(), Copy::new());
    admit(&a, &b, "b", None);
    (a, b)
}

/// `chained`, locked by `a` with the admin key file it returns, which
/// `add-admin` writes; `b` holds the lock.
fn locked() -> (Copy, Copy, String) {
    let (a, b) = chained();
    let admin = a.key("admin.key");
    a.ok(&["sync", "add-admin", &admin]);
    assert!(a.send(&b).0);
    (a, b, admin)
}

#[test]
fn two_copies_with_no_admin_join_by_invite_and_connect() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea"]);
    admit(&a, &b, "b", None);
    assert!(b.has("tea"));
    let members = json(&b.ok(&["sync", "members", "--json"]));
    assert_eq!(members["admins"], serde_json::json!([]), "{members}");
    let via: Vec<_> = members["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["via"].as_str().unwrap().split(' ').next().unwrap())
        .collect();
    assert_eq!(via, ["genesis", "invite"], "{members}");
    b.ok(&["occur", "coffee"]);
    assert!(b.send(&a).0);
    assert!(a.has("coffee"));
    assert_eq!(a.hash(), b.hash());
}

#[test]
fn a_non_members_ops_are_refused_until_it_is_admitted() {
    let a = Copy::new();
    a.ok(&["sync", "new-invite"]);
    let stranger = Copy::new();
    stranger.ok(&["occur", "intrusion"]);

    let (ok, text) = stranger.send(&a);
    assert!(!ok, "{text}");
    assert!(text.contains("not a member of the sync chain"), "{text}");
    assert!(!a.has("intrusion"));
    let check = a.ok(&["check"]);
    assert!(check.contains("outside the sync chain"), "{check}");

    // Offered again, they are held already: no second warning.
    let (_, again) = stranger.send(&a);
    assert!(again.contains("already had 2"), "{again}");
    assert_eq!(
        a.query("SELECT COUNT(*) FROM integrity_warnings WHERE kind = 'sync_unauthorised'"),
        "1"
    );

    // Admitted, what it had sent applies without being sent again.
    admit(&a, &stranger, "s", None);
    assert!(a.has("intrusion"));
    a.ok(&["sync", "ack", &stranger.id()]);
    assert!(!a.ok(&["check"]).contains("outside the sync chain"));
}

/// A non-member's chain act is refused, open chain or locked, and so is its
/// handshake without an invite.
#[test]
fn a_non_members_act_is_refused_open_chain_or_not() {
    for lock in [false, true] {
        let (a, b) = chained();
        if lock {
            a.ok(&["sync", "add-admin", &a.key("admin.key")]);
        }
        let stranger = Copy::new();
        let chain_id = a.query(
            "SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1",
        );
        let revoke = serde_json::json!({
            "type": "revoke", "chain": chain_id, "endpoint": b.id(),
            "created": "2026-09-29T00:00:00Z",
        });
        let out = a.run(&["sync", "apply"], &forged(&stranger, &a, revoke));
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{text}");
        assert!(
            text.contains("its writer is not a member of the sync chain"),
            "{text}"
        );
        assert!(member_of(&a, &b.id())["revoked"].is_null());

        let hello = a.fails(
            &[
                "sync",
                "handshake",
                &stranger.id(),
                "--label",
                "s",
                "--addr",
                "{}",
            ],
            "",
        );
        assert!(hello.contains("not a member of this sync chain"), "{hello}");
        assert!(b
            .ok(&["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"])
            .contains("known"));
    }
}

#[test]
fn in_an_open_chain_a_member_invites_a_third_copy_and_revokes_one() {
    let (a, b) = chained();
    let c = Copy::new();
    // b, which did not start the chain, invites c; a hears of it from b.
    admit(&b, &c, "c", None);
    assert!(b.send(&a).0);
    assert!(member_of(&a, &c.id())["via"]
        .as_str()
        .unwrap()
        .starts_with("invite "));
    // c revokes a: no admin, no key, no prompt.
    assert!(c.ok(&["sync", "revoke", &a.id()[..10]]).contains("revoked"));
    assert!(c.send(&b).0);
    assert!(member_of(&b, &a.id())["revoked"].is_string());
    a.ok(&["occur", "after"]);
    let (ok, text) = a.send(&b);
    assert!(
        !ok && text.contains("made after its endpoint was revoked"),
        "{text}"
    );
}

#[test]
fn the_first_admin_add_locks_the_chain() {
    let (a, b) = chained();
    // Any member may lock it: b, which did not start the chain.
    let admin = b.key("admin.key");
    b.ok(&["sync", "add-admin", &admin]);
    assert!(b.send(&a).0);

    let out = a.fails(&["sync", "new-invite"], "not-a-signature\n");
    assert!(out.contains("https://datum.day/sync/approve#"), "{out}");
    assert!(out.contains("not signed by an admin"), "{out}");
    let out = a.fails(&["sync", "revoke", "b"], "");
    assert!(out.contains("no approval code"), "{out}");
    // A second key needs the first: first come, first served.
    let out = a.fails(&["sync", "add-admin", &a.key("mine.key")], "\n");
    assert!(out.contains("no approval code"), "{out}");

    a.ok(&["sync", "new-invite", "--admin-key", &admin]);
    a.ok(&["sync", "revoke", "b", "--admin-key", &admin]);
    let members = json(&a.ok(&["sync", "members", "--json"]));
    assert_eq!(members["admins"].as_array().unwrap().len(), 1, "{members}");
}

#[test]
fn a_revoke_refuses_what_the_endpoint_made_after_it_on_every_copy() {
    let (a, b, admin) = locked();
    let c = Copy::new();
    admit(&a, &c, "c", Some(&admin));
    b.ok(&["occur", "before"]);
    assert!(b.send(&a).0 && b.send(&c).0);

    // Three characters of an id name nothing: too easy to hit by accident.
    let b_id = b.id();
    let short = a.fails(&["sync", "revoke", &b_id[..3], "--admin-key", &admin], "");
    assert!(short.contains("names 0 members"), "{short}");
    a.ok(&["sync", "revoke", "b", "--admin-key", &admin]);
    b.ok(&["occur", "after"]);
    // c takes b's op before it hears of the revoke…
    assert!(b.send(&c).0);
    assert!(c.has("after"));
    // …and drops it when it does.
    let (ok, text) = a.send(&c);
    assert!(
        !ok && text.contains("made after its endpoint was revoked"),
        "{text}"
    );
    assert!(!c.has("after") && c.has("before"));
    // a refuses it outright.
    let (ok, text) = b.send(&a);
    assert!(
        !ok && text.contains("made after its endpoint was revoked"),
        "{text}"
    );
    assert!(!a.has("after") && a.has("before"));
    assert_eq!(a.hash(), c.hash());
    let members = a.ok(&["sync", "members"]);
    assert!(members.contains("REVOKED"), "{members}");
}

#[test]
fn an_added_admin_works_and_a_removed_one_is_refused_but_never_the_last() {
    let (a, _b, first) = locked();
    let second = a.key("second.key");
    std::fs::write(&second, "11".repeat(32)).unwrap();
    a.ok(&[
        "sync",
        "add-admin",
        &second,
        "--label",
        "second",
        "--admin-key",
        &first,
    ]);
    // A key file's label is its name: its path would sync to every member.
    let before = a.ok(&["sync", "members"]);
    assert!(
        before.contains("admin.key") && !before.contains(&first),
        "{before}"
    );
    a.ok(&["sync", "remove-admin", "admin.key", "--admin-key", &second]);

    let old = a.fails(&["sync", "new-invite", "--admin-key", &first], "");
    assert!(old.contains("not signed by an admin"), "{old}");
    a.ok(&["sync", "new-invite", "--admin-key", &second]);
    let last = a.fails(
        &["sync", "remove-admin", "second", "--admin-key", &second],
        "",
    );
    assert!(last.contains("the last admin cannot be removed"), "{last}");
    let members = a.ok(&["sync", "members"]);
    assert!(
        members.contains("second") && !members.contains("admin.key"),
        "{members}"
    );
}

#[test]
fn a_new_endpoint_is_announced_once_and_stays_in_check_until_acked() {
    let (a, b, _) = locked();
    // a admitted b at a handshake, as any member admits a redeemer: loud,
    // since the invite may have been anyone's. Its own admin key is quiet.
    let admitted = a.ok(&["check"]);
    assert!(
        admitted.contains("⚠ new sync endpoint b") && !admitted.contains("new sync admin"),
        "{admitted}"
    );
    a.ok(&["sync", "ack", "b"]);
    assert!(!a.ok(&["check"]).contains("new to the sync chain"));

    // b heard of a, then of the lock: each announced once, on the first
    // command after it arrived.
    let first = b.stderr_of(&["tail"]);
    assert!(first.contains("⚠ new sync admin"), "{first}");
    assert!(!b.stderr_of(&["tail"]).contains("new sync"));

    let check = b.ok(&["check"]);
    assert!(check.contains("new to the sync chain"), "{check}");
    assert!(
        check.contains("⚠ new sync endpoint") && check.contains("via genesis"),
        "{check}"
    );
    b.ok(&["sync", "ack", &a.id()[..10]]);
    b.ok(&["sync", "ack", "ed25519:"]);
    assert!(!b.ok(&["check"]).contains("new to the sync chain"));
    let none = b.fails(&["sync", "ack", "nobody"], "");
    assert!(none.contains("nothing to acknowledge"), "{none}");
}

#[test]
fn an_admin_act_without_a_valid_signature_is_refused_by_name() {
    let (a, b, _) = locked();
    let out = a.fails(&["sync", "revoke", "b"], "not-a-signature\n");
    assert!(out.contains("https://datum.day/sync/approve#"), "{out}");
    assert!(out.contains("not signed by an admin"), "{out}");
    assert!(member_of(&a, &b.id())["revoked"].is_null());
}

/// A software passkey: a fixed P-256 key answering `navigator.credentials.get`
/// for datum.day, packed as the approve page packs it.
fn approval_code(challenge: &[u8]) -> String {
    let sk = p256::ecdsa::SigningKey::from_slice(&[9u8; 32]).unwrap();
    let mut auth = Sha256::digest(b"datum.day").to_vec();
    auth.extend([0x05, 0, 0, 0, 0]);
    let client = serde_json::json!({
        "type": "webauthn.get", "origin": "https://datum.day",
        "challenge": BASE64URL_NOPAD.encode(challenge),
    })
    .to_string();
    let signed = [auth.as_slice(), &Sha256::digest(client.as_bytes())].concat();
    let sig: p256::ecdsa::Signature = p256::ecdsa::signature::Signer::sign(&sk, &signed);
    let mut out = Vec::new();
    for part in [auth.as_slice(), client.as_bytes()] {
        out.extend((part.len() as u16).to_be_bytes());
        out.extend(part);
    }
    out.extend(sig.to_der().as_bytes());
    BASE64URL_NOPAD.encode(&out)
}

fn passkey_json() -> String {
    let sk = p256::ecdsa::SigningKey::from_slice(&[9u8; 32]).unwrap();
    let spki = [
        &data_encoding::HEXLOWER
            .decode(b"3059301306072a8648ce3d020106082a8648ce3d030107034200")
            .unwrap()[..],
        sk.verifying_key().to_encoded_point(false).as_bytes(),
    ]
    .concat();
    serde_json::json!({"alg": -7, "publicKey": BASE64URL_NOPAD.encode(&spki), "label": "phone"})
        .to_string()
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
    a.ok(&["sync", "add-admin", &passkey_json()]);
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
    ]);
    let members = a.ok(&["sync", "members"]);
    assert!(
        members.contains("phone  es256") && members.contains("x  "),
        "{members}"
    );
}

// ─── A device across chains, and sync status (da-rcb5m) ────────────────────

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// `id`'s entry in `copy`'s `sync members --json`.
fn member_of(copy: &Copy, id: &str) -> serde_json::Value {
    let all = json(&copy.ok(&["sync", "members", "--json"]));
    let hit = all["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["endpoint"] == id);
    hit.cloned().unwrap_or_else(|| panic!("{id} not in {all}"))
}

fn device_of(copy: &Copy) -> String {
    copy.query("SELECT device_id FROM sync_local")
}

#[test]
fn a_copy_describes_itself_at_its_first_invite_and_renames_itself_without_an_admin() {
    let (a, b, _) = locked();
    let me = member_of(&a, &a.id());
    assert_eq!(me["device"], device_of(&a).as_str(), "{me}");
    assert_eq!(me["client"], "cli", "{me}");
    assert!(me["label"].as_str().unwrap().ends_with(" (cli)"), "{me}");
    assert_eq!(me["this"], true);

    // b was admitted at a handshake: it names itself, no key needed.
    assert!(member_of(&b, &b.id())["device"].is_null());
    assert!(b
        .ok(&["sync", "rename", "phone"])
        .contains("renamed b to phone"));
    assert!(b.send(&a).0);
    let seen = member_of(&a, &b.id());
    assert_eq!(seen["label"], "phone", "{seen}");
    assert_eq!(seen["device"], device_of(&b).as_str(), "{seen}");
    assert!(a.ok(&["sync", "members"]).contains("phone"));
}

/// datum.day's device is `DATUM_DEVICE_ID`, `web-` and eight more
/// (da-6kpj5): the copy starts a chain, is shown under it, and renames itself.
#[test]
fn a_device_named_by_the_environment_starts_a_chain_and_renames() {
    let a = Copy::new();
    let run = |args: &[&str]| {
        let out = a
            .cmd(args)
            .env("DATUM_DEVICE_ID", "web-abc12345")
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
        String::from_utf8(out.stdout).unwrap()
    };
    // Every command under the one device: another would be another endpoint.
    let me = || {
        let all = json(&run(&["sync", "members", "--json"]));
        all["members"][0].clone()
    };
    run(&["sync", "new-invite"]);
    assert_eq!(me()["device"], "web-abc12345", "{}", me());
    assert_eq!(me()["this"], true, "{}", me());
    run(&["sync", "rename", "kitchen"]);
    assert_eq!(me()["label"], "kitchen", "{}", me());
    assert_eq!(me()["device"], "web-abc12345", "{}", me());
    let status = json(&run(&["sync", "status", "--json"]));
    assert_eq!(status["this"]["device"], "web-abc12345", "{status}");
}

#[test]
fn another_member_is_renamed_by_an_admin_and_by_no_one_else() {
    let (a, b, admin) = locked();
    b.ok(&["sync", "rename", "phone"]);
    assert!(b.send(&a).0);
    a.ok(&["sync", "rename", "phone", "tablet", "--admin-key", &admin]);
    let renamed = member_of(&a, &b.id());
    assert_eq!(renamed["label"], "tablet");
    assert_eq!(
        renamed["device"],
        device_of(&b).as_str(),
        "the device is kept"
    );
    assert!(a.send(&b).0);
    assert_eq!(member_of(&b, &b.id())["label"], "tablet");

    // b is no admin: its describe of a is refused by the fold, and nothing
    // of it is kept.
    let a_label = member_of(&b, &a.id())["label"]
        .as_str()
        .unwrap()
        .to_string();
    let out = b.fails(
        &["sync", "rename", &a.id()[..10], "evil"],
        "not-a-signature\n",
    );
    assert!(out.contains("https://datum.day/sync/approve#"), "{out}");
    assert!(
        out.contains("the sync chain refuses it: not signed by an admin"),
        "{out}"
    );
    assert_eq!(member_of(&b, &a.id())["label"], a_label.as_str());
    assert!(b.send(&a).0);
    assert_eq!(member_of(&a, &a.id())["label"], a_label.as_str());
}

#[test]
fn two_databases_in_one_data_directory_are_one_device() {
    let a = Copy::new();
    let other = a.key("other.db");
    a.ok(&["sync", "new-invite"]);
    a.ok(&["--db", &other, "sync", "new-invite"]);
    let one = json(&a.ok(&["sync", "status", "--json"]));
    let two = json(&a.ok(&["--db", &other, "sync", "status", "--json"]));
    assert_ne!(one["this"]["endpoint"], two["this"]["endpoint"]);
    assert_eq!(one["this"]["device"], two["this"]["device"]);
    assert_eq!(
        (one["db"].as_str(), two["db"].as_str()),
        (Some("data"), Some("other"))
    );
    let first = member_of(&a, &a.id());
    assert_eq!(first["device"], one["this"]["device"], "{first}");
}

/// `b`'s entry in `a`'s `sync status --json`, having checked no state comes
/// without its time.
fn status_of(a: &Copy, b: &Copy) -> serde_json::Value {
    let all = json(&a.ok(&["sync", "status", "--json"]));
    let b_id = b.id();
    let row = all["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["endpoint"] == b_id.as_str());
    let row = row
        .cloned()
        .unwrap_or_else(|| panic!("{b_id} not in {all}"));
    let when = match row["state"].as_str().unwrap() {
        "synced" => row["last_verify"]["at"].clone(),
        "error" => row["error_at"].clone(),
        "pending" => row["last_seen"].clone(),
        _ => serde_json::Value::String("never".into()),
    };
    assert!(when.is_string(), "a state without its time: {row}");
    row
}

/// Play the transport over the pipe: each copy sends the other what it
/// lacks, and `a` records the session as a transport would.
fn session(a: &Copy, b: &Copy) {
    assert!(a.send(b).0 && b.send(a).0);
    let verify = if a.hash() == b.hash() {
        "match"
    } else {
        "mismatch"
    };
    let vector = b.ok(&["sync", "vector"]);
    a.ok(&[
        "sync",
        "record-session",
        &b.id(),
        "--vector",
        vector.trim(),
        "--verify",
        verify,
    ]);
}

#[test]
fn status_says_never_then_synced_then_pending_then_error_each_with_its_time() {
    let (a, b) = chained();
    // b heard of a over the pipe, never in a session.
    let row = status_of(&b, &a);
    assert_eq!(row["state"], "never", "{row}");
    assert!(row["last_seen"].is_null() && row["last_verify"].is_null());
    let all = json(&a.ok(&["sync", "status", "--json"]));
    assert_eq!(all["this"]["endpoint"], a.id().as_str());
    assert_eq!(all["this"]["device"], device_of(&a).as_str());
    assert!(all["chain"].is_string(), "{all}");

    session(&a, &b);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["to_send"].as_i64()),
        (Some("synced"), Some(0)),
        "{row}"
    );
    assert_eq!(row["last_verify"]["match"], true);
    assert_eq!(
        row["client"],
        serde_json::Value::Null,
        "b never described itself"
    );

    a.ok(&["occur", "later"]);
    let row = status_of(&a, &b);
    // What a holds past what b last had: the entry, and its field's spec.
    let (mine, theirs) = (
        json(&a.ok(&["sync", "vector"])),
        json(&b.ok(&["sync", "vector"])),
    );
    let waiting = mine[a.id()].as_i64().unwrap() - theirs[a.id()].as_i64().unwrap();
    assert!(waiting > 0);
    assert_eq!(
        (row["state"].as_str(), row["to_send"].as_i64()),
        (Some("pending"), Some(waiting)),
        "{row}"
    );
    session(&a, &b);
    assert_eq!(status_of(&a, &b)["state"], "synced");

    // A mismatch stands through a write made since; a session clears it.
    a.ok(&["sync", "record-session", &b.id(), "--verify", "mismatch"]);
    a.ok(&["occur", "since"]);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["error"].as_str()),
        (Some("error"), Some("verify MISMATCH")),
        "{row}"
    );
    session(&a, &b);
    assert_eq!(status_of(&a, &b)["state"], "synced");

    // b revoked, what it makes is refused, and the transport says so.
    a.ok(&["sync", "revoke", "b"]);
    b.ok(&["occur", "after"]);
    let (ok, why) = b.send(&a);
    assert!(!ok, "{why}");
    let why = why.lines().find(|l| l.starts_with("refused:")).unwrap();
    a.ok(&["sync", "record-session", &b.id(), "--error", why]);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["revoked"].as_bool()),
        (Some("error"), Some(true)),
        "{row}"
    );
    assert!(
        row["error"]
            .as_str()
            .unwrap()
            .contains("made after its endpoint was revoked"),
        "{row}"
    );
    let table = a.ok(&["sync", "status"]);
    assert!(
        table.contains("error") && table.contains("REVOKED"),
        "{table}"
    );
}

#[test]
fn status_of_a_copy_in_no_chain_is_empty() {
    let a = Copy::new();
    let all = json(&a.ok(&["sync", "status", "--json"]));
    assert!(all["chain"].is_null(), "{all}");
    assert_eq!(all["members"], serde_json::json!([]));
    assert_eq!(all["this"]["endpoint"], a.id().as_str());
    assert!(a.ok(&["sync", "status"]).contains("no sync chain"));
    // A session with a copy that is no member has nowhere to go.
    let stranger = Copy::new().id();
    let out = a.fails(
        &["sync", "record-session", &stranger, "--verify", "match"],
        "",
    );
    assert!(
        out.contains("not a member of this copy's sync chain"),
        "{out}"
    );
}

#[test]
fn the_network_workers_commands_are_hidden_and_endpoint_key_is_the_secret() {
    let a = Copy::new();
    let key = a.ok(&["sync", "endpoint-key"]);
    assert_eq!(
        key.trim(),
        a.query("SELECT endpoint_secret FROM sync_local")
    );
    let help = format!("{}{}", a.ok(&["sync", "--help"]), a.ok(&["help", "sync"]));
    assert!(help.contains("rename") && help.contains("status"), "{help}");
    for hidden in ["endpoint-key", "record-session", "handshake", "new-invite"] {
        assert!(!help.contains(hidden), "{hidden} in help");
    }
    let found = a.ok(&["__complete", "2", "--", "datum", "sync", ""]);
    assert!(
        found.contains("status") && found.contains("rename"),
        "{found}"
    );
    assert!(
        !found.contains("endpoint-key") && !found.contains("record-session"),
        "{found}"
    );
}

/// A chain op `copy` never wrote but signed with its key, next after what
/// `to` holds of it: what a member running other code could send.
fn forged(copy: &Copy, to: &Copy, record: serde_json::Value) -> String {
    use ed25519_dalek::Signer;
    let id = copy.id();
    let seq: i64 = to
        .query(&format!(
            "SELECT COALESCE(MAX(seq), 0) FROM ops WHERE endpoint = '{id}'"
        ))
        .parse()
        .unwrap();
    let hlc = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ/0000")
        .to_string();
    let mut op = serde_json::json!({
        "endpoint": id, "seq": seq + 1, "hlc": hlc, "slug": "~chain", "hid": "~chain",
        "kind": "chain", "raw": null, "alias_chain": null, "effect": record, "deps": {},
    });
    let secret = data_encoding::HEXLOWER
        .decode(
            copy.query("SELECT endpoint_secret FROM sync_local")
                .as_bytes(),
        )
        .unwrap();
    let key = ed25519_dalek::SigningKey::from_bytes(&secret.try_into().unwrap());
    let digest = Sha256::digest(datum_core::db::canonical_json(&op).as_bytes());
    op["sig"] = data_encoding::HEXLOWER
        .encode(&key.sign(&digest).to_bytes())
        .into();
    datum_core::db::canonical_json(&op)
}

#[test]
fn a_label_with_a_control_character_is_refused_by_the_fold_and_the_command() {
    let (a, b) = chained();
    assert!(b.send(&a).0);
    let chain_id = a
        .query("SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1");
    let describe = |label: &str, client: &str, device: &str| {
        serde_json::json!({
            "type": "describe", "chain": chain_id, "endpoint": b.id(), "device": device,
            "label": label, "client": client, "created": "2026-09-29T00:00:00Z",
        })
    };
    let device = device_of(&b);
    for (record, why) in [
        (
            describe("\u{1b}[2Jevil", "cli", &device),
            "a label cannot contain a control character (U+001B)",
        ),
        (
            describe("adm\u{202E}nimda", "cli", &device),
            "a label cannot contain a bidi control (U+202E)",
        ),
        (
            describe(&"x".repeat(65), "cli", &device),
            "a label is at most 64 characters, not 65",
        ),
        (
            describe("phone", "tv", &device),
            "a describe's client is cli or web",
        ),
        (
            describe("phone", "cli", "web-\u{1b}[2J"),
            "a describe's device is not a device id",
        ),
        (
            describe("phone", "cli", &"d".repeat(65)),
            "a describe's device is not a device id",
        ),
    ] {
        let out = a.run(&["sync", "apply"], &forged(&b, &a, record));
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && text.contains(why), "{why}: {text}");
        assert_eq!(member_of(&a, &b.id())["label"], "b");
    }
    // The forgery itself is sound: a well-formed one applies.
    let ok = describe("phone", "web", &device.to_lowercase());
    a.ok_in(&["sync", "apply"], &forged(&b, &a, ok));
    assert_eq!(member_of(&a, &b.id())["client"], "web");

    // The command refuses first, naming the character; nothing is written.
    let out = b.fails(&["sync", "rename", "\u{1b}[2Jevil"], "");
    assert!(
        out.contains("a label cannot contain a control character (U+001B)"),
        "{out}"
    );
    let out = b.fails(&["sync", "rename", &"x".repeat(65)], "");
    assert!(out.contains("at most 64 characters"), "{out}");
    assert_eq!(member_of(&b, &b.id())["label"], "b");
}

/// Device-to-device pairing, `init` and `add-endpoint` are gone (da-6177s):
/// from help, from completion, and as commands.
#[test]
fn the_removed_commands_are_gone() {
    let a = Copy::new();
    let help = format!("{}{}", a.ok(&["sync", "--help"]), a.ok(&["help", "sync"]));
    let found = a.ok(&["__complete", "2", "--", "datum", "sync", ""]);
    let words: Vec<&str> = found.split_whitespace().collect();
    for gone in ["init", "add-endpoint", "add-client", "peers", "forget"] {
        assert!(!help.contains(&format!("sync {gone}")), "{gone} in help");
        assert!(!words.contains(&gone), "{gone} completes: {found}");
        let out = a.fails(&["sync", gone, "x"], "");
        assert!(out.contains("unknown sync subcommand"), "{gone}: {out}");
    }
    for kept in ["invite", "connect", "revoke", "add-admin"] {
        assert!(words.contains(&kept), "{kept} missing: {found}");
    }
}
