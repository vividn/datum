//! `datum readline <command…>` (da-xloff): each line typed runs the base
//! command with the line as one word after `--`, exactly as typed.

use std::io::Write;
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
        let db = tmp.path().join("t.db");
        Self {
            _tmp: tmp,
            home,
            db,
        }
    }

    fn datum(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
        cmd.args(args)
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("NO_COLOR", "1")
            .env("TZ", "UTC")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    /// `lines` typed into `datum readline <base…>`, then Ctrl-D.
    fn readline(&self, base: &[&str], lines: &[&str]) -> Output {
        let mut child = self
            .datum(&[&["readline"], base].concat())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input.as_bytes()).unwrap();
        drop(stdin);
        child.wait_with_output().unwrap()
    }

    /// Each entry of `field`'s data, in the order written.
    fn data(&self, field: &str) -> Vec<String> {
        let conn = Connection::open(&self.db).unwrap();
        let mut stmt = conn
            .prepare("SELECT data FROM entries WHERE field = ?1 ORDER BY rowid")
            .unwrap();
        let rows = stmt.query_map([field], |r| r.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    }
}

#[test]
fn every_line_is_kept_as_typed_an_empty_one_too_and_ctrl_d_exits_0() {
    let env = Env::new();
    let out = env.readline(
        &["occur", "nightly", "-SR", "line"],
        &["the light went, x=3 of it", "-- a dash first", "x=3", ""],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        env.data("nightly"),
        [
            r#"{"line":"the light went, x=3 of it"}"#,
            r#"{"line":"-- a dash first"}"#,
            r#"{"line":"x=3"}"#,
            r#"{"line":""}"#,
        ]
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "",
        "no receipt on stdout"
    );
    let raw: String = Connection::open(&env.db)
        .unwrap()
        .query_row(
            "SELECT raw_input FROM entries WHERE field = 'nightly' ORDER BY rowid LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!raw.contains("--show"), "the line as typed: {raw}");
}

/// Each Enter says what it wrote, ACTION time field, on a line of its own
/// from a pipe (da-jt3k8): the time is the one the line was held to.
#[test]
fn each_line_says_what_it_wrote_an_empty_one_too() {
    let env = Env::new();
    let said = |at: &str, line: &str| {
        let out = env.readline(&["occur", "nightly", "-SR", "line", "-t", at], &[line]);
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout), "");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    assert_eq!(
        said("2026-01-05T23:41:07Z", "the lamp was on"),
        "MOMENT 2026-01-05 23:41:07+0 nightly\n"
    );
    assert_eq!(
        said("2026-01-05T23:42:00Z", ""),
        "MOMENT 2026-01-05 23:42:00+0 nightly\n"
    );
    assert_eq!(env.data("nightly").len(), 2);
}

/// The base's own `--show` wins over the quiet, the outer parse leaving it.
#[test]
fn a_base_naming_its_show_echoes_it() {
    let env = Env::new();
    let out = env.readline(&["occur", "nightly", "-SR", "line", "--json"], &["a"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let echo: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(echo["docs"][0]["action"], "MOMENT", "{echo}");
}

#[test]
fn a_refused_line_prints_and_the_loop_goes_on() {
    let env = Env::new();
    let out = env.readline(
        &["switch", "timezone", "-I"],
        &["Europe/Berlin", "nowhere", "Europe/Paris"],
    );
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    let said: Vec<&str> = err.lines().collect();
    assert_eq!(said.len(), 3, "a line each: {err}");
    assert!(said[0].starts_with("CHANGE "), "{err}");
    assert!(
        said[1].starts_with("FAILED ") && said[1].contains("nowhere"),
        "the refusal is said on its line: {err}"
    );
    let conn = Connection::open(&env.db).unwrap();
    let states: Vec<String> = conn
        .prepare(
            "SELECT json_extract(state, '$') FROM entries WHERE field = 'timezone' ORDER BY rowid",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(states, ["Europe/Berlin", "Europe/Paris"]);
}

#[test]
fn an_explicit_empty_word_after_dashdash_is_a_value() {
    let env = Env::new();
    let out = env
        .datum(&["occur", "nightly", "-SR", "line", "--", ""])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(env.data("nightly"), [r#"{"line":""}"#]);
    // Two empty words join as any two do.
    let out = env
        .datum(&["occur", "nightly", "-SR", "line", "--", "", ""])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(env.data("nightly")[1], r#"{"line":" "}"#);
}

/// Ctrl-C on a terminal ends it, exit 0, the half-typed line unsaved.
#[cfg(unix)]
#[test]
fn ctrl_c_ends_it_and_the_half_typed_line_is_not_saved() {
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    let env = Env::new();
    let (mut master, slave) = pty();
    let child = env
        .datum(&["readline", "occur", "nightly", "-SR", "line"])
        .stdin(slave.try_clone().unwrap())
        .stdout(slave)
        .spawn()
        .unwrap();
    master.write_all(b"kept\r").unwrap();
    // The next keys go in once the line has landed and the editor reads raw
    // again: in cooked mode the terminal would take Ctrl-C itself.
    let since = Instant::now();
    loop {
        // SAFETY: a zeroed termios is filled by tcgetattr on an open fd.
        let mut t: libc::termios = unsafe { std::mem::zeroed() };
        unsafe { libc::tcgetattr(master.as_raw_fd(), &mut t) };
        let raw = t.c_lflag & libc::ICANON == 0;
        let landed = Connection::open(&env.db)
            .and_then(|c| {
                c.query_row(
                    "SELECT count(*) FROM entries WHERE field = 'nightly'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
            })
            .unwrap_or(0);
        if raw && landed == 1 {
            break;
        }
        assert!(
            since.elapsed() < Duration::from_secs(30),
            "the line never landed"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    master.write_all(b"half\x03").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(env.data("nightly"), [r#"{"line":"kept"}"#]);
}

#[cfg(unix)]
fn pty() -> (std::fs::File, std::fs::File) {
    use std::os::fd::FromRawFd;

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
    unsafe {
        (
            std::fs::File::from_raw_fd(master),
            std::fs::File::from_raw_fd(slave),
        )
    }
}
