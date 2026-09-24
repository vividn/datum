//! da-mjwq: `start_time=` / `end_time=` on a log line are lifted onto the
//! row — `occur_time` and `dur` — through the parser `-t` uses, on every
//! verb that can carry a duration (`add`, `note`, `start`, `switch`,
//! `occur`). Nothing of the keys stays in `data`.
//!
//! Under the reference semantics (da-zwct) a block's `occur_time` is its
//! END and the duration precedes it, so `start posture start_time=4pm 15`
//! is the block 16:00–16:15: the row `start posture -t 16:15 15` writes.
//! Every assertion here is on `occur_time` / `dur` values, never on "a
//! block exists".

mod common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::integrity::{IntegrityKind, ListFilter};
use datum_core::test_util::{freeze_time, fresh_db};
use serde_json::{json, Value};

/// The rows of `field`, oldest first.
fn rows(db: &Db, field: &str) -> Vec<Entry> {
    let mut rows = datum_core::test_util::logged_entries(db, field);
    rows.sort_by_key(|e| e.occur_time.as_ref().map(|t| t.utc));
    rows
}

fn only(db: &Db, field: &str) -> Entry {
    let rows = rows(db, field);
    assert_eq!(rows.len(), 1, "one row for {field}, got {}", rows.len());
    rows.into_iter().next().unwrap()
}

/// The row as everything downstream reads it: the time-anchored columns,
/// the state and the data — everything but the hid and the write times.
fn shape(e: &Entry) -> Value {
    json!({
        "occur_time": e.occur_time.as_ref().map(|t| t.utc.to_rfc3339()),
        "occur_uncertainty": e.occur_uncertainty(),
        "dur": e.dur.as_ref().map(|d| d.to_storage_string()),
        "state": e.state,
        "data": e.data,
        "user_asserted_last_state": e.user_asserted_last_state,
    })
}

fn occur_utc(e: &Entry) -> String {
    e.occur_time
        .as_ref()
        .expect("row has a time")
        .utc
        .to_rfc3339()
}

fn dur(e: &Entry) -> Option<String> {
    e.dur.as_ref().map(|d| d.to_storage_string())
}

/// vividn's line: `start field start_time=4pm 15` "essentially equivalent
/// to `start field -t 16:15 15`". The block is 16:00–16:15, so its twin
/// is the `-t 16:15` row (occur_time is the END) — equal in every column
/// but the hid and the write times.
#[test]
fn start_time_with_a_duration_is_the_row_its_t_twin_writes() {
    let _g = freeze_time("2026-09-10T12:00:00");
    let keyed = fresh_db();
    run_cli(&["start", "posture", "start_time=4pm", "15"], &keyed).expect("keyed");
    let flagged = fresh_db();
    run_cli(&["start", "posture", "-t", "16:15", "15"], &flagged).expect("-t twin");

    let keyed = only(&keyed, "posture");
    let flagged = only(&flagged, "posture");
    assert_eq!(shape(&keyed), shape(&flagged));
    assert_eq!(dur(&keyed).as_deref(), Some("PT15M"));
    assert_eq!(
        keyed
            .occur_time
            .as_ref()
            .unwrap()
            .local_time()
            .naive_local(),
        "2026-09-10T16:15:00".parse().unwrap(),
        "the block ENDS at 16:15; the fifteen minutes precede it"
    );
    assert_eq!(keyed.data, None, "start_time= is consumed, not stored");
}

/// `start posture start_time=-20 end_time=-5` at a pinned clock: the
/// block ends five minutes ago and lasted fifteen. Both offsets resolve
/// against ONE reading of the clock — with two, the span came out
/// 14m59s.
#[test]
fn start_and_end_pair_lands_the_block_between_them() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let db = fresh_db();
    run_cli(&["start", "posture", "start_time=-20", "end_time=-5"], &db).expect("pair");

    let e = only(&db, "posture");
    assert_eq!(occur_utc(&e), "2026-09-10T11:55:00+00:00");
    assert_eq!(dur(&e).as_deref(), Some("PT15M"));
    assert_eq!(e.state, Some(json!(true)));
    assert_eq!(e.data, None, "no start_time/end_time keys in data");
}

/// `end_time=` alone on `start` closes the field's open block at E — the
/// action becomes `end`, so the row is the one `end posture -t -5` writes.
/// Through an alias `posture → start posture`, `posture end_time=-5` is
/// the same row: one alias serves both directions.
#[test]
fn end_time_alone_on_start_is_end_at_that_time() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let keyed = fresh_db();
    run_cli(&["start", "posture", "-t", "11:30"], &keyed).unwrap();
    run_cli(&["start", "posture", "end_time=-5"], &keyed).expect("end_time= alone");

    let flagged = fresh_db();
    run_cli(&["start", "posture", "-t", "11:30"], &flagged).unwrap();
    run_cli(&["end", "posture", "-t", "-5"], &flagged).expect("end -t -5");

    let aliased = fresh_db();
    run_cli(&["alias", "set", "posture", "start posture"], &aliased).unwrap();
    run_cli(&["posture", "-t", "11:30"], &aliased).unwrap();
    run_cli(&["posture", "end_time=-5"], &aliased).expect("alias end_time=");

    let want = shape(&rows(&flagged, "posture")[1]);
    assert_eq!(want["state"], json!(false), "an end row");
    assert_eq!(want["occur_time"], json!("2026-09-10T11:55:00+00:00"));
    assert_eq!(shape(&rows(&keyed, "posture")[1]), want);
    assert_eq!(shape(&rows(&aliased, "posture")[1]), want);
}

/// `start_time=S` alone on `start` is the open block that began at S:
/// `-t S`, nothing more.
#[test]
fn start_time_alone_on_start_opens_the_block_there() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let db = fresh_db();
    run_cli(&["start", "posture", "start_time=-30"], &db).unwrap();
    let e = only(&db, "posture");
    assert_eq!(occur_utc(&e), "2026-09-10T11:30:00+00:00");
    assert_eq!(dur(&e), None, "open block: no duration");
    assert_eq!(e.state, Some(json!(true)));
    assert_eq!(e.data, None);
}

/// `end_time=E` alone on `add` / `occur` is a point at E.
#[test]
fn end_time_alone_on_add_and_occur_is_a_point_there() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let db = fresh_db();
    run_cli(&["add", "thing", "--id", "k", "end_time=-5"], &db).expect("add");
    run_cli(&["occur", "ping", "end_time=-5"], &db).expect("occur");
    for field in ["thing", "ping"] {
        let e = only(&db, field);
        assert_eq!(occur_utc(&e), "2026-09-10T11:55:00+00:00", "{field}");
        assert!(
            e.dur
                .as_ref()
                .is_none_or(|d| d.to_storage_string() == "MOMENT"),
            "{field}: a point, got dur {:?}",
            dur(&e)
        );
        assert_eq!(e.data, None, "{field}");
    }
}

/// `start_time=S` with a duration on `occur` / `add` (`dur=`, since those
/// verbs have no trailing-duration slot): the block S..S+dur.
#[test]
fn start_time_with_dur_key_on_occur_and_add_ends_at_start_plus_dur() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let db = fresh_db();
    run_cli(&["occur", "ev", "start_time=-60", "dur=15"], &db).expect("occur");
    run_cli(
        &["add", "thing", "--id", "j", "start_time=-60", "dur=15"],
        &db,
    )
    .expect("add");
    for field in ["ev", "thing"] {
        let e = only(&db, field);
        assert_eq!(occur_utc(&e), "2026-09-10T11:15:00+00:00", "{field}");
        assert_eq!(dur(&e).as_deref(), Some("PT15M"), "{field}");
    }
}

#[test]
fn reversed_pair_is_an_error_not_a_negative_duration() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let db = fresh_db();
    let err = run_cli(&["start", "posture", "start_time=-5", "end_time=-20"], &db)
        .expect_err("end before start");
    assert!(err.to_string().contains("before"), "got: {err}");
    assert!(rows(&db, "posture").is_empty(), "nothing written");
}

#[test]
fn a_pair_plus_a_duration_is_an_error() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let db = fresh_db();
    let err = run_cli(
        &["start", "posture", "start_time=-20", "end_time=-5", "10"],
        &db,
    )
    .expect_err("three specs of two degrees of freedom");
    assert!(err.to_string().contains("duration"), "got: {err}");
}

/// `-t` and a key both set the time; the error names both.
#[test]
fn t_flag_with_a_key_is_an_error_naming_both() {
    let db = fresh_db();
    let err = run_cli(&["start", "posture", "-t", "16:00", "start_time=4pm"], &db)
        .expect_err("-t and start_time=");
    let msg = err.to_string();
    assert!(
        msg.contains("-t") && msg.contains("start_time="),
        "got: {msg}"
    );

    let err = run_cli(&["start", "posture", "-T", "end_time=-5"], &db).expect_err("-T and a key");
    assert!(err.to_string().contains("end_time="), "got: {err}");
}

/// `switch mood STATE end_time=E` is `end mood -t E -L=STATE`: it closes
/// the block at E and asserts that STATE is what was open. The row records
/// the actual prior state as `last_state` and the asserted one on the
/// entry; when they disagree, `datum check` reports `last_state_mismatch`.
#[test]
fn end_time_alone_on_switch_is_end_asserting_the_state() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let keyed = fresh_db();
    run_cli(&["switch", "mood", "happy", "-t", "11:30"], &keyed).unwrap();
    run_cli(&["switch", "mood", "happy", "end_time=-5"], &keyed).expect("switch end_time=");

    let flagged = fresh_db();
    run_cli(&["switch", "mood", "happy", "-t", "11:30"], &flagged).unwrap();
    run_cli(&["end", "mood", "-t", "-5", "-L=happy"], &flagged).expect("end -L=happy");

    let want = shape(&rows(&flagged, "mood")[1]);
    assert_eq!(want["occur_time"], json!("2026-09-10T11:55:00+00:00"));
    assert_eq!(want["state"], json!(false));
    assert_eq!(want["data"], json!({"last_state": "happy"}));
    assert_eq!(want["user_asserted_last_state"], json!("happy"));
    assert_eq!(shape(&rows(&keyed, "mood")[1]), want);

    // The assertion is checked: `sad` was not what was open.
    let wrong = fresh_db();
    run_cli(&["switch", "mood", "happy", "-t", "11:30"], &wrong).unwrap();
    run_cli(&["switch", "mood", "sad", "end_time=-5"], &wrong).expect("still lands");
    run_cli(&["check"], &wrong).expect("check runs");
    let mismatches = wrong
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::LastStateMismatch),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(mismatches.len(), 1, "one last_state_mismatch");
    let row = &mismatches[0].local_entry;
    assert_eq!(row.user_asserted_last_state, Some(json!("sad")));
    assert_eq!(occur_utc(row), "2026-09-10T11:55:00+00:00");
}

/// The drawing agrees with the row (the da-zwct test shape): after the
/// pair, the block ends on the now-line less five minutes and lasted
/// fifteen. The now-line is the render's own reading of the clock, which a
/// frozen clock reaches through the same `clock::now()` as the wall clock.
///
/// Near midnight the block straddles two days (da-tyggo): at 00:03 it lies
/// wholly in yesterday, at 00:10 it is cut in two. So two days are drawn
/// and the block's slices summed across them — the wall clock left this
/// test red for the first twenty minutes of every day, reading today alone.
#[test]
fn dayview_draws_the_pair_ending_five_minutes_before_now() {
    for now in [
        "2026-09-10T00:03:00",
        "2026-09-10T00:10:00",
        "2026-09-10T12:00:00",
    ] {
        let _g = freeze_time(now);
        let db = fresh_db();
        run_cli(&["start", "posture", "start_time=-20", "end_time=-5"], &db).unwrap();

        let tmp = tempfile::NamedTempFile::with_suffix(".json").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        run_cli(
            &[
                "dayview", "-n", "2", "--width", "2400", "--json", "-o", &path,
            ],
            &db,
        )
        .expect("dayview --json");
        let json: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

        // Minutes since yesterday's midnight, from a band's pixels.
        let minute = json["time_axis_extent"].as_f64().expect("time_axis_extent") / (24.0 * 60.0);
        let at = |day: usize, px: f64| day as f64 * 24.0 * 60.0 + px / minute;
        let days = json["days"].as_array().unwrap();
        assert_eq!(days.len(), 2, "{now}: yesterday and today");
        let now_line = at(
            1,
            days[1]["now_time_pos"].as_f64().expect("today's now-line"),
        );
        let mut slices = vec![];
        for (i, day) in days.iter().enumerate() {
            let fields = day["fields"].as_array().unwrap();
            for f in fields.iter().filter(|f| f["field"] == "posture") {
                for b in f["blocks"].as_array().unwrap() {
                    let offset = b["time_offset"].as_f64().unwrap();
                    let extent = b["time_extent"].as_f64().unwrap();
                    slices.push((at(i, offset), at(i, offset + extent)));
                }
            }
        }

        let end = slices.last().expect("posture rendered").1;
        let lasted: f64 = slices.iter().map(|(s, e)| e - s).sum();
        assert!(
            (end - (now_line - 5.0)).abs() < 1e-6,
            "{now}: the block ends five minutes before the now-line: {slices:?}, now-line {now_line}"
        );
        assert!(
            (lasted - 15.0).abs() < 1e-6,
            "{now}: and lasted fifteen minutes: {slices:?}"
        );
    }
}

/// Relative and absolute forms through the key parse exactly as through
/// `-t`: same instant, same uncertainty. One parser.
#[test]
fn key_values_parse_as_t_does() {
    let _g = freeze_time("2026-09-10T12:00:00");
    for raw in [
        "-20",
        "4pm",
        "16:15",
        "2026-09-10T16:15",
        "-20±5m",
        "yesterday",
    ] {
        let db = fresh_db();
        let key = format!("start_time={raw}");
        run_cli(&["start", "keyed", &key], &db).unwrap_or_else(|e| panic!("{key}: {e}"));
        run_cli(&["start", "flagged", "-t", raw], &db).unwrap_or_else(|e| panic!("-t {raw}: {e}"));
        let keyed = only(&db, "keyed");
        let flagged = only(&db, "flagged");
        assert_eq!(occur_utc(&keyed), occur_utc(&flagged), "{raw}");
        assert_eq!(
            keyed.occur_uncertainty(),
            flagged.occur_uncertainty(),
            "{raw}"
        );
        assert_eq!(dur(&keyed), dur(&flagged), "{raw}");
    }
}

/// Only the two lifted keys leave `data`. Any other `*time` key keeps the
/// suffix typing it always had and stays in the payload.
#[test]
fn other_time_keys_stay_in_data() {
    let _g = freeze_time("2026-09-10T12:00:00Z");
    let db = fresh_db();
    run_cli(&["occur", "sleep", "wake_time=7am"], &db).unwrap();
    let e = only(&db, "sleep");
    assert_eq!(occur_utc(&e), "2026-09-10T12:00:00+00:00", "logged now");
    assert_eq!(e.data, Some(json!({"wake_time": "07:00:00"})));
}
