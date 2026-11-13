//! An invite bound to its redeemer (da-3bkpl). An account's join invite
//! (`sync account --invite`, `sync login`) names the endpoint that minted
//! it, inside what the account key signs: its secret goes to every member
//! the joiner dials, a reader among them where a stale record lists one, so
//! it admits that endpoint alone. An ordinary invite names none: a bearer
//! link, as designed. A build before it admits by the secret alone, and
//! every copy's fold refuses that admit once it runs this one.

/// Why an invite naming `redeemer` admits no `endpoint`, if it is another.
pub(super) fn admits_other(redeemer: Option<&str>, endpoint: &str) -> Option<&'static str> {
    let other = redeemer.is_some_and(|r| r != endpoint);
    other.then_some("the invite admits another endpoint")
}

#[cfg(test)]
mod tests {
    use super::super::{sha256_hex, Chain, Op, CHAIN_HID};
    use serde_json::{json, Value};

    /// An invite naming its redeemer admits that endpoint alone: another's
    /// admit, which a member before it still writes, is refused and uses
    /// nothing, so the redeemer is admitted after it.
    #[test]
    fn an_invite_naming_its_redeemer_admits_no_other() {
        let [a, x, j] = ["a1", "c3", "d4"].map(|h| h.repeat(32));
        let mut ops = Vec::new();
        let mut add = |mut rec: Value| {
            let t = ops.len() + 1;
            rec["chain"] = "c1".into();
            let op = json!({
                "endpoint": a, "seq": t, "hlc": format!("2026-09-29T00:00:{t:02}.000Z/0000"),
                "hid": CHAIN_HID, "slug": CHAIN_HID, "kind": "chain", "effect": rec, "sig": "",
            });
            ops.push(Op::parse(&op.to_string()).unwrap());
            let chain = Chain::fold(ops.clone());
            let why = chain.rejected.get(&(a.clone(), t as i64)).cloned();
            (chain, why)
        };
        add(json!({"type": "genesis", "admins": [], "label": "a"}));
        add(
            json!({"type": "invite", "invite_id": "i", "secret_hash": sha256_hex("s"),
            "expires": "2099-01-01T00:00:00Z", "caps": {}, "redeemer": j}),
        );
        let admit = |who: &str| json!({"type": "admit", "endpoint": who, "label": "", "via": "i", "secret": "s"});
        let (chain, why) = add(admit(&x));
        assert_eq!(why.as_deref(), Some("the invite admits another endpoint"));
        assert!(chain.member(&x).is_none());
        let (chain, why) = add(admit(&j));
        assert_eq!(why, None);
        assert!(chain.member(&j).is_some());
    }
}
