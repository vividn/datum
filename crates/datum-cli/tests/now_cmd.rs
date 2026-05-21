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
use datum_cli::output::{render_now_view, render_snapshot, strip_ansi};
use datum_core::db::{
    NowFieldActive, NowFieldIgnored, NowFieldTrackedFalse, NowSnapshot, TrackedField, NOW_SCHEMA_ID,
};
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
#[ignore = "DST/timezone flake: test computes expected via chrono::Local but render uses now.offset_minutes for stale entry, mismatching across the spring/fall DST cutover (60-day stale entry can cross CET↔CEST). Tracked separately for a fixed-clock fix."]
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

// ─── da-5uph: P1D entries render as a bare date in the now-view ─────────────

/// A P1D (date-only) entry should surface in the now-view as the bare
/// local date — no `@HH:MM`, no `Nm ago`, no relative-day-plus-clock
/// hybrid. The user logged a date; show the date.
#[test]
fn render_p1d_entry_shows_bare_date_in_last_seen_column() {
    let now = DatumTime::now();
    // Pin the date to today in the local zone, then strip the clock to
    // start-of-day — mirrors what the insert path stores for `-D` rows.
    let local_today = now
        .utc
        .with_timezone(&chrono::Local)
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let local_today_utc = local_today
        .and_local_timezone(chrono::Local)
        .single()
        .expect("ambiguous local time")
        .with_timezone(&chrono::Utc);
    let day_start = DatumTime {
        utc: local_today_utc,
        offset_minutes: now.offset_minutes,
        timezone: now.timezone.clone(),
    };
    let tf = [tracked("birthday")];
    let mut latest = HashMap::new();
    let mut e = entry_builder("birthday")
        .verb(Verb::Point)
        .time_start(day_start)
        .build();
    e.occur_time_precision = Some("P1D".to_string());
    latest.insert("birthday".to_string(), e);

    let out = render_now_view(&tf, &latest, &HashMap::new(), &HashMap::new(), false);
    let plain = strip_ansi(&out);

    let expected = local_today.format("%Y-%m-%d").to_string();
    assert!(
        plain.contains(&expected),
        "P1D row must show bare date {expected:?}, got:\n{plain}"
    );
    assert!(
        !plain.contains("@00:") && !plain.contains("00:00:00"),
        "P1D row must not show the synthetic 00:00 clock, got:\n{plain}"
    );
}

// ─── da-84cg: `--json` wire schema ───────────────────────────────────────────

/// `datum now --json` emits a stable, versioned snapshot consumable by other
/// surfaces (web/mobile/wear/voice). Locks the v1 schema shape so future
/// changes are deliberate; bumps to `datum_now_v2` accompany incompatible
/// renames.
#[test]
fn now_json_emits_versioned_snapshot() {
    use std::path::PathBuf;
    use std::process::Command;

    let mut db = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    db.push(format!("datum-now-json-{pid}-{nonce}.db"));
    let _ = std::fs::remove_file(&db);
    let db_arg: PathBuf = db.clone();

    let bin = env!("CARGO_BIN_EXE_datum");
    Command::new(bin)
        .env("DATUM_DB", &db_arg)
        .env("DATUM_COLOR", "0")
        .args(["switch", "activity", "writing"])
        .output()
        .expect("seed switch");

    let out = Command::new(bin)
        .env("DATUM_DB", &db_arg)
        .env("DATUM_COLOR", "0")
        .args(["now", "--json"])
        .output()
        .expect("run datum now --json");
    assert!(
        out.status.success(),
        "datum now --json failed: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("not valid JSON: {e}; stdout was:\n{stdout}"));

    // Versioned schema marker — locked at v1; downstream consumers gate on this.
    assert_eq!(parsed["schema"], json!("datum_now_v1"));
    // The three concentric scopes are always present as arrays (empty
    // when unfiltered), so consumers can iterate without optional checks.
    assert!(parsed["active"].is_array(), "missing `active` array");
    assert!(
        parsed["tracked_false"].is_array(),
        "missing `tracked_false` array",
    );
    assert!(parsed["ignored"].is_array(), "missing `ignored` array");
    assert!(parsed["now"].is_string(), "missing `now` timestamp");

    // Active row carries the field, state, and verb. `since` is the
    // event time — present here because the seed used `switch`.
    let active = parsed["active"].as_array().expect("active array");
    assert_eq!(
        active.len(),
        1,
        "expected one active field, got: {active:?}"
    );
    let row = &active[0];
    assert_eq!(row["field"], json!("activity"));
    assert_eq!(row["state"], json!("writing"));
    assert_eq!(row["verb"], json!("switch"));

    let _ = std::fs::remove_file(&db);
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

// ─── da-mpod: snapshot-driven renderer + layered disclosure ──────────────────

fn empty_snapshot() -> NowSnapshot {
    NowSnapshot {
        schema: NOW_SCHEMA_ID,
        now: chrono::Utc::now(),
        active: Vec::new(),
        tracked_false: Vec::new(),
        ignored: Vec::new(),
    }
}

fn active_row(field: &str, state: &str) -> NowFieldActive {
    let e = switch_entry(field, state);
    let since = e.start_time.as_ref().map(|t| t.utc);
    NowFieldActive {
        field: field.to_string(),
        state: json!(state),
        since,
        verb: "switch".to_string(),
        color: None,
        entry: e,
    }
}

/// da-mpod default disclosure: a snapshot with no `tracked_false` and
/// no `ignored` rows renders only active rows — nothing dim, no
/// separator, no "(off)" markers.
#[test]
fn snapshot_render_default_shows_only_active() {
    let mut snap = empty_snapshot();
    snap.active.push(active_row("mood", "happy"));
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new());
    let plain = strip_ansi(&out);
    assert!(plain.contains("mood"), "active field must appear:\n{plain}");
    assert!(
        plain.contains("happy"),
        "active state must appear:\n{plain}"
    );
    assert!(
        !plain.contains("(off)"),
        "no tracked_false section:\n{plain}"
    );
    assert!(
        !plain.contains('─'),
        "no separator with active-only:\n{plain}"
    );
}

/// da-mpod --tracked layer: tracked-but-false rows render in a dim
/// block beneath the active rows, separated by a horizontal line.
/// The dim ANSI escape `\x1b[2m` must be present.
#[test]
fn snapshot_render_tracked_false_uses_dim_block() {
    let mut snap = empty_snapshot();
    snap.active.push(active_row("mood", "happy"));
    let last_event = chrono::Utc::now() - chrono::Duration::minutes(15);
    snap.tracked_false.push(NowFieldTrackedFalse {
        field: "screens".to_string(),
        since: Some(last_event),
        last_event: Some(last_event),
        color: None,
        entry: None,
    });

    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new());
    let plain = strip_ansi(&out);

    assert!(plain.contains("mood"), "active row present:\n{plain}");
    assert!(
        plain.contains("screens"),
        "tracked_false row present:\n{plain}"
    );
    assert!(
        plain.contains("(off)"),
        "tracked_false renders (off):\n{plain}"
    );
    assert!(plain.contains('─'), "separator divides sections:\n{plain}");
    assert!(
        out.contains("\x1b[2m"),
        "tracked_false row must use DIM:\n{out:?}"
    );

    // Ordering: active before separator before tracked_false.
    let active_at = plain.find("mood").expect("mood missing");
    let sep_at = plain.find('─').expect("separator missing");
    let off_at = plain.find("screens").expect("screens missing");
    assert!(
        active_at < sep_at && sep_at < off_at,
        "expected active → separator → tracked_false; positions {active_at} < {sep_at} < {off_at}"
    );
}

/// da-mpod --all layer: ignored rows render below tracked_false (or
/// below active when tracked_false is empty) with the existing dim
/// no-state treatment.
#[test]
fn snapshot_render_ignored_block_renders_dim() {
    let mut snap = empty_snapshot();
    snap.active.push(active_row("mood", "happy"));
    snap.ignored.push(NowFieldIgnored {
        field: "youtube".to_string(),
        since: None,
        color: None,
    });

    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new());
    let plain = strip_ansi(&out);

    assert!(plain.contains("mood"));
    assert!(plain.contains("youtube"));
    assert!(plain.contains('─'), "separator divides sections:\n{plain}");
    assert!(
        out.contains("\x1b[2m"),
        "ignored row must use DIM:\n{out:?}"
    );
}

/// Empty snapshot (every section empty) emits the legacy sentinel —
/// the user is in a brand-new DB or has filter-hidden everything.
#[test]
fn snapshot_render_empty_emits_sentinel() {
    let out = render_snapshot(&empty_snapshot(), &HashMap::new(), &HashMap::new());
    let plain = strip_ansi(&out);
    assert!(
        plain.contains("No tracked"),
        "expected onboarding sentinel:\n{plain}"
    );
}

/// Overlap counts apply to snapshot rows by field name — a row with
/// `n >= 2` gets the prefixed warning line.
#[test]
fn snapshot_render_overlap_warning_prepends_active_row() {
    let mut snap = empty_snapshot();
    snap.active.push(active_row("mood", "happy"));
    let mut overlaps = HashMap::new();
    overlaps.insert("mood".to_string(), 3u32);
    let out = render_snapshot(&snap, &HashMap::new(), &overlaps);
    let plain = strip_ansi(&out);
    assert!(
        plain.contains("3 overlapping opens on 'mood'"),
        "got:\n{plain}"
    );
}
