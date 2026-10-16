//! A new field name refuses a control character — a newline or a tab in it
//! breaks every line-oriented surface: `tail`, `fields`, the shell
//! completion's `value<TAB>description` lines (da-fi2vu). Refused the way a
//! comma is (da-6p3du), so only a name being chosen is checked.

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};

fn refusal(args: &[&str], db: &datum_core::db::Db) -> String {
    format!("{:#}", run_cli(args, db).unwrap_err())
}

#[test]
fn a_control_character_is_refused_by_name() {
    let db = fresh_db();
    for (args, field, says) in [
        (
            vec!["occur", "a\nb"],
            "a\nb",
            r#"a newline (U+000A): "a\nb""#,
        ),
        (vec!["occur", "c\td"], "c\td", r#"a tab (U+0009): "c\td""#),
        (
            vec!["setup", "x\ry", "--verb", "occur"],
            "x\ry",
            r#"a carriage return (U+000D): "x\ry""#,
        ),
        (
            vec!["occur", "e\u{7f}f"],
            "e\u{7f}f",
            r#"a control character (U+007F): "e\u{7f}f""#,
        ),
    ] {
        let err = refusal(&args, &db);
        assert!(
            err.contains(&format!("field name cannot contain {says}")),
            "{args:?}: {err}"
        );
        assert!(logged_entries(&db, field).is_empty(), "{args:?}");
        assert!(db.load_spec(field).unwrap().is_none(), "{args:?}");
    }
}

/// Only ASCII controls: a C1 next-line (U+0085) is a name
/// `tests/fixtures/field-name.json` accepts.
#[test]
fn spaces_unicode_and_c1_still_log() {
    let db = fresh_db();
    for field in ["tea time", "café", "日記", "my\u{85}coffee"] {
        run_cli(&["occur", field], &db).unwrap();
        assert_eq!(logged_entries(&db, field).len(), 1, "{field}");
    }
}
