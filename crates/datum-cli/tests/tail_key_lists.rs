//! `tail`'s keys (da-6l1hf): a list reads as the state column writes one, a
//! remainder is a key it shows, and `tail -w` follows a change of spec, a
//! delete and an edit, made here or synced in (da-dysdz).

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tempfile::TempDir;

fn datum(dir: &TempDir) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
    c.env("DATUM_DB", dir.path().join("data.db"))
        .env("DATUM_HOME", dir.path().join("home"))
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("NO_COLOR", "1");
    c
}

fn ok(dir: &TempDir, args: &[&str]) -> String {
    let out = datum(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "`datum {}`: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// The table's body rows of `field`, each cut to what follows the dot.
fn after_the_dot(table: &str, field: &str) -> Vec<String> {
    table
        .lines()
        .filter(|l| l.split_whitespace().any(|w| w == field))
        .map(|l| l.split_once('●').unwrap().1.trim().to_string())
        .collect()
}

/// vividn: «it shouldn't show the json syntax, instead it should just be
/// pleased,warm like that».
#[test]
fn a_keys_list_reads_as_a_list() {
    let dir = tempfile::tempdir().unwrap();
    ok(&dir, &["setup", "feel", "--verb", "occur", "-k", "words="]);
    ok(&dir, &["feel", "words=pleased,warm"]);
    ok(&dir, &["feel", "words=calm"]);
    let table = ok(&dir, &["tail", "feel"]);
    assert_eq!(
        after_the_dot(&table, "feel"),
        ["pleased,warm", "calm"],
        "{table}"
    );
    // `tail --column` is one way with it.
    let table = ok(&dir, &["tail", "feel", "--column", "words"]);
    assert!(
        table.contains("pleased,warm") && !table.contains('['),
        "{table}"
    );
    // The row `--json` hands datum.day says it too; `data` stays the list.
    let json: serde_json::Value =
        serde_json::from_str(&ok(&dir, &["tail", "feel", "--json"])).unwrap();
    assert_eq!(json["rows"][0]["keys"], "pleased,warm");
    assert_eq!(
        json["rows"][0]["data"]["words"],
        serde_json::json!(["pleased", "warm"])
    );
}

/// vividn: «`-R words -k words=` should basically be equivalent to `-R
/// words`». Both show their words, in the one place.
#[test]
fn a_remainder_is_a_key_tail_shows() {
    let dir = tempfile::tempdir().unwrap();
    ok(&dir, &["setup", "alone", "--verb", "occur", "-R", "words"]);
    ok(
        &dir,
        &[
            "setup", "both", "--verb", "occur", "-R", "words", "-k", "words=",
        ],
    );
    ok(
        &dir,
        &[
            "setup", "more", "--verb", "occur", "-k", "kind=", "-R", "words",
        ],
    );
    for field in ["alone", "both"] {
        ok(&dir, &[field, "calm"]);
        ok(&dir, &[field, "calm", "curious"]);
        let table = ok(&dir, &["tail", field]);
        assert_eq!(
            after_the_dot(&table, field),
            ["calm", "calm,curious"],
            "{field}: {table}"
        );
    }
    // After the slots, as it binds.
    ok(&dir, &["more", "big", "calm", "curious"]);
    let table = ok(&dir, &["tail", "more"]);
    assert_eq!(
        after_the_dot(&table, "more"),
        ["big calm,curious"],
        "{table}"
    );
}

/// A running watch, killed however the test ends.
struct Watch(Child);

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(seen: &Mutex<Vec<u8>>, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let text = String::from_utf8_lossy(&seen.lock().unwrap()).into_owned();
        if text.contains(what) {
            return;
        }
        assert!(Instant::now() < deadline, "no {what:?} in: {text:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn collect(mut from: impl Read + Send + 'static) -> Arc<Mutex<Vec<u8>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    std::thread::spawn(move || {
        let mut buf = [0; 4096];
        while let Ok(n) = from.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    seen
}

/// vividn: «I had to restart the tail to see it. It should be responsive to
/// changes in spec.» The loop `--json` and a pipe watch by.
#[test]
fn a_piped_watch_redraws_on_a_change_of_spec() {
    let dir = tempfile::tempdir().unwrap();
    ok(&dir, &["occur", "feel", "words=calm"]);
    let mut child = Watch(
        datum(&dir)
            .args(["tail", "-w", "0.1", "feel"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let seen = collect(child.0.stdout.take().unwrap());
    wait_for(&seen, "feel");
    assert!(!String::from_utf8_lossy(&seen.lock().unwrap()).contains("calm"));
    ok(&dir, &["setup", "feel", "-k", "words="]);
    wait_for(&seen, "calm");
}

/// The same on a terminal, in the TUI's own loop: its frame is drawn only
/// when a signature changes, so this needs the real run and not `render_frame`.
#[cfg(unix)]
#[test]
fn the_tui_redraws_on_a_change_of_spec() {
    let dir = tempfile::tempdir().unwrap();
    ok(&dir, &["occur", "feel", "words=calm"]);
    let (_watch, mut master, seen) = tui(&dir, &["tail", "-w", "0.1", "feel"], 24);
    wait_for(&seen, "q quit");
    assert!(!String::from_utf8_lossy(&seen.lock().unwrap()).contains("calm"));
    ok(&dir, &["setup", "feel", "-k", "words="]);
    wait_for(&seen, "calm");
    master.write_all(b"q").unwrap();
}

/// `datum <args>` on a terminal `rows` tall: the process, the terminal's
/// side to type into, and everything drawn on it.
#[cfg(unix)]
fn tui(dir: &TempDir, args: &[&str], rows: u16) -> (Watch, std::fs::File, Arc<Mutex<Vec<u8>>>) {
    use std::os::fd::FromRawFd;

    let (mut master, mut slave) = (0, 0);
    let size = libc::winsize {
        ws_row: rows,
        ws_col: 100,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: openpty writes two fresh descriptors, each owned once below.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            &size,
        )
    };
    assert_eq!(opened, 0, "openpty");
    let (master, slave) = unsafe {
        (
            std::fs::File::from_raw_fd(master),
            std::fs::File::from_raw_fd(slave),
        )
    };
    let watch = Watch(
        datum(dir)
            .args(args)
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave)
            .spawn()
            .unwrap(),
    );
    let seen = collect(master.try_clone().unwrap());
    (watch, master, seen)
}

/// vividn, in tmux with `mouse on` over mosh to termux: «while it is running
/// I can no longer tap around between the other panes and scrolling stops
/// working even in other panes» (da-6kmst). Any-motion tracking (1003) in a
/// pane makes tmux ask the outer terminal for 1003 in place of 1002, and
/// termux, which has no 1003, then reports no mouse at all. The TUI asks
/// for presses and the wheel alone, and still scrolls by key and by wheel.
#[cfg(unix)]
#[test]
fn the_tui_asks_for_the_wheel_and_no_motion() {
    let dir = tempfile::tempdir().unwrap();
    for i in 1..=30 {
        ok(&dir, &["occur", "feel", "-t", &format!("-{i}")]);
    }
    let (watch, mut master, seen) = tui(&dir, &["tail", "-w", "0.1", "feel"], 12);
    wait_for(&seen, "q quit");
    let drawn = String::from_utf8_lossy(&seen.lock().unwrap()).into_owned();
    for asked in ["\x1b[?1000h", "\x1b[?1006h"] {
        assert!(drawn.contains(asked), "no {asked:?} in: {drawn:?}");
    }
    for never in ["?1002h", "?1003h"] {
        assert!(!drawn.contains(never), "{never:?} in: {drawn:?}");
    }

    let mut press = |keys: &[u8], then: &str| {
        seen.lock().unwrap().clear();
        master.write_all(keys).unwrap();
        wait_for(&seen, then);
    };
    press(b"\x1b[A", "browse");
    press(b"G", "scroll · q quit");
    // The wheel up, as SGR reports it at column 5, row 5.
    press(b"\x1b[<64;5;5M", "browse");
    press(b"q", "\x1b[?1000l");
    drop(watch);
}

/// `mood` switched to alpha, bravo, then charlie, a minute apart.
fn three_moods(dir: &TempDir) {
    for (state, t) in [("alpha", "-3"), ("bravo", "-2"), ("charlie", "-1")] {
        ok(dir, &["switch", "mood", state, "-t", t]);
    }
}

/// The hid of the `mood` entry in `state`, in the database `db`.
fn hid_of(dir: &TempDir, db: &str, state: &str) -> String {
    let json: serde_json::Value =
        serde_json::from_str(&ok(dir, &["--db", db, "tail", "mood", "--json"])).unwrap();
    let rows = json["rows"].as_array().unwrap();
    let row = rows.iter().find(|r| r["state"] == state);
    row.unwrap_or_else(|| panic!("no {state} in {json}"))["hid"]
        .as_str()
        .unwrap()
        .to_string()
}

/// What a piped watch drew last: everything after its last clear.
fn last_frame(seen: &Mutex<Vec<u8>>) -> String {
    let text = String::from_utf8_lossy(&seen.lock().unwrap()).into_owned();
    text.rsplit("\x1b[2J").next().unwrap().to_string()
}

/// Waits for a piped watch's last frame to show every one of `shows`, and
/// then says it shows none of `gone`.
fn frame_shows(seen: &Mutex<Vec<u8>>, shows: &[&str], gone: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let frame = loop {
        let frame = last_frame(seen);
        if shows.iter().all(|s| frame.contains(s)) {
            break frame;
        }
        assert!(Instant::now() < deadline, "no {shows:?} in: {frame:?}");
        std::thread::sleep(Duration::from_millis(20));
    };
    for g in gone {
        assert!(!frame.contains(g), "{g:?} still in: {frame:?}");
    }
}

/// A piped `tail -w -n 2 mood` on `db`, with what it drew.
fn piped_watch(dir: &TempDir, db: &str) -> (Watch, Arc<Mutex<Vec<u8>>>) {
    let mut child = Watch(
        datum(dir)
            .args(["--db", db, "tail", "-w", "0.1", "-n", "2", "mood"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let seen = collect(child.0.stdout.take().unwrap());
    (child, seen)
}

/// vividn: «when I delete entries they don't disappear from `tail -w`». A
/// delete in a full window lets an older row in, so the count stays and the
/// newest row is the same one.
#[test]
fn a_piped_watch_drops_a_deleted_entry() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("data.db").to_string_lossy().into_owned();
    three_moods(&dir);
    let (_watch, seen) = piped_watch(&dir, &db);
    frame_shows(&seen, &["bravo", "charlie"], &["alpha"]);
    ok(&dir, &["delete", &hid_of(&dir, &db, "bravo")]);
    frame_shows(&seen, &["alpha", "charlie"], &["bravo"]);
}

/// The TUI's own loop, the same.
#[cfg(unix)]
#[test]
fn the_tui_drops_a_deleted_entry() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("data.db").to_string_lossy().into_owned();
    three_moods(&dir);
    let (_watch, mut master, seen) = tui(&dir, &["tail", "-w", "0.1", "-n", "2", "mood"], 24);
    wait_for(&seen, "q quit");
    let bravo = hid_of(&dir, &db, "bravo");
    seen.lock().unwrap().clear();
    ok(&dir, &["delete", &bravo]);
    wait_for(&seen, "alpha");
    wait_for(&seen, "q quit");
    let drawn = String::from_utf8_lossy(&seen.lock().unwrap()).into_owned();
    assert!(!drawn.contains("bravo"), "bravo still in: {drawn:?}");
    master.write_all(b"q").unwrap();
}

/// The same from datum.day: an edit and a delete made on another copy and
/// applied by a sync, the edit older than the window's newest change.
#[test]
fn a_watch_follows_an_edit_and_a_delete_synced_in() {
    let dir = tempfile::tempdir().unwrap();
    let path = |name: &str| dir.path().join(name).to_string_lossy().into_owned();
    let (here, there) = (path("data.db"), path("there.db"));
    let sync = |from: &str, to: &str| {
        let vector = ok(&dir, &["--db", to, "sync", "pipe", "vector"]);
        let ops = ok(
            &dir,
            &[
                "--db",
                from,
                "sync",
                "pipe",
                "ops",
                "--since",
                vector.trim(),
            ],
        );
        let mut apply = datum(&dir)
            .args(["--db", to, "sync", "pipe", "apply"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        apply
            .stdin
            .take()
            .unwrap()
            .write_all(ops.as_bytes())
            .unwrap();
        assert!(apply.wait().unwrap().success());
    };
    three_moods(&dir);
    sync(&here, &there);
    let bravo = hid_of(&dir, &there, "bravo");
    ok(
        &dir,
        &["--db", &there, "update", &bravo, "state=bravissimo"],
    );
    std::thread::sleep(Duration::from_millis(5));
    let charlie = hid_of(&dir, &here, "charlie");
    ok(&dir, &["update", &charlie, "state=charles"]);

    let (_watch, seen) = piped_watch(&dir, &here);
    frame_shows(&seen, &["bravo", "charles"], &["alpha"]);
    sync(&there, &here);
    frame_shows(&seen, &["bravissimo", "charles"], &["alpha"]);
    ok(&dir, &["--db", &there, "delete", &bravo]);
    sync(&there, &here);
    frame_shows(&seen, &["alpha", "charles"], &["bravissimo"]);
}
