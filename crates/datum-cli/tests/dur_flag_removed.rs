//! da-azqq: `--dur` is gone, and this file is what keeps it gone.
//!
//! The flag was not merely a third spelling of a duration already
//! expressible two other ways (a trailing positional, `datum end work
//! 12m`; and the reserved key, `dur=12m`). It disagreed with those two
//! about which side of `-t` the block falls on: `end work --dur 12m -t
//! 14:00` wrote a block ending at 14:00, while both surviving spellings
//! write one starting at 14:00. Two spellings of "a twelve minute break"
//! landed twelve minutes apart.
//!
//! A deleted flag comes back because nothing objects when someone adds
//! it. These tests object. If you are here because one of them failed
//! after you restored `--dur`, the answer is not to relax the test —
//! it is that the duration has two spellings and they agree with each
//! other.

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;

/// Every logging verb shares one flag registry, so `--dur` is either
/// present on all of them or none. Assert none — and assert the failure
/// is the *tombstone*, which names both surviving spellings, rather than
/// a bare "unknown flag" a reader would take for an oversight.
#[test]
fn dur_flag_is_rejected_on_every_logging_verb() {
    for verb in ["end", "stop", "start", "switch", "occur", "add", "note"] {
        let db = fresh_db();
        for argv in [
            vec![verb, "work", "--dur", "12m"],
            vec![verb, "work", "--dur=12m"],
        ] {
            let err = run_cli(&argv, &db)
                .map(|_| String::new())
                .unwrap_or_else(|e| format!("{e:#}"));
            assert!(
                !err.is_empty(),
                "`datum {}` must fail — `--dur` was removed",
                argv.join(" "),
            );
            assert!(
                err.contains("`--dur` was removed"),
                "`datum {}` must fail with the tombstone message, got: {err}",
                argv.join(" "),
            );
            // The message has to teach the replacement, or the next
            // person restores the flag rather than learning the two
            // forms that survived.
            assert!(
                err.contains("datum end work 12m") && err.contains("dur=12m"),
                "tombstone must name both surviving spellings, got: {err}",
            );
        }
    }
}

/// The reason `--dur` went: the two surviving spellings agree with each
/// other about `-t`, and it did not. Both anchor the block AT `-t` and
/// run forward, so `end work 12m -t 14:00` and `end work dur=12m -t
/// 14:00` are the same twelve minutes.
#[test]
fn both_surviving_spellings_anchor_the_block_at_minus_t() {
    let db = fresh_db();

    run_cli(&["end", "positional", "12m", "-t", "14:00"], &db).expect("trailing positional dur");
    run_cli(&["end", "reserved", "dur=12m", "-t", "14:00"], &db).expect("dur= key");

    let positional = &db.entries_for_field("positional", None).unwrap()[0];
    let reserved = &db.entries_for_field("reserved", None).unwrap()[0];

    assert_eq!(
        positional.occur_time.as_ref().unwrap().utc,
        reserved.occur_time.as_ref().unwrap().utc,
        "both spellings start the block at -t, not one on each side of it",
    );
    assert_eq!(positional.duration_seconds(), Some(12 * 60));
    assert_eq!(reserved.duration_seconds(), Some(12 * 60));

    // And forward, not backward: the block ends after it starts.
    let ts = positional.occur_time.as_ref().unwrap().utc;
    let te = positional
        .computed_end_time()
        .expect("end time derived")
        .utc;
    assert!(te > ts, "the span runs forward from -t");
}
