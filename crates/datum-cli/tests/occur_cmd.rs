//! Parity tests for `datum occur` (P2 / da-vgma).
//!
//! Ports the 9 cases from `occurCmd.test.ts` in the original datum project,
//! adapted to daydumb's flag-based model. Each Rust test maps 1-to-1 to a TS
//! case; headers identify the source item.
//!
//! Divergences from datum:
//!   * daydumb's `-z`/`--timezone` flag is parsed but not yet plumbed into
//!     `build_time` — the cases that verified explicit offsets (`-z 0`,
//!     `-z +3`, `-z Europe/Berlin`) are adapted to use local-zone interpretation.
//!   * `--no-metadata`/`-M` ships with P1 (da-wsax) — the TS case is ported as
//!     a structural skip that documents the future wiring target.
//!   * `-D`/`--full-day` is parsed (da-rsj4); the date-only behavior itself
//!     is plumbed downstream in da-fc0l.2/.3/.4. The TS case is ported as
//!     a flag-acceptance check plus the current `-d DATE` analog.
//!   * Datum's `%fieldStructure` interpolated the stored `field` itself;
//!     daydumb lacks that but does template the slug via `--id`, so the
//!     "composite field" TS case maps to slug interpolation via `--id`.

mod common;

use chrono::{Duration, Utc};
use common::run_cli;
use datum_core::test_util::fresh_db;
use datum_core::{assert_count, assert_entry_matches};
use serde_json::json;

/// TS #1: "creates a document with an occurTime"
///
/// `occur field` creates exactly one entry with `time_start ≈ now` and no
/// `time_end` (point-in-time event). The slug is `field:<minute-truncated ISO>`
/// — daydumb's build_slug normalizes to minute precision, so the exact
/// millisecond form from datum does not apply, but the field-prefix contract
/// does.
#[test]
fn occur_creates_entry_with_time_start() {
    let db = fresh_db();

    let before = Utc::now();
    run_cli(&["occur", "field"], &db).expect("occur field");
    let after = Utc::now();

    assert_count!(db, "field", 1);
    let entry = &db.entries_for_field("field", None).unwrap()[0];
    // da-ju71.2: occur with no explicit dur emits Dur::Moment, locking
    // the verb to Point regardless of state shape.
    assert_entry_matches!(entry, field: "field", verb: "point");

    let ts = entry
        .occur_time
        .as_ref()
        .expect("occur sets occur_time")
        .utc;
    assert!(
        ts >= before - Duration::seconds(1) && ts <= after + Duration::seconds(1),
        "time_start must be within the wall-clock window (before={before} ts={ts} after={after})",
    );
    assert_eq!(
        entry
            .dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("MOMENT"),
        "occur is point-in-time: dur=Moment marks the shape",
    );
    assert!(entry.slug.starts_with("field:"));

    // The entry is retrievable by slug (round-trip through the DB).
    let fetched = db.get_entry(&entry.slug).unwrap();
    assert_eq!(fetched.slug, entry.slug);
}

/// TS #2: "stores the occurTime in the data"
///
/// Datum: `occurCmd("event -d 2021-08-23 -t 12 -z 0")` → occurTime.utc =
/// `2021-08-23T12:00:00.000Z`. Daydumb's `-z` is parsed but unused (see A5
/// follow-up), so we verify against local-zone interpretation of
/// `2021-08-23 12:00`. The invariant being locked in: `-d DATE -t HHMM`
/// consistently produce the same UTC instant.
#[test]
fn date_and_time_flags_set_absolute_time() {
    let db = fresh_db();

    run_cli(&["occur", "event", "-d", "2021-08-23", "-t", "1200"], &db).expect("occur -d -t");

    let entry = &db.entries_for_field("event", None).unwrap()[0];
    assert_entry_matches!(entry, field: "event", verb: "point");

    let ts = entry.occur_time.as_ref().unwrap().utc;
    let expected_local = chrono::NaiveDate::from_ymd_opt(2021, 8, 23)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_local_timezone(chrono::Local)
        .single()
        .expect("2021-08-23 12:00 is unambiguous in every zone");
    let expected_utc = expected_local.with_timezone(&Utc);
    assert_eq!(
        ts, expected_utc,
        "time_start must resolve to local 2021-08-23 12:00",
    );
}

/// TS #3: "stores the occurTime in DataOnly docs"
///
/// Datum's `-M` produced a DataOnly document (no `meta.*` block). Daydumb's
/// Entry always carries `created_at`/`modified_at`/`device_id` columns
/// (NOT NULL in the schema), so the equivalent behavior is to zero the
/// bookkeeping fields instead of omitting them: `created_at` and
/// `modified_at` collapse to the UNIX epoch and `device_id` goes empty.
/// The row's domain data (field/time_start/…) is still populated normally.
#[test]
fn no_metadata_zeros_bookkeeping_fields() {
    let db = fresh_db();

    // With `--no-metadata`: timestamps and device cleared.
    run_cli(&["occur", "event", "--no-metadata"], &db).expect("occur --no-metadata");
    let entry = &db.entries_for_field("event", None).unwrap()[0];
    assert_eq!(entry.device_id, "", "device_id must be cleared");
    assert_eq!(
        entry.create_time,
        chrono::DateTime::UNIX_EPOCH,
        "created_at must be epoch-zero",
    );
    assert_eq!(
        entry.modify_time,
        chrono::DateTime::UNIX_EPOCH,
        "modified_at must be epoch-zero",
    );
    // Domain data still lands — no-metadata only touches bookkeeping.
    assert!(entry.occur_time.is_some(), "time_start is unaffected");
    assert_eq!(entry.field, "event");

    // Without the flag, a normal occur records metadata as usual — the
    // inverse invariant.
    run_cli(&["occur", "other"], &db).expect("occur other");
    let other = &db.entries_for_field("other", None).unwrap()[0];
    assert!(!other.device_id.is_empty(), "device_id must be populated");
    assert!(
        (Utc::now() - other.create_time).num_seconds().abs() < 5,
        "created_at must be set to ~now",
    );
}

/// TS #4: "stores offset" (`-z +3`)
///
/// `-z +3` means "interpret -d/-t in UTC+3". The numeric form maps to
/// `Etc/GMT-3` (POSIX sign inversion) and 12:00 local in UTC+3 = 09:00 UTC.
/// The stored DatumTime.timezone is the resolved IANA name.
#[test]
fn numeric_offset_flag_applies() {
    let db = fresh_db();

    run_cli(
        &[
            "occur",
            "event",
            "-d",
            "2021-08-23",
            "-t",
            "1200",
            "-z",
            "+3",
        ],
        &db,
    )
    .expect("occur -z+3");

    let entry = &db.entries_for_field("event", None).unwrap()[0];
    let start = entry.occur_time.as_ref().unwrap();
    let expected_utc = chrono::NaiveDate::from_ymd_opt(2021, 8, 23)
        .unwrap()
        .and_hms_opt(9, 0, 0)
        .unwrap()
        .and_utc();
    assert_eq!(
        start.utc, expected_utc,
        "`-z +3`: 12:00 in UTC+3 = 09:00 UTC"
    );
    assert_eq!(start.timezone, "Etc/GMT-3", "POSIX-sign IANA name");
    assert_eq!(start.offset_minutes, 180, "UTC+3 offset in minutes");
}

/// TS #5: "stores offset from an IANA timezone" (`-z Europe/Berlin`)
///
/// 2023-12-04 is CET (UTC+1 in winter). 12:00 Berlin = 11:00 UTC.
#[test]
fn iana_timezone_flag_applies() {
    let db = fresh_db();

    run_cli(
        &[
            "occur",
            "event",
            "-d",
            "2023-12-04",
            "-t",
            "1200",
            "-z",
            "Europe/Berlin",
        ],
        &db,
    )
    .expect("occur -z Europe/Berlin");

    let entry = &db.entries_for_field("event", None).unwrap()[0];
    let start = entry.occur_time.as_ref().unwrap();
    let expected_utc = chrono::NaiveDate::from_ymd_opt(2023, 12, 4)
        .unwrap()
        .and_hms_opt(11, 0, 0)
        .unwrap()
        .and_utc();
    assert_eq!(
        start.utc, expected_utc,
        "12:00 CET (winter, UTC+1) = 11:00 UTC"
    );
    assert_eq!(start.timezone, "Europe/Berlin");
    assert_eq!(start.offset_minutes, 60, "CET winter offset");
}

/// TS #6: "records lastState as null if the active state is null"
///
/// Datum writes `lastState` onto the occur document to capture the prior
/// state; daydumb does not duplicate that onto the entry (it reconstructs
/// state from the verb timeline). The portable invariant: before any entry,
/// `get_active_state` returns `null`; after one occur, it compiles to
/// `false` (see `state::compile_state` for the Occur rule) — matching the
/// datum test's observation that "even 1 occur document is enough to switch
/// active state to false from null".
#[test]
fn occur_flips_state_from_null_to_false() {
    let db = fresh_db();

    let before_any = db.get_active_state("event", Utc::now()).unwrap();
    assert_eq!(
        before_any,
        json!(null),
        "no entries ⇒ state is null (datum parity)",
    );

    run_cli(&["occur", "event"], &db).expect("first occur");
    let after_one = db
        .get_active_state("event", Utc::now() + Duration::seconds(1))
        .unwrap();
    assert_eq!(
        after_one,
        json!(false),
        "occur compiles to state=false at time_start",
    );

    run_cli(&["occur", "event"], &db).expect("second occur");
    assert_count!(db, "event", 2);
    // Daydumb does not stamp `last_state` on occur entries — the entry's
    // extra_data is None unless the user provided keys.
    let entries = db.entries_for_field("event", None).unwrap();
    for e in &entries {
        if let Some(extra) = &e.data {
            assert!(
                extra.get("last_state").is_none(),
                "occur does not record last_state (daydumb divergence)",
            );
        }
    }
}

/// TS #7: "handles negative number time arguments correctly"
///
/// Datum: `-d -1d -t -1h` → 1 day + 1 hour back; `-t -20` → 20 min back.
/// Daydumb: `-d=-1` (days) and `-t=-60` (minutes) express the same offsets.
/// The `=` form pins the negative value to the flag so the parser does
/// not treat `-1` as a standalone arg (identical trick in `start_cmd.rs`).
#[test]
fn stacked_negative_offsets_subtract_correctly() {
    let db = fresh_db();

    let before = Utc::now();
    run_cli(&["occur", "event", "-d=-1", "-t=-60"], &db).expect("occur -d=-1 -t=-60");
    let after = Utc::now();

    let e1 = &db.entries_for_field("event", None).unwrap()[0];
    let ts1 = e1.occur_time.as_ref().unwrap().utc;
    let offset = Duration::days(1) + Duration::hours(1);
    let lo = before - offset - Duration::seconds(2);
    let hi = after - offset + Duration::seconds(2);
    assert!(
        ts1 >= lo && ts1 <= hi,
        "expected ~25h back (lo={lo} hi={hi} got={ts1})",
    );

    let before2 = Utc::now();
    run_cli(&["occur", "event", "-t=-20"], &db).expect("occur -t=-20");
    let after2 = Utc::now();

    let entries = db.entries_for_field("event", None).unwrap();
    assert_eq!(entries.len(), 2);
    // Grab the most-recent entry by created_at.
    let e2 = entries
        .iter()
        .max_by_key(|e| e.create_time)
        .expect("entries present");
    let ts2 = e2.occur_time.as_ref().unwrap().utc;
    let lo2 = before2 - Duration::minutes(20) - Duration::seconds(2);
    let hi2 = after2 - Duration::minutes(20) + Duration::seconds(2);
    assert!(
        ts2 >= lo2 && ts2 <= hi2,
        "expected ~20 min back (lo={lo2} hi={hi2} got={ts2})",
    );
}

/// TS #8: "can occur on a full day"
///
/// Datum's `-D`/`--full-day` recorded a whole-day event. Daydumb now parses
/// `-D`/`--full-day` on logging verbs (da-rsj4) — downstream plumbing that
/// drops the clock component from `time_start` is tracked in da-fc0l.2/.3/.4.
/// For now this case locks in the flag-acceptance contract and the `-d DATE`
/// analog, which already anchors to the requested calendar day in local time.
#[test]
fn full_day_flag_accepted() {
    let db = fresh_db();

    run_cli(&["occur", "event", "-D"], &db).expect("-D is accepted on occur");
    run_cli(&["occur", "happening", "--full-day"], &db).expect("--full-day is accepted on occur");

    // `-d DATE` without `-t` anchors at the local instant corresponding to
    // "now's time of day on that date". We only assert that the entry lands
    // on the requested calendar date in local time (avoiding timezone-induced
    // off-by-one near midnight).
    run_cli(&["occur", "event", "-d", "2024-04-23"], &db).expect("occur -d DATE");
    let e = &db.entries_for_field("event", None).unwrap()[0];
    let local_date = e
        .occur_time
        .as_ref()
        .unwrap()
        .utc
        .with_timezone(&chrono::Local)
        .date_naive();
    assert_eq!(
        local_date,
        chrono::NaiveDate::from_ymd_opt(2024, 4, 23).unwrap(),
        "entry lands on 2024-04-23 in local time",
    );
}

/// TS #9: "correctly displays interpolated field values in output"
///
/// Datum interpolated the *field* itself (`%task%` became the entry's
/// stored field name, using extra_data values). Daydumb does not template
/// the field, but A1 ships template interpolation on the slug via `--id`.
/// The closest observable analog: `--id "%task%" task=Testing` produces a
/// slug whose interpolated component is "Testing". Composite templates
/// (`%project%-%task%`) produce concatenated slugs. This locks in the flag
/// wiring on the `occur` verb specifically (other LogArgs commands are
/// covered by the A1 unit tests).
#[test]
fn id_template_interpolates_extra_data_on_occur() {
    let db = fresh_db();

    // Single-key template.
    run_cli(&["occur", "event", "task=Testing", "--id", "%task%"], &db).expect("occur --id %task%");
    let e1 = &db.entries_for_field("event", None).unwrap()[0];
    assert_eq!(e1.slug, "Testing", "slug = interpolated %task%");
    assert_eq!(
        e1.id_structure.as_deref(),
        Some("%task%"),
        "id_structure must persist the template",
    );
    assert_eq!(
        e1.data.as_ref().unwrap().get("task"),
        Some(&json!("Testing")),
    );

    // Composite template with two keys.
    let db2 = fresh_db();
    run_cli(
        &[
            "occur",
            "event",
            "project=Output",
            "task=Interpolation",
            "--id",
            "%project%-%task%",
        ],
        &db2,
    )
    .expect("occur --id %project%-%task%");
    let e2 = &db2.entries_for_field("event", None).unwrap()[0];
    assert_eq!(e2.slug, "Output-Interpolation");
    assert_eq!(e2.id_structure.as_deref(), Some("%project%-%task%"));
}

// ─── Regression: data slot fill + occur-never-sets-state ─────────────────
//
// Pre-fix bug: `datum occur pushup -k n=1 10` silently put `state=10` on
// the entry instead of setting `data.n = 10`. Two distinct issues fed
// the symptom:
//
// 1. parse_log_args (cmd.rs) routed `-k KEY=VALUE` through a side channel
//    ("immediates") that wrote KEY=VALUE directly into the data queue and
//    *did not declare* a slot — so the trailing positional `10` had no
//    slot to consume it and fell through to the PRIMARY_VALUE_KEY
//    remainder, becoming "primary value".
//
// 2. The primary value flowed into Entry::new_legacy's `state` parameter
//    via the (field, verb, value, ...) signature. Action::Occur has no
//    state column override, so the value landed verbatim in `entry.state`
//    despite occur being a stateless point-in-time event.
//
// The fix (cmd.rs): `-k KEY=VALUE` now declares a slot-with-default per
// original datum's `parseDataArgs` semantics, so positionals fill slots
// in order. For Action::Occur, the remainder bucket is disabled — any
// truly unbound positional surfaces as an error rather than silently
// flowing to state. For state-bearing verbs (Switch/Start/Stop) the
// first bare positional is peeled out as the state value before slot
// fill so it doesn't compete for `-k` slots.

/// `-k KEY=VALUE` is a slot-with-default declaration; the next positional
/// fills it. So `occur pushup -k n=1 10` writes `data.n = 10`, not
/// `data.n = 1` with `10` leaking elsewhere.
///
/// The state-side guarantee here is narrow: state must NOT be the
/// positional value (10). Whether state lands as NULL or as the canonical
/// `true` from `derive_verb`'s null→true promotion is a separate schema
/// question tracked by the follow-up "occur shouldn't have state=true"
/// bead. The original bug was the user-visible *value* leak, not the
/// canonicalization.
#[test]
fn occur_k_key_default_is_overridden_by_positional() {
    let db = fresh_db();

    run_cli(&["occur", "pushup", "-k", "n=1", "10"], &db).expect("occur with -k slot fill");

    let entry = &db.entries_for_field("pushup", None).unwrap()[0];
    assert_eq!(entry.field, "pushup");
    assert_ne!(
        entry.state,
        Some(json!(10)),
        "regression guard: positional `10` must NOT leak into state",
    );
    assert_ne!(
        entry.state,
        Some(json!("10")),
        "regression guard: positional `10` (string form) must NOT leak into state",
    );
    let data = entry
        .data
        .as_ref()
        .expect("expected data payload with n=10");
    assert_eq!(
        data.get("n"),
        Some(&json!(10)),
        "positional `10` must override the `-k n=1` default and land at data.n",
    );
}

/// With no positional, the `-k KEY=VALUE` default applies on its own.
#[test]
fn occur_k_key_default_applied_when_no_positional() {
    let db = fresh_db();

    run_cli(&["occur", "pushup", "-k", "n=1"], &db).expect("occur with -k default only");

    let entry = &db.entries_for_field("pushup", None).unwrap()[0];
    let data = entry.data.as_ref().expect("expected data with n=1");
    assert_eq!(data.get("n"), Some(&json!(1)));
}

/// Multiple `-k` slots fill in declaration order from the positional queue.
#[test]
fn occur_multiple_k_slots_fill_in_order() {
    let db = fresh_db();

    run_cli(
        &["occur", "pushup", "-k", "n=1", "-k", "set=2", "10", "3"],
        &db,
    )
    .expect("occur with two -k slots filled by positionals");

    let entry = &db.entries_for_field("pushup", None).unwrap()[0];
    let data = entry.data.as_ref().expect("expected data");
    assert_eq!(data.get("n"), Some(&json!(10)));
    assert_eq!(data.get("set"), Some(&json!(3)));
    // Regression guard: neither positional must have leaked into state.
    assert_ne!(entry.state, Some(json!(10)));
    assert_ne!(entry.state, Some(json!(3)));
}

/// `occur` with an unbound positional (no `-k` slot, no `-K` slug-key, no
/// explicit `KEY=VALUE` token) must error rather than silently routing
/// the value into `entry.state`.
#[test]
fn occur_unbound_positional_is_rejected() {
    let db = fresh_db();

    let err = run_cli(&["occur", "pushup", "10"], &db)
        .expect_err("occur with bare positional should bail");

    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("extra")
            || msg.to_lowercase().contains("unbound")
            || msg.to_lowercase().contains("doesn't carry state"),
        "expected an unbound-positional / extra-data error, got: {msg}",
    );
    assert_eq!(
        db.entries_for_field("pushup", None).unwrap().len(),
        0,
        "no entry should have been written when the positional was rejected",
    );
}

/// Explicit `state=<v>` on `occur` is also rejected — occur is stateless
/// by definition; the user wants `switch`/`start`/`stop` for state changes.
#[test]
fn occur_explicit_state_key_is_rejected() {
    let db = fresh_db();

    let err = run_cli(&["occur", "pushup", "state=10"], &db)
        .expect_err("occur with state=<v> should bail");

    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("doesn't carry state") || msg.to_lowercase().contains("state"),
        "expected an occur-stateless error, got: {msg}",
    );
    assert_eq!(db.entries_for_field("pushup", None).unwrap().len(), 0,);
}

/// da-ju71.2 acceptance: `occur pushup -k n=1 10` lands as a point-in-time
/// event with `state=null`, `verb=point`, and `dur="MOMENT"`. The Moment
/// marker locks `derive_verb` to Point regardless of any state-data the
/// user attached via `-k`, so the entry stays an occur and not a Switch.
#[test]
fn occur_with_keyed_data_emits_moment_dur_and_null_state() {
    let db = fresh_db();

    run_cli(&["occur", "pushup", "-k", "n=1", "10"], &db).expect("occur pushup -k n=1 10 dispatch");

    assert_count!(db, "pushup", 1);
    let entry = &db.entries_for_field("pushup", None).unwrap()[0];

    assert!(
        entry.state.is_none(),
        "occur stays state=null (no null→true promotion); got {:?}",
        entry.state,
    );
    assert_eq!(
        entry
            .dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("MOMENT"),
        "occur with no explicit --dur emits Dur::Moment",
    );
    assert_eq!(
        entry.verb,
        datum_core::entry::Verb::Point,
        "Moment locks the verb to Point",
    );

    // Re-derive from the persisted columns: the Moment rule must agree
    // with the stored verb. Otherwise the column would drift.
    let (rederived, _) = datum_core::derive_verb(
        entry.state.clone(),
        entry.dur.as_ref(),
        entry.following_state.clone(),
    )
    .expect("re-derive from persisted columns");
    assert_eq!(rederived, datum_core::entry::Verb::Point);

    // The structured payload survives in `data`. Slot-fill semantics
    // route `-k n=1` and the bare `10` into the `n` slot; we don't pin
    // the exact value here — the Moment lock holds regardless of how
    // the data is shaped.
    let data = entry.data.as_ref().expect("data payload from -k n");
    assert!(
        data.get("n").is_some(),
        "expected `n` slot in data, got {data:?}",
    );
}
