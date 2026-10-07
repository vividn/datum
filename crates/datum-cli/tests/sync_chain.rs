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
        let vector = other.ok(&["sync", "pipe", "vector"]);
        self.ok(&["sync", "pipe", "ops", "--since", vector.trim()])
    }

    /// Send `other` what it lacks: whether it applied cleanly, and its output.
    fn send(&self, other: &Copy) -> (bool, String) {
        let out = other.run(&["sync", "pipe", "apply"], &self.ops_for(other));
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), text)
    }

    fn id(&self) -> String {
        self.ok(&["sync", "pipe", "ops"]); // mints the key
        self.query("SELECT endpoint_id FROM sync_local")
    }

    fn key(&self, name: &str) -> String {
        self.tmp.path().join(name).display().to_string()
    }

    fn hash(&self) -> String {
        self.ok(&["sync", "pipe", "verify"]).trim().to_string()
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

    fn exec(&self, sql: &str) {
        rusqlite::Connection::open(self.tmp.path().join("data.db"))
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }

    fn stderr_of(&self, args: &[&str]) -> String {
        String::from_utf8(self.run(args, "").stderr).unwrap()
    }
}

/// `copy`'s hello redeeming a new invite of `member`'s — `new-invite`
/// starting an open chain when `member` is in none — plus `extra`, as a
/// served handshake takes it.
fn redeem(
    member: &Copy,
    copy: &Copy,
    label: &str,
    admin_key: Option<&str>,
    extra: &[&str],
) -> Output {
    let mut args = vec!["sync", "new-invite"];
    args.extend(admin_key.map(|k| ["--admin-key", k]).into_iter().flatten());
    let invite = json(&member.ok(&args));
    let secret = invite["secret"].as_str().unwrap();
    let id = copy.id();
    let hello = ["sync", "handshake", &id, "--label", label, "--addr", "{}"];
    member.run(&[&hello[..], &["--invite", secret], extra].concat(), "")
}

/// `copy`, in no chain, admitted to `member`'s chain as `label`, and sent
/// the chain.
fn admit(member: &Copy, copy: &Copy, label: &str, admin_key: Option<&str>) {
    let out = redeem(member, copy, label, admin_key, &["--chain", ""]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(member.send(copy).0);
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

/// `sync chain leave` (da-h7pnc): a chain of one is left with every entry
/// kept, and those entries go to the next copy it syncs with; a copy with
/// another member, or holding a revoked one's data, refuses and says why.
#[test]
fn a_chain_of_one_is_left_with_its_entries_and_a_bigger_one_refuses() {
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

    // Another member: refused, by `--dry-run` too, and nothing goes.
    let ops = a.query("SELECT COUNT(*) FROM ops");
    for argv in [
        &["sync", "chain", "leave", "--dry-run"][..],
        &["sync", "chain", "leave"],
    ] {
        let refused = a.fails(argv, "");
        assert!(refused.contains("has 1 other member(s)"), "{refused}");
        assert!(refused.contains("only a copy holding nothing but its own data"));
    }
    assert_eq!(a.query("SELECT COUNT(*) FROM ops"), ops);
    // Revoked, its data stays: still refused.
    b.ok(&["sync", "chain", "revoke", "a"]);
    let refused = b.fails(&["sync", "chain", "leave"], "");
    assert!(refused.contains("holds other copies' data"), "{refused}");
    assert!(b.has("tea"));
}

/// `a`, holding an open chain, and `b`, invited into it.
fn chained() -> (Copy, Copy) {
    let (a, b) = (Copy::new(), Copy::new());
    admit(&a, &b, "b", None);
    (a, b)
}

/// `chained`, locked by `a` with the admin key file it returns, which
/// `admin add` writes; `b` holds the lock.
fn locked() -> (Copy, Copy, String) {
    let (a, b) = chained();
    let admin = a.key("admin.key");
    a.ok(&["sync", "admin", "add", &admin]);
    assert!(a.send(&b).0);
    (a, b, admin)
}

#[test]
fn two_copies_with_no_admin_join_by_invite_and_connect() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea"]);
    admit(&a, &b, "b", None);
    assert!(b.has("tea"));
    let members = json(&b.ok(&["sync", "chain", "list", "--json"]));
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

    // Held, they count as received: not asked for again (the pipe's one
    // overlap op aside), and no second warning.
    let (_, again) = stranger.send(&a);
    assert!(again.contains("applied 0 · already had 1"), "{again}");
    assert_eq!(
        a.query("SELECT COUNT(*) FROM integrity_warnings WHERE kind = 'sync_unauthorised'"),
        "1"
    );

    // Admitted, what it had sent applies without being sent again.
    admit(&a, &stranger, "s", None);
    assert!(a.has("intrusion"));
    a.ok(&["sync", "chain", "ack", &stranger.id()]);
    assert!(!a.ok(&["check"]).contains("outside the sync chain"));
}

/// A non-member's chain act is refused, open chain or locked, and so is its
/// handshake without an invite.
#[test]
fn a_non_members_act_is_refused_open_chain_or_not() {
    for lock in [false, true] {
        let (a, b) = chained();
        if lock {
            a.ok(&["sync", "admin", "add", &a.key("admin.key")]);
        }
        let stranger = Copy::new();
        let chain_id = a.query(
            "SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1",
        );
        let revoke = serde_json::json!({
            "type": "revoke", "chain": chain_id, "endpoint": b.id(),
            "created": "2026-09-29T00:00:00Z",
        });
        let out = a.run(&["sync", "pipe", "apply"], &forged(&stranger, &a, revoke));
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
        let own = a.fails(
            &["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"],
            "",
        );
        assert!(own.contains("is a copy of this database"), "{own}");
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
    assert!(c
        .ok(&["sync", "chain", "revoke", &a.id()[..10]])
        .contains("revoked"));
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
    b.ok(&["sync", "admin", "add", &admin]);
    assert!(b.send(&a).0);

    let out = a.fails(&["sync", "new-invite"], "not-a-signature\n");
    assert!(out.contains("https://datum.day/sync/approve#"), "{out}");
    assert!(out.contains("not signed by an admin"), "{out}");
    let out = a.fails(&["sync", "chain", "revoke", "b"], "");
    assert!(out.contains("no approval code"), "{out}");
    // A second key needs the first: first come, first served.
    let out = a.fails(&["sync", "admin", "add", &a.key("mine.key")], "\n");
    assert!(out.contains("no approval code"), "{out}");

    a.ok(&["sync", "new-invite", "--admin-key", &admin]);
    a.ok(&["sync", "chain", "revoke", "b", "--admin-key", &admin]);
    let members = json(&a.ok(&["sync", "chain", "list", "--json"]));
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
    let short = a.fails(
        &["sync", "chain", "revoke", &b_id[..3], "--admin-key", &admin],
        "",
    );
    assert!(short.contains("names 0 members"), "{short}");
    a.ok(&["sync", "chain", "revoke", "b", "--admin-key", &admin]);
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
    let members = a.ok(&["sync", "chain", "list"]);
    assert!(members.contains("REVOKED"), "{members}");
}

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
fn a_new_endpoint_is_announced_once_and_stays_in_check_until_acked() {
    let (a, b, _) = locked();
    // a admitted b at a handshake, as any member admits a redeemer: loud,
    // since the invite may have been anyone's. Its own admin key is quiet.
    let admitted = a.ok(&["check"]);
    assert!(
        admitted.contains("⚠ new sync endpoint b") && !admitted.contains("new sync admin"),
        "{admitted}"
    );
    a.ok(&["sync", "chain", "ack", "b"]);
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
    b.ok(&["sync", "chain", "ack", &a.id()[..10]]);
    b.ok(&["sync", "chain", "ack", "ed25519:"]);
    assert!(!b.ok(&["check"]).contains("new to the sync chain"));
    let none = b.fails(&["sync", "chain", "ack", "nobody"], "");
    assert!(none.contains("nothing to acknowledge"), "{none}");
}

#[test]
fn an_admin_act_without_a_valid_signature_is_refused_by_name() {
    let (a, b, _) = locked();
    let out = a.fails(&["sync", "chain", "revoke", "b"], "not-a-signature\n");
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

// ─── A device across chains, and sync status (da-rcb5m) ────────────────────

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// `id`'s entry in `copy`'s `sync chain list --json`.
fn member_of(copy: &Copy, id: &str) -> serde_json::Value {
    let all = json(&copy.ok(&["sync", "chain", "list", "--json"]));
    let hit = all["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["endpoint"] == id);
    hit.cloned().unwrap_or_else(|| panic!("{id} not in {all}"))
}

fn client_of(copy: &Copy) -> String {
    copy.query("SELECT client_id FROM sync_local")
}

#[test]
fn a_copy_describes_itself_at_its_first_invite_and_renames_itself_without_an_admin() {
    let (a, b, _) = locked();
    let me = member_of(&a, &a.id());
    assert_eq!(me["client"], client_of(&a).as_str(), "{me}");
    assert_eq!(me["kind"], "cli", "{me}");
    assert!(me["label"].as_str().unwrap().ends_with(" (cli)"), "{me}");
    assert_eq!(me["this"], true);

    // b was admitted at a handshake: it names itself, no key needed.
    assert!(member_of(&b, &b.id())["client"].is_null());
    assert!(b
        .ok(&["sync", "chain", "rename", "phone"])
        .contains("renamed b to phone"));
    assert!(b.send(&a).0);
    let seen = member_of(&a, &b.id());
    assert_eq!(seen["label"], "phone", "{seen}");
    assert_eq!(seen["client"], client_of(&b).as_str(), "{seen}");
    assert!(a.ok(&["sync", "chain", "list"]).contains("phone"));
}

/// datum.day's client is `DATUM_CLIENT_ID`, `web-` and eight more
/// (da-6kpj5): the copy starts a chain, is shown under it, and renames itself.
#[test]
fn a_client_named_by_the_environment_starts_a_chain_and_renames() {
    let a = Copy::new();
    let run = |args: &[&str]| {
        let out = a
            .cmd(args)
            .env("DATUM_CLIENT_ID", "web-abc12345")
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
        String::from_utf8(out.stdout).unwrap()
    };
    // Every command under the one client: another would be another endpoint.
    let me = || {
        let all = json(&run(&["sync", "chain", "list", "--json"]));
        all["members"][0].clone()
    };
    run(&["sync", "new-invite"]);
    assert_eq!(me()["client"], "web-abc12345", "{}", me());
    assert_eq!(me()["this"], true, "{}", me());
    run(&["sync", "chain", "rename", "kitchen"]);
    assert_eq!(me()["label"], "kitchen", "{}", me());
    assert_eq!(me()["client"], "web-abc12345", "{}", me());
    let status = json(&run(&["sync", "status", "--json"]));
    assert_eq!(status["this"]["client"], "web-abc12345", "{status}");
}

/// An entry, its copy's endpoint, the chain and `config show` name one
/// client, resolved one way: `DATUM_CLIENT_ID`, then the toml, then the file
/// (da-mmkre — entries read the file alone while sync read the env).
#[test]
fn an_entry_the_chain_and_config_show_name_one_client() {
    for (env, toml) in [
        (Some("web-abc12345"), None),
        (None, Some("toml-dev")),
        (None, None),
    ] {
        let a = Copy::new();
        if let Some(t) = toml {
            std::fs::create_dir_all(a.tmp.path().join("config")).unwrap();
            let body = format!("client_id = \"{t}\"\n");
            std::fs::write(a.tmp.path().join("config/datumrc.toml"), body).unwrap();
        }
        let run = |args: &[&str]| {
            let mut c = a.cmd(args);
            match env {
                Some(d) => c.env("DATUM_CLIENT_ID", d),
                None => c.env_remove("DATUM_CLIENT_ID"),
            };
            let out = c.output().unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
            String::from_utf8(out.stdout).unwrap()
        };
        run(&["sync", "new-invite"]);
        run(&["occur", "coffee"]);
        let want = env.or(toml).map(String::from).unwrap_or_else(|| {
            let file = std::fs::read_to_string(a.tmp.path().join("home/client_id"));
            file.unwrap().trim().to_string()
        });
        assert_eq!(a.query("SELECT client_id FROM entries"), want);
        assert_eq!(a.query("SELECT client_id FROM sync_local"), want);
        let status = json(&run(&["sync", "status", "--json"]));
        assert_eq!(status["this"]["client"], want.as_str(), "{status}");
        let described = json(&run(&["sync", "chain", "list", "--json"]));
        assert_eq!(
            described["members"][0]["client"],
            want.as_str(),
            "{described}"
        );
        let show = run(&["config", "show"]);
        let line = show.lines().find(|l| l.starts_with("client_id")).unwrap();
        assert_eq!(
            line.split_whitespace().nth(1),
            Some(want.as_str()),
            "{show}"
        );
    }
}

/// An install from before da-lfsei keeps its id: the data directory's
/// `device_id` file is moved to `client_id` on first read.
#[test]
fn an_install_from_before_the_rename_keeps_its_client_id() {
    let a = Copy::new();
    let home = a.tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("device_id"), "01OLDINSTALL\n").unwrap();
    a.ok(&["occur", "coffee"]);
    assert_eq!(a.query("SELECT client_id FROM entries"), "01OLDINSTALL");
    let file = std::fs::read_to_string(home.join("client_id")).unwrap();
    assert_eq!(file.trim(), "01OLDINSTALL");
    assert!(!home.join("device_id").exists());
}

#[test]
fn another_member_is_renamed_by_an_admin_and_by_no_one_else() {
    let (a, b, admin) = locked();
    b.ok(&["sync", "chain", "rename", "phone"]);
    assert!(b.send(&a).0);
    a.ok(&[
        "sync",
        "chain",
        "rename",
        "phone",
        "tablet",
        "--admin-key",
        &admin,
    ]);
    let renamed = member_of(&a, &b.id());
    assert_eq!(renamed["label"], "tablet");
    assert_eq!(
        renamed["client"],
        client_of(&b).as_str(),
        "the client is kept"
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
        &["sync", "chain", "rename", &a.id()[..10], "evil"],
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
fn two_databases_in_one_data_directory_are_one_client() {
    let a = Copy::new();
    let other = a.key("other.db");
    a.ok(&["sync", "new-invite"]);
    a.ok(&["--db", &other, "sync", "new-invite"]);
    let one = json(&a.ok(&["sync", "status", "--json"]));
    let two = json(&a.ok(&["--db", &other, "sync", "status", "--json"]));
    assert_ne!(one["this"]["endpoint"], two["this"]["endpoint"]);
    assert_eq!(one["this"]["client"], two["this"]["client"]);
    assert_eq!(
        (one["db"].as_str(), two["db"].as_str()),
        (Some("data"), Some("other"))
    );
    let first = member_of(&a, &a.id());
    assert_eq!(first["client"], one["this"]["client"], "{first}");
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
    let vector = b.ok(&["sync", "pipe", "vector"]);
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
    assert_eq!(all["this"]["client"], client_of(&a).as_str());
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
        row["kind"],
        serde_json::Value::Null,
        "b never described itself"
    );

    a.ok(&["occur", "later"]);
    let row = status_of(&a, &b);
    // What a holds past what b last had: the entry, and its field's spec.
    let (mine, theirs) = (
        json(&a.ok(&["sync", "pipe", "vector"])),
        json(&b.ok(&["sync", "pipe", "vector"])),
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
    a.ok(&["sync", "chain", "revoke", "b"]);
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
    for hidden in [
        "endpoint-key",
        "record-session",
        "handshake",
        "new-invite",
        "sync pipe",
        "datum sync run",
    ] {
        assert!(!help.contains(hidden), "{hidden} in help");
    }
    let found = a.ok(&["__complete", "2", "--", "datum", "sync", ""]);
    assert!(
        found.contains("status") && found.contains("chain"),
        "{found}"
    );
    for hidden in [
        "endpoint-key",
        "record-session",
        "handshake",
        "new-invite",
        "pipe",
        "run",
    ] {
        assert!(!found.contains(hidden), "{hidden}: {found}");
    }
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
    let describe = |label: &str, kind: &str, client: &str| {
        serde_json::json!({
            "type": "describe", "chain": chain_id, "endpoint": b.id(), "client": client,
            "label": label, "kind": kind, "created": "2026-09-29T00:00:00Z",
        })
    };
    let client = client_of(&b);
    for (record, why) in [
        (
            describe("\u{1b}[2Jevil", "cli", &client),
            "a label cannot contain a control character (U+001B)",
        ),
        (
            describe("adm\u{202E}nimda", "cli", &client),
            "a label cannot contain a bidi control (U+202E)",
        ),
        (
            describe(&"x".repeat(65), "cli", &client),
            "a label is at most 64 characters, not 65",
        ),
        (
            describe("phone", "tv", &client),
            "a describe's kind is cli or web",
        ),
        (
            describe("phone", "cli", "web-\u{1b}[2J"),
            "a describe's client is not a client id",
        ),
        (
            describe("phone", "cli", &"d".repeat(65)),
            "a describe's client is not a client id",
        ),
    ] {
        let out = a.run(&["sync", "pipe", "apply"], &forged(&b, &a, record));
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && text.contains(why), "{why}: {text}");
        assert_eq!(member_of(&a, &b.id())["label"], "b");
    }
    // The forgery itself is sound: a well-formed one applies.
    let ok = describe("phone", "web", &client.to_lowercase());
    a.ok_in(&["sync", "pipe", "apply"], &forged(&b, &a, ok));
    assert_eq!(member_of(&a, &b.id())["kind"], "web");

    // A describe written before da-lfsei, `{device: <id>, client: cli|web}`,
    // reads as the same member: chain records are permanent.
    let old = serde_json::json!({
        "type": "describe", "chain": chain_id, "endpoint": b.id(), "device": client,
        "label": "old", "client": "cli", "created": "2026-09-29T00:00:01Z",
    });
    a.ok_in(&["sync", "pipe", "apply"], &forged(&b, &a, old));
    let m = member_of(&a, &b.id());
    assert_eq!(
        (&m["client"], &m["kind"]),
        (&client.as_str().into(), &"cli".into()),
        "{m}"
    );
    // The old fold took a null `device` too: that is still the old shape.
    let null = serde_json::json!({
        "type": "describe", "chain": chain_id, "endpoint": b.id(), "device": null,
        "label": "old", "client": "cli", "created": "2026-09-29T00:00:02Z",
    });
    a.ok_in(&["sync", "pipe", "apply"], &forged(&b, &a, null));
    let m = member_of(&a, &b.id());
    assert_eq!(
        (&m["client"], &m["kind"]),
        (&().into(), &"cli".into()),
        "{m}"
    );

    // The command refuses first, naming the character; nothing is written.
    let out = b.fails(&["sync", "chain", "rename", "\u{1b}[2Jevil"], "");
    assert!(
        out.contains("a label cannot contain a control character (U+001B)"),
        "{out}"
    );
    let out = b.fails(&["sync", "chain", "rename", &"x".repeat(65)], "");
    assert!(out.contains("at most 64 characters"), "{out}");
    assert_eq!(member_of(&b, &b.id())["label"], "b");
}

/// The tree regrouped (da-aam46), after device-to-device pairing, `init`
/// and `add-endpoint` went (da-6177s): every old spelling is gone from
/// help, from completion, and as a command; the groups complete, and the
/// hidden commands do not.
#[test]
fn the_old_spellings_are_gone_and_the_groups_complete() {
    let a = Copy::new();
    let help = format!("{}{}", a.ok(&["sync", "--help"]), a.ok(&["help", "sync"]));
    let complete = |words: &[&str]| {
        let at = (words.len() + 2).to_string();
        let argv = [
            &["__complete", &at, "--", "datum", "sync"][..],
            words,
            &[""],
        ]
        .concat();
        let found = a.ok(&argv);
        found
            .split_whitespace()
            .map(String::from)
            .collect::<Vec<_>>()
    };
    let top = complete(&[]);
    for gone in [
        "init",
        "add-endpoint",
        "add-client",
        "peers",
        "forget",
        "now",
        "invite",
        "connect",
        "members",
        "rename",
        "ack",
        "revoke",
        "add-admin",
        "remove-admin",
        "rekey",
        "vector",
        "ops",
        "apply",
        "verify",
    ] {
        assert!(!help.contains(&format!("sync {gone}")), "{gone} in help");
        let out = a.fails(&["sync", gone, "x"], "");
        assert!(out.contains("unknown sync subcommand"), "{gone}: {out}");
    }
    assert_eq!(
        top,
        ["status", "pause", "resume", "chain", "admin", "daemon"]
    );
    assert_eq!(
        complete(&["chain"]),
        ["invite", "connect", "leave", "list", "rename", "revoke", "role", "ack", "rekey"]
    );
    assert_eq!(complete(&["admin"]), ["list", "add", "remove"]);
    assert_eq!(complete(&["daemon"]), ["install", "uninstall", "status"]);
    // `datum sync help` is the page.
    assert_eq!(a.ok(&["sync", "help"]), a.ok(&["help", "sync"]));
}

/// Bare `sync` in no chain says how to join one, and is no failure.
#[test]
fn bare_sync_in_no_chain_says_how_to_start_one() {
    let out = Copy::new().ok(&["sync"]);
    assert!(
        out.contains("`datum sync chain invite` on this device")
            && out.contains("`datum sync chain connect <TICKET>`"),
        "{out}"
    );
}

/// An open chain says so beside its members, quietly; a locked one does not.
#[test]
fn status_says_a_chain_is_open() {
    let (a, b) = chained();
    let open = "no admin: any device can add devices — `datum sync admin add` to protect the chain";
    assert!(a.ok(&["sync", "status"]).contains(open));
    assert_eq!(json(&a.ok(&["sync", "status", "--json"]))["open"], true);
    a.ok(&["sync", "admin", "add", &a.key("admin.key")]);
    assert!(a.send(&b).0);
    for copy in [&a, &b] {
        assert!(!copy.ok(&["sync", "status"]).contains(open));
        assert_eq!(json(&copy.ok(&["sync", "status", "--json"]))["open"], false);
    }
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
        let check = copy.ok(&["check"]);
        let head = "the sync chain was unlocked: other devices may have relied on the lock; `datum sync chain ack unlock` to acknowledge:\n  ⚠ sync chain unlocked by admin.key";
        assert!(check.contains(head), "{check}");
        copy.ok(&["sync", "chain", "ack", "unlock"]);
        assert!(!copy.ok(&["sync", "status"]).contains(unlocked));
    }
    let check = a.ok(&["check"]);
    assert!(
        check.contains("⚠ new sync endpoint unlock") && !check.contains(unlocked),
        "{check}"
    );
}

/// `datum sync pause` (da-wlbyx): the copy neither sends nor receives — its
/// pipe and its handshake refuse, and bare `sync` names when and the fix —
/// `status` leads with it, and only this copy is paused. `resume` lets the
/// same ops through.
#[test]
fn pause_stops_the_pipe_and_resume_lets_it_through() {
    let (a, b) = chained();
    let b_id = b.id();
    a.ok(&["occur", "tea"]);
    assert!(a.ok(&["sync", "pause"]).contains("sync paused"));
    let at = a.query("SELECT paused_at FROM sync_local");
    let refusal = format!("sync is paused on this copy since {at}");
    for why in [
        a.fails(&["sync", "pipe", "ops"], ""),
        b.send(&a).1,
        a.fails(
            &["sync", "handshake", &b_id, "--label", "b", "--addr", "{}"],
            "",
        ),
        a.fails(&["sync"], ""),
    ] {
        assert!(
            why.contains(&refusal) && why.contains("`datum sync resume`"),
            "{why}"
        );
    }
    let status = a.ok(&["sync", "status"]);
    assert!(
        status.starts_with(&format!("⚠ paused since {at}")),
        "{status}"
    );
    assert_eq!(
        json(&a.ok(&["sync", "status", "--json"]))["paused"],
        at.as_str()
    );
    assert!(json(&b.ok(&["sync", "status", "--json"]))["paused"].is_null());
    assert!(a
        .ok(&["sync", "pause"])
        .contains(&format!("already paused, since {at}")));

    assert!(a.ok(&["sync", "resume"]).contains("sync resumed"));
    assert!(a.send(&b).0);
    assert!(b.has("tea"));
    assert!(!a.ok(&["sync", "status"]).contains("paused"));
}

/// A write on a copy with another member that has synced with none for a
/// day warns once a day, naming the pause when it knows it (da-wlbyx). A
/// read does not, nor does a lone member ever.
#[test]
fn a_write_warns_once_a_day_when_not_synced_for_a_day() {
    let (a, b) = chained();
    session(&a, &b);
    let stale = "not synced for";
    assert!(!a.stderr_of(&["occur", "fresh"]).contains(stale));

    a.exec("UPDATE sync_peers SET last_seen = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-3 days')");
    assert!(!a.stderr_of(&["tail"]).contains(stale), "a read is quiet");
    let warned = a.stderr_of(&["occur", "tea"]);
    assert!(
        warned.contains("⚠ not synced for 3 days — `datum sync status`"),
        "{warned}"
    );
    assert!(
        !a.stderr_of(&["occur", "coffee"]).contains(stale),
        "once a day"
    );

    // A day later it warns again, and names the pause.
    a.ok(&["sync", "pause"]);
    a.exec("UPDATE sync_local SET warned_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-25 hours')");
    let at = a.query("SELECT paused_at FROM sync_local");
    let warned = a.stderr_of(&["occur", "juice"]);
    assert!(
        warned.contains(&format!("not synced for 3 days (paused since {at})")),
        "{warned}"
    );

    // With b revoked, a is its chain's lone member: never a warning.
    a.ok(&["sync", "resume"]);
    a.ok(&["sync", "chain", "revoke", "b"]);
    a.exec("UPDATE sync_local SET warned_at = NULL");
    assert!(!a.stderr_of(&["occur", "alone"]).contains(stale));
}

/// A multi-side write commits after every side has run, and warns as a
/// single write does (da-wlbyx).
#[test]
fn a_multi_side_write_warns_too() {
    let (a, b) = chained();
    session(&a, &b);
    a.exec("UPDATE sync_peers SET last_seen = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-3 days')");
    let warned = a.stderr_of(&["occur", "tea", ",", "occur", "coffee"]);
    assert!(a.has("tea") && a.has("coffee"));
    assert!(warned.contains("⚠ not synced for 3 days"), "{warned}");
}

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

// ─── Filtered readers (da-u81u1) ────────────────────────────────────────────

/// What `from` sends `to` as a session with it would (`--peer`), applied,
/// in rounds until `to`'s vector stops moving — a widened filter lowers it
/// once its record applies: whether each applied cleanly, and what it said.
fn feed(from: &Copy, to: &Copy) -> (bool, String) {
    let (mut ok, mut text) = (true, String::new());
    let mut last = String::new();
    for _ in 0..4 {
        let vector = to.ok(&["sync", "pipe", "vector"]);
        if vector == last {
            break;
        }
        let ops = from.ok(&[
            "sync",
            "pipe",
            "ops",
            "--since",
            vector.trim(),
            "--peer",
            &to.id(),
        ]);
        let out = to.run(&["sync", "pipe", "apply"], &ops);
        ok &= out.status.success();
        text += &String::from_utf8_lossy(&out.stdout);
        text += &String::from_utf8_lossy(&out.stderr);
        last = vector;
    }
    (ok, text)
}

/// `copy` admitted to `member`'s chain as a reader of `fields`, and fed.
fn admit_filtered(member: &Copy, copy: &Copy, fields: &str) {
    let invite = json(&member.ok(&["sync", "new-invite", "--fields", fields]));
    let secret = invite["secret"].as_str().unwrap();
    let id = copy.id();
    let hello = ["sync", "handshake", &id, "--label", "r", "--addr", "{}"];
    member.ok(&[&hello[..], &["--invite", secret, "--chain", ""]].concat());
    let (ok, text) = feed(member, copy);
    assert!(ok, "{text}");
}

/// A reader invite, filtered or not, refuses a redeemer that holds entries
/// of its own, saying how many, and admits nothing (da-tti3w): the invite is
/// unused after, so the same redeemer holding none is admitted. A member
/// invite admits a redeemer that holds some.
#[test]
fn a_reader_invite_refuses_a_redeemer_holding_entries() {
    let (a, _b) = chained();
    for caps in [&["--reader"][..], &["--fields", "coffee"]] {
        let invite = json(&a.ok(&[&["sync", "new-invite"], caps].concat()));
        let secret = invite["secret"].as_str().unwrap();
        let r = Copy::new();
        let id = r.id();
        let hello = ["sync", "handshake", &id, "--label", "r", "--addr", "{}"];
        let redeem = [&hello[..], &["--invite", secret, "--chain", ""]].concat();
        let refused = a.fails(&[&redeem[..], &["--holds", "42"]].concat(), "");
        assert!(
            refused.contains(
                "r holds 42 entries of its own; a reader writes nothing, so joining as one \
                 would hide them. Connect from a fresh database (`--db`), or ask for a member invite"
            ),
            "{refused}"
        );
        let list = a.ok(&["sync", "chain", "list", "--json"]);
        assert!(!list.contains(&id), "refused, admitted nothing: {list}");
        a.ok(&redeem);
        assert_eq!(member_of(&a, &id)["role"], "reader");
    }
    let invite = json(&a.ok(&["sync", "new-invite"]));
    let r = Copy::new();
    let id = r.id();
    let hello = ["sync", "handshake", &id, "--label", "m", "--addr", "{}"];
    let secret = invite["secret"].as_str().unwrap();
    a.ok(&[
        &hello[..],
        &["--invite", secret, "--chain", "", "--holds", "42"],
    ]
    .concat());
    assert_eq!(member_of(&a, &id)["role"], "member");
}

/// Every row `copy` holds of `field`, tombstones and history included, and
/// its ops: none, for a field it must not hold. By field and slug, not by
/// text: a random id may spell the field (`LIKE` ignores case).
fn holds_any(copy: &Copy, field: &str) -> bool {
    let rows = copy.query(&format!(
        "SELECT (SELECT COUNT(*) FROM entries WHERE field = '{field}' OR slug = 'spec:{field}')
              + (SELECT COUNT(*) FROM ops WHERE json_extract(body, '$.field') = '{field}'
                   OR substr(slug, 1, {n}) = '{field}:' OR slug = 'spec:{field}')
              + (SELECT COUNT(*) FROM entry_history
                   WHERE json_extract(snapshot, '$.field') = '{field}'
                      OR json_extract(snapshot, '$.slug') = 'spec:{field}')",
        n = field.len() + 1
    ));
    rows != "0"
}

/// The filtered verify both sides of a session compute.
fn verify_with(copy: &Copy, peer: &Copy) -> String {
    copy.ok(&["sync", "pipe", "verify", "--peer", &peer.id()])
        .trim()
        .to_string()
}

/// A reader filtered to `coffee` gets coffee's entries and spec, from any
/// member that serves it, and nothing of `tea` — not even from a member that
/// holds everything; its verify agrees over the filtered set; and it and
/// the members say «filtered: n of m fields».
#[test]
fn a_filtered_reader_gets_its_fields_and_their_specs_and_nothing_else() {
    let (a, b) = chained();
    for (f, copy) in [("coffee", &a), ("tea", &a)] {
        copy.ok(&["spec", "set", f, "--verb", "occur"]);
        copy.ok(&["occur", f]);
    }
    b.ok(&["occur", "coffee", "cups=2"]);
    assert!(b.send(&a).0 && a.send(&b).0);
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'coffee'"),
        "2"
    );
    // b's first coffee declared a spec of its own: the older one stands.
    let spec = "SELECT COUNT(*) FROM entries WHERE slug = 'spec:coffee' AND superseded_by IS NULL";
    assert_eq!(r.query(spec), "1");
    assert!(!holds_any(&r, "tea"));
    // b holds everything too, and, once a's records say what r is, sends
    // it nothing of tea.
    assert!(a.send(&b).0);
    b.ok(&["occur", "tea", "-t", "-2"]);
    b.ok(&["occur", "coffee"]);
    let served = b.ok(&["sync", "pipe", "ops", "--since", "{}", "--peer", &r.id()]);
    let ops: Vec<serde_json::Value> = served
        .lines()
        .filter(|l| l.starts_with('['))
        .flat_map(|l| json(l).as_array().unwrap().clone())
        .collect();
    assert!(!ops.is_empty(), "{served}");
    assert!(ops.iter().all(|o| o["field"] == "coffee"), "{served}");
    assert!(feed(&b, &r).0);
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'coffee'"),
        "3"
    );
    assert!(!holds_any(&r, "tea"));

    assert!(b.send(&a).0);
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
    assert_eq!(verify_with(&b, &r), verify_with(&r, &b));
    assert_ne!(
        verify_with(&a, &r),
        a.hash(),
        "the filtered hash is its own"
    );

    let status = r.ok(&["sync", "status"]);
    assert!(
        status.contains("a reader, filtered: 1 of 2 fields: coffee"),
        "{status}"
    );
    assert!(a
        .ok(&["sync", "chain", "list"])
        .contains("reader (filtered: 1 of 2 fields)"));
    let filter = &member_of(&a, &r.id())["filter"];
    assert_eq!(filter, &serde_json::json!({"fields": ["coffee"], "of": 2}));
    let this = &json(&r.ok(&["sync", "status", "--json"]))["this"];
    assert_eq!(this["filter"]["fields"], serde_json::json!(["coffee"]));
}

/// Widening sends the reader the whole history of the field it adds;
/// narrowing makes its copy delete it — rows, ops and history.
#[test]
fn widening_sends_a_fields_history_and_narrowing_deletes_it() {
    let (a, _b) = chained();
    for (field, at) in [("tea", "-3"), ("coffee", "-2"), ("tea", "-1")] {
        a.ok(&["occur", field, "-t", at]);
    }
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    assert!(!holds_any(&r, "tea"));

    a.ok(&[
        "sync",
        "chain",
        "role",
        "r",
        "reader",
        "--fields",
        "coffee,tea",
    ]);
    let (ok, text) = feed(&a, &r);
    assert!(ok, "{text}");
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'tea'"),
        "2"
    );
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
    let out = a.fails(
        &[
            "sync",
            "chain",
            "role",
            "r",
            "reader",
            "--fields",
            "tea,coffee",
        ],
        "",
    );
    assert!(
        out.contains("r is already a reader of those fields"),
        "{out}"
    );

    a.ok(&["sync", "chain", "role", "r", "reader", "--fields", "coffee"]);
    assert!(feed(&a, &r).0);
    assert!(!holds_any(&r, "tea"));
    assert!(r.has("coffee"));
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));

    // Unfiltered, it is sent the rest, and with no gap left its marks go
    // — on the apply, not on a read: status and the vector leave them.
    a.ok(&["sync", "chain", "role", "r", "reader"]);
    let marks = |c: &Copy| c.query("SELECT COUNT(*) FROM sync_caught_up");
    assert!(feed(&a, &r).0);
    assert_eq!(a.hash(), r.hash());
    assert_eq!(marks(&r), "0");
    a.exec("INSERT INTO sync_caught_up VALUES ('e', '[\"coffee\"]', 1)");
    a.ok(&["sync", "status"]);
    a.ok(&["sync", "pipe", "vector"]);
    assert_eq!(marks(&a), "1");
    a.ok_in(&["sync", "pipe", "apply"], "");
    assert_eq!(marks(&a), "0");
}

/// An entry whose field changes into the filter arrives whole — its create
/// under the old field too — and one that changes out of it is removed.
#[test]
fn an_entry_that_moves_into_the_filter_arrives_whole_and_one_that_leaves_is_removed() {
    let (a, _b) = chained();
    a.ok(&["occur", "tea", "note=first"]);
    let tea = a.query("SELECT hid FROM entries WHERE field = 'tea'");
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    assert!(!holds_any(&r, "tea"));

    a.ok(&["update", &tea, "field=coffee"]);
    let (ok, text) = feed(&a, &r);
    assert!(ok, "{text}");
    assert_eq!(
        r.query(&format!("SELECT field FROM entries WHERE hid = '{tea}'")),
        "coffee"
    );
    assert_eq!(
        r.query(&format!("SELECT COUNT(*) FROM ops WHERE hid = '{tea}'")),
        "2"
    );
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));

    a.ok(&["update", &tea, "field=tea"]);
    assert!(feed(&a, &r).0);
    assert_eq!(
        r.query(&format!("SELECT COUNT(*) FROM entries WHERE hid = '{tea}'")),
        "0"
    );
    assert_eq!(
        r.query(&format!("SELECT COUNT(*) FROM ops WHERE hid = '{tea}'")),
        "0"
    );
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
}

/// The reader's vector is «caught up to #N»: past the ops of a field it is
/// never sent, so a member's status says synced across the gaps, and
/// pending only for what it would send.
#[test]
fn a_filtered_readers_vector_is_caught_up_across_gaps_and_status_counts_what_it_is_sent() {
    let (a, _b) = chained();
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    for (field, at) in [("coffee", "-3"), ("tea", "-2"), ("tea", "-1")] {
        a.ok(&["occur", field, "-t", at]);
    }
    let record = |r: &Copy| {
        let (ok, text) = feed(&a, r);
        assert!(ok, "{text}");
        let vector = r.ok(&["sync", "pipe", "vector"]);
        let verify = match verify_with(&a, r) == verify_with(r, &a) {
            true => "match",
            false => "mismatch",
        };
        a.ok(&[
            "sync",
            "record-session",
            &r.id(),
            "--vector",
            vector.trim(),
            "--verify",
            verify,
        ]);
    };
    record(&r);
    let a_id = a.id();
    let theirs = json(&r.ok(&["sync", "pipe", "vector"]));
    let mine = json(&a.ok(&["sync", "pipe", "vector"]));
    assert_eq!(theirs[&a_id], mine[&a_id], "caught up past the tea ops");
    let row = status_of(&a, &r);
    assert_eq!(
        (row["state"].as_str(), row["to_send"].as_i64()),
        (Some("synced"), Some(0)),
        "{row}"
    );

    a.ok(&["occur", "tea"]);
    assert_eq!(status_of(&a, &r)["to_send"], 0, "tea is not waiting for it");
    assert_eq!(status_of(&a, &r)["state"], "synced");
    a.ok(&["occur", "coffee"]);
    assert_eq!(status_of(&a, &r)["to_send"], 1);
    assert_eq!(status_of(&a, &r)["state"], "pending");
    record(&r);
    assert_eq!(status_of(&a, &r)["state"], "synced");

    // An entry no op covers yet goes as a baseline, to r only if it is
    // coffee: an import's rows, as its ops were never written.
    let uncover = |field: &str| {
        let hid = a.query(&format!(
            "SELECT MIN(hid) FROM entries WHERE field = '{field}'"
        ));
        a.exec(&format!("DELETE FROM ops WHERE hid = '{hid}'"));
    };
    uncover("tea");
    assert_eq!(status_of(&a, &r)["to_send"], 0);
    uncover("coffee");
    assert_eq!(status_of(&a, &r)["to_send"], 1);
}

/// An op outside the filter that reaches a filtered reader anyway — a
/// member's whole log, unfiltered — is refused, and nothing of it is kept.
#[test]
fn an_op_outside_its_filter_sent_to_a_filtered_reader_is_refused() {
    let (a, _b) = chained();
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    a.ok(&["occur", "tea", "-t", "-1"]);
    a.ok(&["occur", "coffee"]);
    let (ok, text) = a.send(&r);
    assert!(!ok, "{text}");
    assert!(text.contains("outside this reader's filter"), "{text}");
    assert!(r.has("coffee"));
    assert!(!holds_any(&r, "tea"));
}

/// `--kit` stands for the fields a kit holds specs for, written into the
/// chain as that list; a filter is a reader's alone.
#[test]
fn a_kit_names_its_fields_and_only_a_reader_is_filtered() {
    let (a, _b) = chained();
    a.ok(&["spec", "set", "coffee", "--verb", "occur"]);
    a.exec(
        "UPDATE entries SET data = json_set(data, '$.kit_id', 'drinks') WHERE slug = 'spec:coffee'",
    );
    let r = Copy::new();
    let invite = json(&a.ok(&["sync", "new-invite", "--kit", "drinks"]));
    let secret = invite["secret"].as_str().unwrap();
    let hello = ["sync", "handshake", &r.id(), "--label", "r", "--addr", "{}"];
    a.ok(&[&hello[..], &["--invite", secret, "--chain", ""]].concat());
    assert_eq!(
        member_of(&a, &r.id())["filter"]["fields"],
        serde_json::json!(["coffee"])
    );
    let out = a.fails(&["sync", "new-invite", "--kit", "nope"], "");
    assert!(
        out.contains("kit nope has no fields in this database"),
        "{out}"
    );
    let out = a.fails(
        &["sync", "chain", "role", "b", "member", "--fields", "coffee"],
        "",
    );
    assert!(out.contains("only a reader is filtered"), "{out}");
}

/// A filtered reader made a member holds its gaps until they are sent: it
/// says so in its vector, the highest contiguous seq, and a filtered reader
/// it serves is marked caught up that far and no further.
#[test]
fn a_filtered_reader_made_a_member_claims_only_what_it_holds_without_gaps() {
    let (a, _b) = chained();
    for (field, at) in [("coffee", "-3"), ("tea", "-2"), ("coffee", "-1")] {
        a.ok(&["occur", field, "-t", at]);
    }
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    a.ok(&["sync", "chain", "role", "r", "member"]);
    // Only the chain's records reach it: it is a member, the tea not yet.
    let records: String = a
        .ops_for(&r)
        .lines()
        .filter(|l| json(l)["hid"] == "~chain")
        .map(|l| format!("{l}\n"))
        .collect();
    r.ok_in(&["sync", "pipe", "apply"], &records);
    let a_id = a.id();
    let head = |c: &Copy| {
        json(&c.ok(&["sync", "pipe", "vector"]))[&a_id]
            .as_i64()
            .unwrap()
    };
    assert!(
        head(&r) < head(&a),
        "r lacks a's tea: {} of {}",
        head(&r),
        head(&a)
    );

    let r2 = Copy::new();
    admit_filtered(&r, &r2, "coffee");
    assert_eq!(head(&r2), head(&r));
    // Once a holds r's records admitting r2, it sends r2 the rest.
    assert!(r.send(&a).0);
    assert!(feed(&a, &r2).0);
    assert_eq!(head(&r2), head(&a));
}
