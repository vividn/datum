//! Integration tests for `datum stats` (da-ye0q).
//!
//! Cover the duration-reconstruction core (switch-block gaps + the open
//! trailing block), the `field:state` selector, time-slice clipping, and the
//! `SLICE` ⇔ `--since/--until` equivalence the bead calls out.

mod common;

use chrono::{DateTime, Utc};
use common::run_cli;
use datum_cli::args::{StatsArgs, StatsPeriod};
use datum_commands::commands::stats::{compute_all_fields, compute_stats};
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
    assert_eq!(r.states[0].ratio, Some(0.75));
    assert_eq!(r.states[1].state, "meeting");
    assert_eq!(r.states[1].count, 1);
    assert_eq!(r.states[1].duration_seconds, 1800);
    assert_eq!(r.states[1].ratio, Some(0.25));
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
    assert_eq!(
        r.states[0].ratio,
        Some(0.75),
        "ratio is vs the tracked total"
    );
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
fn stats_bounds_speak_the_shared_time_grammar() {
    // da-rye2: `--since`/`--until` route through the shared parser rather
    // than a private reimplementation, so the compact `<mmm><day>` date
    // shorthand every logging command takes now works here too — and
    // resolves to exactly the same window as the ISO spelling.
    let _g = freeze_time("2026-06-20T12:00:00Z");
    let db = fresh_db();
    seed_activity(&db);

    let compact = compute_stats(
        &StatsArgs {
            since: Some("jun1-2026".to_string()),
            until: Some("jun30-2026".to_string()),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();
    let iso = compute_stats(
        &StatsArgs {
            since: Some("2026-06-01".to_string()),
            until: Some("2026-06-30".to_string()),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();

    assert_eq!(compact, iso);
    assert!(
        compact.total_count > 0,
        "the window must actually cover June"
    );
}

// ─── A block hands the field back (da-mkqo) ─────────────────────────────────

/// The shape of vividn's ledger: a switch, an interrupting block that has
/// already finished, and the stretch after it that is still the switched
/// state. `datum end work 12m` writes `following_state IS NULL`, which
/// means "resume whatever this block interrupted" (da-phoe) — so the
/// minutes past the block's trailing edge belong to the interrupted state,
/// not to nothing.
///
/// Asserts the TOTAL, not the buckets: `datum = 30m, false = 12m` is
/// individually plausible and passes on the broken code. The defect is
/// only visible in what goes MISSING, so the attributed total has to equal
/// the elapsed span of the window.
#[test]
fn stats_attributes_the_span_a_finished_block_handed_back() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    // switch work datum @10:00; end work dur=12m @10:30 → block [10:30, 10:42).
    entry_builder("work")
        .verb(Verb::Switch)
        .state(json!("datum"))
        .time_start_utc(at("2026-06-15T10:00:00Z"))
        .insert(&db);
    entry_builder("work")
        .verb(Verb::Block)
        .state(json!(false))
        .time_start_utc(at("2026-06-15T10:30:00Z"))
        .time_end_utc(at("2026-06-15T10:42:00Z"))
        .insert(&db);

    let r = compute_stats(&selector("work"), &db).unwrap();

    // 10:00 → now (12:00) is 2h, and every minute of it is attributed.
    assert_eq!(
        r.total_duration_seconds, 7200,
        "the 78m after the block's trailing edge is not attributed to nothing"
    );
    assert_eq!(
        dur(&r, "datum"),
        30 * 60 + 78 * 60,
        "30m before + 78m after"
    );
    assert_eq!(dur(&r, "false"), 12 * 60, "the block's own span, no more");
}

/// Two blocks back to back resume the same interrupted state — a block is
/// an interruption, so it never becomes the state to come back to.
#[test]
fn stats_resumes_past_every_finished_block() {
    let _g = freeze_time("2026-06-15T11:00:00Z");
    let db = fresh_db();
    entry_builder("work")
        .verb(Verb::Switch)
        .state(json!("datum"))
        .time_start_utc(at("2026-06-15T10:00:00Z"))
        .insert(&db);
    for (start, end) in [
        ("2026-06-15T10:10:00Z", "2026-06-15T10:15:00Z"),
        ("2026-06-15T10:30:00Z", "2026-06-15T10:40:00Z"),
    ] {
        entry_builder("work")
            .verb(Verb::Block)
            .state(json!(false))
            .time_start_utc(at(start))
            .time_end_utc(at(end))
            .insert(&db);
    }

    let r = compute_stats(&selector("work"), &db).unwrap();

    assert_eq!(r.total_duration_seconds, 3600, "10:00 → now, all of it");
    assert_eq!(dur(&r, "false"), 15 * 60, "5m + 10m of blocks");
    assert_eq!(dur(&r, "datum"), 45 * 60, "the rest resumes datum");
}

/// With nothing before it, a block has no state to hand back to: the
/// stretch after its trailing edge stays unattributed rather than
/// inventing a state. Same answer `now` gives (da-phoe).
#[test]
fn stats_resumes_to_nothing_when_a_block_interrupted_nothing() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    entry_builder("work")
        .verb(Verb::Block)
        .state(json!(false))
        .time_start_utc(at("2026-06-15T10:00:00Z"))
        .time_end_utc(at("2026-06-15T10:12:00Z"))
        .insert(&db);

    let r = compute_stats(&selector("work"), &db).unwrap();

    assert_eq!(r.total_duration_seconds, 12 * 60, "just the block");
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

/// The per-field API still insists on a field. A bare `stats` is not an
/// error any more — it goes to [`compute_all_fields`] instead (da-ptuwr).
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

// ─── Ratio denominator (da-icba) ────────────────────────────────────────────

/// activity: coding@10:00 → stop@10:30 → coding@11:00 → stop@11:15, with
/// `now` frozen at 12:00. coding = 30m + 15m = 45m tracked; the closed
/// (`false`) state fills the two gaps = 30m + 45m = 75m.
fn seed_with_closed(db: &datum_core::db::Db) {
    for (t, state) in [
        ("2026-06-15T10:00:00Z", json!("coding")),
        ("2026-06-15T10:30:00Z", json!(false)),
        ("2026-06-15T11:00:00Z", json!("coding")),
        ("2026-06-15T11:15:00Z", json!(false)),
    ] {
        entry_builder("activity")
            .verb(Verb::Switch)
            .state(state)
            .time_start_utc(at(t))
            .insert(db);
    }
}

#[test]
fn stats_excludes_closed_state_from_the_denominator() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_with_closed(&db);

    let r = compute_stats(&selector("activity"), &db).unwrap();

    // Durations and counts are untouched — only the ratio denominator moves.
    assert_eq!(r.total_duration_seconds, 7200, "the whole 2h is measured");
    assert_eq!(r.tracked_duration_seconds, 2700, "45m of coding");
    assert_eq!(r.total_count, 4);
    let closed = r.states.iter().find(|s| s.state == "false").unwrap();
    assert_eq!(closed.duration_seconds, 4500);
    assert_eq!(closed.count, 2);
    assert_eq!(closed.ratio, None, "false is outside its own denominator");
    let coding = r.states.iter().find(|s| s.state == "coding").unwrap();
    assert_eq!(coding.duration_seconds, 2700);
    assert_eq!(
        coding.ratio,
        Some(1.0),
        "coding is all of the tracked time, not 37.5% of wall-clock"
    );
}

#[test]
fn stats_include_false_restores_the_wall_clock_denominator() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_with_closed(&db);

    let args = StatsArgs {
        include_false: true,
        ..selector("activity")
    };
    let r = compute_stats(&args, &db).unwrap();

    assert_eq!(r.tracked_duration_seconds, r.total_duration_seconds);
    let closed = r.states.iter().find(|s| s.state == "false").unwrap();
    assert_eq!(closed.ratio, Some(0.625));
    let coding = r.states.iter().find(|s| s.state == "coding").unwrap();
    assert_eq!(coding.ratio, Some(0.375));
}

#[test]
fn stats_excludes_untracked_states_from_the_denominator() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    // reading@10:00 → IGNORED@10:30 → null@11:00 → reading@11:30.
    for (t, state) in [
        ("2026-06-15T10:00:00Z", json!("reading")),
        ("2026-06-15T10:30:00Z", json!("IGNORED")),
        ("2026-06-15T11:00:00Z", json!(null)),
        ("2026-06-15T11:30:00Z", json!("reading")),
    ] {
        entry_builder("activity")
            .verb(Verb::Switch)
            .state(state)
            .time_start_utc(at(t))
            .insert(&db);
    }

    let r = compute_stats(&selector("activity"), &db).unwrap();
    assert_eq!(r.total_duration_seconds, 7200);
    assert_eq!(r.tracked_duration_seconds, 3600, "30m + 30m of reading");
    for untracked in ["IGNORED", "null"] {
        let s = r.states.iter().find(|s| s.state == untracked).unwrap();
        assert_eq!(s.duration_seconds, 1800);
        assert_eq!(s.ratio, None, "{untracked} is outside the denominator");
    }
    let reading = r.states.iter().find(|s| s.state == "reading").unwrap();
    assert_eq!(reading.ratio, Some(1.0));

    let all = compute_stats(
        &StatsArgs {
            include_untracked: true,
            ..selector("activity")
        },
        &db,
    )
    .unwrap();
    assert_eq!(all.tracked_duration_seconds, 7200);
    for state in ["IGNORED", "null", "reading"] {
        let s = all.states.iter().find(|s| s.state == state).unwrap();
        assert!(s.ratio.is_some(), "{state} is back in the denominator");
    }
}

#[test]
fn stats_selecting_an_excluded_state_still_reports_a_ratio() {
    // `stats activity:false` asks about the closed state outright — it is
    // the subject of the query, so it stays in its own denominator.
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_with_closed(&db);

    let r = compute_stats(&selector("activity:false"), &db).unwrap();
    assert_eq!(r.states[0].duration_seconds, 4500);
    assert_eq!(r.states[0].ratio, Some(0.625), "share of the whole 2h");
    assert_eq!(r.tracked_duration_seconds, 7200);
}

#[test]
fn stats_flags_parse_and_dispatch() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_with_closed(&db);
    run_cli(&["stats", "activity", "--include-false"], &db).unwrap();
    run_cli(&["stats", "activity", "--include-untracked", "--json"], &db).unwrap();
}

// ─── Period flags and the shared timing flags (da-rf57) ─────────────────────

/// Seed with an explicit IANA zone so the window boundaries these tests
/// assert on are the database's own zone (`Db::zone_timeline`), not the
/// host's — otherwise every "local midnight" below would move with `TZ`.
fn seed_utc_zoned(db: &datum_core::db::Db, rows: &[(&str, &str)]) {
    seed_zoned(db, "UTC", rows);
}

/// The same, in an arbitrary IANA zone — the db's own zone timeline then
/// decides where local midnight falls, whatever `TZ` the test host is in.
fn seed_zoned(db: &datum_core::db::Db, tz: &str, rows: &[(&str, &str)]) {
    for (t, state) in rows {
        entry_builder("activity")
            .verb(Verb::Switch)
            .state(json!(state))
            .time_start(datum_core::time::DatumTime::from_utc_and_tz(at(t), tz).unwrap())
            .insert(db);
    }
}

/// coding runs from 23:00 the previous day, so the block straddles
/// midnight in both directions: 1h of it belongs to the 14th, and it is
/// still open at `now` on the 15th.
fn seed_across_midnight(db: &datum_core::db::Db) {
    seed_utc_zoned(
        db,
        &[
            ("2026-06-14T20:00:00Z", "meeting"),
            ("2026-06-14T23:00:00Z", "coding"),
        ],
    );
}

/// Total measured duration over one period window.
fn period_total(db: &datum_core::db::Db, period: StatsPeriod) -> i64 {
    compute_stats(
        &StatsArgs {
            period: Some(period),
            ..selector("activity")
        },
        db,
    )
    .unwrap()
    .total_duration_seconds
}

fn dur(r: &datum_commands::commands::stats::StatsResult, state: &str) -> i64 {
    r.states
        .iter()
        .find(|s| s.state == state)
        .map(|s| s.duration_seconds)
        .unwrap_or(0)
}

#[test]
fn today_runs_to_now_and_clips_the_block_that_crossed_midnight() {
    let _g = freeze_time("2026-06-15T09:00:00Z");
    let db = fresh_db();
    seed_across_midnight(&db);

    let r = compute_stats(
        &StatsArgs {
            period: Some(StatsPeriod::Today),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();

    // Nine hours of today have happened; the denominator is those, not 24h.
    assert_eq!(r.total_duration_seconds, 9 * 3600);
    assert_eq!(
        dur(&r, "coding"),
        9 * 3600,
        "only today's share of the block"
    );
    assert_eq!(
        dur(&r, "meeting"),
        0,
        "yesterday's block is out of the window"
    );
    assert_eq!(r.total_count, 0, "neither entry *occurs* today");
}

/// da-4uaa: `--today` opens at LOCAL midnight, and the two are only
/// distinguishable in a zone that is not UTC.
///
/// Every other period test here seeds `UTC`-stamped rows, which puts local
/// midnight and UTC midnight on the same instant — so none of them can tell
/// a local day from a UTC one. These two freeze `now` on the side of
/// midnight where the local and UTC *dates* disagree, once east of UTC and
/// once west, because one side alone is not a test: east, a UTC-aligned day
/// would open a day early and sweep yesterday in; west, it would open
/// nineteen hours late and hand back a nearly empty report.
#[test]
fn today_opens_at_local_midnight_east_of_utc() {
    // 23:30Z is 00:30 the *next* day in Lisbon (UTC+1 in September).
    let _g = freeze_time("2026-09-08T23:30:00Z");
    let db = fresh_db();
    seed_zoned(
        &db,
        "Europe/Lisbon",
        &[
            ("2026-09-08T22:50:00Z", "night"),   // 23:50 local, yesterday
            ("2026-09-08T23:10:00Z", "morning"), // 00:10 local, today
        ],
    );

    let r = compute_stats(
        &StatsArgs {
            period: Some(StatsPeriod::Today),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();

    assert_eq!(
        r.total_duration_seconds, 1800,
        "half an hour of local today has happened"
    );
    assert_eq!(dur(&r, "morning"), 1200, "00:10 local through now");
    assert_eq!(dur(&r, "night"), 600, "only its spill past local midnight");
    assert_eq!(
        r.total_count, 1,
        "the 00:10 entry occurs today; the 23:50 one does not"
    );
}

#[test]
fn today_opens_at_local_midnight_west_of_utc() {
    // 04:30Z is still 23:30 the *previous* day in Chicago (UTC-5 in September).
    let _g = freeze_time("2026-09-09T04:30:00Z");
    let db = fresh_db();
    seed_zoned(
        &db,
        "America/Chicago",
        &[
            ("2026-09-08T04:50:00Z", "night"),   // 23:50 local, yesterday
            ("2026-09-08T05:10:00Z", "morning"), // 00:10 local, today
        ],
    );

    let r = compute_stats(
        &StatsArgs {
            period: Some(StatsPeriod::Today),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();

    assert_eq!(
        r.total_duration_seconds,
        23 * 3600 + 1800,
        "the whole local day so far, not the 4h30m since UTC midnight"
    );
    assert_eq!(dur(&r, "morning"), 23 * 3600 + 1200);
    assert_eq!(dur(&r, "night"), 600, "only its spill past local midnight");
    assert_eq!(r.total_count, 1, "the 00:10 local entry is inside today");
}

#[test]
fn a_bare_date_flag_is_that_day_alone() {
    // vividn's rule: a DATE is bounded. `-y` on the 15th is the whole of the
    // 14th — not "since the 14th", which would sweep today in too.
    let _g = freeze_time("2026-06-15T09:00:00Z");
    let db = fresh_db();
    seed_across_midnight(&db);

    let mut args = selector("activity");
    args.time.yesterday = 1;
    let r = compute_stats(&args, &db).unwrap();

    assert_eq!(
        r.total_duration_seconds,
        4 * 3600,
        "20:00→24:00 on the 14th"
    );
    assert_eq!(dur(&r, "meeting"), 3 * 3600);
    assert_eq!(dur(&r, "coding"), 3600, "clipped at the 15th's midnight");
    assert_eq!(r.total_count, 2, "both entries occur on the 14th");
}

#[test]
fn until_reopens_a_dates_closed_end() {
    // The documented escape hatch for "since two days ago": `-yy --until now`.
    let _g = freeze_time("2026-06-15T09:00:00Z");
    let db = fresh_db();
    seed_across_midnight(&db);

    let mut args = selector("activity");
    args.time.yesterday = 1;
    args.until = Some("now".to_string());
    let r = compute_stats(&args, &db).unwrap();

    assert_eq!(
        r.total_duration_seconds,
        13 * 3600,
        "20:00 on the 14th through 09:00 on the 15th"
    );
    assert_eq!(dur(&r, "coding"), 10 * 3600, "no longer cut at midnight");
}

#[test]
fn a_clock_time_is_a_lower_bound_like_since() {
    // `-t` and `-q` name a moment, so they behave exactly as `--since`
    // does — open-ended, ending at now.
    let _g = freeze_time("2026-06-15T09:00:00Z");
    let db = fresh_db();
    seed_across_midnight(&db);

    let mut by_time = selector("activity");
    by_time.time.time = Some("2026-06-14T22:00:00Z".to_string());
    let mut by_quick = selector("activity");
    by_quick.time.quick = 2; // 10 minutes back

    let since_time = compute_stats(
        &StatsArgs {
            since: Some("2026-06-14T22:00:00Z".to_string()),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();
    let since_quick = compute_stats(
        &StatsArgs {
            since: Some("10m".to_string()),
            ..selector("activity")
        },
        &db,
    )
    .unwrap();

    assert_eq!(compute_stats(&by_time, &db).unwrap(), since_time);
    assert_eq!(compute_stats(&by_quick, &db).unwrap(), since_quick);
    assert_eq!(since_quick.total_duration_seconds, 600);
}

#[test]
fn a_date_plus_a_time_is_a_lower_bound_not_a_day() {
    // `-d ... -t ...` composes into a moment, so the day-bounding rule
    // does not apply — the window runs to now.
    let _g = freeze_time("2026-06-15T09:00:00Z");
    let db = fresh_db();
    seed_across_midnight(&db);

    let mut args = selector("activity");
    // `-z UTC` so the composed wall-clock reading is the seed's own zone,
    // whatever `TZ` the test host happens to be in.
    args.time.timezone = Some("UTC".to_string());
    args.time.date = Some("2026-06-14".to_string());
    args.time.time = Some("22:00".to_string());
    let r = compute_stats(&args, &db).unwrap();

    assert_eq!(r.total_duration_seconds, 11 * 3600, "22:00 through now");
}

#[test]
fn closed_periods_stop_where_the_open_ones_start() {
    let _g = freeze_time("2026-06-17T09:00:00Z"); // a Wednesday
    let db = fresh_db();
    seed_utc_zoned(&db, &[("2026-05-20T10:00:00Z", "coding")]);

    // A single open block runs from May 20 to now, so each window's total
    // is exactly its own length — the periods measure themselves.
    assert_eq!(period_total(&db, StatsPeriod::Today), 9 * 3600);
    assert_eq!(
        period_total(&db, StatsPeriod::Month),
        16 * 86400 + 9 * 3600,
        "June 1 through now"
    );
    assert_eq!(
        period_total(&db, StatsPeriod::LastMonth),
        11 * 86400 + 14 * 3600,
        "May 20 10:00 (the block's start) through June 1 — not through now"
    );
}

#[test]
fn one_window_start_at_a_time() {
    let db = fresh_db();
    let mut both = selector("activity");
    both.period = Some(StatsPeriod::Today);
    both.since = Some("1d".to_string());
    let err = compute_stats(&both, &db).unwrap_err().to_string();
    assert!(err.contains("one window start"), "{err}");

    let mut flags_and_slice = selector("activity");
    flags_and_slice.slice = Some("2026-06".to_string());
    flags_and_slice.time.yesterday = 1;
    assert!(compute_stats(&flags_and_slice, &db).is_err());
}

#[test]
fn an_inverted_window_is_an_error_not_a_zero() {
    // A zero total is indistinguishable from "you did nothing", which is
    // the wrong answer to give for a backwards range.
    let _g = freeze_time("2026-06-15T09:00:00Z");
    let db = fresh_db();
    seed_across_midnight(&db);

    let args = StatsArgs {
        since: Some("2026-06-15T08:00:00Z".to_string()),
        until: Some("2026-06-15T07:00:00Z".to_string()),
        ..selector("activity")
    };
    let err = compute_stats(&args, &db).unwrap_err().to_string();
    assert!(err.contains("empty window"), "{err}");
}

#[test]
fn period_and_timing_flags_parse_and_dispatch() {
    let _g = freeze_time("2026-06-15T09:00:00Z");
    let db = fresh_db();
    seed_across_midnight(&db);
    for flag in [
        "--today",
        "--week",
        "--last-week",
        "--month",
        "--last-month",
    ] {
        run_cli(&["stats", "activity", flag], &db).unwrap_or_else(|e| panic!("{flag}: {e}"));
    }
    run_cli(&["stats", "activity", "-y"], &db).unwrap();
    run_cli(&["stats", "activity", "-yy", "--until", "now"], &db).unwrap();
    run_cli(&["stats", "activity", "-qq", "--json"], &db).unwrap();
    run_cli(&["stats", "activity", "-t", "8:00", "-z", "UTC"], &db).unwrap();
    run_cli(&["stats", "activity", "-d", "2026-06-14", "-z", "UTC"], &db).unwrap();

    // Two different period flags contradict rather than refine.
    assert!(run_cli(&["stats", "activity", "--today", "--week"], &db).is_err());
}

// ─── Every field, ranked: a bare `stats` (da-ptuwr) ─────────────────────────

/// Three shapes at once, so the table has something of each to rank:
/// a switch field (activity, 1h30m of coding + 30m of meeting), a block
/// field (work, one 1h block), and a point field (coffee, two instants and
/// no duration at all). `now` is 12:00.
fn seed_three_shapes(db: &datum_core::db::Db) {
    seed_activity(db);
    entry_builder("work")
        .verb(Verb::Block)
        .state(json!(true))
        .time_start_utc(at("2026-06-15T09:00:00Z"))
        .time_end_utc(at("2026-06-15T10:00:00Z"))
        .insert(db);
    for t in ["2026-06-15T08:00:00Z", "2026-06-15T09:30:00Z"] {
        entry_builder("coffee")
            .verb(Verb::Point)
            .state(json!(null))
            .time_start_utc(at(t))
            .insert(db);
    }
}

#[test]
fn bare_stats_ranks_every_field_by_tracked_time() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_three_shapes(&db);

    let r = compute_all_fields(&StatsArgs::default(), &db).unwrap();

    let names: Vec<&str> = r.fields.iter().map(|f| f.field.as_str()).collect();
    assert_eq!(names, ["activity", "work", "coffee"], "ranked by duration");

    let activity = &r.fields[0];
    assert_eq!(activity.duration_seconds, 7200, "90m coding + 30m meeting");
    assert_eq!(activity.count, 3);
    let work = &r.fields[1];
    assert_eq!(work.duration_seconds, 3600, "the stored 1h block");
    assert_eq!(work.count, 1);
    let coffee = &r.fields[2];
    assert_eq!(coffee.duration_seconds, 0, "a point field takes no time");
    assert_eq!(coffee.count, 2, "and is still counted");

    assert_eq!(r.tracked_duration_seconds, 10800);
    assert_eq!(r.total_count, 6);
    assert_eq!(activity.ratio, Some(2.0 / 3.0));
    assert_eq!(work.ratio, Some(1.0 / 3.0));
    assert_eq!(coffee.ratio, Some(0.0));
}

#[test]
fn bare_stats_ranks_on_tracked_time_not_wall_clock() {
    // The reason the row is a field's TRACKED duration: `activity`'s total
    // is the whole 2h, of which 75m is the closed state — and a table
    // ranked on totals would rank every block field the same (da-icba, one
    // level out).
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_with_closed(&db);

    let r = compute_all_fields(&StatsArgs::default(), &db).unwrap();
    assert_eq!(r.fields.len(), 1);
    assert_eq!(r.fields[0].duration_seconds, 2700, "45m of coding, not 2h");
    assert_eq!(r.fields[0].ratio, Some(1.0));
}

#[test]
fn bare_stats_leaves_out_a_field_with_nothing_in_the_window() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_three_shapes(&db);

    // A window that opens after the last coffee and the work block.
    let args = StatsArgs {
        since: Some("2026-06-15T10:30:00Z".to_string()),
        ..StatsArgs::default()
    };
    let r = compute_all_fields(&args, &db).unwrap();
    let names: Vec<&str> = r.fields.iter().map(|f| f.field.as_str()).collect();
    assert_eq!(names, ["activity"], "a field with no time is not a row");
}

#[test]
fn bare_stats_on_an_empty_database_is_an_empty_table() {
    let db = fresh_db();
    let r = compute_all_fields(&StatsArgs::default(), &db).unwrap();
    assert!(r.fields.is_empty());
    assert_eq!(r.total_count, 0);
    assert_eq!(r.tracked_duration_seconds, 0);
}

#[test]
fn bare_stats_runs_through_cli_dispatch() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_three_shapes(&db);
    // The form that used to be an error: no selector at all.
    run_cli(&["stats"], &db).unwrap();
    run_cli(&["stats", "--json"], &db).unwrap();
    run_cli(&["stats", "--today"], &db).unwrap();
}

// ─── When the window's record starts and ends (da-aynet) ────────────────────

fn instant(t: Option<chrono::DateTime<chrono::FixedOffset>>) -> Option<DateTime<Utc>> {
    t.map(|t| t.with_timezone(&Utc))
}

#[test]
fn stats_says_when_its_window_starts_and_ends() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_three_shapes(&db);

    // Every field: the first coffee to the last activity switch.
    let all = compute_all_fields(&StatsArgs::default(), &db).unwrap();
    assert_eq!(instant(all.earliest), Some(at("2026-06-15T08:00:00Z")));
    assert_eq!(instant(all.latest), Some(at("2026-06-15T11:00:00Z")));

    // One field: its own entries, not the database's.
    let r = compute_stats(&selector("activity"), &db).unwrap();
    assert_eq!(instant(r.earliest), Some(at("2026-06-15T10:00:00Z")));
    assert_eq!(instant(r.latest), Some(at("2026-06-15T11:00:00Z")));

    // The window's, not the whole record's: the coffees and the work block
    // fall before it.
    let args = StatsArgs {
        since: Some("2026-06-15T10:15:00Z".to_string()),
        ..StatsArgs::default()
    };
    let all = compute_all_fields(&args, &db).unwrap();
    assert_eq!(instant(all.earliest), Some(at("2026-06-15T10:30:00Z")));
    assert_eq!(instant(all.latest), Some(at("2026-06-15T11:00:00Z")));
}

#[test]
fn stats_window_bounds_are_null_when_it_counts_nothing() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    seed_activity(&db);

    // The open coding block runs into this window, but no entry lands in it.
    let args = StatsArgs {
        since: Some("2026-06-15T11:30:00Z".to_string()),
        ..selector("activity")
    };
    let r = compute_stats(&args, &db).unwrap();
    assert!(r.tracked_duration_seconds > 0);
    assert_eq!((r.earliest, r.latest), (None, None));

    let all = compute_all_fields(&StatsArgs::default(), &fresh_db()).unwrap();
    assert_eq!((all.earliest, all.latest), (None, None));
}

#[test]
fn stats_window_bounds_keep_the_zone_they_were_logged_in() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    entry_builder("coffee")
        .verb(Verb::Point)
        .time_start(datum_core::time::DatumTime {
            utc: at("2026-06-15T09:00:00Z"),
            offset_minutes: -300,
            timezone: Some("America/Chicago".to_string()),
            uncertainty: None,
        })
        .insert(&db);

    let r = compute_stats(&selector("coffee"), &db).unwrap();
    assert_eq!(
        r.earliest.map(|t| t.to_rfc3339()).as_deref(),
        Some("2026-06-15T04:00:00-05:00")
    );
}

// ─── A field that takes no time says how often (da-uv7fu) ───────────────────

/// Run the real binary in a fresh home, TZ=UTC, colour off.
fn datum(home: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
        .env_remove("DATUM_DB")
        .env("DATUM_HOME", home)
        .env("DATUM_CONFIG_HOME", home.join("config"))
        .env("DATUM_COLOR", "0")
        .env("TZ", "UTC")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// vividn: «in stats for occur based fields, showing the time doesn't make any
/// sense […] it would be nice to show some frequency pattern, like how often
/// per day/week/month/year during the entire history of tracking».
#[test]
fn an_occur_field_shows_how_often_and_no_time() {
    let tmp = tempfile::tempdir().unwrap();
    for d in ["2026-01-01", "2026-01-02", "2026-03-15"] {
        datum(tmp.path(), &["occur", "coffee", "-d", d, "-t", "08:00"]);
    }

    let json: serde_json::Value =
        serde_json::from_str(&datum(tmp.path(), &["stats", "coffee", "--json"])).unwrap();
    let f = &json["frequency"];
    let span = f["span_seconds"].as_i64().unwrap();
    // First entry to now: the rates are the count over that span.
    let first = at("2026-01-01T08:00:00Z");
    assert!(
        (span - (Utc::now() - first).num_seconds()).abs() < 60,
        "{span}"
    );
    let per_day = 3.0 * 86_400.0 / span as f64;
    for (key, days) in [
        ("per_day", 1.0),
        ("per_week", 7.0),
        ("per_month", 365.2425 / 12.0),
        ("per_year", 365.2425),
    ] {
        let got = f[key].as_f64().unwrap();
        assert!((got - per_day * days).abs() < 1e-9, "{key}: {got}");
    }

    let text = datum(tmp.path(), &["stats", "coffee"]);
    let head = text.lines().next().unwrap();
    assert!(
        head.starts_with("coffee  ·  ") && head.contains("/day · ") && head.contains("/year"),
        "{text}"
    );
    assert!(head.ends_with("3 total since 2026-01-01"), "{text}");
    // No time anywhere: not the gaps between the coffees, not a share.
    assert!(!text.contains('%') && !text.contains("tracked"), "{text}");
    assert!(
        !text.split_whitespace().any(|w| w.ends_with(['h', 'm', 's'])
            && w.chars().all(|c| c.is_ascii_digit() || "hms".contains(c))),
        "{text}"
    );

    // One level out, the field's cell is its rate, not `0s`.
    let all = datum(tmp.path(), &["stats"]);
    let row = all.lines().find(|l| l.ends_with("coffee")).unwrap();
    assert!(row.contains("/day") && !row.contains("0s"), "{all}");
    // Both runs pinned to one `--until`: each measures its span to the wall
    // clock otherwise, and a second boundary between them flakes (da-m3aww).
    let until = ["--until", "2026-06-01T08:00:00Z", "--json"];
    let one: serde_json::Value = serde_json::from_str(&datum(
        tmp.path(),
        &[&["stats", "coffee"][..], &until].concat(),
    ))
    .unwrap();
    let all: serde_json::Value =
        serde_json::from_str(&datum(tmp.path(), &[&["stats"][..], &until].concat())).unwrap();
    assert_eq!(all["fields"][0]["frequency"], one["frequency"]);
    assert_eq!(one["frequency"]["span_seconds"], 151 * 86_400);
}

#[test]
fn a_field_that_takes_time_keeps_its_time_and_has_no_frequency() {
    let tmp = tempfile::tempdir().unwrap();
    // A fixed day and one `--until`: `-t -90` / `-t -30` each read the wall
    // clock in their own process, and a second boundary between them made
    // the hour 3601 s (da-sf082).
    datum(
        tmp.path(),
        &["start", "work", "-d", "2026-06-01", "-t", "0630"],
    );
    datum(
        tmp.path(),
        &["stop", "work", "-d", "2026-06-01", "-t", "0730"],
    );
    let until = ["--until", "2026-06-01T08:00:00Z"];

    let text = datum(tmp.path(), &[&["stats", "work"][..], &until].concat());
    assert!(text.starts_with("work  ·  1h30m over 2 entries"), "{text}");
    assert!(text.contains("100.0%") && !text.contains("/day"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&datum(
        tmp.path(),
        &[&["stats", "work", "--json"][..], &until].concat(),
    ))
    .unwrap();
    assert_eq!(json["frequency"], serde_json::Value::Null);
    assert_eq!(json["tracked_duration_seconds"], 3600);
}

/// A rare field reads in significant figures, never `0.0/day`; a lone
/// entry today is a day's worth, not thousands a day.
#[test]
fn rates_keep_their_figures_and_a_span_is_at_least_a_day() {
    let _g = freeze_time("2026-06-15T12:00:00Z");
    let db = fresh_db();
    entry_builder("sneeze")
        .verb(Verb::Point)
        .state(json!(null))
        .time_start_utc(at("2026-06-15T11:59:00Z"))
        .insert(&db);
    let f = compute_stats(&selector("sneeze"), &db)
        .unwrap()
        .frequency
        .unwrap();
    assert_eq!((f.span_seconds, f.per_day), (86_400, 1.0));

    let tmp = tempfile::tempdir().unwrap();
    datum(tmp.path(), &["occur", "eclipse", "-d", "2016-01-01"]);
    let text = datum(tmp.path(), &["stats", "eclipse"]);
    let per_day = text
        .split("/day")
        .next()
        .unwrap()
        .rsplit(' ')
        .next()
        .unwrap();
    assert!(per_day.starts_with("0.000") && per_day != "0.000", "{text}");
    assert_eq!(per_day.trim_start_matches(['0', '.']).len(), 2, "{text}");
}
