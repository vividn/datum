//! The sync chain (da-odwc7): signed membership. A copy in a chain accepts
//! ops only from the endpoints the chain admitted.
//!
//! - **Open or locked (da-6177s).** A chain whose admin set is empty is
//!   OPEN: every live member may write what an admin writes, signed by its
//!   own endpoint key like any op of its. The first `add_admin` — any
//!   member's, first-come by design — LOCKS it: from then on those acts take
//!   an admin's signature. Every sync is between members of a chain.
//!
//! - **Records.** Chain records are ops of their own kind (`chain`, under the
//!   hid [`CHAIN_HID`], so an op's `deps` are the records its writer had
//!   seen), synced like any op but folded into membership, not entries. The
//!   op's `effect` is the record: a `type`, the `chain` id, `created`, and
//!   the type's own keys.
//! - **Admin signature.** In a locked chain every record but `admit` and a
//!   member's describe of itself carries `sig`, an admin
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
//!     them, or none (an open chain); its writer becomes the first member.
//!     One per chain.
//!   - `invite {invite_id, secret_hash, expires, caps}`: pre-authorised, so
//!     any member admits its redeemer; one-time, until `expires`. `caps`
//!     holds the role it grants: `{"role": "reader"}`, or `{}` for a member.
//!   - `admit {endpoint, label, via, secret}`: unsigned, written by the
//!     member that redeemed invite `via`, valid while the invite is unused
//!     and unexpired and `secret` hashes to it. The secret is carried so
//!     every copy can check the admitter had it: members hold the invite's
//!     hash, and without the secret any of them could admit anyone through
//!     an open invite. Once the admit folds, the invite is used and the
//!     secret admits no one.
//!   - `revoke {endpoint}`, `add_admin {key, label}`, `remove_admin {key}`
//!     — the last only when the record says `unlock: true` (da-aam46),
//!     which opens the chain again. Every act is judged at its own HLC, so
//!     a member's act refused while the chain was locked stays refused, and
//!     only its acts after the unlock count.
//!   - `purge {endpoint}` (da-ry11c): every op the endpoint signed is set
//!     aside — refused, its entries folded again without them — and it is
//!     revoked from that HLC on, until a fresh invite re-admits it.
//!     An admin act, as a revoke is; no copy purges itself. `unpurge
//!     {endpoint}` is no longer written (da-q4rau: a rejoin undoes a
//!     purge), but one a chain holds still folds as it always did: the
//!     ops come back, the revoke standing, while the purge is the newest
//!     act about it.
//!   - `wipe {scope}` (da-qzrj9): the chain-wide purge, `sync chain purge
//!     --data | --all`. Every entry op made before it is gone on every copy
//!     holding it — its body, its entry and their history scrubbed, its seq
//!     kept so no vector waits on it — and so is what a copy held of them.
//!     An admin act, as a revoke is. `all` DISSOLVES the chain too: the fold
//!     takes no record after it, and a copy holding it is in no chain to its
//!     user, while it keeps its key and the records, to pass the order on.
//!   - `role {endpoint, role}` (da-9yvb6): `member` or `reader` from its
//!     HLC on, an admin act like a revoke. A reader's `role` or invite
//!     `caps` may carry a filter (da-u81u1): `fields`, the fields it is
//!     sent, and `of`, how many the database had when it was written. The
//!     latest record's filter stands; one without `fields` clears it.
//!   - `describe {endpoint, client, label, kind}` (da-rcb5m): what a
//!     member is — the client it runs in (one per CLI data directory or
//!     browser profile, shared by every db there, so one installation's
//!     copies group as one client — da-lfsei), its label, `cli` or `web`. The latest per endpoint wins. Unsigned when an
//!     endpoint describes ITSELF; about another member, an admin act.
//!
//!   - `leave {endpoint}` (da-dsob1): a member's departure, which it signs
//!     itself before it forgets the chain — unsigned by an admin, and the
//!     one act a reader writes but its describe. From its HLC on the member
//!     is out as a revoked one is: dialled by no one, served nothing, and
//!     its ops before it stand. No copy leaves for another.
//!
//!   An `admit` of an endpoint the chain lists already RE-ADMITS it
//!   (da-j8c4l): a revoked device rejoins by a fresh invite as a new one
//!   joins, keeping its endpoint, its first admission and its label, its
//!   revoke cleared and its role the invite's from the revoke on. Its ops
//!   then fold as any member's, those it made while out among them. A
//!   PURGED device rejoins as at a first connection (da-q4rau): its purge
//!   goes too, so every op it made folds back in on every copy.
//!
//!   Every record but a genesis must be written by a member — a member
//!   that is no reader, but for a describe or a leave of itself — and the same
//!   record twice does nothing the second time. Every label a record
//!   carries must pass [`label_problem`]: each is printed on every member.
//! - **Enforcement.** An entry op is allowed when its endpoint is a member,
//!   was no reader when it made it, and, if revoked, made it before the
//!   revoke ([`Chain::allows`]): admitting an endpoint vouches for the data
//!   it brings, and a reader's is refused whenever it was made. A copy with
//!   no chain allows every op.
//! - **Readers (da-9yvb6)** are served everything and serve only their own
//!   ops: they never relay. A reader's copy refuses a write locally. A
//!   filtered reader (da-u81u1) is served only its fields' entries, each
//!   with its spec, and every chain record (`sync.rs`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use data_encoding::BASE64URL_NOPAD;
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{
    check_signature, digest, endpoint, hex, parse_hlc, push, reconsider, relabel_conflicts, settle,
    sha256_hex, unhex, ApplySummary, Op,
};
use crate::db::Db;
use crate::error::{DatumError, Refusal, Result};

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

/// A client id every member can print (`<kind> on <client>`): not blank,
/// and within a label's rules. `client_id()` mints a ULID, but
/// `DATUM_CLIENT_ID` names any — datum.day's is `web-` and eight more
/// (da-6kpj5).
fn is_client_id(s: &str) -> bool {
    !s.trim().is_empty() && label_problem(s).is_none()
}

/// Why a reader's op is refused.
const READER: &str = "a reader in the sync chain: it receives, and can't write";

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

/// The record an approve link carries (or its fragment alone).
pub fn approve_record(link: &str) -> Option<Value> {
    let fragment = link.rsplit('#').next()?.trim();
    serde_json::from_slice(&BASE64URL_NOPAD.decode(fragment.as_bytes()).ok()?).ok()
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
    /// `genesis` or `invite <id>`.
    pub via: String,
    /// From its latest `describe`: its client id and `cli` / `web`.
    pub client: Option<String>,
    pub kind: Option<String>,
    /// The HLC of the revoke: its ops from then on are refused.
    pub revoked: Option<String>,
    /// The HLC of its purge (da-ry11c): every op of its is refused.
    pub purged: Option<String>,
    /// The HLC of its departure (da-dsob1): revoked from then on too.
    pub left: Option<String>,
    /// The HLC of the newest act about it: an unpurge needs it to be the purge.
    pub(super) last_act: String,
    /// The HLC of its latest re-admission (da-j8c4l), if it rejoined.
    pub rejoined: Option<String>,
    /// Its role from each HLC on, oldest first: its admission's, then each
    /// `role` record's.
    pub roles: Vec<(String, Role)>,
    /// A reader's filter, from its latest role (da-u81u1): none, sent all.
    pub filter: Option<Filter>,
}

/// The fields a filtered reader is sent (da-u81u1), each with its spec, and
/// how many fields the database had when the filter was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub fields: BTreeSet<String>,
    pub of: i64,
}

impl Filter {
    /// A record's filter — `fields` and `of` beside a `role` — or why it
    /// has none it may carry: only a reader is filtered.
    fn parse(r: &Value, role: Role) -> std::result::Result<Option<Filter>, String> {
        let Some(fields) = r.get("fields").filter(|f| !f.is_null()) else {
            return Ok(None);
        };
        if role != Role::Reader {
            return Err("only a reader is filtered".into());
        }
        let fields: BTreeSet<String> = fields
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f.as_str().filter(|f| !f.is_empty()).map(Into::into))
            .collect();
        if fields.is_empty() || fields.len() != r["fields"].as_array().map_or(0, Vec::len) {
            return Err("a filter's fields are a list of field names".into());
        }
        match r["of"].as_i64() {
            Some(of) if of >= fields.len() as i64 => Ok(Some(Filter { fields, of })),
            _ => Err("a filter's `of` counts the database's fields, its own among them".into()),
        }
    }

    /// Whether an entry that belongs to `field` passes it.
    pub fn admits(&self, field: Option<&str>) -> bool {
        field.is_some_and(|f| self.fields.contains(f))
    }

    /// «filtered: 5 of 23 fields»
    pub fn describe(&self) -> String {
        format!("filtered: {} of {} fields", self.fields.len(), self.of)
    }
}

/// What a member may write: a reader writes nothing but its own describe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Member,
    Reader,
}

impl Role {
    /// A record's role: none is a member.
    fn parse(v: &Value) -> std::result::Result<Role, String> {
        match v.as_str() {
            None if v.is_null() => Ok(Role::Member),
            Some("member") => Ok(Role::Member),
            Some("reader") => Ok(Role::Reader),
            _ => Err(format!("a role is member or reader, not {v}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Member => "member",
            Role::Reader => "reader",
        }
    }
}

impl Member {
    /// Its role now.
    pub fn role(&self) -> Role {
        self.roles.last().map_or(Role::Member, |r| r.1)
    }

    /// Its role at `hlc`: the latest role it took at or before `hlc`. An op
    /// made before its admission is judged by the role it was admitted
    /// with, so admitting a member vouches for what it brings, and
    /// admitting a reader vouches for nothing.
    fn role_at(&self, hlc: &str) -> Role {
        let at = self.roles.iter().rev().find(|(t, _)| t.as_str() <= hlc);
        at.or(self.roles.first()).map_or(Role::Member, |r| r.1)
    }

    /// Why an unpurge of it would be refused, if it would: it is not purged,
    /// or an act about it came after the purge.
    fn unpurge_refused(&self) -> Option<&'static str> {
        match &self.purged {
            None => Some("not purged"),
            Some(p) if *p != self.last_act => Some("an act about it came after its purge"),
            Some(_) => None,
        }
    }

    /// The HLC from which some of its ops are refused, if any are: its
    /// revoke, or when it first was a reader (all of them, admitted as one);
    /// a purge refuses all of them, whenever made, so it is `""`, not its HLC.
    pub(super) fn refused_from(&self) -> Option<&str> {
        let reader = self.roles.iter().position(|r| r.1 == Role::Reader);
        let reader = reader.map(|i| if i == 0 { "" } else { self.roles[i].0.as_str() });
        let purged = self.purged.as_ref().map(|_| "");
        reader
            .into_iter()
            .chain(self.revoked.as_deref())
            .chain(purged)
            .min()
    }
}

/// A purge order (da-qzrj9): every entry op made before `hlc` is gone.
#[derive(Debug, Clone)]
pub struct Wipe {
    pub hlc: String,
    /// `all` dissolves the chain too; `data` leaves it.
    pub all: bool,
}

#[derive(Debug)]
struct Invite {
    secret_hash: String,
    expires: String,
    used: bool,
    role: Role,
    filter: Option<Filter>,
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
    /// The digests of the records the fold took.
    taken: BTreeSet<[u8; 32]>,
    /// The records that did nothing, by `(endpoint, seq)`, and why.
    pub rejected: BTreeMap<(String, i64), String>,
    /// Each removal of the last admin (da-aam46): the admin's label, and
    /// the HLC it was removed at.
    pub unlocks: Vec<(String, String)>,
    /// The purge orders taken, oldest first (da-qzrj9).
    pub wipes: Vec<Wipe>,
}

/// The wall time of an HLC, as RFC 3339.
pub fn hlc_time(hlc: &str) -> &str {
    hlc.split('/').next().unwrap_or(hlc)
}

impl Chain {
    pub fn exists(&self) -> bool {
        !self.id.is_empty()
    }

    /// No admin: every member may write the admin acts.
    pub fn is_open(&self) -> bool {
        self.admins.is_empty()
    }

    /// The fold took `record`, whatever its `sig`: the same act, signed.
    pub fn holds(&self, record: &Value) -> bool {
        self.taken.contains(&record_digest(record))
    }

    /// The purge order that took an op made at `hlc`, if one did.
    pub fn wiped(&self, hlc: &str) -> Option<&Wipe> {
        self.wipes.iter().find(|w| hlc < w.hlc.as_str())
    }

    /// The order that dissolved the chain (`purge --all`), if one did.
    pub fn dissolved(&self) -> Option<&Wipe> {
        self.wipes.iter().find(|w| w.all)
    }

    pub fn member(&self, endpoint: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.endpoint == endpoint)
    }

    /// Why an op of `endpoint` at `hlc` is refused, if it is.
    pub fn allows(&self, endpoint: &str, hlc: &str) -> std::result::Result<(), String> {
        self.live(endpoint, hlc)?;
        match self.member(endpoint) {
            Some(m) if m.role_at(hlc) == Role::Reader => Err(READER.into()),
            _ => Ok(()),
        }
    }

    /// Why `endpoint` could not write at `hlc`, reader or not.
    fn live(&self, endpoint: &str, hlc: &str) -> std::result::Result<(), String> {
        match self.member(endpoint) {
            _ if !self.exists() => Ok(()),
            None => Err("not a member of the sync chain".into()),
            Some(Member {
                purged: Some(p), ..
            }) => Err(format!(
                "set aside: its endpoint was reverted ({})",
                hlc_time(p)
            )),
            Some(Member {
                revoked: Some(r), ..
            }) if hlc >= r.as_str() => Err(format!(
                "made after its endpoint was revoked ({})",
                hlc_time(r)
            )),
            Some(_) => Ok(()),
        }
    }

    /// A reader's filter, if `endpoint` is a filtered reader now.
    pub fn filter(&self, endpoint: &str) -> Option<&Filter> {
        let m = self.member(endpoint)?;
        m.filter.as_ref().filter(|_| m.role() == Role::Reader)
    }

    /// The chain the ops held make.
    pub(super) fn load(conn: &Connection) -> Result<Chain> {
        Self::load_where(conn, "applied = 1")
    }

    /// The chain the records held make once those still waiting apply too:
    /// what tells a copy it is a filtered reader before the records before
    /// its admit — some filtered out — could ever arrive (da-u81u1).
    pub(super) fn load_with_waiting(conn: &Connection) -> Result<Chain> {
        Self::load_where(conn, "applied IN (0, 1)")
    }

    /// The chain this copy serves a peer by (da-91wab): the one it holds,
    /// with the records still waiting. A reader's admission waits for as
    /// long as an entry op of its inviter's before it is missing, and only
    /// a member holding that op can bring it: a reader never relays. Who
    /// gets a session and what it is sent are read from here; which ops
    /// apply, from [`Chain::load`] alone.
    pub(super) fn served(conn: &Connection) -> Result<Chain> {
        Ok(Self::fold(Self::served_records(conn)?))
    }

    fn served_records(conn: &Connection) -> Result<Vec<Op>> {
        match Self::load(conn)?.exists() {
            true => Self::records(conn, "applied IN (0, 1)"),
            false => Ok(Vec::new()),
        }
    }

    /// The chain this copy serves by, with `lines`, the records of a
    /// dialer it has not heard of (da-2u95o): what the copy would hold once
    /// it had them, to admit the dialer by, and the records of them it read,
    /// each as its canonical op line, which need not be the line as sent.
    /// Each must be signed by its endpoint, as an op applied is, and the
    /// fold checks it as it does any record. Two kinds are not read. A
    /// genesis: this copy holds its chain's, and an earlier one would be the
    /// chain's first record and its writer the first member. And a record of
    /// an endpoint this copy holds as revoked: only that endpoint's own
    /// clock says it wrote it before the revoke, and this copy serves it no
    /// session it could send such a record in.
    pub(super) fn load_with(conn: &Connection, lines: &str) -> Result<(Chain, String)> {
        let mut ops = Self::served_records(conn)?;
        let held = Self::fold(ops.clone());
        let shown = lines.lines().filter_map(|line| {
            let op = Op::parse(line).ok()?;
            let out = held
                .member(&op.endpoint)
                .is_some_and(|m| m.revoked.is_some());
            let read = !out && op.effect["type"] != "genesis";
            (read && check_signature(line, &op).is_ok()).then_some(op)
        });
        let read: Vec<Op> = shown.collect();
        let lines: Vec<&str> = read.iter().map(|op| op.body.as_str()).collect();
        let lines = lines.join("\n");
        ops.extend(read);
        Ok((Self::fold(ops), lines))
    }

    fn load_where(conn: &Connection, applied: &str) -> Result<Chain> {
        Ok(Self::fold(Self::records(conn, applied)?))
    }

    fn records(conn: &Connection, applied: &str) -> Result<Vec<Op>> {
        let sql = format!("SELECT body FROM ops WHERE hid = ?1 AND {applied}");
        conn.prepare_cached(&sql)?
            .query_map(params![CHAIN_HID], |r| r.get::<_, String>(0))?
            .map(|b| Op::parse(&b?))
            .collect()
    }

    fn fold(mut ops: Vec<Op>) -> Chain {
        ops.sort_by(|a, b| (&a.hlc, &a.endpoint).cmp(&(&b.hlc, &b.endpoint)));
        let mut chain = Chain::default();
        for op in &ops {
            if let Err(why) = chain.step(op) {
                chain.rejected.insert((op.endpoint.clone(), op.seq), why);
            }
        }
        chain
    }

    /// Fold one op in, noting its record as taken when it is.
    fn step(&mut self, op: &Op) -> std::result::Result<(), String> {
        self.check(op)?;
        self.taken.insert(record_digest(&op.effect));
        Ok(())
    }

    fn check(&mut self, op: &Op) -> std::result::Result<(), String> {
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
            if s("chain").is_empty() {
                return Err("a genesis names a chain id".into());
            }
            if !admins.is_empty() {
                admin_signed(&admins, &d, &s("sig"))?;
            }
            self.id = s("chain");
            self.admins = admins;
            return self.admit(
                &op.endpoint,
                &s("label"),
                &op.hlc,
                "genesis",
                Role::Member,
                None,
            );
        }
        if !self.exists() {
            return Err("no genesis before it".into());
        }
        if s("chain") != self.id {
            return Err("a record of another chain".into());
        }
        if let Some(w) = self.dissolved() {
            return Err(format!(
                "the sync chain was dissolved by a purge ({})",
                hlc_time(&w.hlc)
            ));
        }
        // A reader describes itself and leaves, and writes nothing else.
        let own = matches!(kind.as_str(), "describe" | "leave") && op.endpoint == s("endpoint");
        if kind == "leave" && !own {
            return Err("a copy leaves the chain only itself".into());
        }
        match own {
            true => self.live(&op.endpoint, &op.hlc),
            false => self.allows(&op.endpoint, &op.hlc),
        }
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
            let (role, filter) = (invite.role, invite.filter.clone());
            return self.admit(
                &s("endpoint"),
                &s("label"),
                &op.hlc,
                &format!("invite {via}"),
                role,
                filter,
            );
        }
        // An endpoint describes itself and leaves unsigned; anything else
        // takes an admin, once there is one: its writer is a live member already.
        if !self.is_open() && !own {
            admin_signed(&self.admins, &d, &s("sig"))?;
        }
        match kind.as_str() {
            "describe" => {
                // A describe written before da-lfsei is `{device: <id>,
                // client: cli|web}`, and a chain record is permanent.
                let (client, kind) = match r.get("device") {
                    Some(device) => (device, &r["client"]),
                    None => (&r["client"], &r["kind"]),
                };
                let kind_ok = match kind {
                    Value::Null => true,
                    Value::String(k) => k == "cli" || k == "web",
                    _ => false,
                };
                let client_ok = match client {
                    Value::Null => true,
                    Value::String(c) => is_client_id(c),
                    _ => false,
                };
                if !kind_ok {
                    return Err("a describe's kind is cli or web".into());
                }
                if !client_ok {
                    return Err("a describe's client is not a client id".into());
                }
                let m = self
                    .members
                    .iter_mut()
                    .find(|m| m.endpoint == s("endpoint"))
                    .ok_or("not a member")?;
                m.label = s("label");
                m.client = client.as_str().map(Into::into);
                m.kind = kind.as_str().map(Into::into);
                m.last_act = op.hlc.clone();
                Ok(())
            }
            "invite" => {
                let role = Role::parse(&r["caps"]["role"])?;
                let filter = Filter::parse(&r["caps"], role)?;
                self.invites.insert(
                    s("invite_id"),
                    Invite {
                        secret_hash: s("secret_hash"),
                        expires: s("expires"),
                        used: false,
                        role,
                        filter,
                    },
                );
                Ok(())
            }
            "role" => {
                let role = Role::parse(&r["role"])?;
                let filter = Filter::parse(r, role)?;
                let m = self
                    .members
                    .iter_mut()
                    .find(|m| m.endpoint == s("endpoint"))
                    .ok_or("not a member")?;
                m.roles.push((op.hlc.clone(), role));
                m.filter = filter;
                m.last_act = op.hlc.clone();
                Ok(())
            }
            "leave" => {
                let m = self
                    .members
                    .iter_mut()
                    .find(|m| m.endpoint == op.endpoint)
                    .ok_or("not a member")?;
                m.left = Some(op.hlc.clone());
                m.revoked.get_or_insert_with(|| op.hlc.clone());
                m.last_act = op.hlc.clone();
                Ok(())
            }
            "revoke" => {
                let m = self
                    .members
                    .iter_mut()
                    .find(|m| m.endpoint == s("endpoint"))
                    .ok_or("not a member")?;
                m.revoked.get_or_insert_with(|| op.hlc.clone());
                m.last_act = op.hlc.clone();
                Ok(())
            }
            "purge" | "unpurge" => {
                if s("endpoint") == op.endpoint {
                    return Err("a copy does not revert itself".into());
                }
                let m = self
                    .members
                    .iter_mut()
                    .find(|m| m.endpoint == s("endpoint"))
                    .ok_or("not a member")?;
                if kind == "unpurge" {
                    if let Some(why) = m.unpurge_refused() {
                        return Err(why.into());
                    }
                    m.purged = None;
                } else if m.purged.is_some() {
                    return Err("already reverted".into());
                } else {
                    m.purged = Some(op.hlc.clone());
                    m.revoked.get_or_insert_with(|| op.hlc.clone());
                }
                m.last_act = op.hlc.clone();
                Ok(())
            }
            "wipe" => {
                let all = match s("scope").as_str() {
                    "data" => false,
                    "all" => true,
                    other => return Err(format!("a purge's scope is data or all, not '{other}'")),
                };
                self.wipes.push(Wipe {
                    hlc: op.hlc.clone(),
                    all,
                });
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
                    if r["unlock"] != Value::Bool(true) {
                        return Err("the last admin is removed only by an unlock".into());
                    }
                    self.unlocks
                        .push((self.admins[at].label.clone(), op.hlc.clone()));
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
        role: Role,
        filter: Option<Filter>,
    ) -> std::result::Result<(), String> {
        if unhex::<32>(endpoint).is_none() {
            return Err("not an endpoint id".into());
        }
        // Re-admitted: the admission vouches for what it brings, as a first
        // one does, so its revoke goes, and what it made while out is judged
        // by the role it is admitted with now. A purged one's purge goes
        // too: it rejoins as at a first connection, all it made with it.
        if let Some(m) = self.members.iter_mut().find(|m| m.endpoint == endpoint) {
            let from = m.revoked.take().unwrap_or_else(|| hlc.into());
            m.purged = None;
            m.left = None;
            m.rejoined = Some(hlc.into());
            m.last_act = hlc.into();
            m.via = via.into();
            m.roles.push((from, role));
            m.filter = filter;
            return Ok(());
        }
        self.members.push(Member {
            endpoint: endpoint.into(),
            label: label.into(),
            admitted: hlc.into(),
            via: via.into(),
            revoked: None,
            purged: None,
            left: None,
            last_act: hlc.into(),
            rejoined: None,
            client: None,
            kind: None,
            roles: vec![(hlc.into(), role)],
            filter,
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
        // A rejoin vouches for what it brings, as an admission does: told
        // once per re-admission (da-j8c4l).
        if let Some(at) = &m.rejoined {
            stmt.execute(params![
                format!("rejoin:{}:{at}", m.endpoint),
                "rejoin",
                m.label,
                hlc_time(at),
                m.via,
                quiet
            ])?;
        }
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
    // An unlock stays until acknowledged on the copy that did it too: every
    // copy may have relied on the lock (da-aam46).
    for (label, hlc) in &chain.unlocks {
        conn.execute(
            "INSERT OR IGNORE INTO sync_notices (subject, kind, label, at, via, announced, acked)
             VALUES (?1, 'unlock', ?2, ?3, 'unlock', ?4, 0)",
            params![format!("unlock:{hlc}"), label, hlc_time(hlc), quiet],
        )?;
    }
    Ok(())
}

/// [`Db::sync_purge_preview`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PurgePreview {
    pub created: usize,
    pub edited: usize,
    pub deleted: usize,
    /// The specs and other definitions it made or changed, which go too.
    pub defined: usize,
    /// When its first and last ops were made, RFC 3339.
    pub first: Option<String>,
    pub last: Option<String>,
}

/// A notice not yet acknowledged.
#[derive(Debug, Clone)]
pub struct Notice {
    pub subject: String,
    /// `endpoint`, `rejoin`, `admin` or `unlock`.
    pub kind: String,
    pub label: String,
    pub at: String,
    pub via: String,
}

impl Notice {
    /// What `datum sync chain ack` takes to acknowledge it: `unlock`, or
    /// the id its line shows.
    pub fn who(&self) -> &str {
        if self.kind == "unlock" {
            return "unlock";
        }
        let id = self.subject.trim_start_matches("rejoin:");
        &id[..id.len().min(18)]
    }
}

impl std::fmt::Display for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = self.who();
        match self.kind.as_str() {
            "admin" => write!(f, "new sync admin {} ({id}) added {}", self.label, self.at),
            "unlock" => write!(f, "sync chain unlocked by {} at {}", self.label, self.at),
            "rejoin" => write!(
                f,
                "sync endpoint {} ({id}) admitted again {} via {}",
                self.label, self.at, self.via
            ),
            _ => write!(
                f,
                "new sync endpoint {} ({id}) admitted {} via {}",
                self.label, self.at, self.via
            ),
        }
    }
}

// ─── The chain on a Db ──────────────────────────────────────────────────────

/// A copy's chain gone: its ops and the chain's local records (notices, a
/// filtered reader's marks). Every entry stays.
fn forget_chain(conn: &Connection) -> Result<()> {
    conn.execute_batch("DELETE FROM ops; DELETE FROM sync_notices; DELETE FROM sync_caught_up;")?;
    Ok(())
}

impl Db {
    /// The chain this copy holds (`exists()` is false with none).
    pub fn sync_chain(&self) -> Result<Chain> {
        Chain::load(&self.conn)
    }

    /// This copy's endpoint id.
    pub fn sync_endpoint_id(&self) -> Result<String> {
        Ok(endpoint(&self.conn)?.id)
    }

    /// The chain's records as this copy holds them, one op line each: what
    /// it shows a member that has not heard it joined (da-2u95o).
    pub fn sync_chain_records(&self) -> Result<Vec<String>> {
        let ops = Chain::records(&self.conn, "applied = 1")?;
        Ok(ops.into_iter().map(|op| op.body).collect())
    }

    /// Write a chain record as an op of this copy's, and refuse it, writing
    /// nothing, when the chain would not take it. `quiet`: see [`note`].
    pub fn sync_chain_write(&self, record: Value, quiet: bool) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        super::debug::refuse(&tx)?;
        // A copy that starts a chain gives up any join it was making
        // (da-38eye): left later, it is joining nothing.
        if record["type"] == "genesis" {
            tx.execute("UPDATE sync_peers SET redeemed_at = NULL", [])?;
        }
        let ep = endpoint(&tx)?;
        let seq = push(&tx, &ep, (CHAIN_HID, CHAIN_HID, None), "chain", record)?;
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
        relabel_conflicts(&tx)?;
        tx.commit()?;
        if s.wiped {
            self.after_wipe()?;
        }
        Ok(())
    }

    /// What a purge of `endpoint` sets aside (da-ry11c): of its ops applied
    /// now — not those a revoke already refused — the entries it created (a
    /// baseline is one it brought), else deleted, else edited, one count per
    /// entry, specs and the other definitions counted apart; and when its
    /// first and last entry ops were made.
    pub fn sync_purge_preview(&self, endpoint: &str) -> Result<PurgePreview> {
        let mut kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut defined = BTreeSet::new();
        let mut out = PurgePreview::default();
        let mut stmt = self.conn.prepare(
            "SELECT hid, kind, hlc, slug FROM ops WHERE endpoint = ?1 AND hid != ?2 AND applied = 1",
        )?;
        let rows = stmt.query_map(params![endpoint, CHAIN_HID], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (hid, kind, hlc, slug) = row?;
            let field = slug.split(':').next().unwrap_or_default();
            if crate::db::is_meta_field(field) {
                defined.insert(hid);
                continue;
            }
            kinds.entry(hid).or_default().insert(kind);
            let at = hlc_time(&hlc).to_string();
            if out.first.as_ref().is_none_or(|f| at < *f) {
                out.first = Some(at.clone());
            }
            if out.last.as_ref().is_none_or(|l| at > *l) {
                out.last = Some(at);
            }
        }
        out.defined = defined.len();
        for k in kinds.values() {
            match () {
                _ if k.contains("create") || k.contains("baseline") => out.created += 1,
                _ if k.contains("delete") => out.deleted += 1,
                _ => out.edited += 1,
            }
        }
        Ok(out)
    }

    /// This copy's client id: one per data directory, shared by its dbs.
    pub fn sync_client_id(&self) -> Result<String> {
        endpoint(&self.conn)?;
        Ok(self
            .conn
            .query_row("SELECT client_id FROM sync_local", [], |r| r.get(0))?)
    }

    /// Write this copy's own `describe`: unsigned, as only an endpoint
    /// describing itself may be.
    pub fn sync_describe_self(&self, label: &str, kind: &str) -> Result<()> {
        let record = json!({
            "type": "describe", "chain": self.sync_chain()?.id,
            "endpoint": self.sync_endpoint_id()?, "client": self.sync_client_id()?,
            "label": label, "kind": kind, "created": Utc::now().to_rfc3339(),
        });
        self.sync_chain_write(record, true)
    }

    /// Redeem an invite for `endpoint`, which `holds` that many entries:
    /// write its `admit`, or say why not. A reader invite refuses a redeemer
    /// with entries of its own, which as a reader it could never send and
    /// its fold would hide (da-tti3w).
    pub fn sync_admit(&self, endpoint: &str, label: &str, secret: &str, holds: u64) -> Result<()> {
        let chain = self.sync_chain()?;
        let hash = sha256_hex(secret);
        let (id, invite) = chain
            .invites
            .iter()
            .find(|(_, i)| i.secret_hash == hash)
            .ok_or_else(|| DatumError::Refused(Refusal::UnknownInvite, UNKNOWN_INVITE.into()))?;
        if invite.used {
            let why = "invite already used".into();
            return Err(DatumError::Refused(Refusal::InviteUsed, why));
        }
        if invite.expires <= Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true) {
            let why = format!("invite expired ({})", invite.expires);
            return Err(DatumError::Refused(Refusal::InviteExpired, why));
        }
        if invite.role == Role::Reader && holds > 0 {
            let entries = if holds == 1 { "entry" } else { "entries" };
            return Err(DatumError::Other(format!(
                "{label} holds {holds} {entries} of its own; a reader writes nothing, so joining \
                 as one would hide them. Connect from a fresh database (`--db`), or ask for a \
                 member invite"
            )));
        }
        let record = json!({
            "type": "admit", "chain": chain.id, "endpoint": endpoint, "label": label,
            "via": id, "secret": secret, "created": Utc::now().to_rfc3339(),
        });
        self.sync_chain_write(record, false)
    }

    /// Leave this copy's chain, whatever it holds (da-j8c4l): it forgets the
    /// chain and its peers, and keeps every entry — its own and those it
    /// received — with their integrity warnings. Its ops go, so it takes a
    /// new endpoint key: numbering its ops from one again under the old key
    /// would read, to a copy holding the old ones, as a second copy signing
    /// with it. The next sync signs every entry again as a baseline, so a
    /// later invite from it starts a new chain vouching for all it holds,
    /// and a later connect joins another chain the same way. Its pause and
    /// client stay (`sync_local`).
    pub fn sync_chain_leave(&self) -> Result<()> {
        if !self.sync_chain()?.exists() {
            return Err(DatumError::Other("this copy is in no sync chain".into()));
        }
        let tx = self.conn.unchecked_transaction()?;
        forget_chain(&tx)?;
        tx.execute_batch("DELETE FROM sync_peers; UPDATE sync_local SET endpoint_secret = '';")?;
        endpoint(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Tell this copy's chain it leaves (da-dsob1): sign its departure,
    /// unless it has, and say which members hold it — each whose last
    /// session ended holding every op this copy signed, the departure its
    /// last, as a copy that signed it writes nothing more. Only a member
    /// that is no reader counts while there is one, since a reader relays
    /// nothing. None: there is no one to tell — no other live member, or
    /// this copy is revoked, and no member would take its record.
    pub fn sync_depart(&self) -> Result<Option<Vec<String>>> {
        let chain = self.sync_chain()?;
        let me = self.sync_endpoint_id()?;
        let Some(mine) = chain.member(&me) else {
            return Ok(None);
        };
        let others: Vec<&Member> = chain
            .members
            .iter()
            .filter(|m| m.endpoint != me && m.revoked.is_none())
            .collect();
        let relays = others.iter().any(|m| m.role() == Role::Member);
        let tell = others
            .iter()
            .filter(|m| !relays || m.role() == Role::Member);
        if others.is_empty() || mine.left.is_none() && mine.revoked.is_some() {
            return Ok(None);
        }
        if mine.left.is_none() {
            let record = json!({
                "type": "leave", "chain": chain.id, "endpoint": me,
                "created": Utc::now().to_rfc3339(),
            });
            self.sync_chain_write(record, true)?;
        }
        let (_, head) = super::own_head(&self.conn)?;
        let mut held = self.conn.prepare(
            "SELECT 1 FROM sync_peers WHERE endpoint_id = ?1
               AND json_extract(last_vector, ?2) >= ?3",
        )?;
        let mut out = Vec::new();
        for m in tell {
            if held.exists(params![m.endpoint, format!("$.\"{me}\""), head])? {
                out.push(m.label.clone());
            }
        }
        Ok(Some(out))
    }

    /// Leave this copy's chain to join another's (da-h7pnc), or with
    /// `dry_run` only say whether it may: when it is the chain's one live
    /// member and holds no op of another endpoint's, so nothing of anyone
    /// else's goes with it — a chain whose other members were revoked still
    /// holds their data (da-mepav). Unlike [`Db::sync_chain_leave`] it keeps
    /// its key, which the chain it joins has just admitted, and `sync_peers`,
    /// where the member that admitted it is recorded.
    pub fn sync_chain_leave_alone(&self, dry_run: bool) -> Result<()> {
        let chain = self.sync_chain()?;
        let me = self.sync_endpoint_id()?;
        let others = chain
            .members
            .iter()
            .filter(|m| m.revoked.is_none() && m.endpoint != me)
            .count();
        let theirs = self
            .conn
            .prepare("SELECT 1 FROM ops WHERE endpoint != ?1")?
            .exists(params![me])?;
        let why = match (others, theirs) {
            _ if !chain.exists() => Some("this copy is in no sync chain".to_string()),
            // A purge dissolved it (da-qzrj9): nothing of anyone's is left.
            _ if chain.dissolved().is_some() => None,
            (0, false) => None,
            (0, true) => Some(format!(
                "this copy holds other copies' data from sync chain {}",
                chain.id
            )),
            (n, _) => Some(format!("sync chain {} has {n} other member(s)", chain.id)),
        };
        if let Some(why) = why {
            return Err(DatumError::Other(format!(
                "{why}; only a copy holding nothing but its own data leaves its chain to join \
                 another (`datum sync chain leave` first, keeping every entry)"
            )));
        }
        if dry_run {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        forget_chain(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Notices not yet printed.
    pub fn sync_notices_unannounced(&self) -> Result<Vec<Notice>> {
        self.sync_notices("announced = 0")
    }

    /// Mark a notice printed.
    pub fn sync_notice_announced(&self, notice: &Notice) -> Result<()> {
        self.conn.execute(
            "UPDATE sync_notices SET announced = 1 WHERE subject = ?1",
            params![notice.subject],
        )?;
        Ok(())
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

    /// `datum sync chain ack <who>`: acknowledge the notices of an endpoint
    /// or an admin key — by label, or by at least [`MIN_PREFIX`] characters
    /// of its id (`ed25519:ab…` or `ab…`), naming one subject — or every
    /// unlock (`unlock`), and settle the
    /// `sync_unauthorised` warnings of that endpoint (a refused endpoint has
    /// no notice: name it by its whole id). How many rows it settled.
    pub fn sync_ack(&self, who: &str) -> Result<usize> {
        let prefix = |id: &str| who.len() >= MIN_PREFIX && id.starts_with(who);
        // `unlock` is a word of its own: it never names a member or admin
        // that happens to carry that label.
        let named = |n: &Notice| match who {
            "unlock" => n.kind == "unlock",
            _ => n.label == who,
        };
        let hits: Vec<Notice> = self
            .sync_notices_unacked()?
            .into_iter()
            .filter(|n| {
                named(n)
                    || who != "unlock"
                        && (prefix(&n.subject)
                            || n.subject.split_once(':').is_some_and(|(_, k)| prefix(k)))
            })
            .collect();
        let mut subjects: BTreeSet<&str> = hits.iter().map(|n| n.subject.as_str()).collect();
        // An endpoint's rejoins (`rejoin:<id>:<hlc>`) are that endpoint's.
        fn whose(s: &str) -> &str {
            s.strip_prefix("rejoin:")
                .map_or(s, |r| r.split(':').next().unwrap_or(r))
        }
        let named_ids: BTreeSet<&str> = subjects.iter().map(|s| whose(s)).collect();
        if named_ids.len() > 1 && !hits.iter().all(named) {
            return Err(DatumError::Other(format!(
                "{who} names {} subjects; give more of the id",
                named_ids.len()
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
                params![
                    format!("\"op\":\"{}/", whose(subject)),
                    Utc::now().to_rfc3339()
                ],
            )?;
        }
        Ok(settled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chain `records` fold to, each written by one member at its HLC
    /// (`t` seconds), signed by `admin` when given: what each did.
    fn fold(
        records: &[(u32, Value, Option<&[u8; 32]>)],
    ) -> (Chain, Vec<std::result::Result<(), String>>) {
        let mut chain = Chain::default();
        let done = records
            .iter()
            .enumerate()
            .map(|(seq, (t, r, admin))| {
                let mut r = r.clone();
                if let Some(secret) = admin {
                    r["sig"] = sign_ed25519(&r, secret).into();
                }
                let op = json!({
                    "endpoint": "ab".repeat(32), "seq": seq + 1, "hlc": format!("2026-09-29T00:00:{t:02}.000Z/0000"),
                    "hid": CHAIN_HID, "slug": CHAIN_HID, "kind": "chain", "effect": r, "sig": "",
                });
                chain.step(&Op::parse(&op.to_string()).unwrap())
            })
            .collect();
        (chain, done)
    }

    /// The last admin goes only with `unlock` (da-aam46), and the chain is
    /// open from then on — not before: a member's act made while it was
    /// locked stays refused.
    #[test]
    fn the_last_admin_is_removed_only_by_an_unlock() {
        let secret = [3u8; 32];
        let key = ed25519_admin_key(&secret);
        let c = |r: Value| {
            let mut r = r;
            r["chain"] = "c1".into();
            r
        };
        let genesis =
            c(json!({"type": "genesis", "admins": [{"key": key, "label": "k"}], "label": "a"}));
        let remove = c(json!({"type": "remove_admin", "key": key}));
        let mut unlock = remove.clone();
        unlock["unlock"] = true.into();
        let invite = |id: &str| {
            c(
                json!({"type": "invite", "invite_id": id, "secret_hash": "h", "expires": "2099-01-01T00:00:00Z", "caps": {}}),
            )
        };
        let (chain, done) = fold(&[
            (1, genesis, Some(&secret)),
            (2, invite("locked"), None),
            (3, remove, Some(&secret)),
            (4, unlock, Some(&secret)),
            (5, invite("open"), None),
        ]);
        assert!(done[1]
            .as_ref()
            .unwrap_err()
            .contains("not signed by an admin"));
        assert_eq!(
            done[2],
            Err("the last admin is removed only by an unlock".into())
        );
        assert_eq!(done[3], Ok(()));
        assert_eq!(done[4], Ok(()));
        assert!(chain.is_open());
        assert_eq!(chain.unlocks.len(), 1);
        assert_eq!(chain.unlocks[0].0, "k");
    }

    /// A purge (da-ry11c) refuses every op of its endpoint, whenever made,
    /// and revokes it; no copy purges itself; an old chain's unpurge still
    /// stands only while the purge is the newest act about the endpoint,
    /// and leaves the revoke.
    #[test]
    fn a_purge_refuses_every_op_and_an_unpurge_needs_it_to_be_the_newest_act() {
        let (a, b) = ("ab".repeat(32), "cd".repeat(32));
        let at = |t: u32| format!("2026-09-29T00:00:{t:02}.000Z/0000");
        let mut chain = Chain::default();
        let mut step = |t: u32, rec: Value| {
            let mut rec = rec;
            rec["chain"] = "c1".into();
            rec["created"] = at(t).into();
            let op = json!({
                "endpoint": a, "seq": t, "hlc": at(t), "hid": CHAIN_HID,
                "slug": CHAIN_HID, "kind": "chain", "effect": rec, "sig": "",
            });
            let done = chain.step(&Op::parse(&op.to_string()).unwrap());
            let m = chain
                .member(&b)
                .map(|m| (m.purged.is_some(), m.revoked.is_some()));
            (done, m, chain.allows(&b, &at(1)).is_ok())
        };
        let it = |kind: &str, who: &str| json!({"type": kind, "endpoint": who});
        assert_eq!(
            step(1, json!({"type": "genesis", "admins": [], "label": "a"})).0,
            Ok(())
        );
        let invite = json!({"type": "invite", "invite_id": "i", "secret_hash": sha256_hex("s"),
            "expires": "2099-01-01T00:00:00Z", "caps": {}});
        assert_eq!(step(2, invite).0, Ok(()));
        let admit =
            json!({"type": "admit", "endpoint": b, "label": "b", "via": "i", "secret": "s"});
        assert_eq!(step(3, admit).0, Ok(()));
        assert_eq!(step(4, it("unpurge", &b)).0, Err("not purged".into()));
        assert_eq!(
            step(5, it("purge", &a)).0,
            Err("a copy does not revert itself".into())
        );
        // An op from before the purge — and before the admit — is refused.
        assert_eq!(
            step(6, it("purge", &b)),
            (Ok(()), Some((true, true)), false)
        );
        assert_eq!(chain_err(&step(7, it("purge", &b))), "already reverted");
        assert_eq!(
            step(8, it("unpurge", &b)),
            (Ok(()), Some((false, true)), true)
        );
        assert_eq!(step(9, it("purge", &b)).0, Ok(()));
        assert_eq!(step(10, it("revoke", &b)).0, Ok(()));
        assert_eq!(
            chain_err(&step(11, it("unpurge", &b))),
            "an act about it came after its purge"
        );
        // A fresh invite re-admits it as at a first connection (da-q4rau):
        // the purge goes with the revoke, and its old op is allowed again.
        let invite = json!({"type": "invite", "invite_id": "j", "secret_hash": sha256_hex("t"),
            "expires": "2099-01-01T00:00:00Z", "caps": {}});
        assert_eq!(step(12, invite).0, Ok(()));
        let admit =
            json!({"type": "admit", "endpoint": b, "label": "b", "via": "j", "secret": "t"});
        assert_eq!(step(13, admit), (Ok(()), Some((false, false)), true));
        assert_eq!(chain_err(&step(14, it("unpurge", &b))), "not purged");
    }

    fn chain_err<T>(r: &(std::result::Result<(), String>, T, bool)) -> String {
        r.0.clone().unwrap_err()
    }

    /// `holds` (da-bjxts) knows a record the fold took by its unsigned
    /// form, which is what a CLI waiting at the passkey prompt holds, and
    /// not one it refused.
    #[test]
    fn holds_is_the_taken_record_signed_or_not() {
        let secret = [4u8; 32];
        let key = ed25519_admin_key(&secret);
        let genesis = json!({"type": "genesis", "chain": "c1", "admins": [{"key": key, "label": "k"}], "label": "a"});
        let revoke = json!({"type": "revoke", "chain": "c1", "endpoint": "ab".repeat(32)});
        let refused = json!({"type": "revoke", "chain": "c1", "endpoint": "cd".repeat(32)});
        let (chain, done) = fold(&[
            (1, genesis, Some(&secret)),
            (2, revoke.clone(), Some(&secret)),
            (3, refused.clone(), None),
        ]);
        assert_eq!(done[1], Ok(()));
        assert!(done[2].is_err());
        let mut signed = revoke.clone();
        signed["sig"] = sign_ed25519(&revoke, &secret).into();
        assert!(chain.holds(&revoke) && chain.holds(&signed));
        assert!(!chain.holds(&refused));
    }

    /// A reader (da-9yvb6): admitted by a reader invite, it describes
    /// itself and writes nothing else — no invite, revoke, role or describe
    /// of another, and no entry op — in an open chain as in a locked one,
    /// even signed by an admin key. Its role changes at a `role` record's
    /// HLC, either way.
    #[test]
    fn a_readers_acts_are_refused_and_its_role_changes_at_a_records_hlc() {
        let (a, r) = ("ab".repeat(32), "cd".repeat(32));
        let at = |t: u32| format!("2026-09-29T00:00:{t:02}.000Z/0000");
        for locked in [false, true] {
            let secret = [5u8; 32];
            let admins = match locked {
                true => json!([{"key": ed25519_admin_key(&secret), "label": "k"}]),
                false => json!([]),
            };
            let mut chain = Chain::default();
            let step = |chain: &mut Chain, t: u32, by: &str, mut rec: Value| {
                rec["chain"] = "c1".into();
                rec["created"] = at(t).into();
                if locked && rec["type"] != "admit" {
                    rec["sig"] = sign_ed25519(&rec, &secret).into();
                }
                let op = json!({
                    "endpoint": by, "seq": t, "hlc": at(t), "hid": CHAIN_HID,
                    "slug": CHAIN_HID, "kind": "chain", "effect": rec, "sig": "",
                });
                let done = chain.step(&Op::parse(&op.to_string()).unwrap());
                (done, chain.allows(&r, &at(t)).is_ok())
            };
            let describe = |who: &str| json!({"type": "describe", "endpoint": who, "label": "x"});
            let role =
                |who: &str, role: &str| json!({"type": "role", "endpoint": who, "role": role});
            let invite = json!({"type": "invite", "invite_id": "i1", "secret_hash": sha256_hex("s"),
                "expires": "2099-01-01T00:00:00Z", "caps": {"role": "reader"}});
            assert_eq!(
                step(
                    &mut chain,
                    1,
                    &a,
                    json!({"type": "genesis", "admins": admins, "label": "a"})
                )
                .0,
                Ok(())
            );
            assert_eq!(step(&mut chain, 2, &a, invite).0, Ok(()));
            let admit =
                json!({"type": "admit", "endpoint": r, "label": "r", "via": "i1", "secret": "s"});
            assert_eq!(step(&mut chain, 3, &a, admit), (Ok(()), false));
            assert_eq!(chain.member(&r).unwrap().role(), Role::Reader);
            assert_eq!(step(&mut chain, 4, &r, describe(&r)), (Ok(()), false));
            let invite = json!({"type": "invite", "invite_id": "i2", "secret_hash": "h",
                "expires": "2099-01-01T00:00:00Z", "caps": {}});
            for (t, rec) in [
                (5, invite),
                (6, json!({"type": "revoke", "endpoint": a})),
                (7, role(&r, "member")),
                (8, describe(&a)),
            ] {
                let (done, allowed) = step(&mut chain, t, &r, rec);
                assert_eq!(done, Err(format!("its writer is {READER}")), "{t}");
                assert!(!allowed);
            }
            // Promoted at 10: allowed from then on, and not before.
            assert_eq!(step(&mut chain, 10, &a, role(&r, "member")), (Ok(()), true));
            assert!(chain.allows(&r, &at(9)).is_err());
            // A member demoted at 12: allowed before it, refused after.
            assert_eq!(step(&mut chain, 12, &a, role(&a, "reader")).0, Ok(()));
            assert!(chain.allows(&a, &at(11)).is_ok());
            assert_eq!(chain.allows(&a, &at(13)), Err(READER.into()));
            assert_eq!(
                step(&mut chain, 14, &a, role(&r, "admin")).0.unwrap_err(),
                format!("its writer is {READER}")
            );
            assert_eq!(
                step(&mut chain, 15, &r, role(&r, "admin")).0.unwrap_err(),
                "a role is member or reader, not \"admin\""
            );
        }
    }

    /// A departure (da-dsob1) is its own writer's, unsigned in a locked
    /// chain and a reader's too: from its HLC on the member is out, its ops
    /// before it stand, and no copy leaves for another or leaves twice.
    #[test]
    fn a_member_or_reader_leaves_only_itself_and_is_out_from_then_on() {
        let (a, r) = ("ab".repeat(32), "cd".repeat(32));
        let at = |t: u32| format!("2026-09-29T00:00:{t:02}.000Z/0000");
        let secret = [6u8; 32];
        let mut chain = Chain::default();
        let step = |chain: &mut Chain, t: u32, by: &str, mut rec: Value| {
            rec["chain"] = "c1".into();
            rec["created"] = at(t).into();
            if !matches!(rec["type"].as_str(), Some("admit" | "leave")) {
                rec["sig"] = sign_ed25519(&rec, &secret).into();
            }
            let op = json!({
                "endpoint": by, "seq": t, "hlc": at(t), "hid": CHAIN_HID,
                "slug": CHAIN_HID, "kind": "chain", "effect": rec, "sig": "",
            });
            chain.step(&Op::parse(&op.to_string()).unwrap())
        };
        let admins = json!([{"key": ed25519_admin_key(&secret), "label": "k"}]);
        let leave = |who: &str| json!({"type": "leave", "endpoint": who});
        assert_eq!(
            step(
                &mut chain,
                1,
                &a,
                json!({"type": "genesis", "admins": admins, "label": "a"})
            ),
            Ok(())
        );
        let invite = json!({"type": "invite", "invite_id": "i", "secret_hash": sha256_hex("s"),
            "expires": "2099-01-01T00:00:00Z", "caps": {"role": "reader"}});
        assert_eq!(step(&mut chain, 2, &a, invite), Ok(()));
        let admit =
            json!({"type": "admit", "endpoint": r, "label": "r", "via": "i", "secret": "s"});
        assert_eq!(step(&mut chain, 3, &a, admit), Ok(()));
        assert_eq!(
            step(&mut chain, 4, &a, leave(&r)),
            Err("a copy leaves the chain only itself".into())
        );
        assert_eq!(step(&mut chain, 5, &r, leave(&r)), Ok(()));
        assert_eq!(step(&mut chain, 6, &a, leave(&a)), Ok(()));
        let m = chain.member(&a).unwrap();
        assert_eq!(
            (m.left.as_deref(), m.revoked.as_deref()),
            (Some(at(6).as_str()), Some(at(6).as_str()))
        );
        assert!(chain.member(&r).unwrap().left.is_some());
        assert!(chain.allows(&a, &at(5)).is_ok());
        assert!(chain.allows(&a, &at(7)).is_err());
        assert!(step(&mut chain, 7, &a, leave(&a))
            .unwrap_err()
            .contains("revoked"));
    }

    /// A filter (da-u81u1) is a reader's, names its fields, and its `of`
    /// counts them among the database's: a record saying less is refused.
    #[test]
    fn a_filter_is_a_readers_and_its_of_counts_its_own_fields() {
        let f = |v: Value, role| Filter::parse(&v, role);
        let ok = f(json!({"fields": ["a"], "of": 3}), Role::Reader);
        assert_eq!(ok.unwrap().unwrap().of, 3);
        assert_eq!(f(json!({}), Role::Reader), Ok(None));
        for bad in [
            json!({"fields": ["a", "b"], "of": 1}),
            json!({"fields": ["a"]}),
            json!({"fields": [], "of": 1}),
            json!({"fields": ["a", 2], "of": 2}),
        ] {
            assert!(f(bad.clone(), Role::Reader).is_err(), "{bad}");
        }
        assert!(f(json!({"fields": ["a"], "of": 3}), Role::Member).is_err());
    }

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
