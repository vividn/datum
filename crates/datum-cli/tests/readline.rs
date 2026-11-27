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

/// `datum <args>` on a terminal 80 wide: each step's keys go in once the
/// editor reads raw and the screen so far holds the step's text; returns
/// what was drawn, and how it ended.
#[cfg(unix)]
fn on_terminal(env: &Env, args: &[&str], steps: &[(&str, &str)]) -> (String, Output) {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    let (master, slave) = pty();
    let size = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCSWINSZ reads one winsize from a live pointer.
    unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &size) };
    let child = env
        .datum(args)
        .stdin(slave.try_clone().unwrap())
        .stdout(slave)
        .spawn()
        .unwrap();
    let screen = Arc::new(Mutex::new(Vec::new()));
    let mut reader = master.try_clone().unwrap();
    let drawn = screen.clone();
    std::thread::spawn(move || {
        let mut buf = [0; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            drawn.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    let text = || String::from_utf8_lossy(&screen.lock().unwrap()).into_owned();
    let mut master = master;
    let mut seen = 0;
    for (wait_for, keys) in steps {
        let since = Instant::now();
        loop {
            // SAFETY: a zeroed termios is filled by tcgetattr on an open fd.
            let mut t: libc::termios = unsafe { std::mem::zeroed() };
            unsafe { libc::tcgetattr(master.as_raw_fd(), &mut t) };
            let raw = t.c_lflag & libc::ICANON == 0;
            let now = text();
            if raw && now[seen..].contains(wait_for) {
                seen = now.len();
                break;
            }
            assert!(
                since.elapsed() < Duration::from_secs(30),
                "never drew «{wait_for}»: {now:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        master.write_all(keys.as_bytes()).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    (text(), out)
}

/// On a terminal (da-1lnm4): a clock prompt; Enter gives the line's row
/// to the entry as datum prints it; Tab draws it again compact, `time |
/// text`, running nothing; Ctrl-D ends it.
#[cfg(unix)]
#[test]
fn on_a_terminal_the_line_becomes_its_entry_and_tab_turns_the_view() {
    let env = Env::new();
    let (screen, out) = on_terminal(
        &env,
        &["readline", "occur", "nightly", "-SR", "line"],
        &[
            (" | ", "hello there\r"),
            ("line: \"hello there\"", "\t"),
            (" | hello there\r\n", "\x04"),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        env.data("nightly"),
        [r#"{"line":"hello there"}"#],
        "Tab ran nothing"
    );
    // The prompt: the clock.
    let prompt = screen.find("\r\x1b[K").expect(&screen) + 4;
    assert!(is_clock(&screen[prompt..prompt + 11]), "{screen:?}");
    // Enter: up the line's one row, cleared, and the full echo there.
    assert!(screen.contains("\x1b[1A\r\x1b[JMOMENT "), "{screen:?}");
    // Tab: up the echo's three rows and the prompt's, and the compact line.
    let tab = screen.find("\x1b[4A\r\x1b[J").expect(&screen) + 8;
    assert!(is_clock(&screen[tab..tab + 11]), "{screen:?}");
    assert!(
        screen[tab + 11..].starts_with("hello there\r\n"),
        "{screen:?}"
    );
}

/// `HH:MM:SS | `.
fn is_clock(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 11
        && s.ends_with(" | ")
        && (0..8).all(|i| match i {
            2 | 5 => b[i] == b':',
            _ => b[i].is_ascii_digit(),
        })
}

/// `--compact` starts compact; Tab draws the sitting's line full and keeps a
/// half-typed one, cursor and all.
#[cfg(unix)]
#[test]
fn compact_starts_compact_and_tab_keeps_a_half_typed_line() {
    let env = Env::new();
    let (screen, out) = on_terminal(
        &env,
        &["readline", "--compact", "occur", "nightly", "-SR", "line"],
        &[
            (" | ", "first\r"),
            (" | first\r\n", "hal"),
            ("hal", "\t"),
            ("line: \"first\"", "f\r"),
            ("line: \"half\"", "\x04"),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Enter: the compact line in the typed one's row.
    let first = screen.find("\x1b[1A\r\x1b[J").expect(&screen) + 8;
    assert!(is_clock(&screen[first..first + 11]), "{screen:?}");
    assert!(screen[first + 11..].starts_with("first\r\n"), "{screen:?}");
    // Tab: up the compact row and the prompt's, and the full echo.
    assert!(screen.contains("\x1b[2A\r\x1b[JMOMENT "), "{screen:?}");
    assert_eq!(
        env.data("nightly"),
        [r#"{"line":"first"}"#, r#"{"line":"half"}"#],
        "{screen:?}"
    );
}

/// A line that fails stays on screen as typed, its error under it.
#[cfg(unix)]
#[test]
fn on_a_terminal_a_failed_line_stays_as_typed() {
    let env = Env::new();
    let (screen, out) = on_terminal(
        &env,
        &["readline", "switch", "timezone", "-I"],
        &[(" | ", "nowhere\r"), (" | ", "\x04")],
    );
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("FAILED "));
    let typed = screen.find("nowhere").unwrap();
    assert!(
        !screen[typed..].contains("\x1b[J"),
        "nothing erased: {screen:?}"
    );
}

/// The full view is what the command prints unquiet, row for row: here a
/// format string, which stands in for the data block (da-1lnm4).
#[cfg(unix)]
#[test]
fn the_full_view_is_what_the_command_prints() {
    let env = Env::new();
    let base = ["occur", "nightly", "-SR", "line", "-o", "said %line%"];
    let (screen, out) = on_terminal(
        &env,
        &[&["readline"], &base[..]].concat(),
        &[(" | ", "hello\r"), ("said hello", "\x04")],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let from = screen.find("\x1b[1A\r\x1b[J").expect(&screen) + 8;
    let to = from + screen[from..].find("\x1b[?2004h").expect(&screen);
    let drawn: Vec<&str> = screen[from..to]
        .split("\r\n")
        .filter(|l| !l.is_empty())
        .collect();
    // On a database of its own, so it too is the field's first entry.
    let printed = Env::new()
        .datum(&[&base[..], &["--", "hello"]].concat())
        .output()
        .unwrap();
    let printed = String::from_utf8_lossy(&printed.stdout).into_owned();
    // Its hid and its times are its own.
    let mask = |l: &&str| {
        let l = match l.strip_prefix("MOMENT ") {
            Some(rest) => format!("MOMENT xxxxx{}", &rest[5..]),
            None => l.to_string(),
        };
        l.replace(|c: char| c.is_ascii_digit(), "0")
    };
    let printed: Vec<&str> = printed.lines().collect();
    assert_eq!(
        drawn.iter().map(mask).collect::<Vec<_>>(),
        printed.iter().map(mask).collect::<Vec<_>>(),
        "{screen:?}"
    );
    assert_eq!(drawn.len(), 3, "header, info, the format string: {drawn:?}");
}
