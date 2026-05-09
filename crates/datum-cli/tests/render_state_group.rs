//! Worked-example fixtures for da-fck — the state-transition group.
//!
//! Each fixture mirrors a worked example from the bead description:
//!
//! 1. `datum switch emotion happy 5` (block) on a field currently `sad`.
//!    LEAD = `sad` colour, TRAIL = `sad` colour (resumes after block).
//! 2. `datum switch emotion happy` (point) on a field currently `sad`.
//!    LEAD = `sad` colour, TRAIL = `happy` colour (continues forward).
//! 3. First-ever switch on a field. LEAD = IGNORED-grey, TRAIL = state colour.
//! 4. Block on an IGNORED field. LEAD = IGNORED-grey, TRAIL = IGNORED-grey.
//!
//! The asserts pin the bg-colour sequence around the state cell. Colour
//! values come from the renderer's MD5 + YIQ derivation; the test names
//! the expected colours rather than hard-coding RGB bytes so a future
//! palette tweak only affects one place.

use std::path::PathBuf;
use std::process::Command;

fn tmp_db() -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("datum-render-state-group-{pid}-{nonce}.db"));
    p
}

/// Run `datum <args>` with the colour-on test environment so the rendered
/// rows carry truecolor bg escapes that we can assert on.
fn run_color(db: &PathBuf, args: &[&str]) -> String {
    let bin = env!("CARGO_BIN_EXE_datum");
    let out = Command::new(bin)
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "1")
        .env_remove("NO_COLOR")
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

/// MD5-derived RGB triple for `name`, mirroring `render::colors::md5_rgb`.
/// Hard-coded here so the test asserts on absolute byte values without
/// depending on the renderer crate's internals.
fn md5_rgb(name: &str) -> [u8; 3] {
    use md5::Digest;
    let mut h = md5::Md5::new();
    h.update(name.as_bytes());
    let out = h.finalize();
    [out[0], out[1], out[2]]
}

/// ANSI bg escape for an MD5-derived colour.
fn bg_escape(name: &str) -> String {
    let [r, g, b] = md5_rgb(name);
    format!("\x1b[48;2;{r};{g};{b}m")
}

/// `Rgb::IGNORED = #888888` per `render/colors.rs` — bg of LEAD/TRAIL
/// slots when there's no prior or continuing state to depict (da-25l).
const IGNORED_BG_ESC: &str = "\x1b[48;2;136;136;136m";

/// Pull the second line out of the two-line `CREATE` output — that's the
/// info line we want to assert on. The first line is the action header.
fn info_line(stdout: &str) -> String {
    stdout.lines().nth(1).unwrap_or_default().to_string()
}

/// Find positions of every state-cell badge body and the surrounding LEAD
/// / TRAIL bg escapes. `state_body` is the bare state label (no padding) —
/// per da-yiy the cell is `paint_bg(rgb, label)` with no inline space
/// padding, and a single uncoloured space separates it from each slot.
fn assert_lead_then_state_then_trail_bg(
    rendered: &str,
    state_body: &str,
    expected_lead_bg: &str,
    expected_state_bg: &str,
    expected_trail_bg: &str,
) {
    // The output structure for the trio is (da-25l):
    //   <lead_bg><lead_fg> <RESET><state_bg><state_fg>body<RESET><trail_bg><trail_fg> <RESET>
    // LEAD/cell/TRAIL are flush — no whitespace between escape groups.
    let lead_idx = rendered
        .find(expected_lead_bg)
        .unwrap_or_else(|| panic!("LEAD bg {expected_lead_bg:?} not found in:\n{rendered:?}"));
    let body_idx = rendered[lead_idx..]
        .find(state_body)
        .map(|i| lead_idx + i)
        .unwrap_or_else(|| {
            panic!("state body {state_body:?} not found after LEAD in:\n{rendered:?}")
        });
    // The state-cell bg escape sits between LEAD and the body.
    let between = &rendered[lead_idx..body_idx];
    assert!(
        between.contains(expected_state_bg),
        "expected state-cell bg {expected_state_bg:?} between LEAD and body, got:\n{between:?}"
    );
    // The TRAIL bg sits after the body.
    let after = &rendered[body_idx + state_body.len()..];
    assert!(
        after.contains(expected_trail_bg),
        "expected TRAIL bg {expected_trail_bg:?} after body, got:\n{after:?}"
    );
}

// ─── Worked example 1 — block over prior `sad` ───────────────────────────────

#[test]
fn switch_block_resumes_prior_state() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    // Seed prior state: switch emotion sad. Place it well in the past so
    // the (-5min) block fixture sits cleanly after it.
    run_color(&db, &["switch", "emotion", "sad", "-yy"]);

    // Now: 5-minute happy block. `--dur 5m` closes the block at end_time
    // = now, with start_time = now - 5min.
    let out = run_color(&db, &["switch", "emotion", "happy", "--dur", "5m"]);
    let line = info_line(&out);

    // LEAD = sad colour, state cell = happy colour, TRAIL = sad colour
    // (sad resumes after the 5-minute block closes).
    assert_lead_then_state_then_trail_bg(
        &line,
        "happy",
        &bg_escape("sad"),
        &bg_escape("happy"),
        &bg_escape("sad"),
    );

    let _ = std::fs::remove_file(&db);
}

// ─── Worked example 2 — switch point continuing forward ──────────────────────

#[test]
fn switch_point_trail_uses_new_state_color() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    run_color(&db, &["switch", "emotion", "sad", "-yy"]);

    // Switch to happy with no duration (point).
    let out = run_color(&db, &["switch", "emotion", "happy"]);
    let line = info_line(&out);

    // LEAD = sad, state cell = happy, TRAIL = happy (continues forward).
    assert_lead_then_state_then_trail_bg(
        &line,
        "happy",
        &bg_escape("sad"),
        &bg_escape("happy"),
        &bg_escape("happy"),
    );

    let _ = std::fs::remove_file(&db);
}

// ─── Worked example 3 — first-ever entry on a field ──────────────────────────

#[test]
fn first_switch_uses_empty_lead_slot() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    let out = run_color(&db, &["switch", "mood", "happy"]);
    let line = info_line(&out);

    // LEAD = IGNORED-grey (no prior tracked state), state cell = happy,
    // TRAIL = happy.
    assert_lead_then_state_then_trail_bg(
        &line,
        "happy",
        IGNORED_BG_ESC,
        &bg_escape("happy"),
        &bg_escape("happy"),
    );

    let _ = std::fs::remove_file(&db);
}

// ─── Worked example 4 — block on an IGNORED field ───────────────────────────

#[test]
fn block_on_ignored_field_uses_empty_trail_slot() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    // Mark the field as IGNORED in the past so both lead and trail
    // resolve to the IGNORED sentinel.
    run_color(&db, &["switch", "task", "IGNORED", "-yy"]);

    // 5-minute block on an IGNORED field.
    let out = run_color(&db, &["switch", "task", "doing", "--dur", "5m"]);
    let line = info_line(&out);

    // LEAD = IGNORED-grey (IGNORED collapses), state cell = doing,
    // TRAIL = IGNORED-grey (IGNORED resumes after block ends).
    assert_lead_then_state_then_trail_bg(
        &line,
        "doing",
        IGNORED_BG_ESC,
        &bg_escape("doing"),
        IGNORED_BG_ESC,
    );

    let _ = std::fs::remove_file(&db);
}

// ─── Cross-cutting: no directional arrow glyphs anywhere ─────────────────────

#[test]
fn rendered_rows_carry_no_arrow_glyphs() {
    let db = tmp_db();
    let _ = std::fs::remove_file(&db);

    // Exercise the verbs that previously emitted arrows.
    run_color(&db, &["switch", "f", "a", "-yy"]);
    let switch_out = run_color(&db, &["switch", "f", "b"]);
    let block_out = run_color(&db, &["switch", "f", "c", "--dur", "5m"]);
    let start_out = run_color(&db, &["start", "g"]);
    let stop_out = run_color(&db, &["stop", "g"]);

    let tail_out = run_color(&db, &["tail"]);

    for src in [&switch_out, &block_out, &start_out, &stop_out, &tail_out] {
        for arrow in ["→", "←", "↑", "↓", "▶"] {
            assert!(
                !src.contains(arrow),
                "rendered output carries forbidden arrow {arrow:?}:\n{src}"
            );
        }
    }

    let _ = std::fs::remove_file(&db);
}
