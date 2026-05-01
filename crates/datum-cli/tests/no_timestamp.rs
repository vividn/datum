//! Tests for `-T/--no-timestamp` unified semantics (S5 / da-u4k.5).
//!
//! Any log verb (`occur`, `start`, `stop`/`end`, `switch`, `add`) + `-T`
//! produces a timeless, add-equivalent entry: `start_time` and `end_time`
//! are both None, `verb` reads back as `add`. Time-specifying flags mixed
//! with `-T` error out.

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use datum_core::{assert_count, assert_entry_matches};

// ─── Happy path: each log verb + -T yields a timeless add-shaped entry ───────

#[test]
fn occur_minus_t_is_timeless_add() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "-T"], &db).expect("occur -T");
    assert_count!(db, "coffee", 1);
    let e = &db.entries_for_field("coffee", None).unwrap()[0];
    assert_entry_matches!(e, field: "coffee", verb: "add");
    assert!(e.occur_time.is_none(), "occur -T must not set start_time");
    assert!(e.dur.is_none(), "occur -T must not set end_time");
}

#[test]
fn start_minus_t_is_timeless_add() {
    let db = fresh_db();
    run_cli(&["start", "sleep", "-T"], &db).expect("start -T");
    let e = &db.entries_for_field("sleep", None).unwrap()[0];
    assert_entry_matches!(e, verb: "add");
    assert!(e.occur_time.is_none());
    assert!(e.dur.is_none());
}

#[test]
fn stop_minus_t_is_timeless_add() {
    let db = fresh_db();
    run_cli(&["stop", "task", "-T"], &db).expect("stop -T");
    let e = &db.entries_for_field("task", None).unwrap()[0];
    assert_entry_matches!(e, verb: "add");
    assert!(e.occur_time.is_none());
    assert!(e.dur.is_none());
}

#[test]
fn end_minus_t_is_timeless_add() {
    let db = fresh_db();
    run_cli(&["end", "task", "-T"], &db).expect("end -T");
    let e = &db.entries_for_field("task", None).unwrap()[0];
    assert_entry_matches!(e, verb: "add");
    assert!(e.occur_time.is_none());
    assert!(e.dur.is_none());
}

#[test]
fn switch_minus_t_is_timeless_add_preserves_value() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy", "-T"], &db).expect("switch -T");
    let e = &db.entries_for_field("mood", None).unwrap()[0];
    // The primary value ("happy") must still land on the entry — `-T` only
    // suppresses the timestamp, not the payload. Verb rides back as `add`.
    assert_entry_matches!(e, field: "mood", verb: "add", value: "happy");
    assert!(e.occur_time.is_none());
    assert!(e.dur.is_none());
}

#[test]
fn add_minus_t_is_timeless() {
    let db = fresh_db();
    run_cli(&["add", "note", "-T"], &db).expect("add -T");
    let e = &db.entries_for_field("note", None).unwrap()[0];
    assert_entry_matches!(e, verb: "add");
    assert!(e.occur_time.is_none());
    assert!(e.dur.is_none());
}

// ─── Payload preservation: -T still routes through the normal kv parsing ─────

#[test]
fn minus_t_keeps_extra_data_kv_pairs() {
    let db = fresh_db();
    run_cli(&["occur", "word", "-T", "pos=verb", "srcLang=pt"], &db)
        .expect("occur word -T pos=verb srcLang=pt");
    let e = &db.entries_for_field("word", None).unwrap()[0];
    let data = e.data.as_ref().expect("kv pairs land in extra_data");
    assert_eq!(data.get("pos"), Some(&serde_json::json!("verb")));
    assert_eq!(data.get("srcLang"), Some(&serde_json::json!("pt")));
    assert!(e.occur_time.is_none());
}

// ─── Rejection: -T plus any time flag is a hard error ────────────────────────

#[test]
fn minus_t_with_t_flag_errors() {
    let db = fresh_db();
    let err = run_cli(&["occur", "coffee", "-T", "-t", "15:00"], &db)
        .expect_err("-T + -t must reject");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("no-timestamp") && msg.contains("-t"),
        "error must name both flags; got: {msg}"
    );
}

#[test]
fn minus_t_with_d_flag_errors() {
    let db = fresh_db();
    // Use `-d=-1` form to dodge `-1`-as-value-for-other-arg parsing.
    let err = run_cli(&["occur", "coffee", "-T", "-d=-1"], &db)
        .expect_err("-T + -d must reject");
    assert!(format!("{err:#}").contains("-d"), "got: {err:#}");
}

#[test]
fn minus_t_with_dur_errors() {
    let db = fresh_db();
    let err = run_cli(&["start", "sleep", "-T", "--dur", "30m"], &db)
        .expect_err("-T + --dur must reject");
    assert!(format!("{err:#}").contains("--dur"));
}

#[test]
fn minus_t_with_y_flag_errors() {
    let db = fresh_db();
    let err = run_cli(&["occur", "coffee", "-T", "-y"], &db)
        .expect_err("-T + -y must reject");
    assert!(format!("{err:#}").contains("-y"));
}

#[test]
fn minus_t_with_q_flag_errors() {
    let db = fresh_db();
    let err = run_cli(&["occur", "coffee", "-T", "-q"], &db)
        .expect_err("-T + -q must reject");
    assert!(format!("{err:#}").contains("-q"));
}

#[test]
fn minus_t_with_z_flag_errors() {
    let db = fresh_db();
    let err = run_cli(&["occur", "coffee", "-T", "-z", "UTC"], &db)
        .expect_err("-T + -z must reject");
    assert!(format!("{err:#}").contains("-z"));
}

// ─── Non-regression: plain log verbs (no -T) still get timestamps ────────────

#[test]
fn without_minus_t_occur_still_timestamps() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("plain occur");
    let e = &db.entries_for_field("coffee", None).unwrap()[0];
    assert!(
        e.occur_time.is_some(),
        "plain `occur` must still stamp start_time"
    );
}
