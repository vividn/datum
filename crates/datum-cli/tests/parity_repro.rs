//! Parity-diff regression suite for the `da-vkj` reproducer.
//!
//! The user-reported reproducer ran the classic two-switch sequence against
//! a clean DB and asked for the write-action + `datum tail` rendering to
//! match the §2/§6/§7 layout in `docs/visual-parity-analysis.md`. These
//! tests pin the ANSI-stripped render so future renderer drift can't
//! silently regress first-time-tracked detection, prior-state indicator,
//! or the tail info-line wiring.
//!
//! Goldens are ANSI-stripped: colour comes from the `visual_dump_all_fixtures`
//! harness (`cargo test -p datum-commands --test render_fixtures
//! visual_dump_all_fixtures -- --ignored --nocapture`). Keeping the asserts
//! colour-free here means CI doesn't depend on terminal capabilities.

use std::path::PathBuf;
use std::process::Command;

fn tmp_db() -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("datum-parity-repro-{pid}-{nonce}.db"));
    p
}

fn run(db: &PathBuf, args: &[&str]) -> String {
    let bin = env!("CARGO_BIN_EXE_datum");
    let out = Command::new(bin)
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("run datum");
    assert!(
        out.status.success(),
        "datum {:?} failed: stderr={}",
        args,
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Pick the info line from a CREATE/UPDATE two-line write output.
///
/// `show_action_with_context` emits header (action + hid + slug), then the
/// info line, then optional extras. Line 1 (zero-indexed 1) is the info
/// line we care about.
fn info_line_of(stdout: &str) -> String {
    stdout.lines().nth(1).unwrap_or_default().trim().to_string()
}

/// Strip the timestamp prefix so goldens don't depend on wall-clock. The
/// write-path info line starts with `HH:MM:SS+<tz>` followed by two spaces
/// before the field badge; the post-timestamp tail is what we pin.
fn after_timestamp(line: &str) -> &str {
    // Scan to the first run of 2+ spaces (the badge separator).
    let idx = line
        .find("  ")
        .expect("info line must contain a 2-space separator after timestamp");
    line[idx..].trim()
}

#[test]
fn switch_switch_tail_reproducer_matches_spec() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    // Per da-25l the state-transition group renders as
    // `[LEAD][state cell][TRAIL]` flush — no inner whitespace. LEAD/TRAIL
    // are single bg-coloured cells (no `∅` glyph); when there's no prior
    // tracked state the LEAD is painted with the IGNORED grey. The badge
    // gap to the state group is 1 space. ANSI-stripped, every slot
    // collapses to a single space, so the asserts pin the layout via
    // visible character counts.

    // First switch on a brand-new field: LEAD = IGNORED bg-space, TRAIL
    // = new-state bg-space. Stripped: 1 join space + 1 LEAD space before
    // the state cell, then 1 TRAIL space (trimmed).
    let first = run(&db, &["switch", "myfield", "abc"]);
    let first_info = info_line_of(&first);
    assert_eq!(
        after_timestamp(&first_info),
        "myfield  abc",
        "first-switch info line mismatch:\n{first}"
    );

    // Second switch on the same field: LEAD = prior state colour, TRAIL
    // = new state colour. Both slots render as a single space when stripped.
    let second = run(&db, &["switch", "myfield", "abcdefg"]);
    let second_info = info_line_of(&second);
    assert_eq!(
        after_timestamp(&second_info),
        "myfield  abcdefg",
        "second-switch info line mismatch:\n{second}"
    );

    // Per da-6t0, `datum tail myfield` now always renders through the
    // tabular path: a header row followed by aligned data rows that include
    // the `hid` column. The data-row layout (after the timestamp prefix)
    // pads the field column to its widest visible width before the state
    // cell, and the hid column tails the line.
    let tail = run(&db, &["tail", "myfield"]);
    let mut tail_lines = tail.lines();
    let header = tail_lines.next().unwrap_or("");
    for tok in ["time", "field", "state", "hid"] {
        assert!(header.contains(tok), "header missing {tok:?}:\n{tail}");
    }
    let data: Vec<&str> = tail_lines.collect();
    assert_eq!(data.len(), 2, "expected 2 data rows:\n{tail}");
    for row in &data {
        assert!(row.contains("myfield"), "row missing field:\n{row:?}");
    }
    assert!(data[0].contains("abc"), "first row state missing:\n{tail}");
    assert!(
        data[1].contains("abcdefg"),
        "second row state missing:\n{tail}"
    );

    let _ = std::fs::remove_file(&db);
}

/// `datum tail --table` is now equivalent to the default tail path (per
/// da-6t0 the gate is gone). The flag is kept for backward CLI compat;
/// this test pins that it still produces a valid header + data layout.
#[test]
fn tail_table_flag_preserves_legacy_columnar_view() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    run(&db, &["switch", "myfield", "abc"]);
    run(&db, &["switch", "myfield", "abcdefg"]);

    let stdout = run(&db, &["tail", "myfield", "--table"]);
    let first = stdout.lines().next().unwrap_or("");
    // Header tokens drop to any column that has at least one non-empty
    // cell — at minimum "time", "field", "state", "hid" must appear.
    for tok in ["time", "field", "state", "hid"] {
        assert!(
            first.contains(tok),
            "table header missing {tok:?}:\n{stdout}"
        );
    }
    // Data rows retain the field value.
    assert!(stdout.contains("myfield"), "{stdout}");

    let _ = std::fs::remove_file(&db);
}

/// Defect 1 direct pin: the first-tracked `∅` marker MUST NOT appear on
/// the second write to a field, regardless of verb. This guards against
/// the specific regression the bead reports (spec-based detection firing
/// on every switch because switches never create a spec).
#[test]
fn first_tracked_marker_is_single_shot_across_mixed_verbs() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    // Sequence: occur (creates spec via ensure_tracked), then switch.
    // Even with a spec present, the second write MUST still suppress ∅.
    let first = run(&db, &["occur", "mixfield"]);
    let switch = run(&db, &["switch", "mixfield", "done"]);

    assert!(
        first.contains('∅'),
        "first occur on new field must carry ∅:\n{first}"
    );
    assert!(
        !switch.contains('∅'),
        "subsequent switch on existing field must not re-fire ∅:\n{switch}"
    );

    let _ = std::fs::remove_file(&db);
}
