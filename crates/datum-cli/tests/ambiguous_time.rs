//! da-6i7l1: a wall time a fall-back night repeats — 02:30 on 2026-10-25 in
//! Europe/Berlin is 00:30Z and 01:30Z — is asked about, never read as the
//! first in silence. A terminal is asked which, datum.day is told the
//! question (exit 12) and answers on a second run's stdin, and anywhere else
//! the line is refused naming both full spellings. So is one a `*_time`
//! key names (da-job98), spelled as the key.

use std::process::{Command, Output, Stdio};

use rusqlite::Connection;
use tempfile::TempDir;

const LINE: [&str; 6] = ["occur", "tea", "-d", "2026-10-25", "-t", "02:30"];
const FIRST: &str = "-t 2026-10-25T02:30:00+02:00";
const SECOND: &str = "-t 2026-10-25T02:30:00+01:00";

fn datum(dir: &TempDir) -> Command {
    let mut cmd = bare(dir);
    cmd.args(LINE);
    cmd
}

fn run(dir: &TempDir, line: &[&str]) -> Command {
    let mut cmd = bare(dir);
    cmd.args(line);
    cmd
}

fn bare(dir: &TempDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
    cmd.env("DATUM_DB", dir.path().join("t.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path())
        .env("TZ", "Europe/Berlin")
        .env("NO_COLOR", "1")
        .env_remove("DATUM_NO_DEFAULT_ANSWER")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Each tea's slug and zone.
fn teas(dir: &TempDir) -> Vec<(String, Option<String>)> {
    rows(
        dir,
        "SELECT slug, occur_time_tz FROM entries WHERE field = 'tea'",
    )
}

fn rows(dir: &TempDir, sql: &str) -> Vec<(String, Option<String>)> {
    let Ok(conn) = Connection::open(dir.path().join("t.db")) else {
        return vec![];
    };
    let Ok(mut rows) = conn.prepare(sql) else {
        return vec![];
    };
    rows.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// The page's run of `line`, `answer` on its stdin.
fn page(dir: &TempDir, line: &[&str], answer: &str) -> Output {
    use std::io::Write;
    let mut child = run(dir, line)
        .env("DATUM_NO_DEFAULT_ANSWER", "1")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(answer.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// The `DATUM_CHOOSE=` line a page is told.
fn options(out: &Output) -> String {
    let err = said(out);
    assert_eq!(out.status.code(), Some(12), "{err}");
    err.trim_end()
        .lines()
        .last()
        .unwrap_or_default()
        .to_string()
}

/// What the second reading stores: 01:30Z, in the zone it was read in.
fn second_stored() -> Vec<(String, Option<String>)> {
    vec![(
        "tea:2026-10-25T01:30:00.000Z".to_string(),
        Some("Europe/Berlin".to_string()),
    )]
}

fn said(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn no_one_to_ask_refuses_naming_both_spellings() {
    let dir = tempfile::tempdir().unwrap();
    let out = datum(&dir).stdin(Stdio::null()).output().unwrap();
    let err = said(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(
        err.contains(&format!("`{FIRST}`")) && err.contains(&format!("`{SECOND}`")),
        "{err}"
    );
    assert_eq!(teas(&dir), vec![], "nothing written");
}

#[cfg(unix)]
#[test]
fn a_terminal_is_asked_and_the_second_is_stored() {
    use std::io::Write;
    use std::os::fd::FromRawFd;

    let dir = tempfile::tempdir().unwrap();
    let (mut master, mut slave) = (0, 0);
    // SAFETY: openpty writes two fresh descriptors, each owned once below.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(opened, 0, "openpty");
    let (mut master, slave) = unsafe {
        (
            std::fs::File::from_raw_fd(master),
            std::fs::File::from_raw_fd(slave),
        )
    };
    // datum.day's env changes nothing on a terminal: it is still asked.
    let child = datum(&dir)
        .env("DATUM_NO_DEFAULT_ANSWER", "1")
        .stdin(slave)
        .spawn()
        .unwrap();
    master.write_all(b"2\n").unwrap();
    let out = child.wait_with_output().unwrap();
    let err = said(&out);
    assert!(out.status.success(), "{err}");
    assert!(
        err.contains("happens twice in Europe/Berlin")
            && err.contains(&format!("1  {FIRST}"))
            && err.contains(&format!("2  {SECOND}")),
        "the question lists both: {err}"
    );
    assert_eq!(teas(&dir), second_stored());
}

/// datum.day's turn: the question and its options on stderr, exit 12 and
/// nothing written; then the line again with the second option on stdin.
#[test]
fn the_page_is_told_the_options_and_answers_on_a_second_run() {
    let dir = tempfile::tempdir().unwrap();
    let page = |answer: &str| page(&dir, &LINE, answer);

    let asked = page("");
    let err = said(&asked);
    assert_eq!(asked.status.code(), Some(12), "{err}");
    assert_eq!(
        err.trim_end().lines().last(),
        Some(format!("DATUM_CHOOSE={FIRST}|{SECOND}").as_str()),
        "{err}"
    );
    assert_eq!(teas(&dir), vec![], "unanswered: nothing written");

    let answered = page(&format!("{SECOND}\n"));
    assert!(answered.status.success(), "{}", said(&answered));
    assert_eq!(said(&answered), "", "asked once, not again");
    assert_eq!(teas(&dir), second_stored());
}

/// da-z3fzv: retime is asked as a log is, of the time it builds from the
/// entry's own: `-t 02:30` on a tea logged at 01:00 that night.
#[test]
fn retime_is_asked_which_reading() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let logged = bare(&dir).args(&LINE[..5]).arg("01:00").output().unwrap();
    assert!(logged.status.success(), "{}", said(&logged));
    let before = teas(&dir);
    let retime = |page: bool, answer: &str| {
        let mut cmd = bare(&dir);
        if page {
            cmd.env("DATUM_NO_DEFAULT_ANSWER", "1");
        }
        let mut child = cmd
            .args(["retime", "-t", "02:30"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(answer.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };

    let refused = retime(false, "");
    let err = said(&refused);
    assert_eq!(refused.status.code(), Some(1), "{err}");
    assert!(
        err.contains(&format!("`{FIRST}`")) && err.contains(&format!("`{SECOND}`")),
        "{err}"
    );
    assert_eq!(teas(&dir), before, "nothing moved");

    let asked = retime(true, "");
    let err = said(&asked);
    assert_eq!(asked.status.code(), Some(12), "{err}");
    assert_eq!(
        err.trim_end().lines().last(),
        Some(format!("DATUM_CHOOSE={FIRST}|{SECOND}").as_str()),
        "{err}"
    );
    assert_eq!(teas(&dir), before, "unanswered: nothing moved");

    let answered = retime(true, &format!("{SECOND}\n"));
    assert!(answered.status.success(), "{}", said(&answered));
    assert_eq!(teas(&dir), second_stored());
}

/// da-job98: an `occur_time=` key is a wall time as `-t` is, and refused
/// where no one can be asked, naming both as the key.
#[test]
fn a_time_key_no_one_can_be_asked_about_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(&dir, &["occur", "tea", "occur_time=2026-10-25T02:30"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let err = said(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(
        err.contains("`occur_time=2026-10-25T02:30:00+02:00` (the first)")
            && err.contains("`occur_time=2026-10-25T02:30:00+01:00` (the second)"),
        "{err}"
    );
    assert_eq!(teas(&dir), vec![], "nothing written");

    // The pair's start is its own wall time.
    let out = run(
        &dir,
        &[
            "start",
            "nap",
            "start_time=2026-10-25T02:30",
            "end_time=2026-10-25T03:30",
        ],
    )
    .stdin(Stdio::null())
    .output()
    .unwrap();
    let err = said(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(
        err.contains("`start_time=2026-10-25T02:30:00+01:00` (the second)"),
        "{err}"
    );
}

/// Two keys on one line are two questions, the page answering the first
/// before it is told the second, and each key stores the reading picked.
#[test]
fn the_page_answers_each_time_key_in_turn() {
    let dir = tempfile::tempdir().unwrap();
    let line = [
        "occur",
        "tea",
        "occur_time=2026-10-25T02:30",
        "brew_time=2026-10-25T02:10",
    ];
    let occur = "occur_time=2026-10-25T02:30:00+01:00";
    assert_eq!(
        options(&page(&dir, &line, "")),
        format!("DATUM_CHOOSE=occur_time=2026-10-25T02:30:00+02:00|{occur}")
    );
    assert_eq!(
        options(&page(&dir, &line, &format!("{occur}\n"))),
        "DATUM_CHOOSE=brew_time=2026-10-25T02:10:00+02:00|brew_time=2026-10-25T02:10:00+01:00"
    );
    assert_eq!(teas(&dir), vec![], "unanswered: nothing written");

    let answered = page(&dir, &line, &format!("{occur}\n1\n"));
    assert!(answered.status.success(), "{}", said(&answered));
    assert_eq!(teas(&dir), second_stored());
    assert_eq!(
        rows(
            &dir,
            "SELECT json_extract(data, '$.brew_time.utc'), NULL FROM entries WHERE field = 'tea'"
        ),
        vec![("2026-10-25T00:10:00.000Z".to_string(), None)],
        "the first brew_time"
    );
}

/// `update`'s `occur_time=` is asked about as a log's is.
#[test]
fn an_update_s_time_key_is_asked_about() {
    let dir = tempfile::tempdir().unwrap();
    let logged = run(&dir, &["occur", "tea", "-t", "2026-10-25T12:00"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(logged.status.success(), "{}", said(&logged));
    let update = [
        "update",
        "tea:2026-10-25T11:00:00.000Z",
        "occur_time=2026-10-25T02:30",
    ];
    let refused = run(&dir, &update).stdin(Stdio::null()).output().unwrap();
    assert_eq!(refused.status.code(), Some(1), "{}", said(&refused));
    assert!(
        said(&refused).contains("happens twice"),
        "{}",
        said(&refused)
    );
    assert_eq!(teas(&dir)[0].0, "tea:2026-10-25T11:00:00.000Z", "unmoved");

    let answered = page(&dir, &update, "2\n");
    assert!(answered.status.success(), "{}", said(&answered));
    assert_eq!(teas(&dir), second_stored());
}

/// da-1ppce: the line as typed does not say which reading was picked, so a
/// replay takes the one the stored row holds: `reparse` finds an answered
/// key clean, and `--fix` leaves it as stored, whichever was picked — a
/// key's and a pair's `start_time=`.
#[test]
fn reparse_keeps_the_reading_picked() {
    for answer in ["1", "2"] {
        let dir = tempfile::tempdir().unwrap();
        let logs: [&[&str]; 2] = [
            &[
                "occur",
                "tea",
                "-t",
                "2026-10-25T12:00",
                "wake_time=2026-10-25T02:30",
            ],
            &[
                "start",
                "nap",
                "start_time=2026-10-25T02:30",
                "end_time=2026-10-25T03:30",
            ],
        ];
        for line in logs {
            let logged = page(&dir, line, &format!("{answer}\n"));
            assert!(logged.status.success(), "{}", said(&logged));
        }
        let stored = |dir: &TempDir| {
            rows(
                dir,
                "SELECT field || ' ' || coalesce(data, '') || ' ' || coalesce(dur, ''), NULL \
                 FROM entries WHERE field IN ('tea', 'nap') ORDER BY field",
            )
        };
        let before = stored(&dir);
        let ids = rows(
            &dir,
            "SELECT hid, NULL FROM entries WHERE field IN ('tea', 'nap')",
        );
        let ids: Vec<&str> = ids.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        for fix in [false, true] {
            let mut cmd = run(&dir, &["reparse"]);
            cmd.args(&ids);
            if fix {
                cmd.arg("--fix");
            }
            let out = cmd.stdin(Stdio::null()).output().unwrap();
            let said_out = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success(), "{}", said(&out));
            assert_eq!(
                said_out.matches(": clean").count(),
                2,
                "answered {answer}: {said_out}"
            );
        }
        assert_eq!(stored(&dir), before, "answered {answer}: unchanged");
    }
}

/// A key edited since to neither reading replays as the first, so the
/// drift shows.
#[test]
fn reparse_shows_a_key_edited_since_as_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let line = [
        "occur",
        "tea",
        "-t",
        "2026-10-25T12:00",
        "wake_time=2026-10-25T02:30",
    ];
    let logged = page(&dir, &line, "2\n");
    assert!(logged.status.success(), "{}", said(&logged));
    let hid = rows(&dir, "SELECT hid, NULL FROM entries WHERE field = 'tea'")[0]
        .0
        .clone();
    let edited = run(&dir, &["update", &hid, "wake_time=2026-10-25T05:00"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(edited.status.success(), "{}", said(&edited));

    let out = run(&dir, &["reparse", &hid])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let said_out = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", said(&out));
    assert!(
        said_out.contains("\"utc\":\"2026-10-25T04:00:00.000Z\"")
            && said_out.contains("→ {\"wake_time\":{\"o\":2,"),
        "the first reading: {said_out}"
    );
}
