//! `datum migrate import` integration tests (da-a8u).
//!
//! Verifies the v2-aware mapping from legacy datum CouchDB-shaped JSON
//! into v2 entries: dur preservation as intent, end_time derivation,
//! entry_shape correctness, and verb mapping.

mod common;

use common::run_cli;
use datum_core::entry::{EntryShape, Verb};
use datum_core::test_util::fresh_db;
use std::io::Write;

/// Write a backup JSON to a temp file and import it. Returns the path so
/// callers can keep the file alive across the import call.
fn write_backup(docs: serde_json::Value) -> tempfile::NamedTempFile {
    let backup = serde_json::json!({
        "backupTime": "2025-02-02T12:26:32.488Z",
        "docs": docs,
    });
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(serde_json::to_string(&backup).unwrap().as_bytes())
        .unwrap();
    f.flush().unwrap();
    f
}

// ─── 1. occur-only (no dur) → Point/Occur ────────────────────────────

#[test]
fn occur_only_doc_maps_to_point_occur() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "brushteeth:2025-01-08T23:53:08.259Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T23:53:08.259Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "brushteeth"
        },
        "meta": {
            "humanId": "abc123",
            "createTime": {"utc": "2025-01-08T23:53:08.569Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.field, "brushteeth");
    assert_eq!(e.verb, Verb::Occur);
    assert_eq!(e.entry_shape, EntryShape::Point);
    assert!(e.start_time.is_some());
    assert!(e.end_time.is_none());
}

// ─── 2. occur+dur → Block, end_time derived, dur preserved ───────────

#[test]
fn occur_with_dur_maps_to_block_and_derives_end_time() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "exercise:2025-01-08T08:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T08:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "exercise",
            "dur": "PT30M"
        },
        "meta": {
            "humanId": "xyz",
            "createTime": {"utc": "2025-01-08T08:00:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.verb, Verb::Block);
    assert_eq!(e.entry_shape, EntryShape::Block);
    let start = e.start_time.as_ref().unwrap();
    let end = e.end_time.as_ref().unwrap();
    assert_eq!((end.utc - start.utc).num_minutes(), 30);
    // dur literal preserved as user payload in `data`.
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra["dur"], serde_json::json!("PT30M"));
    // `_import` provenance now lives in `extra_metadata` (da-tsw), strictly
    // disjoint from user-payload `data`.
    let metadata = e.extra_metadata.as_ref().expect("extra_metadata must hold _import");
    assert_eq!(
        metadata["_import"]["source"],
        serde_json::json!("datum-v2-couchdb"),
    );
    assert!(
        e.data.as_ref().and_then(|d| d.get("_import")).is_none(),
        "`_import` must NOT appear in user-payload `data`",
    );
}

// ─── 3. switch (boolean state) → Point/Switch ────────────────────────

#[test]
fn switch_doc_with_boolean_state_maps_to_point_switch() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "contacts:2025-01-08T23:53:21.053Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T23:53:21.053Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "contacts",
            "state": false,
            "lastState": true
        },
        "meta": {
            "humanId": "ok",
            "createTime": {"utc": "2025-01-08T23:53:21.464Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.verb, Verb::Switch);
    assert_eq!(e.entry_shape, EntryShape::Point);
    assert_eq!(e.value, Some(serde_json::json!(false)));
    // lastState preserved as intent (no longer stripped).
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra["lastState"], serde_json::json!(true));
}

// ─── 4. explicit dur=null → Point/Occur, dur=null preserved ──────────

#[test]
fn explicit_dur_null_maps_to_point_occur_and_preserves_null() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "floss:2025-01-08T23:53:14.217Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T23:53:14.217Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "floss",
            "dur": serde_json::Value::Null
        },
        "meta": {
            "humanId": "n",
            "createTime": {"utc": "2025-01-08T23:53:14.693Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.verb, Verb::Occur);
    assert_eq!(e.entry_shape, EntryShape::Point);
    assert!(e.end_time.is_none());
    let extra = e.data.as_ref().unwrap();
    assert!(extra["dur"].is_null());
}

// ─── 5. missing occurTime → Point/Add (no time_start) ────────────────

#[test]
fn doc_without_occur_time_maps_to_add() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "journal:abc",
        "data": {
            "field": "journal",
            "note": "no time on this entry"
        },
        "meta": {
            "humanId": "j",
            "createTime": {"utc": "2025-01-08T23:00:00.000Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.verb, Verb::Add);
    assert_eq!(e.entry_shape, EntryShape::Point);
    assert!(e.start_time.is_none());
    assert!(e.end_time.is_none());
}

// ─── 6. v2 intent fields are set on imported rows ────────────────────

#[test]
fn imported_rows_carry_sentinel_intent_fields() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "brushteeth:2025-01-08T23:53:08.259Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T23:53:08.259Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "brushteeth"
        },
        "meta": {
            "humanId": "abc",
            "createTime": {"utc": "2025-01-08T23:53:08.569Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &db.all_entries().unwrap()[0];
    assert_eq!(e.parser_version, 0, "imported rows use sentinel parser_version=0");
    assert_eq!(e.raw_input, "");
    assert!(e.alias_chain.is_none());
    // Provenance lives in `extra_metadata`. The user-payload `data` is
    // empty for this fixture (no fields beyond occurTime/field/state) so it
    // stays NULL — strict separation, no marker bleed-through (da-tsw).
    let metadata = e.extra_metadata.as_ref().unwrap();
    let marker = &metadata["_import"];
    assert_eq!(marker["source"], serde_json::json!("datum-v2-couchdb"));
    assert!(marker["backup_time"].is_string(), "backup_time must be present");
    assert_eq!(
        marker["original_id"],
        serde_json::json!("brushteeth:2025-01-08T23:53:08.259Z"),
    );
    assert!(e.data.is_none(), "no user payload → `data` stays NULL");
}

// ─── 7. Dry-run summary reports point/block split ────────────────────

#[test]
fn dry_run_does_not_insert() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([
        {
            "_id": "exercise:2025-01-08T08:00:00.000Z",
            "data": {
                "occurTime": {"utc": "2025-01-08T08:00:00.000Z", "o": 1, "tz": "UTC"},
                "field": "exercise",
                "dur": "PT30M"
            },
            "meta": {"humanId": "a", "createTime": {"utc": "2025-01-08T08:00:00.500Z", "o": 1, "tz": "UTC"}}
        },
        {
            "_id": "brushteeth:2025-01-08T23:53:08.259Z",
            "data": {
                "occurTime": {"utc": "2025-01-08T23:53:08.259Z", "o": 1, "tz": "UTC"},
                "field": "brushteeth"
            },
            "meta": {"humanId": "b", "createTime": {"utc": "2025-01-08T23:53:08.569Z", "o": 1, "tz": "UTC"}}
        }
    ]));

    run_cli(&["migrate", "import", "--dry-run", f.path().to_str().unwrap()], &db).unwrap();
    assert_eq!(db.all_entries().unwrap().len(), 0, "dry-run must not insert");
}

// ─── 8. Integration: import the 3-day fixture from real data ─────────

#[test]
fn import_3day_fixture_produces_blocks_and_points() {
    let db = fresh_db();
    let fixture = "tests/fixtures/datum_backup_3days.json";
    run_cli(&["migrate", "import", fixture], &db).expect("import 3day fixture");

    let entries = db.all_entries().unwrap();
    assert!(entries.len() > 100, "fixture has hundreds of docs (got {})", entries.len());

    // Some entries must have block-shape (the fixture contains PT*M durs).
    let blocks: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_shape == EntryShape::Block)
        .collect();
    assert!(!blocks.is_empty(), "fixture must produce at least one block entry");

    // Every block entry must have both start and end and a derived non-zero
    // duration that matches the dur literal in extra_data.
    for e in &blocks {
        assert!(e.start_time.is_some(), "block missing start_time");
        assert!(e.end_time.is_some(), "block missing end_time");
        assert!(e.duration_seconds().unwrap() > 0, "block has zero duration");
        let dur_lit = &e.data.as_ref().unwrap()["dur"];
        assert!(
            dur_lit.is_string() || dur_lit.is_number(),
            "block must preserve a non-null dur literal in extra_data, got {dur_lit}"
        );
    }

    // Every imported row must carry the provenance marker — in
    // `extra_metadata`, never in user-payload `data` (da-tsw).
    for e in &entries {
        let metadata = e
            .extra_metadata
            .as_ref()
            .unwrap_or_else(|| panic!("row {} missing extra_metadata", e.slug));
        let marker = &metadata["_import"];
        assert_eq!(
            marker["source"],
            serde_json::json!("datum-v2-couchdb"),
            "row {} missing _import.source marker",
            e.slug,
        );
        assert!(marker["backup_time"].is_string(), "row {} missing backup_time", e.slug);
        assert!(marker["original_id"].is_string(), "row {} missing original_id", e.slug);
        assert!(
            e.data.as_ref().and_then(|d| d.get("_import")).is_none(),
            "row {} leaked `_import` into user-payload `data`",
            e.slug,
        );
        assert_eq!(e.parser_version, 0);
    }
}

// ─── 9. Re-import is a no-op (content_hash dedup) ─────────────────────

#[test]
fn second_import_is_idempotent_via_content_hash() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "brushteeth:2025-01-08T23:53:08.259Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T23:53:08.259Z", "o": 1, "tz": "UTC"},
            "field": "brushteeth"
        },
        "meta": {"humanId": "a", "createTime": {"utc": "2025-01-08T23:53:08.569Z", "o": 1, "tz": "UTC"}}
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();
    let n1 = db.all_entries().unwrap().len();
    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();
    let n2 = db.all_entries().unwrap().len();
    assert_eq!(n1, n2, "re-importing same backup must be idempotent");
}

// ─── 10. create_time + modify_time + marker preserved (da-1el) ────────

#[test]
fn import_preserves_create_and_modify_times_and_marker() {
    let db = fresh_db();
    let create_iso = "2025-01-08T23:53:08.569Z";
    let modify_iso = "2025-03-15T10:00:00.000Z";
    let f = write_backup(serde_json::json!([{
        "_id": "brushteeth:2025-01-08T23:53:08.259Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T23:53:08.259Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "brushteeth"
        },
        "meta": {
            "humanId": "abc",
            "createTime": {"utc": create_iso, "o": 1, "tz": "Europe/Berlin"},
            "modifyTime": {"utc": modify_iso, "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &db.all_entries().unwrap()[0];
    let expect_create: chrono::DateTime<chrono::Utc> = create_iso.parse().unwrap();
    let expect_modify: chrono::DateTime<chrono::Utc> = modify_iso.parse().unwrap();
    assert_eq!(e.create_time, expect_create, "create_time from meta.createTime");
    assert_eq!(e.modify_time, expect_modify, "modify_time from meta.modifyTime");
    assert_ne!(e.create_time, e.modify_time, "distinct source times must land distinctly");

    let marker = &e.extra_metadata.as_ref().unwrap()["_import"];
    assert_eq!(marker["source"], serde_json::json!("datum-v2-couchdb"));
    // backup_time comes from the backup file's `backupTime` field (set by
    // write_backup to a fixed value), so it's stable across re-imports.
    assert_eq!(
        marker["backup_time"],
        serde_json::json!("2025-02-02T12:26:32.488Z"),
    );
    assert_eq!(
        marker["original_id"],
        serde_json::json!("brushteeth:2025-01-08T23:53:08.259Z"),
    );
}

// ─── 11. modify_time falls back to create_time when source omits it ───

#[test]
fn import_falls_back_to_create_time_when_modify_time_missing() {
    let db = fresh_db();
    let create_iso = "2025-01-08T23:53:08.569Z";
    let f = write_backup(serde_json::json!([{
        "_id": "brushteeth:2025-01-08T23:53:08.259Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T23:53:08.259Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "brushteeth"
        },
        "meta": {
            "humanId": "abc",
            "createTime": {"utc": create_iso, "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["migrate", "import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &db.all_entries().unwrap()[0];
    let expect_create: chrono::DateTime<chrono::Utc> = create_iso.parse().unwrap();
    assert_eq!(e.create_time, expect_create);
    assert_eq!(
        e.modify_time, expect_create,
        "modify_time falls back to create_time, never to a fresh now()",
    );
}
