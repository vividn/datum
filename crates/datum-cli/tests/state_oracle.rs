//! da-u5l1o: one oracle decides a field's state at an instant, so the
//! `!` repeat mark, `datum check`'s `repeated_state` and `--if-changed`
//! cannot disagree. vividn's report: after a timed switch with no
//! `following_state` ended, the field was back in the state it had
//! interrupted — but the render and the check still read it as the
//! block's state, so switching to that state again printed `!` and warned.
//! Subprocess, so the printed action is the real one.

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        }
    }

    /// Run and return stdout, asserting exit 0.
    fn ok(&self, args: &[&str]) -> String {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.tmp.path().join("t.db"))
            .env("DATUM_HOME", self.tmp.path())
            .env("DATUM_CONFIG_HOME", self.tmp.path())
            .env("NO_COLOR", "1")
            .env("TZ", "UTC")
            .args(args)
            .output()
            .expect("spawn datum");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(
            out.status.code(),
            Some(0),
            "`datum {}`: stdout={stdout} stderr={}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        stdout
    }
}

/// state1, then a five-minute block of state2 that ended four minutes
/// ago, then state2 again: state1 was back in effect, so it is a change.
fn nates_history(env: &Env) {
    env.ok(&["switch", "field", "state1", "-qqq"]);
    env.ok(&["switch", "field", "state2", "-q", "5"]);
}

#[test]
fn a_switch_after_a_finished_block_is_not_a_repeat() {
    let env = Env::new();
    nates_history(&env);
    let out = env.ok(&["switch", "field", "state2", "-t", "-1"]);
    assert!(out.contains("CREATE"), "{out}");
    assert!(!out.contains("!state2"), "state1 had resumed:\n{out}");
    let check = env.ok(&["check"]);
    assert!(!check.contains("repeated_state"), "{check}");
}

#[test]
fn if_changed_writes_it_and_marks_no_repeat() {
    let env = Env::new();
    nates_history(&env);
    let out = env.ok(&["switch", "field", "state2", "-t", "-1", "--if-changed"]);
    assert!(
        out.contains("CREATE"),
        "the state was state1, so it writes:\n{out}"
    );
    assert!(
        !out.contains("!state2"),
        "and what it wrote is no repeat:\n{out}"
    );
}

/// vividn, 2026-09-29: «an occur should not modify the state». So an occur
/// inside `drinking` leaves it `drinking`, and switching to it again after
/// the occur is a repeat.
#[test]
fn an_occur_leaves_the_state_so_a_switch_back_is_a_repeat() {
    let env = Env::new();
    env.ok(&["switch", "coffee", "drinking", "-qqq"]);
    env.ok(&["occur", "coffee", "-qq"]);
    let out = env.ok(&["switch", "coffee", "drinking", "-q"]);
    assert!(out.contains("!drinking"), "{out}");
    let check = env.ok(&["check"]);
    assert!(check.contains("repeated_state"), "{check}");
}

#[test]
fn a_genuine_repeat_is_still_marked_and_flagged() {
    let env = Env::new();
    env.ok(&["switch", "field", "state2", "-qq"]);
    let out = env.ok(&["switch", "field", "state2", "-q"]);
    assert!(out.contains("!state2"), "{out}");
    let check = env.ok(&["check"]);
    assert!(check.contains("repeated_state"), "{check}");
}

/// da-22750, vividn's report: a moment (dur MOMENT, or zero) names a state
/// for an instant and switches nothing. The day draws one happy block
/// under both dots, each dot in its own state's colour; tail's next switch
/// comes from happy; stats counts happy's whole six hours.
#[test]
fn a_moment_state_resumes_the_state_before_it() {
    let env = Env::new();
    let day = ["-d", "2025-01-08"];
    env.ok(&[&["switch", "mood", "happy", "-t", "09:00"][..], &day].concat());
    env.ok(&[
        "setup",
        "mood",
        "--states",
        "startled,startled2",
        "--state-color",
        "startled=#ff0000",
        "--state-color",
        "startled2=#0000ff",
        "--state-description",
        "startled=x",
        "--state-description",
        "startled2=y",
    ]);
    env.ok(&[
        &["switch", "mood", "startled", "-t", "11:00", "dur=MOMENT"][..],
        &day,
    ]
    .concat());
    env.ok(&[
        &["switch", "mood", "startled2", "-t", "12:00", "0"][..],
        &day,
    ]
    .concat());
    env.ok(&[&["switch", "mood", "tired", "-t", "15:00"][..], &day].concat());

    let view: serde_json::Value =
        serde_json::from_str(&env.ok(&[&["dayview", "--json"][..], &day].concat())).unwrap();
    let mood = &view["days"][0]["fields"][0];
    let titles: Vec<_> = mood["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["mood: happy 09:00–15:00", "mood: tired 15:00 →"]);
    let dots: Vec<_> = mood["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["fg_color"].as_str().unwrap())
        .collect();
    assert_eq!(dots, ["#ff0000", "#0000ff"]);

    let tail: serde_json::Value = serde_json::from_str(&env.ok(&["tail", "--json"])).unwrap();
    let tired = tail["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["state"] == "tired")
        .unwrap();
    assert_eq!(tired["previous_state"], "happy");

    let stats = env.ok(&["stats", "mood"]);
    assert!(
        stats
            .lines()
            .any(|l| l.trim_start().starts_with("6h") && l.ends_with("happy")),
        "{stats}"
    );
}

/// da-vwymi, volya's db: 131 harmless double closes hid 7 repeated opens
/// that held most of her recorded hours. The check lists a repeated open
/// by the span it sits in, longest first, then a repeated close, all one
/// `repeated_state` (da-wdfsw), whose message says the close is usually a
/// duplicate.
#[test]
fn check_lists_repeated_opens_by_longest_span_then_repeated_closes() {
    let env = Env::new();
    let at = |day: &str, t: &str| ["-d", day, "-t", t].map(String::from);
    let log = |args: &[&str], day: &str, t: &str| {
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string())
            .chain(at(day, t))
            .collect();
        env.ok(&args.iter().map(String::as_str).collect::<Vec<_>>());
    };
    log(
        &["switch", "activity", "researching"],
        "2025-08-05",
        "12:00",
    );
    log(
        &["switch", "activity", "researching"],
        "2025-08-05",
        "12:01",
    );
    log(&["switch", "activity", "talking"], "2025-08-05", "13:00");
    // Logged on the 8th while talking had been open since the 5th.
    log(&["switch", "activity", "talking"], "2025-08-08", "09:00");
    log(&["end", "activity"], "2025-08-08", "14:00");
    log(&["end", "activity"], "2025-08-08", "14:05");
    let check = env.ok(&["check"]);
    assert!(check.contains("[WARN ] repeated_state (x3)"), "{check}");
    assert_eq!(check.matches("repeated_state").count(), 1, "{check}");
    assert!(check.contains("MISSING"), "{check}");
    assert!(check.contains("usually a duplicate"), "{check}");
    assert!(check.contains("`datum del <hid>`"), "{check}");
    let long = "talking for 73h: 2025-08-05 13:00 → 2025-08-08 14:00";
    let short = "researching for 1h: 2025-08-05 12:00 → 2025-08-05 13:00";
    let (Some(long_at), Some(short_at)) = (check.find(long), check.find(short)) else {
        panic!("both spans named:\n{check}");
    };
    assert!(long_at < short_at, "the long span first:\n{check}");
    let end_row = check.rfind("14:05").expect("the double end listed");
    assert!(short_at < end_row, "the end after the spans:\n{check}");
    let json = env.ok(&["check", "--json"]);
    assert_eq!(json.matches("\"repeated_state\"").count(), 1, "{json}");
}

/// da-vwymi review: a state with a newline in it renders as two table
/// lines, and its span still stands under its own row, on one line.
#[test]
fn a_multiline_state_keeps_its_span_under_its_row() {
    let env = Env::new();
    env.ok(&[
        "switch",
        "act",
        "two\nlines",
        "-d",
        "2025-08-01",
        "-t",
        "10:00",
    ]);
    env.ok(&[
        "switch",
        "act",
        "two\nlines",
        "-d",
        "2025-08-02",
        "-t",
        "10:00",
    ]);
    env.ok(&["switch", "act", "x", "-d", "2025-08-02", "-t", "11:00"]);
    env.ok(&["switch", "act", "x", "-d", "2025-08-02", "-t", "11:30"]);
    env.ok(&["switch", "act", "y", "-d", "2025-08-02", "-t", "12:00"]);
    let check = env.ok(&["check"]);
    let lines: Vec<&str> = check.lines().collect();
    let span = |s: &str| {
        lines
            .iter()
            .position(|l| l.contains(s))
            .unwrap_or_else(|| panic!("{s}:\n{check}"))
    };
    let two = span("↳ two\\nlines for 25h: 2025-08-01 10:00 → 2025-08-02 11:00");
    let x = span("↳ x for 1h: 2025-08-02 11:00 → 2025-08-02 12:00");
    assert!(lines[x - 1].contains("!x"), "{check}");
    assert!(lines[two - 1].contains("lines"), "{check}");
    assert!(lines[two - 2].contains("!two"), "{check}");
}
