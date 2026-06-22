//! Original-datum reference parity goldens.
//!
//! Catches da-438y-class drift before merge: each fixture pins a known-good
//! output for a specific datum-compat surface so a one-byte regression in
//! the renderer / parser / color path fails CI loudly.
//!
//! Run with: `cargo test --test parity`.
//!
//! Layout (under `tests/parity/`):
//! * `md5_colors/` — input names + expected hex per `#md5(name)[:6]`.
//!   Locks the field/state color algorithm. da-438y was a routing bug
//!   that bypassed this exact path.
//! * `time_syntax/` — TSVs covering compact dates, ISO datetimes,
//!   absolute/relative times, and stacked-flag counts. Locks the
//!   `parse_date_arg` / `parse_time_arg` / `count_stacked_flag` surface
//!   that the retro flagged as "datum-compat religion".
//! * `dayview/` — synthetic entry log + golden dayview JSON. Locks
//!   field/state color resolution end-to-end through the layout engine.
//!
//! Goldens are committed; no live datum dependency in CI. The
//! `tests/parity/regen.sh` helper regenerates them from a local datum
//! install (or md5 / daydumb itself) on demand.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use datum_core::db::Db;
use datum_core::entry::Verb;
use datum_core::test_util::entry_builder;
use datum_parse::time::clock::{install, uninstall, MockClock};
use datum_parse::time::{count_stacked_flag, parse_date_arg, parse_time_arg, DatumTime, Direction};
use md5::{Digest, Md5};
use serde_json::Value as JsonValue;

// ─── Fixture loading ─────────────────────────────────────────────────────────

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("parity")
}

fn read_fixture(rel: &str) -> String {
    let path = fixture_dir().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

/// Iterate non-empty, non-comment data lines of a TSV / list file.
fn data_lines(s: &str) -> impl Iterator<Item = (usize, &str)> {
    s.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .filter(|(_, l)| !l.trim_start().starts_with('#'))
}

// ─── (1) md5 color parity ────────────────────────────────────────────────────

/// `inputs.txt` and `expected.txt` are parallel files. Empty lines in
/// `inputs.txt` are treated as the empty-string input (md5 sentinel
/// `d41d8cd98f...`). The expected line on the same row is the
/// matching `#md5(name)[:6]` hex.
#[test]
fn md5_color_goldens_match_canonical_md5() {
    let inputs = read_fixture("md5_colors/inputs.txt");
    let expected = read_fixture("md5_colors/expected.txt");

    // Strip the trailing empty line that POSIX text files end with so the
    // `inputs.lines()` and `expected.lines()` zip cleanly.
    let in_lines: Vec<&str> = inputs.lines().collect();
    let exp_lines: Vec<&str> = expected.lines().collect();
    assert_eq!(
        in_lines.len(),
        exp_lines.len(),
        "inputs and expected fixtures must have the same number of lines"
    );
    assert!(!in_lines.is_empty(), "md5_colors fixture is empty");

    for (i, (input, want)) in in_lines.iter().zip(exp_lines.iter()).enumerate() {
        let mut hasher = Md5::new();
        hasher.update(input.as_bytes());
        let digest = hasher.finalize();
        let got = format!("#{:02x}{:02x}{:02x}", digest[0], digest[1], digest[2]);
        assert_eq!(
            got,
            *want,
            "md5_colors[{}] input={input:?}: got {got}, want {want}",
            i + 1
        );
    }
}

// ─── (2) time-syntax parity ──────────────────────────────────────────────────

/// Install a `MockClock` for the lifetime of `f`. Cleans up via the same
/// pattern as `datum-parse::time::tests::with_mocked_today`.
fn with_mocked_today(today: NaiveDate, f: impl FnOnce()) {
    let utc = Utc.from_utc_datetime(&today.and_hms_opt(12, 0, 0).unwrap());
    let dt = DatumTime {
        utc,
        offset_minutes: 0,
        timezone: "UTC".into(),
    };
    let prev = install(Arc::new(MockClock::new(dt)));
    f();
    uninstall();
    if let Some(p) = prev {
        install(p);
    }
}

#[test]
fn compact_date_goldens_round_trip_through_parse_date_arg() {
    let body = read_fixture("time_syntax/compact_dates.tsv");
    let mut header_seen = false;
    let mut row_count = 0usize;

    for (lineno, raw) in data_lines(&body) {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if !header_seen {
            assert_eq!(
                cols,
                ["input", "today", "expected", "direction"],
                "compact_dates header (line {lineno})"
            );
            header_seen = true;
            continue;
        }
        assert_eq!(cols.len(), 4, "compact_dates row (line {lineno}): {line:?}");
        let input = cols[0];
        let today: NaiveDate = cols[1]
            .parse()
            .unwrap_or_else(|e| panic!("compact_dates today col (line {lineno}): {e}"));
        let want_iso = cols[2];
        let dir_label = cols[3];

        // The input column carries the trailing `+`/`-` marker already; the
        // direction column is just the structural label we expect to round-trip.
        match dir_label {
            "none" | "future" | "past" => {}
            other => panic!("compact_dates direction (line {lineno}): {other:?}"),
        }

        with_mocked_today(today, || {
            let arg = parse_date_arg(input)
                .unwrap_or_else(|e| panic!("parse_date_arg({input:?}) line {lineno}: {e}"));
            let got = arg
                .absolute
                .unwrap_or_else(|| panic!("compact_dates line {lineno}: no absolute date"));
            assert_eq!(
                got.format("%Y-%m-%d").to_string(),
                want_iso,
                "compact_dates line {lineno} input={input:?}",
            );
            // Direction marker round-trips:
            let want_dir = match dir_label {
                "none" => None,
                "future" => Some(Direction::Future),
                "past" => Some(Direction::Past),
                _ => unreachable!(),
            };
            assert_eq!(
                arg.direction, want_dir,
                "compact_dates direction line {lineno} input={input:?}",
            );
        });
        row_count += 1;
    }

    assert!(
        row_count >= 5,
        "compact_dates fixture too small: {row_count} rows"
    );
}

#[test]
fn relative_offsets_goldens_match_parser_output() {
    let body = read_fixture("time_syntax/relative_offsets.tsv");
    let mut header_seen = false;
    let mut count = 0usize;

    for (lineno, raw) in data_lines(&body) {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if !header_seen {
            assert_eq!(
                cols,
                ["kind", "input", "expected"],
                "relative_offsets header"
            );
            header_seen = true;
            continue;
        }
        assert_eq!(
            cols.len(),
            3,
            "relative_offsets row (line {lineno}): {line:?}"
        );
        let kind = cols[0];
        let input = cols[1];
        let want_n: i64 = cols[2]
            .parse()
            .unwrap_or_else(|e| panic!("expected col line {lineno}: {e}"));

        match kind {
            "date" => {
                let arg = parse_date_arg(input)
                    .unwrap_or_else(|e| panic!("parse_date_arg({input:?}) line {lineno}: {e}"));
                assert_eq!(
                    arg.days_offset, want_n,
                    "relative_offsets[date] line {lineno} input={input:?}",
                );
                assert!(
                    arg.absolute.is_none(),
                    "relative_offsets[date] line {lineno}: bare offset must not be absolute",
                );
            }
            "time" => {
                let arg = parse_time_arg(input)
                    .unwrap_or_else(|e| panic!("parse_time_arg({input:?}) line {lineno}: {e}"));
                assert_eq!(
                    arg.offset,
                    Some(chrono::Duration::minutes(want_n)),
                    "relative_offsets[time] line {lineno} input={input:?}",
                );
                assert!(
                    arg.absolute.is_none(),
                    "relative_offsets[time] line {lineno}: signed bare must not be absolute",
                );
            }
            other => panic!("relative_offsets kind (line {lineno}): {other:?}"),
        }
        count += 1;
    }
    assert!(
        count >= 5,
        "relative_offsets fixture too small: {count} rows"
    );
}

#[test]
fn absolute_time_goldens_match_parse_time_arg() {
    let body = read_fixture("time_syntax/absolute_times.tsv");
    let mut header_seen = false;
    let mut count = 0usize;

    for (lineno, raw) in data_lines(&body) {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if !header_seen {
            assert_eq!(cols, ["input", "expected"], "absolute_times header");
            header_seen = true;
            continue;
        }
        assert_eq!(
            cols.len(),
            2,
            "absolute_times row (line {lineno}): {line:?}"
        );
        let input = cols[0];
        let want = cols[1];

        let arg = parse_time_arg(input)
            .unwrap_or_else(|e| panic!("parse_time_arg({input:?}) line {lineno}: {e}"));
        let got = arg.absolute.unwrap_or_else(|| {
            panic!("absolute_times line {lineno}: no absolute time for {input:?}")
        });
        assert_eq!(
            got.format("%H:%M:%S").to_string(),
            want,
            "absolute_times line {lineno} input={input:?}",
        );
        count += 1;
    }
    assert!(count >= 5, "absolute_times fixture too small: {count} rows");
}

#[test]
fn iso_datetime_goldens_route_through_correct_branch() {
    let body = read_fixture("time_syntax/iso_datetimes.tsv");
    let mut header_seen = false;
    let mut count = 0usize;

    for (lineno, raw) in data_lines(&body) {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if !header_seen {
            assert_eq!(cols, ["kind", "input", "expected"], "iso_datetimes header");
            header_seen = true;
            continue;
        }
        assert_eq!(cols.len(), 3, "iso_datetimes row (line {lineno}): {line:?}");
        let kind = cols[0];
        let input = cols[1];
        let want = cols[2];

        let arg = parse_time_arg(input)
            .unwrap_or_else(|e| panic!("parse_time_arg({input:?}) line {lineno}: {e}"));

        match kind {
            "instant" => {
                let inst: DateTime<Utc> = arg.iso_instant.unwrap_or_else(|| {
                    panic!("iso_datetimes line {lineno}: expected iso_instant for {input:?}")
                });
                // Format with fractional-seconds-suppress-trailing-zeros to match the
                // golden's compact form. chrono's `%+` includes microseconds.
                let got = format_instant_compact(inst);
                assert_eq!(
                    got, want,
                    "iso_datetimes[instant] line {lineno} input={input:?}"
                );
                assert!(
                    arg.iso_naive.is_none(),
                    "iso_datetimes[instant] line {lineno}: must not be naive too"
                );
            }
            "naive" => {
                let n = arg
                    .iso_naive
                    .unwrap_or_else(|| panic!("iso_datetimes[naive] line {lineno}: not naive"));
                let got = format_naive_compact(n);
                assert_eq!(
                    got, want,
                    "iso_datetimes[naive] line {lineno} input={input:?}"
                );
                assert!(
                    arg.iso_instant.is_none(),
                    "iso_datetimes[naive] line {lineno}: must not be instant too"
                );
            }
            other => panic!("iso_datetimes kind (line {lineno}): {other:?}"),
        }
        count += 1;
    }
    assert!(count >= 4, "iso_datetimes fixture too small: {count} rows");
}

/// Render a UTC instant with millisecond precision when sub-second exists,
/// matching how the goldens are written. Sub-millisecond precision is not
/// representable in the fixture (and isn't needed for the cases we pin).
fn format_instant_compact(dt: DateTime<Utc>) -> String {
    let nanos = dt.timestamp_subsec_nanos();
    if nanos == 0 {
        dt.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        let ms = nanos / 1_000_000;
        format!("{}.{ms:03}+00:00", dt.format("%Y-%m-%dT%H:%M:%S"))
    }
}

fn format_naive_compact(n: chrono::NaiveDateTime) -> String {
    let nanos = n.and_utc().timestamp_subsec_nanos();
    if nanos == 0 {
        n.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        let ms = nanos / 1_000_000;
        format!("{}.{ms:03}", n.format("%Y-%m-%dT%H:%M:%S"))
    }
}

#[test]
fn stacked_flag_counts_match_count_stacked_flag() {
    let body = read_fixture("time_syntax/stacked_flags.tsv");
    let mut header_seen = false;
    let mut count = 0usize;

    for (lineno, raw) in data_lines(&body) {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if !header_seen {
            assert_eq!(cols, ["flag", "letter", "count"], "stacked_flags header");
            header_seen = true;
            continue;
        }
        assert_eq!(cols.len(), 3, "stacked_flags row (line {lineno}): {line:?}");
        let flag = cols[0];
        let letter: char = cols[1]
            .chars()
            .next()
            .expect("stacked_flags letter must be a single char");
        let want: usize = cols[2]
            .parse()
            .unwrap_or_else(|e| panic!("stacked_flags count col line {lineno}: {e}"));

        let got = count_stacked_flag(flag, letter).unwrap_or_else(|| {
            panic!("stacked_flags line {lineno}: didn't match {flag:?} {letter:?}")
        });
        assert_eq!(got, want, "stacked_flags line {lineno} flag={flag:?}");
        count += 1;
    }
    assert!(count >= 4, "stacked_flags fixture too small: {count} rows");
}

// ─── (3) dayview render snapshot ─────────────────────────────────────────────

/// Build a deterministic file-backed DB in `dir` and return its path.
/// Entries land at fixed UTC instants on 2026-01-15.
fn populate_dayview_fixture_db(dir: &Path) -> PathBuf {
    let db_path = dir.join("parity-dayview.db");
    let db = Db::open(db_path.to_str().unwrap()).expect("open file db");

    // Anchor: 2026-01-15 (a Thursday, ISO date) — far enough in the past
    // that `now_time_pos` is null when CI runs, keeping the golden stable.
    let day = chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    let at = |h: u32, m: u32| -> DateTime<Utc> {
        Utc.from_utc_datetime(&day.and_hms_opt(h, m, 0).unwrap())
    };

    // coffee occurrence at 08:00 UTC — point-in-time entry.
    entry_builder("coffee")
        .verb(Verb::Point)
        .state(serde_json::json!(1))
        .time_start_utc(at(8, 0))
        .insert(&db);

    // workout block 09:00 → 10:00 UTC, switch verb with implicit-true state.
    entry_builder("workout")
        .verb(Verb::Switch)
        .state(serde_json::json!(true))
        .time_start_utc(at(9, 0))
        .time_end_utc(at(10, 0))
        .insert(&db);

    // project state "meeting" 10:00 → 12:00 UTC — exercises md5(state).
    entry_builder("project")
        .verb(Verb::Switch)
        .state(serde_json::json!("meeting"))
        .time_start_utc(at(10, 0))
        .time_end_utc(at(12, 0))
        .insert(&db);

    // project state "emails" 14:00 → 16:00 UTC — second block, different state.
    entry_builder("project")
        .verb(Verb::Switch)
        .state(serde_json::json!("emails"))
        .time_start_utc(at(14, 0))
        .time_end_utc(at(16, 0))
        .insert(&db);

    drop(db); // flush WAL / close cleanly before subprocess opens the file
    db_path
}

/// Run `datum dayview --json` against the fixture DB with TZ=UTC pinned,
/// and return the parsed JSON.
fn render_dayview_json(db_path: &Path) -> JsonValue {
    let bin = env!("CARGO_BIN_EXE_datum");
    let tmp_out = tempfile::NamedTempFile::with_suffix(".json").expect("tempfile");
    let out_path = tmp_out.path().to_str().unwrap().to_string();

    let out = Command::new(bin)
        .env("TZ", "UTC")
        .env("DATUM_DB", db_path)
        .env("DATUM_COLOR", "0")
        .args([
            "dayview",
            "--start-date",
            "2026-01-15",
            "--end-date",
            "2026-01-15",
            "--json",
            "--width",
            "2000",
            "--day-height",
            "100",
            "-o",
            &out_path,
        ])
        .output()
        .expect("run datum dayview");
    assert!(
        out.status.success(),
        "datum dayview failed: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );

    let raw = std::fs::read_to_string(&out_path).expect("read dayview json");
    serde_json::from_str(&raw).expect("parse dayview json")
}

/// Sub-test: end-to-end snapshot. Render the fixture entries via the
/// compiled `datum` binary with TZ=UTC and compare the JSON to the
/// committed golden. Catches drift in the layout engine, color
/// resolution, and field/state ordering simultaneously.
///
/// Regenerating: set `PARITY_REGEN=1` in the env to overwrite
/// `dayview/expected.json` with the freshly-rendered output. The same
/// flag is wired through `tests/parity/regen.sh dayview`.
#[test]
fn dayview_render_snapshot_matches_golden() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = populate_dayview_fixture_db(tmp.path());
    let actual = render_dayview_json(&db_path);

    let golden_path = fixture_dir().join("dayview/expected.json");

    if std::env::var_os("PARITY_REGEN").is_some() {
        let pretty = serde_json::to_string_pretty(&actual).unwrap();
        std::fs::write(&golden_path, format!("{pretty}\n"))
            .unwrap_or_else(|e| panic!("write {}: {e}", golden_path.display()));
        eprintln!("regenerated {}", golden_path.display());
        return;
    }

    let golden_raw = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", golden_path.display()));
    let golden: JsonValue =
        serde_json::from_str(&golden_raw).expect("golden dayview/expected.json must be valid JSON");

    if actual != golden {
        let diff = pretty_diff_json(&golden, &actual);
        panic!(
            "dayview render drift detected — see diff below.\n\
             Regen via `PARITY_REGEN=1 cargo test --test parity dayview_render_snapshot` after auditing.\n\
             {diff}"
        );
    }
}

/// Sub-test: even if the layout shape drifts, every fill color must
/// derive from `md5(field_or_state)[:6]` for entries without a spec
/// override. This is the explicit da-438y guard — the bug that motivated
/// this whole bead. Independent of the snapshot test so a layout-only
/// drift doesn't mask a color regression.
#[test]
fn dayview_fill_colors_match_md5_for_unspecced_fields() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = populate_dayview_fixture_db(tmp.path());
    let layout = render_dayview_json(&db_path);

    let days = layout["days"].as_array().expect("days array");
    assert_eq!(days.len(), 1, "fixture renders exactly one day");
    let fields = days[0]["fields"].as_array().expect("fields array");

    let mut block_count = 0usize;
    for f in fields {
        let field_name = f["field"].as_str().expect("field name");
        let blocks = f["blocks"].as_array().expect("blocks array");
        for b in blocks {
            let state = b["state_label"].as_str();
            let fill = b["fill"]["Single"].as_str().unwrap_or("");
            assert!(
                fill.starts_with('#'),
                "block fill must be a hex color: {fill:?}",
            );
            // Color resolution per `state_value_rgb` in
            // datum-commands/src/render/colors.rs:
            // * Bool(true) (state_label = "true") → md5(field) — the
            //   implicit-on case (binary field).
            // * String(s)                          → md5(s).
            // * None                               → md5(field).
            // The da-438y bug was that a missing `spec.color` was filled
            // with HSL-from-md5 instead of leaving the renderer to derive
            // md5(name)[:3]. Either path's corruption shows up here.
            let key = match state {
                None | Some("true") => field_name.to_string(),
                Some(s) => s.to_string(),
            };
            let expected = md5_hex_color(&key);
            assert_eq!(
                fill, &expected,
                "block fill drift: field={field_name} state={state:?} key={key:?}",
            );
            block_count += 1;
        }
    }
    assert!(
        block_count >= 3,
        "fixture must render at least 3 blocks; got {block_count}"
    );
}

fn md5_hex_color(name: &str) -> String {
    let mut h = Md5::new();
    h.update(name.as_bytes());
    let d = h.finalize();
    format!("#{:02x}{:02x}{:02x}", d[0], d[1], d[2])
}

fn pretty_diff_json(want: &JsonValue, got: &JsonValue) -> String {
    let want_s = serde_json::to_string_pretty(want).unwrap_or_default();
    let got_s = serde_json::to_string_pretty(got).unwrap_or_default();
    if want_s == got_s {
        return "(structural inequality without textual diff — likely float precision)".to_string();
    }
    let mut out = String::with_capacity(want_s.len() + got_s.len() + 32);
    out.push_str("--- expected (golden)\n");
    out.push_str("+++ actual (rendered)\n");
    let want_lines: Vec<&str> = want_s.lines().collect();
    let got_lines: Vec<&str> = got_s.lines().collect();
    let len = want_lines.len().max(got_lines.len());
    for i in 0..len {
        match (want_lines.get(i), got_lines.get(i)) {
            (Some(a), Some(b)) if a == b => out.push_str(&format!("  {a}\n")),
            (Some(a), Some(b)) => {
                out.push_str(&format!("- {a}\n"));
                out.push_str(&format!("+ {b}\n"));
            }
            (Some(a), None) => out.push_str(&format!("- {a}\n")),
            (None, Some(b)) => out.push_str(&format!("+ {b}\n")),
            (None, None) => break,
        }
    }
    out
}
