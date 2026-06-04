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
    colors::strip_ansi, format_info_line_at, format_state_group, format_table_with, RenderContext,
    TableArgs, TimeMetricCol,
};
use datum_core::{
    entry::{Entry, Verb},
    time::DatumTime,
};
use datum_test_util::no_color_scope;
use serde_json::json;

fn dt(y: i32, mo: u32, d: u32, h: u32, m: u32) -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(y, mo, d, h, m, 0).unwrap();
    DatumTime {
        utc,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    }
}

fn entry_at(field: &str, verb: Verb, value: Option<serde_json::Value>, t: DatumTime) -> Entry {
    Entry::new(field, verb, value, None, Some(t), None, "test-device")
}

fn entry_no_time(field: &str, verb: Verb, value: Option<serde_json::Value>) -> Entry {
    Entry::new(field, verb, value, None, None, None, "test-device")
}

#[test]
fn tail_today_row_has_no_date_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Point, None, dt(2026, 4, 22, 9, 15));
    let line = strip_ansi(&format_info_line_at(
        &e,
        None,
        &RenderContext::default(),
        &now,
    ));
    assert!(
        line.starts_with("09:15:00"),
        "today entries omit the date prefix: {line:?}"
    );
    assert!(!line.contains("-1d"));
    assert!(!line.contains("-2d"));
}

#[test]
fn tail_yesterday_row_has_minus_one_d_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Point, None, dt(2026, 4, 21, 9, 15));
    let line = strip_ansi(&format_info_line_at(
        &e,
        None,
        &RenderContext::default(),
        &now,
    ));
    assert!(
        line.starts_with("-1d "),
        "yesterday → '-1d ' prefix: {line:?}"
    );
}

#[test]
fn tail_three_day_old_row_uses_relative_d_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Point, None, dt(2026, 4, 19, 9, 15));
    let line = strip_ansi(&format_info_line_at(
        &e,
        None,
        &RenderContext::default(),
        &now,
    ));
    assert!(
        line.starts_with("-3d "),
        "3 days back → '-3d ' prefix: {line:?}"
    );
}

#[test]
fn tail_old_row_uses_iso_date_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Point, None, dt(2024, 12, 13, 9, 15));
    let line = strip_ansi(&format_info_line_at(
        &e,
        None,
        &RenderContext::default(),
        &now,
    ));
    assert!(
        line.starts_with("2024-12-13 "),
        "rows older than 3 days → ISO-date prefix: {line:?}"
    );
}

#[test]
fn tail_future_row_uses_plus_d_prefix() {
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("plan", Verb::Point, None, dt(2026, 4, 24, 9, 15));
    let line = strip_ansi(&format_info_line_at(
        &e,
        None,
        &RenderContext::default(),
        &now,
    ));
    assert!(
        line.starts_with("+2d "),
        "2 days ahead → '+2d ' prefix: {line:?}"
    );
}

#[test]
fn tail_state_cell_uses_lead_state_trail_group() {
    // Per da-fck: a `switch happy` row with prior `tired` renders as
    // `[LEAD][state cell][TRAIL]` — bg-coloured slots, no arrows.
    let _g = no_color_scope();
    let e = entry_at(
        "mood",
        Verb::Switch,
        Some(json!("happy")),
        dt(2026, 4, 22, 9, 15),
    );
    let ctx = RenderContext {
        lead_state: Some(json!("tired")),
        trail_state: Some(json!("happy")),
        is_first_occurrence: false,
        ..Default::default()
    };
    let group = format_state_group(&e, None, &ctx).expect("state group");
    let stripped = strip_ansi(&group);
    assert!(
        stripped.contains("happy"),
        "current state badge: {stripped:?}"
    );
    assert!(
        !stripped.contains("←"),
        "no left arrow in new layout: {stripped:?}"
    );
    assert!(
        !stripped.contains("→"),
        "no right arrow in new layout: {stripped:?}"
    );
}

#[test]
fn tail_state_cell_for_start_uses_bg_slots() {
    // `datum start` writes a Switch row with state=true (legacy `Verb::Start`
    // collapsed in da-8y2q.2). The canonical group emits the `start` badge,
    // with LEAD/TRAIL slots coloured by surrounding state.
    let _g = no_color_scope();
    let e = entry_at(
        "workout",
        Verb::Switch,
        Some(serde_json::json!(true)),
        dt(2026, 4, 22, 9, 15),
    );
    let ctx = RenderContext::default();
    let group = format_state_group(&e, None, &ctx).expect("state group");
    let stripped = strip_ansi(&group);
    assert!(stripped.contains("start"), "start label: {stripped:?}");
    assert!(
        !stripped.contains("→"),
        "no arrows in new layout: {stripped:?}"
    );
    // Default ctx → both slots are empty → two IGNORED-bg single-space
    // cells flanking the state body, flush against it (da-25l). Stripped
    // form is exactly " start " — single space on each side.
    assert_eq!(
        stripped, " start ",
        "two IGNORED slots flank the state cell flush: {stripped:?}"
    );
    assert!(
        !stripped.contains("∅"),
        "no ∅ glyph in slots post-da-25l: {stripped:?}"
    );
}

#[test]
fn tail_row_with_occur_time_has_no_c_prefix() {
    // ITEM B (da-7jl): rows with an occur_time render their timestamp
    // bare, no creation prefix.
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let e = entry_at("coffee", Verb::Point, None, dt(2026, 4, 22, 9, 15));
    let line = strip_ansi(&format_info_line_at(
        &e,
        None,
        &RenderContext::default(),
        &now,
    ));
    assert!(
        line.starts_with("09:15:00"),
        "occur row: bare time: {line:?}"
    );
    assert!(
        !line.starts_with('c'),
        "occur row must not be prefixed with 'c': {line:?}"
    );
}

#[test]
fn tail_row_without_occur_time_uses_create_time_with_gray_c_prefix() {
    // ITEM B (da-7jl): when the row has neither start_time nor end_time,
    // hybrid mode falls back to create_time prefixed by a gray 'c' so the
    // time column is never empty. Mirrors the table renderer's
    // `format_gray_prefix("c", ...)` path in render/mod.rs.
    let _g = no_color_scope();
    let now = dt(2026, 4, 22, 17, 0);
    let mut e = entry_no_time("person", Verb::Point, Some(json!({"name": "john"})));
    // Pin create_time deterministically to today so the relative-day
    // prefix is suppressed and the row starts with the bare 'c' marker.
    e.create_time = Utc.with_ymd_and_hms(2026, 4, 22, 9, 15, 0).unwrap();

    let line = strip_ansi(&format_info_line_at(
        &e,
        None,
        &RenderContext::default(),
        &now,
    ));
    assert!(
        line.starts_with('c'),
        "no-occur-time row: must start with 'c' creation prefix: {line:?}"
    );
    // The body after 'c' is an HH:MM:SS stamp in the host's local zone;
    // we only assert the prefix shape — exact local time depends on tz.
    assert!(
        line.contains(":"),
        "stamp digits follow the 'c' prefix: {line:?}"
    );
}

// ─── da-qfv0: repeated-state `!` warning marker ─────────────────────────────

/// A switch whose state matches the prior state (most often the user forgot
/// to log an intermediate transition) paints a literal `!` in the LEAD slot
/// so the no-op is visible at a glance.
#[test]
fn tail_state_repeat_emits_warning_glyph_in_lead_slot() {
    let _g = no_color_scope();
    let e = entry_at(
        "body",
        Verb::Switch,
        Some(json!("vertical")),
        dt(2026, 4, 22, 14, 30),
    );
    let ctx = RenderContext {
        lead_state: Some(json!("vertical")),
        trail_state: Some(json!("vertical")),
        is_state_repeat: true,
        ..Default::default()
    };
    let stripped = strip_ansi(&format_state_group(&e, None, &ctx).expect("state group"));
    assert!(
        stripped.starts_with('!'),
        "LEAD slot must carry the `!` marker: {stripped:?}"
    );
    assert!(
        stripped.contains("vertical"),
        "state cell still renders the value: {stripped:?}"
    );
}

/// A normal transition (different prior state) leaves the LEAD slot alone —
/// no `!`, just the prior-state coloured space.
#[test]
fn tail_normal_transition_has_no_warning_glyph() {
    let _g = no_color_scope();
    let e = entry_at(
        "body",
        Verb::Switch,
        Some(json!("vertical")),
        dt(2026, 4, 22, 14, 30),
    );
    let ctx = RenderContext {
        lead_state: Some(json!("sit")),
        trail_state: Some(json!("vertical")),
        is_state_repeat: false,
        ..Default::default()
    };
    let stripped = strip_ansi(&format_state_group(&e, None, &ctx).expect("state group"));
    assert!(
        !stripped.contains('!'),
        "no `!` on a state change: {stripped:?}"
    );
}

/// When the prior slot has no concrete colour (e.g. the field has been
/// closed — `Bool(false)`), the repeat flag falls through to the default
/// blank slot rather than painting `!` onto a non-existent prior block.
#[test]
fn tail_state_repeat_no_marker_when_prior_field_ended() {
    let _g = no_color_scope();
    let e = entry_at(
        "workout",
        Verb::Switch,
        Some(serde_json::json!(false)),
        dt(2026, 4, 22, 14, 30),
    );
    let ctx = RenderContext {
        lead_state: Some(serde_json::json!(false)),
        trail_state: Some(serde_json::json!(false)),
        is_state_repeat: true,
        ..Default::default()
    };
    let stripped = strip_ansi(&format_state_group(&e, None, &ctx).expect("state group"));
    assert!(
        !stripped.contains('!'),
        "FieldEnded slot suppresses the marker: {stripped:?}"
    );
}

// ─── da-ftw3: per-state and per-field truncation in the table path ──────────

fn render_table(entries: &[Entry], total_width: usize) -> String {
    let args = TableArgs {
        metric: TimeMetricCol::Occur,
        max_total_width: Some(total_width),
        ..Default::default()
    };
    let now = dt(2026, 5, 22, 17, 0);
    format_table_with(entries, None, Some(&now), None, &args).unwrap_or_default()
}

/// Every emitted row stays within the budget — the hard rule (no wrap).
fn assert_no_row_wider_than(table: &str, budget: usize) {
    for line in table.lines() {
        let w = datum_commands::render::visible_width(line);
        assert!(
            w <= budget,
            "row exceeds budget {budget}: width={w}, line={line:?}"
        );
    }
}

#[test]
fn tail_table_per_state_truncates_long_array_states() {
    let _g = no_color_scope();
    let e = entry_at(
        "body",
        Verb::Switch,
        Some(json!(["vertical", "sit", "sofa"])),
        dt(2026, 5, 22, 9, 15),
    );
    let table = strip_ansi(&render_table(std::slice::from_ref(&e), 40));
    // Every parallel state should appear at least in its truncated form
    // — `ve…`, `sit`, `so…` — when the column squeezes them. The exact
    // body width depends on the rest of the layout, but the row must
    // never wrap and the truncation marker must be present.
    let data_line = table.lines().nth(1).unwrap_or("");
    assert!(data_line.contains("body"), "field cell: {data_line:?}");
    assert!(
        data_line.contains('…') || data_line.contains("vertical,sit,sofa"),
        "either full or per-state-truncated: {data_line:?}"
    );
    assert_no_row_wider_than(&table, 40);
}

#[test]
fn tail_table_drop_mode_with_plus_k_marker() {
    let _g = no_color_scope();
    // Five long states force drop-mode at narrow budgets.
    let e = entry_at(
        "body",
        Verb::Switch,
        Some(json!(["vertical", "sit", "sofa", "office", "kitchen"])),
        dt(2026, 5, 22, 9, 15),
    );
    let table = strip_ansi(&render_table(std::slice::from_ref(&e), 30));
    let data_line = table.lines().nth(1).unwrap_or("");
    assert!(
        data_line.contains('+'),
        "drop-mode `+k` marker present: {data_line:?}"
    );
    assert_no_row_wider_than(&table, 30);
}

#[test]
fn tail_table_field_capped_with_ellipsis() {
    let _g = no_color_scope();
    // `environment` is 11 chars, over the 10-char cap.
    let e = entry_at(
        "environment",
        Verb::Switch,
        Some(json!("ambient")),
        dt(2026, 5, 22, 9, 15),
    );
    let table = strip_ansi(&render_table(std::slice::from_ref(&e), 50));
    let data_line = table.lines().nth(1).unwrap_or("");
    assert!(
        data_line.contains("environme…"),
        "field truncated at cap=10: {data_line:?}"
    );
    assert!(
        !data_line.contains("environment "),
        "no full field name after cap: {data_line:?}"
    );
    assert_no_row_wider_than(&table, 50);
}

#[test]
fn tail_table_no_wrap_under_very_narrow_budget() {
    let _g = no_color_scope();
    // Long field + long parallel states + a very narrow terminal —
    // the hard rule says: no row may wrap. We don't constrain the
    // exact content; we only enforce the invariant.
    let e = entry_at(
        "brushteeth",
        Verb::Switch,
        Some(json!(["vertical", "sit", "sofa", "office"])),
        dt(2026, 5, 22, 9, 15),
    );
    let table = strip_ansi(&render_table(std::slice::from_ref(&e), 30));
    assert_no_row_wider_than(&table, 30);
}
