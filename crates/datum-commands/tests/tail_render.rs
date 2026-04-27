//! Renderer-level tests for `datum tail` (da-n1q):
//!
//! - Relative-day prefix appears on rows from a different calendar day.
//! - Today rows keep the bare `HH:MM:SS+TZ` form.
//! - The state cell goes through the canonical `format_state_group` →
//!   `datum switch`-shaped left/state/right group.
//!
//! We hit the renderer entry points directly with synthetic entries so the
//! tests don't depend on the SQLite layer or wall-clock drift.

use chrono::{DateTime, TimeZone, Utc};
use datum_commands::render::{
    colors::strip_ansi, format_info_line_at, format_state_group, RenderContext,
};
use datum_core::{
    entry::{Entry, Verb},
    time::DatumTime,
};
use serde_json::json;
use std::sync::Mutex;

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

fn no_color_scope() -> (std::sync::MutexGuard<'static, ()>, ScopedEnv, ScopedEnv) {
    let g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let d = ScopedEnv::set("DATUM_COLOR", Some("0"));
    let n = ScopedEnv::set("NO_COLOR", Some("1"));
    (g, d, n)
}

fn dt(y: i32, mo: u32, d: u32, h: u32, m: u32) -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(y, mo, d, h, m, 0).unwrap();
    DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
}

fn entry_at(field: &str, verb: Verb, value: Option<serde_json::Value>, t: DatumTime) -> Entry {
    Entry::new(field, verb, value, None, None, Some(t), None, "test-device")
}

#[test]
fn tail_today_row_has_no_date_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Occur, None, dt(2026, 4, 22, 9, 15));
    let line = strip_ansi(&format_info_line_at(&e, None, &RenderContext::default(), &now));
    assert!(line.starts_with("09:15:00"), "today entries omit the date prefix: {line:?}");
    assert!(!line.contains("-1d"));
    assert!(!line.contains("-2d"));
}

#[test]
fn tail_yesterday_row_has_minus_one_d_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Occur, None, dt(2026, 4, 21, 9, 15));
    let line = strip_ansi(&format_info_line_at(&e, None, &RenderContext::default(), &now));
    assert!(line.starts_with("-1d "), "yesterday → '-1d ' prefix: {line:?}");
}

#[test]
fn tail_three_day_old_row_uses_relative_d_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Occur, None, dt(2026, 4, 19, 9, 15));
    let line = strip_ansi(&format_info_line_at(&e, None, &RenderContext::default(), &now));
    assert!(line.starts_with("-3d "), "3 days back → '-3d ' prefix: {line:?}");
}

#[test]
fn tail_old_row_uses_iso_date_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Occur, None, dt(2024, 12, 13, 9, 15));
    let line = strip_ansi(&format_info_line_at(&e, None, &RenderContext::default(), &now));
    assert!(
        line.starts_with("2024-12-13 "),
        "rows older than 3 days → ISO-date prefix: {line:?}"
    );
}

#[test]
fn tail_future_row_uses_plus_d_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("plan", Verb::Occur, None, dt(2026, 4, 24, 9, 15));
    let line = strip_ansi(&format_info_line_at(&e, None, &RenderContext::default(), &now));
    assert!(line.starts_with("+2d "), "2 days ahead → '+2d ' prefix: {line:?}");
}

#[test]
fn tail_state_cell_uses_canonical_left_state_right_group() {
    // A `switch happy` row with prior `tired` must render the state cell
    // as `<left-arrow><state badge><right-arrow>` per §6, matching what
    // `datum switch` itself emits.
    let _g = no_color_scope();
    let e = entry_at(
        "mood",
        Verb::Switch,
        Some(json!("happy")),
        dt(2026, 4, 22, 9, 15),
    );
    let ctx = RenderContext {
        last_state: Some(json!("tired")),
        is_first_occurrence: false,
    };
    let group = format_state_group(&e, None, &ctx).expect("state group");
    let stripped = strip_ansi(&group);
    assert!(stripped.contains("←"), "left arrow from prior state: {stripped:?}");
    assert!(stripped.contains("happy"), "current state badge: {stripped:?}");
    assert!(stripped.contains("→"), "right arrow signalling continuation: {stripped:?}");
}

#[test]
fn tail_state_cell_for_start_includes_arrows() {
    // `datum start` writes a Switch with value=true. The canonical group
    // emits ` start →` (with field-color block + right arrow).
    let _g = no_color_scope();
    let e = entry_at("workout", Verb::Start, None, dt(2026, 4, 22, 9, 15));
    let ctx = RenderContext::default();
    let group = format_state_group(&e, None, &ctx).expect("state group");
    let stripped = strip_ansi(&group);
    assert!(stripped.contains("start"), "start label: {stripped:?}");
    assert!(stripped.contains("→"), "right arrow: {stripped:?}");
}
