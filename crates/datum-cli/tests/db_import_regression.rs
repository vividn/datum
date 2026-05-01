//! Regression test for `datum db import` against a committed synthetic
//! fixture (da-zio, rewritten for v=20 schema under da-5ns.6).
//!
//! Locks the **structural correctness** of legacy-data ingestion against
//! the new (occur_time, state, dur, following_state, verb) schema so
//! future refactors cannot silently break it. Doesn't assert rendered
//! output — golden-file diffs from the in-flux renderer would be noise.
//!
//! Fixture: `tests/fixtures/datum_backup_synthetic.json`. Each doc covers
//! a distinct slice of the v=20 import mapping. Verbs below are derived
//! from (state, dur, following_state) via `datum_core::derive_verb` —
//! the `entry.verb` column is hardcoded "point" until da-5ns.3 wires
//! derivation into the insert path.
//!
//! "Verb (derived)" is what `derive_verb(state, dur, fs)` returns when called
//! on the **stored** columns. Note: the null→true promotion is destructive
//! — once a "no state" entry is stored as `state=true`, re-deriving gives
//! Switch (state set, no dur) rather than Point. This is by design; the
//! renderer treats a Switch-to-true the same as a no-state Point.
//!
//! | Doc # | Field          | dur in   | dur out | state in        | state out      | Verb (derived) |
//! | ----- | -------------- | -------- | ------- | --------------- | -------------- | -------------- |
//! | 1     | mood           | absent   | NULL    | "happy"         | "happy"        | Switch         |
//! | 2     | sleep          | "PT8H"   | "PT8H"  | absent          | true           | Block          |
//! | 3     | work           | "PT3M"   | "PT3M"  | "focus"         | "focus"        | Block          |
//! | 4     | brushteeth     | absent   | NULL    | absent          | true           | Switch (re-derived from state=true)|
//! | 5     | tracking       | absent   | NULL    | true            | true           | Switch         |
//! | 6     | test           | "PT1M"   | "PT1M"  | absent          | true           | Block          |
//! | 7     | consume        | absent   | NULL    | absent          | true           | Switch (re-derived from state=true)|
//! | 8     | malformed_dur  | bad str  | NULL    | absent          | true           | Switch (re-derived from state=true)|
//! | 9     | feeling        | absent   | NULL    | {prim, int}     | {prim, int}    | Switch         |
//! | 10    | tags           | absent   | NULL    | [arr]           | [arr]          | Switch         |
//! | 11    | thinking       | null     | "PT0S"  | "idea"          | "idea"         | Point          |
//!
//! Counts: 11 imported (8 dur=NULL/zero, 3 dur=positive).

mod common;

use common::run_cli;
use datum_core::derive_verb;
use datum_core::entry::Verb;
use datum_core::test_util::fresh_db;
use std::collections::HashSet;

fn verb_of(e: &datum_core::entry::Entry) -> Verb {
    derive_verb(e.state.clone(), e.dur.as_deref(), e.following_state.clone())
        .expect("derive_verb on imported entry")
        .0
}

fn is_block_dur(d: &Option<String>) -> bool {
    match d.as_deref() {
        None => false,
        Some(s) if matches!(s, "PT0S" | "P0D" | "PT0M" | "PT0H") => false,
        Some(_) => true,
    }
}

const FIXTURE: &str = "tests/fixtures/datum_backup_synthetic.json";

#[test]
fn db_import_synthetic_locks_db_shape() {
    let db = fresh_db();
    run_cli(&["db", "import", FIXTURE], &db).expect("synthetic import");

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 11, "fixture defines 11 importable docs");

    // Per the table above: 8 entries have dur NULL or zero, 3 have positive dur.
    let zero_or_null = entries.iter().filter(|e| !is_block_dur(&e.dur)).count();
    let positive = entries.iter().filter(|e| is_block_dur(&e.dur)).count();
    assert_eq!(
        zero_or_null, 8,
        "expected 8 entries with dur=NULL or dur=zero (docs 1, 4, 5, 7, 8, 9, 10, 11)",
    );
    assert_eq!(
        positive, 3,
        "expected 3 entries with positive dur (docs 2, 3, 6)"
    );

    // Every entry with positive dur must carry the original `dur` literal in
    // the `data` blob as intent (datum-compatible read-back) AND store it
    // verbatim in the `dur` column.
    for e in entries.iter().filter(|e| is_block_dur(&e.dur)) {
        assert!(
            e.occur_time.is_some(),
            "block {} missing occur_time",
            e.slug
        );
        let dur_str = e.dur.as_deref().expect("block has dur set");
        let extra = e.data.as_ref().expect("block must have data blob");
        assert!(
            extra["dur"].is_string(),
            "block {} must preserve `dur` literal in data, got {}",
            e.slug,
            extra["dur"],
        );
        assert_eq!(
            extra["dur"].as_str().unwrap(),
            dur_str,
            "block {} data.dur literal must match dur column",
            e.slug,
        );
    }

    // Provenance + sentinel intent fields on every imported row.
    // `_import` lives in `extra_metadata` (da-tsw), strictly disjoint from
    // user-payload `data`.
    for e in &entries {
        let metadata = e
            .extra_metadata
            .as_ref()
            .unwrap_or_else(|| panic!("row {} missing extra_metadata", e.slug));
        assert_eq!(
            metadata["_import"]["source"],
            serde_json::json!("datum-v2-couchdb"),
            "row {} missing _import.source marker",
            e.slug,
        );
        assert!(
            e.data.as_ref().and_then(|d| d.get("_import")).is_none(),
            "row {} leaked `_import` into user-payload `data`",
            e.slug,
        );
        assert_eq!(
            e.parser_version, 0,
            "row {} parser_version must be 0 (sentinel for imported)",
            e.slug,
        );
    }

    // hid is non-empty and unique across imported rows (PK invariant).
    let mut seen = HashSet::new();
    for e in &entries {
        assert!(!e.hid.is_empty(), "row {} has empty hid", e.slug);
        assert!(seen.insert(e.hid.clone()), "duplicate hid {}", e.hid);
    }

    // ─── Per-doc structural asserts (v=20 columns) ──────────────────────

    // Doc 1 (mood, string state, no dur): state set + no dur → Switch.
    let mood = entries
        .iter()
        .find(|e| e.field == "mood")
        .expect("doc 1: mood");
    assert!(mood.dur.is_none());
    assert_eq!(mood.state, Some(serde_json::json!("happy")));
    assert_eq!(verb_of(mood), Verb::Switch);

    // Doc 2 (sleep / PT8H): no state → state=true (promoted), dur="PT8H" → Block.
    let sleep = entries
        .iter()
        .find(|e| e.field == "sleep")
        .expect("doc 2: sleep");
    assert_eq!(sleep.dur.as_deref(), Some("PT8H"), "sleep dur verbatim");
    assert_eq!(
        sleep.state,
        Some(serde_json::json!(true)),
        "null→true promotion"
    );
    assert_eq!(verb_of(sleep), Verb::Block);
    assert_eq!(
        sleep.data.as_ref().unwrap()["dur"],
        serde_json::json!("PT8H"),
        "sleep dur literal also preserved in data",
    );

    // Doc 3 (work, string state + dur): Block with state preserved.
    let work = entries
        .iter()
        .find(|e| e.field == "work")
        .expect("doc 3: work");
    assert_eq!(work.dur.as_deref(), Some("PT3M"));
    assert_eq!(work.state, Some(serde_json::json!("focus")));
    assert_eq!(verb_of(work), Verb::Block);

    // Doc 5 (tracking, boolean state): Switch with state=true.
    let tracking = entries
        .iter()
        .find(|e| e.field == "tracking")
        .expect("doc 5: tracking");
    assert!(tracking.dur.is_none());
    assert_eq!(tracking.state, Some(serde_json::json!(true)));
    assert_eq!(verb_of(tracking), Verb::Switch);

    // Doc 6 (test / PT1M, o:0 UTC): tz=UTC, offset=0 round-trip.
    let test = entries
        .iter()
        .find(|e| e.field == "test")
        .expect("doc 6: test");
    assert_eq!(test.dur.as_deref(), Some("PT1M"));
    assert_eq!(test.occur_time.as_ref().unwrap().offset_minutes, 0);
    assert_eq!(test.occur_time.as_ref().unwrap().timezone, "UTC");

    // Doc 7 (consume): extras `id`, `medium` go into `data` and don't
    // collide with Entry.hid.
    let consume = entries
        .iter()
        .find(|e| e.field == "consume")
        .expect("doc 7: consume");
    let consume_extra = consume.data.as_ref().unwrap();
    assert_eq!(consume_extra["id"], serde_json::json!("song-42"));
    assert_eq!(consume_extra["medium"], serde_json::json!("audio"));
    assert_ne!(
        consume.hid, "song-42",
        "datum doc's `data.id` must NOT overwrite Entry.hid",
    );

    // Doc 8 (malformed dur): warn-not-fail, dur=NULL, literal preserved
    // in `data`. State absent → promoted to true; re-derive of
    // (state=true, dur=NULL, fs=None) → Switch.
    let malformed = entries
        .iter()
        .find(|e| e.field == "malformed_dur")
        .expect("doc 8: malformed_dur");
    assert!(malformed.dur.is_none(), "unparseable dur → dur column NULL");
    assert_eq!(
        malformed.state,
        Some(serde_json::json!(true)),
        "null→true promotion"
    );
    assert_eq!(verb_of(malformed), Verb::Switch);
    assert_eq!(
        malformed.data.as_ref().unwrap()["dur"],
        serde_json::json!("not-a-duration"),
        "malformed dur literal must survive verbatim in data",
    );

    // Doc 9 (feeling, object state): object state → Switch verbatim.
    let feeling = entries
        .iter()
        .find(|e| e.field == "feeling")
        .expect("doc 9: feeling");
    assert!(feeling.dur.is_none());
    assert_eq!(verb_of(feeling), Verb::Switch);
    assert_eq!(
        feeling.state,
        Some(serde_json::json!({"primary": "calm", "intensity": 5})),
        "object state must round-trip verbatim",
    );

    // Doc 10 (tags, array state): array state → Switch verbatim.
    let tags = entries
        .iter()
        .find(|e| e.field == "tags")
        .expect("doc 10: tags");
    assert!(tags.dur.is_none());
    assert_eq!(verb_of(tags), Verb::Switch);
    assert_eq!(tags.state, Some(serde_json::json!(["focus", "deep-work"])));

    // Doc 11 (thinking, state + dur=null): explicit null → dur="PT0S".
    // state="idea" + DurKind::Zero + no fs → Verb::Point (state preserved).
    let thinking = entries
        .iter()
        .find(|e| e.field == "thinking")
        .expect("doc 11: thinking");
    assert_eq!(thinking.dur.as_deref(), Some("PT0S"), "null dur → PT0S");
    assert_eq!(thinking.state, Some(serde_json::json!("idea")));
    assert_eq!(verb_of(thinking), Verb::Point);
    assert!(
        thinking.data.as_ref().unwrap()["dur"].is_null(),
        "explicit dur=null must round-trip into data",
    );
}

#[test]
fn db_import_synthetic_is_idempotent() {
    let db = fresh_db();
    run_cli(&["db", "import", FIXTURE], &db).expect("first import");
    let n1 = db.all_entries().unwrap().len();
    assert_eq!(n1, 11);

    // Second run on the same DB: content_hash dedup must skip every doc.
    run_cli(&["db", "import", FIXTURE], &db).expect("second import");
    let n2 = db.all_entries().unwrap().len();
    assert_eq!(n2, n1, "re-importing same fixture must not add rows");
}

// ─── Binary harness: lock the user-visible summary line ──────────────────

#[test]
fn db_import_synthetic_summary_reports_correct_counts() {
    let mut db_path = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    db_path.push(format!("datum-db-import-regression-{pid}-{nonce}.db"));
    let _ = std::fs::remove_file(&db_path);

    let bin = env!("CARGO_BIN_EXE_datum");
    let out = std::process::Command::new(bin)
        .env("DATUM_DB", &db_path)
        .env("DATUM_COLOR", "0")
        .args(["db", "import", FIXTURE])
        .output()
        .expect("run datum binary");
    let _ = std::fs::remove_file(&db_path);

    assert!(
        out.status.success(),
        "import failed: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("11 imported"),
        "expected '11 imported' in stdout, got: {stdout}",
    );
    assert!(
        stdout.contains("8 point"),
        "expected '8 point' in stdout, got: {stdout}",
    );
    assert!(
        stdout.contains("3 block"),
        "expected '3 block' in stdout, got: {stdout}",
    );
}
