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

/// `-R <field>` (da-4t5f): un-keyed positionals route to `extra_data.<field>`
/// instead of the primary `value` column. Without `-S` they accumulate as
/// an array (handle_data_args's append semantics), matching datum's
/// remainder-bucket behaviour. The entry's stored state is the Action's
/// canonical default (Action::Occur with no positional → `state = true`,
/// per derive_verb's null→true promotion).
#[test]
fn dash_r_routes_remainder_to_named_field() {
    let db = fresh_db();
    run_cli(&["occur", "note", "-R", "extra", "one", "two"], &db)
        .expect("occur note -R extra one two");

    let entry = &db.entries_for_field("note", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["extra"], json!(["one", "two"]));
}

/// `-R <field> -S <words…>` (da-4t5f): joins remainder positionals into
/// a single space-separated string at `extra_data.<field>`. This is the
/// canonical datum-parity behaviour — the bead-spec example was
/// `datum log -R notes -S this is a long note` → notes='this is a long note'.
#[test]
fn dash_r_dash_s_joins_remainder_into_string_at_field() {
    let db = fresh_db();
    run_cli(
        &["occur", "log", "-R", "notes", "-S", "this", "is", "a", "long", "note"],
        &db,
    )
    .expect("occur log -R notes -S this is a long note");

    let entry = &db.entries_for_field("log", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["notes"], json!("this is a long note"));
}

/// `-C <words…>` (da-4t5f): shorthand equivalent to `-SR comment` —
/// remainder positionals join into a single string and land in
/// `extra_data.comment`. Bead-spec example: `datum log -C ate too many tacos`
/// → comment='ate too many tacos'.
#[test]
fn dash_c_joins_remainder_into_comment_string() {
    let db = fresh_db();
    run_cli(
        &["occur", "log", "-C", "ate", "too", "many", "tacos"],
        &db,
    )
    .expect("occur log -C ate too many tacos");

    let entry = &db.entries_for_field("log", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["comment"], json!("ate too many tacos"));
}

/// `-S` without `-R`/`-C` (da-4t5f): joins all un-keyed positionals into
/// a single space-separated string at the primary value column. Falls out
/// of routing the join through daydumb's PRIMARY_VALUE_KEY sentinel.
#[test]
fn dash_s_alone_joins_remainder_into_primary_value_string() {
    let db = fresh_db();
    run_cli(
        &["occur", "log", "-S", "a", "long", "phrase"],
        &db,
    )
    .expect("occur log -S a long phrase");

    let entry = &db.entries_for_field("log", None).unwrap()[0];
    assert_eq!(entry.value, Some(json!("a long phrase")));
}

/// `-C` composes with prior `-c` flags: `-c` populates the comment first,
/// then `-C`'s joined remainder appends, producing an array — the same
/// shape datum's `comment_remainder_appends_to_existing_comments` test
/// pins at the data_args layer.
#[test]
fn dash_c_appends_to_explicit_comment_flag() {
    let db = fresh_db();
    run_cli(
        &["occur", "log", "-c", "explicit comment", "-C", "remainder", "words"],
        &db,
    )
    .expect("occur log -c explicit -C remainder words");

    let entry = &db.entries_for_field("log", None).unwrap()[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(
        extra["comment"],
        json!(["explicit comment", "remainder words"])
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
