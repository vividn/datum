//! A member's role and revoke, judged by step (da-l0as8, F18): each `role`
//! record, a revoke and an admission, and which of them a member's op is judged by.

use serde_json::Value;

use super::{Filter, Member, Role};

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

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::super::tests::fold;
    use super::super::{ed25519_admin_key, secret_hash, READER};
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
}
