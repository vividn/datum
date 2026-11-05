//! The glossary's words are the commands (da-gpz6m): `change`, `record` and
//! `remark` beside `moment`, `on` and `off`, with the old words kept as
//! aliases; `setup --kind` with `--as` and the old `--verb`; and help and
//! errors that say the new words, the old ones only as aliases.

use crate::common;

use std::process::Command;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::Value;

/// What `lines` leave in `field`, less what differs between any two
/// writes: the ids, the clocks (a remark's slug is its create time) and the
/// line as typed.
fn written(lines: &[Vec<String>], field: &str) -> Vec<Value> {
    let db = fresh_db();
    for line in lines {
        let line: Vec<&str> = line.iter().map(String::as_str).collect();
        run_cli(&line, &db).unwrap_or_else(|e| panic!("datum {}: {e:#}", line.join(" ")));
    }
    logged_entries(&db, field)
        .into_iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).unwrap();
            let doc = v.as_object_mut().unwrap();
            for key in [
                "hid",
                "create_time",
                "modify_time",
                "slug",
                "raw_input",
                "alias_chain",
            ] {
                doc.remove(key);
            }
            v
        })
        .collect()
}

/// `lines`, each glossary word in them spelled as `old` names it.
fn spelled(lines: &[&[&str]], old: &[(&str, &str)]) -> Vec<Vec<String>> {
    let word = |w: &str| {
        old.iter()
            .find(|(new, _)| *new == w)
            .map_or(w.to_string(), |(_, o)| o.to_string())
    };
    lines
        .iter()
        .map(|l| {
            l.iter()
                .map(|t| t.split(' ').map(word).collect::<Vec<_>>().join(" "))
                .collect()
        })
        .collect()
}

/// Each old word for each command, so every alias is read once.
const OLD: &[&[(&str, &str)]] = &[
    &[
        ("moment", "occur"),
        ("on", "start"),
        ("off", "stop"),
        ("change", "switch"),
        ("record", "add"),
        ("remark", "note"),
    ],
    &[("off", "end"), ("record", "card")],
];

#[test]
fn each_command_and_its_aliases_log_the_same_entry() {
    let cases: &[(&str, &[&[&str]])] = &[
        ("mood", &[&["change", "mood", "happy", "-t", "9:00"]]),
        ("mood", &[&["change", "mood", "sad", "20m", "-t", "9:00"]]),
        // `change light off` turns it off, as `off light` does.
        (
            "light",
            &[
                &["on", "light", "-t", "8:00"],
                &["change", "light", "off", "-t", "9:00"],
            ],
        ),
        ("book", &[&["record", "book", "dune", "title=x"]]),
        ("book", &[&["record", "book", "-K", "title", "Dune"]]),
        ("coffee", &[&["remark", "coffee", "too", "strong"]]),
        (
            "work",
            &[
                &["on", "work", "-t", "9:00"],
                &["off", "work", "-t", "17:00"],
            ],
        ),
        ("tea", &[&["moment", "tea", "-t", "9:00"]]),
        // A trailing command word changes the line, as v2's did.
        ("tea", &[&["moment", "tea", "change", "-t", "9:00"]]),
        // Sides, arrays and an alias's body.
        (
            "mood",
            &[&["moment", "tea", "-t", "9:00", ",", "change", "mood", "calm"]],
        ),
        ("a", &[&["change", "a,b", "x", "-t", "9:00"]]),
        (
            "environment",
            &[
                &["alias", "set", "envi", "change environment"],
                &["envi", "home", "-t", "9:00"],
                &["envi", "x", "change", "out", "-t", "10:00"],
            ],
        ),
    ];
    for (field, lines) in cases {
        let new = written(&spelled(lines, &[]), field);
        assert!(!new.is_empty(), "{lines:?} wrote nothing");
        for old in OLD {
            let old_lines = spelled(lines, old);
            assert_eq!(
                written(&old_lines, field),
                new,
                "{lines:?} and {old_lines:?} wrote different entries"
            );
        }
    }
}

#[test]
fn kind_as_and_verb_store_the_same_spec() {
    let spec = |args: &[&str]| {
        let db = fresh_db();
        run_cli(&[&["setup", "x"], args].concat(), &db).unwrap();
        serde_json::to_value(db.load_spec("x").unwrap().unwrap()).unwrap()
    };
    for (kind, old) in [
        ("moment", "occur"),
        ("on/off", "start"),
        ("change", "switch"),
        ("record", "add"),
    ] {
        let want = spec(&["--kind", kind]);
        assert_eq!(want["verb"], old, "{kind} stores the old name");
        for args in [
            &["--as", kind][..],
            &[&format!("--kind={kind}")],
            &["--verb", old],
            &["--kind", old],
        ] {
            assert_eq!(spec(args), want, "setup x {args:?}");
        }
    }
}

fn datum(args: &[&str]) -> std::process::Output {
    let tmp = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_HOME", tmp.path())
        .env("DATUM_CONFIG_HOME", tmp.path())
        .env("DATUM_DB", tmp.path().join("v.db"))
        .env("NO_COLOR", "1")
        .env_remove("COLUMNS")
        // An error's backtrace names functions (`parse_verb`), not words.
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE")
        .args(args)
        .output()
        .expect("spawn datum")
}

/// A line naming an old word other than as an alias, or `None`. The words
/// a reader is told of stay: a stored column (`verb`, `state|verb|…`, a
/// spec's `"verb"` key), the checker of it, and the shell loop's list.
fn old_word(line: &str) -> Option<&'static str> {
    const OLD_COMMANDS: &[&str] = &[
        "switch", "add", "card", "note", "occur", "start", "stop", "end",
    ];
    if line.contains("(alias: ") || line.contains("(also: ") {
        return None;
    }
    for w in OLD_COMMANDS {
        for form in [format!("datum {w} "), format!("`{w} "), format!("`{w}`")] {
            if line.contains(&form) {
                return Some(w);
            }
        }
    }
    let stored = [
        "verb,",
        "|verb|",
        "\"verb\"",
        "`verb`",
        "stored verb",
        "check verbs",
        "verbs  ",
    ];
    let verb = line
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .any(|w| matches!(w, "verb" | "verbs" | "--verb"));
    (verb && !stored.iter().any(|s| line.contains(s)) && !line.contains("the old `--verb`"))
        .then_some("verb")
}

#[test]
fn no_help_page_names_an_old_word_but_as_an_alias() {
    let table: Value = serde_json::from_slice(&datum(&["help", "--json"]).stdout).unwrap();
    fn walk(c: &Value, path: Vec<String>, out: &mut Vec<Vec<String>>) {
        out.push(path.clone());
        for s in c["subcommands"].as_array().into_iter().flatten() {
            let mut p = path.clone();
            p.push(s["name"].as_str().unwrap().to_string());
            walk(s, p, out);
        }
    }
    let mut pages = vec![vec!["help".to_string()]];
    for topic in ["logging", "names", "time", "env"] {
        pages.push(vec!["help".into(), topic.into()]);
    }
    for c in table.as_array().unwrap() {
        let mut found = Vec::new();
        walk(c, vec![c["name"].as_str().unwrap().to_string()], &mut found);
        pages.extend(found.into_iter().map(|mut p| {
            p.push("--help".into());
            p
        }));
    }
    assert!(pages.len() > 60, "{} pages", pages.len());
    let mut said = Vec::new();
    for page in &pages {
        let args: Vec<&str> = page.iter().map(String::as_str).collect();
        let text = String::from_utf8(datum(&args).stdout).unwrap();
        assert!(!text.is_empty(), "{args:?}");
        for line in text.lines() {
            if let Some(w) = old_word(line) {
                said.push(format!("{args:?} says {w}: {line}"));
            }
        }
    }
    assert!(said.is_empty(), "{}", said.join("\n"));
    // The scan finds an old word where one is.
    assert_eq!(old_word("  datum switch mood happy"), Some("switch"));
    assert_eq!(old_word("needs `--verb`, unless"), Some("verb"));
    assert_eq!(
        old_word("  change   Put a field in a new state (alias: switch)"),
        None
    );
}

#[test]
fn errors_say_the_new_words() {
    for line in [
        &["setup", "q", "--kind", "bogus"][..],
        &["track", "q"],
        &["moment", "q", "-I", "x"],
        &["moment", "q", "--if-changed"],
        &["moment", "q", "state=x"],
        &["record", "q"],
        &["on", "q", "+=x"],
    ] {
        let out = datum(line);
        assert!(!out.status.success(), "{line:?} should fail");
        let err = String::from_utf8(out.stderr).unwrap();
        assert!(!err.is_empty(), "{line:?}");
        for l in err.lines() {
            assert_eq!(old_word(l), None, "{line:?}: {err}");
        }
    }
}

/// What a person reads of the user docs says a kind of data by its word
/// (da-7j00d): no `switch`, `card` or field «shape» for a kind, no old
/// command typed. concepts.md names the old words where it says what they
/// were: its glossary table, the aliases paragraph and the stored column.
#[test]
fn the_user_docs_say_the_new_words() {
    let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let kind = |l: &str| {
        let lower = l.to_lowercase();
        ["shape", "**switch**", "**card**"]
            .into_iter()
            .find(|w| lower.contains(w))
    };
    let mut said = Vec::new();
    for name in ["README", "basics", "concepts", "agents", "install", "sync"] {
        let text = std::fs::read_to_string(docs.join(format!("{name}.md"))).unwrap();
        let mut told = false;
        for line in text.lines() {
            // A passage telling the old words, to its blank line or next bullet.
            if line.trim().is_empty() || line.starts_with("- ") {
                told = false;
            }
            if name == "concepts"
                && (line.starts_with("The old words still run")
                    || line.starts_with("- **Stored classifier**"))
            {
                told = true;
            }
            if told || (name == "concepts" && line.starts_with('|')) {
                continue;
            }
            // A file's path names its module, not a word.
            let words: String = line
                .split('`')
                .enumerate()
                .map(|(i, s)| if i % 2 == 1 && s.contains('/') { "" } else { s })
                .collect::<Vec<_>>()
                .join("`");
            if let Some(w) = old_word(&words).or(kind(&words)) {
                said.push(format!("docs/{name}.md says {w}: {line}"));
            }
        }
    }
    assert!(said.is_empty(), "{}", said.join("\n"));
    assert_eq!(kind("- ▞ **switch**: switch data"), Some("**switch**"));
    assert_eq!(kind("## Field shapes"), Some("shape"));
}
