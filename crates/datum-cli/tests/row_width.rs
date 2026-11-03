//! `--width N` on every list of rows (da-ky7m6): a terminal's width when
//! stdout is one, unlimited when piped, `0` unlimited anywhere; the text
//! columns give way with `…`, and numbers, dates, times and ids stay whole.
#![cfg(unix)]

use std::io::Read;
use std::process::{Command, Stdio};

use tempfile::TempDir;

/// Long enough that no 40-column line holds it.
const LONG: &str = "a_very_long_field_name_of_33_chars";

fn datum(dir: &TempDir) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
    c.env("DATUM_DB", dir.path().join("data.db"))
        .env("DATUM_HOME", dir.path().join("home"))
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("NO_COLOR", "1")
        .env_remove("COLUMNS");
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

/// What `datum <args>` prints to a terminal `cols` wide.
fn on_a_terminal(dir: &TempDir, args: &[&str], cols: u16) -> String {
    use std::os::fd::FromRawFd;
    let (mut master, mut slave) = (0, 0);
    let size = libc::winsize {
        ws_row: 50,
        ws_col: cols,
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
    let (mut master, slave) = unsafe {
        (
            std::fs::File::from_raw_fd(master),
            std::fs::File::from_raw_fd(slave),
        )
    };
    let mut child = datum(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(slave)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    // The slave closes with the child: Linux then reads EIO, as EOF.
    while let Ok(n @ 1..) = master.read(&mut buf) {
        out.extend_from_slice(&buf[..n]);
    }
    assert!(
        child.wait().unwrap().success(),
        "`datum {}`",
        args.join(" ")
    );
    String::from_utf8(out).unwrap().replace('\r', "")
}

fn widest(text: &str) -> usize {
    text.lines().map(|l| l.chars().count()).max().unwrap_or(0)
}

/// Under 40 columns no line is wider, every `whole` string is printed
/// whole, and the long name is cut; piped, and at `--width 0` on the
/// terminal, it is whole again.
fn fits(dir: &TempDir, args: &[&str], whole: &[&str]) {
    let fit = on_a_terminal(dir, args, 40);
    assert!(
        widest(&fit) <= 40,
        "`{}` at 40 columns:\n{fit}",
        args.join(" ")
    );
    for w in whole {
        assert!(fit.contains(w), "`{}` cut {w:?}:\n{fit}", args.join(" "));
    }
    assert!(
        !fit.contains(LONG),
        "`{}` kept the name whole:\n{fit}",
        args.join(" ")
    );
    assert!(
        fit.contains(&LONG[..5]),
        "`{}` lost the name:\n{fit}",
        args.join(" ")
    );
    assert!(
        fit.lines().any(|l| l
            .split_whitespace()
            .any(|c| c.starts_with(&LONG[..5]) && c.ends_with('…'))),
        "`{}` did not cut the name with …:\n{fit}",
        args.join(" ")
    );

    let piped = ok(dir, args);
    assert!(piped.contains(LONG), "`{}` piped:\n{piped}", args.join(" "));
    let unlimited = on_a_terminal(dir, &[args, &["--width", "0"]].concat(), 40);
    assert!(
        unlimited.contains(LONG),
        "`{} --width 0`:\n{unlimited}",
        args.join(" ")
    );
    assert!(widest(&unlimited) > 40);

    let narrower = on_a_terminal(dir, &[args, &["--width", "30"]].concat(), 100);
    assert!(
        widest(&narrower) <= 30,
        "`{} --width 30`:\n{narrower}",
        args.join(" ")
    );
}

#[test]
fn tail_fits_the_terminal_and_cuts_the_field() {
    let dir = TempDir::new().unwrap();
    ok(&dir, &["moment", LONG]);
    let hid = ok(&dir, &["ids", "-n", "1"]);
    fits(&dir, &["tail", "-n", "2"], &[&hid.trim()[..5]]);
}

#[test]
fn a_kit_command_fits_its_view() {
    let dir = TempDir::new().unwrap();
    let kit = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kits/chores.toml");
    ok(&dir, &["kit", "install", "--yes", kit]);
    ok(&dir, &["moment", LONG, "next_date=+0"]);
    let today = ok(&dir, &["get", "_LAST::data"]);
    let today: serde_json::Value = serde_json::from_str(&today).unwrap();
    let today = today["next_date"].as_str().unwrap().to_string();
    fits(&dir, &["chorelist"], &[&today]);
}

#[test]
fn kit_ls_fits_and_keeps_its_counts() {
    let dir = TempDir::new().unwrap();
    let kit = dir.path().join("kit.toml");
    std::fs::write(
        &kit,
        format!("[kit]\nname = \"{LONG}\"\nversion = \"12.3\"\n\n[[alias]]\nname = \"zz\"\nexpands_to = \"moment zz\"\n"),
    )
    .unwrap();
    ok(&dir, &["kit", "install", "--yes", kit.to_str().unwrap()]);
    fits(&dir, &["kit", "ls"], &["12.3"]);
}

#[test]
fn every_row_list_names_the_flag_and_nothing_else_takes_it() {
    let dir = TempDir::new().unwrap();
    let kit = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kits/chores.toml");
    ok(&dir, &["kit", "install", "--yes", kit]);
    for page in [
        "tail",
        "head",
        "now",
        "grep",
        "check",
        "stats",
        "fields",
        "states",
        "view",
        "kit",
        "alias",
        "chorelist",
    ] {
        let help = ok(&dir, &[page, "--help"]);
        assert!(
            help.contains("--width"),
            "`{page} --help` names no --width:\n{help}"
        );
    }
    // A group's subcommand that prints no rows refuses it too.
    for args in [
        &["moment", "x"][..],
        &["kit", "show", "chores"],
        &["alias", "show", "x"],
    ] {
        let out = datum(&dir)
            .args(args)
            .args(["--width", "40"])
            .output()
            .unwrap();
        assert!(!out.status.success(), "`{}` took --width", args.join(" "));
        assert!(String::from_utf8_lossy(&out.stderr).contains("prints no rows"));
    }
    let out = datum(&dir).args(["tail", "--width"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("--width requires a value"));
}
