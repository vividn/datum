//! Tests for `-T/--no-timestamp` semantics (S5 / da-u4k.5, da-5ns).
//!
//! Any log verb (`occur`, `start`, `stop`/`end`, `switch`, `add`/`note`) +
//! `-T` produces a timeless entry: `occur_time` is NULL and the slug is
//! `field:c<create_time>` (da-r17h — was `:unset`), and the verb is whatever
//! derive_verb returns for the action's canonical (state, dur, fs) triple. A
//! state-bearing verb stamps `dur=MOMENT`, so it is a point naming a state,
//! not a transition (da-w149). Distinct create_times keep many-per-field
//! timeless entries apart. Time-specifying flags mixed with `-T` error out;
//! a duration is dropped (da-fu7a0).

use crate::common;

use common::run_cli;
use datum_core::dur::Dur;
use datum_core::test_util::fresh_db;
use datum_core::{assert_count, assert_entry_matches};

/// A timeless entry's slug is `field:c<create_time>` (da-r17h): the body
/// after the `field:` delimiter begins with the `c` create-time marker.
fn slug_is_timeless(slug: &str) -> bool {
    slug.split_once(':')
        .is_some_and(|(_, body)| body.starts_with('c'))
}

// ─── Happy path: each log verb + -T yields a timeless add-shaped entry ───────

#[test]
fn occur_minus_t_is_timeless_point() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "-T"], &db).expect("occur -T");
    assert_count!(db, "coffee", 1);
    let e = &datum_core::test_util::logged_entries(&db, "coffee")[0];
    // occur -T: state=None, dur="MOMENT" (da-ju71.2) → Point. `-T`
    // suppresses the timestamp, not the occur-shape intent.
    assert_entry_matches!(e, field: "coffee", verb: "point");
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "timeless slug is create_time-keyed (got: {})",
        e.slug
    );
    assert_eq!(
        e.dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("MOMENT"),
        "occur emits Dur::moment()"
    );
}

#[test]
fn start_minus_t_is_timeless_point() {
    let db = fresh_db();
    run_cli(&["start", "sleep", "-T"], &db).expect("start -T");
    let e = &datum_core::test_util::logged_entries(&db, "sleep")[0];
    // start -T: action canonicalizes state=true, dur=MOMENT → Point.
    assert_entry_matches!(e, verb: "point", state: true);
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "timeless slug: {}",
        e.slug
    );
    assert_eq!(e.dur, Some(Dur::moment()));
}

#[test]
fn stop_minus_t_is_timeless_point() {
    let db = fresh_db();
    run_cli(&["stop", "task", "-T"], &db).expect("stop -T");
    let e = &datum_core::test_util::logged_entries(&db, "task")[0];
    // stop -T: action canonicalizes state=false, dur=MOMENT → Point.
    assert_entry_matches!(e, verb: "point", state: false);
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "timeless slug: {}",
        e.slug
    );
    assert_eq!(e.dur, Some(Dur::moment()));
}

#[test]
fn end_minus_t_is_timeless_point() {
    let db = fresh_db();
    run_cli(&["end", "task", "-T"], &db).expect("end -T");
    let e = &datum_core::test_util::logged_entries(&db, "task")[0];
    assert_entry_matches!(e, verb: "point", state: false);
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "timeless slug: {}",
        e.slug
    );
    assert_eq!(e.dur, Some(Dur::moment()));
}

#[test]
fn switch_minus_t_preserves_value() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy", "-T"], &db).expect("switch -T");
    let e = &datum_core::test_util::logged_entries(&db, "mood")[0];
    // The primary value ("happy") still lands on the entry — `-T` only
    // suppresses the timestamp, not the payload. state="happy", dur=MOMENT
    // → Point.
    assert_entry_matches!(e, field: "mood", verb: "point", state: "happy");
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "timeless slug: {}",
        e.slug
    );
    assert_eq!(e.dur, Some(Dur::moment()));
}

#[test]
fn add_minus_t_is_timeless_point() {
    let db = fresh_db();
    run_cli(&["add", "note", "-T"], &db).expect("add -T");
    let e = &datum_core::test_util::logged_entries(&db, "note")[0];
    // add -T: state=None (Add doesn't canonicalize), dur=None → Point.
    assert_entry_matches!(e, verb: "point");
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "timeless slug: {}",
        e.slug
    );
    assert!(e.dur.is_none());
}

// ─── Regression: -T entry is genuinely timeless (occur_time NULL) ────────────

/// da-o7iz: `<verb> foo -T` produces a timeless, note-equivalent entry —
/// `occur_time` reads back as `None` (not a `create_time` stub) and the slug
/// is `field:c<create_time>` (da-r17h), exactly like a bare `note foo`. Guards
/// against the stale belief that "daydumb always anchors entries to
/// `time_start`" (the comments formerly in `start_cmd.rs`/`stop_cmd.rs`
/// claiming `-T` has no equivalent).
#[test]
fn minus_t_entry_is_timeless_occur_time_none() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "-T"], &db).expect("occur coffee -T");
    let e = &datum_core::test_util::logged_entries(&db, "coffee")[0];
    assert!(
        e.occur_time.is_none(),
        "`-T` must leave occur_time NULL (timeless), got {:?}",
        e.occur_time
    );
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "timeless slug is create_time-keyed (got: {})",
        e.slug
    );
}

// ─── Payload preservation: -T still routes through the normal kv parsing ─────

#[test]
fn minus_t_keeps_extra_data_kv_pairs() {
    let db = fresh_db();
    run_cli(&["occur", "word", "-T", "pos=verb", "srcLang=pt"], &db)
        .expect("occur word -T pos=verb srcLang=pt");
    let e = &datum_core::test_util::logged_entries(&db, "word")[0];
    let data = e.data.as_ref().expect("kv pairs land in extra_data");
    assert_eq!(data.get("pos"), Some(&serde_json::json!("verb")));
    assert_eq!(data.get("srcLang"), Some(&serde_json::json!("pt")));
    assert!(
        slug_is_timeless(e.slug.as_str()),
        "timeless slug: {}",
        e.slug
    );
}

// ─── Rejection: -T plus any time flag is a hard error ────────────────────────

#[test]
fn minus_t_with_t_flag_errors() {
    let db = fresh_db();
    let err =
        run_cli(&["occur", "coffee", "-T", "-t", "15:00"], &db).expect_err("-T + -t must reject");
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
    let err = run_cli(&["occur", "coffee", "-T", "-d=-1"], &db).expect_err("-T + -d must reject");
    assert!(format!("{err:#}").contains("-d"), "got: {err:#}");
}

/// da-fu7a0: `-T` makes note-data, which has no duration — `dur=` and the
/// trailing positional are dropped, as `retime -T` drops an entry's, and
/// the note is logged a timeless MOMENT point. (The retired `--dur`'s death
/// is covered by `dur_flag_removed.rs`.)
#[test]
fn minus_t_drops_a_duration() {
    let db = fresh_db();
    for (args, field) in [
        (&["occur", "run", "-T", "dur=30m"][..], "run"),
        (&["start", "work", "-T", "dur=2h"][..], "work"),
        (&["start", "sleep", "-T", "30m"][..], "sleep"),
        (
            &["occur", "coffee", "-T", "dur=5m±1m", "dur_uncertainty=2m"][..],
            "coffee",
        ),
    ] {
        run_cli(args, &db).unwrap_or_else(|e| panic!("{args:?}: {e:#}"));
        let e = &datum_core::test_util::logged_entries(&db, field)[0];
        assert_entry_matches!(e, verb: "point");
        assert!(e.occur_time.is_none(), "{args:?}: timeless");
        assert!(slug_is_timeless(e.slug.as_str()), "{args:?}: {}", e.slug);
        assert_eq!(e.dur, Some(Dur::moment()), "{args:?}");
    }
}

#[test]
fn minus_t_with_y_flag_errors() {
    let db = fresh_db();
    let err = run_cli(&["occur", "coffee", "-T", "-y"], &db).expect_err("-T + -y must reject");
    assert!(format!("{err:#}").contains("-y"));
}

#[test]
fn minus_t_with_q_flag_errors() {
    let db = fresh_db();
    let err = run_cli(&["occur", "coffee", "-T", "-q"], &db).expect_err("-T + -q must reject");
    assert!(format!("{err:#}").contains("-q"));
}

#[test]
fn minus_t_with_z_flag_errors() {
    let db = fresh_db();
    let err =
        run_cli(&["occur", "coffee", "-T", "-z", "UTC"], &db).expect_err("-T + -z must reject");
    assert!(format!("{err:#}").contains("-z"));
}

// ─── Non-regression: plain log verbs (no -T) still get timestamps ────────────

#[test]
fn without_minus_t_occur_still_timestamps() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("plain occur");
    let e = &datum_core::test_util::logged_entries(&db, "coffee")[0];
    assert!(
        e.occur_time.is_some(),
        "plain `occur` must still stamp start_time"
    );
}

// ─── da-s9ngd: a timeless entry dedups by its slug ───────────────────────────

/// A timeless entry's identity is its create_time, which its slug carries and
/// its content hash leaves out, so the same words logged twice are two
/// entries. The same slug with the same content is still one.
#[test]
fn identical_timeless_entries_are_two_entries() {
    let db = fresh_db();
    for args in [
        &["note", "diary", "hello"][..],
        &["occur", "chore", "-T", "item=x"][..],
    ] {
        run_cli(args, &db).expect("first");
        std::thread::sleep(std::time::Duration::from_millis(2));
        run_cli(args, &db).expect("second");
    }
    assert_count!(db, "diary", 2);
    assert_count!(db, "chore", 2);

    run_cli(&["add", "book", "--id", "gatsby"], &db).expect("first");
    run_cli(&["add", "book", "--id", "gatsby"], &db).expect("same slug, same content");
    assert_count!(db, "book", 1);
}
