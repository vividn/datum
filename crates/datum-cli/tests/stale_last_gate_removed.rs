//! da-s6no: the fifteen-minute gate on a stale `_LAST` is gone from `undo`
//! and from `delete`, and this file keeps it gone.
//!
//! The gate had several halves. Bare `undo` refused a `_LAST` pointer older
//! than fifteen minutes — prompting on a tty, refusing outright without one,
//! which is every script and the whole web app — and its tombstone fallback
//! looked only inside `--window`. Bare `delete` prompted on the same clock.
//! All of it existed because a mutation was the last word on an entry. It is
//! not: the history trail keeps every version, `undo` steps back and `redo`
//! steps forward. vividn, 2026-09-15: "let's remove the 15 minute gate on undo
//! since redo is possible; still put a warning though after 10 minutes", and
//! at 20:19 "you can remove the delete protection, since undo is so easy, but
//! do give the warning".
//!
//! What is left is a warning on stderr past ten minutes. If a test here failed
//! after someone restored a refusal, the answer is not to relax the test — it
//! is that these commands are reversible and so have nothing to protect the
//! user from.
//!
//! `update` and `retime` keep their own prompt: vividn named `undo` and `delete`
//! only. `stale_update_still_has_its_own_prompt` below is what says so.

mod common;

use chrono::{Duration, Utc};
use common::run_cli;
use datum_core::test_util::fresh_db;

/// Age the `_LAST` pointer by `minutes`, as if the user had walked away.
fn age_pointer(db: &datum_core::db::Db, minutes: i64) {
    db.conn()
        .execute(
            "UPDATE last_docs SET time = ?1",
            rusqlite::params![(Utc::now() - Duration::minutes(minutes)).to_rfc3339()],
        )
        .unwrap();
}

/// `undo --window <DUR>` still fails with the tombstone, which names what
/// replaced the gate, rather than with a bare "unknown flag" a reader takes
/// for an oversight. It took a duration that meant something, so silently
/// ignoring it would change what the script asked for.
#[test]
fn undos_window_is_rejected_with_a_message_that_teaches() {
    for argv in [
        vec!["undo", "--window", "15m"],
        vec!["undo", "--window=15m"],
    ] {
        let db = fresh_db();
        run_cli(&["occur", "coffee"], &db).expect("create");
        let err = run_cli(&argv, &db)
            .map(|_| String::new())
            .unwrap_or_else(|e| format!("{e:#}"));
        assert!(
            err.contains("was removed from `undo`"),
            "`datum {}` must fail with the tombstone, got: {err}",
            argv.join(" ")
        );
        assert!(
            err.contains("redo"),
            "the tombstone must name what replaced the gate, got: {err}"
        );
    }
}

/// da-8uig, vividn: "undo --force should also just be a no-op" — the twin of
/// `delete_still_accepts_force_even_with_nothing_to_skip` below. A bare
/// switch carries no meaning past "don't stop to ask me", and nothing has
/// asked since da-s6no, so it is accepted and the undo proceeds, stale
/// `_LAST` and all. (That it also still WARNS past ten minutes is
/// `subprocess_undo_force_is_inert_and_still_warns` in `e2e_subprocess.rs`,
/// where stderr is observable.)
#[test]
fn undo_still_accepts_force_even_with_nothing_to_skip() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "cups=2"], &db).expect("create");
    run_cli(&["update", "_", "cups=3"], &db).expect("update");
    age_pointer(&db, 90);

    run_cli(&["undo", "--force"], &db).expect("--force is still a valid flag");

    let entries = db.list_including_deleted().unwrap();
    assert_eq!(
        entries[0].data.as_ref().unwrap()["cups"],
        2,
        "`--force` is inert, so the undo happened exactly as a bare one would"
    );
}

/// The behaviour those flags used to guard: a `_LAST` well past fifteen
/// minutes no longer stops bare `undo`. It proceeds, and the entry moves.
#[test]
fn a_stale_last_no_longer_refuses_undo() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "cups=2"], &db).expect("create");
    run_cli(&["update", "_", "cups=3"], &db).expect("update");
    age_pointer(&db, 90);

    run_cli(&["undo"], &db).expect("a stale `_LAST` must not refuse");

    let entries = db.list_including_deleted().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].data.as_ref().unwrap()["cups"],
        2,
        "the undo actually happened"
    );
}

/// A tombstone older than any window is reachable too — the fallback path had
/// its own half of the gate, and it went with the rest.
#[test]
fn an_old_tombstone_is_reachable_without_a_flag() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("create");
    let hid = db.list_including_deleted().unwrap()[0].hid.to_string();
    run_cli(&["delete", &hid], &db).expect("delete");

    // Age the tombstone AND clear the pointer, so the fallback is the only
    // path left — exactly the shape the old `--window` refusal guarded.
    db.conn()
        .execute(
            "UPDATE entries SET deleted_at = ?1",
            rusqlite::params![(Utc::now() - Duration::hours(30)).to_rfc3339()],
        )
        .unwrap();
    db.conn().execute("DELETE FROM last_docs", []).unwrap();

    run_cli(&["undo"], &db).expect("an old tombstone must still come back");
    assert!(db.get_entry(&hid).unwrap().deleted_at.is_none());
}

/// With nothing to act on at all, `undo` still fails — the gate went, the
/// honest "there is nothing here" did not.
#[test]
fn an_empty_database_still_has_nothing_to_undo() {
    let db = fresh_db();
    let err = run_cli(&["undo"], &db).expect_err("nothing has ever been written");
    assert!(err.to_string().contains("Nothing to undo"), "got: {err}");
}

/// vividn at 20:19: delete's own protection goes the same way. Bare `delete`
/// on a stale `_LAST` tombstones the entry instead of prompting — and with no
/// `--force`, which is what proves the prompt is gone rather than skipped.
#[test]
fn a_stale_last_no_longer_refuses_delete() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("create");
    let hid = db.list_including_deleted().unwrap()[0].hid.to_string();
    age_pointer(&db, 90);

    run_cli(&["delete"], &db).expect("a stale `_LAST` must not refuse");

    assert!(
        db.get_entry_including_deleted(&hid)
            .unwrap()
            .deleted_at
            .is_some(),
        "the delete actually happened"
    );
    // And it is reversible, which is the whole reason the gate could go.
    run_cli(&["undo"], &db).expect("undo brings it back");
    assert!(db.get_entry(&hid).unwrap().deleted_at.is_none());
}

/// `--force` on `delete` is still ACCEPTED — scripts carry it everywhere —
/// it simply has no prompt left to skip.
#[test]
fn delete_still_accepts_force_even_with_nothing_to_skip() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("create");
    age_pointer(&db, 90);
    run_cli(&["delete", "--force"], &db).expect("--force is still a valid flag");
    assert_eq!(db.count_entries().unwrap(), 0);
}

/// The boundary that is left. vividn named `undo` and `delete`; `update` keeps
/// the prompt it had, so a stale `_LAST` there still reaches the confirmation
/// path rather than silently rewriting an entry from an hour ago. In a test
/// there is no tty, so the yes-default carries it through — what this asserts
/// is that the code path still exists to be answered, not that it refuses.
#[test]
fn stale_update_still_has_its_own_prompt() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "cups=2"], &db).expect("create");
    age_pointer(&db, 90);
    run_cli(&["update", "_", "cups=3"], &db).expect("update answers its own prompt");
    let entries = db.list_including_deleted().unwrap();
    assert_eq!(entries[0].data.as_ref().unwrap()["cups"], 3);
}
