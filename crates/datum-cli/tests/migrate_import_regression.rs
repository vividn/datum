//! Regression test for `datum migrate import` against a committed synthetic
//! fixture (da-zio).
//!
//! This test locks the **structural correctness** of legacy-data ingestion
//! so future refactors (parser changes, schema migrations, render rewrites)
//! cannot silently break it. It deliberately does NOT assert rendered output
//! — the `dayview` / `now` renderers are still being tuned and golden-file
//! diffs would generate noise. Only the import-mapping shape is locked here.
//!
//! Fixture: `tests/fixtures/datum_backup_synthetic.json`. Each doc covers a
//! distinct slice of the import-mapping matrix:
//!
//! | Doc # | Field          | Shape    | Notes                                                                        |
//! | ----- | -------------- | -------- | ---------------------------------------------------------------------------- |
//! | 1     | mood           | point    | string `state=happy`, no dur → `Verb::Switch` (state present, dur absent)    |
//! | 2     | sleep          | block    | `dur=PT8H` → `end_time = start + 8h`, dur literal preserved in extra_data    |
//! | 3     | work           | block    | string `state=focus` + `dur=PT3M` → `Verb::Block` (state present + dur)      |
//! | 4     | brushteeth     | point    | minimal occur-only — no state, no dur                                        |
//! | 5     | tracking       | point    | boolean `state=true` → `Verb::Switch`                                        |
//! | 6     | test           | block    | `dur=PT1M`, `o=0` UTC → tz/offset round-trip                                 |
//! | 7     | consume        | point    | extras `id`, `medium` survive into extra_data; no collision with `Entry.hid` |
//! | 8     | malformed_dur  | point    | unparseable `dur` → warn-not-fail, no `end_time`, literal preserved verbatim |
//! | 9     | feeling        | point    | object `state={primary, intensity}` → `Verb::Switch`, value stored verbatim  |
//! | 10    | tags           | point    | array `state=[...]` → `Verb::Switch` (single entry; no expansion)            |
//! | 11    | thinking       | point    | string `state=idea` + `dur=null` → `Verb::Switch` (null dur ≡ absent)        |
//!
//! Counts: 11 imported (8 point, 3 block).

mod common;

use common::run_cli;
use datum_core::entry::{EntryShape, Verb};
use datum_core::test_util::fresh_db;
use std::collections::HashSet;

const FIXTURE: &str = "tests/fixtures/datum_backup_synthetic.json";

#[test]
fn migrate_import_synthetic_locks_db_shape() {
    let db = fresh_db();
    run_cli(&["migrate", "import", FIXTURE], &db).expect("synthetic import");

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 11, "fixture defines 11 importable docs");

    let points = entries
        .iter()
        .filter(|e| e.entry_shape == EntryShape::Point)
        .count();
    let blocks = entries
        .iter()
        .filter(|e| e.entry_shape == EntryShape::Block)
        .count();
    assert_eq!(
        points, 8,
        "expected 8 point entries (docs 1, 4, 5, 7, 8, 9, 10, 11)",
    );
    assert_eq!(blocks, 3, "expected 3 block entries (docs 2, 3, 6)");

    // Every block has both times set and the original `dur` literal preserved
    // in extra_data as intent (datum-compatible read-back).
    for e in entries
        .iter()
        .filter(|e| e.entry_shape == EntryShape::Block)
    {
        assert!(
            e.start_time.is_some(),
            "block {} missing start_time",
            e.slug
        );
        assert!(e.end_time.is_some(), "block {} missing end_time", e.slug);
        let extra = e.data.as_ref().expect("block must have extra_data");
        assert!(
            !extra["dur"].is_null(),
            "block {} must preserve `dur` literal in extra_data",
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

    // ─── Per-doc structural asserts ──────────────────────────────────────

    // Doc 1 (mood, string state, no dur): non-boolean state still classifies
    // as Switch under the state-aware rule (was Occur before da-1wf).
    let mood = entries.iter().find(|e| e.field == "mood").expect("doc 1: mood");
    assert_eq!(mood.entry_shape, EntryShape::Point);
    assert_eq!(mood.verb, Verb::Switch);
    assert_eq!(mood.value, Some(serde_json::json!("happy")));

    // Doc 2 (sleep / PT8H): end_time = start + 8h.
    let sleep = entries
        .iter()
        .find(|e| e.field == "sleep")
        .expect("doc 2: sleep");
    let span = sleep.end_time.as_ref().unwrap().utc - sleep.start_time.as_ref().unwrap().utc;
    assert_eq!(span.num_hours(), 8, "sleep block must span exactly 8h");
    assert_eq!(
        sleep.data.as_ref().unwrap()["dur"],
        serde_json::json!("PT8H"),
        "sleep dur literal must round-trip",
    );

    // Doc 3 (work): block with value preserved and dur intent kept.
    let work = entries
        .iter()
        .find(|e| e.field == "work")
        .expect("doc 3: work");
    assert_eq!(work.entry_shape, EntryShape::Block);
    assert_eq!(work.value, Some(serde_json::json!("focus")));
    assert_eq!(
        work.data.as_ref().unwrap()["dur"],
        serde_json::json!("PT3M"),
    );

    // Doc 5 (tracking, boolean state): switch shape is Point + value=true.
    let tracking = entries
        .iter()
        .find(|e| e.field == "tracking")
        .expect("doc 5: tracking");
    assert_eq!(tracking.entry_shape, EntryShape::Point);
    assert_eq!(tracking.value, Some(serde_json::json!(true)));

    // Doc 6 (test / PT1M, o:0 UTC): tz=UTC, offset=0 round-trip through DatumTime.
    let test = entries
        .iter()
        .find(|e| e.field == "test")
        .expect("doc 6: test");
    assert_eq!(test.entry_shape, EntryShape::Block);
    assert_eq!(test.start_time.as_ref().unwrap().offset_minutes, 0);
    assert_eq!(test.start_time.as_ref().unwrap().timezone, "UTC");
    assert_eq!(
        (test.end_time.as_ref().unwrap().utc - test.start_time.as_ref().unwrap().utc).num_minutes(),
        1,
    );

    // Doc 7 (consume): no field-name collision — `id` and `medium` from the
    // datum doc go into extra_data, NOT into the Entry's own `hid` PK.
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

    // Doc 8 (malformed dur): imports as Point/Occur, dur literal preserved
    // verbatim, no derived end_time.
    let malformed = entries
        .iter()
        .find(|e| e.field == "malformed_dur")
        .expect("doc 8: malformed_dur");
    assert_eq!(malformed.entry_shape, EntryShape::Point);
    assert!(
        malformed.end_time.is_none(),
        "unparseable dur must NOT derive an end_time",
    );
    assert_eq!(
        malformed.data.as_ref().unwrap()["dur"],
        serde_json::json!("not-a-duration"),
        "malformed dur literal must survive verbatim in extra_data",
    );

    // Doc 9 (feeling, object state): object value stored verbatim under Switch.
    let feeling = entries.iter().find(|e| e.field == "feeling").expect("doc 9: feeling");
    assert_eq!(feeling.entry_shape, EntryShape::Point);
    assert_eq!(feeling.verb, Verb::Switch);
    assert_eq!(
        feeling.value,
        Some(serde_json::json!({"primary": "calm", "intensity": 5})),
        "object state must round-trip as the entry's value verbatim",
    );

    // Doc 10 (tags, array state): array value stored as a single entry; the
    // render layer decides what to do with multi-state values.
    let tags = entries.iter().find(|e| e.field == "tags").expect("doc 10: tags");
    assert_eq!(tags.entry_shape, EntryShape::Point);
    assert_eq!(tags.verb, Verb::Switch);
    assert_eq!(tags.value, Some(serde_json::json!(["focus", "deep-work"])));

    // Doc 11 (thinking, state + dur=null): null dur is treated as absent,
    // so this is Switch (state present, no derived end_time). The literal
    // null survives in extra_data as datum-compatible read-back.
    let thinking = entries.iter().find(|e| e.field == "thinking").expect("doc 11: thinking");
    assert_eq!(thinking.entry_shape, EntryShape::Point);
    assert_eq!(thinking.verb, Verb::Switch);
    assert_eq!(thinking.value, Some(serde_json::json!("idea")));
    assert!(thinking.end_time.is_none());
    assert!(
        thinking.data.as_ref().unwrap()["dur"].is_null(),
        "explicit dur=null must round-trip into extra_data",
    );
}

#[test]
fn migrate_import_synthetic_is_idempotent() {
    let db = fresh_db();
    run_cli(&["migrate", "import", FIXTURE], &db).expect("first import");
    let n1 = db.all_entries().unwrap().len();
    assert_eq!(n1, 11);

    // Second run on the same DB: content_hash dedup must skip every doc.
    run_cli(&["migrate", "import", FIXTURE], &db).expect("second import");
    let n2 = db.all_entries().unwrap().len();
    assert_eq!(n2, n1, "re-importing same fixture must not add rows");
}

// ─── Binary harness: lock the user-visible summary line ──────────────────

#[test]
fn migrate_import_synthetic_summary_reports_correct_counts() {
    let mut db_path = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    db_path.push(format!("datum-migrate-import-regression-{pid}-{nonce}.db"));
    let _ = std::fs::remove_file(&db_path);

    let bin = env!("CARGO_BIN_EXE_datum");
    let out = std::process::Command::new(bin)
        .env("DATUM_DB", &db_path)
        .env("DATUM_COLOR", "0")
        .args(["migrate", "import", FIXTURE])
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
