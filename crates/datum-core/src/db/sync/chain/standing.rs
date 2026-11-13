//! The standing admission (da-zkwiu): a device's code, which admits each
//! fresh copy of it by its device key, replacing the copies before.
//!
//! `standing {device, caps, label, expires?}` is an admin act as a revoke
//! is, until `expires` or a `revoke_standing {device}`; a later one for the
//! same device replaces it. A copy joins by it in two records (da-tkbtu):
//!
//! - `claim {endpoint, device_key, device_sig}`, the endpoint's own op, so
//!   its consent: the device's key signs for the endpoint ([`bind_device`]),
//!   which the device's key does for any endpoint id it is shown, and the
//!   endpoint's own signature says it agreed. Another's copy of it is
//!   refused, and refused before it is seen, so a copy backdated before it
//!   keeps it out neither.
//! - `admit {endpoint, label, via: "standing"}`, a live member's, written at
//!   the member's clock as an invite's admit is, which admits an endpoint
//!   whose claim the chain holds, once, judging the standing's revoke and
//!   expiry at the member's clock and not the joiner's. It retires the
//!   device's other live endpoints in the chain, each revoked at the admit's
//!   HLC, its ops before it standing. A copy made afresh needs a new endpoint
//!   (two copies with one key would number their ops from one seq), and is
//!   still one device.

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde_json::{json, Value};

use super::{bind_device, bound_device, expired, hlc_time, role_of};
use super::{check_signature, Chain, Filter, Role, CHAIN_HID};
use crate::db::sync::Op;
use crate::db::sync::{advance, clock_ahead, format_hlc, next_hlc, now_ms, parse_hlc, unhex};
use crate::db::Db;
use crate::error::{DatumError, Result};

/// What a dialer carries where an invite's secret goes to be admitted by
/// its device's standing admission (da-zkwiu): then, a line each, every op
/// it has signed up to its own claim ([`Db::sync_standing_claim`]).
pub const STANDING: &str = "standing:";

/// Why another endpoint's copy of a claim is refused (da-tkbtu).
const NOT_OWN: &str = "a standing claim is its endpoint's own op";

/// A reader writes nothing, so a redeemer holding entries of its own may
/// not join as one: its fold would hide them (da-tti3w).
pub(super) fn reader_holds(role: Role, label: &str, holds: u64) -> Result<()> {
    if role != Role::Reader || holds == 0 {
        return Ok(());
    }
    let entries = if holds == 1 { "entry" } else { "entries" };
    Err(DatumError::Other(format!(
        "{label} holds {holds} {entries} of its own; a reader writes nothing, so joining \
         as one would hide them. Join from a fresh database (`--db`), or ask for a \
         member invite"
    )))
}

/// A `standing` record (da-zkwiu): what its device's endpoints are
/// admitted as, until when, and since when it admits none.
#[derive(Debug, Clone)]
pub struct Standing {
    pub label: String,
    pub role: Role,
    pub filter: Option<Filter>,
    pub expires: Option<String>,
    pub hlc: String,
    pub revoked: Option<String>,
    /// It has admitted an endpoint: a notice of a new device is told once
    /// per standing record (da-w27bp).
    pub(super) admitted: bool,
}

impl Chain {
    /// A `standing` record: its device's endpoints are admitted from here on.
    pub(super) fn take_standing(&mut self, op: &Op) -> std::result::Result<(), String> {
        let r = &op.effect;
        let s = |k: &str| r[k].as_str().unwrap_or_default().to_string();
        let device = s("device");
        if unhex::<32>(&device).is_none() {
            return Err("a standing admission names a device key".into());
        }
        let role = role_of(&r["caps"]["role"])?;
        let filter = Filter::parse(&r["caps"], role)?;
        let expires = match r.get("expires").filter(|e| !e.is_null()) {
            None => None,
            Some(e) => match e.as_str().map(DateTime::parse_from_rfc3339) {
                Some(Ok(_)) => e.as_str().map(Into::into),
                _ => return Err("a standing admission expires at an RFC 3339 time".into()),
            },
        };
        let label = s("label");
        let hlc = op.hlc.clone();
        let st = Standing {
            label,
            role,
            filter,
            expires,
            hlc,
            revoked: None,
            admitted: false,
        };
        self.standings.insert(device, st);
        Ok(())
    }

    /// A `revoke_standing` record: its device's standing admission admits
    /// no one from here on.
    pub(super) fn revoke_standing(&mut self, op: &Op) -> std::result::Result<(), String> {
        let s = |k: &str| op.effect[k].as_str().unwrap_or_default().to_string();
        let st = self
            .standings
            .get_mut(&s("device"))
            .ok_or("no standing admission for that device")?;
        if st.revoked.is_some() {
            return Err("its standing admission is revoked already".into());
        }
        st.revoked = Some(op.hlc.clone());
        Ok(())
    }

    /// A `claim` (da-tkbtu): its endpoint's own op, which a standing admit
    /// of it then needs. A later one replaces it: a dial again.
    pub(super) fn take_claim(&mut self, op: &Op) -> std::result::Result<(), String> {
        let r = &op.effect;
        let endpoint = r["endpoint"].as_str().unwrap_or_default();
        if op.endpoint != endpoint {
            return Err(NOT_OWN.into());
        }
        if !self.exists() || r["chain"] != self.id.as_str() {
            return Err("a claim of another sync chain".into());
        }
        let device = bound_device(r).ok_or("not signed for by its device's key")?;
        self.claims.insert(endpoint.into(), device);
        Ok(())
    }

    /// An admit by its device's standing admission (da-zkwiu), a live
    /// member's: of an endpoint whose own claim the chain holds (da-tkbtu)
    /// and does not hold yet, at the member's clock. It retires the
    /// device's other live endpoints from the admit on.
    pub(super) fn admit_standing(&mut self, op: &Op) -> std::result::Result<(), String> {
        let r = &op.effect;
        let endpoint = r["endpoint"].as_str().unwrap_or_default();
        let device = self.claims.get(endpoint).cloned();
        let device = device.ok_or("no standing claim of its own by that endpoint")?;
        // Once: a retired or revoked endpoint, its key an old copy's, comes
        // back by an invite, and retires no one again. A live one's again,
        // a dial again, changes nothing.
        if let Some(m) = self.member(endpoint) {
            if m.revoked.is_none() && m.device.as_deref() == Some(device.as_str()) {
                return Ok(());
            }
            return Err(format!(
                "{endpoint} is in the chain already: a standing admission admits an \
                 endpoint once, and one retired comes back by an invite"
            ));
        }
        let st = self
            .standings
            .get(&device)
            .ok_or("no standing admission for its device")?;
        if let Some(at) = &st.revoked {
            let at = hlc_time(at);
            return Err(format!(
                "its device's standing admission was revoked ({at})"
            ));
        }
        if let Some(e) = st.expires.as_deref().filter(|e| expired(&op.hlc, e)) {
            return Err(format!("its device's standing admission expired ({e})"));
        }
        let (role, filter) = (st.role, st.filter.clone());
        // As `invite <id>` names its invite: the device's key is `device`.
        let via = format!("standing {}", &device[..8]);
        let label = r["label"].as_str().unwrap_or_default();
        self.admit(endpoint, label, &op.hlc, &via, role, filter)?;
        // Its first admit is news; one after it, the device again (da-w27bp).
        let st = self.standings.get_mut(&device).expect("judged above");
        let returned = std::mem::replace(&mut st.admitted, true);
        for m in self.members.iter_mut() {
            if m.endpoint == endpoint {
                (m.device, m.claimed, m.returned) = (Some(device.clone()), true, returned);
            } else if m.device.as_deref() == Some(device.as_str()) && m.revoked.is_none() {
                m.revoked = Some(op.hlc.clone());
                m.last_act = op.hlc.clone();
            }
        }
        Ok(())
    }
}

impl Db {
    /// This copy's own claim to `chain` by the device whose secret is
    /// `device` (da-tkbtu), signed as its next op, with the proof a dial
    /// carries: [`STANDING`] and every op this copy has signed up to it,
    /// entries too, since a member applies an endpoint's ops in seq order
    /// and one missing would hold the claim behind it. A dial again signs
    /// another.
    pub fn sync_standing_claim(&self, device: &[u8; 32], chain: &str) -> Result<String> {
        let me = self.sync_endpoint_id()?;
        let (key, sig) = bind_device(device, chain, &me);
        let record = json!({
            "type": "claim", "chain": chain, "endpoint": me,
            "device_key": key, "device_sig": sig, "created": Utc::now().to_rfc3339(),
        });
        self.sync_chain_write(record, true)?;
        let lines = self
            .conn
            .prepare("SELECT body FROM ops WHERE endpoint = ?1 ORDER BY seq")?
            .query_map(params![me], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(format!("{STANDING}{}", lines.join("\n")))
    }

    /// Admit `endpoint` by the standing admission its own claim in `proof`
    /// names (da-zkwiu, da-tkbtu), `proof` a [`Db::sync_standing_claim`]'s
    /// after its prefix: judge the claim and an admit at this copy's clock
    /// against the chain it holds, then apply its lines and write the admit,
    /// or say why the chain refuses it, holding nothing of the dial
    /// (da-mmcbl): a claim kept would travel on with the chain.
    pub fn sync_admit_standing(
        &self,
        endpoint: &str,
        label: &str,
        proof: &str,
        holds: u64,
    ) -> Result<()> {
        let refuse = |why: String| Err(DatumError::Other(why));
        let last = proof.lines().last().unwrap_or_default();
        let op = Op::parse(last)?;
        let r = &op.effect;
        if op.endpoint != endpoint || r["type"] != "claim" {
            return refuse(format!("{label} carries no standing claim of its own"));
        }
        if let Err(why) = check_signature(last, &op) {
            return refuse(format!("{label}'s standing claim: {why}"));
        }
        let device_key = r["device_key"].as_str().unwrap_or_default();
        let chain = self.sync_chain()?;
        // A dial again of a copy its claim admitted: it is in, and nothing is written.
        let live =
            |m: &super::Member| m.revoked.is_none() && m.device.as_deref() == Some(device_key);
        if chain.member(endpoint).is_some_and(live) {
            return Ok(());
        }
        let st = chain.standings.get(device_key).ok_or_else(|| {
            DatumError::Other("no standing admission for this device in the sync chain".into())
        })?;
        reader_holds(st.role, label, holds)?;
        let record = json!({
            "type": "admit", "chain": chain.id, "endpoint": endpoint, "label": label,
            "via": "standing", "created": Utc::now().to_rfc3339(),
        });
        if let Some(why) = self.sync_standing_judged(&op, &record)? {
            return refuse(format!("the sync chain refuses it: {why}"));
        }
        self.sync_apply(proof)?;
        self.sync_chain_write(record, false)
    }

    /// How the chain this copy holds would refuse `claim` and then `admit`,
    /// this copy's next op, at the HLC its write takes once the claim has
    /// applied. A claim dated past [`MAX_CLOCK_AHEAD`](crate::db::sync::MAX_CLOCK_AHEAD) would be held, not
    /// applied, so the admit after it is refused.
    fn sync_standing_judged(&self, claim: &Op, admit: &Value) -> Result<Option<String>> {
        let now = now_ms();
        if clock_ahead(&claim.hlc, now).is_some() {
            let at = hlc_time(&claim.hlc);
            return Ok(Some(format!(
                "its claim is dated ahead of this copy's clock ({at})"
            )));
        }
        let me = self.sync_endpoint_id()?;
        let next = next_hlc(&self.conn)?;
        let hlc = match parse_hlc(&claim.hlc).filter(|_| claim.hlc >= next) {
            Some(c) => {
                let (ms, counter) = advance(Some(c), now);
                format_hlc(ms, counter)
            }
            None => next,
        };
        let op = json!({"endpoint": me, "seq": i64::MAX, "hlc": hlc,
            "hid": CHAIN_HID, "slug": CHAIN_HID, "kind": "chain", "effect": admit, "sig": ""});
        let admit = Op::parse(&op.to_string())?;
        let key = |o: &Op| (o.endpoint.clone(), o.seq);
        let mut ops = Chain::records(&self.conn, "applied = 1")?;
        ops.retain(|o| key(o) != key(claim));
        ops.extend([claim.clone(), admit.clone()]);
        let rejected = Chain::fold(ops).rejected;
        Ok([claim, &admit]
            .iter()
            .find_map(|o| rejected.get(&key(o)).cloned()))
    }
}

#[cfg(test)]
mod tests {

    use super::super::{ed25519_admin_key, secret_hash, sign_ed25519, CHAIN_HID};
    use super::*;
    use crate::db::sync::hex;
    use ed25519_dalek::SigningKey;

    /// `refused` is a refusal that says `why`.
    fn refuses(refused: Option<String>, why: &str) {
        assert!(
            refused.as_deref().is_some_and(|r| r.contains(why)),
            "{refused:?}"
        );
    }

    /// The endpoint id of the secret `[n; 32]`.
    fn endpoint(n: u8) -> String {
        hex(SigningKey::from_bytes(&[n; 32]).verifying_key().as_bytes())
    }

    /// The claim of `endpoint(n)` by `device` in chain `c1`, which its own
    /// op must carry.
    fn claim(device: &[u8; 32], n: u8) -> Value {
        let (key, sig) = bind_device(device, "c1", &endpoint(n));
        json!({"type": "claim", "endpoint": endpoint(n), "device_key": key, "device_sig": sig})
    }

    /// A member's standing admit of `endpoint(n)`.
    fn admit(n: u8) -> Value {
        json!({"type": "admit", "endpoint": endpoint(n), "label": "agent", "via": "standing"})
    }

    /// `rec` as the next of `ops`, `by`'s at `hlc` or the next second; and how
    /// the fold refuses it, if it does. The fold reads no op signature.
    fn add_at(ops: &mut Vec<Op>, by: &str, mut rec: Value, hlc: Option<&str>) -> Option<String> {
        let t = ops.len() + 1;
        rec["chain"] = "c1".into();
        let hlc = hlc.map_or_else(|| format!("2026-11-12T00:00:{t:02}.000Z/0000"), Into::into);
        let op = json!({
            "endpoint": by, "seq": t, "hlc": hlc,
            "hid": CHAIN_HID, "slug": CHAIN_HID, "kind": "chain", "effect": rec, "sig": "",
        });
        ops.push(Op::parse(&op.to_string()).unwrap());
        Chain::fold(ops.clone())
            .rejected
            .get(&(by.into(), t as i64))
            .cloned()
    }

    fn add(ops: &mut Vec<Op>, by: &str, rec: Value) -> Option<String> {
        add_at(ops, by, rec, None)
    }

    fn at(ops: &[Op], e: &str) -> super::super::Member {
        Chain::fold(ops.to_vec()).member(e).unwrap().clone()
    }

    /// A chain `c1` whose admin `a`, `[9; 32]`'s key when `locked`, holds a
    /// standing admission for `device`.
    fn chain(ops: &mut Vec<Op>, device: &[u8; 32], locked: bool) -> impl Fn(Value) -> Value {
        let signed = move |mut rec: Value| {
            rec["chain"] = "c1".into();
            if locked {
                rec["sig"] = sign_ed25519(&rec, &[9u8; 32]).into();
            }
            rec
        };
        let admins = match locked {
            true => json!([{"key": ed25519_admin_key(&[9u8; 32]), "label": "k"}]),
            false => json!([]),
        };
        let genesis = json!({"type": "genesis", "admins": admins, "label": "a"});
        assert_eq!(add(ops, &endpoint(1), signed(genesis)), None);
        let key = bind_device(device, "", "").0;
        let standing = json!({"type": "standing", "device": key, "caps": {}, "label": "agent"});
        assert_eq!(add(ops, &endpoint(1), signed(standing)), None);
        signed
    }

    /// A member admits by standing an endpoint whose own claim the chain
    /// holds (da-tkbtu), once, retiring its device's other endpoints alone,
    /// whose ops before it stand; the fold is the same in any order.
    #[test]
    fn a_standing_admit_retires_its_own_devices_endpoints_alone() {
        let [a, e1, e2] = [1, 2, 3].map(endpoint);
        let device = [7u8; 32];
        let key = bind_device(&device, "", "").0;
        let ops = &mut Vec::new();
        let _ = chain(ops, &device, false);
        let mut early = admit(2);
        early["created"] = "before its claim".into();
        refuses(add(ops, &a, early), "no standing claim of its own");
        assert_eq!(add(ops, &e1, claim(&device, 2)), None);
        assert_eq!(add(ops, &a, admit(2)), None);
        refuses(add(ops, &a, claim(&device, 3)), "its endpoint's own op");
        assert_eq!(add(ops, &a, claim(&device, 1)), None, "a claims itself");
        refuses(add(ops, &a, admit(1)), "in the chain already");
        assert_eq!(add(ops, &e2, claim(&device, 3)), None);
        assert_eq!(add(ops, &a, admit(3)), None);
        let chain = Chain::fold(ops.clone());
        let at = |e: &str| chain.member(e).unwrap();
        assert_eq!(at(&e1).revoked.as_deref(), Some(at(&e2).admitted.as_str()));
        assert!(chain.allows(&e1, "2026-11-12T00:00:05.000Z/0000").is_ok());
        assert!(chain.allows(&e1, &at(&e2).admitted).is_err());
        assert_eq!(at(&e2).device.as_deref(), Some(key.as_str()));
        assert!(at(&a).revoked.is_none());
        let shown = |c: &Chain| format!("{:?} {:?}", c.members, c.standings);
        let mut reversed = ops.clone();
        reversed.reverse();
        assert_eq!(shown(&Chain::fold(reversed)), shown(&chain));
        add(ops, &a, json!({"type": "revoke_standing", "device": key}));
        assert_eq!(add(ops, &endpoint(4), claim(&device, 4)), None);
        refuses(add(ops, &a, admit(4)), "standing admission was revoked");
    }

    /// A standing record's first admit is news, and each after it its device
    /// coming back (da-w27bp); a standing record issued again, or another
    /// device's, is news once more.
    #[test]
    fn a_standing_records_first_admit_alone_is_news() {
        let a = endpoint(1);
        let device = [7u8; 32];
        let key = bind_device(&device, "", "").0;
        let ops = &mut Vec::new();
        let _ = chain(ops, &device, false);
        for n in [2, 3, 4] {
            add(ops, &endpoint(n), claim(&device, n));
            assert_eq!(add(ops, &a, admit(n)), None);
        }
        let returned = |ops: &[Op], n: u8| at(ops, &endpoint(n)).returned;
        assert_eq!([2, 3, 4].map(|n| returned(ops, n)), [false, true, true]);

        let again = json!({"type": "standing", "device": key, "caps": {}, "label": "again"});
        assert_eq!(add(ops, &a, again), None);
        let other = [8u8; 32];
        let standing = json!({"type": "standing", "device": bind_device(&other, "", "").0,
            "caps": {}, "label": "bot"});
        assert_eq!(add(ops, &a, standing), None);
        for (d, n) in [(&device, 5), (&device, 6), (&other, 7), (&other, 8)] {
            add(ops, &endpoint(n), claim(d, n));
            assert_eq!(add(ops, &a, admit(n)), None);
        }
        let news = [5, 6, 7, 8].map(|n| returned(ops, n));
        assert_eq!(news, [false, true, false, true]);
    }

    /// A device claims no endpoint for itself: its endpoint x writing v's
    /// claim is refused, so no admit of v by standing follows and x cannot
    /// retire v (da-tkbtu, ístina's F16). And an invite's re-admission
    /// clears the device tag a standing admit set, though not its own
    /// describe's.
    #[test]
    fn a_device_claims_no_endpoint_that_did_not_claim_itself() {
        let [a, x, v, w] = [1, 2, 3, 4].map(endpoint);
        let device = [7u8; 32];
        let key = bind_device(&device, "", "").0;
        let ops = &mut Vec::new();
        let _ = chain(ops, &device, false);
        add(ops, &x, claim(&device, 2));
        assert_eq!(add(ops, &a, admit(2)), None);
        refuses(add(ops, &x, claim(&device, 3)), "its endpoint's own op");
        refuses(add(ops, &x, admit(3)), "no standing claim of its own");
        let invite = |id: &str| {
            json!({"type": "invite", "invite_id": id, "secret_hash": crate::db::sync::sha256_hex(id),
                "expires": "2099-01-01T00:00:00Z", "caps": {}})
        };
        let redeem = |e: &str, id: &str| json!({"type": "admit", "endpoint": e, "label": "v", "via": id, "secret": id});
        add(ops, &a, invite("i"));
        assert_eq!(add(ops, &a, redeem(&v, "i")), None);
        assert_eq!(at(ops, &v).device, None);

        // w is admitted by standing, retiring x; an invite re-admits it, and the tag goes.
        add(ops, &w, claim(&device, 4));
        assert_eq!(add(ops, &a, admit(4)), None);
        assert_eq!(at(ops, &w).device, Some(key.clone()));
        assert!(at(ops, &x).revoked.is_some());
        add(ops, &a, invite("j"));
        assert_eq!(add(ops, &a, redeem(&w, "j")), None);
        assert_eq!(at(ops, &w).device, None);
        add(ops, &endpoint(5), claim(&device, 5));
        assert_eq!(add(ops, &a, admit(5)), None);
        assert!(at(ops, &w).revoked.is_none(), "w is no longer the device's");

        // w's own describe binds it, and an invite's re-admission keeps that.
        let (_, sig) = bind_device(&device, "c1", &w);
        let describe = json!({"type": "describe", "endpoint": w, "label": "w",
            "device_key": key, "device_sig": sig});
        add(ops, &w, describe);
        add(ops, &a, invite("k"));
        assert_eq!(add(ops, &a, redeem(&w, "k")), None);
        assert_eq!(at(ops, &w).device, Some(key));
    }

    /// No copy of a claim or an admit undoes an admission (vremya's probe on
    /// #2030): in a locked chain a member writing a retired endpoint's claim
    /// or admit again, or a copy of a claim backdated before the original,
    /// revokes no one, as its direct revoke is refused; nor does an admit
    /// bring back a retired endpoint or one an admin revoked.
    #[test]
    fn a_claim_or_an_admit_written_again_is_refused() {
        let [_, x, v, w] = [1, 2, 3, 4].map(endpoint);
        let a = endpoint(1);
        let device = [7u8; 32];
        let ops = &mut Vec::new();
        let signed = chain(ops, &device, true);
        add(ops, &x, claim(&device, 2));
        assert_eq!(add(ops, &a, admit(2)), None, "x is admitted");
        let invite = json!({"type": "invite", "invite_id": "iv", "secret_hash": secret_hash("s"),
            "expires": "2099-01-01T00:00:00Z", "caps": {}});
        assert_eq!(add(ops, &a, signed(invite)), None);
        let redeem =
            json!({"type": "admit", "via": "iv", "secret": "s", "endpoint": v, "label": "v"});
        assert_eq!(add(ops, &a, redeem), None, "v joins by invite");
        let original = ops.len() + 1;
        assert_eq!(add(ops, &w, claim(&device, 4)), None);
        assert_eq!(add(ops, &a, admit(4)), None, "w is admitted, retiring x");
        let revoke = json!({"type": "revoke", "endpoint": w});
        assert!(add(ops, &v, revoke).is_some(), "an unsigned revoke");
        let mut replay = claim(&device, 2);
        replay["created"] = "again".into();
        refuses(add(ops, &v, replay), "its endpoint's own op");
        let mut readmit = admit(2);
        readmit["created"] = "again".into();
        refuses(add(ops, &v, readmit.clone()), "in the chain already");
        // Backdated before w's own claim, v's copy of it would be the first seen.
        let backdated = claim(&device, 4);
        let before = format!("2026-11-12T00:00:{:02}.500Z/0000", original - 1);
        refuses(
            add_at(ops, &v, backdated, Some(&before)),
            "its endpoint's own op",
        );
        add(ops, &x, claim(&device, 2));
        readmit["created"] = "by an admin".into();
        refuses(add(ops, &a, readmit), "in the chain already");
        assert!(at(ops, &w).revoked.is_none(), "w stays");
        assert!(at(ops, &x).revoked.is_some(), "x stays retired");

        let revoke = json!({"type": "revoke", "endpoint": w, "created": "the admin's"});
        assert_eq!(add(ops, &a, signed(revoke)), None, "an admin revokes w");
        let mut again = admit(4);
        again["created"] = "again".into();
        refuses(add(ops, &a, again), "in the chain already");
        assert!(at(ops, &w).revoked.is_some(), "w stays revoked");
    }

    /// A joiner dates its own claim, and not its admission (vremya's, on
    /// #2030): a claim dated before the standing's revoke or expiry is
    /// admitted by no member's admit after it, and the claim taken before
    /// the revoke stands.
    #[test]
    fn a_claim_dated_before_its_standings_end_admits_no_one() {
        let a = endpoint(1);
        let device = [7u8; 32];
        let key = bind_device(&device, "", "").0;
        let ops = &mut Vec::new();
        let _ = chain(ops, &device, false);
        add(ops, &endpoint(2), claim(&device, 2));
        assert_eq!(add(ops, &a, admit(2)), None);
        add(ops, &a, json!({"type": "revoke_standing", "device": key}));
        let dated = Some("2026-11-12T00:00:03.500Z/0000");
        assert_eq!(add_at(ops, &endpoint(6), claim(&device, 6), dated), None);
        refuses(add(ops, &a, admit(6)), "standing admission was revoked");
        assert!(at(ops, &endpoint(2)).revoked.is_none(), "x stands");

        let device = [8u8; 32];
        let key = bind_device(&device, "", "").0;
        let standing = json!({"type": "standing", "device": key, "caps": {}, "label": "agent2",
            "expires": "2026-11-12T00:00:09Z"});
        add(ops, &a, standing);
        let dated = Some("2026-11-12T00:00:08.500Z/0000");
        assert_eq!(add_at(ops, &endpoint(9), claim(&device, 9), dated), None);
        refuses(add(ops, &a, admit(9)), "standing admission expired");
    }

    /// A member-level agent, admitted by its standing, adds its own key as
    /// the first admin of an open chain and locks it (ístina's F19), which is
    /// why `sync chain agent` makes none there; in a locked chain its
    /// `add_admin`, unsigned or signed by its own key, is refused.
    #[test]
    fn a_member_agent_adds_an_admin_only_to_an_open_chain() {
        let agent = endpoint(2);
        let own =
            json!({"type": "add_admin", "key": ed25519_admin_key(&[5u8; 32]), "label": "mine"});
        let mut signed = own.clone();
        signed["chain"] = "c1".into();
        signed["sig"] = sign_ed25519(&signed, &[5u8; 32]).into();
        for locked in [false, true] {
            let device = [7u8; 32];
            let ops = &mut Vec::new();
            let _ = chain(ops, &device, locked);
            add(ops, &agent, claim(&device, 2));
            assert_eq!(add(ops, &endpoint(1), admit(2)), None);
            assert_eq!(at(ops, &agent).role_at("9999"), Role::Member);
            match locked {
                false => assert_eq!(
                    add(ops, &agent, signed.clone()),
                    None,
                    "open: it locks the chain"
                ),
                true => {
                    assert!(
                        add(ops, &agent, own.clone()).is_some(),
                        "an unsigned add_admin"
                    );
                    assert!(
                        add(ops, &agent, signed.clone()).is_some(),
                        "signed by its own key"
                    );
                    assert_eq!(Chain::fold(ops.clone()).admins.len(), 1);
                }
            }
        }
    }
}
