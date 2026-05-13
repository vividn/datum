//! Visual-parity fixtures — the 20 acceptance cases from §11 of
//! `docs/visual-parity-analysis.md`, revised by `da-fck`.
//!
//! Each fixture synthesizes an [`Entry`] + [`RenderContext`] that mirrors the
//! CLI invocation listed in the spec, calls the new renderer directly, and
//! asserts on the ANSI-stripped info line. A smaller subset also asserts on
//! the *colour-bearing* form to prove MD5/YIQ + truecolor escapes are
//! emitted where expected.
//!
//! Per da-25l, the state-transition group is `[LEAD][state cell][TRAIL]`
//! flush against itself with no whitespace inside. LEAD/TRAIL are single
//! bg-coloured spaces; when there's no prior or continuing tracked state,
//! the slot renders with the IGNORED grey rather than the old `∅`-glyph
//! fallback (semantically, "no prior state" IS a state — it's IGNORED).
//! Only an explicitly-stopped trail (`Bool(false)`) renders as an
//! uncoloured space. These fixtures pin the layout (state-cell body,
//! `¢`/dot glyphs, separator counts) without asserting on exact colour
//! bytes — that's the visual-tuning surface.

use chrono::{DateTime, TimeZone, Utc};
use datum_commands::render::{
    action::{format_info_line, RenderContext},
    colors::{strip_ansi, visible_width},
};
use datum_core::{
    entry::{Entry, Verb},
    time::DatumTime,
};
use serde_json::json;
use std::sync::Mutex;

// ─── Environment guard ───────────────────────────────────────────────────────
// `cargo test` threads share a process, so env-var flips for one test could
// bleed into another. A shared Mutex serialises the few tests that flip
// `DATUM_COLOR` / `NO_COLOR`. Each guard restores the prior value on drop.

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct ScopedEnv {
    key: &'static str,
    prior: Option<std::ffi::OsString>,
}
impl ScopedEnv {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let prior = std::env::var_os(key);
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
        Self { key, prior }
    }
}
impl Drop for ScopedEnv {
    fn drop(&mut self) {
        match self.prior.take() {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

/// Scope that forces colours off so ANSI-stripped assertions don't depend on
/// whether the test binary happens to have a TTY.
fn no_color_scope() -> (std::sync::MutexGuard<'static, ()>, ScopedEnv, ScopedEnv) {
    let g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let d = ScopedEnv::set("DATUM_COLOR", Some("0"));
    let n = ScopedEnv::set("NO_COLOR", Some("1"));
    (g, d, n)
}

/// Scope that forces colours on.
fn color_scope() -> (std::sync::MutexGuard<'static, ()>, ScopedEnv, ScopedEnv) {
    let g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let d = ScopedEnv::set("DATUM_COLOR", Some("1"));
    let n = ScopedEnv::set("NO_COLOR", None);
    (g, d, n)
}

// ─── Entry-construction helpers ──────────────────────────────────────────────

/// Fixed timestamp — 2026-04-22T17:00:00Z. Tests only care about relative
/// ordering and never the wall clock, so hard-coding keeps output stable.
fn fixed_time(h: u32, m: u32, s: u32) -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 4, 22, h, m, s).unwrap();
    DatumTime {
        utc,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    }
}

fn future_time(h: u32, m: u32) -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(2099, 1, 1, h, m, 0).unwrap();
    DatumTime {
        utc,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    }
}

fn entry(
    field: &str,
    verb: Verb,
    value: Option<serde_json::Value>,
    start: Option<DatumTime>,
    end: Option<DatumTime>,
) -> Entry {
    Entry::new_legacy(
        field.to_string(),
        verb,
        value,
        None,
        start,
        end,
        "test-device".to_string(),
    )
}

/// Compact constructor for a `RenderContext` — most fixtures only need to
/// pin a few slots, so default the rest.
fn ctx_with(
    lead: Option<serde_json::Value>,
    trail: Option<serde_json::Value>,
    is_first: bool,
) -> RenderContext {
    RenderContext {
        lead_state: lead,
        trail_state: trail,
        is_first_occurrence: is_first,
        ongoing_state: None,
    }
}

// ─── Fixture 1 — `datum add new_occur` ────────────────────────────────────────

#[test]
fn fx01_add_new_occur() {
    let _g = no_color_scope();
    // da-8y2q.2: legacy `Verb::Add` collapsed into `Verb::Point`. Time-stamped
    // adds now render as dots (matching the rest of the Point family); the
    // `¢`/NON_OCCUR glyph is reserved for timeless entries (no `occur_time`).
    let e = entry(
        "new_occur",
        Verb::Point,
        None,
        Some(fixed_time(16, 58, 47)),
        None,
    );
    let ctx = ctx_with(None, None, true);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains(" new_occur "), "field badge present: {out:?}");
    assert!(out.contains("●"), "point dot: {out:?}");
    assert!(
        !out.contains("¢"),
        "time-stamped Point renders as dot, not NON_OCCUR: {out:?}"
    );
}

#[test]
fn fx02_occur_first() {
    let _g = no_color_scope();
    let e = entry(
        "new_occur",
        Verb::Point,
        None,
        Some(fixed_time(16, 58, 56)),
        None,
    );
    let ctx = ctx_with(None, None, true);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("∅"), "first-tracked marker: {out:?}");
    assert!(out.contains("●"), "point dot: {out:?}");
    let null_pos = out.find('∅').unwrap();
    let dot_pos = out.find('●').unwrap();
    assert!(null_pos < dot_pos, "∅ precedes dot: {out:?}");
}

#[test]
fn fx03_occur_again() {
    let _g = no_color_scope();
    let e = entry(
        "new_occur",
        Verb::Point,
        None,
        Some(fixed_time(16, 58, 59)),
        None,
    );
    let ctx = ctx_with(None, None, false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(!out.contains("∅"), "repeat occur has no ∅: {out:?}");
    assert!(out.contains("●"), "still has dot: {out:?}");
}

#[test]
fn fx04_start_first() {
    let _g = no_color_scope();
    // da-8y2q.2: legacy `Verb::Start` collapsed into `Verb::Switch` with
    // canonicalized `state=true`. The "start" label flows from
    // `render_switch_badge`'s `Bool(true)` arm, so the value must be set.
    let e = entry(
        "new_start",
        Verb::Switch,
        Some(json!(true)),
        Some(fixed_time(16, 59, 40)),
        None,
    );
    // First-time start: no prior, trail = Bool(true).
    let ctx = ctx_with(None, Some(json!(true)), true);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(
        !out.contains("∅"),
        "no ∅ glyph in LEAD slot post-da-25l: {out:?}"
    );
    assert!(out.contains("start"), "state label 'start': {out:?}");
    assert!(!out.contains("→"), "no right-arrow: {out:?}");
    assert!(!out.contains("←"), "no left-arrow: {out:?}");
}

#[test]
fn fx05_end() {
    let _g = no_color_scope();
    // da-8y2q.2: legacy `Verb::Stop` collapsed into `Verb::Switch` with
    // canonicalized `state=false`.
    let e = entry(
        "new_start",
        Verb::Switch,
        Some(json!(false)),
        Some(fixed_time(16, 59, 46)),
        None,
    );
    // Prior state = true (active block); trail = false (stopped).
    let ctx = ctx_with(Some(json!(true)), Some(json!(false)), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("end"), "state label 'end': {out:?}");
    assert!(!out.contains("→"), "no arrows: {out:?}");
    assert!(!out.contains("←"), "no arrows: {out:?}");
}

#[test]
fn fx06_start_after_end() {
    let _g = no_color_scope();
    let e = entry(
        "new_start",
        Verb::Switch,
        Some(json!(true)),
        Some(fixed_time(16, 59, 52)),
        None,
    );
    // Prior = false (stopped); trail = true.
    let ctx = ctx_with(Some(json!(false)), Some(json!(true)), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(
        !out.contains("∅"),
        "no first-marker (Bool(false) is a state): {out:?}"
    );
    assert!(!out.contains("!"), "no warning glyph: {out:?}");
    assert!(out.contains("start"), "'start' label: {out:?}");
    assert!(!out.contains("→"), "no arrows: {out:?}");
}

#[test]
fn fx07_switch_first() {
    let _g = no_color_scope();
    let e = entry(
        "new_switch",
        Verb::Switch,
        Some(json!("state1")),
        Some(fixed_time(17, 0, 21)),
        None,
    );
    let ctx = ctx_with(None, Some(json!("state1")), true);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(
        !out.contains("∅"),
        "no ∅ glyph in LEAD slot post-da-25l: {out:?}"
    );
    assert!(out.contains("state1"), "new state label: {out:?}");
    assert!(!out.contains("→"), "no arrows: {out:?}");
}

#[test]
fn fx08_switch_prior_scalar() {
    let _g = no_color_scope();
    let e = entry(
        "new_switch",
        Verb::Switch,
        Some(json!("state2")),
        Some(fixed_time(17, 0, 30)),
        None,
    );
    let ctx = ctx_with(Some(json!("state1")), Some(json!("state2")), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(!out.contains("←"), "no left-arrow in new layout: {out:?}");
    assert!(out.contains("state2"), "new state: {out:?}");
    assert!(!out.contains("→"), "no right-arrow: {out:?}");
}

#[test]
fn fx09_switch_chain() {
    let _g = no_color_scope();
    let e = entry(
        "new_switch",
        Verb::Switch,
        Some(json!("state3")),
        Some(fixed_time(17, 0, 38)),
        None,
    );
    let ctx = ctx_with(Some(json!("state2")), Some(json!("state3")), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("state3"));
    assert!(!out.contains("←"));
    assert!(!out.contains("→"));
}

#[test]
fn fx10_switch_to_repeat_label() {
    let _g = no_color_scope();
    let e = entry(
        "new_switch",
        Verb::Switch,
        Some(json!("repeat")),
        Some(fixed_time(17, 0, 52)),
        None,
    );
    let ctx = ctx_with(Some(json!("state3")), Some(json!("repeat")), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("repeat"));
    assert!(!out.contains("→"));
}

#[test]
fn fx11_switch_same_state_repeat() {
    let _g = no_color_scope();
    let e = entry(
        "new_switch",
        Verb::Switch,
        Some(json!("repeat")),
        Some(fixed_time(17, 0, 55)),
        None,
    );
    // Same state both sides — the new layout flags this as a single
    // continuous bg-coloured strip; no '!' glyph.
    let ctx = ctx_with(Some(json!("repeat")), Some(json!("repeat")), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(
        !out.contains("!"),
        "no warning glyph in new layout: {out:?}"
    );
    assert!(out.contains("repeat"));
}

#[test]
fn fx12_end_switch() {
    let _g = no_color_scope();
    // da-8y2q.2: legacy `Verb::Stop` collapsed into `Verb::Switch` + state=false.
    let e = entry(
        "new_switch",
        Verb::Switch,
        Some(json!(false)),
        Some(fixed_time(17, 1, 13)),
        None,
    );
    let ctx = ctx_with(Some(json!("repeat")), Some(json!(false)), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("end"));
    assert!(!out.contains("←"));
}

#[test]
fn fx13_start_already_active() {
    let _g = no_color_scope();
    let e = entry(
        "repeat_start",
        Verb::Switch,
        None,
        Some(fixed_time(17, 2, 0)),
        None,
    );
    // Prior already true; new layout has no '!' glyph.
    let ctx = ctx_with(Some(json!(true)), Some(json!(true)), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(
        !out.contains("!"),
        "no warning glyph in new layout: {out:?}"
    );
    assert!(out.contains("start"));
}

#[test]
fn fx14_switch_with_dur_first() {
    let _g = no_color_scope();
    let e = entry(
        "dur_blocks",
        Verb::Switch,
        Some(json!("state1")),
        Some(fixed_time(17, 2, 26)),
        None,
    );
    let ctx = ctx_with(None, Some(json!("state1")), true);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(
        !out.contains("∅"),
        "no ∅ glyph in LEAD slot post-da-25l: {out:?}"
    );
    assert!(out.contains("state1"));
}

#[test]
fn fx15_switch_with_duration_block() {
    let _g = no_color_scope();
    // 4-minute switch at 17:10 — start=17:10:00, end=17:14:00. After the
    // block ends, prior 'state1' resumes.
    let start = fixed_time(17, 10, 0);
    let end = fixed_time(17, 14, 0);
    let e = entry(
        "dur_blocks",
        Verb::Switch,
        Some(json!("state2")),
        Some(start),
        Some(end),
    );
    let ctx = ctx_with(Some(json!("state1")), Some(json!("state1")), false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("state2"));
    assert!(out.contains("4m"), "duration present: {out:?}");
    assert!(!out.contains("←"));
    assert!(!out.contains("→"));
}

#[test]
fn fx16_switch_array_state() {
    let _g = no_color_scope();
    let e = entry(
        "field",
        Verb::Switch,
        Some(json!(["multi1", "multi2"])),
        Some(fixed_time(17, 12, 13)),
        None,
    );
    let ctx = ctx_with(
        Some(json!("prior")),
        Some(json!(["multi1", "multi2"])),
        false,
    );
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("multi1"));
    assert!(out.contains(","), "comma separator: {out:?}");
    assert!(out.contains("multi2"));
    assert!(!out.contains("→"));
}

#[test]
fn fx17_switch_scalar_with_array_prior_and_dur() {
    let _g = no_color_scope();
    // After a 1-minute scalar block on top of an array-state field, the
    // array state resumes.
    let start = future_time(17, 15);
    let end = future_time(17, 16);
    let e = entry(
        "field",
        Verb::Switch,
        Some(json!("momentary_single")),
        Some(start),
        Some(end),
    );
    let ctx = ctx_with(
        Some(json!(["multi1", "multi2"])),
        Some(json!(["multi1", "multi2"])),
        false,
    );
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("momentary_single"));
    assert!(out.contains("1m"), "duration: {out:?}");
    // Per da-9vop, parallel-state LEAD/TRAIL slots render as `▞`
    // checkerboards so both member colours stay visible. The scalar state
    // cell itself ("momentary_single") is still solid bg-painted text —
    // verify by checking the slots wrap the badge directly with `▞`.
    assert!(
        out.contains("▞momentary_single▞"),
        "parallel-state slots wrap scalar badge: {out:?}",
    );
}

#[test]
fn fx18_ignore_renders_grey() {
    let _g = no_color_scope();
    let e = entry(
        "foo",
        Verb::Switch,
        Some(json!("IGNORED")),
        Some(fixed_time(17, 30, 0)),
        None,
    );
    // Trail = IGNORED → IGNORED-bg slot (no ∅ glyph post-da-25l).
    let ctx = ctx_with(None, Some(json!("IGNORED")), true);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("IGNORED"), "state label present: {out:?}");
    assert!(out.contains(" foo "), "field badge: {out:?}");
    assert!(
        !out.contains("∅"),
        "no ∅ glyph in slots post-da-25l: {out:?}"
    );
}

#[test]
fn fx19_track_false_state() {
    let _g = no_color_scope();
    // `datum track foo` writes switch with value=false.
    let e = entry(
        "foo",
        Verb::Switch,
        Some(json!(false)),
        Some(fixed_time(17, 30, 0)),
        None,
    );
    let ctx = ctx_with(None, Some(json!(false)), true);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains(" foo "), "field badge: {out:?}");
    assert!(out.contains("end"), "false renders as 'end' label: {out:?}");
}

#[test]
fn fx20_add_with_keys() {
    let _g = no_color_scope();
    // da-8y2q.2: legacy `Verb::Add` collapsed into `Verb::Point`. Time-stamped
    // adds render as dots, not the NON_OCCUR (`¢`) glyph (which is now
    // reserved for timeless entries — `occur_time = None`).
    let mut e = entry(
        "thing",
        Verb::Point,
        None,
        Some(fixed_time(17, 40, 0)),
        None,
    );
    e.data = Some(json!({ "key": "value" }));
    let ctx = ctx_with(None, None, false);
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains(" thing "), "field badge: {out:?}");
    assert!(out.contains("●"), "point dot (time-stamped): {out:?}");
    assert!(!out.contains("∅"), "repeat point has no ∅: {out:?}");
}

// ─── Cross-cutting: no arrow glyphs anywhere ─────────────────────────────────

/// The post-da-fck contract: no directional arrows in any rendered row.
#[test]
fn no_directional_arrows_anywhere() {
    let _g = no_color_scope();
    let cases: Vec<(Entry, RenderContext)> = vec![
        // start, first
        (
            entry("a", Verb::Switch, None, Some(fixed_time(9, 0, 0)), None),
            ctx_with(None, Some(json!(true)), true),
        ),
        // start, repeat
        (
            entry("a", Verb::Switch, None, Some(fixed_time(9, 1, 0)), None),
            ctx_with(Some(json!(true)), Some(json!(true)), false),
        ),
        // stop after running
        (
            entry("a", Verb::Switch, None, Some(fixed_time(9, 2, 0)), None),
            ctx_with(Some(json!(true)), Some(json!(false)), false),
        ),
        // switch with prior
        (
            entry(
                "a",
                Verb::Switch,
                Some(json!("on")),
                Some(fixed_time(9, 3, 0)),
                None,
            ),
            ctx_with(Some(json!("off")), Some(json!("on")), false),
        ),
        // switch block (4m duration), prior resumes
        (
            entry(
                "a",
                Verb::Switch,
                Some(json!("on")),
                Some(fixed_time(9, 5, 0)),
                Some(fixed_time(9, 9, 0)),
            ),
            ctx_with(Some(json!("off")), Some(json!("off")), false),
        ),
    ];
    for (e, ctx) in &cases {
        let out = strip_ansi(&format_info_line(e, None, ctx));
        for arrow in ["→", "←", "↑", "↓", "▶"] {
            assert!(
                !out.contains(arrow),
                "rendered row carries {arrow:?}: {out:?}"
            );
        }
    }
}

// ─── With-color smoke tests ──────────────────────────────────────────────────

/// MD5 + YIQ drives field backgrounds: rendering with colours on must emit a
/// truecolor bg escape (`48;2;…`). Proves §1's "emit via ANSI truecolor" rule.
#[test]
fn color_mode_emits_truecolor_backgrounds() {
    let _g = color_scope();
    let e = entry(
        "coffee",
        Verb::Switch,
        Some(json!("espresso")),
        Some(fixed_time(9, 15, 0)),
        None,
    );
    let ctx = ctx_with(None, Some(json!("espresso")), true);
    let out = format_info_line(&e, None, &ctx);
    assert!(
        out.contains("\x1b[48;2;"),
        "expected truecolor bg escape: {out:?}"
    );
    assert!(
        out.contains("\x1b[38;2;"),
        "expected truecolor fg escape: {out:?}"
    );
}

/// Future timestamps get the ANSI underline escape per §3.
#[test]
fn future_timestamp_is_underlined() {
    let _g = color_scope();
    let e = entry(
        "planahead",
        Verb::Point,
        None,
        Some(future_time(10, 0)),
        None,
    );
    let ctx = ctx_with(None, None, true);
    let out = format_info_line(&e, None, &ctx);
    assert!(
        out.contains("\x1b[4m"),
        "expected underline escape: {out:?}"
    );
}

/// da-5uph: an entry with `occur_time_precision = P1D` is a date-only
/// row. `format_info_line` must lead with the bare local date — no
/// `HH:MM:SS`, no zone offset, no relative-day prefix wrapping the
/// clock — so the user sees what they logged, not a synthetic
/// `00:00:00` stamp.
#[test]
fn p1d_info_line_is_date_only() {
    let _g = no_color_scope();
    let mut e = entry(
        "birthday",
        Verb::Point,
        None,
        Some(fixed_time(0, 0, 0)),
        None,
    );
    e.occur_time_precision = Some("P1D".to_string());
    let ctx = RenderContext::default();
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(
        out.starts_with("2026-04-22"),
        "P1D info-line must start with bare date: {out:?}"
    );
    assert!(
        !out.contains("00:00:00"),
        "P1D info-line must omit clock part: {out:?}"
    );
    assert!(out.contains(" birthday "), "field badge present: {out:?}");
}

/// da-5uph: a row without `occur_time_precision` (i.e. NULL → exact-
/// second) keeps the full `HH:MM:SS` + offset rendering — the new
/// behaviour is scoped to P1D only.
#[test]
fn full_precision_info_line_keeps_clock() {
    let _g = no_color_scope();
    let e = entry("ping", Verb::Point, None, Some(fixed_time(14, 30, 0)), None);
    // precision left as None — exact-second.
    let ctx = RenderContext::default();
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(
        out.contains("14:30:00"),
        "exact-second info-line must show clock: {out:?}"
    );
}

// Manual visual inspection: see `tests/goldens/terminal/fxNN_*.ansi` —
// `cat` renders the colored bytes directly. The goldens harness in
// `tests/render_goldens.rs` (da-8qwy) covers regression detection.

/// Visible width of a rendered line ignores ANSI escapes — a sanity check
/// the fixture harness depends on.
#[test]
fn visible_width_matches_stripped_length() {
    let _g = color_scope();
    let e = entry("ping", Verb::Point, None, Some(fixed_time(1, 2, 3)), None);
    let ctx = RenderContext::default();
    let out = format_info_line(&e, None, &ctx);
    assert_eq!(visible_width(&out), strip_ansi(&out).chars().count());
}
