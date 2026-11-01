//! `a,b` on the reading commands, and `stats a,b`'s pooled row (da-oqpsp).

use std::path::{Path, PathBuf};
use std::process::Command;

use datum_core::db::Db;
use datum_core::entry::Verb;
use datum_core::test_util::entry_builder;
use serde_json::{json, Value};

fn datum(db: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .env("TZ", "UTC")
        .args(args)
        .output()
        .expect("spawn datum");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn ok(db: &Path, args: &[&str]) -> String {
    let (ok, out, err) = datum(db, args);
    assert!(ok, "{args:?}: {err}");
    out
}

fn json_of(db: &Path, args: &[&str]) -> Value {
    serde_json::from_str(&ok(db, args)).unwrap()
}

const DAY: &str = "2026-01-05";

/// The bead's own: `b` 09:00–10:00 and `c` 09:30–11:00, on a past day.
fn intervals(dir: &Path) -> PathBuf {
    let db = dir.join("t.db");
    ok(&db, &["start", "b", "1h", "-d", DAY, "-t", "10:00"]);
    ok(&db, &["start", "c", "1:30", "-d", DAY, "-t", "11:00"]);
    db
}

fn pooled_row(db: &Path, fields: &str) -> Value {
    json_of(db, &["stats", fields, "-d", DAY, "--json"])
}

#[test]
fn the_sum_is_2h30_and_the_union_2h() {
    let dir = tempfile::tempdir().unwrap();
    let db = intervals(dir.path());
    let got = pooled_row(&db, "b,c");
    assert_eq!(
        got["pooled"],
        json!({
            "fields": ["b", "c"],
            "count": 2,
            "sum_seconds": 9000,
            "union_seconds": 7200,
            "intersections": [{ "fields": ["b", "c"], "seconds": 1800 }],
            "rate": { "per": "day", "duration_seconds": 7200.0 },
        })
    );
    let rows = got["fields"].as_array().unwrap();
    assert_eq!(rows[0]["field"], "c");
    assert_eq!(rows[0]["duration_seconds"], 5400);
    assert_eq!(rows[1]["field"], "b");
    assert_eq!(rows[1]["duration_seconds"], 3600);

    let text = ok(&db, &["stats", "b,c", "-d", DAY]);
    assert!(text.starts_with("b,c  ·  2 entries\n"), "{text}");
    let last = text.lines().last().unwrap();
    assert!(last.contains("2h"), "{text}");
    assert!(last.contains("any of these"), "{text}");
    assert!(last.contains("(2h30m summed)"), "{text}");
    let row = text.lines().find(|l| l.contains("b & c")).unwrap();
    assert!(row.contains("30m"), "{text}");
}

/// Apart, the union is the sum: an overlap is not assumed.
#[test]
fn fields_that_never_overlap_sum_to_their_union() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    ok(&db, &["start", "b", "1h", "-d", DAY, "-t", "10:00"]);
    ok(&db, &["start", "c", "1h", "-d", DAY, "-t", "12:00"]);
    let p = &pooled_row(&db, "b,c")["pooled"];
    assert_eq!(p["sum_seconds"], 7200);
    assert_eq!(p["union_seconds"], 7200);
    assert_eq!(p["intersections"], json!([]));
    let text = ok(&db, &["stats", "b,c", "-d", DAY]);
    assert!(!text.contains(" & "), "{text}");
    assert!(text.contains("(no overlap)"), "{text}");
}

fn seconds_of(pooled: &Value, fields: &[&str]) -> Option<i64> {
    (pooled["intersections"].as_array().unwrap().iter())
        .find(|c| c["fields"] == json!(fields))
        .and_then(|c| c["seconds"].as_i64())
}

/// a 09:00–12:00, b 10:00–12:00, c 11:00–13:00, e 15:00–16:00. Each row is the
/// time all of it ran, so `a & b` holds the stretch of all three; `b` and `e`,
/// and the rest with `e`, never met.
#[test]
fn each_intersection_is_true_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    ok(&db, &["start", "a", "3h", "-d", DAY, "-t", "12:00"]);
    ok(&db, &["start", "b", "2h", "-d", DAY, "-t", "12:00"]);
    ok(&db, &["start", "c", "2h", "-d", DAY, "-t", "13:00"]);
    ok(&db, &["start", "e", "1h", "-d", DAY, "-t", "16:00"]);
    let got = pooled_row(&db, "a,b,c,e");
    let p = &got["pooled"];
    assert_eq!(seconds_of(p, &["a", "b"]), Some(7200));
    assert_eq!(seconds_of(p, &["a", "c"]), Some(3600));
    assert_eq!(seconds_of(p, &["b", "c"]), Some(3600));
    assert_eq!(seconds_of(p, &["a", "b", "c"]), Some(3600));
    assert_eq!(p["intersections"].as_array().unwrap().len(), 4, "{p}");
    assert_eq!(p["sum_seconds"], (3 + 2 + 2 + 1) * 3600);
    assert_eq!(p["union_seconds"], (4 + 1) * 3600);

    let text = ok(&db, &["stats", "a,b,c,e", "-d", DAY]);
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle}: {text}"))
    };
    assert!(
        at("a & b\n") < at("a & b & c\n"),
        "fewest fields first: {text}"
    );
    assert!(at("a & b & c\n") < at("any of these"), "{text}");
    assert!(text.contains("(8h summed)"), "{text}");
    assert!(
        !text.contains("& e"),
        "a combination that never happened: {text}"
    );
    assert!(!text.contains("overlapping"), "{text}");
}

/// The names keep the order typed, whatever the rows' ranking.
#[test]
fn a_combination_reads_in_the_order_typed() {
    let dir = tempfile::tempdir().unwrap();
    let db = intervals(dir.path());
    let got = pooled_row(&db, "c,b");
    assert_eq!(seconds_of(&got["pooled"], &["c", "b"]), Some(1800));
}

/// Rows double with each field running at once, so past the limit it refuses.
#[test]
fn too_many_at_once_is_refused_by_how_many() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    let names: Vec<String> = (0..17).map(|i| format!("f{i}x")).collect();
    for n in &names {
        ok(&db, &["start", n, "1h", "-d", DAY, "-t", "10:00"]);
    }
    let (ok_, _, err) = datum(&db, &["stats", &names.join(","), "-d", DAY]);
    assert!(!ok_);
    assert!(err.contains("17 of these ran at once"), "{err}");
    let fine = pooled_row(&db, &names[..16].join(","));
    assert_eq!(fine["pooled"]["union_seconds"], 3600);
}

/// The `false` between a switch's states is not doing it, so it joins no
/// union; `--include-false` puts it back, as it does in a row.
#[test]
fn the_union_counts_what_the_rows_count() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    ok(&db, &["switch", "m", "x", "-d", DAY, "-t", "09:00"]);
    ok(&db, &["switch", "m", "false", "-d", DAY, "-t", "10:00"]);
    ok(&db, &["start", "b", "1h", "-d", DAY, "-t", "12:00"]);
    let got = pooled_row(&db, "m,b");
    assert_eq!(got["pooled"]["union_seconds"], 7200);
    assert_eq!(got["pooled"]["sum_seconds"], 7200);
    let wide = json_of(
        &db,
        &["stats", "m,b", "-d", DAY, "--include-false", "--json"],
    );
    // `m` runs from 09:00 to the day's end, b from 11:00: 15h and 13h.
    assert_eq!(wide["pooled"]["sum_seconds"], 100800);
    assert_eq!(wide["pooled"]["union_seconds"], 54000);
    assert_eq!(seconds_of(&wide["pooled"], &["m", "b"]), Some(46800));
}

#[test]
fn a_field_with_no_entries_is_named_as_such() {
    let dir = tempfile::tempdir().unwrap();
    let db = intervals(dir.path());
    let got = pooled_row(&db, "b,nosuch");
    assert_eq!(got["pooled"]["count"], 1);
    assert_eq!(got["pooled"]["union_seconds"], 3600);
    let row = &got["fields"][1];
    assert_eq!(row["field"], "nosuch");
    assert_eq!(row["count"], 0);
    assert_eq!(row["ratio"], Value::Null);

    let text = ok(&db, &["stats", "b,nosuch", "-d", DAY]);
    let line = text.lines().find(|l| l.contains("nosuch  no")).unwrap();
    assert!(line.ends_with("nosuch  no entries"), "{text}");
    assert!(text.contains("(no overlap)"), "{text}");
}

/// Moments take no time: the pooled row says how often, as a row does.
#[test]
fn moments_pool_to_a_count() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    ok(&db, &["occur", "tea", "-d", DAY, "-t", "09:00"]);
    ok(&db, &["occur", "cof", "-d", DAY, "-t", "10:00"]);
    let p = &pooled_row(&db, "tea,cof")["pooled"];
    assert_eq!(p["count"], 2);
    assert_eq!(p["union_seconds"], 0);
    assert_eq!(p["rate"], json!({ "per": "day", "count": 2.0 }));
    let text = ok(&db, &["stats", "tea,cof", "-d", DAY]);
    let last = text.lines().last().unwrap();
    assert!(last.contains("2/day"), "{text}");
    assert!(last.ends_with("any of these"), "{text}");
}

#[test]
fn one_field_alone_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let db = intervals(dir.path());
    let alone = ok(&db, &["stats", "b", "-d", DAY]);
    assert_eq!(ok(&db, &["stats", "b,", "-d", DAY]), alone);
    assert_eq!(ok(&db, &["stats", "b,b", "-d", DAY]), alone);
    assert!(!alone.contains("any of these"), "{alone}");
    let json = json_of(&db, &["stats", "b", "-d", DAY, "--json"]);
    assert_eq!(json["field"], "b");
    assert!(json.get("pooled").is_none());
}

#[test]
fn a_list_refuses_what_takes_one_field() {
    let dir = tempfile::tempdir().unwrap();
    let db = intervals(dir.path());
    let (ok_, _, err) = datum(&db, &["stats", "b,c:x"]);
    assert!(!ok_);
    assert!(err.contains("one field"), "{err}");
    let (ok_, _, err) = datum(&db, &["stats", "b,c", "--group-by", "client"]);
    assert!(!ok_);
    assert!(err.contains("one field"), "{err}");
    // A parallel state keeps its comma.
    assert!(datum(&db, &["stats", "b:x,y"]).0);
}

/// Each name of a list starts a field as a name alone does.
#[test]
fn a_list_reads_each_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    ok(&db, &["start", "bake", "1h", "-d", DAY, "-t", "10:00"]);
    ok(&db, &["start", "cycle", "1h", "-d", DAY, "-t", "10:00"]);
    let got = pooled_row(&db, "ba,cy");
    assert_eq!(got["pooled"]["fields"], json!(["bake", "cycle"]));
    assert_eq!(got["pooled"]["union_seconds"], 3600);
}

fn listed(db: &Path) -> PathBuf {
    for line in [
        &["switch", "a", "one", "-d", DAY, "-t", "09:00"][..],
        &["switch", "b", "two", "-d", DAY, "-t", "10:00"],
        &["switch", "c", "three", "-d", DAY, "-t", "11:00"],
    ] {
        ok(db, line);
    }
    db.to_path_buf()
}

#[test]
fn every_reading_command_takes_a_list() {
    let dir = tempfile::tempdir().unwrap();
    let db = listed(&dir.path().join("t.db"));
    let names = |v: &Value, key: &str| -> Vec<String> {
        let mut got: Vec<String> = (v.as_array().unwrap().iter())
            .map(|r| r[key].as_str().unwrap().to_string())
            .collect();
        got.sort();
        got
    };

    for head in ["tail", "head"] {
        let rows = json_of(&db, &[head, "a,b", "--show", "json"]);
        assert_eq!(names(&rows["rows"], "field"), ["a", "b"], "{head}");
    }
    let (ok_, _, err) = datum(&db, &["tail", "a,b:one"]);
    assert!(!ok_);
    assert!(err.contains("one field"), "{err}");
    assert!(!datum(&db, &["tail", "a,b,c:one"]).0);
    assert_eq!(ok(&db, &["get", "a,b"]).matches("EXISTS").count(), 2);
    let found = ok(&db, &["get", "a,c", "state=one"]);
    assert!(found.contains("one") && !found.contains("three"), "{found}");
    assert_eq!(ok(&db, &["count", "a,b"]).trim(), "2");
    assert_eq!(
        ok(&db, &["count", "a,b", "--group-by", "field"])
            .lines()
            .count(),
        2
    );
    assert_eq!(ok(&db, &["ids", "a,b"]).lines().count(), 2);
    assert_eq!(ok(&db, &["ids", "a,c", "--slug"]).lines().count(), 2);

    let hits = ok(&db, &["grep", "-f", "a,b", "o"]);
    assert!(hits.contains("one") && hits.contains("two"), "{hits}");
    assert!(!hits.contains("three"), "{hits}");

    assert_eq!(
        names(&json_of(&db, &["fields", "a,b", "--json"]), "field"),
        ["a", "b"]
    );
    let many = json_of(&db, &["states", "a,b", "--json"]);
    assert_eq!(names(&many, "field"), ["a", "b"]);
    assert_eq!(json_of(&db, &["states", "a", "--json"])["field"], "a");
    let text = ok(&db, &["states", "a,b"]);
    assert!(text.starts_with("a\n"), "{text}");
    assert!(text.contains("\nb\n"), "{text}");

    let now = json_of(&db, &["now", "a,b", "--json"]);
    assert_eq!(names(&now["active"], "field"), ["a", "b"]);
    let text = ok(&db, &["now", "a", "--bare"]);
    assert!(text.contains("one") && !text.contains("two"), "{text}");
    let everything = json_of(&db, &["now", "--json"]);
    assert_eq!(names(&everything["active"], "field"), ["a", "b", "c"]);

    let drawn = ok(&db, &["dayview", "a,b", "-d", DAY]);
    assert!(drawn.contains("data-field=\"a\""), "a is drawn");
    assert!(!drawn.contains("data-field=\"c\""), "c is not");
    let drawn = ok(&db, &["nowview", "a,b", "-o", "-"]);
    assert!(drawn.contains("data-field=\"b\""), "b is drawn");
    assert!(!drawn.contains("data-field=\"c\""), "c is not");
}

/// `now` names a field for what it holds: off or ignored is an answer, and
/// a name it does not know says so.
#[test]
fn now_shows_a_named_field_whatever_it_holds() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    ok(&db, &["switch", "a", "one"]);
    ok(&db, &["switch", "a", "false"]);
    assert!(json_of(&db, &["now", "--json"])["tracked_false"]
        .as_array()
        .unwrap()
        .is_empty());
    let named = json_of(&db, &["now", "a", "--json"]);
    assert_eq!(named["tracked_false"][0]["field"], "a");
    let (ok_, _, err) = datum(&db, &["now", "a,nosuch", "--bare"]);
    assert!(ok_);
    assert!(err.contains("no field 'nosuch'"), "{err}");
}

/// A field from before names could not hold a comma: written straight to
/// the database, as a v2 import could.
fn old_comma_field(path: &Path) {
    let db = Db::open(path.to_str().unwrap()).unwrap();
    entry_builder("a,b")
        .verb(Verb::Switch)
        .state(json!("old"))
        .insert(&db);
}

#[test]
fn an_old_name_with_a_comma_is_read_literally() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    old_comma_field(&db);

    assert_eq!(ok(&db, &["count", "a,b"]).trim(), "1");
    assert_eq!(json_of(&db, &["stats", "a,b", "--json"])["field"], "a,b");
    assert_eq!(ok(&db, &["tail", "a,b"]).matches("old").count(), 1);

    let moved = ok(&db, &["rename", "a,b", "c"]);
    assert!(moved.contains("a,b → c"), "{moved}");
    assert_eq!(ok(&db, &["count", "c"]).trim(), "1");
}

/// Once `a` and `b` are both fields, `a,b` is the two of them; `rename`,
/// which takes one name, still reads the old one.
#[test]
fn fields_named_by_the_parts_win_the_list_and_rename_stays_literal() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    old_comma_field(&db);
    ok(&db, &["switch", "a", "one"]);
    assert_eq!(
        ok(&db, &["count", "a,b"]).trim(),
        "1",
        "b is no field: the old name"
    );
    ok(&db, &["switch", "b", "two"]);
    ok(&db, &["switch", "b", "three"]);
    assert_eq!(
        ok(&db, &["count", "a,b"]).trim(),
        "3",
        "both fields: the list"
    );
    ok(&db, &["rename", "a,b", "c"]);
    assert_eq!(ok(&db, &["count", "c"]).trim(), "1");
}
