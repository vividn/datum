//! Parity tests for `datum now` (P10 / da-pkwv).
//!
//! `now` is daydumb-native — datum's `nowview` is structurally different, so
//! there are no datum cases to port. This file is the regression-lock bead:
//! it pins the currently-visible behavior (tracked vs ignored, `-A` flag,
//! active/stopped separator, sentinels) before any spec-color / color-pipeline
//! changes from the A0 wave land.
//!
//! Most assertions hit [`render_now_view`] directly (pure function, easy to
//! inspect). One end-to-end case dispatches through the shared CLI harness
//! from P0 (da-5mf8) to confirm `cmd_now` is wired up.

mod common;

use std::collections::HashMap;

use chrono::Duration;
use common::run_cli;
use datum_cli::output::{render_now_view, strip_ansi};
use datum_core::db::TrackedField;
use datum_core::entry::{Entry, Verb};
use datum_core::test_util::{entry_builder, fresh_db};
use datum_core::time::DatumTime;
use serde_json::json;

// ─── Small helpers ───────────────────────────────────────────────────────────

fn tracked(field: &str) -> TrackedField {
    TrackedField {
        field: field.to_string(),
        ignored: false,
        color: None,
    }
}

fn ignored(field: &str) -> TrackedField {
    TrackedField {
        field: field.to_string(),
        ignored: true,
        color: None,
    }
}

/// Minimal "just happened" Switch entry; its state becomes the active row.
fn switch_entry(field: &str, state: &str) -> Entry {
    entry_builder(field)
        .verb(Verb::Switch)
        .value(json!(state))
        .time_start(DatumTime::now())
        .build()
}

// ─── Rendered output: tracked / ignored / sentinels ──────────────────────────

#[test]
fn render_empty_tracked_shows_sentinel() {
    // No tracked fields at all (e.g. brand-new db) → the view prints its
    // terminal sentinel rather than silently producing nothing.
    let out = render_now_view(
        &[],
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        false,
    );
    let plain = strip_ansi(&out);
    assert!(
        plain.contains("No tracked (non-ignored) fields"),
        "expected sentinel, got:\n{plain}"
    );
}

#[test]
fn render_tracked_field_with_no_entries_shows_no_entries_marker() {
    // Field is tracked but nothing's been logged for it yet. The row must
    // still appear so the user sees the field exists — with a "(no entries)"
    // marker in the time column — rather than being silently hidden.
    let tf = [tracked("mood")];
    let out = render_now_view(
        &tf,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        false,
    );
    let plain = strip_ansi(&out);
    assert!(plain.contains("mood"), "field must appear:\n{plain}");
    assert!(
        plain.contains("(no entries)"),
        "expected '(no entries)' marker, got:\n{plain}"
    );
}

#[test]
fn render_switch_entry_appears_as_active_row_with_bullet_and_state() {
    // A recent `switch mood happy` should render as an active row — bullet
    // present, state visible, "ago" duration for the timestamp.
    let tf = [tracked("mood")];
    let mut latest = HashMap::new();
    latest.insert("mood".to_string(), switch_entry("mood", "happy"));

    let out = render_now_view(&tf, &latest, &HashMap::new(), &HashMap::new(), false);
    let plain = strip_ansi(&out);
    assert!(
        plain.contains("●"),
        "active row must have ● bullet:\n{plain}"
    );
    assert!(plain.contains("mood"), "field must appear:\n{plain}");
    assert!(plain.contains("happy"), "state must appear:\n{plain}");
}

/// S12 (da-u4k.12): an array value renders each state, comma-joined, in the
/// state column. Visible width excludes ANSI, so the separator length still
/// matches the content width after multi-color painting.
#[test]
fn render_array_state_shows_comma_joined_members() {
    let tf = [tracked("project")];
    let mut latest = HashMap::new();
    latest.insert(
        "project".to_string(),
        entry_builder("project")
            .verb(Verb::Switch)
            .value(json!(["datum", "household"]))
            .time_start(DatumTime::now())
            .build(),
    );

    let out = render_now_view(&tf, &latest, &HashMap::new(), &HashMap::new(), false);
    let plain = strip_ansi(&out);
    assert!(plain.contains("datum"), "first state must appear:\n{plain}");
    assert!(
        plain.contains("household"),
        "second state must appear:\n{plain}"
    );
    assert!(
        plain.contains("datum,household"),
        "array renders as comma-joined states:\n{plain}"
    );
    // The ugly JSON literal must NOT leak through.
    assert!(
        !plain.contains("[\""),
        "array must not render as raw JSON:\n{plain}"
    );
}

#[test]
fn render_ignored_field_hidden_by_default() {
    // `ignored=true` specs must NOT appear when `show_ignored=false`, and the
    // "no (non-ignored)" sentinel should fire because nothing else is tracked.
    let tf = [ignored("youtube")];
    let out = render_now_view(
        &tf,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        false,
    );
    let plain = strip_ansi(&out);
    assert!(
        !plain.contains("youtube"),
        "ignored field must not render:\n{plain}"
    );
    assert!(
        plain.contains("No tracked (non-ignored) fields"),
        "sentinel fires when nothing to show:\n{plain}"
    );
}

#[test]
fn render_ignored_field_shown_dim_after_separator_with_show_all() {
    // `datum now -A` surfaces ignored fields in a dim block, preceded by a
    // horizontal separator if there's any active/stopped content above it.
    let tf = [tracked("mood"), ignored("youtube")];
    let mut latest = HashMap::new();
    latest.insert("mood".to_string(), switch_entry("mood", "happy"));

    let out = render_now_view(&tf, &latest, &HashMap::new(), &HashMap::new(), true);
    let plain = strip_ansi(&out);

    assert!(
        plain.contains("youtube"),
        "show_all must include ignored:\n{plain}"
    );
    assert!(plain.contains("─"), "separator must appear:\n{plain}");

    // The ignored row is emitted with the DIM ANSI code (\x1b[2m).
    assert!(
        out.contains("\x1b[2m"),
        "ignored rows must use DIM ANSI (\\x1b[2m):\n{out:?}"
    );

    // Ordering: separator must come AFTER the mood row and BEFORE the
    // ignored block — this is the "dim text after a separator" contract.
    let mood_at = plain.find("mood").expect("mood row missing");
    let sep_at = plain.find('─').expect("separator missing");
    let yt_at = plain.find("youtube").expect("youtube row missing");
    assert!(
        mood_at < sep_at && sep_at < yt_at,
        "expected mood → separator → youtube; got positions {mood_at} < {sep_at} < {yt_at} in:\n{plain}"
    );
}

#[test]
fn render_separator_divides_active_from_stopped() {
    // When both an active (Switch w/ truthy value) and a stopped (Stop verb)
    // field are present, the view groups them with a horizontal separator.
    let tf = [tracked("mood"), tracked("workout")];
    let mut latest = HashMap::new();
    latest.insert("mood".to_string(), switch_entry("mood", "happy"));
    latest.insert(
        "workout".to_string(),
        entry_builder("workout")
            .verb(Verb::Switch)
            .value(serde_json::json!(false))
            .time_start(DatumTime::now())
            .build(),
    );

    let out = render_now_view(&tf, &latest, &HashMap::new(), &HashMap::new(), false);
    let plain = strip_ansi(&out);

    let mood_at = plain.find("mood").expect("active mood row missing");
    let workout_at = plain.find("workout").expect("stopped workout row missing");
    let sep_at = plain.find('─').expect("separator missing");
    assert!(
        mood_at < sep_at && sep_at < workout_at,
        "expected active → separator → stopped; got {mood_at} < {sep_at} < {workout_at} in:\n{plain}"
    );
}

// ─── End-to-end: `datum now` through the shared CLI harness ──────────────────

#[test]
fn now_dispatches_end_to_end_via_run_cli() {
    // Full pipeline: track a field, log a switch, run `datum now` (and
    // `datum now -A`). This proves the arg-parse → dispatch path is alive;
    // output content is asserted by the render_* tests above.
    let db = fresh_db();
    run_cli(&["track", "mood"], &db).expect("track dispatch");
    // `-t +1` keeps the switch's timestamp distinct from `track`'s default
    // `now()` — both write Switch entries with slug `mood:<start_time>`, and
    // on fast machines a same-millisecond collision hard-errors (da-p5j,
    // family of da-e65). Same-test fix pattern used in test_entries_for_field.
    run_cli(&["switch", "mood", "happy", "-t", "+1"], &db).expect("switch dispatch");

    // Bare `now` honours the default ignored-hidden contract.
    run_cli(&["now"], &db).expect("now dispatch");

    // `-A` is a presence-only flag; its expansion belongs to the render tests.
    run_cli(&["now", "-A"], &db).expect("now -A dispatch");

    // da-raz: `now` now also accepts the band-tuning flags ported from
    // the old `nowview --terminal` mode. They must dispatch without error
    // even when stdout isn't a TTY (the band still emits, just without
    // ANSI color).
    run_cli(&["now", "--bare"], &db).expect("now --bare dispatch");
    run_cli(&["now", "--no-color", "--cols", "60"], &db).expect("now --no-color --cols dispatch");

    // Sanity: both writes landed. da-2sr: `track` now writes a switch with
    // value=NULL, and the `switch happy` is a second switch entry.
    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.verb == Verb::Switch));
}

// ─── da-n1q: stale `last seen` rows use the relative-day prefix ──────────────

/// An old stopped field shouldn't render as `NNNNNh ago` — bead da-n1q
/// requires the relative-day prefix used by `tail` so the value stays
/// readable. We construct an entry whose start_time is well over 3 days
/// in the past; the rendered "last seen" must include the ISO-date prefix
/// rather than an hours-only ago string.
#[test]
fn render_old_stopped_field_uses_iso_date_prefix() {
    let now = DatumTime::now();
    let stale_utc = now.utc - Duration::days(60);
    let stale = DatumTime {
        utc: stale_utc,
        offset_minutes: now.offset_minutes,
        timezone: now.timezone.clone(),
    };
    let tf = [tracked("nightlight")];
    let mut latest = HashMap::new();
    latest.insert(
        "nightlight".to_string(),
        entry_builder("nightlight")
            .verb(Verb::Switch)
            .time_start(stale)
            .build(),
    );

    let out = render_now_view(&tf, &latest, &HashMap::new(), &HashMap::new(), false);
    let plain = strip_ansi(&out);

    // The rendered date prefix must match the local-calendar date of the
    // stale entry (60 days back, so guaranteed to be >3 days → ISO prefix).
    let expected = stale_utc
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string();
    assert!(
        plain.contains(&expected),
        "expected ISO-date prefix {expected:?} in last-seen column, got:\n{plain}"
    );
    // Old `Nh ago` shouldn't appear for entries this stale — the prefix
    // replaces the ago string.
    assert!(
        !plain.contains("h ago"),
        "expected no 'h ago' fragment for stale row, got:\n{plain}"
    );
}

// ─── Gated on A0: per-field spec color ───────────────────────────────────────

#[test]
#[ignore = "A0 wave: spec-color pipeline not locked yet; re-enable once A0 lands"]
fn render_honors_spec_color_on_tracked_field() {
    // When A0 finalizes the color pipeline, a tracked field with a stored
    // color should emit that color's ANSI code (hex or ANSI-256) for the
    // field name, not the deterministic hash fallback.
    let tf = [TrackedField {
        field: "mood".to_string(),
        ignored: false,
        color: Some("ff6600".to_string()),
    }];
    let mut latest = HashMap::new();
    latest.insert("mood".to_string(), switch_entry("mood", "happy"));

    let out = render_now_view(&tf, &latest, &HashMap::new(), &HashMap::new(), false);

    // True-color escape for #ff6600: `\x1b[38;2;255;102;0m`.
    assert!(
        out.contains("\x1b[38;2;255;102;0m"),
        "expected 24-bit color escape for ff6600 in output:\n{out:?}"
    );
}
