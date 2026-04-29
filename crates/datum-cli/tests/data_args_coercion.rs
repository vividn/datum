//! Suffix-coercion + `handle_data_args` parity tests for da-6iy.
//!
//! Verifies that the orphaned `_time` / `_date` / `_dur` / `_duration`
//! coercion logic in `datum_parse::input::data_args` actually runs through
//! the command flow now that `cmd::parse_log_args` delegates to
//! `handle_data_args`. Pre-da-6iy, `wake_time=8am` stored the literal
//! string `"8am"`; post-da-6iy it should resolve to `"08:00:00"`.

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use serde_json::json;

/// `wake_time=8am` — `_time` suffix coerces a clock-time string to ISO
/// `HH:MM:SS`. The pre-da-6iy code path stored the raw token verbatim.
#[test]
fn time_suffix_coerces_clock_string() {
    let db = fresh_db();
    run_cli(&["occur", "morning", "wake_time=8am"], &db).expect("occur with wake_time=8am");

    let entry = &db.entries_for_field("morning", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(
        extra["wake_time"],
        json!("08:00:00"),
        "_time suffix must coerce `8am` → `08:00:00`",
    );
}

/// `wakeTime=8am` — camelCase variant: same coercion applies via the
/// uppercase-letter branch of `matches_suffix`.
#[test]
fn time_suffix_camel_case_coerces() {
    let db = fresh_db();
    run_cli(&["occur", "morning", "wakeTime=8am"], &db).expect("occur with wakeTime=8am");

    let entry = &db.entries_for_field("morning", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["wakeTime"], json!("08:00:00"));
}

/// `bed_time=15:13` — HH:MM input format also resolves through the
/// time parser to `HH:MM:SS`.
#[test]
fn time_suffix_handles_hhmm_input() {
    let db = fresh_db();
    run_cli(&["occur", "evening", "bed_time=15:13"], &db).expect("occur with bed_time=15:13");

    let entry = &db.entries_for_field("evening", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["bed_time"], json!("15:13:00"));
}

/// `weight=75.2kg` — unit-annotation form lands as `{"v": 75.2, "u": "kg"}`
/// even when routed through `handle_data_args`. The unit-annot detector
/// is a daydumb extension over datum's `inferType`.
#[test]
fn unit_annotation_survives_handle_data_args() {
    let db = fresh_db();
    run_cli(&["occur", "weight", "75.2kg", "src=morning"], &db)
        .expect("occur weight 75.2kg src=morning");

    let entry = &db.entries_for_field("weight", None).unwrap()[0];
    assert_eq!(
        entry.value,
        Some(json!({"v": 75.2, "u": "kg"})),
        "unit-annotated primary value preserved",
    );
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["src"], json!("morning"));
}

/// `-R/-S/-l/-C` audit (bead da-6iy scope #5): the LogArgs parser does
/// not lift `data_args::FLAG_SPECS` (per `parse.rs::log_args_registry`,
/// which tracks slug-keys / comments separately). Wiring `handle_data_args`
/// in does not on its own promote `-R`/`-S`/`-l`/`-C` into log syntax —
/// the LogArgs flag registry would need new entries for that. Today the
/// daydumb default behaviour is fixed: un-keyed positionals fold into the
/// primary `value` column via the sentinel remainder bucket, and any
/// `-R` / `-S` / `-l` / `-C` token would be rejected as an unknown flag.
/// Surfacing them is tracked separately; this test pins the current state.
#[test]
fn dash_r_remainder_flag_not_surfaced_on_log_args_today() {
    let db = fresh_db();
    let err = run_cli(
        &["occur", "note", "-R", "extra", "tail"],
        &db,
    )
    .expect_err("-R must error until LogArgs lifts data_args::FLAG_SPECS");
    let s = err.to_string();
    assert!(
        s.contains("-R") || s.contains("Unknown") || s.contains("unknown"),
        "expected an unknown-flag error for -R, got: {s}",
    );
}

/// Repeated `-c` flags compose into an array — single `-c` stays scalar.
/// The shape mirrors datum's `handleDataArgs` comment-shaping rule, now
/// that LogArgs.comment is a `Vec<String>` rather than `Option<String>`.
#[test]
fn multiple_comments_become_array() {
    let db = fresh_db();
    run_cli(
        &["occur", "log", "-c", "first comment", "-c", "second comment"],
        &db,
    )
    .expect("occur with two -c flags");

    let entry = &db.entries_for_field("log", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["comment"], json!(["first comment", "second comment"]));
}

#[test]
fn single_comment_stays_scalar() {
    let db = fresh_db();
    run_cli(&["occur", "log", "-c", "lone comment"], &db).expect("occur with one -c");

    let entry = &db.entries_for_field("log", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["comment"], json!("lone comment"));
}
