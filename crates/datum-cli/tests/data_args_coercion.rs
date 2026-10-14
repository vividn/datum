//! Suffix-coercion + `handle_data_args` tests (da-6iy).
//!
//! A `*_time` value is an instant: parsed as `-t` parses its value, from
//! the entry's time, and stored as v2 stored it, `{utc, o, tz}` (da-x15of).

use crate::common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use datum_parse::time::DatumTime;
use serde_json::{json, Value};

/// The `*_time` values of one entry logged at 2024-01-15 09:00 in Berlin.
fn time_keys(kvs: &[&str]) -> Value {
    let db = fresh_db();
    let mut argv = vec![
        "occur",
        "morning",
        "-t",
        "2024-01-15T09:00",
        "-z",
        "Europe/Berlin",
    ];
    argv.extend(kvs);
    run_cli(&argv, &db).expect("occur with *_time keys");
    let entry = &datum_core::test_util::logged_entries(&db, "morning")[0];
    entry.data.clone().expect("data populated")
}

/// The wall clock a stored `*_time` value names, in its own zone.
fn local(v: &Value) -> String {
    let t = DatumTime::from_v2_json(v).unwrap_or_else(|| panic!("not an instant: {v}"));
    t.local_time().naive_local().to_string()
}

/// v2 at the pin (f5e7c1b8), Europe/Berlin, stored these for an entry at
/// 2024-01-15 09:00: `c_time=2024-01-15T07:15` as {utc
/// "2024-01-15T06:15:00.000Z", o 1, tz "Europe/Berlin"}, and a clock time,
/// an offset or a day word the same shape. v2 read those three against the
/// wall clock; v3 reads them against the entry (da-rp2rq), so `8am` is the
/// entry's 8am. `sunday` is the nearest Sunday's midnight, as on `-t`.
#[test]
fn a_time_key_stores_v2s_instant() {
    let berlin = |utc: &str| json!({"utc": utc, "o": 1, "tz": "Europe/Berlin"});
    let data = time_keys(&[
        "wake_time=8am",
        "wakeTime=8am",
        "bed_time=15:13",
        "next_time=+3h",
        "finish_time=sunday",
        "c_time=2024-01-15T07:15",
        "z_time=2024-01-15T07:15:00Z",
    ]);
    assert_eq!(data["wake_time"], berlin("2024-01-15T07:00:00.000Z"));
    assert_eq!(data["wakeTime"], berlin("2024-01-15T07:00:00.000Z"));
    assert_eq!(data["bed_time"], berlin("2024-01-15T14:13:00.000Z"));
    assert_eq!(data["next_time"], berlin("2024-01-15T11:00:00.000Z"));
    assert_eq!(data["finish_time"], berlin("2024-01-13T23:00:00.000Z"));
    assert_eq!(data["c_time"], berlin("2024-01-15T06:15:00.000Z"));
    // The instant in the literal's own zone, as `-t` keeps it and as v2
    // stored it: `Z` is UTC (da-kvosn).
    assert_eq!(
        data["z_time"],
        json!({"utc": "2024-01-15T07:15:00.000Z", "o": 0, "tz": "UTC"})
    );
}

/// The bug: `next_time=+16h` at 20:00 is tomorrow noon, and its day is
/// kept — a clock time stored `12:00:00` and lost it at log time.
#[test]
fn a_relative_time_across_midnight_keeps_its_day() {
    let db = fresh_db();
    run_cli(
        &[
            "occur",
            "plants",
            "-t",
            "2024-01-15T20:00",
            "next_time=+16h",
        ],
        &db,
    )
    .unwrap();
    let entry = &datum_core::test_util::logged_entries(&db, "plants")[0];
    let data = entry.data.as_ref().expect("data populated");
    assert_eq!(local(&data["next_time"]), "2024-01-16 12:00:00");
}

/// `weight=75.2kg` is the string typed, as the switch state and as a
/// data value alike — there is no number+unit inference (da-jh7io). Uses
/// `switch` so the bare `75.2kg` positional is peeled into `state` (occur
/// is stateless per da-k5o6 and rejects bare positionals).
#[test]
fn digits_then_letters_stay_strings_through_handle_data_args() {
    let db = fresh_db();
    run_cli(&["switch", "weight", "75.2kg", "src=5pm", "n=007"], &db)
        .expect("switch weight 75.2kg src=5pm n=007");

    let entry = &datum_core::test_util::logged_entries(&db, "weight")[0];
    assert_eq!(entry.state, Some(json!("75.2kg")));
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["src"], json!("5pm"));
    // datum's `inferType`: `007` is not strict JSON, so it stays a string.
    assert_eq!(extra["n"], json!("007"));
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

    let entry = &datum_core::test_util::logged_entries(&db, "note")[0];
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
        &[
            "occur", "log", "-R", "notes", "-S", "this", "is", "a", "long", "note",
        ],
        &db,
    )
    .expect("occur log -R notes -S this is a long note");

    let entry = &datum_core::test_util::logged_entries(&db, "log")[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["notes"], json!("this is a long note"));
}

/// `-C <words…>` (da-4t5f): shorthand equivalent to `-SR comment` —
/// remainder positionals join into a single string and land in
/// `extra_data.comment`. Bead-spec example: `datum log -C ate too many tacos`
/// → comment=['ate too many tacos'] (an array since da-ult68).
#[test]
fn dash_c_joins_remainder_into_comment_string() {
    let db = fresh_db();
    run_cli(&["occur", "log", "-C", "ate", "too", "many", "tacos"], &db)
        .expect("occur log -C ate too many tacos");

    let entry = &datum_core::test_util::logged_entries(&db, "log")[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["comment"], json!(["ate too many tacos"]));
}

/// `-SR <bucket> <words…>` (da-gq3x): the bundled short form for
/// `-S -R` must split in the pre-pass so `-R` claims the bucket name
/// and the trailing positionals join into a single string. Regression
/// of da-4t5f's "rest of args → single string" behaviour.
#[test]
fn dash_sr_bundle_joins_remainder_into_named_bucket() {
    let db = fresh_db();
    run_cli(
        &[
            "occur", "log", "-SR", "notes", "all", "the", "rest", "is", "one", "string",
        ],
        &db,
    )
    .expect("occur log -SR notes all the rest is one string");

    let entry = &datum_core::test_util::logged_entries(&db, "log")[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["notes"], json!("all the rest is one string"));
}

/// `-S` without `-R`/`-C` (da-tcr8): "string the rest" — the user
/// asked to stringify the remainder, so "the rest" defaults to the
/// `extra_data` bucket (joined into one string) rather than erroring. This
/// supersedes the da-k5o6 rejection.
/// Explicit `-R <field>` / `-C` still override the destination.
#[test]
fn dash_s_alone_on_occur_infers_extra_data() {
    let db = fresh_db();
    run_cli(&["occur", "log", "-S", "a", "long", "phrase"], &db)
        .expect("occur -S alone should land in extra_data");

    let entry = &datum_core::test_util::logged_entries(&db, "log")[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(
        extra["extra_data"],
        json!("a long phrase"),
        "-S alone joins the remainder into the extra_data bucket",
    );
}

/// `-C` composes with prior `-c` flags: `-c` populates the comment first,
/// then `-C`'s joined remainder appends, producing an array — the same
/// shape datum's `comment_remainder_appends_to_existing_comments` test
/// pins at the data_args layer.
#[test]
fn dash_c_appends_to_explicit_comment_flag() {
    let db = fresh_db();
    run_cli(
        &[
            "occur",
            "log",
            "-c",
            "explicit comment",
            "-C",
            "remainder",
            "words",
        ],
        &db,
    )
    .expect("occur log -c explicit -C remainder words");

    let entry = &datum_core::test_util::logged_entries(&db, "log")[0];
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
        &[
            "occur",
            "log",
            "-c",
            "first comment",
            "-c",
            "second comment",
        ],
        &db,
    )
    .expect("occur with two -c flags");

    let entry = &datum_core::test_util::logged_entries(&db, "log")[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["comment"], json!(["first comment", "second comment"]));
}

#[test]
fn single_comment_is_still_an_array() {
    let db = fresh_db();
    run_cli(&["occur", "log", "-c", "lone comment"], &db).expect("occur with one -c");

    let entry = &datum_core::test_util::logged_entries(&db, "log")[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["comment"], json!(["lone comment"]));
}

/// Durations and time flags have their own grammar, untouched by the
/// removal of number+unit inference (da-jh7io): `-t +30s` is thirty
/// seconds ahead, `-q` five minutes ago, and `dur=30m` half an hour.
#[test]
fn durations_and_time_flags_keep_their_units() {
    let db = fresh_db();
    let before = chrono::Utc::now();
    run_cli(&["occur", "ahead", "-t", "+30s"], &db).expect("occur ahead -t +30s");
    run_cli(&["occur", "back", "-q"], &db).expect("occur back -q");
    run_cli(&["start", "half", "dur=30m"], &db).expect("start half dur=30m");
    let after = chrono::Utc::now();

    let at = |field: &str| {
        datum_core::test_util::logged_entries(&db, field)[0]
            .occur_time
            .as_ref()
            .unwrap()
            .utc
    };
    let ahead = at("ahead") - chrono::Duration::seconds(30);
    assert!(before <= ahead && ahead <= after, "-t +30s: {ahead}");
    let back = at("back") + chrono::Duration::minutes(5);
    assert!(before <= back && back <= after, "-q: {back}");

    let half = &datum_core::test_util::logged_entries(&db, "half")[0];
    assert_eq!(half.duration_seconds(), Some(30 * 60));
}

/// da-rp2rq: a timeless entry (`-T`, `note`) anchors relative `*date` /
/// `*time` values at its create time in the local zone — the chore-shirk
/// `occur chore -T next_date=+7` — as a timed entry anchors at its occur
/// time. An update anchors at the entry's own time as its log line did
/// (da-60n1b).
#[test]
fn relative_temporal_keys_anchor_a_timeless_entry_at_its_create_time() {
    let _g = datum_core::test_util::freeze_time("2026-09-10T12:00:00");
    let db = fresh_db();
    let data = |field: &str| {
        datum_core::test_util::logged_entries(&db, field)[0]
            .data
            .clone()
    };

    run_cli(
        &["occur", "vacuum", "-T", "next_date=+7", "next_time=+2h"],
        &db,
    )
    .unwrap();
    let vacuum = data("vacuum").unwrap();
    assert_eq!(vacuum["next_date"], json!("2026-09-17"));
    assert_eq!(local(&vacuum["next_time"]), "2026-09-10 14:00:00");
    // `-D` is not refused beside `-T`; it must not move the anchor to midnight.
    run_cli(&["occur", "mop", "-T", "-D", "next_time=+2h"], &db).unwrap();
    assert_eq!(
        local(&data("mop").unwrap()["next_time"]),
        "2026-09-10 14:00:00"
    );
    run_cli(&["note", "chore", "next_date=+3"], &db).unwrap();
    assert_eq!(data("chore"), Some(json!({"next_date": "2026-09-13"})));

    run_cli(&["occur", "dishes", "-t", "-1d", "next_date=+1"], &db).unwrap();
    assert_eq!(
        data("dishes"),
        Some(json!({"next_date": "2026-09-10"})),
        "a timed entry anchors at its occur time"
    );

    let hid = datum_core::test_util::logged_entries(&db, "dishes")[0]
        .hid
        .to_string();
    run_cli(&["update", &hid, "next_date=+7"], &db).unwrap();
    assert_eq!(
        data("dishes"),
        Some(json!({"next_date": "2026-09-16"})),
        "an update anchors at the entry's occur time"
    );
}

/// da-bcdjm: `occur_time=` places the entry, so relative `*date` / `*time`
/// keys anchor at it, as they do at `-t` — and the key itself is resolved
/// once, not again at the anchor it sets.
#[test]
fn relative_temporal_keys_anchor_at_an_occur_time_key() {
    let _g = datum_core::test_util::freeze_time("2026-09-10T12:00:00");
    let db = fresh_db();
    let entry = |field: &str| datum_core::test_util::logged_entries(&db, field)[0].clone();
    let occur = |field: &str| {
        let t = entry(field).occur_time.unwrap();
        t.local_time().naive_local().to_string()
    };

    run_cli(
        &["occur", "a1", "occur_time=2026-01-01T10:00", "next_date=+1"],
        &db,
    )
    .unwrap();
    assert_eq!(occur("a1"), "2026-01-01 10:00:00");
    assert_eq!(entry("a1").data, Some(json!({"next_date": "2026-01-02"})));

    run_cli(&["occur", "a2", "occur_time=+2h", "next_time=+30m"], &db).unwrap();
    assert_eq!(occur("a2"), "2026-09-10 14:00:00");
    let a2 = entry("a2").data.unwrap();
    assert_eq!(local(&a2["next_time"]), "2026-09-10 14:30:00");
}

/// da-fwsre: a glued comma makes a list and drops one trailing comma, a
/// comma before a space or ending a word is text, and `_` separates digits.
#[test]
fn glued_comma_lists_and_comma_space_text() {
    let db = fresh_db();
    run_cli(
        &[
            "occur",
            "x",
            "v=a,b",
            "note=met Anna, Ben",
            "w=Anna,",
            "one=,Anna",
            "l=a,b,",
            "c=,,",
            "-k",
            "q",
            r#"",,""#,
            "n=1_000",
        ],
        &db,
    )
    .expect("occur x with comma values");

    let entry = &datum_core::test_util::logged_entries(&db, "x")[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["v"], json!(["a", "b"]));
    assert_eq!(extra["note"], json!("met Anna, Ben"));
    assert_eq!(extra["w"], json!("Anna,"));
    assert_eq!(extra["one"], json!(["Anna"]));
    assert_eq!(extra["l"], json!(["a", "b"]));
    // Nothing but commas is text; as a bare word `,,` is the sides token,
    // so a positional one is JSON-quoted (vividn, 17:22).
    assert_eq!(extra["c"], json!(",,"));
    assert_eq!(extra["q"], json!(",,"));
    assert_eq!(extra["n"], json!(1000));
}

/// da-qzfnb: a single-quoted value means what the double-quoted one does,
/// as v2's relaxed-json read it — vividn's `-k key "',,'"` stores «,,».
#[test]
fn single_quoted_values_are_strings() {
    let db = fresh_db();
    run_cli(
        &[
            "occur",
            "x",
            "-k",
            "key",
            "',,'",
            "v='a,b'",
            "n='5'",
            "l=['a', 'b']",
            "u='unbalanced",
        ],
        &db,
    )
    .expect("occur x with single-quoted values");

    let entry = &datum_core::test_util::logged_entries(&db, "x")[0];
    let extra = entry.data.as_ref().expect("extra_data populated");
    assert_eq!(extra["key"], json!(",,"));
    assert_eq!(extra["v"], json!("a,b"));
    assert_eq!(extra["n"], json!("5"));
    assert_eq!(extra["l"], json!(["a", "b"]));
    assert_eq!(extra["u"], json!("'unbalanced"));
}
