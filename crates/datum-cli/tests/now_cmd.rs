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

use crate::common;

use std::collections::HashMap;

use chrono::Duration;
use common::run_cli;
use datum_cli::output::{render_snapshot, strip_ansi};
use datum_core::db::{
    NowFieldActive, NowFieldIgnored, NowFieldTrackedFalse, NowSnapshot, TrackedField, NOW_SCHEMA_ID,
};
use datum_core::entry::{Entry, Verb};
use datum_core::test_util::{entry_builder, fresh_db};
use datum_core::time::{DatumTime, Zone};
use serde_json::{json, Value as JsonValue};

// ─── da-adoi: test-side snapshot builder ─────────────────────────────────────
//
// The legacy `render_now_view(tracked, latest, state_colors, overlap,
// show_ignored)` retired in this bead. Tests that used to build a
// (TrackedField list, latest-entry map) tuple now build a NowSnapshot
// the same way `Db::current_now` does and render through
// `render_snapshot`. This helper does the conversion so the test
// fixtures stay readable.
fn build_snapshot(
    tracked: &[TrackedField],
    latest: &HashMap<String, Entry>,
    show_ignored: bool,
) -> NowSnapshot {
    let mut snap = NowSnapshot {
        schema: NOW_SCHEMA_ID,
        now: chrono::Utc::now(),
        active: Vec::new(),
        tracked_false: Vec::new(),
        ignored: Vec::new(),
    };
    for tf in tracked {
        if tf.ignored {
            if show_ignored {
                let entry = latest.get(&tf.field);
                let since = entry.and_then(|e| e.occur_time.as_ref()).map(|t| t.utc);
                snap.ignored.push(NowFieldIgnored {
                    field: tf.field.clone(),
                    since,
                    color: tf.color.clone(),
                });
            }
            continue;
        }
        match latest.get(&tf.field) {
            None => snap.tracked_false.push(NowFieldTrackedFalse {
                field: tf.field.clone(),
                since: None,
                last_event: None,
                color: tf.color.clone(),
                entry: None,
            }),
            Some(e) => {
                let state = e.state.clone().unwrap_or(JsonValue::Null);
                let since = e.occur_time.as_ref().map(|t| t.utc);
                if is_truthy(&state) {
                    snap.active.push(NowFieldActive {
                        field: tf.field.clone(),
                        state,
                        since,
                        verb: e.verb.to_string(),
                        color: tf.color.clone(),
                        entry: e.clone(),
                    });
                } else {
                    snap.tracked_false.push(NowFieldTrackedFalse {
                        field: tf.field.clone(),
                        since,
                        last_event: since,
                        color: tf.color.clone(),
                        entry: Some(e.clone()),
                    });
                }
            }
        }
    }
    snap
}

/// Truthiness in the `datum now` sense — mirrors `db::now::is_truthy`,
/// which is private. Kept tight; the only callers are this file's
/// `build_snapshot` and `render_snapshot`'s active/tracked_false
/// classification (already handled inside `Db::current_now`).
fn is_truthy(v: &JsonValue) -> bool {
    match v {
        JsonValue::Null => false,
        JsonValue::Bool(b) => *b,
        JsonValue::String(s) if s == "IGNORED" => false,
        JsonValue::Array(items) => items.iter().any(is_truthy),
        _ => true,
    }
}

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
        .state(json!(state))
        .time_start(DatumTime::now())
        .build()
}

// ─── Rendered output: tracked / ignored / sentinels ──────────────────────────

#[test]
fn render_empty_tracked_shows_sentinel() {
    // No tracked fields at all (e.g. brand-new db) → the view prints its
    // terminal sentinel rather than silently producing nothing.
    let snap = build_snapshot(&[], &HashMap::new(), false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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
    let snap = build_snapshot(&tf, &HashMap::new(), false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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

    let snap = build_snapshot(&tf, &latest, false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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
            .state(json!(["datum", "household"]))
            .time_start(DatumTime::now())
            .build(),
    );

    let snap = build_snapshot(&tf, &latest, false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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
    let snap = build_snapshot(&tf, &HashMap::new(), false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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

    let snap = build_snapshot(&tf, &latest, true);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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
            .state(serde_json::json!(false))
            .time_start(DatumTime::now())
            .build(),
    );

    let snap = build_snapshot(&tf, &latest, false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
    let plain = strip_ansi(&out);

    let mood_at = plain.find("mood").expect("active mood row missing");
    let workout_at = plain.find("workout").expect("stopped workout row missing");
    let sep_at = plain.find('─').expect("separator missing");
    assert!(
        mood_at < sep_at && sep_at < workout_at,
        "expected active → separator → stopped; got {mood_at} < {sep_at} < {workout_at} in:\n{plain}"
    );
}

// ─── da-phoe: `now` after a block that has handed the field back ─────────────

/// `datum end work 12m` writes a block with `following_state` NULL, which
/// means "resume whatever state preceded me". `now` read that NULL as
/// "the field is off, and stays off", so from the end of the break
/// onwards the field's real state was invisible — and every minute of it
/// was attributed to nothing.
///
/// The whole span is probed, not just the end: the block's own 12
/// minutes always read correctly, and the defect lives entirely in the
/// stretch that goes MISSING after them.
///
/// da-zwct: `end work 12m -t -60` means the break ENDED an hour ago and
/// lasted the twelve minutes before that — the block is [-72, -60], not
/// [-60, -48].
#[test]
fn now_resumes_the_state_a_block_interrupted() {
    let db = fresh_db();
    run_cli(&["switch", "work", "datum", "-t", "-180"], &db).expect("switch dispatch");
    run_cli(&["end", "work", "12m", "-t", "-60"], &db).expect("end dispatch");

    let now = chrono::Utc::now();
    let snapshot = |mins: i64| {
        db.current_now(&datum_core::db::NowFilter {
            include_tracked_false: true,
            include_ignored: true,
            snapshot_time: Some(now + Duration::minutes(mins)),
        })
        .expect("current_now")
    };
    let active_state = |mins: i64| {
        snapshot(mins)
            .active
            .into_iter()
            .find(|r| r.field == "work")
            .map(|r| r.state)
    };

    assert_eq!(
        active_state(-120),
        Some(json!("datum")),
        "before the break, work is datum"
    );
    assert_eq!(active_state(-65), None, "during the break, work is off");
    assert!(
        snapshot(-65)
            .tracked_false
            .iter()
            .any(|r| r.field == "work"),
        "during the break the field is tracked-but-false, not missing"
    );
    assert_eq!(
        active_state(-30),
        Some(json!("datum")),
        "the block hands the field back at its trailing edge — the \
         instant it stores"
    );
    assert_eq!(
        active_state(0),
        Some(json!("datum")),
        "and it is still datum now — this is the stretch that used to \
         read `work (off)` for the rest of the day"
    );

    // The resumed span begins when the block ENDED, not when the
    // governing switch was logged three hours ago.
    let block = db.entries_for_field("work", Some(1)).expect("rows")[0].clone();
    // da-zwct: the block's trailing edge IS its stored instant; the
    // twelve minutes it lasted lie before it, so no arithmetic here.
    let resumed_at = block.occur_time.expect("block occur").utc;
    let row = snapshot(0)
        .active
        .into_iter()
        .find(|r| r.field == "work")
        .expect("work is active");
    assert_eq!(
        row.since,
        Some(resumed_at),
        "`since` is the resume boundary, not the block's leading edge"
    );
    assert_eq!(
        row.verb, "switch",
        "the governing row is the switch the block interrupted"
    );
}

/// da-e47ap: a moment switches nothing (da-22750) dated ahead of now as
/// well as behind it. Wall-clock `now` reads with no upper bound, so a
/// moment to come is its field's newest row.
#[test]
fn now_passes_over_a_moment_dated_ahead() {
    let db = fresh_db();
    run_cli(&["switch", "energy", "high", "-t", "-60"], &db).expect("switch dispatch");
    let high = db.entries_for_field("energy", Some(1)).expect("rows")[0].clone();
    run_cli(&["switch", "energy", "low", "dur=0", "-t", "+120"], &db).expect("moment dispatch");
    // A field of moments alone holds no state, whichever side of now.
    run_cli(&["switch", "mood", "calm", "dur=0", "-t", "-120"], &db).expect("moment dispatch");
    run_cli(&["switch", "mood", "calm", "dur=0", "-t", "+120"], &db).expect("moment dispatch");

    let snapshot = db
        .current_now(&datum_core::db::NowFilter {
            include_tracked_false: true,
            include_ignored: true,
            snapshot_time: None,
        })
        .expect("current_now");
    assert!(snapshot.tracked_false.iter().any(|r| r.field == "mood"));
    let [row] = &snapshot.active[..] else {
        panic!("energy alone is active: {:?}", snapshot.active);
    };
    assert_eq!(row.field, "energy");
    assert_eq!(row.state, json!("high"));
    assert_eq!(row.since, high.occur_time.map(|t| t.utc));
}

// ─── End-to-end: `datum now` through the shared CLI harness ──────────────────

#[test]
fn now_dispatches_end_to_end_via_run_cli() {
    // Full pipeline: track a field, log a switch, run `datum now` (and
    // `datum now -A`). This proves the arg-parse → dispatch path is alive;
    // output content is asserted by the render_* tests above.
    let db = fresh_db();
    run_cli(&["track", "mood", "--verb", "switch"], &db).expect("track dispatch");
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

    // Sanity: both writes landed — `track`'s declaration (da-c8mh marks it
    // as one) and the `switch happy` beside it.
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
/// da-07l3: the test used to compute its expected date via
/// `stale_utc.with_timezone(&chrono::Local)`, which is DST-aware. The
/// renderer goes through `DatumTime::local_time()`, which uses
/// `FixedOffset::east_opt(offset_minutes * 60)` — the offset captured
/// at "now" with no DST awareness. On a 60-day stale entry crossing
/// the spring/fall DST cutover, the two computations disagree by one
/// hour and (when that hour straddles midnight) by one day, flaking
/// the assertion. Now `expected` runs through the exact same
/// `FixedOffset` the renderer uses so the date strings can never
/// drift apart.
#[test]
fn render_old_stopped_field_uses_iso_date_prefix() {
    let now = DatumTime::now();
    let stale_utc = now.utc - Duration::days(60);
    let stale = DatumTime {
        utc: stale_utc,
        offset_minutes: now.offset_minutes,
        timezone: now.timezone.clone(),
        uncertainty: None,
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

    let snap = build_snapshot(&tf, &latest, false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
    let plain = strip_ansi(&out);

    // Compute the expected date through the same FixedOffset the
    // renderer uses (see DatumTime::local_time). Don't reach for
    // chrono::Local — its DST-awareness diverges from the renderer's
    // single offset_minutes snapshot when the stale instant lies on
    // the other side of a DST cutover.
    let render_offset = chrono::FixedOffset::east_opt(now.offset_minutes * 60)
        .expect("offset_minutes within FixedOffset range");
    let expected = stale_utc
        .with_timezone(&render_offset)
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
    let zone = Zone::host();
    let local_today = zone
        .local_naive(now.utc)
        .date()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    // Start-of-day is a boundary, resolved the same way `strip_to_start_of_day`
    // resolves it, so this expectation cannot drift from the insert path.
    let local_today_utc = zone.resolve(local_today).for_boundary();
    let day_start = DatumTime {
        utc: local_today_utc,
        offset_minutes: now.offset_minutes,
        timezone: now.timezone.clone(),
        uncertainty: None,
    };
    let tf = [tracked("birthday")];
    let mut latest = HashMap::new();
    let mut e = entry_builder("birthday")
        .verb(Verb::Point)
        .time_start(day_start)
        .build();
    e.set_occur_uncertainty(Some("P1D".to_string()));
    latest.insert("birthday".to_string(), e);

    let snap = build_snapshot(&tf, &latest, false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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

    let db = datum_test_util::TmpDb::new("datum-now-json");
    let db_arg: PathBuf = db.to_path_buf();

    let bin = env!("CARGO_BIN_EXE_datum");
    Command::new(bin)
        .env("DATUM_DB", &db_arg)
        .env("NO_COLOR", "1")
        .args(["switch", "activity", "writing"])
        .output()
        .expect("seed switch");

    let out = Command::new(bin)
        .env("DATUM_DB", &db_arg)
        .env("NO_COLOR", "1")
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

    let snap = build_snapshot(&tf, &latest, false);
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);

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
    let since = e.occur_time.as_ref().map(|t| t.utc);
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
    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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

    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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

    let out = render_snapshot(&snap, &HashMap::new(), &HashMap::new(), true, None);
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
    let out = render_snapshot(
        &empty_snapshot(),
        &HashMap::new(),
        &HashMap::new(),
        true,
        None,
    );
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
    let out = render_snapshot(&snap, &HashMap::new(), &overlaps, true, None);
    let plain = strip_ansi(&out);
    assert!(
        plain.contains("3 overlapping opens on 'mood'"),
        "got:\n{plain}"
    );
}

// ─── da-tx8j: time-travel + flag rebinding ───────────────────────────────────
//
// `datum now -t <time>` answers "what was the state at <past time>?";
// `-a` is now the show-tracked-false flag (was `-t/--tracked`) and `-A`
// long-name is `--all` (was `--show-all`). The legacy spellings are
// pre-v1 rip-and-replace removals — they should error as unknown.

/// `datum now -t -1h` returns the state the field was in 1h ago, not
/// the current state. Drives end-to-end through the parser + dispatch.
#[test]
fn now_time_travel_returns_past_state_not_present() {
    use datum_core::db::NowFilter;
    use datum_core::entry::Verb;
    let db = fresh_db();

    // Two switches on `mood`: "happy" 90 minutes ago, "sad" 30 minutes
    // ago. A snapshot at "1 hour ago" should resolve to "happy" — the
    // "sad" entry is in the future relative to that anchor.
    let now = chrono::Utc::now();
    let ninety_min_ago = now - Duration::minutes(90);
    let thirty_min_ago = now - Duration::minutes(30);

    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("happy"))
        .time_start_utc(ninety_min_ago)
        .insert(&db);
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("sad"))
        .time_start_utc(thirty_min_ago)
        .insert(&db);

    // Live snapshot: latest is "sad".
    let live = db.current_now(&NowFilter::default()).unwrap();
    let live_mood = live
        .active
        .iter()
        .find(|a| a.field == "mood")
        .expect("mood active in live snapshot");
    assert_eq!(live_mood.state, json!("sad"));

    // Snapshot pinned to 1 hour ago: should resolve to "happy".
    let past = db
        .current_now(&NowFilter {
            snapshot_time: Some(now - Duration::hours(1)),
            ..NowFilter::default()
        })
        .unwrap();
    let past_mood = past
        .active
        .iter()
        .find(|a| a.field == "mood")
        .expect("mood active in past snapshot");
    assert_eq!(past_mood.state, json!("happy"));
    // The snapshot's `now` field is the anchor, not wall-clock now.
    assert_eq!(past.now, now - Duration::hours(1));
}

/// `datum now -a` is the new (da-tx8j) shorthand for "include
/// tracked-but-false fields". The legacy `-t/--tracked` spelling MUST
/// error so the user discovers the rebind (pre-v1 rip-and-replace).
#[test]
fn now_flag_a_is_tracked_false_layer_and_t_is_rejected_as_unknown_switch() {
    let db = fresh_db();
    // `-a` dispatches cleanly.
    run_cli(&["now", "-a"], &db).expect("now -a (tracked-false layer) dispatches");
    // `-t` without a value is rejected by parse_now_args' time_args
    // wiring — it's now a value-bearing flag (snapshot time), not a
    // switch.
    let err = run_cli(&["now", "-t"], &db).expect_err("now -t (no value) must error");
    assert!(
        err.to_string().to_lowercase().contains("expected value")
            || err.to_string().to_lowercase().contains("missing value")
            || err.to_string().contains("--time"),
        "expected value-required error on -t, got: {err}",
    );
}

/// `datum now -A` is the all-disclosure layer (active + tracked-false +
/// ignored). The new long alias `--all` (was `--show-all`) must work;
/// the legacy `--show-all` is removed (rip-and-replace).
#[test]
fn now_short_a_and_long_all_both_select_all_layer() {
    let db = fresh_db();
    run_cli(&["now", "-A"], &db).expect("-A dispatches");
    run_cli(&["now", "--all"], &db).expect("--all dispatches");
    // Legacy `--show-all` is gone.
    let err =
        run_cli(&["now", "--show-all"], &db).expect_err("--show-all must be rejected post-rebind");
    assert!(
        err.to_string().to_lowercase().contains("show-all")
            || err.to_string().to_lowercase().contains("unknown")
            || err.to_string().to_lowercase().contains("unexpected"),
        "expected unknown-flag error on --show-all, got: {err}",
    );
}

/// `datum now -t <past> -a` combines time-travel with the tracked-false
/// layer: fields whose state at `<past>` was falsy still surface in the
/// tracked_false bucket of the snapshot.
#[test]
fn now_time_travel_combines_with_tracked_false_layer() {
    use datum_core::db::NowFilter;
    use datum_core::entry::Verb;
    let db = fresh_db();

    let now = chrono::Utc::now();
    let two_h_ago = now - Duration::hours(2);
    let thirty_min_ago = now - Duration::minutes(30);

    // `coffee` was switched-off 2h ago, then on 30m ago. At "1h ago",
    // coffee was OFF (false).
    entry_builder("coffee")
        .verb(Verb::Switch)
        .state(json!(false))
        .time_start_utc(two_h_ago)
        .insert(&db);
    entry_builder("coffee")
        .verb(Verb::Switch)
        .state(json!(true))
        .time_start_utc(thirty_min_ago)
        .insert(&db);

    let snap = db
        .current_now(&NowFilter {
            include_tracked_false: true,
            snapshot_time: Some(now - Duration::hours(1)),
            ..NowFilter::default()
        })
        .unwrap();
    // Active list does NOT contain coffee at 1h ago (it was off then).
    assert!(
        snap.active.iter().all(|a| a.field != "coffee"),
        "coffee should not be active at 1h-ago snapshot: {:?}",
        snap.active.iter().map(|a| &a.field).collect::<Vec<_>>(),
    );
    // Tracked-false list DOES contain it.
    assert!(
        snap.tracked_false.iter().any(|f| f.field == "coffee"),
        "coffee should appear in tracked_false at 1h-ago snapshot: {:?}",
        snap.tracked_false
            .iter()
            .map(|f| &f.field)
            .collect::<Vec<_>>(),
    );
}

/// da-n0dr: the snapshot anchor scopes the FIELD LIST too, not just the
/// per-field states. A field switched to IGNORED after the anchor was
/// still tracked AT the anchor — `now -t <before>` must classify it
/// active, while the live snapshot classifies it ignored.
#[test]
fn now_time_travel_classifies_ignored_at_anchor_not_wall_clock() {
    use datum_core::db::NowFilter;
    use datum_core::entry::Verb;
    let db = fresh_db();

    let now = chrono::Utc::now();
    // `mood` active 3h ago, switched IGNORED 1h ago.
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("happy"))
        .time_start_utc(now - Duration::hours(3))
        .insert(&db);
    entry_builder("mood")
        .verb(Verb::Switch)
        .state(json!("IGNORED"))
        .time_start_utc(now - Duration::hours(1))
        .insert(&db);

    // Live: mood is ignored — absent from active, present in the
    // ignored bucket when requested.
    let live = db
        .current_now(&NowFilter {
            include_ignored: true,
            ..NowFilter::default()
        })
        .unwrap();
    assert!(
        live.active.iter().all(|a| a.field != "mood"),
        "mood must not be active in the live snapshot"
    );
    assert!(
        live.ignored.iter().any(|f| f.field == "mood"),
        "mood must be in the live ignored bucket"
    );

    // Anchored 2h ago (before the IGNORED switch): mood was running —
    // it belongs in active with its then-current state, NOT in the
    // ignored bucket the wall-clock classification would put it in.
    let past = db
        .current_now(&NowFilter {
            include_ignored: true,
            snapshot_time: Some(now - Duration::hours(2)),
            ..NowFilter::default()
        })
        .unwrap();
    let past_mood = past
        .active
        .iter()
        .find(|a| a.field == "mood")
        .expect("mood active at the 2h-ago anchor");
    assert_eq!(past_mood.state, json!("happy"));
    assert!(
        past.ignored.iter().all(|f| f.field != "mood"),
        "mood must not be classified ignored at the 2h-ago anchor"
    );
}

/// da-n0dr: a field whose first-ever entry is AFTER the anchor did not
/// exist in the world-as-of-T — it must not surface as a phantom
/// tracked_false member of the time-travelled snapshot.
#[test]
fn now_time_travel_excludes_fields_born_after_anchor() {
    use datum_core::db::NowFilter;
    use datum_core::entry::Verb;
    let db = fresh_db();

    let now = chrono::Utc::now();
    // `coffee`'s first-ever entry is 30 minutes ago.
    entry_builder("coffee")
        .verb(Verb::Switch)
        .state(json!(true))
        .time_start_utc(now - Duration::minutes(30))
        .insert(&db);

    // Snapshot anchored 2h ago: coffee hadn't been seen yet — unseen
    // fields default to IGNORED, so it must not appear active or
    // tracked_false.
    let past = db
        .current_now(&NowFilter {
            include_tracked_false: true,
            snapshot_time: Some(now - Duration::hours(2)),
            ..NowFilter::default()
        })
        .unwrap();
    assert!(
        past.active.iter().all(|a| a.field != "coffee"),
        "coffee did not exist at the anchor — not active"
    );
    assert!(
        past.tracked_false.iter().all(|f| f.field != "coffee"),
        "coffee did not exist at the anchor — no phantom tracked_false row"
    );
}

/// da-5v3pl: the state list honours the same colour gate as the band —
/// `NO_COLOR`, `FORCE_COLOR`, the tty and `--no-color` — and prints no
/// escape at all when colour is off. `FORCE_COLOR=1` (what the web sets)
/// still paints it.
#[test]
fn now_state_list_honours_the_colour_gate() {
    use std::process::{Command, Stdio};

    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("now-colour.db");
    let run = |env: &[(&str, &str)], args: &[&str]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
        cmd.env("DATUM_DB", &db)
            .env_remove("FORCE_COLOR")
            .env_remove("CLICOLOR_FORCE")
            .env_remove("NO_COLOR")
            .stdout(Stdio::piped())
            .args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("run datum");
        assert!(out.status.success(), "{args:?} failed: {out:?}");
        String::from_utf8(out.stdout).expect("utf8 stdout")
    };
    run(&[], &["switch", "activity", "writing"]);
    run(&[], &["track", "mood", "--verb", "switch"]);

    for (env, args) in [
        (&[("FORCE_COLOR", "0")][..], &["now", "--tracked"][..]),
        (&[("NO_COLOR", "1")][..], &["now", "--tracked"][..]),
        (&[][..], &["now", "--tracked"][..]),
        (
            &[("FORCE_COLOR", "1")][..],
            &["now", "--tracked", "--no-color"][..],
        ),
    ] {
        let out = run(env, args);
        assert!(out.contains("(off)"), "{env:?} {args:?}:\n{out}");
        assert!(
            !out.contains("\x1b["),
            "{env:?} {args:?} coloured:\n{out:?}"
        );
    }
    let out = run(&[("FORCE_COLOR", "1")], &["now", "--bare", "--tracked"]);
    assert!(
        out.contains("\x1b[2m"),
        "tracked-false row must be DIM:\n{out:?}"
    );
    assert!(
        out.contains("\x1b[38;2;"),
        "state list must be coloured:\n{out:?}"
    );
}

/// Run the real binary against `db` with colour off, `TZ` = [`noon_zone`], a
/// `HOME` beside the db and the given env on
/// top — `datum now`'s width is only honest end to end (da-coi0m).
fn run_datum(db: &std::path::Path, env: &[(&str, &str)], args: &[&str]) -> String {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
    cmd.env("DATUM_DB", db)
        .env("HOME", db.parent().expect("db dir"))
        .env("NO_COLOR", "1")
        .env("TZ", noon_zone())
        .env_remove("COLUMNS")
        .stdout(Stdio::piped())
        .args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run datum");
    assert!(out.status.success(), "{args:?} failed: {out:?}");
    String::from_utf8(out.stdout).expect("utf8 stdout")
}

/// The `Etc/GMT` zone whose clock reads 12:xx right now, so entries seeded
/// minutes back never fall on yesterday: under `TZ=UTC` a run in the first
/// 40 minutes after midnight printed `-1d 23:52` for `20m ago` and the
/// column widened (da-0ekq7). IANA's `Etc/GMT` signs are inverted.
fn noon_zone() -> String {
    let secs = std::time::UNIX_EPOCH.elapsed().expect("clock").as_secs();
    let offset = 12 - (secs / 3600 % 24) as i64;
    format!("Etc/GMT{:+}", -offset)
}

fn width(line: &str) -> usize {
    datum_cli::output::term_layout::visible_width(line)
}

/// A db whose `now` cannot fit a narrow terminal whole: a long field name,
/// a long state, a parallel state and wide (CJK) text.
fn wide_now_db(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let db = dir.path().join("wide.db");
    let long_field = "an_extremely_long_field_name_that_goes_on";
    run_datum(
        &db,
        &[],
        &[
            "switch",
            "project",
            "daydumb-very-long-project-name",
            "-t",
            "-40",
        ],
    );
    run_datum(
        &db,
        &[],
        &[
            "switch",
            "mood",
            "happy and content with the world",
            "-t",
            "-20",
        ],
    );
    run_datum(&db, &[], &["start", long_field, "-t", "-10"]);
    run_datum(
        &db,
        &[],
        &["switch", "body", "vertical,sitting,typing", "-t", "-30"],
    );
    run_datum(
        &db,
        &[],
        &["switch", "場所", "東京の事務所の机", "-t", "-15"],
    );
    run_datum(&db, &[], &["occur", "coffee", "-t", "-5"]);
    db
}

/// da-coi0m (1): `$COLUMNS` sizes the whole of `datum now` — the state list
/// as well as the band — and `--cols` beats it. Before, only the band read
/// it and the list ran on at its natural width.
#[test]
fn now_sizes_list_and_band_to_columns() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = wide_now_db(&dir);
    for (env, args, cols) in [
        (&[("COLUMNS", "60")][..], &["now"][..], 60),
        (&[("COLUMNS", "60")][..], &["now", "--cols", "44"][..], 44),
    ] {
        let out = run_datum(&db, env, args);
        let list = out.lines().find(|l| l.contains("mood")).expect("mood row");
        assert_eq!(width(list), cols, "state list at {cols}:\n{out}");
        let band = out
            .lines()
            .find(|l| l.starts_with("coffee"))
            .expect("band row");
        assert_eq!(width(band), cols, "band at {cols}:\n{out}");
    }
    // Piped with nothing set, the list is printed whole.
    let out = run_datum(&db, &[], &["now", "--bare"]);
    assert!(out.contains("happy and content with the world"), "{out}");
}

/// da-coi0m (2): at every width no line of `datum now` runs past it — the
/// columns are cut with `…` on purpose — the band's floor is as wide as its
/// rows with `┴` under the divider `│`, and an axis label is drawn whole or
/// not at all (a narrow band once wrote `-6-45-30m`).
#[test]
fn now_fits_every_width_without_wrapping() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = wide_now_db(&dir);
    for cols in [25, 32, 40, 60, 100] {
        let c = cols.to_string();
        let out = run_datum(&db, &[("COLUMNS", &c)], &["now"]);
        for line in out.lines() {
            assert!(
                width(line) <= cols,
                "{cols}: {line:?} is {} wide:\n{out}",
                width(line)
            );
        }
        assert!(out.contains('…'), "{cols}: something was cut:\n{out}");
        let lines: Vec<&str> = out.lines().collect();
        let floor = lines.iter().position(|l| l.contains('└')).expect("floor");
        let row = lines[floor - 1];
        assert_eq!(width(lines[floor]), cols, "{cols}: floor:\n{out}");
        let col = |l: &str, ch| l.find(ch).map(|i| width(&l[..i]));
        assert_eq!(col(lines[floor], '┴'), col(row, '│'), "{cols}:\n{out}");
        for label in lines[floor + 1].split_whitespace() {
            let minutes = label
                .strip_prefix(['-', '+'])
                .and_then(|m| m.strip_suffix('m'))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            let clock = label.len() == 5
                && label.bytes().enumerate().all(|(i, b)| match i {
                    2 => b == b':',
                    _ => b.is_ascii_digit(),
                });
            let whole = minutes || clock;
            assert!(whole, "{cols}: axis label {label:?} is cut:\n{out}");
        }
    }
}

/// da-coi0m (3): with a time arg the band is centred on that instant, half
/// its hour either side, the clock on the divider — as nowview's is — where
/// it used to be dropped. Fixed times: a coffee ten minutes after the
/// instant lands right of the divider, a switch before it left.
#[test]
fn now_band_centres_on_the_time_args() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("pinned.db");
    run_datum(
        &db,
        &[],
        &["switch", "mood", "happy", "-d", "2026-01-05", "-t", "1000"],
    );
    run_datum(
        &db,
        &[],
        &["occur", "coffee", "-d", "2026-01-05", "-t", "1025"],
    );
    let out = run_datum(
        &db,
        &[("COLUMNS", "60")],
        &["now", "-d", "2026-01-05", "-t", "1015"],
    );
    let lines: Vec<&str> = out.lines().collect();
    let divider = |l: &str| l.chars().position(|c| c == '│').expect("divider");
    let coffee = lines
        .iter()
        .find(|l| l.starts_with("coffee"))
        .expect("band row");
    let dot = coffee.chars().position(|c| c == '●').expect("coffee dot");
    assert!(dot > divider(coffee), "10:25 is after 10:15:\n{out}");
    let mood = lines
        .iter()
        .find(|l| l.starts_with("mood"))
        .expect("mood row");
    let left: String = mood.chars().take(divider(mood)).collect();
    assert!(
        left.contains('░') && left.contains('█'),
        "happy began at 10:00:\n{out}"
    );
    let axis = lines.last().expect("axis");
    let clock = axis.find("10:15").expect("the instant's clock") + 2;
    assert!(
        clock.abs_diff(divider(mood)) <= 1,
        "clock on the divider:\n{out}"
    );
    assert!(axis.contains("-30m") && axis.contains("+30m"), "{out}");
}

/// da-sy7zl: a flag is no row of `now`'s, as `datum help flag` says. Its
/// comment is stored as its state, so one dated ahead of now was the
/// field's newest row and `now` read «mood come back in 3h»; one behind
/// a stopped field was its `last_event`.
#[test]
fn now_reads_no_flag_as_a_fields_state_or_last_event() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy", "-t", "-180"], &db).expect("switch");
    run_cli(&["flag", "mood", "come back", "-t", "+180"], &db).expect("future flag");
    run_cli(&["switch", "nap", "true", "-t", "-180"], &db).expect("switch");
    run_cli(&["end", "nap", "-t", "-120"], &db).expect("end");
    run_cli(&["flag", "nap", "was it a nap", "-t", "-60"], &db).expect("past flag");

    let snap = db
        .current_now(&datum_core::db::NowFilter {
            include_tracked_false: true,
            ..Default::default()
        })
        .expect("current_now");
    let ago =
        |t: Option<chrono::DateTime<chrono::Utc>>| (snap.now - t.expect("a time")).num_minutes();

    let [mood] = &snap.active[..] else {
        panic!("mood alone is active: {:?}", snap.active);
    };
    assert_eq!(
        (mood.field.as_str(), &mood.state),
        ("mood", &json!("happy"))
    );
    assert_eq!(ago(mood.since), 180);

    let [nap] = &snap.tracked_false[..] else {
        panic!("nap alone is stopped: {:?}", snap.tracked_false);
    };
    assert_eq!(nap.field, "nap");
    assert_eq!(
        ago(nap.last_event),
        120,
        "the end, not the flag an hour after it"
    );
}
