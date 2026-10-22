//! A REPL's lines run in one process (da-3f9nl), so what a command keeps in
//! a `thread_local!` outlives its line unless something resets it. Each test
//! here runs a line that sets one, then a line that must not see it, and
//! holds the second to what it prints in a REPL of its own.
//!
//! The 16, and where each is held:
//!
//! | thread-local | |
//! |---|---|
//! | `COLOR_OVERRIDE` | `a_lines_color_is_that_lines` |
//! | `ECHO` | `a_json_echo_is_that_lines` |
//! | `MULTI_COLLECTOR`, `PENDING_DECLARATIONS` | `a_failed_window_leaves_nothing_for_the_next_line` |
//! | `DUPLICATE_NOOP` | `a_dedup_is_that_lines_and_the_repl_exits_0` |
//! | `ACTION`, `BATCH_MARK` | `undo_after_a_failed_write_takes_back_the_write_before_it` |
//! | `INTENT_CTX`, `INVOCATION` | `a_write_carries_its_own_line` |
//! | `CURRENT_CLOCK` | `a_bare_log_after_a_t_log_is_now` |
//! | `SESSION` | `a_sync_as_line_binds_no_later_line` |
//! | `ENDPOINT` | `a_rekey_signs_the_next_line_with_the_new_key` |
//! | `QUEUED` | `spool.rs`: `a_repl_write_after_a_queued_one_is_written` |
//! | `READER` | not here: a cache keyed by the endpoint and the chain records it is derived from, so any line that changes either misses it |
//! | `SKEW`, `WALKED` | not in the binary: `#[cfg(test)]` |

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use rusqlite::Connection;
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let db = tmp.path().join("repl.db");
        Self {
            _tmp: tmp,
            home,
            db,
        }
    }

    /// `lines` typed into one REPL.
    fn repl(&self, lines: &[&str]) -> Output {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn query(&self, sql: &str) -> String {
        Connection::open(&self.db)
            .unwrap()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }

    /// What a REPL printed on stdout, past its banner, and on stderr, each
    /// hid named `<hid>`: the two sides of a comparison mint their own.
    fn printed(&self, out: &Output) -> (String, String) {
        let c = Connection::open(&self.db).unwrap();
        let mut s = c.prepare("SELECT hid FROM entries").unwrap();
        let hids: Vec<String> = s
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let mask = |text: String| {
            hids.iter().fold(text, |t, hid| {
                t.replace(hid, "<hid>").replace(&hid[..5], "<hid>")
            })
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let (_banner, stdout) = stdout
            .split_once("`exit` or Ctrl-D to quit.\n")
            .expect("the REPL's banner");
        let stdout = stdout.to_string();
        (
            mask(stdout),
            mask(String::from_utf8_lossy(&out.stderr).into()),
        )
    }
}

/// `second` typed after `first` in one REPL prints what it prints in a REPL
/// of its own, after `first` ran in another, each on a database `seed` made.
/// Returns all the shared REPL printed.
fn second_line_as_alone(seed: &[&str], first: &str, second: &str) -> String {
    let (together, apart) = (Env::new(), Env::new());
    for env in [&together, &apart] {
        env.repl(seed);
    }
    let out = together.repl(&[first, second]);
    let both = together.printed(&out);
    let out_1 = apart.repl(&[first]);
    let one = apart.printed(&out_1);
    let out_2 = apart.repl(&[second]);
    let two = apart.printed(&out_2);
    assert_eq!(
        both,
        (one.0 + &two.0, one.1 + &two.1),
        "`{second}` after `{first}` in one REPL printed otherwise than alone"
    );
    assert!(out.status.success(), "the REPL exited {:?}", out.status);
    both.0 + &both.1
}

#[test]
fn a_lines_color_is_that_lines() {
    let printed = second_line_as_alone(&["occur coffee -t 10:00"], "--color always tail", "tail");
    assert!(
        printed.contains('\x1b'),
        "the first line was not painted: {printed}"
    );
}

#[test]
fn a_json_echo_is_that_lines() {
    let printed = second_line_as_alone(
        &["occur tea -t 9:00"],
        "occur cake -t 9:30 --show json",
        "occur coffee -t 10:00",
    );
    assert!(printed.contains(r#"{"docs":"#), "no echo: {printed}");
}

/// The window fails at its atomic insert, after side 1 buffered a row and
/// declared its field: the next line's first log of that field declares it.
#[test]
fn a_failed_window_leaves_nothing_for_the_next_line() {
    let printed = second_line_as_alone(
        &["occur bb -t 11:00"],
        "occur newfield -t 11:00 , occur bb -t 11:00 cups=2",
        "occur newfield -t 15:00",
    );
    assert!(
        printed.contains("error:"),
        "the window did not fail: {printed}"
    );
}

/// `main` exits 11 when the run deduped; a REPL is not a run that deduped.
#[test]
fn a_dedup_is_that_lines_and_the_repl_exits_0() {
    let printed = second_line_as_alone(
        &["occur coffee -t 10:00"],
        "occur coffee -t 10:00",
        "occur tea -t 9:00",
    );
    assert!(
        printed.contains("EXISTS"),
        "the first line did not dedup: {printed}"
    );
}

#[test]
fn undo_after_a_failed_write_takes_back_the_write_before_it() {
    let printed = second_line_as_alone(&["occur tea -t 9:00"], "delete nosuchhid", "undo");
    assert!(
        printed.contains("error:"),
        "the delete did not fail: {printed}"
    );
}

/// The intent and the op's `raw` are the line's own, after a line that set
/// them and wrote nothing to take them.
#[test]
fn a_write_carries_its_own_line() {
    let env = Env::new();
    env.repl(&["occur tea -t 9:00", "tail", "occur coffee -t 10:00"]);
    let raw = |what: &str| {
        env.query(&format!(
            "SELECT {what} FROM entries e JOIN ops o USING (hid)
             WHERE e.field = 'coffee' AND o.kind = 'create'"
        ))
    };
    assert_eq!(raw("e.raw_input"), "occur coffee -t 10:00");
    assert_eq!(
        raw("json_extract(o.body, '$.raw')"),
        "occur coffee -t 10:00"
    );
}

#[test]
fn a_bare_log_after_a_t_log_is_now() {
    let env = Env::new();
    env.repl(&["occur tea -t 9:00", "occur coffee"]);
    let off = env.query(
        "SELECT CAST(ABS(unixepoch('now') - unixepoch(occur_time)) AS TEXT)
         FROM entries WHERE field = 'coffee'",
    );
    assert!(
        off.parse::<i64>().unwrap() < 60,
        "coffee is {off} s from now"
    );
}

/// A hidden `sync --as` line binds the thread to that endpoint; the next
/// line is no session's.
#[test]
fn a_sync_as_line_binds_no_later_line() {
    let printed = second_line_as_alone(
        &["occur cake -t 8:00"],
        "sync --as deadbeef status",
        "occur tea -t 9:00",
    );
    assert!(printed.contains("CREATE"), "{printed}");
}

#[test]
fn a_rekey_signs_the_next_line_with_the_new_key() {
    let env = Env::new();
    env.repl(&[
        "occur cake -t 8:00",
        "sync chain rekey",
        "occur pie -t 8:00",
    ]);
    let signer = |field: &str| {
        env.query(&format!(
            "SELECT o.endpoint FROM entries e JOIN ops o USING (hid) WHERE e.field = '{field}'"
        ))
    };
    assert_ne!(signer("cake"), signer("pie"));
    assert_eq!(
        signer("pie"),
        env.query("SELECT endpoint_id FROM sync_local")
    );
}
