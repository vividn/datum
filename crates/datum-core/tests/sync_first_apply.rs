//! da-kg95e: a first sync applies every entry of the other copy at once.
//! Ordered by `Op::order`, which puts tombstone baselines after live ones,
//! each pass applied one run of seqs and re-read every op still waiting:
//! quadratic in a lived-in db. This holds a 20k-entry apply into an empty
//! db to a generous bound, and checks the copies converge.

use std::collections::BTreeMap;
use std::time::Instant;

use chrono::{Duration, TimeZone, Utc};
use datum_core::db::{ConflictStrategy, Db};
use datum_core::dur::Dur;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use serde_json::json;

/// `n` entries over a dozen fields, one per 10 minutes: switches, occurs
/// and blocks, a tenth of them deleted, as a lived-in db holds. Bulk
/// inserted, as a restored backup: no op covers them, so each is signed a
/// baseline, and live and tombstone baselines alternate in seq.
fn populate(db: &Db, n: usize) {
    let base = Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap();
    let entries: Vec<Entry> = (1..=n)
        .map(|i| {
            let t = DatumTime {
                utc: base - Duration::minutes(10 * i as i64),
                offset_minutes: 0,
                timezone: Some("UTC".into()),
                uncertainty: None,
            };
            let (verb, state, dur) = match i % 3 {
                0 => (Verb::Point, json!(true), Some(Dur::moment())),
                1 => (Verb::Switch, json!(["on", "off"][i % 2]), None),
                _ => (
                    Verb::Block,
                    json!(true),
                    Some(Dur::of(Duration::minutes(5))),
                ),
            };
            let field = format!("f{}", i % 12);
            let mut e = Entry::new(&field, verb, Some(state), None, Some(t), dur, "bench");
            e.deleted_at = (i % 10 == 0).then_some(base);
            e
        })
        .collect();
    db.insert_entries_bulk(&entries, ConflictStrategy::Ignore, &mut |_| {})
        .unwrap();
}

#[test]
fn a_first_sync_of_20k_lived_in_entries_applies_in_one_pass() {
    let tmp = tempfile::tempdir().unwrap();
    let path = |n: &str| tmp.path().join(n).to_string_lossy().into_owned();
    let (a, b) = (
        Db::open_in_memory().unwrap(),
        Db::open(&path("b.db")).unwrap(),
    );
    populate(&a, 20_000);

    let t = Instant::now();
    let lines = a.sync_ops_since(&BTreeMap::new(), false).unwrap();
    let signed = t.elapsed();
    let t = Instant::now();
    let s = b.sync_apply(&lines.join("\n")).unwrap();
    let applied = t.elapsed();
    eprintln!("{} ops: read {signed:?}, apply {applied:?}", lines.len());

    assert_eq!((s.applied, s.held, s.refused.len()), (lines.len(), 0, 0));
    assert_eq!(
        a.sync_state_hash().unwrap().0,
        b.sync_state_hash().unwrap().0
    );
    // Measured unloaded: 6 s release, 26 s debug; one pass per run of live
    // or tombstone baselines took 632 s release. Generous for a loaded
    // runner, far under the quadratic.
    assert!(applied.as_secs() < 180, "apply took {applied:?}");
}
