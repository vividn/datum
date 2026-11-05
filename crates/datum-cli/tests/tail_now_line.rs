//! da-99sz4: a tail listing entries on both sides of now has a yellow dotted
//! line between the last past row and the first future one (vividn,
//! 2026-11-05: «in the CLI tail, if there are entries in the future separate
//! the past and future entries with a yellow dotted line»).

use datum_commands::render::{strip_ansi, visible_width};
use std::process::Command;
use tempfile::TempDir;

const DOTS: char = '┈';

fn datum(dir: &TempDir, color: bool, args: &[&str]) -> String {
    let d = dir.path();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
    cmd.args(args)
        .env("HOME", d)
        .env("DATUM_HOME", d.join("home"))
        .env("DATUM_CONFIG_HOME", d.join("config"))
        .env("XDG_CONFIG_HOME", d.join("xc"))
        .env("XDG_DATA_HOME", d.join("xd"))
        .env("XDG_STATE_HOME", d.join("xs"))
        .env("DATUM_DB", d.join("data.db"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("TZ", "UTC")
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE");
    if color {
        cmd.env("FORCE_COLOR", "1");
    } else {
        cmd.env("NO_COLOR", "1");
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The indices of the dotted lines among the table's lines.
fn dotted(lines: &[&str]) -> Vec<usize> {
    let lines = lines.iter().map(|l| strip_ansi(l));
    let lines: Vec<String> = lines.collect();
    let is_dots = |l: &String| !l.is_empty() && l.chars().all(|c| c == DOTS);
    (0..lines.len()).filter(|&i| is_dots(&lines[i])).collect()
}

#[test]
fn the_line_stands_between_past_and_future_and_nowhere_else() {
    let dir = TempDir::new().unwrap();
    datum(&dir, false, &["moment", "coffee", "-t", "-120"]);
    datum(&dir, false, &["moment", "dentist", "-t", "+180"]);

    let plain = datum(&dir, false, &["tail"]);
    let lines: Vec<&str> = plain.lines().collect();
    // Header, coffee, the line, dentist.
    assert_eq!(dotted(&lines), [2], "{plain}");
    assert!(
        lines[1].contains("coffee") && lines[3].contains("dentist"),
        "{plain}"
    );
    assert!(!plain.contains('\x1b'), "{plain}");
    let widest = lines.iter().map(|l| visible_width(l)).max().unwrap();
    assert_eq!(visible_width(lines[2]), widest, "{plain}");

    let colored = datum(&dir, true, &["tail"]);
    let lines: Vec<&str> = colored.lines().collect();
    assert_eq!(dotted(&lines), [2], "{colored}");
    // The renderer's warning yellow, #ffd700.
    assert!(lines[2].starts_with("\x1b[38;2;255;215;0m"), "{colored:?}");
    assert_eq!(strip_ansi(&colored), plain);

    // Data has no line.
    let json = datum(&dir, false, &["tail", "--show", "json"]);
    assert!(!json.contains(DOTS), "{json}");
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(doc["rows"].as_array().unwrap().len(), 2, "{json}");

    // A listing by create time has no now in it.
    let by_create = datum(&dir, false, &["tail", "-m", "create"]);
    assert!(!by_create.contains(DOTS), "{by_create}");
}

#[test]
fn no_line_with_only_one_side() {
    let dir = TempDir::new().unwrap();
    datum(&dir, false, &["moment", "coffee", "-t", "-120"]);
    datum(&dir, false, &["moment", "tea", "-t", "-60"]);
    let past = datum(&dir, false, &["tail"]);
    assert!(!past.contains(DOTS), "{past}");

    let dir = TempDir::new().unwrap();
    datum(&dir, false, &["moment", "dentist", "-t", "+180"]);
    datum(&dir, false, &["moment", "trip", "-t", "+300"]);
    let future = datum(&dir, false, &["tail"]);
    assert!(!future.contains(DOTS), "{future}");
}
