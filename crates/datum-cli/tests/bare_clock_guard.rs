//! A test build refuses a log at a clock today, which passes or fails by the
//! hour the test runs at (da-5kpsw, `refuse_bare_clock`). It refuses today's
//! clock at any hour, so this test does not depend on when it runs.

use std::process::Output;

const SAYS: &str = "a clock today, so it passes or fails by the hour";

fn log(dir: &std::path::Path, armed: &str, line: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.join("t.db"))
        .env("DATUM_HOME", dir)
        .env("DATUM_CONFIG_HOME", dir)
        .env("DATUM_TEST_REFUSE_BARE_CLOCK", armed)
        .args(line)
        .args(["--show", "none"])
        .output()
        .unwrap()
}

#[test]
fn a_clock_today_is_refused_and_any_other_time_is_not() {
    let dir = tempfile::tempdir().unwrap();
    for (i, time) in [
        &["-t", "10:00"][..],
        &["-d", "today", "-t", "10"],
        &["-t", "today 10am"],
        &["start_time=9am"],
    ]
    .into_iter()
    .enumerate()
    {
        let field = format!("refused{i}");
        let line = [&["moment", field.as_str()][..], time].concat();
        let out = log(dir.path(), "1", &line);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && stderr.contains(SAYS),
            "{line:?}: {stderr}"
        );
        // Off, the same line logs: the refusal is the guard's alone.
        let out = log(dir.path(), "0", &line);
        assert!(out.status.success(), "{line:?} at 0: {out:?}");
    }
    for (i, time) in [
        &["-y", "-t", "10:00"][..],
        &["-d", "+1", "-t", "10:00"],
        &["-t", "+5"],
        &["-t", "-5"],
        &[],
        &["-t", "2026-01-05T10:00:00+01:00"],
    ]
    .into_iter()
    .enumerate()
    {
        let field = format!("passed{i}");
        let line = [&["moment", field.as_str()][..], time].concat();
        let out = log(dir.path(), "1", &line);
        assert!(out.status.success(), "{line:?}: {out:?}");
    }
}
