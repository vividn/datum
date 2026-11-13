//! The account record (da-rnxp4): which chains a passkey's account is in,
//! and a member of each to dial, so a device newly signed in joins them with
//! no server of ours. It is a pkarr packet (BEP44, as n0's pkarr relay
//! takes it) under the account key, one TXT record `_datum` holding the
//! list sealed with the account's encryption key: anyone may fetch it, only
//! the account's devices read it, and only its key writes it.
//!
//! A packet's DNS part is at most 1000 bytes, so the list is compact (a
//! chain's id, its database's name, members' endpoint ids, and the relay to
//! reach them by when it is not n0's), and over the limit each chain keeps
//! fewer members, down to one, the most recently seen: a joiner needs one
//! member online, and the chain's records give it the rest. Past one member
//! a chain it is refused, naming the most that fit.

use super::chain::{approve_records, approved, batch, record_digest, Admin, Role};
use super::{digest, hex, unhex};
use aes_gcm::aead::{Aead, KeyInit};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The account key's approval of what `link` carries, as a key file's would
/// be (da-k04lr): the record's [`sign_ed25519`], or a batch's digest signed.
pub fn approve_ed25519(link: &str, secret: &[u8; 32]) -> Option<String> {
    let records = approve_records(link)?;
    let d = match records.as_slice() {
        [one] => record_digest(one),
        all => digest(&batch(all)),
    };
    Some(hex(&SigningKey::from_bytes(secret).sign(&d).to_bytes()))
}

/// The signature of an approval `code` that `admins` take for every one of
/// `records` (each carrying its `batch`, if any): a code may hold several,
/// joined by `.`, as datum.day's does, its passkey asked once for both its
/// WebAuthn assertion and the account key's (da-k04lr). None taken, the
/// first, for the chain to refuse in its own words.
pub fn pick_approval<'a>(admins: &[Admin], records: &[Value], code: &'a str) -> &'a str {
    let mut each = code.trim().split('.');
    let first = each.next().unwrap_or_default();
    std::iter::once(first)
        .chain(each)
        .find(|sig| {
            records.iter().all(|r| {
                let mut r = r.clone();
                r["sig"] = (*sig).into();
                approved(admins, &r, &record_digest(&r)).is_ok()
            })
        })
        .unwrap_or(first)
}

/// The admins a genesis names, to check an approval of it against.
pub fn genesis_admins(genesis: &Value) -> Vec<Admin> {
    let keys = genesis["admins"].as_array().into_iter().flatten();
    keys.filter_map(|a| a["key"].as_str())
        .map(|key| Admin {
            key: key.into(),
            label: String::new(),
            added: String::new(),
        })
        .collect()
}

/// What datum.day asks a passkey's WebAuthn PRF of, as `prf.eval.first`: the
/// same 32 bytes come back on every device the passkey is synced to, and
/// nothing else produces them. The two keys below are derived from them.
pub const ACCOUNT_PRF_SALT: &str = "datum.day account v1";

/// An account's two keys, by HKDF-SHA256 (RFC 5869, no salt) of the PRF
/// output with a fixed `info` each: `sign`, the Ed25519 secret of the
/// account key, an admin of every chain made signed in; `seal`, the key for
/// what only the account's own devices read. A device keeps `seal` and the
/// account key's public half alone, never the PRF output (da-k04lr): the
/// `info` strings differ, so `seal` gives nothing of `sign`, and every admin
/// act asks the passkey again.
pub struct AccountKeys {
    pub sign: [u8; 32],
    pub seal: [u8; 32],
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    match key.len() > 64 {
        true => k[..32].copy_from_slice(&Sha256::digest(key)),
        false => k[..key.len()].copy_from_slice(key),
    }
    let pad = |b: u8| k.map(|x| x ^ b);
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(msg)
        .finalize();
    Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

/// HKDF-SHA256's first 32 bytes of output.
pub(super) fn hkdf32(salt: &[u8], ikm: &[u8], info: &[u8]) -> [u8; 32] {
    let prk = hmac_sha256(salt, ikm);
    hmac_sha256(&prk, &[info, &[1]].concat())
}

pub fn account_keys(prf: &[u8; 32]) -> AccountKeys {
    AccountKeys {
        sign: hkdf32(&[], prf, b"datum account key v1"),
        seal: hkdf32(&[], prf, b"datum account encryption key v1"),
    }
}

/// The TXT record's name, under the account key's zone.
const NAME: &str = "_datum";
/// BEP44's limit on a pkarr packet's DNS part.
const MAX_DNS: usize = 1000;
/// The members a chain lists while they fit.
const FEW: usize = 3;
const NONCE: usize = 12;
/// A pkarr relay's body, as iroh-dns's `SignedPacket::to_relay_payload`
/// has it: the signature, `..SIG`; the seq, `SIG..SEQ`, a u64 big-endian
/// (bencoded only in what the signature covers, `signable`); the DNS
/// packet, `SEQ..`.
const SIG: usize = 64;
const SEQ: usize = SIG + 8;

/// A body's seq.
fn seq(body: &[u8]) -> Option<u64> {
    Some(u64::from_be_bytes(body.get(SIG..SEQ)?.try_into().ok()?))
}

/// One of the account's chains, as the record lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub chain: String,
    pub db: String,
    /// Endpoint ids, hex, the most recently seen first.
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// The relay its members are reached by, when not n0's.
    pub relay: Option<String>,
    pub chains: Vec<Listed>,
}

/// z-base-32, pkarr's spelling of a key in a name and a relay's URL.
pub fn z32(key: &[u8; 32]) -> String {
    let mut spec = data_encoding::Specification::new();
    spec.symbols.push_str("ybndrfg8ejkmcpqxot1uwisza345h769");
    spec.encoding().expect("z-base-32").encode(key)
}

/// The account key's public half.
pub fn public(sign: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(sign).verifying_key().to_bytes()
}

/// The account key, as pkarr names it.
pub fn name(sign: &[u8; 32]) -> String {
    z32(&public(sign))
}

fn put(out: &mut Vec<u8>, s: &[u8]) {
    out.push(s.len().min(255) as u8);
    out.extend_from_slice(&s[..s.len().min(255)]);
}

fn encode(r: &Record) -> Vec<u8> {
    let mut out = vec![1];
    put(&mut out, r.relay.as_deref().unwrap_or("").as_bytes());
    for c in &r.chains {
        put(&mut out, c.chain.as_bytes());
        put(&mut out, c.db.as_bytes());
        let members: Vec<[u8; 32]> = c.members.iter().filter_map(|m| unhex(m)).collect();
        out.push(members.len() as u8);
        members.iter().for_each(|m| out.extend_from_slice(m));
    }
    out
}

fn take<'a>(b: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    let (head, rest) = (b.get(..n)?, b.get(n..)?);
    *b = rest;
    Some(head)
}

fn text(b: &mut &[u8]) -> Option<String> {
    let n = take(b, 1)?[0] as usize;
    String::from_utf8(take(b, n)?.to_vec()).ok()
}

fn decode(mut b: &[u8]) -> Option<Record> {
    (take(&mut b, 1)? == [1]).then_some(())?;
    let relay = Some(text(&mut b)?).filter(|r| !r.is_empty());
    let mut chains = Vec::new();
    while !b.is_empty() {
        let (chain, db) = (text(&mut b)?, text(&mut b)?);
        let n = take(&mut b, 1)?[0] as usize;
        let members = (0..n)
            .map(|_| take(&mut b, 32).map(hex))
            .collect::<Option<_>>()?;
        chains.push(Listed { chain, db, members });
    }
    Some(Record { relay, chains })
}

/// The DNS packet: one answer, `_datum.<zone>` TXT, `text` in strings of
/// 255 bytes at most, a TTL of two hours.
fn packet(zone: &str, text: &str) -> Vec<u8> {
    let mut p = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
    for label in [NAME, zone] {
        put(&mut p, label.as_bytes());
    }
    p.push(0);
    p.extend_from_slice(&[0, 16, 0, 1, 0, 0, 0x1c, 0x20]);
    let chunks: Vec<&[u8]> = text.as_bytes().chunks(255).collect();
    let rdlen = chunks.iter().map(|c| c.len() + 1).sum::<usize>() as u16;
    p.extend_from_slice(&rdlen.to_be_bytes());
    chunks.iter().for_each(|c| put(&mut p, c));
    p
}

/// What BEP44 signs of a packet.
fn signable(micros: u64, packet: &[u8]) -> Vec<u8> {
    let mut s = format!("3:seqi{micros}e1:v{}:", packet.len()).into_bytes();
    s.extend_from_slice(packet);
    s
}

/// The record as a pkarr relay's body, `sig ‖ time ‖ packet`, hex, signed
/// by `sign` and sealed with `seal`, at `micros`; each chain trimmed to as
/// few members as make it fit.
pub fn publish(
    r: &Record,
    sign: &[u8; 32],
    seal: &[u8; 32],
    micros: u64,
) -> Result<String, String> {
    // What the encoding cannot hold is refused, never cut or dropped.
    let texts = r
        .chains
        .iter()
        .flat_map(|c| [&c.chain, &c.db])
        .chain(&r.relay);
    if let Some(t) = texts.into_iter().find(|t| t.len() > 255) {
        return Err(format!(
            "{t:?} is longer than an account record holds (255 bytes)"
        ));
    }
    let members = r.chains.iter().flat_map(|c| &c.members);
    if let Some(m) = members.into_iter().find(|m| unhex::<32>(m).is_none()) {
        return Err(format!("{m:?} is no endpoint id"));
    }
    let key = SigningKey::from_bytes(sign);
    let zone = z32(key.verifying_key().as_bytes());
    // A nonce is the same size whichever, so each try seals with its own.
    let fits = |r: &Record| {
        (1..=FEW).rev().find_map(|few| {
            let mut trimmed = r.clone();
            trimmed
                .chains
                .iter_mut()
                .for_each(|c| c.members.truncate(few));
            let nonce: [u8; NONCE] = rand::random();
            let sealed = aes_gcm::Aes256Gcm::new(seal.into())
                .encrypt(&nonce.into(), encode(&trimmed).as_slice())
                .ok()?;
            let text = data_encoding::BASE64URL_NOPAD.encode(&[&nonce[..], &sealed].concat());
            Some(packet(&zone, &text)).filter(|p| p.len() <= MAX_DNS)
        })
    };
    if let Some(p) = fits(r) {
        let sig = key.sign(&signable(micros, &p)).to_bytes();
        return Ok(hex(&[&sig[..], &micros.to_be_bytes(), &p].concat()));
    }
    // The longest run of the chains that fits, each at one member.
    let most = (1..r.chains.len()).rev().find(|&n| {
        fits(&Record {
            relay: r.relay.clone(),
            chains: r.chains[..n].to_vec(),
        })
        .is_some()
    });
    Err(match most {
        Some(most) => format!(
            "the account is in {} sync chains, and its record holds at most {most}: a pkarr packet is 1000 bytes",
            r.chains.len()
        ),
        None => "not one of the account's sync chains fits its record: a pkarr packet is 1000 bytes".into(),
    })
}

/// The record a relay's `body` (hex) holds, if the account `public` signed
/// it and `seal` opens it.
pub fn open(body: &str, public: &[u8; 32], seal: &[u8; 32]) -> Option<Record> {
    let body = &data_encoding::HEXLOWER_PERMISSIVE
        .decode(body.as_bytes())
        .ok()?;
    let sig = ed25519_dalek::Signature::from_slice(body.get(..SIG)?).ok()?;
    let micros = seq(body)?;
    let p = body.get(SEQ..)?;
    VerifyingKey::from_bytes(public)
        .ok()?
        .verify(&signable(micros, p), &sig)
        .ok()?;
    // Our own packet's one answer: skip the header and the name, then the
    // type, class, TTL and length, to its strings.
    let mut at = 12;
    while *p.get(at)? != 0 {
        at += 1 + *p.get(at)? as usize;
    }
    let rdata = p.get(at + 11..)?;
    let (mut text, mut i) = (Vec::new(), 0);
    while i < rdata.len() {
        let n = rdata[i] as usize;
        text.extend_from_slice(rdata.get(i + 1..i + 1 + n)?);
        i += 1 + n;
    }
    let bytes = data_encoding::BASE64URL_NOPAD.decode(&text).ok()?;
    let (nonce, sealed) = (bytes.get(..NONCE)?, bytes.get(NONCE..)?);
    let plain = aes_gcm::Aes256Gcm::new(seal.into())
        .decrypt(nonce.into(), sealed)
        .ok()?;
    decode(&plain)
}

/// The sequence number a publish signs: `now`, or past the record it read
/// (`held`, hex) where that is ahead, since a relay refuses a packet whose
/// number is lower than the one it holds, and a device's clock may trail
/// the last publisher's.
pub fn stamp(now: u64, held: Option<&str>) -> u64 {
    let held = held.and_then(|h| data_encoding::HEXLOWER_PERMISSIVE.decode(h.as_bytes()).ok());
    held.and_then(|b| seq(&b)).map_or(now, |s| now.max(s + 1))
}

/// `mine`, then the chains of `held` it lacks: a device publishes what it
/// holds itself and what the record held, so a device that has not joined
/// yet drops nothing, and its own members win.
pub fn union(mine: Record, held: Option<Record>) -> Record {
    let mut out = mine;
    for c in held.map(|h| h.chains).unwrap_or_default() {
        if out.chains.iter().all(|m| m.chain != c.chain) {
            out.chains.push(c);
        }
    }
    out
}

impl crate::db::Db {
    /// This copy's chain as the account record lists it, under the name
    /// `db`, when `key` (`ed25519:<hex>`) is one of its admins: this copy
    /// first, then each other live full member, the most recently seen first.
    pub fn sync_account_listed(&self, key: &str, db: &str) -> crate::error::Result<Option<Listed>> {
        let chain = self.sync_chain_served()?;
        if !chain.exists() || chain.admins.iter().all(|a| a.key != key) {
            return Ok(None);
        }
        let mut peers = self.sync_peers()?;
        peers.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        let seen = |e: &str| peers.iter().position(|p| p.id == e).unwrap_or(usize::MAX);
        let me = self.sync_endpoint_id()?;
        let mut members: Vec<String> = chain
            .members
            .iter()
            .filter(|m| m.revoked.is_none() && m.purged.is_none() && m.left.is_none())
            // A joiner sends its full member's secret to each listed member in turn, and a
            // reader or a writer admits no one, so it is no address to send it to (da-omsdk).
            .filter(|m| m.role() == Role::Member)
            .filter(|m| m.endpoint != me)
            .map(|m| m.endpoint.clone())
            .collect();
        members.sort_by_key(|e| seen(e));
        members.insert(0, me);
        Ok(Some(Listed {
            chain: chain.id,
            db: db.to_string(),
            members,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_762_800_000_000_000;

    fn keys(n: u8) -> crate::db::sync::chain::AccountKeys {
        crate::db::sync::chain::account_keys(&[n; 32])
    }

    /// `n` chains, each with `members` members, as an account's databases name them.
    fn chains(n: usize, members: usize) -> Record {
        let chain = |i: usize| Listed {
            chain: format!("01kb{i:022}"),
            db: format!("datum{i}"),
            members: (0..members)
                .map(|m| hex(&[(i * 8 + m) as u8; 32]))
                .collect(),
        };
        Record {
            relay: None,
            chains: (0..n).map(chain).collect(),
        }
    }

    /// Published by one device and opened by another of the account; a
    /// stranger's keys read nothing of it, nor does a byte changed.
    #[test]
    fn the_account_reads_its_record_and_no_one_else_does() {
        let (a, b) = (keys(7), keys(8));
        let mut r = chains(2, 2);
        r.relay = Some("http://127.0.0.1:3340/".into());
        let body = publish(&r, &a.sign, &a.seal, NOW).unwrap();
        assert_eq!(open(&body, &public(&a.sign), &a.seal), Some(r.clone()));
        let bytes = data_encoding::HEXLOWER.decode(body.as_bytes()).unwrap();
        assert_eq!(seq(&bytes), Some(NOW));
        // The seq is the one the signature covers: another opens nothing.
        let mut reseq = bytes.clone();
        reseq[SIG..SEQ].copy_from_slice(&(NOW + 1).to_be_bytes());
        assert_eq!(open(&hex(&reseq), &public(&a.sign), &a.seal), None);
        // Another account: its key did not sign it, and its seal opens nothing.
        assert_eq!(open(&body, &public(&b.sign), &b.seal), None);
        assert_eq!(open(&body, &public(&a.sign), &b.seal), None);
        let mut forged = bytes.clone();
        *forged.last_mut().unwrap() ^= 1;
        assert_eq!(open(&hex(&forged), &public(&a.sign), &a.seal), None);
        // The name a relay keys it by is pkarr's: z-base-32 of the public key.
        assert_eq!(name(&a.sign).len(), 52);
        assert_eq!(z32(&[0; 32]), "y".repeat(52));
    }

    /// The most chains a record holds: each trimmed to its most recently
    /// seen member, every chain kept; one more is refused, naming the most.
    #[test]
    fn over_the_limit_members_go_before_chains_and_past_one_each_it_is_refused() {
        let a = keys(7);
        let most = (1..64)
            .take_while(|&n| publish(&chains(n, 3), &a.sign, &a.seal, NOW).is_ok())
            .last()
            .unwrap();
        assert!(most >= 8, "{most}");
        let body = publish(&chains(most, 3), &a.sign, &a.seal, NOW).unwrap();
        assert!(body.len() <= 2 * (SEQ + MAX_DNS));
        let read = open(&body, &public(&a.sign), &a.seal).unwrap();
        assert_eq!(read.chains.len(), most);
        for (got, all) in read.chains.iter().zip(chains(most, 3).chains) {
            assert_eq!(got.members, all.members[..1]);
        }
        // A few chains keep a few members.
        let few = open(
            &publish(&chains(2, 3), &a.sign, &a.seal, NOW).unwrap(),
            &public(&a.sign),
            &a.seal,
        );
        assert_eq!(few.unwrap().chains[0].members.len(), 3);
        let past = publish(&chains(most + 1, 3), &a.sign, &a.seal, NOW).unwrap_err();
        assert!(
            past.contains(&format!("in {} sync chains", most + 1)),
            "{past}"
        );
        assert!(past.contains(&format!("at most {most}")), "{past}");
    }

    /// A name too long for its byte of length, or a member that is no
    /// endpoint id, is refused: never stored cut or dropped.
    #[test]
    fn what_the_record_cannot_hold_is_refused() {
        let a = keys(7);
        let mut long = chains(1, 1);
        long.relay = Some(format!("https://{}/", "r".repeat(250)));
        assert!(publish(&long, &a.sign, &a.seal, NOW)
            .unwrap_err()
            .contains("255 bytes"));
        let mut bad = chains(1, 1);
        bad.chains[0].members.push("zz".into());
        assert!(publish(&bad, &a.sign, &a.seal, NOW)
            .unwrap_err()
            .contains("no endpoint id"));
        let mut huge = chains(1, 1);
        huge.chains[0].db = "d".repeat(255);
        huge.chains[0].chain = "c".repeat(255);
        huge.relay = Some("r".repeat(255));
        let why = publish(&huge, &a.sign, &a.seal, NOW).unwrap_err();
        assert!(why.starts_with("not one of"), "{why}");
    }

    /// A device whose clock trails the last publisher's still outranks the
    /// record it read, which a relay would otherwise refuse; one ahead
    /// stamps its own clock.
    #[test]
    fn a_publish_outranks_the_record_it_read_whatever_its_clock() {
        let a = keys(7);
        let ahead = publish(&chains(1, 1), &a.sign, &a.seal, NOW + 60_000_000).unwrap();
        let seq = |body: &str| {
            let b = data_encoding::HEXLOWER.decode(body.as_bytes()).unwrap();
            super::seq(&b).unwrap()
        };
        assert_eq!(stamp(NOW, Some(&ahead)), NOW + 60_000_001);
        let next = publish(&chains(1, 1), &a.sign, &a.seal, stamp(NOW, Some(&ahead))).unwrap();
        assert!(seq(&next) > seq(&ahead));
        assert_eq!(stamp(NOW + 120_000_000, Some(&ahead)), NOW + 120_000_000);
        assert_eq!(stamp(NOW, None), NOW);
    }

    /// A device publishes what it holds and what the record held besides.
    #[test]
    fn a_device_publishes_its_chains_and_keeps_the_records_others() {
        let held = chains(2, 2);
        let mut mine = chains(1, 1);
        mine.chains[0].members = vec![hex(&[9; 32])];
        let both = union(mine.clone(), Some(held.clone()));
        assert_eq!(both.chains.len(), 2);
        assert_eq!(both.chains[0], mine.chains[0]);
        assert_eq!(both.chains[1], held.chains[1]);
    }
}
