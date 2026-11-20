//! A value holding a control or bidi character is drawn visibly on every
//! terminal sink, the way a device label is refused one (da-0z8cb): an ESC
//! or a BEL reaching a reader's terminal could retitle or clear it, and a
//! bidi override makes one value display as another. The value stays as
//! stored, `--show json` carries it as JSON does, and `\n` and `\t` are
//! left as they are.

use std::path::Path;

/// ESC (an OSC retitle), BEL, a right-to-left override, a newline, a tab.
const VALUE: &str = "a\u{1b}]0;PWN\u{7}b\u{202E}c\nd\te";
/// Each drawn as its escape.
const DRAWN: [&str; 3] = ["\\u{1b}]0;PWN", "\\u{7}b", "\\u{202e}c"];

fn datum(db: &Path, color: bool, args: &[&str]) -> String {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_datum"));
    cmd.arg("--db")
        .arg(db)
        .args(args)
        .env("HOME", db.parent().unwrap())
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("TZ", "Europe/Berlin")
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR");
    match color {
        true => cmd.env("FORCE_COLOR", "1"),
        false => cmd.env("NO_COLOR", "1"),
    };
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// No raw control but `\n` and `\t`, no bidi control, and with color an
/// ESC only as datum's own SGR (`ESC [ digits ; m`), never drawn as text.
fn assert_safe(what: &str, out: &str) {
    let mut rest = out;
    while let Some(i) = rest.find('\u{1b}') {
        let sgr = rest[i + 1..].strip_prefix('[').and_then(|s| {
            let end = s.find(|c: char| !(c.is_ascii_digit() || c == ';'))?;
            (s.as_bytes()[end] == b'm').then_some(end)
        });
        assert!(
            sgr.is_some(),
            "{what}: a raw escape that is no SGR: {out:?}"
        );
        rest = &rest[i + 1..];
    }
    let raw = |c: char| {
        (c.is_control() && !matches!(c, '\n' | '\t' | '\u{1b}'))
            || datum_core::visible::is_bidi_control(c)
    };
    assert!(!out.chars().any(raw), "{what}: a raw control: {out:?}");
    // The value's ESC is an OSC's (`\u{1b}]`): a `\u{1b}[` drawn is datum's
    // own color, escaped where it should have been painted.
    assert!(
        !out.contains("\\u{1b}["),
        "{what}: color drawn as text: {out:?}"
    );
}

/// [`assert_safe`], and the value's controls each drawn.
fn assert_drawn(what: &str, out: &str) {
    assert_safe(what, out);
    for drawn in DRAWN {
        assert!(out.contains(drawn), "{what}: {drawn} not drawn: {out:?}");
    }
}

#[test]
fn controls_in_a_value_are_drawn_visibly_on_every_terminal_sink() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    let note = format!("note={VALUE}");
    let title = format!("title={VALUE}");
    let writes: [&[&str]; 4] = [
        &["switch", "mood", VALUE, "-y", "-t", "10:00"],
        &["moment", "coffee", &note, "-y", "-t", "11:00"],
        &["note", "tea", VALUE, "-y", "-t", "12:00"],
        &["record", "book", "b1", &title],
    ];
    for args in writes {
        assert_drawn(&format!("{args:?}"), &datum(&db, false, args));
    }
    // Each read, and whether it paints with color on.
    let reads: [(&[&str], bool); 10] = [
        (&["get", "mood"], true),
        (&["get", "coffee"], true),
        (&["get", "book"], true),
        (&["history", "mood"], true),
        (&["tail"], true),
        (&["tail", "--column", "note"], true),
        (&["why", "tea"], false),
        (&["why", "tea", "--full"], true),
        (&["stats", "mood"], false),
        (&["states", "mood"], false),
    ];
    for (args, paints) in reads {
        assert_drawn(&format!("{args:?}"), &datum(&db, false, args));
        let out = datum(&db, true, args);
        assert_drawn(&format!("{args:?} color"), &out);
        // What color mode promises survives: real SGR, painted.
        assert_eq!(out.contains("\u{1b}["), paints, "{args:?} color: {out:?}");
    }
    // The all-fields stats paints each field and holds no value.
    let out = datum(&db, true, &["stats"]);
    assert_safe("stats color", &out);
    assert!(out.contains("\u{1b}["), "stats color: {out:?}");

    // `\n` and `\t` stay as they are.
    let got = datum(&db, false, &["get", "coffee"]);
    assert!(got.contains("c\nd\te"), "{got:?}");

    // The value stays as stored, and JSON carries it as JSON does; only
    // what the echo draws as text (`info`) is drawn visibly.
    let json = datum(&db, false, &["get", "mood", "--show", "json"]);
    assert!(
        json.contains("\\u001b") && json.contains('\u{202E}'),
        "{json}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let doc = &parsed["docs"][0];
    assert_eq!(doc["state"], VALUE, "{json}");
    assert_drawn("echo info", doc["info"].as_str().unwrap());
}
