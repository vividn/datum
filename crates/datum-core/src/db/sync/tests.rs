use super::*;

/// The code a row and its `conflicts` are folded by, against what it was
/// at [`FOLD_RULE`] (da-sqpjt). A change to it fails here: if the same
/// ops now fold to another row, bump `FOLD_RULE`, so every copy refolds
/// as it opens; then record the hash beside the rule either way.
///
/// The code is the items `ITEMS` names, each from its line to the first
/// line that is `;` ended (a const) or a `}` in the first column, which
/// rustfmt writes only where a top-level item ends. A line that is a
/// comment is left out; a comment after code on its line is not. What
/// it does not name it does not see: a helper a listed item comes to
/// call joins `ITEMS`, and a renamed item is renamed there.
#[test]
fn the_fold_rule_is_fingerprinted() {
    const RECORDED: (u32, &str) = (
        3,
        "14354a4eff02e6bd173257b165f9e83484c5002253dacd6f0e14bca7277c3ae1",
    );
    const ITEMS: &[&str] = &[
        "fn fold_into",
        "fn values",
        "const NOT_A_CONFLICT",
        "impl Op",
        "fn fold",
        "fn entry_from",
        "const PROVENANCE",
        "fn contestable",
        "fn settles",
        "fn intents",
        "fn automatic",
        "fn contention",
        "fn conflicts_of",
        "fn about",
        "fn side",
        "fn refresh_conflicts",
        "fn content",
        "fn own",
        "const TAKEN_BACK",
        "fn merged_into",
        "fn merge_rows",
        "fn merge_from",
        "fn derived",
        "fn rederive",
        "fn write_merged",
        "fn met",
        "fn refold",
    ];
    let source = include_str!("../sync.rs");
    let mut code = String::new();
    for item in ITEMS {
        let start = source
            .lines()
            .position(|l| {
                let l = l
                    .trim_start_matches("pub(super) ")
                    .trim_start_matches("pub(crate) ");
                l.strip_prefix(item)
                    .is_some_and(|rest| rest.starts_with([' ', '(', ':', '<']))
            })
            .unwrap_or_else(|| {
                panic!("`{item}` is not in sync.rs: name what replaced it in ITEMS")
            });
        let end = if item.starts_with("const") { ";" } else { "}" };
        for line in source.lines().skip(start) {
            if !line.trim_start().starts_with("//") {
                code.push_str(line);
                code.push('\n');
            }
            if line == end || end == ";" && line.ends_with(';') {
                break;
            }
        }
    }
    let now = (FOLD_RULE, sha256_hex(&code));
    assert_eq!(
        (now.0, now.1.as_str()),
        RECORDED,
        "the fold's code changed: if the same ops fold to another row now, bump FOLD_RULE; \
         then record {now:?} as RECORDED"
    );
}

/// An op signed before v48 folds as today's (da-lfsei): its effect's
/// `device_id` reads as `client_id`, and its body, which is signed,
/// is kept as it came.
#[test]
fn an_op_from_before_v48_folds_its_device_id_as_the_client_id() {
    let old = |effect: Value| {
        json!({"endpoint": "e", "seq": 1, "hlc": "h", "hid": "x", "slug": "s",
               "kind": "entry", "effect": effect, "sig": "g"})
        .to_string()
    };
    let create = Op::parse(&old(json!({"create": {"device_id": "d1"}}))).unwrap();
    let set = Op::parse(&old(json!({"set": {"device_id": "d2"}}))).unwrap();
    assert!(create.body.contains("device_id"));
    let mut row = None;
    fold_into(&mut row, &create.effect);
    assert_eq!(row.as_ref().unwrap()["client_id"], "d1");
    fold_into(&mut row, &set.effect);
    assert_eq!(
        row,
        Some(json!({"client_id": "d2"}).as_object().unwrap().clone())
    );
}

/// A rekey a session's copy is refused leaves its key (da-otz0d).
#[test]
fn a_rekey_refused_a_sessions_copy_changes_nothing() {
    let db = Db::open_in_memory().unwrap();
    let id = db.sync_endpoint_id().unwrap();
    db.sync_session_as(Some(&id));
    assert!(db.sync_rekey().is_err());
    assert_eq!(db.sync_endpoint_id().unwrap(), id);
    db.sync_session_as(None);
    assert_ne!(db.sync_rekey().unwrap(), id);
}

#[test]
fn canonical_json_sorts_keys_at_every_depth() {
    let v = json!({"b": 1, "a": {"d": [{"f": 1, "e": 2}], "c": null}});
    assert_eq!(
        canonical_json(&v),
        r#"{"a":{"c":null,"d":[{"e":2,"f":1}]},"b":1}"#
    );
}

/// Its scalars are written as `Value` writes them, the bytes every op
/// so far was signed over (da-f4vrr wrote it without the sorted copy).
#[test]
fn canonical_json_writes_scalars_as_value_does() {
    let v: Value = serde_json::from_str(
        r#"{"z":[1.5,-0.0,1e300,18446744073709551615,-9223372036854775808,
            "é\"\\\n\u0001😀",true,{},[]],"a":"ü"}"#,
    )
    .unwrap();
    assert_eq!(canonical_json(&v), v.to_string());
}

#[test]
fn hlc_orders_as_a_string_and_round_trips() {
    let a = format_hlc(1_790_000_000_123, 3);
    assert_eq!(a, "2026-09-21T14:13:20.123Z/0003");
    assert_eq!(parse_hlc(&a), Some((1_790_000_000_123, 3)));
    assert!(format_hlc(1_790_000_000_123, 10) > a);
    assert!(format_hlc(1_790_000_000_124, 0) > format_hlc(1_790_000_000_123, 9999));
    assert_eq!(parse_hlc("2026-09-21T14:13:20.123Z/3"), None);
}

#[test]
fn hlc_never_runs_behind_and_a_full_counter_moves_the_wall() {
    assert_eq!(advance(None, 50), (50, 0));
    assert_eq!(advance(Some((40, 7)), 50), (50, 0));
    assert_eq!(
        advance(Some((60, 7)), 50),
        (60, 8),
        "a clock behind the last op"
    );
    let full = advance(Some((60, HLC_COUNTER_MAX)), 50);
    assert_eq!(full, (61, 0));
    assert!(format_hlc(full.0, full.1) > format_hlc(60, HLC_COUNTER_MAX));
}

// ─── A fast clock (da-ns8dq) ────────────────────────────────────────────

/// Past a year by a margin: at exactly 365 days the wait read «365 days»
/// only because a millisecond passed, and «1 year» when none did.
const YEAR: i64 = 400 * 24 * 3600 * 1000;

/// Log an entry on `db` with its clock `skew` ms fast; its ops as lines.
fn log_at(db: &Db, skew: i64, field: &str) -> Vec<String> {
    SKEW.with(|s| s.set(skew));
    let e = Entry::new(
        field,
        crate::entry::Verb::Point,
        None,
        None,
        None,
        None,
        "t",
    );
    db.insert_entry(&e).unwrap();
    SKEW.with(|s| s.set(0));
    db.sync_ops_since(&BTreeMap::new(), false, None).unwrap()
}

fn apply_at(db: &Db, skew: i64, lines: &[String]) -> ApplySummary {
    SKEW.with(|s| s.set(skew));
    let s = db.sync_apply(&lines.join("\n")).unwrap();
    SKEW.with(|c| c.set(0));
    s
}

/// The HLC of the latest op `db` signed itself, as ms.
fn own_latest(db: &Db) -> i64 {
    let (mine, _) = own_head(&db.conn).unwrap();
    let hlc: String = db
        .conn
        .query_row(
            "SELECT MAX(hlc) FROM ops WHERE endpoint = ?1",
            [mine],
            |r| r.get(0),
        )
        .unwrap();
    parse_hlc(&hlc).unwrap().0
}

#[test]
fn an_op_from_a_clock_a_year_ahead_waits_and_does_not_pin_this_clock() {
    let (a, b, c) = (
        Db::open_in_memory().unwrap(),
        Db::open_in_memory().unwrap(),
        Db::open_in_memory().unwrap(),
    );
    let from_a = log_at(&a, YEAR, "coffee");
    let s = apply_at(&b, 0, &from_a);
    assert_eq!((s.applied, s.held), (0, from_a.len()));
    let waits = b.sync_clock_waits().unwrap();
    assert_eq!(waits.len(), 1);
    assert_eq!(waits[0].endpoint, own_head(&a.conn).unwrap().0);
    assert_eq!(waits[0].ops, from_a.len());
    assert!(
        waits[0].to_string().contains("1 change from")
            && waits[0].to_string().contains("its clock is 1 year ahead")
    );

    let from_b = log_at(&b, 0, "tea");
    assert!(own_latest(&b) <= now_ms(), "b's op is stamped at a's year");

    // B's clock passes the op's HLC less the limit: the next apply
    // applies it, and B folds as a copy that applied it at once.
    let late = YEAR - MAX_CLOCK_AHEAD.as_millis() as i64 + 60_000;
    let s = apply_at(&b, late, &[]);
    assert_eq!((s.applied, s.held), (from_a.len(), 0));
    assert!(b.sync_clock_waits().unwrap().is_empty());
    apply_at(&c, late, &from_a);
    apply_at(&c, late, &from_b);
    assert_eq!(
        b.sync_state_hash(None).unwrap().0,
        c.sync_state_hash(None).unwrap().0
    );
}

/// The state hash is kept until an entry changes (da-4be5x): a verify
/// after one reads the kept hash, and an insert, an update or a delete
/// of any entry row forgets it.
#[test]
fn the_state_hash_is_kept_until_an_entry_changes() {
    let db = Db::open_in_memory().unwrap();
    log_at(&db, 0, "coffee");
    let plant = |db: &Db| {
        db.sync_state_hash(None).unwrap();
        db.conn
            .execute("UPDATE sync_state_hashes SET hash = 'kept'", [])
            .unwrap();
        assert_eq!(db.sync_state_hash(None).unwrap().0, "kept");
    };
    for change in ["UPDATE entries SET state = '\"x\"'", "DELETE FROM entries"] {
        plant(&db);
        db.conn.execute(change, []).unwrap();
        let fresh = state_hash(shared_rows(&db.conn, None, None).unwrap().0);
        assert_eq!(db.sync_state_hash(None).unwrap(), fresh, "after {change}");
    }
    plant(&db);
    log_at(&db, 0, "tea");
    assert_ne!(
        db.sync_state_hash(None).unwrap().0,
        "kept",
        "after an insert"
    );
}

/// An op names who and what signed it (da-zjc9w), and the build before
/// origins reads it: that build's `Op`, this one without `origin`, denies
/// no key it does not know, and a signature is checked over the body as
/// sent, so the op verifies, applies and is served on whole.
#[test]
fn a_build_before_origins_reads_an_op_carrying_one() {
    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct Before {
        endpoint: String,
        seq: i64,
        hlc: String,
        hid: String,
        slug: String,
        kind: String,
        #[serde(default)]
        field: Option<String>,
        #[serde(default)]
        raw: Option<String>,
        #[serde(default)]
        alias_chain: Option<Value>,
        effect: Value,
        #[serde(default)]
        deps: BTreeMap<String, i64>,
        #[serde(default)]
        queued: Option<String>,
        sig: String,
    }
    let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
    let lines = log_at(&a, 0, "coffee");
    for line in &lines {
        let op: Value = serde_json::from_str(line).unwrap();
        assert_eq!(op["origin"]["program"], "datum", "{line}");
        assert_eq!(op["origin"]["version"], crate::build(), "{line}");
        serde_json::from_value::<Before>(op).expect("the build before reads it");
    }
    assert_eq!(apply_at(&b, 0, &lines).applied, lines.len());
    let served = b.sync_ops_since(&BTreeMap::new(), false, None).unwrap();
    assert!(lines.iter().all(|l| served.contains(l)), "{served:?}");
}

/// A round with a limit is the ops lowest by HLC (da-ymgiv): each
/// applies whole, and the next round goes on from the vector it left.
#[test]
fn rounds_of_a_limit_apply_whole_and_continue_from_the_vector() {
    let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
    // Two endpoints, each op made after the other's last: a cut by
    // endpoint and seq would send ops whose deps it left out.
    let c = Db::open_in_memory().unwrap();
    for (i, f) in ["a", "b", "c", "d", "e", "f", "g"].into_iter().enumerate() {
        let (from, to) = if i % 2 == 0 { (&a, &c) } else { (&c, &a) };
        let lines = log_at(from, 0, f);
        apply_at(to, 0, &lines);
    }
    let all = a.sync_ops_since(&BTreeMap::new(), false, None).unwrap();
    let mut rounds = 0;
    loop {
        let since = b.sync_vector().unwrap();
        let round = a.sync_ops_round(&since, false, None, Some(3)).unwrap();
        if round.is_empty() {
            break;
        }
        assert!(round.len() <= 3, "{round:?}");
        let s = apply_at(&b, 0, &round);
        assert_eq!((s.applied, s.held), (round.len(), 0), "{round:?}");
        rounds += 1;
    }
    assert_eq!(rounds, all.len().div_ceil(3));
    assert_eq!(
        a.sync_state_hash(None).unwrap(),
        b.sync_state_hash(None).unwrap()
    );
}

#[test]
fn an_op_a_minute_ahead_applies_at_once() {
    let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
    let from_a = log_at(&a, 60_000, "coffee");
    let s = apply_at(&b, 0, &from_a);
    assert_eq!((s.applied, s.held), (from_a.len(), 0));
    assert!(b.sync_clock_waits().unwrap().is_empty());
}

#[test]
fn a_refused_op_does_not_move_the_clock() {
    let (a, b) = (Db::open_in_memory().unwrap(), Db::open_in_memory().unwrap());
    apply_at(&b, 0, &log_at(&a, YEAR, "coffee"));
    b.conn.execute("UPDATE ops SET applied = 2", []).unwrap();
    log_at(&b, 0, "tea");
    assert!(own_latest(&b) <= now_ms());
}
