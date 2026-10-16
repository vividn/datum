//! `datum check`'s all-clear names what it examined (da-qd9ao).
//!
//! On the main this bead branched from, an empty database and a populated
//! one printed the same «✓ No consistency issues found.», so «nothing
//! wrong» and «nothing looked at» read identically. The line now carries
//! its denominator — entries, field specs, aliases and the checkers that
//! ran — counted from the walk the checkers made.

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn datum(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "datum {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn all_clear_over_an_empty_db_differs_from_one_over_entries() {
    let dir = TempDir::new().unwrap();
    let empty = dir.path().join("empty.db");
    let seeded = dir.path().join("seeded.db");
    datum(&seeded, &["occur", "coffee"]);
    datum(&seeded, &["occur", "coffee", "-q"]);
    datum(&seeded, &["alias", "set", "cf", "occur coffee"]);

    let all =
        "checkers: base, verbs, aliases, slugs, integrity, flags, tz, refs, stats, sync, spool";
    datum(&empty, &["db", "migrate"]);
    let on_empty = datum(&empty, &["check"]);
    let on_seeded = datum(&seeded, &["check"]);
    assert_ne!(
        on_empty, on_seeded,
        "a green over nothing must not look like one over data"
    );
    assert_eq!(
        on_empty,
        format!("✓ No consistency issues found: 0 entries, 0 field specs, 0 aliases examined, 0 cached days recomputed, 0 differ, 0 rows read against 0 sync ops, 0 differ ({all}).\n")
    );
    // Two coffees, the spec the first one wrote and the alias: specs and
    // aliases are entries too, and the checkers walk them.
    assert_eq!(
        on_seeded,
        format!(
            "✓ No consistency issues found: 4 entries, 1 field spec, 1 alias examined, 0 cached days recomputed, 0 differ, 4 rows read against 4 sync ops, 0 differ ({all}).\n"
        )
    );

    // A filter names only the checkers that ran, and what they looked at.
    assert_eq!(
        datum(&seeded, &["check", "aliases"]),
        "✓ No consistency issues found: 0 entries, 0 field specs, 1 alias examined (checkers: aliases).\n"
    );

    let json: Value = serde_json::from_str(&datum(&seeded, &["check", "--json"])).unwrap();
    assert_eq!(json["examined"]["entries"], 4);
    assert_eq!(json["examined"]["specs"], 1);
    assert_eq!(json["examined"]["aliases"], 1);
    assert_eq!(json["examined"]["checkers"].as_array().unwrap().len(), 11);
    assert_eq!(
        json["examined"]["sync"],
        serde_json::json!({"rows": 4, "ops": 4})
    );
}

/// da-1i6mi: `check stats` says how many cached days it recomputed, so a
/// clean run over an empty cache does not read like one over a full one.
#[test]
fn check_stats_names_the_cached_days_it_recomputed() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("stats.db");
    datum(&db, &["switch", "mood", "calm", "-yyy"]);
    let only = "(checkers: stats).\n";
    let clean = |days: &str| {
        format!("✓ No consistency issues found: 0 entries, 0 field specs, 0 aliases examined, {days} recomputed, 0 differ {only}")
    };
    assert_eq!(datum(&db, &["check", "stats"]), clean("0 cached days"));
    datum(&db, &["stats"]);
    // Three days ago, the two after it: the closed days the span covers.
    assert_eq!(datum(&db, &["check", "stats"]), clean("3 cached days"));

    let json: Value = serde_json::from_str(&datum(&db, &["check", "stats", "--json"])).unwrap();
    assert_eq!(json["examined"]["stats_days"], 3);
    let json: Value = serde_json::from_str(&datum(&db, &["check", "aliases", "--json"])).unwrap();
    assert!(json["examined"].get("stats_days").is_none(), "{json}");
}

/// da-m34q7: a stored warning `check` cannot decode is a finding that names
/// the row, beside the findings of the rows it can — where it used to abort
/// the whole command with «Database error».
#[test]
fn a_warning_row_that_does_not_decode_is_a_finding() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("datum.db");
    datum(&db, &["start", "work", "-t", "-5"]);
    datum(&db, &["start", "work"]);
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(
            "INSERT INTO integrity_warnings
                 (id, hid, entry_slug, local_entry, detected_at, detected_by, kind)
             VALUES ('w-old', 'w-old', 'work:x', '{\"device_id\": \"d\"}',
                     '2026-10-01T00:00:00Z', 'local_validator', 'overlap'),
                    ('w-bad', 'w-bad', 'work:x', 'not json',
                     '2026-10-01T00:00:00Z', 'local_validator', 'overlap')",
        )
        .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", &db)
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path())
        .env("NO_COLOR", "1")
        .arg("check")
        .output()
        .unwrap();
    let (stdout, stderr) = (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    );
    assert_eq!(out.status.code(), Some(1), "{stdout}{stderr}");
    assert!(!stderr.contains("Database error"), "{stderr}");
    assert!(
        stdout.contains(
            "2 stored integrity warnings cannot be read:\n  w-old: missing field `hid` at line 1 column 18\n  w-bad: expected ident at line 1 column 2"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("1 field opened a second time while already open"),
        "the readable row is still reported: {stdout}"
    );
}

/// da-tgulk: a flag's words are in `state` alone, so `check flags` reads
/// the words an `update` wrote, where it read a second copy in
/// `extra_metadata` that no edit reaches; and its hint names what clears a
/// flag, `datum del`, where it said «deleting or updating the row».
#[test]
fn a_flags_words_have_one_copy_and_del_alone_clears_it() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("flag.db");
    datum(&db, &["flag", "mood", "check this"]);
    let issues = || -> Value {
        serde_json::from_str::<Value>(&datum(&db, &["check", "flags", "--json"])).unwrap()["issues"]
            .take()
    };
    let hid = issues()[0]["entries"][0]["hid"]
        .as_str()
        .unwrap()
        .to_string();

    datum(&db, &["update", &hid, "state=checked: fine"]);
    let issue = issues()[0].take();
    assert_eq!(issue["entries"][0]["state"], "checked: fine");
    assert_eq!(issue["example"]["detail"], "checked: fine");
    let message = issue["message"].as_str().unwrap();
    assert!(
        message.ends_with(
            "mark one done with `datum update <hid> done=true`, or delete it with `datum del <hid>`."
        ),
        "{message}"
    );
    let row = datum_core::db::Db::open(db.to_str().unwrap()).unwrap();
    let row = datum_core::test_util::logged_entries(&row, "mood").remove(0);
    assert_eq!(row.extra_metadata, Some(serde_json::json!({"flag": true})));

    datum(&db, &["del", &hid]);
    assert_eq!(issues(), serde_json::json!([]));
}

/// da-wvte1: a flag with `done=true` in its data is dealt with: `check`
/// lists it no more, and it stays in the record. `done=false` still pends.
#[test]
fn a_flag_marked_done_leaves_check_and_stays_in_the_record() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("flag.db");
    datum(&db, &["flag", "mood", "check this"]);
    let issues = || -> Value {
        serde_json::from_str::<Value>(&datum(&db, &["check", "flags", "--json"])).unwrap()["issues"]
            .take()
    };
    let hid = issues()[0]["entries"][0]["hid"]
        .as_str()
        .unwrap()
        .to_string();

    datum(&db, &["update", &hid, "done=false"]);
    assert_eq!(issues()[0]["count"], 1);

    datum(&db, &["update", &hid, "done=true"]);
    assert_eq!(issues(), serde_json::json!([]));
    let row = datum_core::db::Db::open(db.to_str().unwrap()).unwrap();
    let row = datum_core::test_util::logged_entries(&row, "mood").remove(0);
    assert_eq!(row.extra_metadata, Some(serde_json::json!({"flag": true})));
    assert_eq!(row.data.unwrap()["done"], true);
}

/// da-gmjyx: a meta note's words are in `state` alone, as a flag's are, so
/// `meta show` prints the words an `update` wrote, where it read a second
/// copy in `extra_metadata` that no edit reaches.
#[test]
fn a_meta_notes_words_have_one_copy() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("meta.db");
    let made = datum(&db, &["meta", "mood", "rate it at bedtime"]);
    let hid = made.split_whitespace().nth(1).unwrap();
    assert!(datum(&db, &["meta", "show", "mood"]).ends_with("  rate it at bedtime\n"));

    datum(&db, &["update", hid, "state=rate it on waking"]);
    let shown = datum(&db, &["meta", "show", "mood"]);
    assert!(shown.ends_with("  rate it on waking\n"), "{shown}");
    assert_eq!(shown.lines().count(), 1, "{shown}");
    let row = datum_core::db::Db::open(db.to_str().unwrap()).unwrap();
    let row = datum_core::test_util::logged_entries(&row, "mood").remove(0);
    assert_eq!(row.extra_metadata, Some(serde_json::json!({"meta": true})));
}
