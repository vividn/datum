//! Parity tests for `datum update` — da-vf3c2.
//!
//! Ports the 11 cases of `src/commands/__test__/updateCmd.test.ts` in the
//! original datum (`reference/datum-v2`, c8599d84), checked against its
//! `updateCmd.ts`, `documentControl/updateDoc.ts` and `combineData.ts`. One
//! Rust test per TS case, each headed with the case it mirrors. The renames
//! `_id` → slug, `data.<key>` → a data key, `humanId` → hid, `value` → `state`
//! are applied, not reported.
//!
//! Divergences from datum:
//!   * daydumb has no data-only docs (a payload with no `data`/`meta`): every
//!     entry carries a field. TS #2 is ported onto an ordinary entry.
//!   * `--strategy update` is daydumb's, not datum's row of the same name;
//!     `merge` is datum's (da-scdm3), and useOld, useNew, preferOld,
//!     preferNew and append are their `combineData` rows too
//!     (`datum_parse::combine`, da-0id4s), and the other six are refused
//!     (da-zd0vx). TS #3's `xor` is one of them.
//!   * `update` is a SHALLOW overlay: a payload object replaces the stored
//!     one, where datum's `update` strategy recursed into objects present on
//!     both sides. Chosen in c4050ccc (da-gd1j, "update: shallow overlay");
//!     `merge` is the deep one. TS #4 is asserted on nested data so it shows.
//!   * A default `_LAST` past fifteen minutes PROMPTS `[Y/n]` rather than
//!     throwing `LastDocsTooOldError` (da-uzxj). TS #11 declines the prompt
//!     and asserts nothing was written.
//!   * TS #3 and #4 spied on `quickId` / `updateDoc` with jest mocks. There
//!     is no seam to mock here, so each asserts what the spy stood for: the
//!     row the id resolved to carries the payload under the strategy.
//!
//! Two were bugs, fixed with this port (da-vf3c2):
//!   * TS #5: an update that changed nothing printed `UPDATE` and wrote a
//!     history version; it now prints `NODIFF` and writes nothing, as
//!     `updateDoc.ts` did.
//!   * TS #2: a positional filling a `-k KEY=` slot was overwritten by the
//!     empty `KEY=` drop sentinel, storing `""`.

use crate::common;

use chrono::{Duration, Utc};
use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::{fresh_db, logged_all};
use serde_json::{json, Value};
use std::process::{Command, Output, Stdio};

/// The stored `data` of the row `id` names.
fn data(db: &Db, id: &str) -> Value {
    db.get_entry(id).unwrap().data.unwrap_or(Value::Null)
}

/// Versions in the row's history trail.
fn versions(db: &Db, id: &str) -> i64 {
    let hid = db.get_entry(id).unwrap().hid.0;
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM entry_history WHERE entry_hid = ?1",
            [hid],
            |r| r.get(0),
        )
        .unwrap()
}

/// `datum <args>` as a process on `db_path`, with `input` on stdin.
fn run_bin(db_path: &std::path::Path, args: &[&str], input: &str) -> Output {
    use std::io::Write;
    let home = db_path.with_extension("home");
    let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db_path)
        .env("DATUM_HOME", &home)
        .env("DATUM_CONFIG_HOME", &home)
        .env("NO_COLOR", "1")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn datum");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait datum")
}

// ─── TS #1: "can update an existing doc from the first few letters of its humanId" ─

#[test]
fn updates_a_row_from_a_prefix_of_its_hid() {
    let db = fresh_db();
    run_cli(&["note", "doc", "foo=bar", "--id", "doc_to_update"], &db).unwrap();
    let hid = logged_all(&db)[0].hid.0.clone();
    run_cli(
        &[
            "update",
            &hid[..3],
            "--strategy",
            "update",
            "foo=baz",
            "newField=newData",
        ],
        &db,
    )
    .unwrap();
    assert_eq!(
        data(&db, "doc:doc_to_update"),
        json!({"foo": "baz", "newField": "newData"})
    );
}

// ─── TS #2: "can update a dataonly doc from the first letters of its id" ──────

/// Adapted: no data-only docs. `merge` collects both values, as in datum
/// (da-scdm3), and the `-k` slots are filled by the positionals.
#[test]
fn merge_fills_declared_key_slots_from_positionals() {
    let db = fresh_db();
    run_cli(&["note", "d", "foo=bar", "--id", "some_data_only"], &db).unwrap();
    run_cli(
        &[
            "update",
            "d:some_data_only",
            "--strategy",
            "merge",
            "-k",
            "foo",
            "-k",
            "newField=",
            "baz",
            "newData",
        ],
        &db,
    )
    .unwrap();
    assert_eq!(
        data(&db, "d:some_data_only"),
        json!({"foo": ["bar", "baz"], "newField": "newData"})
    );
}

// ─── TS #3: "calls quickId and updateDoc" ─────────────────────────────────────

/// Adapted: the spies become the row they stood for — the quick id resolves,
/// and the named strategy (not the default) is what combines the payload.
/// datum's `xor` is refused (da-zd0vx), so `append` stands in for it.
#[test]
fn the_named_strategy_combines_the_payload_into_the_resolved_row() {
    let db = fresh_db();
    run_cli(
        &[
            "note",
            "n",
            r#"o={"a":0}"#,
            "foo=baz",
            "--id",
            "input_quick",
        ],
        &db,
    )
    .unwrap();
    let err = run_cli(
        &["update", "n:input_quick", "--strategy", "xor", "foo=bar"],
        &db,
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("invalid --strategy value: 'xor'"),
        "{err:#}"
    );
    run_cli(
        &["update", "n:input_quick", "--strategy", "append", "foo=bar"],
        &db,
    )
    .unwrap();
    assert_eq!(
        data(&db, "n:input_quick"),
        json!({"o": {"a": 0}, "foo": ["baz", "bar"]})
    );
}

/// Every combineData name kept works from the CLI, the stored data as A and
/// the payload as B (da-0id4s); the table itself is tested in datum-parse.
#[test]
fn every_combine_strategy_works_from_the_cli() {
    let db = fresh_db();
    for (i, (strategy, want)) in [
        ("useOld", json!({"a": 1, "both": "x"})),
        ("useNew", json!({"b": 2, "both": "y"})),
        ("preferOld", json!({"a": 1, "b": 2, "both": "x"})),
        ("preferNew", json!({"a": 1, "b": 2, "both": "y"})),
        ("append", json!({"a": 1, "b": 2, "both": ["x", "y"]})),
        ("merge", json!({"a": 1, "b": 2, "both": ["x", "y"]})),
    ]
    .into_iter()
    .enumerate()
    {
        // A field each: the same content twice is one entry (EXISTS).
        let (field, slug) = (format!("c{i}"), format!("c{i}:x"));
        run_cli(&["note", &field, "a=1", "both=x", "--id", "x"], &db).unwrap();
        run_cli(
            &["update", &slug, "--strategy", strategy, "b=2", "both=y"],
            &db,
        )
        .unwrap();
        assert_eq!(data(&db, &slug), want, "{strategy}");
    }
}

/// `datum merge` is datum's merge (da-scdm3): a key both hold collects both
/// values into a list, and a value already in it is not added again.
#[test]
fn merge_collects_values_without_duplicates() {
    let db = fresh_db();
    run_cli(&["note", "fruit", "item=apple", "--id", "x"], &db).unwrap();
    run_cli(&["merge", "fruit:x", "item=pear"], &db).unwrap();
    assert_eq!(data(&db, "fruit:x")["item"], json!(["apple", "pear"]));
    run_cli(&["merge", "fruit:x", "item=pear"], &db).unwrap();
    assert_eq!(data(&db, "fruit:x")["item"], json!(["apple", "pear"]));

    // The deep merge where the new value wins is preferNew.
    run_cli(&["note", "veg", "item=leek", "--id", "x"], &db).unwrap();
    run_cli(
        &["update", "veg:x", "--strategy", "preferNew", "item=kale"],
        &db,
    )
    .unwrap();
    assert_eq!(data(&db, "veg:x")["item"], json!("kale"));
}

/// A comment appends (da-ult68) by the row's own going first in the payload;
/// a strategy that joins the two arrays itself must not get them twice.
#[test]
fn a_joining_strategy_takes_each_comment_once() {
    let db = fresh_db();
    for (i, (strategy, want)) in [
        ("append", json!(["c1", "c2"])),
        ("preferNew", json!(["c1", "c2"])),
    ]
    .into_iter()
    .enumerate()
    {
        let (field, slug) = (format!("k{i}"), format!("k{i}:x"));
        run_cli(&["note", &field, "-c", "c1", "--id", "x"], &db).unwrap();
        run_cli(&["update", &slug, "--strategy", strategy, "-c", "c2"], &db).unwrap();
        assert_eq!(data(&db, &slug)["comment"], want, "{strategy}");
    }
}

/// An entry with no data is an empty A, not a reason to skip the strategy:
/// `useOld` keeps nothing of the payload.
#[test]
fn a_strategy_applies_to_an_entry_with_no_data() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).unwrap();
    let hid = logged_all(&db)[0].hid.0.clone();
    run_cli(&["update", &hid, "--strategy", "useOld", "b=2"], &db).unwrap();
    assert_eq!(data(&db, &hid), Value::Null);
    run_cli(&["update", &hid, "--strategy", "preferOld", "b=2"], &db).unwrap();
    assert_eq!(data(&db, &hid), json!({"b": 2}));
}

// ─── TS #4: "uses update as the default updateStrategy" ───────────────────────

/// A bare `update` does what `--strategy update` does — which is shallow on a
/// nested object (da-gd1j; datum's recursed, see the header).
#[test]
fn update_is_the_default_strategy() {
    let db = fresh_db();
    // Two fields, as one row's twin would be the same content (EXISTS).
    for field in ["bare", "named"] {
        run_cli(&["note", field, r#"o={"a":0,"b":2}"#, "--id", "x"], &db).unwrap();
    }
    run_cli(&["update", "bare:x", r#"o={"a":1}"#], &db).unwrap();
    run_cli(
        &["update", "named:x", "--strategy", "update", r#"o={"a":1}"#],
        &db,
    )
    .unwrap();
    assert_eq!(data(&db, "bare:x"), data(&db, "named:x"));
    assert_eq!(data(&db, "bare:x"), json!({"o": {"a": 1}}));
}

// ─── TS #5: "outputs an UPDATE message or a NODIFF message when show is standard" ─

/// Was a bug (fixed here): the second update printed `UPDATE` and wrote a
/// version. An update that changes nothing writes nothing.
#[test]
fn prints_update_then_nodiff_and_a_nodiff_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("u.db");
    let run = |args: &[&str]| {
        let out = run_bin(&db_path, args, "");
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    run(&["note", "zzz", "foo=bar", "--id", "zzz"]);
    let first = run(&["update", "zzz:zzz", "foo=baz", "--show", "standard"]);
    assert!(first.contains("UPDATE"), "got: {first}");
    let second = run(&["update", "zzz:zzz", "foo=baz", "--show", "standard"]);
    assert!(second.contains("NODIFF"), "got: {second}");
    assert!(!second.contains("UPDATE"), "got: {second}");

    let db = Db::open(db_path.to_str().unwrap()).unwrap();
    assert_eq!(versions(&db, "zzz:zzz"), 2, "create + the one real update");
}

// ─── TS #6: "can update multiple documents with a compound quickId" ───────────

#[test]
fn a_compound_quick_id_updates_every_row() {
    let db = fresh_db();
    run_cli(&["note", "f", "foo=bar", "bar=foo", "--id", "zzz"], &db).unwrap();
    run_cli(&["note", "f", "foo=bar", "bar=foo2", "--id", "yyy"], &db).unwrap();
    run_cli(
        &["update", ",f:zzz,f:yyy", "foo=baz", "newField=newData"],
        &db,
    )
    .unwrap();
    assert_eq!(
        data(&db, "f:zzz"),
        json!({"foo": "baz", "bar": "foo", "newField": "newData"})
    );
    assert_eq!(
        data(&db, "f:yyy"),
        json!({"foo": "baz", "bar": "foo2", "newField": "newData"})
    );
}

// ─── TS #7: "can take a data specification before the quick id argument by using an ='s notation" ─

#[test]
fn data_may_come_before_the_quick_id() {
    let db = fresh_db();
    run_cli(&["note", "doc", "foo=bar", "--id", "doc_to_update"], &db).unwrap();
    let hid = logged_all(&db)[0].hid.0.clone();
    run_cli(&["update", "foo=baz", &hid[..4], "another=thing"], &db).unwrap();
    let row = db.get_entry("doc:doc_to_update").unwrap();
    assert_eq!(row.hid.0, hid);
    assert_eq!(row.data.unwrap(), json!({"foo": "baz", "another": "thing"}));
}

// ─── TS #8: "can update a key with an undefined value easily" ─────────────────

#[test]
fn an_empty_value_removes_the_key() {
    let db = fresh_db();
    run_cli(
        &["note", "field", "foo=bar", "baz=qux", "--id", "foobar"],
        &db,
    )
    .unwrap();
    run_cli(&["update", "field:foobar", "foo="], &db).unwrap();
    let row = db.get_entry("field:foobar").unwrap();
    assert_eq!(row.field, "field");
    assert_eq!(row.data.unwrap(), json!({"baz": "qux"}));
}

// ─── TS #9: "can update the last added document when no quickId is provided" ──

/// With no quick id the target is `_LAST`, and a templated slug follows the
/// data it is built from.
#[test]
fn no_quick_id_updates_the_last_row_and_reslugs_it() {
    let db = fresh_db();
    run_cli(&["note", "field", "foo=bar", "--id", "%foo"], &db).unwrap();
    let hid = logged_all(&db)[0].hid.0.clone();
    run_cli(&["update", "foo=baz"], &db).unwrap();
    let row = db.get_entry("field:baz").unwrap();
    assert_eq!(row.hid.0, hid, "the same row, under its new slug");
    assert_eq!(row.field, "field");
    assert_eq!(row.data.unwrap(), json!({"foo": "baz"}));
    assert!(db.get_entry("field:bar").is_err(), "the old slug is gone");
}

// ─── TS #10: "can update all the of documents returned by getCmd at once" ─────

#[test]
fn updates_every_row_the_last_get_returned() {
    let db = fresh_db();
    run_cli(&["note", "field", "foo=bar"], &db).unwrap();
    run_cli(&["note", "field", "bar=foo"], &db).unwrap();
    let rows = logged_all(&db);
    let (hid1, hid2) = (rows[0].hid.0.clone(), rows[1].hid.0.clone());
    run_cli(&["get", &format!("{hid1},{hid2},")], &db).unwrap();
    run_cli(&["update", "foo=baz"], &db).unwrap();
    let got = |h: &str| db.get_entry(h).unwrap().data.unwrap();
    let (d1, d2) = (got(&hid1), got(&hid2));
    let (bar_row, foo_row) = if d1.get("bar").is_some() {
        (d1, d2)
    } else {
        (d2, d1)
    };
    assert_eq!(foo_row, json!({"foo": "baz"}));
    assert_eq!(bar_row, json!({"bar": "foo", "foo": "baz"}));
}

// ─── TS #11: "won't update the last doc if the ref is more than 15 minutes old" ─

/// Adapted: a stale default `_LAST` asks `[Y/n]` instead of throwing
/// (da-uzxj). Declined, nothing is written — which is what the throw
/// protected.
#[test]
fn a_stale_last_asks_first_and_a_no_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("u.db");
    assert!(
        run_bin(&db_path, &["note", "field", "foo=bar", "--id", "%foo"], "")
            .status
            .success()
    );
    Db::open(db_path.to_str().unwrap())
        .unwrap()
        .conn()
        .execute(
            "UPDATE last_docs SET time = ?1",
            [(Utc::now() - Duration::minutes(16)).to_rfc3339()],
        )
        .unwrap();

    let out = run_bin(&db_path, &["update", "foo=baz"], "n\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[Y/n]"), "a stale _LAST must ask: {stderr}");

    let db = Db::open(db_path.to_str().unwrap()).unwrap();
    assert_eq!(data(&db, "field:bar"), json!({"foo": "bar"}));
    assert!(db.get_entry("field:baz").is_err());
}

// ─── rekey: `utils/rekey.ts`, through the same `updateDoc.ts` (da-wh822) ──────

/// `rekey` renames a data key on the one row. A key the id template names,
/// renamed away, empties that part of the slug, and the receipt is
/// `updateDoc.ts`'s for an id that moved: `RENAME`, then `UPDATE` of the
/// same hid.
#[test]
fn rekey_renames_a_data_key_in_place_and_prints_rename_then_update() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("u.db");
    let run = |args: &[&str]| {
        let out = run_bin(&db_path, args, "");
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let created = run(&["add", "met", "who=ben", "--id", "%who%"]);
    let hid = created.split(' ').nth(1).unwrap();
    let renamed = run(&["rekey", "met:ben", "who=eve"]);
    assert!(
        renamed.starts_with(&format!("RENAME met:ben ⟶ met:\nUPDATE {hid} met:\n")),
        "got: {renamed}"
    );

    let db = Db::open(db_path.to_str().unwrap()).unwrap();
    let rows = logged_all(&db);
    assert_eq!(rows.len(), 1, "no copy, no old row");
    assert_eq!(rows[0].slug, "met:");
    assert_eq!(rows[0].data, Some(json!({"eve": "ben"})));
}
