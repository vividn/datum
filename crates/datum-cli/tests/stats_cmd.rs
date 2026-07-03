//! Integration tests for `datum stats` (da-ye0q).
//!
//! Cover the duration-reconstruction core (switch-block gaps + the open
//! trailing block), the `field:state` selector, time-slice clipping, and the
//! `SLICE` ⇔ `--since/--until` equivalence the bead calls out.

mod common;

use chrono::{DateTime, Utc};
use common::run_cli;
use datum_cli::args::StatsArgs;
use datum_commands::commands::stats::compute_stats;
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, freeze_time, fresh_db};
use serde_json::json;

fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

/// activity switches: coding@10:00 → meeting@10:30 → coding@11:00 (open),
/// with `now` frozen at 12:00. coding = 30m + 60m, meeting = 30m.
fn seed_activity(db: &datum_core::db::Db) {
    for (t, state) in [
        ("2026-06-15T10:00:00Z", "coding"),
        ("2026-06-15T10:30:00Z", "meeting"),
        ("2026-06-15T11:00:00Z", "coding"),
    ] {
        entry_builder("activity")
            .verb(Verb::Switch)
            .state(json!(state))
            .time_start_utc(at(t))
            .insert(db);
    }
}

fn selector(sel: &str) -> StatsArgs {
    StatsArgs {
        selector: Some(sel.to_string()),
        ..StatsArgs::default()
    }
}

#[test]
fn stats_reconstructs_durations_and_ratios() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_activity(&db);

    let r = compute_stats(&selector("activity"), &db).unwrap();

    assert_eq!(r.total_count, 3);
    assert_eq!(r.total_duration_seconds, 7200, "90m coding + 30m meeting");
    // Longest-first ordering.
    assert_eq!(r.states[0].state, "coding");
    assert_eq!(r.states[0].count, 2);
    assert_eq!(r.states[0].duration_seconds, 5400);
    assert_eq!(r.states[0].ratio, 0.75);
    assert_eq!(r.states[1].state, "meeting");
    assert_eq!(r.states[1].count, 1);
    assert_eq!(r.states[1].duration_seconds, 1800);
    assert_eq!(r.states[1].ratio, 0.25);
}

#[test]
fn stats_since_clips_the_leading_block() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_activity(&db);

    // Window opens mid-way through the meeting block (10:45).
    let args = StatsArgs {
        since: Some("2026-06-15T10:45:00Z".to_string()),
        ..selector("activity")
    };
    let r = compute_stats(&args, &db).unwrap();

    // Duration is clipped to the window; the pre-window coding block (ends
    // 10:30) contributes nothing.
    assert_eq!(
        r.total_duration_seconds, 4500,
        "60m coding + 15m meeting tail"
    );
    // Only the 11:00 coding entry occurs inside the window → count 1.
    assert_eq!(r.total_count, 1);
    let coding = r.states.iter().find(|s| s.state == "coding").unwrap();
    assert_eq!(coding.duration_seconds, 3600);
    assert_eq!(coding.count, 1);
    let meeting = r.states.iter().find(|s| s.state == "meeting").unwrap();
    assert_eq!(meeting.duration_seconds, 900);
    assert_eq!(meeting.count, 0, "meeting entry predates the window");
}

#[test]
fn stats_field_state_selector_isolates_one_state() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_activity(&db);

    let r = compute_stats(&selector("activity:coding"), &db).unwrap();

    assert_eq!(r.selected_state.as_deref(), Some("coding"));
    assert_eq!(r.states.len(), 1);
    assert_eq!(r.states[0].state, "coding");
    assert_eq!(r.states[0].duration_seconds, 5400);
    assert_eq!(r.states[0].count, 2);
    assert_eq!(r.states[0].ratio, 0.75, "ratio is vs the whole field");
    // Totals stay whole-field so the JSON still describes the field.
    assert_eq!(r.total_duration_seconds, 7200);
    assert_eq!(r.total_count, 3);
}

#[test]
fn stats_field_state_absent_yields_zero_row() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_activity(&db);

    let r = compute_stats(&selector("activity:sleeping"), &db).unwrap();
    assert_eq!(r.states.len(), 1);
    assert_eq!(r.states[0].state, "sleeping");
    assert_eq!(r.states[0].count, 0);
    assert_eq!(r.states[0].duration_seconds, 0);
}

#[test]
fn stats_shorthand_slice_matches_since_until() {
    // The bead's contract: `stats activity 2026-06` ≡
    // `stats activity --since 2026-06-01 --until 2026-06-30`.
    let _g = freeze_time("2026-06-20T12:00:00Z");
    let db = fresh_db();
    seed_activity(&db);

    let shorthand = compute_stats(
        &StatsArgs {
            slice: Some("2026-06".to_string()),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();
    let explicit = compute_stats(
        &StatsArgs {
            since: Some("2026-06-01".to_string()),
            until: Some("2026-06-30".to_string()),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();

    assert_eq!(shorthand, explicit);
}

#[test]
fn stats_stored_dur_block_does_not_gap_fill() {
    // A discrete block with a stored `dur` is measured by that span; the gap
    // to the next entry is NOT attributed to it (unlike a bare switch).
    let _g = freeze_time("2026-06-15T20:00:00Z");
    let db = fresh_db();
    // run: 30-minute block at 08:00, then yoga: 20-minute block at 18:00.
    entry_builder("exercise")
        .verb(Verb::Switch)
        .state(json!("run"))
        .time_start_utc(at("2026-06-15T08:00:00Z"))
        .time_end_utc(at("2026-06-15T08:30:00Z"))
        .insert(&db);
    entry_builder("exercise")
        .verb(Verb::Switch)
        .state(json!("yoga"))
        .time_start_utc(at("2026-06-15T18:00:00Z"))
        .time_end_utc(at("2026-06-15T18:20:00Z"))
        .insert(&db);

    let r = compute_stats(&selector("exercise"), &db).unwrap();
    let run = r.states.iter().find(|s| s.state == "run").unwrap();
    assert_eq!(
        run.duration_seconds, 1800,
        "just the stored 30m, no 10h gap"
    );
    let yoga = r.states.iter().find(|s| s.state == "yoga").unwrap();
    assert_eq!(yoga.duration_seconds, 1200);
    assert_eq!(r.total_duration_seconds, 3000);
}

#[test]
fn stats_missing_selector_errors() {
    let db = fresh_db();
    assert!(compute_stats(&StatsArgs::default(), &db).is_err());
}

#[test]
fn stats_slice_with_since_is_rejected() {
    let db = fresh_db();
    let args = StatsArgs {
        slice: Some("2026".to_string()),
        since: Some("2026-06-01".to_string()),
        ..selector("activity")
    };
    assert!(compute_stats(&args, &db).is_err());
}

#[test]
fn stats_runs_through_cli_dispatch() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_activity(&db);
    // Smoke-test the full parse → dispatch path for both forms.
    run_cli(&["stats", "activity"], &db).unwrap();
    run_cli(&["stats", "activity:coding", "--json"], &db).unwrap();
    run_cli(&["stats", "activity", "2026-06"], &db).unwrap();
}
