//! v2's trailing verbs (`changeDatumCommand` in `dataArgs.ts`, da-z5vek):
//! on a log line, `occur` / `start` / `end` / `switch` filling the `dur=`
//! slot, or leading the words nothing else takes, is the line's command.
//! The command always comes first: nothing swaps a field before it.

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::json;

fn newest(db: &Db, field: &str) -> Entry {
    let mut rows = logged_entries(db, field);
    rows.sort_by_key(|e| e.occur_time.as_ref().map(|t| t.utc));
    rows.pop().unwrap_or_else(|| panic!("no {field} entry"))
}

/// `bed end` (the sleep kit's `bed` → `start sleep`) ends sleep, and writes
/// nothing under the alias's own name: the head word is never the field.
#[test]
fn an_alias_and_a_trailing_end_end_the_aliased_field() {
    for (alias, field) in [("bed", "sleep"), ("fast", "fasting")] {
        let db = fresh_db();
        run_cli(&["alias", "set", alias, &format!("start {field}")], &db).unwrap();
        run_cli(&[alias, "-t", "-60"], &db).unwrap();
        run_cli(&[alias, "end", "30m"], &db).unwrap();
        let end = newest(&db, field);
        assert_eq!(end.state, Some(json!(false)), "{alias}");
        assert_eq!(end.duration_seconds(), Some(30 * 60), "{alias}");
        assert_eq!(logged_entries(&db, field).len(), 2, "{alias}");
        assert!(logged_entries(&db, alias).is_empty(), "{alias}");
    }
}

#[test]
fn a_verb_in_the_dur_slot_or_left_over_changes_the_command() {
    let db = fresh_db();
    run_cli(&["start", "sleep", "end"], &db).unwrap();
    assert_eq!(newest(&db, "sleep").state, Some(json!(false)));

    run_cli(&["occur", "coffee", "start"], &db).unwrap();
    assert_eq!(newest(&db, "coffee").state, Some(json!(true)));

    run_cli(&["occur", "mood", "switch", "tired"], &db).unwrap();
    assert_eq!(newest(&db, "mood").state, Some(json!("tired")));
}

/// A switch's state is the state being left: `mood happy end` is `end mood
/// -l happy`, through the field's own alias.
#[test]
fn a_switch_ended_by_a_trailing_verb_asserts_the_state_it_named() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy", "-t", "-5"], &db).unwrap();
    run_cli(&["mood", "happy", "end"], &db).unwrap();
    let end = newest(&db, "mood");
    assert_eq!(end.state, Some(json!(false)));
    assert_eq!(end.user_asserted_last_state, Some(json!("happy")));
}

/// A remainder takes the word as text: a note's words are its comment.
#[test]
fn a_verb_with_a_remainder_to_go_to_is_text() {
    let db = fresh_db();
    run_cli(&["alias", "set", "jot", "note jot"], &db).unwrap();
    run_cli(&["jot", "the", "end", "of", "it"], &db).unwrap();
    let jot = newest(&db, "jot");
    assert_eq!(jot.data, Some(json!({"comment": ["the end of it"]})));
}

/// No alias, no swap: `F tail` names no command.
#[test]
fn a_field_before_a_command_is_an_unknown_subcommand() {
    let db = fresh_db();
    for words in [
        &["zzz", "tail"][..],
        &["zzz", "end"],
        &["zzz", "switch", "on"],
    ] {
        let err = run_cli(words, &db).unwrap_err().to_string();
        assert!(
            err.contains("unknown subcommand: 'zzz'"),
            "{words:?}: {err}"
        );
    }
}
