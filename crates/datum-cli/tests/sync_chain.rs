//! The sync chain (da-odwc7): signed membership, checked by every copy — the
//! real binary, ops piped between copies as in `sync_ops`. The admin key is
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

/// `a`, holding a chain with the admin key file `admin`, and `b`, admitted
/// by `add-endpoint` and holding the chain.
fn chained() -> (Copy, Copy, String) {
    let (a, b) = (Copy::new(), Copy::new());
    let admin = a.key("admin.key");
    a.ok(&["sync", "init", "--admin-key", &admin]);
    let b_id = b.id();
    a.ok(&[
        "sync",
        "add-endpoint",
        &b_id,
        "--label",
        "b",
        "--admin-key",
        &admin,
    ]);
    assert!(a.send(&b).0);
    (a, b, admin)
}

#[test]
fn a_non_members_ops_are_refused_until_it_is_admitted() {
    let (a, admin) = (Copy::new(), "admin.key");
    let admin = a.key(admin);
    a.ok(&["sync", "init", "--admin-key", &admin]);
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
    let id = stranger.id();
    a.ok(&["sync", "add-endpoint", &id, "--admin-key", &admin]);
    assert!(a.has("intrusion"));
    a.ok(&["sync", "ack", &id]);
    assert!(!a.ok(&["check"]).contains("outside the sync chain"));
}

#[test]
fn a_revoke_refuses_what_the_endpoint_made_after_it_on_every_copy() {
    let (a, b, admin) = chained();
    let c = Copy::new();
    a.ok(&[
        "sync",
        "add-endpoint",
        &c.id(),
        "--label",
        "c",
        "--admin-key",
        &admin,
    ]);
    assert!(a.send(&c).0);
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
    let (a, _b, first) = chained();
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

    let x = Copy::new().id();
    let old = a.fails(&["sync", "add-endpoint", &x, "--admin-key", &first], "");
    assert!(old.contains("not signed by an admin"), "{old}");
    a.ok(&["sync", "add-endpoint", &x, "--admin-key", &second]);
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
    let (a, b, _) = chained();
    // a admitted b itself: nothing to announce there.
    assert!(!a.stderr_of(&["tail"]).contains("new sync"));
    assert!(!a.ok(&["check"]).contains("new to the sync chain"));

    let first = b.stderr_of(&["tail"]);
    assert!(first.contains("⚠ new sync endpoint"), "{first}");
    assert!(
        first.contains("admitted") && first.contains("via genesis"),
        "{first}"
    );
    assert!(first.contains("⚠ new sync admin"), "{first}");
    assert!(!b.stderr_of(&["tail"]).contains("new sync"));

    let check = b.ok(&["check"]);
    assert!(check.contains("new to the sync chain"), "{check}");
    b.ok(&["sync", "ack", &a.id()[..10]]);
    b.ok(&["sync", "ack", "ed25519:"]);
    assert!(!b.ok(&["check"]).contains("new to the sync chain"));
    let none = b.fails(&["sync", "ack", "nobody"], "");
    assert!(none.contains("nothing to acknowledge"), "{none}");
}

#[test]
fn an_admin_act_without_a_valid_signature_is_refused_by_name() {
    let (a, _b, _) = chained();
    let x = Copy::new().id();
    let out = a.fails(&["sync", "add-endpoint", &x], "not-a-signature\n");
    assert!(out.contains("https://datum.day/sync/approve#"), "{out}");
    assert!(out.contains("not signed by an admin"), "{out}");
    assert!(!a.ok(&["sync", "members"]).contains(&x[..10]));
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
    let json = passkey_json();
    let out = approve(&a, &["sync", "init", "--admin-passkey", &json]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let x = Copy::new().id();
    let out = approve(&a, &["sync", "add-endpoint", &x, "--label", "x"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let members = a.ok(&["sync", "members"]);
    assert!(
        members.contains("phone  es256") && members.contains("x  "),
        "{members}"
    );
}
