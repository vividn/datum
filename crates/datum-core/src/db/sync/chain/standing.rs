//! The standing admission (da-zkwiu): a device's code, which admits each
//! fresh copy of it by its device key, replacing the copies before.

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::{bind_device, bound_device, expired, hlc_time, role_of, Chain, Filter, Role};
use crate::db::sync::{unhex, Op};
use crate::db::Db;
use crate::error::{DatumError, Result};

/// What a dialer carries where an invite's secret goes to be admitted by
/// its device's standing admission (da-zkwiu): `standing:<key>:<sig>`.
pub const STANDING: &str = "standing:";

/// [`STANDING`]'s proof that `endpoint` in `chain` is the device's whose
/// secret is `device`.
pub fn standing_proof(device: &[u8; 32], chain: &str, endpoint: &str) -> String {
    let (key, sig) = bind_device(device, chain, endpoint);
    format!("{STANDING}{key}:{sig}")
}

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
        let revoked = None;
        let st = Standing {
            label,
            role,
            filter,
            expires,
            hlc,
            revoked,
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

    /// An admit by its device's standing admission (da-zkwiu): the device
    /// key signs for the endpoint, and every endpoint it `replaces` is the
    /// device's, revoked from the admit on.
    pub(super) fn admit_standing(&mut self, op: &Op) -> std::result::Result<(), String> {
        let r = &op.effect;
        let device = bound_device(r).ok_or("not signed for by its device's key")?;
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
        let endpoint = r["endpoint"].as_str().unwrap_or_default();
        let replaces: Vec<&str> = match &r["replaces"] {
            Value::Null => vec![],
            Value::Array(a) if a.iter().all(Value::is_string) => {
                a.iter().filter_map(Value::as_str).collect()
            }
            _ => return Err("an admit's `replaces` is a list of endpoint ids".into()),
        };
        for e in &replaces {
            let m = self.member(e).filter(|_| *e != endpoint);
            if m.is_none_or(|m| m.device.as_deref() != Some(device.as_str())) {
                return Err(format!(
                    "replaces {e}, which is no other endpoint of its device"
                ));
            }
        }
        // A key proves nothing about an endpoint it did not make: one the
        // chain holds as another device's, or as no device's, stays theirs.
        if self
            .member(endpoint)
            .is_some_and(|m| m.device.as_deref() != Some(device.as_str()))
        {
            return Err(format!("{endpoint} is a member that is not its device's"));
        }
        // As `invite <id>` names its invite: the device's key is `device`.
        let via = format!("standing {}", &device[..8]);
        let label = r["label"].as_str().unwrap_or_default();
        self.admit(endpoint, label, &op.hlc, &via, role, filter)?;
        for m in self.members.iter_mut() {
            if m.endpoint == endpoint {
                m.device = Some(device.clone());
            } else if replaces.contains(&m.endpoint.as_str()) && m.revoked.is_none() {
                m.revoked = Some(op.hlc.clone());
                m.last_act = op.hlc.clone();
            }
        }
        Ok(())
    }
}

impl Db {
    /// Admit `endpoint` by the standing admission of the device whose key
    /// `device_sig` signed for it (da-zkwiu), replacing the device's live
    /// endpoints in the chain: write its `admit`, or say why the chain
    /// refuses it.
    pub fn sync_admit_standing(
        &self,
        endpoint: &str,
        label: &str,
        device_key: &str,
        device_sig: &str,
        holds: u64,
    ) -> Result<()> {
        let chain = self.sync_chain()?;
        let st = chain.standings.get(device_key).ok_or_else(|| {
            DatumError::Other("no standing admission for this device in the sync chain".into())
        })?;
        reader_holds(st.role, label, holds)?;
        let replaces: Vec<&str> = chain
            .members
            .iter()
            .filter(|m| m.endpoint != endpoint && m.revoked.is_none())
            .filter(|m| m.device.as_deref() == Some(device_key))
            .map(|m| m.endpoint.as_str())
            .collect();
        let record = json!({
            "type": "admit", "chain": chain.id, "endpoint": endpoint, "label": label,
            "via": "standing", "device_key": device_key, "device_sig": device_sig,
            "replaces": replaces, "created": Utc::now().to_rfc3339(),
        });
        self.sync_chain_write(record, false)
    }
}

#[cfg(test)]
mod tests {
    use super::super::CHAIN_HID;
    use super::*;

    /// `refused` is a refusal that says `why`.
    fn refuses(refused: Option<String>, why: &str) {
        assert!(
            refused.as_deref().is_some_and(|r| r.contains(why)),
            "{refused:?}"
        );
    }

    /// A standing admit (da-zkwiu) replaces only its own device's endpoints,
    /// whose ops before it stand, and the fold is the same in any order.
    #[test]
    fn a_standing_admit_replaces_its_own_devices_endpoints_alone() {
        let [a, e1, e2, e3] = ["a1", "e1", "e2", "e3"].map(|h| h.repeat(32));
        let device = [7u8; 32];
        let key = bind_device(&device, "c1", &e1).0;
        let mut ops = Vec::new();
        let mut add = |rec: Value| {
            let t = ops.len() + 1;
            let mut rec = rec;
            rec["chain"] = "c1".into();
            let op = json!({
                "endpoint": a, "seq": t, "hlc": format!("2026-11-12T00:00:{t:02}.000Z/0000"),
                "hid": CHAIN_HID, "slug": CHAIN_HID, "kind": "chain", "effect": rec, "sig": "",
            });
            ops.push(Op::parse(&op.to_string()).unwrap());
            let chain = Chain::fold(ops.clone());
            (
                chain.rejected.get(&(a.clone(), t as i64)).cloned(),
                ops.clone(),
            )
        };
        let admit = |e: &str, replaces: &[&str]| {
            let (key, sig) = bind_device(&device, "c1", e);
            json!({"type": "admit", "endpoint": e, "label": "agent", "via": "standing",
                "device_key": key, "device_sig": sig, "replaces": replaces})
        };
        add(json!({"type": "genesis", "admins": [], "label": "a"}));
        add(
            json!({"type": "standing", "device": key, "caps": {"role": "writer"}, "label": "agent"}),
        );
        assert_eq!(add(admit(&e1, &[])).0, None);
        refuses(add(admit(&e2, &[&a])).0, "no other endpoint of its device");
        refuses(add(admit(&e2, &[&e2])).0, "no other endpoint of its device");
        refuses(add(admit(&a, &[])).0, "not its device's");
        let (none, held) = add(admit(&e2, &[&e1]));
        assert_eq!(none, None);
        let chain = Chain::fold(held.clone());
        let at = |e: &str| chain.member(e).unwrap();
        assert_eq!(at(&e1).revoked.as_deref(), Some(at(&e2).admitted.as_str()));
        assert!(chain.allows(&e1, "2026-11-12T00:00:04.000Z/0000").is_ok());
        assert!(chain.allows(&e1, &at(&e2).admitted).is_err());
        assert_eq!(
            (at(&e2).role(), at(&e2).device.as_deref()),
            (Role::Writer, Some(key.as_str()))
        );
        assert!(at(&a).revoked.is_none());
        let shown = |c: &Chain| format!("{:?} {:?}", c.members, c.standings);
        let mut reversed = held.clone();
        reversed.reverse();
        assert_eq!(shown(&Chain::fold(reversed)), shown(&chain));
        add(json!({"type": "revoke_standing", "device": key}));
        refuses(add(admit(&e3, &[&e2])).0, "standing admission was revoked");
    }
}
