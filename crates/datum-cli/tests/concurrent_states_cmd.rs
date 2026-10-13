//! S12 parity tests (da-u4k.12): concurrent array states on a field.
//!
//! Covers:
//!   * parser: comma-separated `switch field a,b` → JSON array value
//!   * trailing/leading/empty segments rejected with a helpful error
//!   * single-state switch stays scalar
//!   * state timeline: an array value is the active state between events
//!   * same-state-repeat detection: scalar→array, array→scalar, array→array
//!     overlap are all flagged via the D5 conflict queue
//!   * last-state mismatch assertion handles array values order-insensitively

use crate::common;

use common::run_cli;
use datum_core::integrity::{IntegrityKind, ListFilter};
use datum_core::test_util::fresh_db;
use serde_json::json;

/// Acceptance: `datum switch field state1,state2` creates an entry with
/// `value = ["state1","state2"]`. Trailing comma not required.
#[test]
fn switch_comma_separated_states_becomes_array_value() {
    let db = fresh_db();

    run_cli(&["switch", "task", "reading,writing"], &db)
        .expect("switch with comma-separated states");

    let entries = datum_core::test_util::logged_entries(&db, "task");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].state.as_ref(),
        Some(&json!(["reading", "writing"])),
        "comma-split states land as a JSON array in `value`",
    );
}

/// Acceptance: a single state with no comma stays scalar — NOT wrapped.
#[test]
fn switch_single_state_stays_scalar() {
    let db = fresh_db();

    run_cli(&["switch", "task", "reading"], &db).expect("single-state switch");

    let entries = datum_core::test_util::logged_entries(&db, "task");
    assert_eq!(entries[0].state.as_ref(), Some(&json!("reading")));
}

/// da-lvvp / da-5z01: edge commas on a switch state-list are tolerated
/// — old datum aliases produced inputs like `,a,b` and `a,b,`, and the
/// parser strips the empty edge so the input round-trips during the
/// transitional period.
#[test]
fn switch_trailing_comma_tolerated() {
    let db = fresh_db();

    run_cli(&["switch", "task", "reading,writing,"], &db)
        .expect("trailing comma should be tolerated");

    let entries = datum_core::test_util::logged_entries(&db, "task");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].state.as_ref(),
        Some(&json!(["reading", "writing"])),
        "trailing edge comma stripped, remaining states land as an array",
    );
}

#[test]
fn switch_leading_comma_tolerated() {
    let db = fresh_db();

    run_cli(&["switch", "task", ",reading,writing"], &db)
        .expect("leading comma should be tolerated");

    let entries = datum_core::test_util::logged_entries(&db, "task");
    assert_eq!(
        entries[0].state.as_ref(),
        Some(&json!(["reading", "writing"])),
        "leading edge comma stripped, remaining states land as an array",
    );
}

/// da-5z01 regression: the original real-world failure case.
/// `switch body ,vertical,ground` must parse as `["vertical","ground"]`.
#[test]
fn switch_leading_comma_two_states_da_5z01() {
    let db = fresh_db();

    run_cli(&["switch", "body", ",vertical,ground"], &db)
        .expect("da-5z01: `,vertical,ground` must round-trip to [vertical,ground]");

    let entries = datum_core::test_util::logged_entries(&db, "body");
    assert_eq!(
        entries[0].state.as_ref(),
        Some(&json!(["vertical", "ground"])),
    );
}

/// Single-state with edge commas degenerates to scalar — `,reading`
/// and `reading,` are both equivalent to bare `reading`.
#[test]
fn switch_edge_comma_single_state_stays_scalar() {
    let db = fresh_db();

    run_cli(&["switch", "task", ",reading"], &db).expect("leading-edge single state");

    let entries = datum_core::test_util::logged_entries(&db, "task");
    assert_eq!(entries[0].state.as_ref(), Some(&json!("reading")));
}

#[test]
fn switch_empty_segment_rejected() {
    let db = fresh_db();
    let err = run_cli(&["switch", "task", "a,,b"], &db)
        .expect_err("empty segment must error")
        .to_string();
    assert!(err.contains("empty segment"), "{err}");
    assert_eq!(
        datum_core::test_util::logged_entries(&db, "task").len(),
        0,
        "rejected parse must not persist any entry",
    );
}

/// Edge-comma tolerance does not extend to interior `,,`: `,a,,b`
/// still rejected to prevent silent `null` array elements.
#[test]
fn switch_edge_plus_interior_empty_still_rejected() {
    let db = fresh_db();
    let err = run_cli(&["switch", "task", ",a,,b"], &db)
        .expect_err("interior empty must error even with edge comma")
        .to_string();
    assert!(err.contains("empty segment"), "{err}");
}

/// State timeline: with an array value, the active state at `time_start`
/// compiles to the normalized array.
#[test]
fn array_state_is_queryable_over_timeline() {
    let db = fresh_db();

    run_cli(&["switch", "project", "datum,household"], &db).expect("switch to concurrent states");

    let entries = datum_core::test_util::logged_entries(&db, "project");
    let ts = entries[0].occur_time.as_ref().unwrap().utc;
    assert_eq!(
        db.get_active_state("project", ts).unwrap(),
        json!(["datum", "household"]),
        "array value flows through state compilation unchanged",
    );
}

/// da-g1b8: `datum switch field state1` while the field is currently
/// `[state1,state2]` DROPS an element — an ordinary transition, not a
/// repeat. `repeated_state` means equality; partial overlap is not it.
#[test]
fn array_to_scalar_with_overlap_is_not_repeated_state() {
    use datum_core::integrity;
    let db = fresh_db();

    run_cli(&["switch", "project", "datum,household", "-t=-1"], &db).expect("seed array state");
    run_cli(&["switch", "project", "datum"], &db).expect("switch back to single member");

    let hits = integrity::list_repeated_states(db.conn()).unwrap();
    assert!(
        hits.is_empty(),
        "narrowing a parallel state to one of its members is a transition",
    );
}

/// da-g1b8, the reported case: scalar → array that ADDS an element
/// (`vertical` → `[vertical,sit]`) is ordinary use of parallel states.
#[test]
fn scalar_to_array_with_overlap_is_not_repeated_state() {
    use datum_core::integrity;
    let db = fresh_db();

    run_cli(&["switch", "body", "vertical", "-t=-1"], &db).expect("seed scalar");
    run_cli(&["switch", "body", "vertical,sit"], &db).expect("add a concurrent element");

    let hits = integrity::list_repeated_states(db.conn()).unwrap();
    assert!(
        hits.is_empty(),
        "adding an element to a parallel state is a transition, not a repeat",
    );
}

/// da-g1b8: array → array sharing some but not all elements swaps one
/// concurrent state for another. Still a transition.
#[test]
fn array_to_array_with_partial_overlap_is_not_repeated_state() {
    use datum_core::integrity;
    let db = fresh_db();

    run_cli(&["switch", "project", "datum,household", "-t=-1"], &db).expect("seed first array");
    run_cli(&["switch", "project", "datum,garden"], &db).expect("switch to overlapping array");

    let hits = integrity::list_repeated_states(db.conn()).unwrap();
    assert!(
        hits.is_empty(),
        "partial overlap between old and new arrays is a transition",
    );
}

/// Array → disjoint array (no shared elements) is NOT a repeat.
#[test]
fn array_to_array_disjoint_is_not_repeated_state() {
    use datum_core::integrity;
    let db = fresh_db();

    run_cli(&["switch", "project", "datum,household", "-t=-1"], &db).expect("seed first array");
    run_cli(&["switch", "project", "garden,cooking"], &db).expect("switch to disjoint array");

    let hits = integrity::list_repeated_states(db.conn()).unwrap();
    assert!(
        hits.is_empty(),
        "fully disjoint element sets are a clean transition, not a repeat",
    );
}

/// da-g1b8: reordering the same members is not a transition — the
/// multiset is unchanged, so it stays flagged.
#[test]
fn array_to_reordered_same_members_is_repeated_state() {
    use datum_core::integrity;
    let db = fresh_db();

    run_cli(&["switch", "body", "vertical,sit", "-t=-1"], &db).expect("seed array state");
    run_cli(&["switch", "body", "sit,vertical"], &db).expect("re-log the same members reordered");

    let hits = integrity::list_repeated_states(db.conn()).unwrap();
    assert_eq!(
        hits.len(),
        1,
        "same multiset in a different order is a repeat, not a transition",
    );
}

/// `-L=a,b` assertion compares order-insensitively against the computed
/// prior state — `[b,a]` actual should match.
#[test]
fn last_state_assertion_matches_array_regardless_of_order() {
    let db = fresh_db();

    run_cli(&["switch", "project", "a,b", "-t=-1"], &db).expect("seed [a,b]");
    // Assert the prior was "b,a" (same elements, different order). No
    // mismatch should be filed.
    run_cli(&["switch", "project", "c", "-L=b,a"], &db)
        .expect("switch with -L assertion (reordered)");

    let mismatches = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::LastStateMismatch),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(
        mismatches.len(),
        0,
        "array assertion is order-insensitive; reordered `-L` should not file a mismatch",
    );
}

/// `-L=a,b` vs actual `["a","c"]` (different elements) IS a mismatch.
#[test]
fn last_state_assertion_flags_element_mismatch() {
    let db = fresh_db();

    run_cli(&["switch", "project", "a,c", "-t=-1"], &db).expect("seed [a,c]");
    run_cli(&["switch", "project", "d", "-L=a,b"], &db).expect("switch with wrong -L assertion");

    let mismatches = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::LastStateMismatch),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(mismatches.len(), 1);
}

/// `-L=a,b` is stored on the entry as a JSON array in
/// `user_asserted_last_state`.
#[test]
fn last_state_assertion_records_array_value() {
    let db = fresh_db();

    run_cli(&["switch", "project", "c", "-L=a,b"], &db).expect("switch with comma-separated -L");

    let entry = &datum_core::test_util::logged_entries(&db, "project")[0];
    assert_eq!(
        entry.user_asserted_last_state,
        Some(json!(["a", "b"])),
        "-L accepts comma-separated array assertions",
    );
}

/// da-z9efl: IGNORED and TRACKED describe the whole field at an instant, so
/// a parallel state holding either is refused, however it was spelled, and
/// the refusal names the sentinel and why. Nothing is written.
#[test]
fn a_parallel_state_refuses_ignored_and_tracked() {
    let db = fresh_db();
    for argv in [
        &["switch", "task", "reading,TRACKED"][..],
        &["switch", "task", "IGNORED,reading"],
        &["switch", "task", "state=reading,TRACKED"],
        &["start", "task", "state=reading,IGNORED"],
        &[
            "switch",
            "task",
            "reading",
            "5m",
            "following_state=idle,TRACKED",
        ],
        &[
            "switch", "task", "reading", ",", "switch", "task", "TRACKED",
        ],
        &[
            "switch",
            "task",
            "reading",
            "5m",
            "following_state=TRACKED",
            ",",
            "switch",
            "task",
            "reading",
            "5m",
            "following_state=idle",
        ],
    ] {
        let err = run_cli(argv, &db).expect_err("refused").to_string();
        let sentinel = if argv.join(" ").contains("IGNORED") {
            "IGNORED"
        } else {
            "TRACKED"
        };
        assert!(
            err.starts_with(&format!("{sentinel} cannot be one of several states"))
                && err.contains("describe the whole field at that instant"),
            "{argv:?}: {err}"
        );
    }
    assert!(datum_core::test_util::logged_all(&db).is_empty());
}

/// da-z9efl: `null` is a value someone assigns, so it may stand among
/// parallel states; a data key's list holding the word TRACKED is no state.
#[test]
fn a_parallel_state_keeps_null_and_a_data_list_keeps_tracked() {
    let db = fresh_db();
    run_cli(&["switch", "task", "reading,null"], &db).expect("null among states");
    run_cli(&["note", "memo", "tags=a,TRACKED"], &db).expect("a data list");
    let task = datum_core::test_util::logged_entries(&db, "task");
    assert_eq!(task[0].state, Some(json!(["reading", null])));
    let memo = datum_core::test_util::logged_entries(&db, "memo");
    assert_eq!(
        memo[0].data.as_ref().unwrap()["tags"],
        json!(["a", "TRACKED"])
    );
}

/// da-z9efl: `update` refuses what it sets, and only that: a row written
/// before the refusal keeps its state and can still have other keys edited.
#[test]
fn update_refuses_a_sentinel_it_sets_but_edits_an_old_row() {
    let db = fresh_db();
    let old = datum_core::test_util::entry_builder("task")
        .verb(datum_core::entry::Verb::Switch)
        .state(json!(["reading", "TRACKED"]))
        .insert(&db);
    let hid = old.hid.to_string();
    let err = run_cli(&["update", &hid, "state=reading,IGNORED"], &db)
        .expect_err("refused")
        .to_string();
    assert!(
        err.starts_with("IGNORED cannot be one of several states"),
        "{err}"
    );
    run_cli(&["update", &hid, "mood=calm"], &db).expect("an old row stays editable");
    let task = datum_core::test_util::logged_entries(&db, "task");
    assert_eq!(task[0].state, Some(json!(["reading", "TRACKED"])));
}

/// da-cktfm: a state in brackets that is not JSON was split on its commas
/// with the brackets kept (`[a,IGNORED]` → `["[a","IGNORED]"]`), which also
/// slipped a sentinel past the refusal above. It is refused on every
/// spelling of a state, naming the JSON form; the JSON forms and a data
/// key's bracket text stand.
#[test]
fn a_bracketed_state_that_is_not_json_is_refused() {
    let db = fresh_db();
    for argv in [
        &["switch", "task", "[a,IGNORED]"][..],
        &["switch", "task", "[a, IGNORED]"],
        &["switch", "task", "state=[a,b]"],
        &["switch", "task", "state=[a, b]"],
        &["start", "task", "state=[a,b]"],
        &["switch", "task", "a", "5m", "following_state=[a,b]"],
        &["switch", "task", "a", "-L", "[a,b]"],
    ] {
        let err = run_cli(argv, &db).expect_err("refused").to_string();
        assert!(
            err.contains(r#"" is not a list: the bracket form is JSON: ["a","#),
            "{argv:?}: {err}"
        );
    }
    assert!(datum_core::test_util::logged_all(&db).is_empty());

    run_cli(&["switch", "task", "a"], &db).expect("a plain state");
    let hid = datum_core::test_util::logged_entries(&db, "task")[0]
        .hid
        .to_string();
    let err = run_cli(&["update", &hid, "state=[a,b]"], &db).expect_err("refused");
    assert!(err.to_string().contains("is not a list"), "{err}");

    run_cli(&["switch", "task", r#"["a","b"]"#], &db).expect("JSON");
    run_cli(&["switch", "task", "state=['c','d']", "-t", "+1"], &db).expect("relaxed JSON");
    run_cli(&["note", "memo", "tags=[a,b]"], &db).expect("a data key");
    let states: Vec<_> = datum_core::test_util::logged_entries(&db, "task")
        .into_iter()
        .map(|e| e.state)
        .collect();
    assert!(states.contains(&Some(json!(["a", "b"]))), "{states:?}");
    assert!(states.contains(&Some(json!(["c", "d"]))), "{states:?}");
    let memo = datum_core::test_util::logged_entries(&db, "memo");
    assert_eq!(memo[0].data.as_ref().unwrap()["tags"], json!(["[a", "b]"]));
}
