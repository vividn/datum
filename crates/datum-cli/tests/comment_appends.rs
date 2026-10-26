//! `-c` always APPENDS to the `data.comment` array, and `delete -c` records
//! why as `deleted: <why>` (da-ult68).

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::{json, Value};

fn comments(db: &datum_core::db::Db, field: &str) -> Vec<Value> {
    db.list_including_deleted()
        .unwrap()
        .into_iter()
        .filter(|e| e.field == field)
        .map(|e| e.data.unwrap_or(Value::Null)["comment"].clone())
        .collect()
}

#[test]
fn update_c_appends_to_the_comments() {
    let db = fresh_db();
    run_cli(&["occur", "x", "-c", "a"], &db).unwrap();
    assert_eq!(comments(&db, "x"), [json!(["a"])]);
    run_cli(&["update", "x:_", "-c", "b"], &db).unwrap();
    assert_eq!(comments(&db, "x"), [json!(["a", "b"])]);
}

/// A row written before comments were arrays holds a STRING; `update -c`
/// keeps it and appends after it, never replacing it (vremya, #159).
#[test]
fn update_c_keeps_a_legacy_string_comment() {
    let db = fresh_db();
    let e = datum_core::test_util::entry_builder("x")
        .extra_data(json!({"comment": "electricity"}))
        .insert(&db);
    assert_eq!(comments(&db, "x"), [json!("electricity")]);
    run_cli(&["update", e.hid.as_str(), "-c", "paid"], &db).unwrap();
    assert_eq!(comments(&db, "x"), [json!(["electricity", "paid"])]);
}

#[test]
fn comment_remainder_appends() {
    let db = fresh_db();
    run_cli(&["occur", "x", "-C", "the", "rest"], &db).unwrap();
    run_cli(&["occur", "y", "-c", "a", "-C", "the", "rest"], &db).unwrap();
    assert_eq!(comments(&db, "x"), [json!(["the rest"])]);
    assert_eq!(comments(&db, "y"), [json!(["a", "the rest"])]);
}

#[test]
fn delete_c_records_why_on_the_tombstone() {
    let db = fresh_db();
    run_cli(&["occur", "x", "-c", "a"], &db).unwrap();
    run_cli(&["delete", "x:_", "-c", "wrong time"], &db).unwrap();
    assert!(logged_entries(&db, "x").is_empty(), "the row is deleted");
    let dead = datum_core::test_util::logged_all(&db);
    assert!(dead[0].deleted_at.is_some());
    assert_eq!(comments(&db, "x"), [json!(["a", "deleted: wrong time"])]);
}

/// `undo` after `delete -c` brings the row back exactly as it was before the
/// delete — the reason gone with the tombstone — whether the row has a
/// history trail (undo reverts through it) or none, as an imported or
/// pre-history row has (canary glm + vremya, #159). Without a trail undo
/// used to fall back to `undelete`, which keeps `data`, reason included.
#[test]
fn undo_of_delete_c_restores_the_row_exactly() {
    let restored = |data: Value, trail: bool| {
        let db = fresh_db();
        let e = datum_core::test_util::entry_builder("x")
            .extra_data(data)
            .insert(&db);
        if !trail {
            db.conn()
                .execute(
                    "DELETE FROM entry_history WHERE entry_hid = ?1",
                    [e.hid.as_str()],
                )
                .unwrap();
        }
        let before = logged_entries(&db, "x")[0].data.clone();
        run_cli(&["delete", e.hid.as_str(), "-c", "why"], &db).unwrap();
        run_cli(&["undo", e.hid.as_str()], &db).unwrap();
        let live = logged_entries(&db, "x");
        (
            live.len() == 1 && live[0].data == before,
            live[0].data.clone(),
        )
    };
    let kept = json!({"comment": ["kept"]});
    let bare = json!({"n": 1});
    assert_eq!(
        [
            restored(kept.clone(), false),
            restored(bare.clone(), false),
            restored(kept.clone(), true),
            restored(bare.clone(), true),
        ],
        [
            (true, Some(kept.clone())),
            (true, Some(bare.clone())),
            (true, Some(kept)),
            (true, Some(bare)),
        ],
    );
}

#[test]
fn comma_batched_delete_c_appends_to_every_row() {
    let db = fresh_db();
    run_cli(&["occur", "x"], &db).unwrap();
    run_cli(&["occur", "y", "-c", "kept"], &db).unwrap();
    run_cli(&["delete", "x:_,y:_", "--comment", "dupe"], &db).unwrap();
    assert_eq!(comments(&db, "x"), [json!(["deleted: dupe"])]);
    assert_eq!(comments(&db, "y"), [json!(["kept", "deleted: dupe"])]);
}

/// One comment reads as itself wherever a value prints bare — the
/// `tail --column` table and a `-o` format string, as in the data block —
/// and two print as the list the data block shows (glm + vremya, #159).
#[test]
fn one_comment_prints_bare_in_columns_and_format_strings() {
    let tmp = tempfile::tempdir().unwrap();
    let datum = |args: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", tmp.path().join("t.db"))
            .env("DATUM_HOME", tmp.path())
            .env("DATUM_CONFIG_HOME", tmp.path())
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "datum {args:?} failed");
        String::from_utf8(out.stdout).unwrap()
    };
    datum(&["occur", "x", "-c", "a"]);
    datum(&["occur", "y", "-c", "a", "-c", "b"]);

    let formatted: Vec<String> = datum(&["tail", "-o", "%comment%"])
        .lines()
        .map(str::to_string)
        .collect();

    let column: Vec<String> = datum(&["tail", "--column", "comment"])
        .lines()
        .skip(1)
        .map(|l| l.rsplit("  ").next().unwrap().to_string())
        .collect();
    let want = vec!["a".to_string(), r#"["a","b"]"#.to_string()];
    assert_eq!((formatted, column), (want.clone(), want));
}

/// A `get` matcher reads a comment as the data block does: one comment is
/// itself, so `comment=a` finds the row `-c a` wrote, as it did when a
/// lone comment was stored as a string.
#[test]
fn a_get_matcher_reads_one_comment_as_itself() {
    let db = fresh_db();
    run_cli(&["occur", "x", "-c", "a"], &db).unwrap();
    run_cli(&["occur", "y", "comment=b"], &db).unwrap();
    let found = |args: &[&str]| run_cli(args, &db).is_ok();
    assert_eq!(
        (
            found(&["get", "x", "comment=a"]),
            found(&["get", "y", "comment=b"]),
            found(&["get", "x", "comment~^a$"]),
        ),
        (true, true, true),
    );
}

/// `comment <hid> words…` is `update <hid> --strategy append -C words…`: the
/// words are one comment, appended, on a timed entry or a timeless one, and
/// each earlier version stays in the history (da-7sc0u).
#[test]
fn comment_appends_one_comment_to_an_existing_entry() {
    let db = fresh_db();
    run_cli(&["occur", "x"], &db).unwrap();
    run_cli(&["note", "y", "a", "thought"], &db).unwrap();
    for field in ["x", "y"] {
        let hid = logged_entries(&db, field)[0].hid.as_str().to_string();
        run_cli(&["comment", &hid, "looks", "fine"], &db).unwrap();
        run_cli(&["comment", &hid, "a", "second"], &db).unwrap();
    }
    assert_eq!(comments(&db, "x"), [json!(["looks fine", "a second"])]);
    assert_eq!(
        comments(&db, "y"),
        [json!(["a thought", "looks fine", "a second"])]
    );

    let e = &logged_entries(&db, "x")[0];
    let at = |v| db.entry_at_version(e, v).unwrap().unwrap().data;
    assert_eq!(
        (at(1), at(2)),
        (None, Some(json!({"comment": ["looks fine"]}))),
        "each earlier version is in the history"
    );

    run_cli(
        &[
            "update",
            e.hid.as_str(),
            "--strategy",
            "append",
            "-C",
            "same",
            "thing",
        ],
        &db,
    )
    .unwrap();
    assert_eq!(
        comments(&db, "x"),
        [json!(["looks fine", "a second", "same thing"])]
    );
}

#[test]
fn comment_refuses_an_unknown_hid_a_strategy_and_no_words() {
    let db = fresh_db();
    run_cli(&["occur", "x"], &db).unwrap();
    let hid = logged_entries(&db, "x")[0].hid.as_str().to_string();
    let err = |args: &[&str]| run_cli(args, &db).unwrap_err().to_string();
    assert_eq!(
        (
            err(&["comment", "zzzzzz", "nope"]),
            err(&["comment", &hid, "--strategy", "useNew", "nope"]),
            err(&["comment", &hid]),
        ),
        (
            "Entry not found: zzzzzz".to_string(),
            "Invalid argument: comment takes no --strategy: it is update --strategy append -C"
                .to_string(),
            "Invalid argument: comment takes the words of the comment: comment <hid> words…"
                .to_string(),
        )
    );
    assert_eq!(comments(&db, "x"), [Value::Null]);
}

/// A bare `--` ends the keys as well as the flags (da-gqy3b, vividn
/// 2026-10-26): every word after it is a positional as typed, so under
/// `-C` a URL, an `x=y` or a `-t` is the comment's text, typed directly, in
/// an alias's words, or in `comment <hid>`'s. A line without `--` keeps
/// its keys, and a word after `--` still fills a slot.
#[test]
fn words_after_a_bare_double_dash_are_as_typed() {
    let db = fresh_db();
    let data = |field: &str| logged_entries(&db, field)[0].data.clone();
    let sentence = "see https://example.com/?a=b and dose=5 -t now";
    let words: Vec<&str> = sentence.split(' ').collect();
    run_cli(&[&["occur", "x", "-C", "--"][..], &words].concat(), &db).unwrap();
    assert_eq!(data("x"), Some(json!({"comment": [sentence]})));

    run_cli(&["alias", "set", "jot", "occur jotting -C"], &db).unwrap();
    run_cli(&[&["jot", "--"][..], &words].concat(), &db).unwrap();
    assert_eq!(data("jotting"), Some(json!({"comment": [sentence]})));
    let hid = logged_entries(&db, "jotting")[0].hid.as_str().to_string();
    run_cli(&["comment", &hid, "--", "a=b", "-c"], &db).unwrap();
    assert_eq!(comments(&db, "jotting"), [json!([sentence, "a=b -c"])]);

    run_cli(&["occur", "y", "-C", "say", "dose=5", "--", "x=y"], &db).unwrap();
    assert_eq!(data("y"), Some(json!({"comment": ["say x=y"], "dose": 5})));

    run_cli(&["switch", "temp", "--", "-5"], &db).unwrap();
    run_cli(&["switch", "mood", "--", "end"], &db).unwrap();
    let state = |field: &str| logged_entries(&db, field)[0].state.clone();
    assert_eq!(
        (state("temp"), state("mood")),
        (Some(json!(-5)), Some(json!("end")))
    );
}
