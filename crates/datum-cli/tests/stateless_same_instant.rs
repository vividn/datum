//! da-2fkr acceptance: two transactions on one day.
//!
//! `entries_field_occurtime` was built to stop two *state-changes* for one
//! field at one instant (da-ga8b), but its predicate never mentioned state,
//! so it also refused two stateless points — vividn's finance `tx` field,
//! where a recurring charge is dated to the day rather than to a moment.
//! These tests hold the narrowed rule from the outside: what the CLI now
//! accepts, what it still refuses, and what it says when it refuses.

mod common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::integrity::{self, IntegrityKind, ListFilter};
use datum_core::test_util::fresh_db;

/// Live (non-superseded, non-tombstoned) rows for a field, oldest first by
/// slug so the assertions name slugs rather than counting.
fn live_slugs(db: &Db, field: &str) -> Vec<String> {
    let mut slugs: Vec<String> = datum_core::test_util::logged_entries(db, field)
        .into_iter()
        .filter(|e| e.deleted_at.is_none() && e.superseded_by.is_none())
        .map(|e| e.slug.to_string())
        .collect();
    slugs.sort();
    slugs
}

/// The bead's headline: rent and electricity, both dated 2026-09-01, both
/// stored. The id template is what makes their slugs differ — the same
/// `--id %comment%` that did this job in datum v2.
#[test]
fn two_transactions_on_one_day_both_land() {
    let db = fresh_db();

    run_cli(
        &[
            "occur",
            "tx",
            "-k",
            "amount=1200",
            "-k",
            "comment=rent",
            "-d",
            "2026-09-01",
            "--id",
            "%comment%",
        ],
        &db,
    )
    .expect("rent");
    run_cli(
        &[
            "occur",
            "tx",
            "-k",
            "amount=40",
            "-k",
            "comment=electricity",
            "-d",
            "2026-09-01",
            "--id",
            "%comment%",
        ],
        &db,
    )
    .expect("electricity must land beside rent");

    assert_eq!(live_slugs(&db, "tx"), vec!["tx:electricity", "tx:rent"]);

    let entries = datum_core::test_util::logged_entries(&db, "tx");
    let times: Vec<String> = entries
        .iter()
        .map(|e| e.occur_time.as_ref().unwrap().utc.to_rfc3339())
        .collect();
    assert_eq!(times[0], times[1], "both sit at the same instant");
    assert!(
        entries.iter().all(|e| e.state.is_none()),
        "a transaction asserts no state",
    );

    // (7) Nothing about the pair is an integrity finding.
    assert!(
        integrity::list(db.conn(), &ListFilter::default())
            .unwrap()
            .is_empty(),
        "two stateless points at one instant are two events, not a finding",
    );
}

/// da-ga8b's case, unchanged: two rows that DO assert a state at one instant
/// are still refused, and still with the collider-naming message.
#[test]
fn two_state_changes_at_one_instant_are_still_refused() {
    let db = fresh_db();

    run_cli(&["switch", "project", "alpha", "-d", "2026-09-03"], &db).expect("alpha");
    let err = run_cli(
        &[
            "switch",
            "project",
            "beta",
            "-d",
            "2026-09-03",
            "--id",
            "%state%",
        ],
        &db,
    )
    .expect_err("the second state-change at one instant must be refused");

    let msg = format!("{err}");
    assert!(
        msg.contains("collides with") && msg.contains("field=project"),
        "the (field, occur_time) diagnostic must survive: {msg}"
    );
    assert_eq!(live_slugs(&db, "project"), vec!["project:2026-09-03"]);
}

/// Two stateless points with no id template still collide — on the *slug*,
/// because the default template is `%occur_time%` and they both resolve to
/// `tx:2026-09-02`. The message has to name the way out, since the slug is
/// now the only thing standing between the two.
#[test]
fn stateless_slug_collision_names_the_id_template() {
    let db = fresh_db();

    run_cli(&["occur", "tx", "-k", "amount=7", "-d", "2026-09-02"], &db).expect("first");
    let err = run_cli(&["occur", "tx", "-k", "amount=9", "-d", "2026-09-02"], &db)
        .expect_err("same slug, different data");

    let msg = format!("{err}");
    assert!(
        msg.contains("slug tx:2026-09-02"),
        "the message must name the contested slug: {msg}"
    );
    assert!(
        msg.contains("--id") && msg.contains("id-template"),
        "and the id template as the way out: {msg}"
    );
    assert_eq!(live_slugs(&db, "tx"), vec!["tx:2026-09-02"]);
}

/// The same-invocation merge is the same rule in a second place: two
/// transactions on one side of a `,` chain must stay two rows, not fold
/// into one with `amount: [1, 2]`.
#[test]
fn multi_invocation_keeps_two_transactions_apart() {
    let db = fresh_db();

    run_cli(
        &[
            "occur",
            "tx",
            "-k",
            "amount=1",
            "-k",
            "comment=rent",
            "-d",
            "2026-09-01",
            "--id",
            "%comment%",
            ",",
            "occur",
            "tx",
            "-k",
            "amount=2",
            "-k",
            "comment=elec",
            "-d",
            "2026-09-01",
            "--id",
            "%comment%",
        ],
        &db,
    )
    .expect("both sides land");

    assert_eq!(live_slugs(&db, "tx"), vec!["tx:elec", "tx:rent"]);
    let amounts: Vec<serde_json::Value> = {
        let mut entries = datum_core::test_util::logged_entries(&db, "tx");
        entries.sort_by(|a, b| a.slug.cmp(&b.slug));
        entries
            .iter()
            .map(|e| e.data.as_ref().unwrap().get("amount").unwrap().clone())
            .collect()
    };
    assert_eq!(
        amounts,
        vec![serde_json::json!(2), serde_json::json!(1)],
        "each transaction keeps its own amount — no array",
    );
}

/// And the da-gh0.C case it must not have broken: two switches on one side
/// of a `,` chain still fold to a single Parallel row.
#[test]
fn multi_invocation_still_folds_two_switches() {
    let db = fresh_db();

    run_cli(
        &[
            "switch",
            "body",
            "vertical",
            "-d",
            "2026-09-01",
            ",",
            "switch",
            "body",
            "sit",
            "-d",
            "2026-09-01",
        ],
        &db,
    )
    .expect("both sides land");

    let entries = datum_core::test_util::logged_entries(&db, "body");
    assert_eq!(entries.len(), 1, "the two switches fold to one row");
    assert_eq!(
        entries[0].state,
        Some(serde_json::json!(["vertical", "sit"])),
    );
}

/// The integrity checker must never call two stateless points a collision —
/// asserted directly against the kind, so a future detector that files one
/// is caught even if `datum check`'s summary stays green.
#[test]
fn no_identity_collision_is_filed_for_stateless_points() {
    let db = fresh_db();
    for comment in ["rent", "electricity"] {
        run_cli(
            &[
                "occur",
                "tx",
                "-k",
                &format!("comment={comment}"),
                "-d",
                "2026-09-01",
                "--id",
                "%comment%",
            ],
            &db,
        )
        .expect("both land");
    }

    let collisions = integrity::list(
        db.conn(),
        &ListFilter {
            kind: Some(IntegrityKind::IdentityCollision),
            pending: None,
        },
    )
    .unwrap();
    assert!(
        collisions.is_empty(),
        "expected no identity_collision rows, got {collisions:?}"
    );
}
