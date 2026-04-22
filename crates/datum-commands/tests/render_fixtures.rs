//! Visual-parity fixtures — the 20 acceptance cases from §11 of
//! `docs/visual-parity-analysis.md`.
//!
//! Each fixture synthesizes an [`Entry`] + [`RenderContext`] that mirrors the
//! CLI invocation listed in the spec, calls the new renderer directly, and
//! asserts on the ANSI-stripped info line. A smaller subset also asserts on
//! the *colour-bearing* form to prove MD5/YIQ + truecolor escapes are
//! emitted where expected.
//!
//! The fixtures do **not** exercise the full command-layer path (db writes,
//! spec creation, `ensure_tracked`): that's already covered by command-level
//! tests. What these fixtures pin is the rendered string, so changes to the
//! visual layout can't sneak through without updating the spec.

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
    DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
}

fn future_time(h: u32, m: u32) -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(2099, 1, 1, h, m, 0).unwrap();
    DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
}

fn entry(
    field: &str,
    verb: Verb,
    value: Option<serde_json::Value>,
    start: Option<DatumTime>,
    end: Option<DatumTime>,
) -> Entry {
    Entry::new(
        field.to_string(),
        verb,
        value,
        None,
        None,
        start,
        end,
        "test-device".to_string(),
    )
}

// ─── Fixture 1 — `datum add new_occur` ────────────────────────────────────────

#[test]
fn fx01_add_new_occur() {
    let _g = no_color_scope();
    let e = entry("new_occur", Verb::Add, None, Some(fixed_time(16, 58, 47)), None);
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains(" new_occur "), "field badge present: {out:?}");
    assert!(out.contains("¢"), "add glyph present: {out:?}");
    assert!(!out.contains("∅"), "add must not carry ∅: {out:?}");
}

#[test]
fn fx02_occur_first() {
    let _g = no_color_scope();
    let e = entry("new_occur", Verb::Occur, None, Some(fixed_time(16, 58, 56)), None);
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
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
    let e = entry("new_occur", Verb::Occur, None, Some(fixed_time(16, 58, 59)), None);
    let ctx = RenderContext { last_state: None, is_first_occurrence: false };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(!out.contains("∅"), "repeat occur has no ∅: {out:?}");
    assert!(out.contains("●"), "still has dot: {out:?}");
}

#[test]
fn fx04_start_first() {
    let _g = no_color_scope();
    let e = entry("new_start", Verb::Start, None, Some(fixed_time(16, 59, 40)), None);
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("∅"), "first marker: {out:?}");
    assert!(out.contains("start"), "state label 'start': {out:?}");
    assert!(out.contains("→"), "right-arrow: {out:?}");
}

#[test]
fn fx05_end() {
    let _g = no_color_scope();
    let e = entry("new_start", Verb::Stop, None, Some(fixed_time(16, 59, 46)), None);
    // Prior state = true (active block)
    let ctx = RenderContext { last_state: Some(json!(true)), is_first_occurrence: false };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("←"), "left-arrow before end: {out:?}");
    assert!(out.contains("end"), "state label 'end': {out:?}");
    assert!(!out.contains("→"), "end has no right-arrow: {out:?}");
}

#[test]
fn fx06_start_after_end() {
    let _g = no_color_scope();
    let e = entry("new_start", Verb::Start, None, Some(fixed_time(16, 59, 52)), None);
    // Prior state = false (stopped)
    let ctx = RenderContext { last_state: Some(json!(false)), is_first_occurrence: false };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(!out.contains("∅"), "no first-marker: {out:?}");
    assert!(!out.contains("←"), "no left-arrow on prior=false: {out:?}");
    assert!(!out.contains("!"), "no warning: {out:?}");
    assert!(out.contains("start"), "'start' label: {out:?}");
    assert!(out.contains("→"), "right-arrow: {out:?}");
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
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("∅"), "first marker: {out:?}");
    assert!(out.contains("state1"), "new state label: {out:?}");
    assert!(out.contains("→"), "right-arrow: {out:?}");
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
    let ctx = RenderContext {
        last_state: Some(json!("state1")),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("←"), "left-arrow: {out:?}");
    assert!(out.contains("state2"), "new state: {out:?}");
    assert!(out.contains("→"), "right-arrow: {out:?}");
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
    let ctx = RenderContext {
        last_state: Some(json!("state2")),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("←"));
    assert!(out.contains("state3"));
    assert!(out.contains("→"));
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
    let ctx = RenderContext {
        last_state: Some(json!("state3")),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("←"));
    assert!(out.contains("repeat"));
    assert!(out.contains("→"));
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
    let ctx = RenderContext {
        last_state: Some(json!("repeat")),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("!"), "yellow warning: {out:?}");
    assert!(out.contains("repeat"));
    assert!(out.contains("→"));
}

#[test]
fn fx12_end_switch() {
    let _g = no_color_scope();
    let e = entry("new_switch", Verb::Stop, None, Some(fixed_time(17, 1, 13)), None);
    let ctx = RenderContext {
        last_state: Some(json!("repeat")),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("←"));
    assert!(out.contains("end"));
}

#[test]
fn fx13_start_already_active() {
    let _g = no_color_scope();
    let e = entry(
        "repeat_start",
        Verb::Start,
        None,
        Some(fixed_time(17, 2, 0)),
        None,
    );
    let ctx = RenderContext {
        last_state: Some(json!(true)),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("!"), "yellow ! for already-active: {out:?}");
    assert!(out.contains("start"));
    assert!(out.contains("→"));
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
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("∅"));
    assert!(out.contains("state1"));
    assert!(out.contains("→"));
}

#[test]
fn fx15_switch_with_duration_block() {
    let _g = no_color_scope();
    // 4-minute switch at 17:10 — start=17:10:00, end=17:14:00
    let start = fixed_time(17, 10, 0);
    let end = fixed_time(17, 14, 0);
    let e = entry(
        "dur_blocks",
        Verb::Switch,
        Some(json!("state2")),
        Some(start),
        Some(end),
    );
    let ctx = RenderContext {
        last_state: Some(json!("state1")),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("←"));
    assert!(out.contains("state2"));
    assert!(out.contains("→"));
    assert!(out.contains("4m"), "duration present: {out:?}");
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
    let ctx = RenderContext {
        last_state: Some(json!("prior")),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("←"), "left arrow from scalar prior: {out:?}");
    assert!(out.contains("multi1"));
    assert!(out.contains(","), "comma separator: {out:?}");
    assert!(out.contains("multi2"));
    assert!(out.contains("→"));
}

#[test]
fn fx17_switch_scalar_with_array_prior_and_dur() {
    let _g = no_color_scope();
    let start = future_time(17, 15);
    let end = future_time(17, 16);
    let e = entry(
        "field",
        Verb::Switch,
        Some(json!("momentary_single")),
        Some(start),
        Some(end),
    );
    let ctx = RenderContext {
        last_state: Some(json!(["multi1", "multi2"])),
        is_first_occurrence: false,
    };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("▞"), "checkerboard marker from array prior: {out:?}");
    assert!(out.contains("momentary_single"));
    assert!(out.contains("1m"), "duration: {out:?}");
    // Checkerboard should appear both before and after (left + right char).
    let count = out.matches("▞").count();
    assert!(count >= 2, "checkerboard emitted twice: {out:?}");
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
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains("IGNORED"), "state label present: {out:?}");
    assert!(out.contains(" foo "), "field badge: {out:?}");
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
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains(" foo "), "field badge: {out:?}");
    assert!(out.contains("end"), "false renders as 'end' label: {out:?}");
}

#[test]
fn fx20_add_with_keys() {
    let _g = no_color_scope();
    let mut e = entry("thing", Verb::Add, None, Some(fixed_time(17, 40, 0)), None);
    e.data = Some(json!({ "key": "value" }));
    let ctx = RenderContext { last_state: None, is_first_occurrence: false };
    let out = strip_ansi(&format_info_line(&e, None, &ctx));
    assert!(out.contains(" thing "), "field badge: {out:?}");
    assert!(out.contains("¢"), "add marker: {out:?}");
    assert!(!out.contains("∅"), "add never carries ∅: {out:?}");
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
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
    let out = format_info_line(&e, None, &ctx);
    assert!(out.contains("\x1b[48;2;"), "expected truecolor bg escape: {out:?}");
    assert!(out.contains("\x1b[38;2;"), "expected truecolor fg escape: {out:?}");
}

/// Future timestamps get the ANSI underline escape per §3.
#[test]
fn future_timestamp_is_underlined() {
    let _g = color_scope();
    let e = entry(
        "planahead",
        Verb::Occur,
        None,
        Some(future_time(10, 0)),
        None,
    );
    let ctx = RenderContext { last_state: None, is_first_occurrence: true };
    let out = format_info_line(&e, None, &ctx);
    assert!(out.contains("\x1b[4m"), "expected underline escape: {out:?}");
}

/// Visible width of a rendered line ignores ANSI escapes — a sanity check
/// the fixture harness depends on.
#[test]
fn visible_width_matches_stripped_length() {
    let _g = color_scope();
    let e = entry("ping", Verb::Occur, None, Some(fixed_time(1, 2, 3)), None);
    let ctx = RenderContext::default();
    let out = format_info_line(&e, None, &ctx);
    assert_eq!(visible_width(&out), strip_ansi(&out).chars().count());
}
