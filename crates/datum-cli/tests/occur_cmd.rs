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
use datum_core::time::Zone;
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
    // da-ju71.2: occur with no explicit dur emits Dur::moment(), locking
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
    let expected_utc = Zone::host()
        .resolve(
            chrono::NaiveDate::from_ymd_opt(2021, 8, 23)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
        )
        .single()
        .expect("2021-08-23 12:00 is unambiguous in every zone");
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
    assert_eq!(
        start.timezone.as_deref(),
        Some("Etc/GMT-3"),
        "POSIX-sign IANA name"
    );
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
    assert_eq!(start.timezone.as_deref(), Some("Europe/Berlin"));
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

/// da-iyv3: live insert wires `args.time.full_day` through `TimeBuilder`,
/// which puts the resulting uncertainty on the `DatumTime` itself
/// (column `occur_time_uncertainty`).
///   * `-D` → P1D + today 00:00 local
///   * `-d -1` (no time) → P1D + yesterday 00:00 local
///   * `-d -1 -t 14:00` → uncertainty NULL + yesterday 14:00 local
///   * `-D -t 14:00` → P1D + today 00:00 (full-day wins over clock)
#[test]
fn full_day_writes_uncertainty_and_strips_clock() {
    use chrono::{Local, NaiveTime};

    let db = fresh_db();

    // -D alone → P1D, today 00:00 local.
    run_cli(&["occur", "today_full", "-D"], &db).expect("occur -D");
    let e = &db.entries_for_field("today_full", None).unwrap()[0];
    let local = e.occur_time.as_ref().unwrap().utc.with_timezone(&Local);
    assert_eq!(
        e.occur_uncertainty(),
        Some("P1D"),
        "-D stamps occur_time_uncertainty=P1D",
    );
    assert_eq!(
        local.time(),
        NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        "-D strips clock to 00:00 local",
    );
    assert_eq!(
        local.date_naive(),
        Local::now().date_naive(),
        "-D anchors to today in local time",
    );

    // -d -1 (no time, no quick) → P1D, yesterday 00:00 local.
    run_cli(&["occur", "yest_full", "-d", "-1"], &db).expect("occur -d -1");
    let e = &db.entries_for_field("yest_full", None).unwrap()[0];
    let local = e.occur_time.as_ref().unwrap().utc.with_timezone(&Local);
    assert_eq!(
        e.occur_uncertainty(),
        Some("P1D"),
        "-d -1 (no clock input) is a date-only event",
    );
    assert_eq!(local.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    // -d -1 -t 14:00 → no uncertainty stated, yesterday 14:00 local.
    run_cli(&["occur", "yest_clock", "-d", "-1", "-t", "14:00"], &db)
        .expect("occur -d -1 -t 14:00");
    let e = &db.entries_for_field("yest_clock", None).unwrap()[0];
    let local = e.occur_time.as_ref().unwrap().utc.with_timezone(&Local);
    assert!(
        e.occur_uncertainty().is_none(),
        "explicit -t states no uncertainty (NULL)",
    );
    assert_eq!(local.time(), NaiveTime::from_hms_opt(14, 0, 0).unwrap());

    // -D -t 14:00 → full-day wins; clock is stripped, P1D recorded.
    run_cli(&["occur", "full_overrides_clock", "-D", "-t", "14:00"], &db)
        .expect("occur -D -t 14:00");
    let e = &db.entries_for_field("full_overrides_clock", None).unwrap()[0];
    let local = e.occur_time.as_ref().unwrap().utc.with_timezone(&Local);
    assert_eq!(
        e.occur_uncertainty(),
        Some("P1D"),
        "-D wins over clock per builder rule",
    );
    assert_eq!(local.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
}

/// da-4fyb: a date-only entry slugs on the bare local date, not on the
/// midnight timestamp its `occur_time` happens to store. `%occur_time%`
/// interpolation is what builds the default slug, and a `T00:00:00.000Z`
/// suffix there reads as a real time-of-day the entry does not have — and,
/// east of UTC, names the wrong calendar day.
#[test]
fn full_day_slugs_on_bare_date() {
    use chrono::Local;

    let db = fresh_db();

    // Bracket the CLI call: reading the date only AFTER it runs makes the
    // assertion flake when local midnight falls between the two (Copilot
    // review, PR #86). The entry's date must be whichever side of midnight
    // the CLI actually landed on, so accept either.
    let before = Local::now().date_naive();
    run_cli(&["occur", "fullday_slug", "-D"], &db).expect("occur -D");
    let after = Local::now().date_naive();
    let e = &db.entries_for_field("fullday_slug", None).unwrap()[0];
    let acceptable = [
        format!("fullday_slug:{}", before.format("%Y-%m-%d")),
        format!("fullday_slug:{}", after.format("%Y-%m-%d")),
    ];
    assert!(
        acceptable.contains(&e.slug.as_str().to_string()),
        "slug {} matched neither side of the midnight boundary ({acceptable:?})",
        e.slug.as_str(),
    );

    // A timed entry on the same day keeps the full stamp.
    run_cli(&["occur", "timed_slug", "-t", "14:00"], &db).expect("occur -t 14:00");
    let e = &db.entries_for_field("timed_slug", None).unwrap()[0];
    assert_eq!(
        e.slug.as_str(),
        format!(
            "timed_slug:{}",
            e.occur_time
                .as_ref()
                .unwrap()
                .utc
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        ),
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

    run_cli(&["occur", "shortflag", "-D"], &db).expect("-D is accepted on occur");
    run_cli(&["occur", "longflag", "--full-day"], &db).expect("--full-day is accepted on occur");

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
    // da-ivld: slug = `<field>:<body>`.
    assert_eq!(
        e1.slug, "event:Testing",
        "slug = field:interpolated(%task%)"
    );
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
    assert_eq!(e2.slug, "event:Output-Interpolation");
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
        "occur with no explicit --dur emits Dur::moment()",
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

// ─── da-ywmz: flexible_positional for `field` on `add`/`occur` ────────────

/// Acceptance: `dm occur abc` and `dm occur field=abc` produce equivalent
/// entries — `entry.field = "abc"` in both forms, with no leftover `field`
/// key in extra_data. The positional and the keyed assignment route through
/// the same flex-slot machinery introduced in da-ywmz.
#[test]
fn occur_field_positional_and_field_kv_are_equivalent() {
    let db_pos = fresh_db();
    run_cli(&["occur", "abc"], &db_pos).expect("occur abc");
    let pos_entries = db_pos.entries_for_field("abc", None).unwrap();
    assert_eq!(pos_entries.len(), 1, "occur abc → 1 entry on field=abc");
    assert_eq!(pos_entries[0].field, "abc");
    assert!(
        pos_entries[0].data.is_none(),
        "no leftover `field` key in extra_data",
    );

    let db_kv = fresh_db();
    run_cli(&["occur", "field=abc"], &db_kv).expect("occur field=abc");
    let kv_entries = db_kv.entries_for_field("abc", None).unwrap();
    assert_eq!(
        kv_entries.len(),
        1,
        "occur field=abc → 1 entry on field=abc"
    );
    assert_eq!(kv_entries[0].field, "abc");
    assert!(
        kv_entries[0].data.is_none(),
        "no leftover `field` key in extra_data",
    );
}

/// Acceptance: `-k field=ghi abc` lets the positional override the
/// `-k`-declared default. The user's bare positional binds to the
/// `field` slot via slot-fill, beating the `ghi` default.
#[test]
fn occur_positional_overrides_dash_k_field_default() {
    let db = fresh_db();
    run_cli(&["occur", "-k", "field=ghi", "abc"], &db).expect("occur -k field=ghi abc");
    let entries = db.entries_for_field("abc", None).unwrap();
    assert_eq!(
        entries.len(),
        1,
        "positional `abc` wins over -k default `ghi`"
    );
    assert_eq!(entries[0].field, "abc");
    // The default `ghi` never lands anywhere — it was overridden, not stored.
    assert!(db.entries_for_field("ghi", None).unwrap().is_empty());
    assert!(
        entries[0].data.is_none(),
        "no leftover `field` key in extra_data after override",
    );
}

/// Acceptance: an alias body that bakes `field=abc` is satisfied when the
/// user adds extra positionals to fill downstream slots. The alias-baked
/// `field=` injects via the same flex-slot path, so the user doesn't need
/// to repeat the field on the command line.
#[test]
fn occur_alias_baked_field_kv() {
    let db = fresh_db();
    run_cli(&["alias", "set", "abc", "occur field=abc"], &db).expect("alias set abc");
    run_cli(&["abc"], &db).expect("alias abc expands to occur field=abc");
    let entries = db.entries_for_field("abc", None).unwrap();
    assert_eq!(entries.len(), 1, "alias-baked field= sets entry.field");
    assert_eq!(entries[0].field, "abc");
    assert!(
        entries[0].data.is_none(),
        "alias-baked `field=abc` consumed onto the column, not extra_data",
    );
}

/// `add field=abc` (with no other positional) is the keyed form of the
/// field; entry.field = "abc" and the `field` key is consumed off the
/// payload onto the column.
#[test]
fn add_field_kv_consumed_onto_column() {
    let db = fresh_db();
    run_cli(&["note", "field=abc"], &db).expect("note field=abc");
    let entries = db.entries_for_field("abc", None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].field, "abc");
    assert!(
        entries[0].data.is_none(),
        "no leftover `field` key in extra_data",
    );
}

/// `add -k field=ghi abc kv=v` — positional `abc` overrides the `field`
/// default; subsequent `kv=v` lands in extra_data as normal.
#[test]
fn add_positional_overrides_field_default_extras_preserved() {
    let db = fresh_db();
    run_cli(&["note", "-k", "field=ghi", "abc", "kv=v"], &db).expect("note -k field=ghi abc kv=v");
    let entries = db.entries_for_field("abc", None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].field, "abc");
    let extra = entries[0].data.as_ref().expect("kv=v lands in extra_data");
    assert_eq!(extra.get("kv"), Some(&json!("v")));
    assert!(
        extra.get("field").is_none(),
        "field slot consumed onto column, not leaked into extra_data",
    );
}

// ─── --dry-run / --dryrun ─────────────────────────────────────────────────

/// `dm occur coffee --dry-run` previews the would-be entry without inserting.
/// Acceptance criterion for da-wc2h: dry-run prints the slug, field, and data
/// and exits 0; a follow-up `entries_for_field` query must show nothing.
#[test]
fn occur_dry_run_does_not_insert() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "--dry-run"], &db).expect("dry-run must succeed");
    assert_count!(db, "coffee", 0);
}

/// `dm occur coffee --dryrun` (no hyphen) is accepted as an alias for
/// `--dry-run` to match the user-facing feature name in da-wc2h.
#[test]
fn occur_dryrun_alias_does_not_insert() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "--dryrun"], &db).expect("dryrun must succeed");
    assert_count!(db, "coffee", 0);
}

/// `dm switch project money --dry-run` previews the switch (state routing
/// included) without writing. Acceptance criterion for da-wc2h.
#[test]
fn switch_dry_run_does_not_insert() {
    let db = fresh_db();
    run_cli(&["switch", "project", "money", "--dry-run"], &db)
        .expect("switch dry-run must succeed");
    assert_count!(db, "project", 0);
}

/// da-i5vs acceptance, renamed by da-b8rs: `dm occur foo
/// dur_uncertainty=PT30M dur=PT1H30M` routes both reserved keys to
/// columns. `dur` ends up as the ISO-8601 duration; the uncertainty lands
/// on `entry.dur` (column `dur_uncertainty`) rather than in `data`.
#[test]
fn occur_routes_dur_uncertainty_reserved_key_to_column() {
    let db = fresh_db();
    run_cli(
        &["occur", "foo", "dur_uncertainty=PT30M", "dur=PT1H30M"],
        &db,
    )
    .expect("occur with reserved keys must dispatch");

    assert_count!(db, "foo", 1);
    let entry = &db.entries_for_field("foo", None).unwrap()[0];
    assert_eq!(
        entry.dur_uncertainty(),
        Some("PT30M"),
        "dur_uncertainty= must land on the column, not data",
    );
    assert_eq!(
        entry
            .dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("PT1H30M"),
        "dur= still routes to the dur column",
    );
    assert!(
        entry.data.is_none()
            || !entry
                .data
                .as_ref()
                .and_then(|d| d.as_object())
                .map(|o| o.contains_key("dur_uncertainty"))
                .unwrap_or(false),
        "dur_uncertainty must NOT leak into data: {:?}",
        entry.data,
    );
}

/// da-b8rs: an uncertainty qualifies something. With no duration on the
/// entry there is nothing for `dur_uncertainty=` to be *about*, and the old
/// loose column would have stored it anyway — an orphan value no reader
/// could interpret. Fail instead of writing it.
#[test]
fn dur_uncertainty_without_a_duration_is_rejected() {
    let db = fresh_db();
    let err = run_cli(&["start", "foo", "dur_uncertainty=PT30M"], &db)
        .expect_err("dur_uncertainty= with no duration must fail");
    assert!(
        err.to_string().contains("needs a duration"),
        "unhelpful error: {err}"
    );
    assert_count!(db, "foo", 0);
}

/// The same rule on the occur side: `-T` makes a timeless entry, which has
/// no instant for an uncertainty to qualify.
#[test]
fn occur_time_uncertainty_without_a_time_is_rejected() {
    let db = fresh_db();
    let err = run_cli(&["add", "foo", "-T", "occur_time_uncertainty=PT5M"], &db)
        .expect_err("occur_time_uncertainty= on a timeless entry must fail");
    assert!(
        err.to_string().contains("needs a time"),
        "unhelpful error: {err}"
    );
    assert_count!(db, "foo", 0);
}

/// da-i5vs: bare `unit=foo` is no longer a reserved column key — the
/// vestigial `unit` column was dropped (da-d0vs / da-xvco). It now lands
/// in `data` like any other ad-hoc key.
#[test]
fn occur_unit_key_lands_in_data() {
    let db = fresh_db();
    run_cli(&["occur", "weight", "unit=kg"], &db).expect("occur weight unit=kg must dispatch");

    let entry = &db.entries_for_field("weight", None).unwrap()[0];
    let data = entry.data.as_ref().expect("unit= should land in data");
    assert_eq!(
        data.get("unit").and_then(|v| v.as_str()),
        Some("kg"),
        "unit= must fall through to data: {data:?}",
    );
}
