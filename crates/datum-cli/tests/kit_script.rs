//! A kit command's `script` (da-et0lm): datum command lines run in order in
//! the kit's database, each `{arg}` filled from the command's args.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use rusqlite::Connection;
use tempfile::TempDir;

const JOURNAL: &str = r#"[kit]
name = "journal"

[[command]]
name = "write"
args = ["notebook"]
script = """
change {notebook} write
readline occur {notebook} -SR line
end {notebook}
"""
"#;

struct Env {
    tmp: TempDir,
    home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let db = tmp.path().join("j.db");
        Self { tmp, home, db }
    }

    /// `datum <args>` with `stdin`, then EOF.
    fn datum(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(args)
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    /// Install `toml` as a kit.
    fn install(&self, toml: &str) -> Output {
        let file = self.tmp.path().join("kit.toml");
        std::fs::write(&file, toml).unwrap();
        self.datum(&["kit", "install", &file.to_string_lossy()], "")
    }

    /// Each entry of `field`: its state and data, in the order written.
    fn rows(&self, field: &str) -> Vec<(Option<String>, Option<String>)> {
        let conn = Connection::open(&self.db).unwrap();
        let mut stmt = conn
            .prepare("SELECT state, data FROM entries WHERE field = ?1 ORDER BY rowid")
            .unwrap();
        let rows = stmt
            .query_map([field], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        rows.map(Result::unwrap).collect()
    }

    fn count(&self) -> i64 {
        Connection::open(&self.db)
            .unwrap()
            .query_row("SELECT count(*) FROM entries", [], |r| r.get(0))
            .unwrap()
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn the_journal_write_runs_its_lines_in_order() {
    let env = Env::new();
    assert!(env.install(JOURNAL).status.success());
    let out = env.datum(&["write", "nightly"], "the light went, x=3 of it\nx=3\n\n");
    assert!(out.status.success(), "{}", stderr(&out));
    let s = |v: &str| Some(v.to_string());
    assert_eq!(
        env.rows("nightly"),
        [
            (s("\"write\""), None),
            (None, s(r#"{"line":"the light went, x=3 of it"}"#)),
            (None, s(r#"{"line":"x=3"}"#)),
            (None, s(r#"{"line":""}"#)),
            (s("false"), None),
        ]
    );
}

#[test]
fn a_missing_arg_refuses_with_nothing_written() {
    let env = Env::new();
    assert!(env.install(JOURNAL).status.success());
    let before = env.count();
    let out = env.datum(&["write"], "a line\n");
    assert!(!out.status.success());
    assert!(stderr(&out).contains("notebook"), "{}", stderr(&out));
    assert_eq!(env.count(), before, "nothing written");
}

#[test]
fn a_refused_line_stops_the_script_and_is_named() {
    let env = Env::new();
    let kit = r#"[kit]
name = "k"

[[command]]
name = "thrice"
args = ["f"]
script = """
moment {f}
# a comment is no command
moment {f} -t notatime
moment after
"""
"#;
    assert!(env.install(kit).status.success());
    let out = env.datum(&["thrice", "tea"], "");
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("line 3, «moment {f} -t notatime»"),
        "names the line: {err}"
    );
    assert_eq!(env.rows("tea").len(), 1, "the line before it ran");
    assert!(env.rows("after").is_empty(), "the line after it did not");
}

#[test]
fn install_refuses_a_placeholder_that_is_no_arg() {
    let env = Env::new();
    let out = env.install(&JOURNAL.replace("end {notebook}", "end {book}"));
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("«{book}»") && err.contains("(notebook)"),
        "names both: {err}"
    );
}

/// Only a kit script reads a `readline` line's lines: a `datum script` line
/// still refuses one, as a line of a script.
#[test]
fn a_script_file_still_refuses_readline() {
    let env = Env::new();
    let file = env.tmp.path().join("s.txt");
    std::fs::write(&file, "readline occur tea -SR line\n").unwrap();
    let out = env.datum(&["script", &file.to_string_lossy()], "a line\n");
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("not a line of a script"),
        "{}",
        stderr(&out)
    );
    assert!(env.rows("tea").is_empty());
}

/// `datum help kit` names a level-3 command and that only the CLI runs it
/// (da-u29fe): nothing else in datum's help shows one.
#[test]
fn help_kit_names_a_script_command_and_its_limit() {
    let out = Env::new().datum(&["help", "kit"], "");
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("level-3") && help.contains("script = "),
        "{help}"
    );
    assert!(help.contains("Only the datum CLI runs one"), "{help}");
}
