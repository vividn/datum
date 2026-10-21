//! `on`, `off` and `card`, datum.day's names for `start`, `stop` and `add`,
//! are those commands (da-zz6en): the same lines under either name write
//! the same entries, typed or in an alias's body.

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::Value;

/// What `lines` leave in `field`, less what differs between any two
/// writes: the ids, the clocks and the line as typed.
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
                "raw_input",
                "alias_chain",
            ] {
                doc.remove(key);
            }
            v
        })
        .collect()
}

/// `lines` with datum.day's names, or with each word of them the CLI's.
fn spelled(lines: &[&[&str]], cli: bool) -> Vec<Vec<String>> {
    fn word(w: &str, cli: bool) -> &str {
        match (w, cli) {
            ("on", true) => "start",
            ("off", true) => "stop",
            ("card", true) => "add",
            _ => w,
        }
    }
    let token = |t: &&str| {
        t.split(' ')
            .map(|w| word(w, cli))
            .collect::<Vec<_>>()
            .join(" ")
    };
    lines
        .iter()
        .map(|l| l.iter().map(token).collect())
        .collect()
}

#[test]
fn on_off_and_card_write_what_start_stop_and_add_do() {
    let cases: &[(&str, &[&[&str]])] = &[
        ("work", &[&["on", "work", "-t", "9:00"]]),
        ("run", &[&["on", "run", "25m", "-t", "12:30"]]),
        (
            "work",
            &[
                &["on", "work", "-t", "9:00"],
                &["off", "work", "-t", "17:00"],
            ],
        ),
        ("book", &[&["card", "book", "-K", "title", "Dune"]]),
        // In an alias's body, where a trailing command reads the start's
        // duration slot: `bed tail` is `tail sleep`, not a duration «tail».
        (
            "sleep",
            &[
                &["alias", "set", "bed", "on sleep"],
                &["bed", "-t", "22:00"],
                &["bed", "tail"],
            ],
        ),
        // A spec's key binds the alias's positional before the duration.
        (
            "run",
            &[
                &["setup", "run", "--verb", "start", "-k", "pace"],
                &["alias", "set", "r", "on run"],
                &["r", "5", "20m", "-t", "8:00"],
            ],
        ),
        (
            "b",
            &[
                &["on", "b", "-t", "8:00"],
                &["alias", "set", "m", "on a , off b"],
                &["m", "-t", "9:00"],
            ],
        ),
    ];
    for (field, lines) in cases {
        let (web, cli) = (spelled(lines, false), spelled(lines, true));
        let (a, b) = (written(&web, field), written(&cli, field));
        assert!(!a.is_empty(), "{web:?} wrote nothing");
        assert_eq!(a, b, "{web:?} and {cli:?} wrote different entries");
    }
}
