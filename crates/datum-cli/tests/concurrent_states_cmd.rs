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

mod common;

use common::run_cli;
use datum_core::conflicts::{ConflictKind, ListFilter};
use datum_core::test_util::fresh_db;
use serde_json::json;

/// Acceptance: `datum switch field state1,state2` creates an entry with
/// `value = ["state1","state2"]`. Trailing comma not required.
#[test]
fn switch_comma_separated_states_becomes_array_value() {
    let db = fresh_db();

    run_cli(&["switch", "task", "reading,writing"], &db)
        .expect("switch with comma-separated states");

    let entries = db.entries_for_field("task", None).unwrap();
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

    let entries = db.entries_for_field("task", None).unwrap();
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

    let entries = db.entries_for_field("task", None).unwrap();
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

    let entries = db.entries_for_field("task", None).unwrap();
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

    let entries = db.entries_for_field("body", None).unwrap();
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

    let entries = db.entries_for_field("task", None).unwrap();
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
        db.entries_for_field("task", None).unwrap().len(),
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

    run_cli(&["switch", "project", "datum,household"], &db)
        .expect("switch to concurrent states");

    let entries = db.entries_for_field("project", None).unwrap();
    let ts = entries[0].occur_time.as_ref().unwrap().utc;
    assert_eq!(
        db.get_active_state("project", ts).unwrap(),
        json!(["datum", "household"]),
        "array value flows through state compilation unchanged",
    );
}

/// Acceptance: `datum switch field state1` while the field is currently
/// `[state1,state2]` files a `same_state_repeat` (partial overlap).
#[test]
fn array_to_scalar_with_overlap_flagged_as_same_state_repeat() {
    let db = fresh_db();

    run_cli(&["switch", "project", "datum,household", "-t=-1"], &db)
        .expect("seed array state");
    run_cli(&["switch", "project", "datum"], &db).expect("switch back to single member");

    let conflicts = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::SameStateRepeat),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(
        conflicts.len(),
        1,
        "switching back to an already-active member is a partial repeat",
    );
}

/// Scalar → array where the new array includes the current scalar is also
/// a partial repeat.
#[test]
fn scalar_to_array_with_overlap_flagged_as_same_state_repeat() {
    let db = fresh_db();

    run_cli(&["switch", "project", "datum", "-t=-1"], &db).expect("seed scalar");
    run_cli(&["switch", "project", "datum,household"], &db)
        .expect("switch to array containing same member");

    let conflicts = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::SameStateRepeat),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(conflicts.len(), 1);
}

/// Array → array with overlapping elements is a partial repeat.
#[test]
fn array_to_array_with_overlap_flagged_as_same_state_repeat() {
    let db = fresh_db();

    run_cli(&["switch", "project", "datum,household", "-t=-1"], &db)
        .expect("seed first array");
    run_cli(&["switch", "project", "datum,gastown"], &db).expect("switch to overlapping array");

    let conflicts = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::SameStateRepeat),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(
        conflicts.len(),
        1,
        "overlapping elements between old and new arrays is a partial repeat",
    );
}

/// Array → disjoint array (no shared elements) is NOT a same-state-repeat.
#[test]
fn array_to_array_disjoint_is_not_same_state_repeat() {
    let db = fresh_db();

    run_cli(&["switch", "project", "datum,household", "-t=-1"], &db)
        .expect("seed first array");
    run_cli(&["switch", "project", "gastown,beads"], &db).expect("switch to disjoint array");

    let conflicts = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::SameStateRepeat),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(
        conflicts.len(),
        0,
        "fully disjoint element sets are a clean transition, not a repeat",
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
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::LastStateMismatch),
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
    run_cli(&["switch", "project", "d", "-L=a,b"], &db)
        .expect("switch with wrong -L assertion");

    let mismatches = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::LastStateMismatch),
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

    run_cli(&["switch", "project", "c", "-L=a,b"], &db)
        .expect("switch with comma-separated -L");

    let entry = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(
        entry.user_asserted_last_state,
        Some(json!(["a", "b"])),
        "-L accepts comma-separated array assertions",
    );
}
