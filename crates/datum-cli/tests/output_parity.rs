//! End-to-end snapshot-style tests for the ported datum output format.
//!
//! These run the compiled `datum` binary so they exercise the full arg-parse →
//! dispatch → output path. Colors are forced off via `DATUM_COLOR=0` so the
//! assertions stay portable across terminals / CI.

use std::path::PathBuf;
use std::process::Command;

fn tmp_db() -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("datum-output-parity-{pid}-{nonce}.db"));
    p
}

fn run(db: &PathBuf, args: &[&str]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_datum");
    let out = Command::new(bin)
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("run datum");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "datum {:?} failed: stderr={stderr}",
        args,
    );
    (stdout, stderr)
}

#[test]
fn occur_emits_two_line_create_with_hid_and_extra() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    let (stdout, _) = run(&db, &["occur", "coffee", "amount=200"]);

    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 3, "expected 3+ lines, got: {stdout:?}");

    // Header: action + hid + slug (datum's user-facing `field:time` id).
    assert!(lines[0].contains("CREATE"), "line 0: {:?}", lines[0]);
    assert!(
        lines[0].contains("coffee:"),
        "expected slug (field:time) in header: {:?}",
        lines[0]
    );

    // Info line: time + field
    assert!(lines[1].contains("coffee"), "line 1: {:?}", lines[1]);

    // Extra data — per da-gvj1 the user-data block renders `key: value`
    // (numbers bare per rule 11), replacing the legacy `key=value` form.
    assert!(
        stdout.contains("amount: 200"),
        "expected amount in extra data: {stdout:?}",
    );

    let _ = std::fs::remove_file(&db);
}

#[test]
fn tail_table_flag_emits_aligned_table_with_hid_column() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    run(&db, &["occur", "coffee"]);
    run(&db, &["switch", "mood", "happy"]);
    let (stdout, _) = run(&db, &["tail", "--table"]);

    // Table header mentions our columns.
    assert!(stdout.contains("time"), "{stdout}");
    assert!(stdout.contains("field"), "{stdout}");
    assert!(stdout.contains("hid"), "{stdout}");
    // Data rows present.
    assert!(stdout.contains("coffee"), "{stdout}");
    assert!(stdout.contains("mood"), "{stdout}");

    let _ = std::fs::remove_file(&db);
}

/// Per da-6t0: default `datum tail` always renders through the tabular
/// path. The header row (`time | field | state | dur | hid` — empty
/// columns dropped) is on by default, columns are aligned, and the
/// `hid` column tails every data row.
#[test]
fn tail_default_emits_table_with_header_and_hid_column() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    run(&db, &["occur", "coffee"]);
    run(&db, &["switch", "mood", "happy"]);
    let (stdout, _) = run(&db, &["tail"]);

    let header = stdout.lines().next().unwrap_or("");
    for tok in ["time", "field", "hid"] {
        assert!(
            header.contains(tok),
            "default tail header missing {tok:?}:\n{stdout}"
        );
    }
    // Data rows retain the field names in plain text.
    assert!(stdout.contains("coffee"), "{stdout}");
    assert!(stdout.contains("mood"), "{stdout}");

    let _ = std::fs::remove_file(&db);
}

/// Defect 1 (da-vkj): a second `switch` on a field must NOT re-trigger the
/// first-tracked indicator. Pre-fix, `is_first_occurrence` probed spec
/// existence (never created for switch), firing on every write.
///
/// Per da-25l the LEAD slot for switch verbs no longer carries a `∅`
/// glyph — first-tracked is conveyed by the IGNORED-grey LEAD bg, which
/// is colour-only. Test: neither switch should carry `∅` (since neither
/// goes through the occur path that would emit one).
#[test]
fn second_switch_on_same_field_suppresses_first_tracked_marker() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    let (first, _) = run(&db, &["switch", "myfield", "abc"]);
    let (second, _) = run(&db, &["switch", "myfield", "abcdefg"]);

    assert!(
        !first.contains('∅'),
        "first switch must not carry ∅ (LEAD is now IGNORED-bg, no glyph):\n{first}"
    );
    assert!(
        !second.contains('∅'),
        "second switch on same field must not re-render ∅:\n{second}"
    );
    // Per da-25l the prior-state transition is conveyed via the LEAD slot's
    // bg colour. Both rows must render without `∅`.

    let _ = std::fs::remove_file(&db);
}

#[test]
fn show_none_silences_action_output() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    let (stdout, _) = run(&db, &["occur", "coffee", "--show", "none"]);
    assert_eq!(stdout, "", "expected silent output, got: {stdout:?}");
    let _ = std::fs::remove_file(&db);
}

#[test]
fn show_minimal_emits_header_only() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    let (stdout, _) = run(&db, &["occur", "coffee", "amount=1", "--show", "minimal"]);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "minimal should emit 1 line, got: {stdout:?}"
    );
    assert!(lines[0].contains("CREATE"));

    let _ = std::fs::remove_file(&db);
}

#[test]
fn update_emits_update_action_tag() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    run(&db, &["occur", "coffee", "amount=1", "--id", "pin"]);
    let (stdout, _) = run(&db, &["update", "coffee:pin", "amount=5"]);
    assert!(stdout.contains("UPDATE"), "expected UPDATE: {stdout:?}");
    assert!(
        stdout.contains("amount: 5"),
        "expected new value: {stdout:?}"
    );
    let _ = std::fs::remove_file(&db);
}

#[test]
fn delete_emits_delete_action_tag() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    run(&db, &["occur", "coffee", "--id", "pin"]);
    let (stdout, _) = run(&db, &["del", "coffee:pin", "--force"]);
    assert!(stdout.contains("DELETE"), "expected DELETE: {stdout:?}");
    let _ = std::fs::remove_file(&db);
}

/// da-jose: at the default show, `-o` appends the format string to the
/// stock header + info lines (replacing the data block), matching datumv2.
/// An explicit `--show format` collapses to the format string alone.
#[test]
fn format_string_renders_custom_template() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    // Default show: header + info line + format string, and the data block
    // (the `amount: 7` columnar view) is replaced by the format string.
    let (stdout, _) = run(
        &db,
        &["occur", "coffee", "amount=7", "-o", "%field%:%amount%"],
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.len() >= 3,
        "default `-o` should keep the two info lines plus the format string, got: {stdout:?}"
    );
    assert!(
        lines[0].starts_with("CREATE"),
        "first line should be the action header, got: {stdout:?}"
    );
    assert!(
        lines[1].contains("coffee"),
        "second line should be the main info line, got: {stdout:?}"
    );
    assert_eq!(
        lines.last().copied(),
        Some("coffee:7"),
        "format string should be the final line, got: {stdout:?}"
    );
    assert!(
        !stdout.contains("amount: 7"),
        "format string should replace the data block, got: {stdout:?}"
    );

    // Explicit `--show format`: format string only.
    let (stdout, _) = run(
        &db,
        &[
            "occur",
            "coffee",
            "amount=7",
            "--show",
            "format",
            "-o",
            "%field%:%amount%",
        ],
    );
    assert_eq!(
        stdout.trim(),
        "coffee:7",
        "`--show format` should emit only the format string, got: {stdout:?}"
    );
    let _ = std::fs::remove_file(&db);
}

#[test]
fn header_has_action_hid_and_slug_not_ulid() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);
    let (stdout, _) = run(&db, &["occur", "coffee"]);
    // Header layout (colors off, whitespace-collapsed): CREATE <hid> <slug>
    // The slug is datum-compatible `field:YYYY-MM-DDTHH:MMZ`, NOT the raw ULID.
    let first = stdout.lines().next().unwrap();
    let toks: Vec<&str> = first.split_whitespace().collect();
    assert_eq!(toks.len(), 3, "expected 3 header tokens, got {toks:?}");
    assert_eq!(toks[0], "CREATE");

    let hid = toks[1];
    assert_eq!(hid.len(), 5, "hid length: {hid:?}");
    assert_eq!(hid, hid.to_lowercase(), "hid should be lowercase: {hid:?}");

    let slug = toks[2];
    assert!(
        slug.starts_with("coffee:"),
        "expected slug starting with field: {slug:?}"
    );
    // Internal ULID (26-char Crockford base32 starting "01") must NOT appear as the id.
    assert!(
        !(slug.len() == 26 && slug.starts_with("01")),
        "slug looks like a ULID: {slug:?}"
    );
    let _ = std::fs::remove_file(&db);
}
