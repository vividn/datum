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
    /// The database file in `tmp` its commands run on.
    db: std::cell::Cell<&'static str>,
}

impl Copy {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
            db: std::cell::Cell::new("data.db"),
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", self.tmp.path().join(self.db.get()))
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .env("USER", "ann")
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

    /// The state hash, of a copy whose rows are what its ops fold to
    /// (da-n0ruf): a purge, a revoke and a reader's filter each drop rows on
    /// purpose, and `check sync` reads none of them as a fault.
    fn hash(&self) -> String {
        let out = self.run(&["check", "sync"], "");
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "`check sync`: {said}");
        self.ok(&["sync", "pipe", "verify"]).trim().to_string()
    }

    fn has(&self, field: &str) -> bool {
        self.query(&format!(
            "SELECT COUNT(*) FROM entries WHERE field = '{field}' AND deleted_at IS NULL"
        )) != "0"
    }

    fn query(&self, sql: &str) -> String {
        rusqlite::Connection::open(self.tmp.path().join(self.db.get()))
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
        rusqlite::Connection::open(self.tmp.path().join(self.db.get()))
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

/// The chain a copy's records name.
fn chain_of(copy: &Copy) -> String {
    copy.query("SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1")
}

/// `a`, holding an open chain, and `b`, invited into it.
/// The backups beside `copy`'s database that a chain restore left it
/// (da-uk23v).
fn kept(copy: &Copy) -> Vec<std::path::PathBuf> {
    let dir = std::fs::read_dir(copy.tmp.path()).unwrap();
    let mut kept: Vec<_> = dir
        .map(|f| f.unwrap().path())
        .filter(|p| p.to_string_lossy().contains("data.db.wiped-"))
        .collect();
    kept.sort();
    kept
}

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

/// A software passkey: a fixed P-256 key answering `navigator.credentials.get`
/// for datum.day, packed as the approve page packs it.
pub(crate) fn approval_code(challenge: &[u8]) -> String {
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

pub(crate) fn passkey_json() -> String {
    let sk = p256::ecdsa::SigningKey::from_slice(&[9u8; 32]).unwrap();
    let spki = [
        &data_encoding::HEXLOWER
            .decode(b"3059301306072a8648ce3d020106082a8648ce3d030107034200")
            .unwrap()[..],
        sk.verifying_key().to_sec1_point(false).as_bytes(),
    ]
    .concat();
    serde_json::json!({"alg": -7, "publicKey": BASE64URL_NOPAD.encode(&spki), "label": "phone"})
        .to_string()
}

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

/// Every row `copy` holds of `field`, tombstones and history included, and
/// its ops: none, for a field it must not hold. By field and slug, not by
/// text: a random id may spell the field (`LIKE` ignores case).
fn holds_any(copy: &Copy, field: &str) -> bool {
    let rows = copy.query(&format!(
        "SELECT (SELECT COUNT(*) FROM entries WHERE field = '{field}' OR slug = 'spec:{field}')
              + (SELECT COUNT(*) FROM ops WHERE json_extract(nullif(body, ''), '$.field') = '{field}'
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

mod admins;
mod commands;
mod db_purge;
mod filtered;
mod grid;
mod handshake;
mod join;
mod labels;
mod leave;
mod list;
mod membership;
mod notices;
mod orphans;
mod purge;
mod readers;
mod refused;
mod restore;
mod status;
mod unjoin;
mod unsent;
