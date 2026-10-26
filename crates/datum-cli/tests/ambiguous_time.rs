//! da-6i7l1: a wall time a fall-back night repeats — 02:30 on 2026-10-25 in
//! Europe/Berlin is 00:30Z and 01:30Z — is asked about, never read as the
//! first in silence. A terminal is asked which, datum.day is told the
//! question (exit 12) and answers on a second run's stdin, and anywhere else
//! the line is refused naming both full spellings.

use std::process::{Command, Output, Stdio};

use rusqlite::Connection;
use tempfile::TempDir;

const LINE: [&str; 5] = ["occur", "tea", "-d", "2026-10-25", "-t"];
const FIRST: &str = "-t 2026-10-25T02:30:00+02:00";
const SECOND: &str = "-t 2026-10-25T02:30:00+01:00";

fn datum(dir: &TempDir) -> Command {
    let mut cmd = bare(dir);
    cmd.args(LINE).arg("02:30");
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
    let Ok(conn) = Connection::open(dir.path().join("t.db")) else {
        return vec![];
    };
    let Ok(mut rows) = conn.prepare("SELECT slug, occur_time_tz FROM entries WHERE field = 'tea'")
    else {
        return vec![];
    };
    rows.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
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
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let page = |answer: &str| {
        let mut child = datum(&dir)
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
    };

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
    let logged = bare(&dir).args(LINE).arg("01:00").output().unwrap();
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
