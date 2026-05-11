//! Parity tests for `datum switch` (P5 / da-4pne).
//!
//! Ports the 16 cases from `switchCmd.test.ts` in the original datum project,
//! adapted to daydumb's flag-based model. Each Rust test maps 1-to-1 to a TS
//! case; doc headers identify the source.
//!
//! Divergences from datum:
//!   * daydumb's `--last-state=<v>` on switch is a user **assertion** recorded
//!     in a dedicated column (S6). The system cross-checks against the
//!     computed prior state and files a `last_state_mismatch` conflict when
//!     they disagree, but never refuses the switch. Bare `--last-state` with
//!     no value is a no-op. Auto-record of prior state into `extra_data` is
//!     NOT performed — state queries run over the timeline.
//!   * daydumb has no `-T` / `--omit-timestamp`. Switch entries always anchor
//!     `time_start`. The three TS cases that hinge on "no occurTime" become
//!     structural checks that daydumb anchors time regardless.
//!   * daydumb has no positional-duration slot. Duration is supplied only via
//!     `--dur`. Positionals after the state value append into `value` (as an
//!     array); none are silently re-interpreted as a duration. The "skip
//!     duration with `.` or `''`" TS case has no analogue to port.
//!   * daydumb has no "required keys" (`-k KEY` without `=VALUE`) concept.
//!     `-k` always takes `KEY=VALUE`.
//!   * daydumb does NOT parse `.key=value` as complex-state dot syntax. A
//!     leading-dot key is stored verbatim in `extra_data`. Structured states
//!     are not synthesized from positionals.
//!   * `switch <field>` with no state value stores `value=None` — daydumb does
//!     not default to `true` the way datum did. State compilation yields
//!     `Null` at that point rather than `true`.
//!
//! The A6 bead (da-1mof) ports the `--last-state` dedup and `--moment`
//! behaviors in its own test module. P5 covers the remaining switch surface
//! and re-asserts the A6-scoped behaviors via the P0 CLI harness at the
//! command-dispatch layer.

mod common;

use common::run_cli;
use datum_core::action::Action;
use datum_core::entry::Verb;
use datum_core::test_util::{fresh_db, spec_builder};
use datum_core::{assert_count, assert_entry_matches};
use serde_json::json;

/// TS #1: "creates an occur document with a custom state".
///
/// `switch field newState` produces a single `Verb::Switch` entry with
/// `value="newState"` and `time_start` set to ~now. State compilation at
/// `time_start` resolves to `"newState"`.
#[test]
fn switch_creates_entry_with_state() {
    let db = fresh_db();

    run_cli(&["switch", "field", "newState"], &db).expect("switch dispatch");

    assert_count!(db, "field", 1);
    let entry = &db.entries_for_field("field", None).unwrap()[0];
    assert_entry_matches!(entry, field: "field", verb: "switch", value: "newState");
    let ts = entry
        .occur_time
        .as_ref()
        .expect("switch anchors time_start")
        .utc;
    assert!(
        entry.dur.is_none(),
        "bare switch (no --dur, no --moment) does not derive time_end",
    );
    assert_eq!(
        db.get_active_state("field", ts).unwrap(),
        json!("newState"),
        "state at time_start is the switched-to value",
    );
}

/// TS #2: "records what it thinks the last state was".
///
/// Datum denormalized the prior state onto the switch entry
/// (`doc.data.lastState`). Daydumb does not — it relies on state compilation
/// over the entry history. The portable assertion: strictly before the
/// second switch, the first switch's value still governs.
#[test]
fn switch_prior_state_queryable_via_compilation() {
    let db = fresh_db();

    // First switch, 1 min ago so it strictly precedes the second.
    run_cli(&["switch", "environment", "outside", "-t=-1"], &db).expect("first switch");
    run_cli(&["switch", "environment", "inside"], &db).expect("second switch");

    let entries = db.entries_for_field("environment", None).unwrap();
    assert_eq!(entries.len(), 2);

    let inside = entries
        .iter()
        .find(|e| e.state.as_ref() == Some(&json!("inside")))
        .expect("inside entry present");
    let inside_ts = inside.occur_time.as_ref().unwrap().utc;
    let just_before_inside = inside_ts - chrono::Duration::seconds(1);

    assert_eq!(
        db.get_active_state("environment", just_before_inside)
            .unwrap(),
        json!("outside"),
        "prior switch value governs strictly before the new switch",
    );
    assert_eq!(
        db.get_active_state("environment", inside_ts).unwrap(),
        json!("inside"),
        "new switch value takes over at its time_start",
    );
}

/// TS #3: "can take in a manually specified last state".
///
/// Port (S6): daydumb's `-L=<value>` is a user assertion recorded in a
/// dedicated column, not a dedup guard. When the assertion matches the
/// computed prior state, no conflict is filed; when it disagrees, a
/// `last_state_mismatch` row goes into the D5 queue. The switch always
/// lands either way.
#[test]
fn switch_last_state_records_assertion_on_entry() {
    let db = fresh_db();

    run_cli(&["switch", "project", "datum", "-t=-1"], &db).expect("seed switch");
    run_cli(&["switch", "project", "household", "-L=datum"], &db).expect("-L=<value> assertion");

    let entries = db.entries_for_field("project", None).unwrap();
    assert_eq!(entries.len(), 2);
    let household = entries
        .iter()
        .find(|e| e.state.as_ref() == Some(&json!("household")))
        .expect("household entry present");
    assert_eq!(
        household.user_asserted_last_state,
        Some(json!("datum")),
        "-L=<v> records the assertion on the entry"
    );
    if let Some(extra) = household.data.as_ref() {
        assert!(
            extra.get("last_state").is_none() && extra.get("lastState").is_none(),
            "daydumb switch -L does not denormalize prior state into extra_data",
        );
    }
}

/// TS #4: "does not include a lastState if there is no occurTime".
///
/// Divergence: daydumb has no `-T` / `--omit-timestamp`. Every switch entry
/// anchors `time_start`, so the precondition of this TS case — "no
/// occurTime" — cannot arise. The portable assertion: the follow-up switch
/// still records a `time_start` and carries the boolean value as expected.
#[test]
fn switch_always_anchors_time_no_omit_timestamp_flag() {
    let db = fresh_db();

    run_cli(&["switch", "vacuum", "true"], &db).expect("first switch");
    run_cli(&["switch", "vacuum", "false"], &db).expect("second switch");

    let entries = db.entries_for_field("vacuum", None).unwrap();
    assert_eq!(entries.len(), 2);
    let falsy = entries
        .iter()
        .find(|e| e.state.as_ref() == Some(&json!(false)))
        .expect("false entry present");
    assert!(
        falsy.occur_time.is_some(),
        "daydumb always anchors time_start (no --omit-timestamp flag)",
    );
    assert_entry_matches!(falsy, field: "vacuum", verb: "switch", value: false);
}

/// TS #5: "still includes lastState if manually specified even when there is
/// no occurTime".
///
/// Divergence: daydumb has no `--omit-timestamp` flag — switch entries always
/// anchor `time_start`. The portable assertion: passing `--last-state`
/// (bare) alongside a state change still records the new entry with
/// `time_start` set; the flag is a no-op assertion-wise with no value.
#[test]
fn switch_with_last_state_still_anchors_time() {
    let db = fresh_db();

    run_cli(&["switch", "project", "linux", "-t=-1"], &db).expect("seed switch");
    run_cli(&["switch", "project", "backend", "--last-state"], &db)
        .expect("switch with --last-state");

    let backend = db
        .entries_for_field("project", None)
        .unwrap()
        .into_iter()
        .find(|e| e.state.as_ref() == Some(&json!("backend")))
        .expect("backend entry present");
    assert!(
        backend.occur_time.is_some(),
        "switch always anchors time_start; --last-state does not strip it",
    );
}

/// TS #6: "can do a state of null".
///
/// The literal string `"null"` is stored verbatim on the entry, and
/// `normalize_state` collapses both `Null` and `"null"` to `Value::Null`.
/// At query time, the active state is `null`.
#[test]
fn switch_state_null_normalizes_to_null_at_query_time() {
    let db = fresh_db();

    run_cli(&["switch", "vacuum", "null"], &db).expect("switch vacuum null");
    run_cli(&["switch", "luft", "null"], &db).expect("switch luft null");

    let vacuum = &db.entries_for_field("vacuum", None).unwrap()[0];
    let luft = &db.entries_for_field("luft", None).unwrap()[0];

    // Stored form: string "null"; the normalization lives in state compile.
    assert_entry_matches!(vacuum, verb: "switch", value: "null");
    assert_entry_matches!(luft, verb: "switch", value: "null");

    let vacuum_ts = vacuum.occur_time.as_ref().unwrap().utc;
    let luft_ts = luft.occur_time.as_ref().unwrap().utc;
    assert_eq!(
        db.get_active_state("vacuum", vacuum_ts).unwrap(),
        json!(null)
    );
    assert_eq!(db.get_active_state("luft", luft_ts).unwrap(), json!(null));
}

/// TS #7: "assumes a state of true by default".
///
/// v3 (da-5ns): switch with no explicit state promotes `state` to JSON `true`
/// at insert time. The earlier daydumb-vs-datum divergence (storing `None`)
/// was reverted by the time-shape pivot — a switch-style verb without a
/// supplied state means "this field is on / present", and storing `true`
/// keeps the renderer from having to guess.
#[test]
fn switch_without_state_promotes_to_true() {
    let db = fresh_db();

    run_cli(&["switch", "eat"], &db).expect("switch with no state");

    let entry = &db.entries_for_field("eat", None).unwrap()[0];
    assert_entry_matches!(entry, field: "eat", verb: "switch");
    assert_eq!(
        entry.state,
        Some(json!(true)),
        "null→true promotion fires for switch verbs without explicit state",
    );
    let ts = entry.occur_time.as_ref().unwrap().utc;
    assert_eq!(
        db.get_active_state("eat", ts).unwrap(),
        json!(true),
        "promoted state is observable at compile_state",
    );
}

/// TS #8: "records a block of state when specifying duration".
///
/// `switch ... --dur 10m` emits a closed block: `time_end = time_start +
/// 10m`. During the block, state is the switched-to value; at `time_end`,
/// state reverts to `false`.
#[test]
fn switch_dur_creates_forward_block_ending_false() {
    let db = fresh_db();

    run_cli(&["switch", "project", "household", "--dur", "10m"], &db).expect("switch --dur 10m");

    let entry = &db.entries_for_field("project", None).unwrap()[0];
    // v3 (da-5ns): state="household" + dur=positive → derive_verb returns Block.
    assert_entry_matches!(entry, field: "project", verb: "block", value: "household");
    assert_eq!(entry.duration_seconds(), Some(10 * 60));

    let ts = entry.occur_time.as_ref().unwrap().utc;
    let te = entry.computed_end_time().unwrap().utc;
    assert_eq!((te - ts).num_seconds(), 10 * 60);

    // Midway through the block, state is "household".
    let mid = ts + chrono::Duration::minutes(5);
    assert_eq!(
        db.get_active_state("project", mid).unwrap(),
        json!("household")
    );
    // Under v3 Block has no implicit trailing-edge event — the state at
    // time_end remains "household". A trailing transition requires
    // BlockSwitch (state + dur + following_state).
    assert_eq!(
        db.get_active_state("project", te).unwrap(),
        json!("household")
    );
}

/// TS #9: "records a hole of time in the state when given a negative duration".
///
/// Datum's negative duration produced a backward block. Under v3
/// (da-5ns), the CLI insert path clamps the time delta to zero in
/// `Entry::new_legacy` — `(end - start).num_seconds().max(0)` —
/// collapsing `--dur=-10m` to `dur="PT0S"`. With state="household" set
/// (positional → state) and dur=Zero, derive_verb returns Point. The
/// "backward hole" semantic is no longer expressible at the CLI; users
/// who want a hole must run `start <field> -t=-Nm` then a `stop`.
#[test]
fn switch_negative_dur_collapses_to_point() {
    let db = fresh_db();

    run_cli(&["switch", "project", "household", "--dur=-10m"], &db).expect("switch --dur=-10m");

    let entry = &db.entries_for_field("project", None).unwrap()[0];
    assert_entry_matches!(entry, field: "project", verb: "point", value: "household");
    assert_eq!(
        entry.duration_seconds(),
        Some(0),
        "negative dur clamps to zero seconds in Entry::new_legacy",
    );

    let ts = entry.occur_time.as_ref().unwrap().utc;
    // Under v3 Point emits Bool(false) at occur_time regardless of the
    // entry's stored `state` (carried for downstream consumers but not
    // used by the state compiler — see state::collect_events).
    assert_eq!(db.get_active_state("project", ts).unwrap(), json!(false));
}

/// TS #10: "when --moment/-m is specified, dur is null and there is no
/// duration positional argument".
///
/// Mirrors the A6 behavior (da-1mof) at the CLI layer: `--moment` /
/// `-m` produces a point-in-time entry where `time_start == time_end`.
/// Extra `-k KEY=VALUE` attaches to `extra_data` without affecting timing.
#[test]
fn switch_moment_produces_point_in_time_entry() {
    let db = fresh_db();

    run_cli(
        &[
            "switch",
            "project",
            "household",
            "-m",
            "-k",
            "skillPoints=3",
        ],
        &db,
    )
    .expect("switch --moment with -k");

    let entry = &db.entries_for_field("project", None).unwrap()[0];
    // v3 (da-5ns): --moment yields dur="PT0S"; with state="household" set
    // and dur=Zero, derive_verb returns Point.
    assert_entry_matches!(entry, field: "project", verb: "point", value: "household");
    let ts = entry.occur_time.as_ref().expect("occur_time set");
    let te = entry
        .computed_end_time()
        .expect("computed_end_time available on --moment");
    assert_eq!(
        ts.utc, te.utc,
        "--moment: occur_time == computed_end_time (point-in-time)",
    );
    let extra = entry.data.as_ref().expect("skillPoints in extra_data");
    assert_eq!(extra["skillPoints"], json!(3));
}

/// TS #11: "when --omit-timestamp/-T is specified, there is no positional
/// duration argument".
///
/// Divergence: daydumb has no `-T` / `--omit-timestamp`. The salvageable
/// assertion: `-k` keys attach cleanly to `extra_data`, and without `--dur`
/// no `time_end` is derived.
#[test]
fn switch_extras_do_not_imply_duration() {
    let db = fresh_db();

    run_cli(
        &["switch", "project", "household", "-k", "skillPoints=3"],
        &db,
    )
    .expect("switch with -k");

    let entry = &db.entries_for_field("project", None).unwrap()[0];
    assert_entry_matches!(entry, field: "project", verb: "switch", value: "household");
    assert!(
        entry.dur.is_none(),
        "no --dur ⇒ no time_end (moment-like default)",
    );
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["skillPoints"], json!(3));
}

/// TS #12: "can skip the duration if the duration is given as . or ''".
///
/// Divergence: daydumb has no positional duration slot. Duration is ONLY
/// supplied via `--dur`, so there is nothing to "skip". The portable
/// assertion: `-k KEY=VALUE` attaches without producing an implicit duration,
/// matching datum's outcome (`optional=50`, no `dur`).
#[test]
fn switch_extras_never_produce_implicit_duration() {
    let db = fresh_db();

    run_cli(
        &["switch", "project", "household", "-k", "optional=50"],
        &db,
    )
    .expect("switch with optional key");

    let entry = &db.entries_for_field("project", None).unwrap()[0];
    assert_entry_matches!(entry, field: "project", verb: "switch", value: "household");
    assert!(
        entry.dur.is_none(),
        "no --dur ⇒ no time_end; `-k KEY=VALUE` never becomes a duration",
    );
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["optional"], json!(50));
}

/// TS #13: "can make complex states by using dot syntax".
///
/// Post-da-ju71.4: leading-dot keys fold into a structured `state.*`
/// subtree. With a bare positional state present, the positional becomes
/// `state.id` and dot keys are siblings under `state`. The entry's
/// `state` is the resulting Object, not the bare scalar.
#[test]
fn switch_dot_prefixed_keys_fold_into_structured_state() {
    let db = fresh_db();

    run_cli(&["switch", "project", "household", ".subtask=mop"], &db)
        .expect("switch with .subtask=mop");

    let entry = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(entry.field, "project");
    let state = entry.state.as_ref().expect("state set");
    assert_eq!(state["id"], json!("household"));
    assert_eq!(state["subtask"], json!("mop"));
    assert!(
        entry
            .data
            .as_ref()
            .and_then(|d| d.get(".subtask"))
            .is_none(),
        "leading-dot keys no longer leak into extra_data",
    );
}

/// TS #14: "does not record a lastState if there is no occurTime".
///
/// Divergence: daydumb does not denormalize `last_state` onto switch entries
/// at all — it is always absent, not just when time is omitted. The portable
/// assertion is the stronger daydumb invariant: a follow-up switch has no
/// `last_state` key in `extra_data`, regardless of time flags.
#[test]
fn switch_does_not_auto_record_last_state_in_extra_data() {
    let db = fresh_db();

    run_cli(&["switch", "field", "someState", "-t=-1"], &db).expect("first switch");
    run_cli(&["switch", "field", "anotherState"], &db).expect("second switch");

    let entries = db.entries_for_field("field", None).unwrap();
    let second = entries
        .iter()
        .find(|e| e.state.as_ref() == Some(&json!("anotherState")))
        .expect("second entry present");
    if let Some(extra) = second.data.as_ref() {
        let obj = extra.as_object().expect("extra_data is an object");
        assert!(
            !obj.contains_key("last_state") && !obj.contains_key("lastState"),
            "daydumb does not auto-record prior state on switch entries: {extra:?}",
        );
    }
}

/// TS #15: "handles both required and optional keys for complex state
/// correctly".
///
/// Divergence: daydumb has no "required keys" concept (`-k key` without
/// `=value`) and no complex-state synthesis. The portable assertion: the
/// flat key/value surface still round-trips — multiple `-k KEY=VALUE`
/// flags attach cleanly, `--dur` produces the expected block — and none
/// of that silently mutates into a structured `state` object.
#[test]
fn switch_multiple_keys_attach_without_complex_state_synthesis() {
    let db = fresh_db();

    run_cli(
        &[
            "switch",
            "book",
            "the wind in the willows",
            "-k",
            "author=kenneth grahame",
            "-k",
            "genre=fiction",
            "--dur",
            "5m",
        ],
        &db,
    )
    .expect("switch with multiple -k keys + --dur");

    let entry = &db.entries_for_field("book", None).unwrap()[0];
    // v3: state=set + dur=positive → derive_verb returns Block.
    assert_entry_matches!(
        entry,
        field: "book",
        verb: "block",
        value: "the wind in the willows",
    );
    assert_eq!(entry.duration_seconds(), Some(5 * 60));

    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["author"], json!("kenneth grahame"));
    assert_eq!(extra["genre"], json!("fiction"));
    // Crucially: the value is the flat state string, not a synthesized
    // {id, author, genre} object.
    assert!(
        entry.state.as_ref().unwrap().is_string(),
        "switch value stays flat; daydumb does not promote keys into a composite state",
    );
}

/// TS #16: "handles dot syntax required and optional keys correctly".
///
/// Post-da-ju71.4: dot-prefixed keys fold into a structured `state.*`
/// subtree alongside the bare positional, which becomes `state.id`.
#[test]
fn switch_dot_syntax_keys_fold_into_structured_state() {
    let db = fresh_db();

    run_cli(
        &[
            "switch",
            "consume",
            "book_fiction",
            ".medium=text",
            ".title=title",
            ".author=author",
        ],
        &db,
    )
    .expect("switch with dot-prefixed keys");

    let entry = &db.entries_for_field("consume", None).unwrap()[0];
    assert_eq!(entry.field, "consume");
    let state = entry.state.as_ref().expect("state set");
    assert_eq!(state["id"], json!("book_fiction"));
    assert_eq!(state["medium"], json!("text"));
    assert_eq!(state["title"], json!("title"));
    assert_eq!(state["author"], json!("author"));
    // Leading-dot keys no longer surface as literal extra_data keys.
    if let Some(extra) = entry.data.as_ref() {
        let obj = extra.as_object().expect("data is an object");
        assert!(!obj.contains_key(".medium"));
        assert!(!obj.contains_key(".title"));
        assert!(!obj.contains_key(".author"));
    }
}

// ─── Extra parity checks (scope #4: derived-alias routing) ──────────────────

/// Acceptance criterion from the bead: derived aliases generated from a
/// field spec with `allowed_states` must route through the CLI pre-pass and
/// land on the switch code path, producing a `Verb::Switch` entry with the
/// aliased state as its value.
///
/// A6 (da-1mof) already covers the core routing path; this test exercises
/// the same routing under the P5 harness and additionally verifies that
/// `-k` flags supplied alongside the alias flow into `extra_data` (i.e.
/// alias expansion plays nicely with other log flags).
#[test]
fn derived_alias_from_spec_routes_to_switch_with_extra_keys() {
    let db = fresh_db();

    spec_builder("mood").default_verb(Action::Switch).save(&db);
    let mut spec = db.load_spec("mood").unwrap().unwrap();
    spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
    spec.regenerate_derived_aliases();
    db.save_field_spec(&spec).unwrap();

    let alias = db.lookup_alias("happy").unwrap().expect("happy alias");
    assert_eq!(alias.expands_to, "switch mood happy");

    run_cli(&["happy", "-k", "intensity=7"], &db).expect("derived alias + extra key");

    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.verb, Verb::Switch);
    assert_eq!(e.state.as_ref(), Some(&json!("happy")));
    let extra = e.data.as_ref().unwrap();
    assert_eq!(
        extra["intensity"],
        json!(7),
        "alias expansion composes cleanly with -k flags in the CLI pre-pass",
    );
}

// ─── da-ffp6: trailing positional → duration, not extra state ────────────

/// da-ffp6: `switch project ai 3` → state=ai, dur=3min. The trailing bare
/// positional is interpreted as a duration (default unit: minutes), NOT
/// silently appended to the state list as a second concurrent state.
#[test]
fn switch_trailing_bare_positional_becomes_duration_default_minutes() {
    let db = fresh_db();

    run_cli(&["switch", "project", "ai", "3"], &db)
        .expect("trailing `3` should parse as 3-minute duration");

    let entries = db.entries_for_field("project", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.field, "project");
    assert_eq!(e.state.as_ref(), Some(&json!("ai")));
    assert_eq!(
        e.duration_seconds(),
        Some(3 * 60),
        "trailing `3` is duration in minutes, not a second state",
    );
}

/// da-ffp6: `switch project ai 30m` → state=ai, dur=30min. Suffixed
/// duration form parses just like a `--dur` value.
#[test]
fn switch_trailing_positional_with_unit_suffix_becomes_duration() {
    let db = fresh_db();

    run_cli(&["switch", "project", "ai", "30m"], &db)
        .expect("trailing `30m` should parse as duration");

    let e = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(e.state.as_ref(), Some(&json!("ai")));
    assert_eq!(e.duration_seconds(), Some(30 * 60));
}

/// da-ffp6: `switch project ai 1:30` → state=ai, dur=1h30m. Colon-separated
/// HH:MM form parses just like `--dur 1:30`.
#[test]
fn switch_trailing_positional_colon_form_becomes_duration() {
    let db = fresh_db();

    run_cli(&["switch", "project", "ai", "1:30"], &db)
        .expect("trailing `1:30` should parse as 90-minute duration");

    let e = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(e.state.as_ref(), Some(&json!("ai")));
    assert_eq!(e.duration_seconds(), Some(90 * 60));
}

/// da-ffp6: `switch project ai PT30M` — ISO 8601 duration form.
#[test]
fn switch_trailing_positional_iso_duration_form() {
    let db = fresh_db();

    run_cli(&["switch", "project", "ai", "PT30M"], &db)
        .expect("trailing `PT30M` should parse as ISO duration");

    let e = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(e.duration_seconds(), Some(30 * 60));
}

/// da-ffp6: `switch project ai,b 3` → state=[ai,b], dur=3min. Comma-array
/// state still works alongside a trailing duration positional.
#[test]
fn switch_comma_array_with_trailing_duration_positional() {
    let db = fresh_db();

    run_cli(&["switch", "project", "ai,b", "3"], &db)
        .expect("array state + trailing duration should both apply");

    let e = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(
        e.state.as_ref(),
        Some(&json!(["ai", "b"])),
        "comma form still produces an array state",
    );
    assert_eq!(
        e.duration_seconds(),
        Some(3 * 60),
        "trailing positional applies as duration alongside the array state",
    );
}

/// da-ffp6: a trailing bare positional that doesn't parse as a duration is
/// rejected — this is the path that, before the fix, would silently
/// produce a 2-element state array.
#[test]
fn switch_trailing_non_duration_positional_rejected() {
    let db = fresh_db();

    let err = run_cli(&["switch", "project", "ai", "b"], &db)
        .expect_err("`b` is not a duration; whitespace-multi-state must error")
        .to_string();
    assert!(
        err.contains("not a valid duration") && err.contains("comma"),
        "error should hint at comma form: {err}",
    );
    assert_eq!(
        db.entries_for_field("project", None).unwrap().len(),
        0,
        "rejected parse must not persist any entry",
    );
}

/// da-ffp6: trailing duration positional + explicit `--dur` is a conflict
/// (better to fail loudly than to silently pick a winner).
#[test]
fn switch_trailing_duration_conflicts_with_explicit_dur_flag() {
    let db = fresh_db();

    let err = run_cli(&["switch", "project", "ai", "3", "--dur", "5m"], &db)
        .expect_err("trailing dur + --dur should conflict")
        .to_string();
    assert!(
        err.contains("--dur") || err.contains("conflict"),
        "error should mention the conflict: {err}",
    );
}

/// da-ffp6: only one trailing duration positional permitted; two is an
/// error rather than silently picking one.
#[test]
fn switch_two_trailing_duration_positionals_rejected() {
    let db = fresh_db();

    let err = run_cli(&["switch", "project", "ai", "3", "5m"], &db)
        .expect_err("two trailing duration positionals should error")
        .to_string();
    assert!(
        err.contains("multiple") || err.contains("at most one"),
        "error should call out the multiplicity: {err}",
    );
}

/// da-ffp6: KEY=VALUE positionals after the state still attach (as
/// state.* members post-da-ju71.4); they don't count as the "trailing"
/// duration slot, so a duration positional may appear before, between,
/// or after them.
#[test]
fn switch_trailing_duration_with_kv_positionals() {
    let db = fresh_db();

    run_cli(&["switch", "project", "ai", ".note=hi", "3"], &db)
        .expect("KV positional + trailing duration should both apply");

    let e = &db.entries_for_field("project", None).unwrap()[0];
    let state = e.state.as_ref().expect("state set");
    // .note folds into structured state alongside the positional `ai`.
    assert_eq!(state["id"], json!("ai"));
    assert_eq!(state["note"], json!("hi"));
    assert_eq!(e.duration_seconds(), Some(3 * 60));
}

/// da-z8o4: `switch <field> end` promotes to the `end` verb instead of
/// recording a literal state "end". Verifies the rewrite produces an
/// entry identical (modulo timing) to `end <field>`.
#[test]
fn switch_field_end_routes_through_end_verb() {
    let db = fresh_db();

    run_cli(&["switch", "project", "end"], &db).expect("switch <field> end dispatch");

    let entries = db.entries_for_field("project", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    // `end` is a CLI alias for `stop`; both produce Verb::Switch with state=false.
    assert_entry_matches!(e, field: "project", verb: "switch");
    assert_eq!(e.state.as_ref(), Some(&json!(false)));
}

/// da-z8o4: `switch <field> stop` is equivalent to `switch <field> end`.
#[test]
fn switch_field_stop_routes_through_end_verb() {
    let db = fresh_db();

    run_cli(&["switch", "project", "stop"], &db).expect("switch <field> stop dispatch");

    let entries = db.entries_for_field("project", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_entry_matches!(e, field: "project", verb: "switch");
    assert_eq!(e.state.as_ref(), Some(&json!(false)));
}

/// da-z8o4: `switch <field> start` promotes to the `start` verb
/// (state=true / block open).
#[test]
fn switch_field_start_routes_through_start_verb() {
    let db = fresh_db();

    run_cli(&["switch", "project", "start"], &db).expect("switch <field> start dispatch");

    let entries = db.entries_for_field("project", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_entry_matches!(e, field: "project", verb: "switch");
    assert_eq!(e.state.as_ref(), Some(&json!(true)));
}

/// da-z8o4: `switch <field> end` and `end <field>` produce structurally
/// identical entries (modulo timing).
#[test]
fn switch_field_end_equivalent_to_end_field() {
    let db_a = fresh_db();
    let db_b = fresh_db();

    run_cli(&["switch", "project", "end"], &db_a).expect("switch end dispatch");
    run_cli(&["end", "project"], &db_b).expect("end dispatch");

    let a = &db_a.entries_for_field("project", None).unwrap()[0];
    let b = &db_b.entries_for_field("project", None).unwrap()[0];
    assert_eq!(a.field, b.field);
    assert_eq!(a.verb, b.verb);
    assert_eq!(a.state, b.state);
    assert_eq!(a.value, b.value);
}

/// da-z8o4: a non-reserved state value after `switch <field>` still
/// writes a literal-state entry (no regression on the common path).
#[test]
fn switch_field_non_reserved_state_remains_literal() {
    let db = fresh_db();

    run_cli(&["switch", "project", "ai"], &db).expect("switch <field> <state> dispatch");

    let entries = db.entries_for_field("project", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_entry_matches!(e, field: "project", verb: "switch", value: "ai");
}
