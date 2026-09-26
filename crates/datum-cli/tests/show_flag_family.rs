//! One output-flag family (da-qt6z).
//!
//! `--show <V>` and `--json` are read by the shared argument layer for
//! every command, the way `--db` is — not thirteen times by hand. Three
//! claims, each of which fails on the main this bead branched from:
//!
//! 1. `--json` is exactly `--show json`, on every command that has a JSON
//!    document — and the loop prints its DENOMINATOR, so a command that
//!    quietly leaves the family is a visible `12 of 13`.
//! 2. `--show` reaches a command that never had it (`fields --show json`),
//!    and `--json` reaches one that never had it (`occur x --json`).
//! 3. `spec` is in the family like everything else: its `--json <JSON>`
//!    input flag is `--from-json <JSON>` now (vividn, 2026-09-15), and the
//!    old spelling says so rather than silently dropping the body.
//!
//! Driven through the real binary, because what is asserted is stdout.

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn datum(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("spawn datum")
}

fn ok(db: &Path, args: &[&str]) -> String {
    let out = datum(db, args);
    assert!(
        out.status.success(),
        "datum {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// Blank the wall clock a `now`-shaped reader puts in its output — an
/// RFC-3339 stamp, an epoch-millisecond integer, or a geometry float that
/// a layout derived from one. The two spellings run milliseconds apart, and that difference is not the
/// one under test — every key, string, label and colour still is, and a
/// command that ignored the flag would differ in all of them.
fn mask_times(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if is_stamp(&c[i..]) {
            out.push_str("<time>");
            i += 19;
            while i < c.len() && (c[i].is_ascii_digit() || matches!(c[i], '.' | 'Z' | '+' | ':')) {
                i += 1;
            }
            continue;
        }
        if c[i] == '.' {
            let run = c[i + 1..].iter().take_while(|d| d.is_ascii_digit()).count();
            if run >= 1 {
                out.push_str(".<frac>");
                i += 1 + run;
                continue;
            }
        }
        let run = c[i..].iter().take_while(|d| d.is_ascii_digit()).count();
        if run >= 4 {
            out.push_str("<n>");
            i += run;
            continue;
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// `YYYY-MM-DDTHH:MM:SS` at the head of `c` — 19 characters.
fn is_stamp(c: &[char]) -> bool {
    const SHAPE: [char; 19] = [
        'd', 'd', 'd', 'd', '-', 'd', 'd', '-', 'd', 'd', 'T', 'd', 'd', ':', 'd', 'd', ':', 'd',
        'd',
    ];
    c.len() >= SHAPE.len()
        && SHAPE.iter().zip(c).all(|(&want, &got)| match want {
            'd' => got.is_ascii_digit(),
            other => got == other,
        })
}

/// The day the seed is logged on — a past one, so `dayview` of it draws
/// the open `mood` block to the day's end and not to the wall clock, which
/// moved between the two spellings and made them differ (da-u56cg).
const DAY: &str = "2024-03-16";

/// A fresh db with three entries — enough that every reader below has
/// something to render.
fn seeded() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let db = tmp.path().join("show_family.db");
    ok(&db, &["occur", "coffee", "-d", DAY, "-t", "9:15"]);
    ok(&db, &["switch", "mood", "happy", "-d", DAY, "-t", "9:20"]);
    ok(&db, &["occur", "coffee", "-d", DAY, "-t", "9:30"]);
    (tmp, db)
}

/// The thirteen commands that carried their own `json: bool`, each with
/// arguments that make it a command the CLI will actually run.
const JSON_COMMANDS: &[&[&str]] = &[
    &["fields"],
    &["now"],
    &["nowview"],
    &["tail"],
    &["dayview", "-d", DAY],
    &["stats", "coffee"],
    &["count"],
    &["history", "coffee:_"],
    &["ids"],
    &["config", "show"],
    &["check"],
    &["alias", "list"],
    // `db restore` of a backup this db just wrote — a no-op replay, which
    // is still a report, and the report is what the flag shapes.
    &["db", "restore", "-", "--dry-run"],
];

#[test]
fn json_is_show_json_on_every_command_that_has_one() {
    let (_tmp, db) = seeded();
    let backup = db.parent().unwrap().join("backup.jsonl");
    ok(&db, &["db", "backup", backup.to_str().unwrap()]);

    let mut agreed = 0usize;
    for argv in JSON_COMMANDS {
        let mut with_json: Vec<&str> = argv.to_vec();
        let mut with_show: Vec<&str> = argv.to_vec();
        // `db restore -` reads the backup from stdin; give both spellings
        // the same file instead so the two runs see the same input.
        if argv[0] == "db" {
            let path = backup.to_str().unwrap();
            with_json[2] = path;
            with_show[2] = path;
        }
        with_json.push("--json");
        with_show.extend_from_slice(&["--show", "json"]);

        let a = datum(&db, &with_json);
        let b = datum(&db, &with_show);
        assert_eq!(
            a.status.code(),
            b.status.code(),
            "{argv:?}: --json and --show json exited differently"
        );
        assert_eq!(
            mask_times(&String::from_utf8_lossy(&a.stdout)),
            mask_times(&String::from_utf8_lossy(&b.stdout)),
            "{argv:?}: --json and --show json printed different bytes"
        );
        assert!(
            a.status.success() || argv[0] == "check",
            "datum {with_json:?}: {}",
            String::from_utf8_lossy(&a.stderr)
        );
        // Agreeing on nothing would be cheap: both spellings must have
        // actually produced the document.
        let text = String::from_utf8_lossy(&a.stdout);
        let start = text
            .find(['{', '['])
            .unwrap_or_else(|| panic!("{argv:?} printed no JSON: {text:?}"));
        serde_json::from_str::<serde_json::Value>(text[start..].trim())
            .unwrap_or_else(|e| panic!("{argv:?}: not JSON ({e}): {text:?}"));
        agreed += 1;
    }
    assert_eq!(
        agreed,
        JSON_COMMANDS.len(),
        "{agreed} of {} commands agree",
        JSON_COMMANDS.len()
    );
}

/// The family reaches commands that had only one half of it before.
#[test]
fn both_spellings_reach_a_command_that_had_neither() {
    let (_tmp, db) = seeded();
    // `fields` took `--json` and rejected `--show`.
    let listing = ok(&db, &["fields", "--show", "json"]);
    assert!(listing.trim_start().starts_with('['), "{listing:?}");
    // …and the log verbs took `--show` and rejected `--json`.
    let echo = ok(&db, &["occur", "x", "--json"]);
    assert!(echo.contains("\"docs\":["), "{echo:?}");
}

/// `--show none` on a reader silences stdout and leaves the exit code
/// alone; `--show none --show json` prints the document, because the last
/// one wins now (the `none` latch is gone).
#[test]
fn none_silences_a_reader_and_does_not_latch() {
    let (_tmp, db) = seeded();
    for argv in [
        vec!["tail", "--show", "none"],
        vec!["fields", "--show", "none"],
        vec!["count", "--show", "none"],
    ] {
        let out = datum(&db, &argv);
        assert!(out.status.success(), "{argv:?} should still exit 0");
        assert_eq!(out.stdout, b"", "{argv:?} should print nothing");
    }

    let printed = ok(&db, &["occur", "tea", "--show", "none", "--show", "json"]);
    assert_eq!(printed.lines().count(), 1, "one document: {printed:?}");
    assert!(printed.contains("\"docs\":["), "{printed:?}");
}

/// A value the named command has no rendering for is an error naming
/// both — loud beats silently ignored.
#[test]
fn a_show_the_command_cannot_draw_is_refused_by_name() {
    let (_tmp, db) = seeded();
    let out = datum(&db, &["fields", "--show", "minimal"]);
    assert!(!out.status.success(), "should have refused");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("fields: --show minimal is not a rendering fields has"),
        "{stderr:?}"
    );
}

/// `spec` joins the family: the spec body is `--from-json` now, and the
/// old `--json` spelling says where it went instead of dropping it.
#[test]
fn spec_set_takes_the_body_from_from_json() {
    let (_tmp, db) = seeded();
    let body = r#"{"field":"x","verb":"occur"}"#;
    ok(&db, &["spec", "set", "x", "--from-json", body]);
    let shown = ok(&db, &["spec", "show", "x"]);
    assert!(shown.contains("\"verb\": \"occur\""), "{shown:?}");

    let out = datum(&db, &["spec", "set", "y", "--json", body]);
    assert!(!out.status.success(), "the old spelling must not pass");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--from-json"), "{stderr:?}");

    // …and `--json` now means what it means everywhere: `--show json`.
    let explicit = ok(&db, &["spec", "show", "x", "--json"]);
    assert_eq!(explicit, shown, "`--show json` is spec show's default");
}

/// `--json` takes no value, so it must not eat the flag behind it.
#[test]
fn json_does_not_swallow_the_next_flag() {
    let (_tmp, db) = seeded();
    let two = ok(&db, &["tail", "--json", "-n", "2"]);
    let one = ok(&db, &["tail", "--json", "-n", "1"]);
    assert!(
        two.len() > one.len(),
        "-n still bounds the rows: {} vs {}",
        two.len(),
        one.len()
    );
}
