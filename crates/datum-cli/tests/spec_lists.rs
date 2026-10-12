//! da-eqb30: «strict» split by what it does. A spec may take only the keys
//! it lists (`--only-listed-keys`, was `--strict`) and only the states it
//! lists (`--only-listed-states`); a never-false field with the second
//! cannot be ended. Leftover words go where the spec's `--remainder` says,
//! the per-field stand-in for the retired `-l`/`--lenient`; a batch that
//! changes nothing unless every id resolves is `--all-or-nothing`.

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::json;

fn spec(db: &Db, field: &str, args: &[&str]) {
    run_cli(&[&["spec", "set", field], args].concat(), db).unwrap();
}

fn refused(db: &Db, line: &[&str]) -> String {
    format!(
        "{:#}",
        run_cli(line, db).expect_err("the line should be refused")
    )
}

#[test]
fn only_listed_keys_refuses_an_undeclared_key() {
    let db = fresh_db();
    spec(
        &db,
        "coffee",
        &[
            "--verb",
            "occur",
            "--data-key",
            "cups",
            "--only-listed-keys",
        ],
    );
    let msg = refused(&db, &["occur", "coffee", "2", "brand=x"]);
    assert!(
        msg.contains("'brand'") && msg.contains("only_listed_keys"),
        "{msg}"
    );
    // A key the spec lists, by `--data-key` as well as a subfield, passes.
    run_cli(&["occur", "coffee", "2"], &db).unwrap();
    spec(&db, "coffee", &["--any-keys"]);
    run_cli(&["occur", "coffee", "2", "brand=x"], &db).unwrap();
    assert_eq!(logged_entries(&db, "coffee").len(), 2);
}

#[test]
fn only_listed_states_refuses_an_unlisted_state() {
    let db = fresh_db();
    spec(
        &db,
        "env",
        &["--verb", "switch", "--state", "home", "--state", "office"],
    );
    // Off by default: an unlisted state is logged, as before.
    run_cli(&["switch", "env", "cafe"], &db).unwrap();

    spec(&db, "env", &["--only-listed-states"]);
    for line in [
        &["switch", "env", "park"][..],
        &["switch", "env", "home,park"],
        &["start", "env"],
    ] {
        let msg = refused(&db, line);
        assert!(msg.contains("only_listed_states"), "{line:?}: {msg}");
    }
    run_cli(&["switch", "env", "office"], &db).unwrap();
    // Not never-false: off stays allowed.
    run_cli(&["end", "env"], &db).unwrap();
    // A later `spec set` keeps it; `--any-states` clears it.
    spec(&db, "env", &["--state", "home"]);
    assert!(db.load_spec("env").unwrap().unwrap().only_listed_states);
    spec(&db, "env", &["--any-states"]);
    run_cli(&["switch", "env", "park"], &db).unwrap();
}

#[test]
fn a_never_false_field_with_only_listed_states_cannot_be_ended() {
    let db = fresh_db();
    spec(
        &db,
        "env",
        &["--verb", "switch", "--state", "home", "--never-false"],
    );
    run_cli(&["switch", "env", "home"], &db).unwrap();
    // never_false alone is the ring's: `end` is still taken.
    run_cli(&["end", "env"], &db).unwrap();

    spec(&db, "env", &["--only-listed-states"]);
    for line in [
        &["end", "env"][..],
        &["switch", "env", "false"],
        &["switch", "env", "home", "5", ",", "switch", "env", "false"],
    ] {
        let msg = refused(&db, line);
        assert!(
            msg.contains("never_false") && msg.contains("only_listed_states"),
            "{line:?}: {msg}"
        );
    }
    // Listing `false` keeps the field's end.
    spec(&db, "env", &["--state", "home", "--state", "false"]);
    run_cli(&["end", "env"], &db).unwrap();
}

#[test]
fn a_spec_remainder_takes_the_leftover_words() {
    let db = fresh_db();
    spec(&db, "tea", &["--verb", "occur"]);
    let msg = refused(&db, &["occur", "tea", "green", "leaf"]);
    assert!(msg.contains("green"), "{msg}");

    spec(&db, "tea", &["--remainder", "extra_data"]);
    run_cli(&["occur", "tea", "green", "leaf"], &db).unwrap();
    // Through the field's own alias, and joined by a bare `-S`.
    run_cli(&["tea", "oolong"], &db).unwrap();
    run_cli(&["occur", "tea", "-S", "white", "peony"], &db).unwrap();
    // The remainder is a key the spec lists.
    spec(&db, "tea", &["--only-listed-keys"]);
    run_cli(&["tea", "oolong"], &db).unwrap();
    spec(&db, "tea", &["--any-keys"]);
    // The line's own `-R` wins.
    run_cli(&["occur", "tea", "-R", "kind", "sencha"], &db).unwrap();
    // Newest first; one word is stored as itself, two as a list.
    let data: Vec<_> = logged_entries(&db, "tea")
        .into_iter()
        .map(|e| e.data.unwrap())
        .collect();
    assert_eq!(
        data,
        [
            json!({"kind": "sencha"}),
            json!({"extra_data": "oolong"}),
            json!({"extra_data": "white peony"}),
            json!({"extra_data": "oolong"}),
            json!({"extra_data": ["green", "leaf"]}),
        ]
    );

    // `reparse` replays the remainder the entry was logged with.
    let hid = logged_entries(&db, "tea")[4].hid.to_string();
    spec(&db, "tea", &["--remainder", ""]);
    assert_eq!(db.load_spec("tea").unwrap().unwrap().remainder, None);
    run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
    assert_eq!(
        db.get_entry(&hid).unwrap().data,
        Some(json!({"extra_data": ["green", "leaf"]}))
    );
    refused(&db, &["occur", "tea", "green", "leaf"]);
}

#[test]
fn the_old_spellings_are_unknown_flags() {
    let db = fresh_db();
    run_cli(&["occur", "tea"], &db).unwrap();
    for (line, flag) in [
        (&["occur", "tea", "-l", "x"][..], "-l"),
        (&["occur", "tea", "--lenient", "x"], "--lenient"),
        (&["delete", "tea:_", "--strict"], "--strict"),
        (&["update", "tea:_", "a=1", "--strict"], "--strict"),
        (&["spec", "set", "tea", "--strict"], "--strict"),
        (&["spec", "set", "tea", "--no-strict"], "--no-strict"),
    ] {
        let msg = refused(&db, line);
        assert!(
            msg.contains("unexpected flag") && msg.contains(flag),
            "{line:?}: {msg}"
        );
    }
}

#[test]
fn all_or_nothing_deletes_nothing_when_one_id_fails() {
    let db = fresh_db();
    run_cli(&["occur", "tea"], &db).unwrap();
    let hid = logged_entries(&db, "tea")[0].hid.to_string();
    refused(
        &db,
        &["delete", &format!("{hid},nosuchid"), "--all-or-nothing"],
    );
    assert_eq!(logged_entries(&db, "tea").len(), 1);
}

/// The remainder may be any key: a bare `-S` joins into it, and under
/// `--only-listed-keys` it is a key the spec names, typed or not.
#[test]
fn a_remainder_other_than_extra_data() {
    let db = fresh_db();
    spec(
        &db,
        "tea",
        &[
            "--verb",
            "occur",
            "--remainder",
            "notes",
            "--only-listed-keys",
        ],
    );
    run_cli(&["occur", "tea", "-S", "white", "peony"], &db).unwrap();
    run_cli(&["occur", "tea", "notes=hello"], &db).unwrap();
    let msg = refused(&db, &["occur", "tea", "brand=x"]);
    assert!(msg.contains("'brand'"), "{msg}");
    let data: Vec<_> = logged_entries(&db, "tea")
        .into_iter()
        .map(|e| e.data.unwrap())
        .collect();
    assert_eq!(
        data,
        [json!({"notes": "hello"}), json!({"notes": "white peony"})]
    );
}
