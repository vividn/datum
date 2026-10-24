//! Tests for `-T/--no-timestamp` semantics (S5 / da-u4k.5, da-5ns).
//!
//! Any log verb (`occur`, `start`, `stop`/`end`, `switch`, `add`/`note`) +
//! `-T` produces a timeless entry: `occur_time` is NULL and the slug is
//! `field:<create_time>c` (da-r17h — was `:unset`), and the verb is whatever
//! derive_verb returns for the action's canonical (state, dur, fs) triple. A
//! state-bearing verb stamps `dur=MOMENT`, so it is a point naming a state,
//! not a transition (da-w149). Distinct create_times keep many-per-field
//! timeless entries apart. Time-specifying flags mixed with `-T` error out,
//! and so does a duration on any timeless entry (da-mec79).

use crate::common;

use common::run_cli;
use datum_core::dur::Dur;
use datum_core::test_util::fresh_db;
use datum_core::{assert_count, assert_entry_matches};

/// A timeless entry's slug is `field:<create_time>c` (da-r17h): the body
/// after the `field:` delimiter ends with the `c` create-time marker.
fn slug_is_timeless(slug: &str) -> bool {
    slug.split_once(':')
        .is_some_and(|(_, body)| body.ends_with('c'))
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
/// is `field:<create_time>c` (da-r17h), exactly like a bare `note foo`. Guards
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

/// da-mec79: a timeless entry carries no duration (vividn, 2026-10-23), so
/// one on the line — `dur=`, the trailing positional, a `,`-chain's, its
/// uncertainty — is refused on every verb, `-T` or a note's own
/// timelessness, and nothing is written. (The retired `--dur`'s death is
/// covered by `dur_flag_removed.rs`.)
#[test]
fn a_timeless_entry_refuses_a_duration() {
    let db = fresh_db();
    for (args, field) in [
        (&["occur", "w", "-T", "dur=10m"][..], "w"),
        (&["switch", "x", "on", "-T", "10m"][..], "x"),
        (&["start", "sleep", "-T", "30m"][..], "sleep"),
        (&["switch", "z", "on,off", "-T", "10m"][..], "z"),
        (
            &["occur", "coffee", "-T", "dur_uncertainty=2m"][..],
            "coffee",
        ),
        (&["note", "n", "dur=5m"][..], "n"),
        (&["add", "a", "--id", "a", "k=1", "dur=5m"][..], "a"),
    ] {
        let err = run_cli(args, &db).expect_err(&format!("{args:?} must refuse"));
        assert!(
            format!("{err:#}").contains("a timeless entry (no time) carries no duration"),
            "{args:?}: {err:#}"
        );
        assert_count!(db, field, 0);
    }
}

/// da-mec79: so is `update` setting one on a timeless entry, which keeps
/// what it had; given a time alongside, it takes both.
#[test]
fn update_refuses_a_duration_on_a_timeless_entry() {
    let db = fresh_db();
    run_cli(&["note", "n", "hello"], &db).expect("note");
    let hid = datum_core::test_util::logged_entries(&db, "n")[0]
        .hid
        .to_string();
    let err = run_cli(&["update", &hid, "dur=10m"], &db).expect_err("must refuse");
    assert!(
        format!("{err:#}").contains("carries no duration"),
        "{err:#}"
    );
    let e = &datum_core::test_util::logged_entries(&db, "n")[0];
    assert!(e.dur.is_none() && e.occur_time.is_none());

    run_cli(&["update", &hid, "occur_time=10:00", "dur=10m"], &db).expect("timed update");
    let e = &datum_core::test_util::logged_entries(&db, "n")[0];
    assert!(e.occur_time.is_some());
    assert_eq!(e.dur, Some(Dur::parse("PT10M").unwrap()));
}

/// da-mec79: a note and an add stay `dur: null`, a timeless occur or
/// switch `MOMENT` (da-w149), and an entry given a time keeps its duration.
#[test]
fn timeless_durations_and_timed_ones() {
    let db = fresh_db();
    for (args, field, dur) in [
        (&["note", "n", "hello"][..], "n", None),
        (&["add", "a", "--id", "a", "k=1"][..], "a", None),
        (&["occur", "w", "-T"][..], "w", Some(Dur::moment())),
        (&["switch", "x", "on", "-T"][..], "x", Some(Dur::moment())),
        (
            &["note", "t", "-t", "10:00", "dur=5m"][..],
            "t",
            Some(Dur::parse("PT5M").unwrap()),
        ),
        (
            &["add", "b", "--id", "b", "occur_time=10:00", "dur=5m"][..],
            "b",
            Some(Dur::parse("PT5M").unwrap()),
        ),
    ] {
        run_cli(args, &db).unwrap_or_else(|e| panic!("{args:?}: {e:#}"));
        let e = &datum_core::test_util::logged_entries(&db, field)[0];
        assert_eq!(e.dur, dur, "{args:?}");
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

/// da-06jxa: a `-T` id puts its `c` after the time, so a field's ids sorted
/// as text are in time order, the `-T` entry right after an occur entry of
/// its own instant and before the next.
#[test]
fn a_minus_t_id_sorts_beside_the_occur_ids_of_its_time() {
    let db = fresh_db();
    run_cli(&["occur", "y", "-T"], &db).expect("occur y -T");
    let timeless = datum_core::test_util::logged_entries(&db, "y")[0].clone();
    let at = |mins: i64| {
        (timeless.create_time + chrono::Duration::minutes(mins))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    for t in [at(-1), at(0), at(1)] {
        run_cli(&["occur", "y", "-t", &t], &db).expect("occur y -t");
    }
    let mut slugs: Vec<String> = datum_core::test_util::logged_entries(&db, "y")
        .into_iter()
        .map(|e| e.slug.0)
        .collect();
    slugs.sort();
    let stamp = |t: String| format!("y:{t}");
    assert_eq!(
        slugs,
        [
            stamp(at(-1)),
            stamp(at(0)),
            timeless.slug.0.clone(),
            stamp(at(1))
        ],
        "the -T id is {}",
        timeless.slug
    );
}
