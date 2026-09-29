//! The sync chain (da-odwc7): signed membership. A copy in a chain accepts
//! ops only from the endpoints the chain's admins admitted.
//!
//! - **Records.** Chain records are ops of their own kind (`chain`, under the
//!   hid [`CHAIN_HID`], so an op's `deps` are the records its writer had
//!   seen), synced like any op but folded into membership, not entries. The
//!   op's `effect` is the record: a `type`, the `chain` id, `created`, and
//!   the type's own keys.
//! - **Admin signature.** Every record but `admit` carries `sig`, an admin
//!   key's signature over [`record_digest`] (SHA-256 of the canonical record
//!   without `sig`). An admin key is `ed25519:<hex>` — a raw key held in a
//!   file, for tests and headless setups — or a passkey, `es256:<SEC1
//!   point>` / `eddsa:<key>` (base64url), whose `sig` is an approval code:
//!   a WebAuthn assertion whose challenge is that digest (see
//!   [`verify_assertion`]).
//! - **Fold.** Membership is the fold of every chain op held, in
//!   `(hlc, endpoint)` order, each record checked against the fold before
//!   it. It reads only the SET of records, so every copy holding them
//!   agrees; a record that fails its check stays held and does nothing.
//!   - `genesis {admins, label}`: the first admin keys, signed by one of
//!     them; its writer becomes the first member. One per chain.
//!   - `invite {invite_id, secret_hash, expires, caps}`: pre-authorised, so
//!     any member admits its redeemer; one-time, until `expires`.
//!   - `admit {endpoint, label, via, secret}`: unsigned, written by the
//!     member that redeemed invite `via`, valid while the invite is unused
//!     and unexpired and `secret` hashes to it. The secret is carried so
//!     every copy can check the admitter had it: members hold the invite's
//!     hash, and without the secret any of them could admit anyone through
//!     an open invite. Once the admit folds, the invite is used and the
//!     secret admits no one.
//!   - `add_endpoint {endpoint, label}`, `revoke {endpoint}`,
//!     `add_admin {key, label}`, `remove_admin {key}` — never the last.
//!   - `describe {endpoint, device, label, client}` (da-rcb5m): what a
//!     member is — the device it runs on (one per data directory, shared by
//!     every db there, so one laptop's copies group as one device), its
//!     label, `cli` or `web`. The latest per endpoint wins. Unsigned when an
//!     endpoint describes ITSELF; about another member, admin-signed like
//!     every admin act, so no member can relabel another.
//!
//!   Every record but a genesis must be written by a member, and the same
//!   record twice does nothing the second time. Every label a record
//!   carries must pass [`label_problem`]: each is printed on every member.
//! - **Enforcement.** An entry op is allowed when its endpoint is a member
//!   and, if revoked, was made before the revoke ([`Chain::allows`]):
//!   admitting an endpoint vouches for the data it brings. A copy with no
//!   chain allows every op.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use data_encoding::BASE64URL_NOPAD;
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{
    digest, endpoint, hex, parse_hlc, push, reconsider, settle, sha256_hex, unhex, ApplySummary, Op,
};
use crate::db::Db;
use crate::error::{DatumError, Result};

/// The fewest characters of an id that may name it: a one-letter prefix
/// revoking whichever endpoint happens to start with it is a footgun.
pub const MIN_PREFIX: usize = 4;

/// What a member not yet holding an invite says to its redeemer, which
/// `connect` takes as a cue to try the ticket's next address.
pub const UNKNOWN_INVITE: &str = "unknown invite";

/// The longest label a member or an admin may carry, in characters.
pub const MAX_LABEL: usize = 64;

/// Why `label` may not name a member or an admin, if it may not. A label is
/// printed on every member's terminal, so a control character (Unicode Cc:
/// C0, DEL, C1) is refused — an escape there clears or retitles the screen —
/// and so is a bidi control, which lets one label display as another's.
pub fn label_problem(label: &str) -> Option<String> {
    let bidi = |c: char| matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
    if let Some(c) = label.chars().find(|c| c.is_control() || bidi(*c)) {
        let what = match c.is_control() {
            true => "a control character",
            false => "a bidi control",
        };
        return Some(format!(
            "a label cannot contain {what} (U+{:04X}): {label:?}",
            c as u32
        ));
    }
    let n = label.chars().count();
    (n > MAX_LABEL).then(|| format!("a label is at most {MAX_LABEL} characters, not {n}"))
}

/// A device id every member can print (`<client> on <device>`): not blank,
/// and within a label's rules. `device_id()` mints a ULID, but
/// `DATUM_DEVICE_ID` names any — datum.day's is `web-` and eight more
/// (da-6kpj5).
fn is_device_id(s: &str) -> bool {
    !s.trim().is_empty() && label_problem(s).is_none()
}

/// The hid every chain op is filed under.
pub const CHAIN_HID: &str = "~chain";
/// A passkey assertion is valid for this relying party alone.
pub const RP_ID: &str = "datum.day";
pub const ORIGIN: &str = "https://datum.day";

// ─── Admin keys and their signatures ────────────────────────────────────────

/// What an admin signs: the record without its `sig`.
pub fn record_digest(record: &Value) -> [u8; 32] {
    let mut unsigned = record.clone();
    if let Some(m) = unsigned.as_object_mut() {
        m.remove("sig");
    }
    digest(&unsigned)
}

/// The admin key of an Ed25519 secret held in a file.
pub fn ed25519_admin_key(secret: &[u8; 32]) -> String {
    format!(
        "ed25519:{}",
        hex(SigningKey::from_bytes(secret).verifying_key().as_bytes())
    )
}

/// `sig` for a record, signed with an Ed25519 admin secret.
pub fn sign_ed25519(record: &Value, secret: &[u8; 32]) -> String {
    hex(&SigningKey::from_bytes(secret)
        .sign(&record_digest(record))
        .to_bytes())
}

/// A new Ed25519 admin secret, hex, for a key file.
pub fn new_admin_secret() -> String {
    hex(&SigningKey::generate(&mut rand::rngs::OsRng).to_bytes())
}

/// The secret a key file holds.
pub fn read_admin_secret(text: &str) -> Option<[u8; 32]> {
    unhex::<32>(text.trim())
}

/// The hash an invite record carries of its secret.
pub fn secret_hash(secret: &str) -> String {
    sha256_hex(secret)
}

/// Where a passkey approves `record`: the record rides in the fragment,
/// which never reaches a server.
pub fn approve_link(record: &Value) -> String {
    format!(
        "{ORIGIN}/sync/approve#{}",
        BASE64URL_NOPAD.encode(super::canonical_json(record).as_bytes())
    )
}

/// The admin key of a passkey, from what the browser hands over at creation:
/// `{"alg": -7 | -8, "publicKey": <base64url SPKI DER>}` (`getPublicKey()`).
pub fn passkey_admin_key(json: &Value) -> std::result::Result<String, String> {
    // The DER before the key bytes: fixed for each curve.
    const P256: &str = "3059301306072a8648ce3d020106082a8648ce3d030107034200";
    const ED25519: &str = "302a300506032b6570032100";
    let spki = json["publicKey"]
        .as_str()
        .and_then(|s| BASE64URL_NOPAD.decode(s.as_bytes()).ok())
        .ok_or("publicKey is not base64url")?;
    let (kind, prefix, len) = match json["alg"].as_i64() {
        Some(-7) => ("es256", P256, 65),
        Some(-8) => ("eddsa", ED25519, 32),
        _ => return Err("alg must be -7 (ES256) or -8 (EdDSA)".into()),
    };
    let key = spki
        .strip_prefix(unhex_vec(prefix).as_slice())
        .filter(|k| k.len() == len)
        .ok_or("publicKey is not an SPKI key of that alg")?;
    let key = format!("{kind}:{}", BASE64URL_NOPAD.encode(key));
    AdminKey::parse(&key)?;
    Ok(key)
}

fn unhex_vec(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

enum AdminKey {
    Ed25519(ed25519_dalek::VerifyingKey),
    Es256(p256::ecdsa::VerifyingKey),
    PasskeyEd25519(ed25519_dalek::VerifyingKey),
}

impl AdminKey {
    fn parse(s: &str) -> std::result::Result<Self, String> {
        let bad = || format!("not an admin key: {s}");
        let b64 = |k: &str| BASE64URL_NOPAD.decode(k.as_bytes()).map_err(|_| bad());
        let ed = |b: &[u8]| {
            <[u8; 32]>::try_from(b)
                .ok()
                .and_then(|b| ed25519_dalek::VerifyingKey::from_bytes(&b).ok())
                .ok_or_else(bad)
        };
        match s.split_once(':') {
            Some(("ed25519", k)) => Ok(Self::Ed25519(ed(&unhex::<32>(k).ok_or_else(bad)?)?)),
            Some(("eddsa", k)) => Ok(Self::PasskeyEd25519(ed(&b64(k)?)?)),
            Some(("es256", k)) => p256::ecdsa::VerifyingKey::from_sec1_bytes(&b64(k)?)
                .map(Self::Es256)
                .map_err(|_| bad()),
            _ => Err(bad()),
        }
    }

    fn verify(&self, digest: &[u8; 32], sig: &str) -> std::result::Result<(), String> {
        match self {
            Self::Ed25519(k) => {
                let sig = unhex::<64>(sig).ok_or("sig is not an Ed25519 signature")?;
                k.verify_strict(digest, &ed25519_dalek::Signature::from_bytes(&sig))
                    .map_err(|_| "bad signature".into())
            }
            _ => verify_assertion(self, digest, sig),
        }
    }
}

/// Check a passkey's approval code for a record whose digest is
/// `challenge`. The code is base64url of `u16 len ‖ authenticatorData ‖
/// u16 len ‖ clientDataJSON ‖ signature` (lengths big-endian): what
/// `navigator.credentials.get` returns, packed to paste. Checked: the RP ID
/// hash, user presence AND verification, `webauthn.get`, the origin, the
/// challenge, and the signature over `authenticatorData ‖
/// SHA-256(clientDataJSON)`. Not signCount: synced passkeys report 0.
fn verify_assertion(
    key: &AdminKey,
    challenge: &[u8; 32],
    code: &str,
) -> std::result::Result<(), String> {
    let bytes = BASE64URL_NOPAD
        .decode(code.trim().as_bytes())
        .map_err(|_| "the approval code is not base64url")?;
    let mut rest = bytes.as_slice();
    let mut part = || -> std::result::Result<&[u8], String> {
        let (n, tail) = rest
            .split_first_chunk::<2>()
            .ok_or("the approval code is cut short")?;
        let n = u16::from_be_bytes(*n) as usize;
        let (part, tail) = tail
            .split_at_checked(n)
            .ok_or("the approval code is cut short")?;
        rest = tail;
        Ok(part)
    };
    let (auth, client) = (part()?, part()?);
    let sig = rest;
    if auth.len() < 37 {
        return Err("authenticatorData is too short".into());
    }
    if auth[..32] != Sha256::digest(RP_ID.as_bytes())[..] {
        return Err(format!("the assertion is not for {RP_ID} (rpIdHash)"));
    }
    if auth[32] & 0x05 != 0x05 {
        return Err("the assertion lacks user presence and verification (UP, UV)".into());
    }
    let cd: Value =
        serde_json::from_slice(client).map_err(|_| "clientDataJSON is not JSON".to_string())?;
    if cd["type"] != "webauthn.get" {
        return Err("clientDataJSON is not a webauthn.get".into());
    }
    if cd["origin"] != ORIGIN {
        return Err(format!("the assertion's origin is not {ORIGIN}"));
    }
    if cd["crossOrigin"] == true {
        return Err("the assertion was made in a cross-origin frame".into());
    }
    if cd["challenge"] != BASE64URL_NOPAD.encode(challenge) {
        return Err("the assertion's challenge is not this record".into());
    }
    let signed = [auth, &Sha256::digest(client)].concat();
    let ok = match key {
        // p256 enforces no low-S, so an authenticator's high-S signature
        // verifies too (`a_high_s_signature_passes`).
        AdminKey::Es256(k) => p256::ecdsa::Signature::from_der(sig)
            .is_ok_and(|s| p256::ecdsa::signature::Verifier::verify(k, &signed, &s).is_ok()),
        AdminKey::PasskeyEd25519(k) => <[u8; 64]>::try_from(sig).is_ok_and(|s| {
            k.verify_strict(&signed, &ed25519_dalek::Signature::from_bytes(&s))
                .is_ok()
        }),
        AdminKey::Ed25519(_) => false,
    };
    ok.then_some(()).ok_or_else(|| "bad signature".into())
}

// ─── The fold ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Admin {
    pub key: String,
    pub label: String,
    /// The HLC of the record that added it.
    pub added: String,
}

#[derive(Debug, Clone)]
pub struct Member {
    pub endpoint: String,
    pub label: String,
    pub admitted: String,
    /// `genesis`, `admin`, or `invite <id>`.
    pub via: String,
    /// From its latest `describe`: its device id and `cli` / `web`.
    pub device: Option<String>,
    pub client: Option<String>,
    /// The HLC of the revoke: its ops from then on are refused.
    pub revoked: Option<String>,
}

#[derive(Debug)]
struct Invite {
    secret_hash: String,
    expires: String,
    used: bool,
}

/// Membership: the fold of the chain's records.
#[derive(Debug, Default)]
pub struct Chain {
    /// Empty when no genesis is held: no chain.
    pub id: String,
    pub admins: Vec<Admin>,
    pub members: Vec<Member>,
    invites: BTreeMap<String, Invite>,
    seen: BTreeSet<[u8; 32]>,
    /// The records that did nothing, by `(endpoint, seq)`, and why.
    pub rejected: BTreeMap<(String, i64), String>,
}

/// The wall time of an HLC, as RFC 3339.
pub fn hlc_time(hlc: &str) -> &str {
    hlc.split('/').next().unwrap_or(hlc)
}

impl Chain {
    pub fn exists(&self) -> bool {
        !self.id.is_empty()
    }

    pub fn member(&self, endpoint: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.endpoint == endpoint)
    }

    /// Why an op of `endpoint` at `hlc` is refused, if it is.
    pub fn allows(&self, endpoint: &str, hlc: &str) -> std::result::Result<(), String> {
        match self.member(endpoint) {
            _ if !self.exists() => Ok(()),
            None => Err("not a member of the sync chain".into()),
            Some(Member {
                revoked: Some(r), ..
            }) if hlc >= r.as_str() => Err(format!(
                "made after its endpoint was revoked ({})",
                hlc_time(r)
            )),
            Some(_) => Ok(()),
        }
    }

    /// The chain the ops held make.
    pub(super) fn load(conn: &Connection) -> Result<Chain> {
        let mut ops = conn
            .prepare_cached("SELECT body FROM ops WHERE hid = ?1 AND applied = 1")?
            .query_map(params![CHAIN_HID], |r| r.get::<_, String>(0))?
            .map(|b| Op::parse(&b?))
            .collect::<Result<Vec<Op>>>()?;
        ops.sort_by(|a, b| (&a.hlc, &a.endpoint).cmp(&(&b.hlc, &b.endpoint)));
        let mut chain = Chain::default();
        for op in &ops {
            if let Err(why) = chain.step(op) {
                chain.rejected.insert((op.endpoint.clone(), op.seq), why);
            }
        }
        Ok(chain)
    }

    fn step(&mut self, op: &Op) -> std::result::Result<(), String> {
        let r = &op.effect;
        let s = |k: &str| r[k].as_str().unwrap_or_default().to_string();
        let kind = s("type");
        let d = record_digest(r);
        if !self.seen.insert(d) {
            return Err("a record already in the chain".into());
        }
        let admins = r["admins"].as_array().into_iter().flatten();
        for label in admins.map(|a| &a["label"]).chain([&r["label"]]) {
            if let Some(why) = label.as_str().and_then(label_problem) {
                return Err(why);
            }
        }
        if kind == "genesis" {
            if self.exists() {
                return Err("a second genesis".into());
            }
            let admins: Vec<Admin> = r["admins"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| Admin {
                    key: a["key"].as_str().unwrap_or_default().into(),
                    label: a["label"].as_str().unwrap_or_default().into(),
                    added: op.hlc.clone(),
                })
                .collect();
            if s("chain").is_empty() || admins.is_empty() {
                return Err("a genesis names a chain id and an admin".into());
            }
            admin_signed(&admins, &d, &s("sig"))?;
            self.id = s("chain");
            self.admins = admins;
            return self.admit(&op.endpoint, &s("label"), &op.hlc, "genesis");
        }
        if !self.exists() {
            return Err("no genesis before it".into());
        }
        if s("chain") != self.id {
            return Err("a record of another chain".into());
        }
        self.allows(&op.endpoint, &op.hlc)
            .map_err(|why| format!("its writer is {why}"))?;
        if kind == "admit" {
            let via = s("via");
            let invite = self.invites.get_mut(&via).ok_or(UNKNOWN_INVITE)?;
            let wall = parse_hlc(&op.hlc).map(|(ms, _)| ms).unwrap_or(i64::MAX);
            let expires = DateTime::parse_from_rfc3339(&invite.expires)
                .map(|t| t.timestamp_millis())
                .unwrap_or(i64::MIN);
            if invite.used {
                return Err("invite already used".into());
            }
            if sha256_hex(&s("secret")) != invite.secret_hash {
                return Err("not the invite's secret".into());
            }
            if wall > expires {
                return Err(format!("invite expired ({})", invite.expires));
            }
            invite.used = true;
            return self.admit(
                &s("endpoint"),
                &s("label"),
                &op.hlc,
                &format!("invite {via}"),
            );
        }
        // An endpoint describes itself unsigned; anything else takes an admin.
        if kind != "describe" || op.endpoint != s("endpoint") {
            admin_signed(&self.admins, &d, &s("sig"))?;
        }
        match kind.as_str() {
            "describe" => {
                let client_ok = match &r["client"] {
                    Value::Null => true,
                    Value::String(c) => c == "cli" || c == "web",
                    _ => false,
                };
                let device_ok = match &r["device"] {
                    Value::Null => true,
                    Value::String(d) => is_device_id(d),
                    _ => false,
                };
                if !client_ok {
                    return Err("a describe's client is cli or web".into());
                }
                if !device_ok {
                    return Err("a describe's device is not a device id".into());
                }
                let m = self
                    .members
                    .iter_mut()
                    .find(|m| m.endpoint == s("endpoint"))
                    .ok_or("not a member")?;
                m.label = s("label");
                m.device = r["device"].as_str().map(Into::into);
                m.client = r["client"].as_str().map(Into::into);
                Ok(())
            }
            "invite" => {
                self.invites.insert(
                    s("invite_id"),
                    Invite {
                        secret_hash: s("secret_hash"),
                        expires: s("expires"),
                        used: false,
                    },
                );
                Ok(())
            }
            "add_endpoint" => self.admit(&s("endpoint"), &s("label"), &op.hlc, "admin"),
            "revoke" => {
                let m = self
                    .members
                    .iter_mut()
                    .find(|m| m.endpoint == s("endpoint"))
                    .ok_or("not a member")?;
                m.revoked.get_or_insert_with(|| op.hlc.clone());
                Ok(())
            }
            "add_admin" => {
                let key = s("key");
                AdminKey::parse(&key)?;
                if self.admins.iter().any(|a| a.key == key) {
                    return Err("already an admin".into());
                }
                self.admins.push(Admin {
                    key,
                    label: s("label"),
                    added: op.hlc.clone(),
                });
                Ok(())
            }
            "remove_admin" => {
                let at = self
                    .admins
                    .iter()
                    .position(|a| a.key == s("key"))
                    .ok_or("not an admin")?;
                if self.admins.len() == 1 {
                    return Err("the last admin cannot be removed".into());
                }
                self.admins.remove(at);
                Ok(())
            }
            other => Err(format!("unknown chain record '{other}'")),
        }
    }

    fn admit(
        &mut self,
        endpoint: &str,
        label: &str,
        hlc: &str,
        via: &str,
    ) -> std::result::Result<(), String> {
        if unhex::<32>(endpoint).is_none() {
            return Err("not an endpoint id".into());
        }
        if self.member(endpoint).is_some() {
            return Err("already in the chain".into());
        }
        self.members.push(Member {
            endpoint: endpoint.into(),
            label: label.into(),
            admitted: hlc.into(),
            via: via.into(),
            revoked: None,
            device: None,
            client: None,
        });
        Ok(())
    }
}

/// Signed by one of `admins`, or why not.
fn admin_signed(admins: &[Admin], digest: &[u8; 32], sig: &str) -> std::result::Result<(), String> {
    let mut why = Vec::new();
    for a in admins {
        match AdminKey::parse(&a.key).and_then(|k| k.verify(digest, sig)) {
            Ok(()) => return Ok(()),
            Err(e) => why.push(e),
        }
    }
    why.dedup();
    Err(format!("not signed by an admin: {}", why.join("; ")))
}

// ─── Notices: the loud warning (local, never synced) ────────────────────────

/// Note every member and admin not noted yet — this copy's own endpoint
/// aside. `quiet` notes them as seen: an act done here, by the person here.
pub(super) fn note(conn: &Connection, chain: &Chain, quiet: bool) -> Result<()> {
    let mine = endpoint(conn)?.id;
    let mut stmt = conn.prepare_cached(
        "INSERT OR IGNORE INTO sync_notices (subject, kind, label, at, via, announced, acked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
    )?;
    for m in chain.members.iter().filter(|m| m.endpoint != mine) {
        stmt.execute(params![
            m.endpoint,
            "endpoint",
            m.label,
            hlc_time(&m.admitted),
            m.via,
            quiet
        ])?;
    }
    for a in &chain.admins {
        stmt.execute(params![
            a.key,
            "admin",
            a.label,
            hlc_time(&a.added),
            "admin",
            quiet
        ])?;
    }
    Ok(())
}

/// A notice not yet acknowledged.
#[derive(Debug, Clone)]
pub struct Notice {
    pub subject: String,
    /// `endpoint` or `admin`.
    pub kind: String,
    pub label: String,
    pub at: String,
    pub via: String,
}

impl std::fmt::Display for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = &self.subject[..self.subject.len().min(18)];
        match self.kind.as_str() {
            "admin" => write!(f, "new sync admin {} ({id}) added {}", self.label, self.at),
            _ => write!(
                f,
                "new sync endpoint {} ({id}) admitted {} via {}",
                self.label, self.at, self.via
            ),
        }
    }
}

// ─── The chain on a Db ──────────────────────────────────────────────────────

impl Db {
    /// The chain this copy holds (`exists()` is false with none).
    pub fn sync_chain(&self) -> Result<Chain> {
        Chain::load(&self.conn)
    }

    /// This copy's endpoint id.
    pub fn sync_endpoint_id(&self) -> Result<String> {
        Ok(endpoint(&self.conn)?.id)
    }

    /// Write a chain record as an op of this copy's, and refuse it, writing
    /// nothing, when the chain would not take it. `quiet`: see [`note`].
    pub fn sync_chain_write(&self, record: Value, quiet: bool) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let ep = endpoint(&tx)?;
        let seq = push(&tx, &ep, CHAIN_HID, CHAIN_HID, "chain", record)?;
        let chain = Chain::load(&tx)?;
        if let Some(why) = chain.rejected.get(&(ep.id, seq)) {
            return Err(DatumError::Other(format!(
                "the sync chain refuses it: {why}"
            )));
        }
        let mut s = ApplySummary::default();
        reconsider(&tx, &chain, &mut s)?;
        settle(&tx, &mut s)?;
        note(&tx, &chain, quiet)?;
        tx.commit()?;
        Ok(())
    }

    /// This copy's device id: one per data directory, shared by its dbs.
    pub fn sync_device_id(&self) -> Result<String> {
        endpoint(&self.conn)?;
        Ok(self
            .conn
            .query_row("SELECT device_id FROM sync_local", [], |r| r.get(0))?)
    }

    /// Write this copy's own `describe`: unsigned, as only an endpoint
    /// describing itself may be.
    pub fn sync_describe_self(&self, label: &str, client: &str) -> Result<()> {
        let record = json!({
            "type": "describe", "chain": self.sync_chain()?.id,
            "endpoint": self.sync_endpoint_id()?, "device": self.sync_device_id()?,
            "label": label, "client": client, "created": Utc::now().to_rfc3339(),
        });
        self.sync_chain_write(record, true)
    }

    /// Redeem an invite for `endpoint`: write its `admit`, or say why not.
    pub fn sync_admit(&self, endpoint: &str, label: &str, secret: &str) -> Result<()> {
        let chain = self.sync_chain()?;
        let hash = sha256_hex(secret);
        let (id, invite) = chain
            .invites
            .iter()
            .find(|(_, i)| i.secret_hash == hash)
            .ok_or_else(|| DatumError::Other(UNKNOWN_INVITE.into()))?;
        if invite.used {
            return Err(DatumError::Other("invite already used".into()));
        }
        if invite.expires <= Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true) {
            return Err(DatumError::Other(format!(
                "invite expired ({})",
                invite.expires
            )));
        }
        let record = json!({
            "type": "admit", "chain": chain.id, "endpoint": endpoint, "label": label,
            "via": id, "secret": secret, "created": Utc::now().to_rfc3339(),
        });
        self.sync_chain_write(record, false)
    }

    /// Notices not yet printed, now marked printed.
    pub fn sync_notices_unannounced(&self) -> Result<Vec<Notice>> {
        let out = self.sync_notices("announced = 0")?;
        if !out.is_empty() {
            self.conn.execute(
                "UPDATE sync_notices SET announced = 1 WHERE announced = 0",
                [],
            )?;
        }
        Ok(out)
    }

    /// Notices not yet acknowledged (`datum check`).
    pub fn sync_notices_unacked(&self) -> Result<Vec<Notice>> {
        self.sync_notices("acked = 0")
    }

    fn sync_notices(&self, filter: &str) -> Result<Vec<Notice>> {
        let rows = self
            .conn
            .prepare(&format!(
                "SELECT subject, kind, label, at, via FROM sync_notices WHERE {filter} ORDER BY at"
            ))?
            .query_map([], |r| {
                Ok(Notice {
                    subject: r.get(0)?,
                    kind: r.get(1)?,
                    label: r.get(2)?,
                    at: r.get(3)?,
                    via: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// `datum sync ack <who>`: acknowledge the notices of an endpoint or an
    /// admin key — by label, or by at least [`MIN_PREFIX`] characters of its
    /// id (`ed25519:ab…` or `ab…`), naming one subject — and settle the
    /// `sync_unauthorised` warnings of that endpoint (a refused endpoint has
    /// no notice: name it by its whole id). How many rows it settled.
    pub fn sync_ack(&self, who: &str) -> Result<usize> {
        let prefix = |id: &str| who.len() >= MIN_PREFIX && id.starts_with(who);
        let hits: Vec<Notice> = self
            .sync_notices_unacked()?
            .into_iter()
            .filter(|n| {
                n.label == who
                    || prefix(&n.subject)
                    || n.subject.split_once(':').is_some_and(|(_, k)| prefix(k))
            })
            .collect();
        let mut subjects: BTreeSet<&str> = hits.iter().map(|n| n.subject.as_str()).collect();
        if subjects.len() > 1 && hits.iter().any(|n| n.label != who) {
            return Err(DatumError::Other(format!(
                "{who} names {} subjects; give more of the id",
                subjects.len()
            )));
        }
        if unhex::<32>(who).is_some() {
            subjects.insert(who);
        }
        let mut settled = 0;
        for subject in subjects {
            settled += self.conn.execute(
                "UPDATE sync_notices SET acked = 1, announced = 1 WHERE subject = ?1",
                params![subject],
            )?;
            settled += self.conn.execute(
                "UPDATE integrity_warnings SET resolution = 'keep_both', resolved_at = ?2
                 WHERE kind = 'sync_unauthorised' AND resolution IS NULL
                   AND instr(local_entry, ?1) > 0",
                params![format!("\"op\":\"{subject}/"), Utc::now().to_rfc3339()],
            )?;
        }
        Ok(settled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A software authenticator: a fixed P-256 key signing as a passkey would.
    fn authenticator() -> (p256::ecdsa::SigningKey, String) {
        let sk = p256::ecdsa::SigningKey::from_slice(&[7u8; 32]).unwrap();
        let point = sk.verifying_key().to_encoded_point(false);
        (
            sk,
            format!("es256:{}", BASE64URL_NOPAD.encode(point.as_bytes())),
        )
    }

    struct Assertion {
        rp_id: &'static str,
        flags: u8,
        kind: &'static str,
        origin: &'static str,
        cross_origin: bool,
        high_s: bool,
        challenge: [u8; 32],
    }

    fn good(challenge: [u8; 32]) -> Assertion {
        Assertion {
            rp_id: RP_ID,
            flags: 0x05,
            kind: "webauthn.get",
            origin: ORIGIN,
            cross_origin: false,
            high_s: false,
            challenge,
        }
    }

    fn code(sk: &p256::ecdsa::SigningKey, a: &Assertion, tamper: bool) -> String {
        let mut auth = Sha256::digest(a.rp_id.as_bytes()).to_vec();
        auth.push(a.flags);
        auth.extend([0, 0, 0, 0]); // signCount: synced passkeys report 0
        let client = json!({
            "type": a.kind, "origin": a.origin, "crossOrigin": a.cross_origin,
            "challenge": BASE64URL_NOPAD.encode(&a.challenge),
        })
        .to_string();
        let signed = [auth.as_slice(), &Sha256::digest(client.as_bytes())].concat();
        let mut sig: p256::ecdsa::Signature = p256::ecdsa::signature::Signer::sign(sk, &signed);
        if a.high_s {
            let (r, s) = sig.split_scalars();
            sig = p256::ecdsa::Signature::from_scalars(r.to_bytes(), (-*s).to_bytes()).unwrap();
            assert!(sig.normalize_s().is_some(), "the vector is high-S");
        }
        let mut sig = sig.to_der().as_bytes().to_vec();
        if tamper {
            let n = sig.len();
            sig[n - 1] ^= 1;
        }
        let mut out = Vec::new();
        for part in [&auth, client.as_bytes()] {
            out.extend((part.len() as u16).to_be_bytes());
            out.extend(part);
        }
        out.extend(sig);
        BASE64URL_NOPAD.encode(&out)
    }

    fn verify(key: &str, challenge: &[u8; 32], code: &str) -> std::result::Result<(), String> {
        AdminKey::parse(key).unwrap().verify(challenge, code)
    }

    #[test]
    fn a_known_good_es256_assertion_passes() {
        let (sk, key) = authenticator();
        let record = json!({"type": "invite", "chain": "c1", "invite_id": "i1"});
        let d = record_digest(&record);
        assert_eq!(verify(&key, &d, &code(&sk, &good(d), false)), Ok(()));
    }

    fn refused(a: Assertion, tamper: bool) -> String {
        let (sk, key) = authenticator();
        let d = record_digest(&json!({"type": "revoke", "chain": "c1"}));
        verify(&key, &d, &code(&sk, &a, tamper)).unwrap_err()
    }

    #[test]
    fn an_assertion_for_another_record_is_refused() {
        let other = record_digest(&json!({"type": "revoke", "chain": "c2"}));
        assert!(refused(good(other), false).contains("challenge"));
    }

    #[test]
    fn an_assertion_from_another_origin_is_refused() {
        let d = record_digest(&json!({"type": "revoke", "chain": "c1"}));
        let a = Assertion {
            origin: "https://evil.example",
            ..good(d)
        };
        assert!(refused(a, false).contains("origin"));
    }

    #[test]
    fn an_assertion_without_user_verification_is_refused() {
        let d = record_digest(&json!({"type": "revoke", "chain": "c1"}));
        let a = Assertion {
            flags: 0x01,
            ..good(d)
        };
        assert!(refused(a, false).contains("UV"));
    }

    #[test]
    fn an_assertion_for_another_rp_id_is_refused() {
        let d = record_digest(&json!({"type": "revoke", "chain": "c1"}));
        let a = Assertion {
            rp_id: "evil.example",
            ..good(d)
        };
        assert!(refused(a, false).contains("rpIdHash"));
    }

    #[test]
    fn a_high_s_signature_passes() {
        let (sk, key) = authenticator();
        let d = record_digest(&json!({"type": "revoke", "chain": "c1"}));
        let a = Assertion {
            high_s: true,
            ..good(d)
        };
        assert_eq!(verify(&key, &d, &code(&sk, &a, false)), Ok(()));
    }

    #[test]
    fn a_cross_origin_assertion_is_refused() {
        let d = record_digest(&json!({"type": "revoke", "chain": "c1"}));
        let a = Assertion {
            cross_origin: true,
            ..good(d)
        };
        assert!(refused(a, false).contains("cross-origin"));
    }

    #[test]
    fn a_tampered_signature_is_refused() {
        let d = record_digest(&json!({"type": "revoke", "chain": "c1"}));
        assert_eq!(refused(good(d), true), "bad signature");
    }

    #[test]
    fn a_registration_is_not_an_assertion() {
        let d = record_digest(&json!({"type": "revoke", "chain": "c1"}));
        let a = Assertion {
            kind: "webauthn.create",
            ..good(d)
        };
        assert!(refused(a, false).contains("webauthn.get"));
    }

    #[test]
    fn a_passkey_spki_becomes_its_admin_key() {
        let (sk, key) = authenticator();
        let point = sk.verifying_key().to_encoded_point(false);
        let spki = [
            unhex_vec("3059301306072a8648ce3d020106082a8648ce3d030107034200"),
            point.as_bytes().to_vec(),
        ]
        .concat();
        let json = json!({"alg": -7, "publicKey": BASE64URL_NOPAD.encode(&spki)});
        assert_eq!(passkey_admin_key(&json), Ok(key));
        assert!(
            passkey_admin_key(&json!({"alg": -8, "publicKey": BASE64URL_NOPAD.encode(&spki)}))
                .is_err()
        );
    }
}
