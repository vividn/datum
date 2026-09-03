//! End-to-end test harness for `dispatch_argv` (da-1wat).
//!
//! Drives table-driven cases through the unified comma-split-first
//! dispatch path — the same path the binary uses — and asserts on the
//! resulting DB state (rows, fields, state JSON, dur, data payload).
//!
//! ## Why
//!
//! The multi-alias wiring bug landed because no test exercised the
//! binary's actual entrypoint: unit and integration coverage each
//! proved their slice in isolation, neither proved `dispatch_argv` was
//! the path the binary took. This harness closes that gap by routing
//! every case through `dispatch_argv` and asserting DB outcomes (not
//! just exit codes), so future features can land with one-line
//! additions and catch regressions at the integration boundary.
//!
//! ## Cohorts
//!
//! - **Fast cohort**: included in every `cargo test` run. Targets <30s
//!   total runtime. Polecat pre-submit (`scripts/check.sh`) gates on
//!   these.
//! - **Full cohort**: requires `--features e2e-full`. CI runs this in a
//!   dedicated job. Polecats append new full cases but do not execute
//!   the full cohort during their session.
//!
//! ## Adding a case
//!
//! Each case is a single `e2e_case!` invocation. Pick the cohort
//! (`fast` or `full`) and supply a `CaseSpec { argv, setup, expected }`:
//!
//! ```ignore
//! e2e_case!(fast, my_case: CaseSpec {
//!     argv: "switch mood happy",
//!     setup: no_setup,
//!     expected: rows([row("mood").state(json!("happy"))]),
//! });
//! ```
//!
//! See the `cases` module below for the live matrix.

#![allow(dead_code)]

use std::collections::BTreeMap;

use anyhow::Result;
use datum_commands::cmd::dispatch_argv;
use datum_core::action::Action;
use datum_core::db::Db;
use datum_core::test_util::{alias_builder, fresh_db, set_time, spec_builder, TimeGuard};
use datum_parse::alias::shell_split;
use serde_json::{json, Value as JsonValue};

// ─── Public harness types ────────────────────────────────────────────

pub type SetupFn = fn(&Db);

#[derive(Clone)]
pub struct CaseSpec {
    pub argv: &'static str,
    pub setup: SetupFn,
    pub expected: Expected,
    /// When `Some`, install a [`MockClock`] frozen at this `-t` value
    /// for the duration of the case. Without it the case uses real time
    /// (which is fine when no time-sensitive assertion is in play).
    pub freeze_time: Option<&'static str>,
}

#[derive(Clone)]
pub enum Expected {
    /// Exact set of rows expected post-dispatch. Order-independent.
    Rows(Vec<ExpectedRow>),
    /// Substring match on the surfaced error message.
    Error(&'static str),
    /// Dispatch should succeed and write no entry rows.
    NoRows,
}

#[derive(Clone, Debug, Default)]
pub struct ExpectedRow {
    pub field: &'static str,
    pub state: ExpectedState,
    /// Expected `dur` string (e.g. "PT30M", "MOMENT"). `None` ⇒ NULL.
    pub dur: Option<&'static str>,
    /// Expected (key, JSON value) pairs that must be present in the
    /// row's `data` payload. Extra keys allowed; missing or mismatched
    /// keys fail.
    pub data: BTreeMap<&'static str, JsonValue>,
}

impl ExpectedRow {
    pub fn state(mut self, s: JsonValue) -> Self {
        self.state = ExpectedState::Simple(s);
        self
    }

    pub fn no_state(mut self) -> Self {
        self.state = ExpectedState::None;
        self
    }

    pub fn dur(mut self, d: &'static str) -> Self {
        self.dur = Some(d);
        self
    }

    pub fn data(mut self, key: &'static str, v: JsonValue) -> Self {
        self.data.insert(key, v);
        self
    }
}

#[derive(Clone, Debug, Default)]
pub enum ExpectedState {
    /// state column is NULL.
    #[default]
    None,
    /// state is JSON `<v>`.
    Simple(JsonValue),
}

// ─── Helpers ──────────────────────────────────────────────────────────

pub fn no_setup(_db: &Db) {}

pub fn row(field: &'static str) -> ExpectedRow {
    ExpectedRow {
        field,
        ..Default::default()
    }
}

pub fn rows(rs: Vec<ExpectedRow>) -> Expected {
    Expected::Rows(rs)
}

pub fn err(needle: &'static str) -> Expected {
    Expected::Error(needle)
}

pub fn no_rows() -> Expected {
    Expected::NoRows
}

// ─── Driver ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RowView {
    field: String,
    state_json: Option<String>,
    dur: Option<String>,
    data_json: Option<String>,
}

fn live_rows(db: &Db) -> Vec<RowView> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT field, state, dur, data \
             FROM entries WHERE deleted_at IS NULL \
             ORDER BY rowid",
        )
        .expect("prepare live_rows query");
    stmt.query_map([], |r| {
        Ok(RowView {
            field: r.get::<_, String>(0)?,
            state_json: r.get::<_, Option<String>>(1)?,
            dur: r.get::<_, Option<String>>(2)?,
            data_json: r.get::<_, Option<String>>(3)?,
        })
    })
    .expect("execute live_rows query")
    .map(|r| r.unwrap())
    .collect()
}

pub fn run_case(name: &str, spec: &CaseSpec) {
    let db = fresh_db();
    let _time_guard: Option<TimeGuard> = spec.freeze_time.map(set_time);
    (spec.setup)(&db);

    let parts = shell_split(spec.argv);
    let mut argv = Vec::with_capacity(parts.len() + 1);
    argv.push("datum".to_string());
    argv.extend(parts);

    let result = dispatch_argv(argv, &db);
    assert_outcome(name, &db, &spec.expected, result);
}

fn assert_outcome(name: &str, db: &Db, expected: &Expected, result: Result<()>) {
    match (expected, result) {
        (Expected::Error(needle), Err(e)) => {
            let msg = format!("{e:#}");
            assert!(
                msg.to_lowercase().contains(&needle.to_lowercase()),
                "[{name}] error did not contain {needle:?}.\n  full: {msg}"
            );
        }
        (Expected::Error(needle), Ok(())) => panic!(
            "[{name}] expected error matching {needle:?}, but dispatch succeeded.\n  rows: {:?}",
            live_rows(db)
        ),
        (_, Err(e)) => panic!("[{name}] dispatch failed: {e:#}"),
        (Expected::Rows(expected), Ok(())) => assert_rows(name, db, expected),
        (Expected::NoRows, Ok(())) => {
            let rs = live_rows(db);
            assert!(
                rs.is_empty(),
                "[{name}] expected no rows, got {}: {rs:?}",
                rs.len(),
            );
        }
    }
}

fn assert_rows(name: &str, db: &Db, expected: &[ExpectedRow]) {
    let actual = live_rows(db);
    assert_eq!(
        actual.len(),
        expected.len(),
        "[{name}] row count mismatch: expected {}, got {}\n  actual: {actual:#?}",
        expected.len(),
        actual.len(),
    );
    let mut taken = vec![false; actual.len()];
    for exp in expected {
        let pos = actual
            .iter()
            .enumerate()
            .find(|(i, a)| !taken[*i] && rows_match(exp, a))
            .map(|(i, _)| i);
        match pos {
            Some(i) => taken[i] = true,
            None => panic!(
                "[{name}] no matching row for expected:\n  {exp:#?}\n  actual rows: {actual:#?}"
            ),
        }
    }
}

fn rows_match(exp: &ExpectedRow, act: &RowView) -> bool {
    if exp.field != act.field {
        return false;
    }
    match (&exp.state, &act.state_json) {
        (ExpectedState::None, None) => {}
        (ExpectedState::Simple(want), Some(got_str)) => {
            let Ok(got) = serde_json::from_str::<JsonValue>(got_str) else {
                return false;
            };
            if &got != want {
                return false;
            }
        }
        _ => return false,
    }
    if exp.dur != act.dur.as_deref() {
        return false;
    }
    if !exp.data.is_empty() {
        let data: JsonValue = act
            .data_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(JsonValue::Null);
        for (k, want) in &exp.data {
            let got = match &data {
                JsonValue::Object(m) => m.get(*k),
                _ => None,
            };
            if got != Some(want) {
                return false;
            }
        }
    }
    true
}

// ─── Case-tagging macro ───────────────────────────────────────────────

/// Define a single E2E case. Expands to a `#[test] fn` named `$name`.
/// Cohort tag (`fast` or `full`) controls whether the test runs by
/// default or requires `--features e2e-full`.
macro_rules! e2e_case {
    (fast, $name:ident : $spec:expr) => {
        #[test]
        fn $name() {
            run_case(stringify!($name), &$spec);
        }
    };
    (full, $name:ident : $spec:expr) => {
        #[test]
        #[cfg_attr(
            not(feature = "e2e-full"),
            ignore = "full cohort: enable with --features e2e-full"
        )]
        fn $name() {
            run_case(stringify!($name), &$spec);
        }
    };
}

// ─── Setup helpers used by cases ──────────────────────────────────────

fn seed_body_alias(db: &Db) {
    alias_builder("bo".to_string(), "switch body".to_string()).save(db);
}

fn seed_mood_alias(db: &Db) {
    alias_builder("md".to_string(), "switch mood".to_string()).save(db);
}

fn seed_body_and_mood_aliases(db: &Db) {
    seed_body_alias(db);
    seed_mood_alias(db);
}

fn seed_oov_alias(db: &Db) {
    // Multi-alias body: `oov` is a two-side declared multi (matches the
    // pattern exercised in `multi_alias_e2e.rs`).
    alias_builder(
        "oov".to_string(),
        "switch environment outside , switch body vertical".to_string(),
    )
    .save(db);
}

fn seed_word_spec_with_alias(db: &Db) {
    spec_builder("word".to_string())
        .default_verb(Action::Add)
        .save(db);
    alias_builder(
        "vbp".to_string(),
        "note word srcLang=pt pos=verb".to_string(),
    )
    .save(db);
}

fn seed_mood_field_spec(db: &Db) {
    spec_builder("mood".to_string())
        .default_verb(Action::Switch)
        .save(db);
}

// =====================================================================
// CASE MATRIX
// =====================================================================
//
// Each row below is a `#[test] fn`. Adding coverage is a one-line
// invocation of `e2e_case!`. Keep the matrix organized by category;
// resist the temptation to inline complex multi-step setup — for those,
// hoist into a named helper above and reference it from `setup:`.

// ─── Per-verb baseline ───────────────────────────────────────────────
//
// Every logging verb in every realistic invocation shape (direct,
// bare-alias, multi-direct, multi-alias) × no data / with data.

e2e_case!(fast, verb_occur_direct: CaseSpec {
    argv: "occur coffee",
    setup: no_setup,
    // `Action::Occur` with no explicit dur emits `Dur::moment()`
    // ("MOMENT") so the truth table classifies as `Verb::Point`.
    expected: rows(vec![row("coffee").no_state().dur("MOMENT")]),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, verb_occur_with_data: CaseSpec {
    argv: "occur coffee -k cups=2",
    setup: no_setup,
    expected: rows(vec![
        row("coffee").no_state().dur("MOMENT").data("cups", json!(2)),
    ]),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, verb_switch_direct: CaseSpec {
    argv: "switch mood happy",
    setup: no_setup,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, verb_switch_with_data: CaseSpec {
    argv: "switch mood happy -k weather=sunny",
    setup: no_setup,
    expected: rows(vec![
        row("mood").state(json!("happy")).data("weather", json!("sunny")),
    ]),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, verb_start_direct: CaseSpec {
    argv: "start workout",
    setup: no_setup,
    expected: rows(vec![row("workout").state(json!(true))]),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, verb_stop_direct: CaseSpec {
    argv: "stop workout",
    setup: no_setup,
    expected: rows(vec![row("workout").state(json!(false))]),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, verb_note_direct: CaseSpec {
    // da-r17h: `note` is the timeless shape-inferring verb (auto create_time
    // id), with the same strict positional gate (da-5ns.3) — bare positional
    // values require an accompanying `-k`/slug-key. The simplest legal form
    // is bare field, optional `key=value` extra-data. (`add` would error here
    // demanding an explicit id.)
    argv: "note foo",
    setup: no_setup,
    expected: rows(vec![row("foo").no_state()]),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, verb_note_with_data: CaseSpec {
    argv: "note foo bar=baz",
    setup: no_setup,
    expected: rows(vec![row("foo").no_state().data("bar", json!("baz"))]),
    freeze_time: Some("09:00"),
});

// Field-first form: the pre-pass swaps `<field> <verb>` to
// `<verb> <field>`.
e2e_case!(fast, verb_switch_field_first: CaseSpec {
    argv: "mood switch happy",
    setup: no_setup,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("09:00"),
});

// Bare-alias single-row form: `bo vertical` → `switch body vertical`.
e2e_case!(fast, verb_switch_bare_alias_single: CaseSpec {
    argv: "bo vertical",
    setup: seed_body_alias,
    expected: rows(vec![row("body").state(json!("vertical"))]),
    freeze_time: Some("09:00"),
});

// ─── -k key trichotomy ───────────────────────────────────────────────

e2e_case!(fast, k_key_value_pair: CaseSpec {
    argv: "occur coffee -k cups=2",
    setup: no_setup,
    expected: rows(vec![row("coffee").no_state().dur("MOMENT").data("cups", json!(2))]),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, k_key_string_value: CaseSpec {
    argv: "occur coffee -k roast=light",
    setup: no_setup,
    expected: rows(vec![row("coffee").no_state().dur("MOMENT").data("roast", json!("light"))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, k_key_multiple: CaseSpec {
    argv: "occur coffee -k cups=2 -k roast=light",
    setup: no_setup,
    expected: rows(vec![
        row("coffee").no_state().dur("MOMENT")
            .data("cups", json!(2))
            .data("roast", json!("light")),
    ]),
    freeze_time: Some("09:00"),
});

// ─── Multi-alias / multi-direct (da-gh0 Rules 1–15) ──────────────────
//
// Coverage for the comma-trigger dispatch path. These are the cases
// that broke when the binary skipped MULTI_COMMA dispatch — the entire
// motivation for this harness.

// Rule 1 — standalone `,` is a top-level multi trigger.
e2e_case!(fast, multi_direct_two_fields: CaseSpec {
    argv: "switch environment outside , switch body vertical",
    setup: no_setup,
    expected: rows(vec![
        row("environment").state(json!("outside")),
        row("body").state(json!("vertical")),
    ]),
    freeze_time: Some("09:00"),
});

// Rule — same-field two switches collapse to JSON array (parallel).
e2e_case!(fast, multi_same_field_parallel_array: CaseSpec {
    argv: "switch body vertical , switch body sit",
    setup: no_setup,
    expected: rows(vec![row("body").state(json!(["vertical", "sit"]))]),
    freeze_time: Some("09:00"),
});

// Rule 9 — repeated same-field same-state collapses to single state.
e2e_case!(fast, multi_same_field_same_state_collapses: CaseSpec {
    argv: "switch body vertical , switch body vertical",
    setup: no_setup,
    expected: rows(vec![row("body").state(json!("vertical"))]),
    freeze_time: Some("09:00"),
});

// Bare-alias multi: `bo vertical , bo sit` → parallel via alias.
e2e_case!(fast, multi_bare_alias_parallel: CaseSpec {
    argv: "bo vertical , bo sit",
    setup: seed_body_alias,
    expected: rows(vec![row("body").state(json!(["vertical", "sit"]))]),
    freeze_time: Some("09:00"),
});

// Multi-alias body expansion: `oov` is a fully bound two-side multi.
// This is the exact case the original wiring bug missed — the binary
// must route through `dispatch_argv`'s multi path, not the single
// path, for the comma-split inside the alias body to fire.
e2e_case!(fast, multi_alias_body_oov: CaseSpec {
    argv: "oov",
    setup: seed_oov_alias,
    expected: rows(vec![
        row("environment").state(json!("outside")),
        row("body").state(json!("vertical")),
    ]),
    freeze_time: Some("09:00"),
});

// Three-side multi. Positional state values are JSON-coerced: bare
// digits land as numbers, not strings, so the expectation mirrors the
// `switch` verb's actual storage shape.
e2e_case!(full, multi_three_sides: CaseSpec {
    argv: "switch a 1 , switch b 2 , switch c 3",
    setup: no_setup,
    expected: rows(vec![
        row("a").state(json!(1)),
        row("b").state(json!(2)),
        row("c").state(json!(3)),
    ]),
    freeze_time: Some("09:00"),
});

// Same-field with three distinct states → JSON array.
e2e_case!(full, multi_same_field_three_states: CaseSpec {
    argv: "switch body vertical , switch body sit , switch body supine",
    setup: no_setup,
    expected: rows(vec![row("body").state(json!(["vertical", "sit", "supine"]))]),
    freeze_time: Some("09:00"),
});

// Cross-verb multi: switch + occur in one comma-split. The occur side
// carries `dur=MOMENT` from Action::Occur's auto-fill, while the switch
// side has no dur.
e2e_case!(full, multi_cross_verb: CaseSpec {
    argv: "switch mood happy , occur coffee",
    setup: no_setup,
    expected: rows(vec![
        row("mood").state(json!("happy")),
        row("coffee").no_state().dur("MOMENT"),
    ]),
    freeze_time: Some("09:00"),
});

// Multi-alias with bare-alias on each side.
e2e_case!(full, multi_bare_alias_two_fields: CaseSpec {
    argv: "bo vertical , md happy",
    setup: seed_body_and_mood_aliases,
    expected: rows(vec![
        row("body").state(json!("vertical")),
        row("mood").state(json!("happy")),
    ]),
    freeze_time: Some("09:00"),
});

// ─── Time-flag scoping (da-gh0.D) ────────────────────────────────────

// `-q` (5min ago) — single row, time-flag applies cleanly.
e2e_case!(fast, time_q_minus_five: CaseSpec {
    argv: "switch mood happy -q",
    setup: no_setup,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("09:00"),
});

// `-y` (yesterday) — also single row.
e2e_case!(fast, time_y_yesterday: CaseSpec {
    argv: "switch mood happy -y",
    setup: no_setup,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("09:00"),
});

// Per-side `-t` across distinct fields: each side picks its own time,
// the multi dispatch records the rows under the leftmost side's
// occur_time (current behavior pending da-gh0.D side-scoping). Both
// rows live regardless because the fields differ.
e2e_case!(full, time_distinct_fields_multi: CaseSpec {
    argv: "switch environment outside -t 08:00 , switch body vertical -t 09:00",
    setup: no_setup,
    expected: rows(vec![
        row("environment").state(json!("outside")),
        row("body").state(json!("vertical")),
    ]),
    freeze_time: Some("12:00"),
});

// ─── Structured / parallel state ─────────────────────────────────────

// Two distinct fields with shared occur_time — atomic commit.
e2e_case!(fast, struct_two_fields_shared_time: CaseSpec {
    argv: "switch environment outside , switch body vertical",
    setup: no_setup,
    expected: rows(vec![
        row("environment").state(json!("outside")),
        row("body").state(json!("vertical")),
    ]),
    freeze_time: Some("09:00"),
});

// ─── Error paths ─────────────────────────────────────────────────────

// Empty side after a comma is rejected.
e2e_case!(fast, err_trailing_comma_empty_side: CaseSpec {
    argv: "switch body vertical ,",
    setup: no_setup,
    expected: err("empty"),
    freeze_time: Some("09:00"),
});

// Standalone comma with nothing on either side.
e2e_case!(full, err_only_comma: CaseSpec {
    argv: ",",
    setup: no_setup,
    expected: err("empty"),
    freeze_time: Some("09:00"),
});

// Unknown verb at top level.
e2e_case!(full, err_unknown_verb: CaseSpec {
    argv: "florpify mood happy",
    setup: no_setup,
    expected: err("unknown"),
    freeze_time: Some("09:00"),
});

// Empty argv: bare `datum` (with no subcommand) is the REPL trigger in
// `main()`, but here we drop straight into dispatch. The parser must
// surface a non-success outcome rather than silently writing a row.
// (Specifically `switch mood` would promote to `start mood`, so we
// pick a verb that has a clearer hard-required positional gap.)
e2e_case!(full, err_block_missing_dur: CaseSpec {
    argv: "block workout",
    setup: no_setup,
    expected: err(""),
    freeze_time: Some("09:00"),
});

// ─── Admin / non-logging verbs ───────────────────────────────────────
//
// These don't write entry rows; they print output. The harness asserts
// dispatch succeeds without raising. (Render coverage lives in
// goldens; e2e just confirms wiring.)

e2e_case!(fast, admin_now: CaseSpec {
    argv: "now",
    setup: no_setup,
    expected: no_rows(),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, admin_spec_list: CaseSpec {
    argv: "spec list",
    setup: seed_mood_field_spec,
    // `spec list` is a read; the seeded spec row lives in field_specs,
    // not in `entries`, so the entries table stays empty.
    expected: no_rows(),
    freeze_time: Some("09:00"),
});

e2e_case!(fast, admin_alias_list: CaseSpec {
    argv: "alias list",
    setup: seed_body_alias,
    expected: no_rows(),
    freeze_time: Some("09:00"),
});

e2e_case!(full, admin_tail_empty: CaseSpec {
    argv: "tail",
    setup: no_setup,
    expected: no_rows(),
    freeze_time: Some("09:00"),
});

e2e_case!(full, admin_get_unknown: CaseSpec {
    // `get` with no slug; admin commands surface their own error.
    argv: "get",
    setup: no_setup,
    expected: err(""),
    freeze_time: Some("09:00"),
});

// ─── Alias / pre-pass paths ──────────────────────────────────────────

// `vbp` alias from CLAUDE.md example: `note word srcLang=pt pos=verb`.
// `add`/`note` enforces the v=20 strict positional gate (da-5ns.3), so
// the alias body alone is what the harness exercises here (no trailing
// positional value — pure key=val extra-data + the bare field).
e2e_case!(fast, alias_vbp_expansion: CaseSpec {
    argv: "vbp",
    setup: seed_word_spec_with_alias,
    expected: rows(vec![
        row("word").no_state()
            .data("srcLang", json!("pt"))
            .data("pos", json!("verb")),
    ]),
    freeze_time: Some("09:00"),
});

// ─── Extended full cohort ────────────────────────────────────────────
//
// Bulk coverage for the per-verb × invocation-shape matrix from the
// bead body. These run only with `--features e2e-full` so polecat
// pre-submit stays fast.

e2e_case!(full, verb_occur_field_first: CaseSpec {
    argv: "coffee occur",
    setup: no_setup,
    expected: rows(vec![row("coffee").no_state().dur("MOMENT")]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, verb_start_field_first: CaseSpec {
    argv: "workout start",
    setup: no_setup,
    expected: rows(vec![row("workout").state(json!(true))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, verb_stop_field_first: CaseSpec {
    argv: "workout stop",
    setup: no_setup,
    expected: rows(vec![row("workout").state(json!(false))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, bare_alias_switch: CaseSpec {
    argv: "md happy",
    setup: seed_mood_alias,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, multi_four_sides: CaseSpec {
    argv: "switch a x , switch b y , switch c z , switch d w",
    setup: no_setup,
    expected: rows(vec![
        row("a").state(json!("x")),
        row("b").state(json!("y")),
        row("c").state(json!("z")),
        row("d").state(json!("w")),
    ]),
    freeze_time: Some("09:00"),
});

// Same-field same-state across THREE rows collapses to single state.
e2e_case!(full, multi_same_field_state_three_collapse: CaseSpec {
    argv: "switch body sit , switch body sit , switch body sit",
    setup: no_setup,
    expected: rows(vec![row("body").state(json!("sit"))]),
    freeze_time: Some("09:00"),
});

// Mixed: two distinct states then a repeat — `vertical`,`sit`,`sit`
// stays as a 3-element parallel array. Same-state collapse (Rule 9)
// only fires when *every* side carries the same state.
e2e_case!(full, multi_same_field_repeat_collapse: CaseSpec {
    argv: "switch body vertical , switch body sit , switch body sit",
    setup: no_setup,
    expected: rows(vec![row("body").state(json!(["vertical", "sit", "sit"]))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, multi_data_on_one_side: CaseSpec {
    argv: "switch mood happy -k weather=sunny , occur coffee",
    setup: no_setup,
    expected: rows(vec![
        row("mood").state(json!("happy")).data("weather", json!("sunny")),
        row("coffee").no_state().dur("MOMENT"),
    ]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, time_qq_ten_min: CaseSpec {
    argv: "switch mood happy -qq",
    setup: no_setup,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, time_yyy_three_days: CaseSpec {
    argv: "switch mood happy -yyy",
    setup: no_setup,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, time_explicit_hhmm: CaseSpec {
    argv: "switch mood happy -t 14:30",
    setup: no_setup,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("16:00"),
});

e2e_case!(full, time_d_yesterday: CaseSpec {
    argv: "switch mood happy -d -1",
    setup: no_setup,
    expected: rows(vec![row("mood").state(json!("happy"))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, switch_numeric_value: CaseSpec {
    argv: "switch count 42",
    setup: no_setup,
    expected: rows(vec![row("count").state(json!(42))]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, switch_float_value: CaseSpec {
    argv: "switch weight 75.2",
    setup: no_setup,
    expected: rows(vec![row("weight").state(json!(75.2))]),
    freeze_time: Some("09:00"),
});

// Unit-annotated values parse to {"v": <n>, "u": "<unit>"} per the data
// model. `occur` carries dur=MOMENT.
e2e_case!(full, occur_with_unit_annotated_data: CaseSpec {
    argv: "occur weight -k value=75.2kg",
    setup: no_setup,
    expected: rows(vec![
        row("weight").no_state().dur("MOMENT").data(
            "value",
            json!({"v": 75.2, "u": "kg"}),
        ),
    ]),
    freeze_time: Some("09:00"),
});

e2e_case!(full, k_multi_values_per_call: CaseSpec {
    argv: "occur food -k meal=breakfast -k calories=350",
    setup: no_setup,
    expected: rows(vec![
        row("food").no_state().dur("MOMENT")
            .data("meal", json!("breakfast"))
            .data("calories", json!(350)),
    ]),
    freeze_time: Some("09:00"),
});

// Bare-field invocation is *not* auto-routed to the field-spec's
// `default_action` at this layer — the user must either supply the
// verb (`mood switch happy`) or install an alias for the bare token.
e2e_case!(full, fieldspec_bare_is_not_routed: CaseSpec {
    argv: "mood happy",
    setup: seed_mood_field_spec,
    expected: err("unknown"),
    freeze_time: Some("09:00"),
});

e2e_case!(full, err_double_comma: CaseSpec {
    argv: "switch body vertical ,, switch body sit",
    setup: no_setup,
    expected: err(""),
    freeze_time: Some("09:00"),
});

e2e_case!(full, err_alias_unknown: CaseSpec {
    argv: "qqqqqzzzzz",
    setup: no_setup,
    expected: err(""),
    freeze_time: Some("09:00"),
});

e2e_case!(full, admin_spec_list_empty: CaseSpec {
    argv: "spec list",
    setup: no_setup,
    expected: no_rows(),
    freeze_time: Some("09:00"),
});

e2e_case!(full, admin_alias_set_then_list: CaseSpec {
    argv: "alias set tst \"switch body\"",
    setup: no_setup,
    expected: no_rows(),
    freeze_time: Some("09:00"),
});
