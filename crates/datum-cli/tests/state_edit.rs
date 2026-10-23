//! da-9eh7d: `switch F +=S` adds a state to the set the field is in, `-=S`
//! takes one out, and `_0` names the set in the state switched to. The set
//! is read at the entry's own time, in its write, and recorded as its `-l`.

use std::process::Command;

use crate::common::run_cli;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::{json, Value};

fn newest_entry(db: &Db, field: &str) -> Entry {
    logged_entries(db, field)
        .into_iter()
        .max_by_key(|e| e.occur_time.as_ref().map(|t| t.utc))
        .expect("an entry")
}

/// The field's newest entry: its state and the set it asserted.
fn newest(db: &Db, field: &str) -> (Value, Option<Value>) {
    let e = newest_entry(db, field);
    (e.state.unwrap(), e.user_asserted_last_state)
}

fn refused(db: &Db, line: &[&str]) -> String {
    format!("{:#}", run_cli(line, db).expect_err("refused"))
}

#[test]
fn an_add_from_off_from_one_state_and_from_a_set() {
    let db = fresh_db();
    run_cli(&["switch", "x", "+=queued", "-t", "-10"], &db).unwrap();
    // No state, recorded as `-l null` records it.
    assert_eq!(newest(&db, "x"), (json!("queued"), Some(json!("null"))));
    run_cli(&["switch", "x", "+=parked", "-t", "-9"], &db).unwrap();
    assert_eq!(
        newest(&db, "x"),
        (json!(["queued", "parked"]), Some(json!("queued")))
    );
    run_cli(&["switch", "x", "+=late", "-t", "-8"], &db).unwrap();
    assert_eq!(newest(&db, "x").0, json!(["queued", "parked", "late"]));

    // `false` is off too, and the keys stay out of the data.
    run_cli(&["end", "x", "-t", "-7"], &db).unwrap();
    run_cli(&["switch", "x", "+=queued", "note=hi", "-t", "-6"], &db).unwrap();
    let e = newest_entry(&db, "x");
    assert_eq!(e.state, Some(json!("queued")));
    assert_eq!(e.data, Some(json!({"note": "hi"})));
    assert!(e.raw_input.unwrap().contains("+=queued"));
}

#[test]
fn an_add_of_a_held_state_repeats_the_set() {
    let db = fresh_db();
    run_cli(&["switch", "x", "queued,parked", "-t", "-5"], &db).unwrap();
    run_cli(&["switch", "x", "+=parked"], &db).unwrap();
    assert_eq!(newest(&db, "x").0, json!(["queued", "parked"]));
}

#[test]
fn a_removal_down_to_one_then_off_and_of_a_missing_state_refused() {
    let db = fresh_db();
    run_cli(&["switch", "x", "queued,parked", "-t", "-5"], &db).unwrap();
    let msg = refused(&db, &["switch", "x", "-=late", "-t", "-4"]);
    assert!(
        msg.contains(r#"-=late: x is in ["queued","parked"], so nothing is written"#),
        "{msg}"
    );
    assert_eq!(logged_entries(&db, "x").len(), 1, "nothing written");

    run_cli(&["switch", "x", "-=parked", "-t", "-3"], &db).unwrap();
    assert_eq!(
        newest(&db, "x"),
        (json!("queued"), Some(json!(["queued", "parked"])))
    );
    run_cli(&["switch", "x", "-=queued"], &db).unwrap();
    assert_eq!(newest(&db, "x").0, json!(false));
}

#[test]
fn zero_is_the_current_set_in_a_list() {
    let db = fresh_db();
    run_cli(&["switch", "x", "queued", "-t", "-5"], &db).unwrap();
    run_cli(&["switch", "x", "_0,parked", "-t", "-4"], &db).unwrap();
    assert_eq!(
        newest(&db, "x"),
        (json!(["queued", "parked"]), Some(json!("queued")))
    );
    run_cli(&["switch", "x", "_0"], &db).unwrap();
    assert_eq!(newest(&db, "x").0, json!(["queued", "parked"]));
}

#[test]
fn an_edit_claims_with_minus_big_l_and_refuses_minus_l() {
    let db = fresh_db();
    run_cli(&["switch", "x", "queued", "-t", "-5"], &db).unwrap();
    let msg = refused(&db, &["switch", "x", "+=parked", "-L", "late"]);
    assert!(msg.contains("-L late: x is in queued"), "{msg}");
    let msg = refused(&db, &["switch", "x", "+=parked", "-l", "queued"]);
    assert!(msg.contains("-l cannot assert others: drop -l"), "{msg}");
    run_cli(&["switch", "x", "+=parked", "-L", "queued"], &db).unwrap();
    assert_eq!(newest(&db, "x").0, json!(["queued", "parked"]));
}

#[test]
fn each_member_passes_only_listed_states() {
    let db = fresh_db();
    run_cli(
        &[
            "setup",
            "x",
            "--verb",
            "switch",
            "--states",
            "queued,parked",
            "--only-listed-states",
        ],
        &db,
    )
    .unwrap();
    run_cli(&["switch", "x", "queued", "-t", "-5"], &db).unwrap();
    let msg = refused(&db, &["switch", "x", "+=late"]);
    assert!(msg.contains("'late' is not one of"), "{msg}");
    run_cli(&["switch", "x", "+=parked"], &db).unwrap();
    assert_eq!(newest(&db, "x").0, json!(["queued", "parked"]));
}

#[test]
fn a_state_beside_an_edit_holds_zero() {
    let db = fresh_db();
    run_cli(&["switch", "x", "queued", "-t", "-5"], &db).unwrap();
    let msg = refused(&db, &["switch", "x", "parked", "+=late"]);
    assert!(msg.contains("names them with _0 (`_0,parked`)"), "{msg}");
    // `off` ends the field and `on` starts it, which name no states.
    for word in ["off", "on"] {
        let msg = refused(&db, &["switch", "x", word, "+=late"]);
        assert!(msg.contains("(`on`, `off`) does not name"), "{msg}");
    }
    run_cli(&["switch", "x", "_0,parked", "+=late", "-t", "-4"], &db).unwrap();
    assert_eq!(newest(&db, "x").0, json!(["queued", "parked", "late"]));
}

#[test]
fn start_and_end_take_no_edit() {
    let db = fresh_db();
    for line in [["start", "x", "+=a"], ["end", "x", "-=a"]] {
        let msg = refused(&db, &line);
        assert!(msg.contains("datum switch <field>"), "{msg}");
    }
    assert!(logged_entries(&db, "x").is_empty());
}

#[test]
fn undo_by_line_refuses_an_edit() {
    let db = fresh_db();
    run_cli(&["start", "x", "-t", "-5"], &db).unwrap();
    run_cli(&["switch", "x", "+=late"], &db).unwrap();
    let msg = refused(&db, &["switch", "x", "+=late", "-u"]);
    assert!(msg.contains("`datum undo` takes it back"), "{msg}");
    assert_eq!(logged_entries(&db, "x").len(), 2, "nothing deleted");
}

/// A field on with no state named is in the states it is given.
#[test]
fn an_add_from_on_names_the_state() {
    let db = fresh_db();
    run_cli(&["start", "x", "-t", "-5"], &db).unwrap();
    run_cli(&["switch", "x", "+=parked"], &db).unwrap();
    assert_eq!(newest(&db, "x"), (json!("parked"), Some(json!(true))));
}

/// A backdated edit changes the set its field was in at its time, and
/// lands though a later change follows it.
#[test]
fn a_backdated_edit_reads_its_own_time() {
    let db = fresh_db();
    run_cli(&["switch", "x", "queued", "-t", "-10"], &db).unwrap();
    run_cli(&["switch", "x", "parked", "-t", "-2"], &db).unwrap();
    run_cli(&["switch", "x", "+=late", "-t", "-5"], &db).unwrap();
    let edit = logged_entries(&db, "x")
        .into_iter()
        .find(|e| e.raw_input.as_deref().is_some_and(|r| r.contains("+=late")))
        .unwrap();
    assert_eq!(edit.state, Some(json!(["queued", "late"])));
    assert_eq!(edit.user_asserted_last_state, Some(json!("queued")));
}

fn spawn_env(tmp: &std::path::Path) -> impl Fn(&[&str]) -> Command + '_ {
    move |args: &[&str]| {
        let home = tmp.join("home");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
        cmd.env("DATUM_DB", tmp.join("edit.db"))
            .env("DATUM_HOME", &home)
            .env("DATUM_CONFIG_HOME", &home)
            .env("NO_COLOR", "1")
            .args(args);
        cmd
    }
}

/// The states `x` is in now, sorted.
fn held(db: &std::path::Path) -> Vec<String> {
    let state: String = rusqlite::Connection::open(db)
        .unwrap()
        .query_row(
            "SELECT state FROM entries WHERE field = 'x' AND deleted_at IS NULL
              ORDER BY occur_time DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut held: Vec<String> = match serde_json::from_str(&state).unwrap() {
        Value::Array(items) => items.iter().map(|v| v.as_str().unwrap().into()).collect(),
        one => vec![one.as_str().unwrap().into()],
    };
    held.sort();
    held
}

/// Writers adding at once lose no member: each reads the set where it
/// lands and, made at now, is placed when it lands.
#[test]
fn racing_adds_lose_no_member() {
    const PROCS: usize = 6;
    let tmp = tempfile::tempdir().unwrap();
    let datum = spawn_env(tmp.path());
    let ok = |args: &[&str]| assert!(datum(args).output().unwrap().status.success());
    ok(&["switch", "x", "queued", "-t", "-1"]);

    let barrier = std::sync::Barrier::new(PROCS);
    let landed: Vec<Option<String>> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..PROCS)
            .map(|p| {
                let (datum, barrier) = (&datum, &barrier);
                s.spawn(move || {
                    let member = format!("m{p}");
                    barrier.wait();
                    let out = datum(&["switch", "x", &format!("+={member}")])
                        .output()
                        .unwrap();
                    // Two writes of one field in one millisecond share a
                    // slug, whatever they say: that refusal is not the edit's.
                    let err = String::from_utf8_lossy(&out.stderr);
                    assert!(
                        out.status.success() || err.contains("Duplicate entry"),
                        "{err}"
                    );
                    out.status.success().then_some(member)
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    let mut landed: Vec<String> = landed.into_iter().flatten().collect();
    landed.push("queued".into());
    landed.sort();
    assert_eq!(held(&tmp.path().join("edit.db")), landed);
}

/// An edit is never queued behind a held lock: queued, it would land the
/// set it read before the lock, and two such lose a member.
#[test]
fn edits_wait_out_a_held_lock_and_lose_no_member() {
    let tmp = tempfile::tempdir().unwrap();
    let datum = spawn_env(tmp.path());
    let db = tmp.path().join("edit.db");
    assert!(datum(&["switch", "x", "queued", "-t", "-1"])
        .output()
        .unwrap()
        .status
        .success());

    let lock = rusqlite::Connection::open(&db).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    let racers: Vec<_> = ["+=a", "+=b"]
        .iter()
        .map(|add| {
            datum(&["switch", "x", add])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    // Past the 750 ms a write waits before it queues.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    lock.execute_batch("COMMIT").unwrap();
    for racer in racers {
        let out = racer.wait_with_output().unwrap();
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success() && !said.contains("QUEUED"), "{said}");
    }
    assert_eq!(held(&db), ["a", "b", "queued"]);
}
