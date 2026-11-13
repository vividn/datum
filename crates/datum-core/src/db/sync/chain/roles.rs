//! A member's role and revoke, judged by step (da-l0as8, F18): each `role`
//! record, a revoke and an admission, and which of them a member's op is judged by.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{Chain, Filter, Member, Op, Role, READER, WRITER};

/// A member's role and filter from a record on (da-l0as8): from its HLC, or,
/// where the record carries the `seq` of the member's ops it stands to, from
/// its op past that seq, whatever the op's HLC. The op's signer chooses its
/// HLC, and so could date it before a demotion; its seqs are gapless and
/// never reused. A record from before seqs, and an admission, go by the HLC.
#[derive(Debug, Clone)]
pub struct Step {
    pub hlc: String,
    pub seq: Option<i64>,
    pub role: Role,
    pub filter: Option<Filter>,
}

impl Step {
    /// Whether its member's op at `hlc` and `seq` is judged by it or a later step.
    fn reaches(&self, hlc: &str, seq: i64) -> bool {
        self.seq.map_or(hlc >= self.hlc.as_str(), |cut| seq > cut)
    }
}

/// The `seq` a record stands to, if it carries one.
pub(super) fn cut(r: &Value) -> std::result::Result<Option<i64>, String> {
    r.get("seq")
        .map(|v| {
            v.as_i64()
                .ok_or("a record's seq is a count of its member's ops")
        })
        .transpose()
        .map_err(Into::into)
}

impl Member {
    /// Its role now.
    pub fn role(&self) -> Role {
        self.roles.last().map_or(Role::Member, |r| r.role)
    }

    /// The step its op at `hlc` and `seq` is judged by: the latest it
    /// reaches ([`Step`]). An op made before its admission is judged by the
    /// role it was admitted with, so admitting a member vouches for what it
    /// brings, and admitting a reader vouches for nothing.
    pub(in crate::db::sync) fn step_at(&self, hlc: &str, seq: i64) -> Option<&Step> {
        let at = self.roles.iter().rev().find(|s| s.reaches(hlc, seq));
        at.or(self.roles.first())
    }

    /// Its role for its op at `hlc` and `seq`.
    pub(in crate::db::sync) fn role_at(&self, hlc: &str, seq: i64) -> Role {
        self.step_at(hlc, seq).map_or(Role::Member, |s| s.role)
    }

    /// Whether its revoke reaches its op at `hlc` and `seq`.
    pub(super) fn revoked_at(&self, hlc: &str, seq: i64) -> bool {
        self.revoked
            .as_deref()
            .is_some_and(|r| match self.revoked_seq {
                Some(cut) => seq > cut,
                None => hlc >= r,
            })
    }

    /// Revoked at `hlc`, standing to `seq`, unless it is already.
    pub(super) fn revoke(&mut self, hlc: &str, seq: Option<i64>) {
        if self.revoked.is_none() {
            (self.revoked, self.revoked_seq) = (Some(hlc.into()), seq);
        }
    }

    /// Why an unpurge of it would be refused, if it would: it is not purged,
    /// or an act about it came after the purge.
    pub(super) fn unpurge_refused(&self) -> Option<&'static str> {
        match &self.purged {
            None => Some("not purged"),
            Some(p) if *p != self.last_act => Some("an act about it came after its purge"),
            Some(_) => None,
        }
    }

    /// Where some of its ops are refused, if any are: its ops at or after the
    /// HLC, or past the seq, of its revoke and of each step that made it a
    /// reader or a bounded writer (da-mogvf; all, admitted as one); a purge
    /// refuses all of them, whenever made. The two are kept apart, each the
    /// least of its kind, and an op past either is read again: where no cut
    /// is of a kind, `"~"`, after every HLC, or `i64::MAX` reaches none.
    pub(in crate::db::sync) fn refused_from(&self) -> Option<(&str, i64)> {
        let unwriting = self
            .roles
            .iter()
            .enumerate()
            .filter(|(_, s)| s.role != Role::Member);
        let unwriting = unwriting.map(|(i, s)| match i {
            0 => ("", None),
            _ => (s.hlc.as_str(), s.seq),
        });
        let revoked = self.revoked.as_deref().map(|r| (r, self.revoked_seq));
        let purged = self.purged.as_ref().map(|_| ("", None));
        let from = unwriting.chain(revoked).chain(purged);
        from.fold(None, |from, (hlc, seq)| {
            let (h, s) = from.unwrap_or(("~", i64::MAX));
            Some(match seq {
                Some(seq) => (h, s.min(seq)),
                None => (h.min(hlc), s),
            })
        })
    }
}

impl Chain {
    /// The fold of `ops`, sorted, with each record judged by its writer's
    /// standing in the whole fold as well as in the fold so far (da-va3vp).
    /// A record is stepped in HLC order, which its writer chooses, so one
    /// dated before its writer's demotion or revoke is stepped before that
    /// exists. So each pass refuses every record the pass before took whose
    /// writer that pass's whole fold refuses: a reader, a writer or an
    /// endpoint revoked at it, by their seq cuts. A pass refusing more can
    /// take a record the one before refused (a second admit by one invite),
    /// so the passes go on until one refuses nothing more; what a pass
    /// refused stays refused, so they end. A `role` or `revoke` is judged
    /// as before: two that cut each other's writer need a tie rule
    /// (da-ag3ir), and the first pass's outcome is today's.
    pub(super) fn judged(ops: &[Op]) -> Chain {
        let key = |op: &Op| (op.endpoint.clone(), op.seq);
        let mut late = BTreeMap::new();
        loop {
            let chain = Chain {
                late,
                ..Chain::default()
            }
            .fold_once(ops);
            let more: Vec<_> = ops
                .iter()
                .filter(|op| !chain.rejected.contains_key(&key(op)))
                .filter_map(|op| Some((key(op), chain.refuses_late(op)?)))
                .collect();
            if more.is_empty() {
                return chain;
            }
            late = chain.late;
            late.extend(more);
        }
    }

    /// Why this fold refuses the writer of `op`, at its HLC and seq: as
    /// [`Chain::acts`] does, or as [`Chain::live`] does an endpoint's own
    /// describe, but reading a purge only as the revoke it is, so
    /// what a purged endpoint signed before it stands, as it does today.
    fn refuses_late(&self, op: &Op) -> Option<String> {
        let r = &op.effect;
        let kind = r["type"].as_str().unwrap_or_default();
        // A leave revokes its own writer at its own HLC, and takes no one
        // but its writer out.
        if matches!(kind, "genesis" | "claim" | "role" | "revoke" | "leave") {
            return None;
        }
        let m = self.member(&op.endpoint)?;
        let own = kind == "describe" && r["endpoint"] == op.endpoint.as_str();
        let why = match m.role_at(&op.hlc, op.seq) {
            _ if m.revoked_at(&op.hlc, op.seq) => self.live(&op.endpoint, &op.hlc, op.seq).err()?,
            _ if own => return None,
            Role::Reader => READER.into(),
            Role::Writer => WRITER.into(),
            Role::Member => return None,
        };
        Some(why)
    }

    /// Why the whole fold refuses `op`'s writer, if a pass before found it does.
    pub(super) fn late_for(&self, op: &Op) -> std::result::Result<(), String> {
        if self.late.is_empty() {
            return Ok(());
        }
        let why = self.late.get(&(op.endpoint.clone(), op.seq));
        why.map_or(Ok(()), |why| Err(why.clone()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::super::tests::fold;
    use super::super::{
        ed25519_admin_key, secret_hash, sign_ed25519, Chain, Op, CHAIN_HID, READER,
    };
    use super::Role;

    /// F18 (ístina, 11-12; da-l0as8): a demoted member signs its next ops
    /// dated before its demotion. A `role` record carrying the seq of its
    /// ops it stands to judges every op past that seq by the new role,
    /// whatever its HLC, and every op at or below it by the old; one with no
    /// seq, signed before da-l0as8, keeps judging by the HLC.
    #[test]
    fn a_demotion_is_not_escaped_by_backdating() {
        let secret = [3u8; 32];
        let key = ed25519_admin_key(&secret);
        let c = |r: Value| {
            let mut r = r;
            r["chain"] = "c1".into();
            r
        };
        let v = "cd".repeat(32);
        let at = |t: u32| format!("2026-09-29T00:00:{t:02}.000Z/0000");
        for demoted_to in ["reader", "writer"] {
            let demoted = Role::parse(demoted_to).unwrap();
            for seq in [Some(7), None] {
                let mut role = json!({"type": "role", "endpoint": v, "role": demoted_to});
                if let Some(seq) = seq {
                    role["seq"] = seq.into();
                }
                let (chain, done) = fold(&[
                    (
                        1,
                        c(
                            json!({"type": "genesis", "admins": [{"key": key, "label": "k"}], "label": "a"}),
                        ),
                        Some(&secret),
                    ),
                    (
                        2,
                        c(
                            json!({"type": "invite", "invite_id": "iv", "secret_hash": secret_hash("s"),
                        "expires": "2099-01-01T00:00:00Z", "caps": {}}),
                        ),
                        Some(&secret),
                    ),
                    (
                        3,
                        c(
                            json!({"type": "admit", "via": "iv", "secret": "s", "endpoint": v, "label": "v"}),
                        ),
                        None,
                    ),
                    (10, c(role), Some(&secret)),
                ]);
                assert!(done.iter().all(|d| d.is_ok()), "{done:?}");
                let m = chain.member(&v).unwrap();
                let (backdated, honest) = (m.role_at(&at(5), 8), m.role_at(&at(5), 7));
                match seq {
                    Some(_) => assert_eq!(
                        backdated, demoted,
                        "past the cut, dated before: {demoted_to}"
                    ),
                    None => assert_eq!(
                        backdated,
                        Role::Member,
                        "no seq: judged by the HLC, as before"
                    ),
                }
                assert_eq!(
                    honest,
                    Role::Member,
                    "at the cut: the role before ({demoted_to})"
                );
                assert_eq!(m.role_at(&at(15), 8), demoted);
                if demoted == Role::Reader && seq.is_some() {
                    assert_eq!(chain.allows(&v, &at(5), 8), Err(READER.into()));
                    assert!(chain.allows(&v, &at(5), 7).is_ok());
                }
            }
        }
    }

    /// F15's version (da-l0as8): a revoke carrying the seq it stands to
    /// refuses the endpoint's ops past it, dated before it or not, and
    /// keeps those at or below it; a re-admission takes the revoke's cut
    /// as where its role begins.
    #[test]
    fn a_revoke_is_not_escaped_by_backdating() {
        let secret = [3u8; 32];
        let key = ed25519_admin_key(&secret);
        let c = |r: Value| {
            let mut r = r;
            r["chain"] = "c1".into();
            r
        };
        let v = "cd".repeat(32);
        let at = |t: u32| format!("2026-09-29T00:00:{t:02}.000Z/0000");
        let invite = |id: &str, caps: Value| {
            c(
                json!({"type": "invite", "invite_id": id, "secret_hash": secret_hash("s"),
                "expires": "2099-01-01T00:00:00Z", "caps": caps}),
            )
        };
        let admit = |id: &str| {
            c(json!({"type": "admit", "via": id, "secret": "s", "endpoint": v, "label": "v"}))
        };
        let mut records = vec![
            (
                1,
                c(json!({"type": "genesis", "admins": [{"key": key, "label": "k"}], "label": "a"})),
                Some(&secret),
            ),
            (2, invite("iv", json!({})), Some(&secret)),
            (3, admit("iv"), None),
            (
                10,
                c(json!({"type": "revoke", "endpoint": v, "seq": 7})),
                Some(&secret),
            ),
        ];
        let (chain, done) = fold(&records);
        assert!(done.iter().all(|d| d.is_ok()), "{done:?}");
        assert!(
            chain.allows(&v, &at(5), 7).is_ok(),
            "at the cut: made before the revoke"
        );
        let refused = chain.allows(&v, &at(5), 8).unwrap_err();
        assert!(refused.contains("was expelled"), "{refused}");
        assert!(chain.member(&v).unwrap().refused_from() == Some(("~", 7)));
        // Re-admitted as a reader: what it made past the cut is a reader's.
        records.push((12, invite("i2", json!({"role": "reader"})), Some(&secret)));
        records.push((13, admit("i2"), None));
        let (chain, done) = fold(&records);
        assert!(done.iter().all(|d| d.is_ok()), "{done:?}");
        let m = chain.member(&v).unwrap();
        assert_eq!(
            (m.role_at(&at(5), 7), m.role_at(&at(5), 8)),
            (Role::Member, Role::Reader)
        );
        assert!(chain.allows(&v, &at(20), 9).is_err());
    }

    /// da-va3vp (ístina's measurement, 11-13): `a` admits `m` at :03 and
    /// demotes it at :10, standing to `m`'s seq 0. `m`'s chain records
    /// dated before :10 are stepped before the demotion, and the whole fold
    /// refuses them: in a locked chain a message (an `upgrade`, a `grant`),
    /// in an open one an admin act (an `invite`). A demotion without a seq
    /// judges by the HLC, as before; a record dated after it is refused as
    /// it always was; and a purge refuses no record made before it. Every
    /// fold gives the same in reverse arrival order.
    #[test]
    fn a_demoted_members_backdated_record_is_refused() {
        const K: [u8; 32] = [7; 32];
        let [a, m] = ["a1", "b2"].map(|h| h.repeat(32));
        let op = |by: &str, seq: i64, t: u32, mut rec: Value, signed: bool| {
            rec["chain"] = "c1".into();
            if signed {
                rec["sig"] = sign_ed25519(&rec, &K).into();
            }
            let op = json!({
                "endpoint": by, "seq": seq, "hlc": format!("2026-09-29T00:00:{t:02}.000Z/0000"),
                "hid": CHAIN_HID, "slug": CHAIN_HID, "kind": "chain", "effect": rec, "sig": "",
            });
            Op::parse(&op.to_string()).unwrap()
        };
        let fold = |ops: &[Op]| {
            let chain = Chain::fold(ops.to_vec());
            let back = Chain::fold(ops.iter().rev().cloned().collect());
            assert_eq!(
                back.rejected, chain.rejected,
                "the fold depends on arrival order"
            );
            chain
        };
        // (locked, the demotion's seq, or how m is removed at :10)
        for (locked, demotion) in [
            (
                true,
                json!({"type": "role", "endpoint": m, "role": "reader", "seq": 0}),
            ),
            (
                true,
                json!({"type": "role", "endpoint": m, "role": "reader"}),
            ),
            (
                false,
                json!({"type": "role", "endpoint": m, "role": "reader", "seq": 0}),
            ),
            (false, json!({"type": "purge", "endpoint": m})),
        ] {
            let admins = match locked {
                true => json!([{"key": ed25519_admin_key(&K), "label": "admin"}]),
                false => json!([]),
            };
            let invite = |id: &str| {
                json!({"type": "invite", "invite_id": id, "secret_hash": secret_hash("s"),
                    "expires": "2099-01-01T00:00:00Z", "caps": {}})
            };
            let cut = demotion.get("seq").is_some();
            let purge = demotion["type"] == "purge";
            let mut ops = vec![
                op(
                    &a,
                    1,
                    1,
                    json!({"type": "genesis", "admins": admins, "label": "a"}),
                    locked,
                ),
                op(&a, 2, 2, invite("i"), locked),
                op(
                    &a,
                    3,
                    3,
                    json!({"type": "admit", "endpoint": m, "label": "m", "via": "i", "secret": "s"}),
                    false,
                ),
                op(&a, 4, 10, demotion, locked),
                op(
                    &m,
                    1,
                    5,
                    json!({"type": "upgrade", "datum": "3.9.0"}),
                    false,
                ),
                op(
                    &m,
                    2,
                    6,
                    json!({"type": "grant", "to": "11".repeat(32), "eph": "22".repeat(32), "sealed": "x"}),
                    false,
                ),
                op(
                    &m,
                    3,
                    12,
                    json!({"type": "upgrade", "datum": "3.9.1"}),
                    false,
                ),
            ];
            // kimi-k3's case: two admits by one invite, the second refused
            // only while the first is taken.
            let admit = |e: &str| json!({"type": "admit", "endpoint": e.repeat(32), "label": e, "via": "x", "secret": "s"});
            // An admit takes no admin, in a locked chain too.
            ops.push(op(&a, 5, 4, invite("x"), locked));
            ops.push(op(&m, 5, 8, admit("e1"), false));
            ops.push(op(&m, 6, 9, admit("e2"), false));
            if !locked {
                ops.push(op(&m, 4, 7, invite("j"), false));
            }
            let chain = fold(&ops);
            let what = format!("locked {locked}, cut {cut}, purge {purge}");
            let refused = |seq: i64| chain.rejected.get(&(m.clone(), seq));
            assert!(
                !chain.rejected.contains_key(&(a.clone(), 4)),
                "{what}: the demotion stands"
            );
            assert!(refused(3).is_some(), "{what}: control, dated after it");
            let used = "invite already used".to_string();
            match cut {
                true => assert_eq!(
                    refused(6),
                    Some(&format!("its writer is {READER}")),
                    "{what}"
                ),
                false => assert_eq!(refused(6), Some(&used), "{what}"),
            }
            let backdated = [1, 2, 5].into_iter().chain((!locked).then_some(4));
            for seq in backdated {
                match cut {
                    true => assert_eq!(
                        refused(seq),
                        Some(&format!("its writer is {READER}")),
                        "{what}: {seq}"
                    ),
                    false => assert_eq!(refused(seq), None, "{what}: {seq}, judged by the HLC"),
                }
            }
            assert_eq!(
                chain.upgrade.as_ref().map(|u| u.by == m),
                (!cut).then_some(true),
                "{what}"
            );
            assert_eq!(chain.grants.iter().any(|g| g.by == m), !cut, "{what}");
            assert_eq!(chain.holds(&ops[4].effect), !cut, "{what}");
        }
    }
}
