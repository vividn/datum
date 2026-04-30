//! End-to-end migration chain test (da-9u9.2).
//!
//! Walks the full registry: from a fresh v=CURRENT DB, walk all the way
//! down to v1, then forward to CURRENT, then back down to v1. Verifies
//! that:
//!   * every step's `down` succeeds (schema-only reversibility),
//!   * every step's `apply` succeeds when run on the down-walked shape,
//!   * the round-trip is stable: no step accumulates state that breaks a
//!     subsequent forward or backward walk.

use datum_core::db::Db;
use datum_core::migrations::{
    apply_migrations, apply_migrations_down, read_schema_version, CURRENT_SCHEMA_VERSION,
    MIGRATIONS,
};

/// Helper: fresh in-memory DB at v=CURRENT.
fn fresh_current() -> Db {
    Db::open_in_memory().unwrap()
}

#[test]
fn chain_walks_down_to_v1_then_up_to_current_then_down_again() {
    let db = fresh_current();
    let conn = db.conn();

    assert_eq!(
        read_schema_version(conn).unwrap(),
        Some(CURRENT_SCHEMA_VERSION),
        "fresh in-memory DB starts at CURRENT",
    );

    // Phase 1: walk down v=CURRENT → v=1, one step at a time.
    for step in MIGRATIONS.iter().rev() {
        let before = read_schema_version(conn).unwrap().unwrap();
        assert_eq!(
            before, step.to,
            "down walk expected to be at v{} before reversing v{}→v{}",
            step.to, step.from, step.to,
        );
        let applied = apply_migrations_down(conn, MIGRATIONS, before, step.from).unwrap();
        assert_eq!(
            applied.len(),
            1,
            "exactly one down step should run per call"
        );
        let after = read_schema_version(conn).unwrap().unwrap();
        assert_eq!(
            after, step.from,
            "down step v{}→v{} must land at v{}",
            step.to, step.from, step.from,
        );
    }
    assert_eq!(
        read_schema_version(conn).unwrap(),
        Some(1),
        "after down-walking the registry, schema_version is 1",
    );

    // Phase 2: walk up v=1 → v=CURRENT, one step at a time.
    for step in MIGRATIONS.iter() {
        let before = read_schema_version(conn).unwrap().unwrap();
        assert_eq!(
            before, step.from,
            "up walk expected to be at v{} before applying v{}→v{}",
            step.from, step.from, step.to,
        );
        let applied = apply_migrations(conn, MIGRATIONS, before, step.to).unwrap();
        assert_eq!(applied.len(), 1, "exactly one up step should run per call");
        let after = read_schema_version(conn).unwrap().unwrap();
        assert_eq!(
            after, step.to,
            "up step v{}→v{} must land at v{}",
            step.from, step.to, step.to,
        );
    }
    assert_eq!(
        read_schema_version(conn).unwrap(),
        Some(CURRENT_SCHEMA_VERSION),
        "after up-walking the registry, schema_version is CURRENT",
    );

    // Phase 3: walk back down again. Must be stable — the second down
    // pass exercises the same downs against a chain-walked shape, not
    // the freshly-opened shape.
    let applied = apply_migrations_down(conn, MIGRATIONS, CURRENT_SCHEMA_VERSION, 1).unwrap();
    assert_eq!(
        applied.len() as u32,
        CURRENT_SCHEMA_VERSION - 1,
        "second down pass walks every registered step",
    );
    assert_eq!(read_schema_version(conn).unwrap(), Some(1));
}

/// Bulk down + bulk up in single calls (no per-step assertions). Catches
/// transaction-level regressions — both runners must commit cleanly when
/// asked to walk the entire span at once.
#[test]
fn chain_walks_all_at_once_in_both_directions() {
    let db = fresh_current();
    let conn = db.conn();

    let down_summaries =
        apply_migrations_down(conn, MIGRATIONS, CURRENT_SCHEMA_VERSION, 1).unwrap();
    assert_eq!(down_summaries.len() as u32, CURRENT_SCHEMA_VERSION - 1);
    assert_eq!(read_schema_version(conn).unwrap(), Some(1));

    let up_summaries = apply_migrations(conn, MIGRATIONS, 1, CURRENT_SCHEMA_VERSION).unwrap();
    assert_eq!(up_summaries.len() as u32, CURRENT_SCHEMA_VERSION - 1);
    assert_eq!(
        read_schema_version(conn).unwrap(),
        Some(CURRENT_SCHEMA_VERSION),
    );
}

/// Walking down to the same version is a no-op.
#[test]
fn down_walk_is_no_op_when_current_equals_target() {
    let db = fresh_current();
    let applied = apply_migrations_down(
        db.conn(),
        MIGRATIONS,
        CURRENT_SCHEMA_VERSION,
        CURRENT_SCHEMA_VERSION,
    )
    .unwrap();
    assert!(applied.is_empty());
}

/// Asking the runner to walk "down" to a higher version is rejected.
#[test]
fn down_walk_errors_when_target_above_current() {
    let db = fresh_current();
    let err = apply_migrations_down(db.conn(), MIGRATIONS, 1, 2).unwrap_err();
    assert!(
        matches!(err, datum_core::error::DatumError::IncompatibleSchema(_)),
        "expected IncompatibleSchema for backwards down-walk, got {err:?}",
    );
}
