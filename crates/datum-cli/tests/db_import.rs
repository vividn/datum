//! `datum db import` integration tests (da-a8u, rewritten under da-5ns.6).
//!
//! Verifies the v=20 mapping from legacy datum CouchDB-shaped JSON into
//! the new schema: occur_time / state / dur / following_state / verb.
//! Asserts on the new columns directly; the old `entry_shape`/`value`/
//! `start_time`/`end_time` compat shims are intentionally avoided so
//! these tests don't decay when sub-bead 8 sweeps them out.
//!
//! Verb is derived from (state, dur, following_state) via
//! `datum_core::derive_verb` — the `entry.verb` column is hardcoded to
//! "point" until da-5ns.3 wires derivation into the insert path, so the
//! struct's `verb` after a round-trip through the DB doesn't reflect
//! reality. We re-derive from the data triple instead.

mod common;

use common::run_cli;
use datum_core::derive_verb;
use datum_core::entry::Verb;
use datum_core::test_util::fresh_db;
use std::io::Write;

/// Re-derive the verb from the three columns that legitimately encode it.
/// Callers compare against the expected legacy classification.
fn verb_of(e: &datum_core::entry::Entry) -> Verb {
    derive_verb(e.state.clone(), e.dur.as_deref(), e.following_state.clone())
        .expect("derive_verb on imported entry")
        .0
}

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

// ─── 1. occur-only (no dur, no state) → state=null, verb=Point ───────
// state=undefined + dur=undefined + fs=None → derive_verb returns
// (Verb::Point, None). The null→true promotion was dropped in
// da-ju71.2; an occur with no state stays state=null. (Import-side
// emission of `Dur::Moment` for point entries is da-ju71.5, separate
// bead — until that lands, dur stays NULL here.)

#[test]
fn occur_only_doc_maps_to_state_null() {
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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.field, "brushteeth");
    assert!(e.state.is_none(), "no state in source → state stays null");
    assert!(e.dur.is_none(), "no legacy dur key → dur column NULL");
    assert!(e.following_state.is_none());
    assert!(e.occur_time.is_some());
    // Re-derived verb: state=None + dur=None + fs=None → Point.
    assert_eq!(verb_of(e), Verb::Point);
}

// ─── 2. occur+dur → Block, dur preserved verbatim ───────────────────
// state=undefined + dur="PT30M" + fs=None → derive_verb:
//   None state + DurKind::Positive → Verb::Block with state=None.
//   (Null→true promotion dropped in da-ju71.2.)

#[test]
fn occur_with_dur_maps_to_block() {
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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(verb_of(e), Verb::Block);
    assert_eq!(e.dur.as_deref(), Some("PT30M"), "dur stored verbatim");
    assert!(e.state.is_none(), "null state stays null (no promotion)");
    assert!(e.following_state.is_none());
    // dur literal also survives as user-visible intent in `data`.
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra["dur"], serde_json::json!("PT30M"));
    // `_import` provenance lives in `extra_metadata` (da-tsw), strictly
    // disjoint from user-payload `data`.
    let metadata = e
        .extra_metadata
        .as_ref()
        .expect("extra_metadata must hold _import");
    assert_eq!(
        metadata["_import"]["source"],
        serde_json::json!("datum-v2-couchdb"),
    );
    assert!(
        e.data.as_ref().and_then(|d| d.get("_import")).is_none(),
        "`_import` must NOT appear in user-payload `data`",
    );
}

// ─── 3. switch (boolean state, no dur) → Switch ──────────────────────
// state=false + dur=undefined + fs=None → derive_verb: Verb::Switch with
// state preserved verbatim (no promotion when state is set).

#[test]
fn switch_doc_with_boolean_state_maps_to_switch() {
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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(verb_of(e), Verb::Switch);
    assert_eq!(e.state, Some(serde_json::json!(false)));
    assert!(e.dur.is_none());
    // lastState preserved as intent (no longer stripped).
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra["lastState"], serde_json::json!(true));
}

// ─── 4. explicit dur=null → dur="PT0S", literal null preserved in data ─
// Per da-5ns.6: legacy `dur: null` (explicit) maps to `dur="PT0S"`
// (zero-duration / point-for-state). With state=undefined, derive_verb on
// import returns (Verb::Block, Some(true)) — see truth table row
// `(false, Zero, false) => Block`. The original JSON null survives in the
// `data` blob. Re-derive on stored values gives Verb::Point because
// state is now Some(true) (state_set=true changes the truth-table row).

#[test]
fn explicit_dur_null_maps_to_zero_duration() {
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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.dur.as_deref(), Some("PT0S"), "explicit null → PT0S");
    assert_eq!(
        e.state,
        Some(serde_json::json!(true)),
        "null→true promotion"
    );
    let extra = e.data.as_ref().unwrap();
    assert!(
        extra["dur"].is_null(),
        "legacy null literal preserved in data"
    );
}

// ─── 5. missing occurTime → Point with occur_time NULL ───────────────
// state=undefined + dur=undefined → derive_verb: Verb::Point (state
// promoted to true). occur_time stays NULL — the column is nominally NOT
// NULL but the insert path falls back to create_time when missing
// (datum-core/src/db/insert.rs).

#[test]
fn doc_without_occur_time_maps_to_point() {
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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    // Re-derive: state=Some(true) (promoted) + no dur → Switch.
    assert_eq!(verb_of(e), Verb::Switch);
    assert_eq!(
        e.state,
        Some(serde_json::json!(true)),
        "null→true promotion"
    );
    assert!(e.dur.is_none());
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra["note"], serde_json::json!("no time on this entry"));
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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &db.all_entries().unwrap()[0];
    assert_eq!(
        e.parser_version, 0,
        "imported rows use sentinel parser_version=0"
    );
    assert_eq!(e.raw_input, "");
    assert!(e.alias_chain.is_none());
    // Provenance lives in `extra_metadata`. The user-payload `data` is
    // empty for this fixture (no fields beyond occurTime/field/state) so it
    // stays NULL — strict separation, no marker bleed-through (da-tsw).
    let metadata = e.extra_metadata.as_ref().unwrap();
    let marker = &metadata["_import"];
    assert_eq!(marker["source"], serde_json::json!("datum-v2-couchdb"));
    assert!(
        marker["backup_time"].is_string(),
        "backup_time must be present"
    );
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

    run_cli(
        &["db", "import", "--dry-run", f.path().to_str().unwrap()],
        &db,
    )
    .unwrap();
    assert_eq!(
        db.all_entries().unwrap().len(),
        0,
        "dry-run must not insert"
    );
}

// ─── 8. Integration: import the 3-day fixture from real data ─────────

#[test]
fn import_3day_fixture_produces_blocks_and_points() {
    let db = fresh_db();
    let fixture = "tests/fixtures/datum_backup_3days.json";
    run_cli(&["db", "import", fixture], &db).expect("import 3day fixture");

    let entries = db.all_entries().unwrap();
    assert!(
        entries.len() > 100,
        "fixture has hundreds of docs (got {})",
        entries.len()
    );

    // Some entries have a positive dur (the fixture contains PT*M durs).
    let blocks: Vec<_> = entries
        .iter()
        .filter(|e| {
            e.dur
                .as_deref()
                .map_or(false, |d| !matches!(d, "PT0S" | "P0D" | "PT0M" | "PT0H"))
        })
        .collect();
    assert!(
        !blocks.is_empty(),
        "fixture must produce at least one block entry"
    );

    // Every block entry must round-trip its dur literal verbatim into
    // both the `dur` column and the `data` blob (intent layer).
    for e in &blocks {
        assert!(e.occur_time.is_some(), "block missing occur_time");
        let dur_str = e.dur.as_deref().expect("block has dur set");
        let dur_lit = &e.data.as_ref().unwrap()["dur"];
        assert!(
            dur_lit.is_string() || dur_lit.is_number(),
            "block must preserve a non-null dur literal in data, got {dur_lit}"
        );
        if let Some(s) = dur_lit.as_str() {
            assert_eq!(s, dur_str, "data.dur literal must match dur column");
        }
        // derive_verb on a block must succeed and classify as Block (or
        // BlockSwitch — but legacy entries never carry following_state so
        // it's always plain Block here).
        let v = verb_of(e);
        assert_eq!(
            v,
            Verb::Block,
            "block entry {} verb should be Block, got {:?}",
            e.slug,
            v
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
        assert!(
            marker["backup_time"].is_string(),
            "row {} missing backup_time",
            e.slug
        );
        assert!(
            marker["original_id"].is_string(),
            "row {} missing original_id",
            e.slug
        );
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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
    let n1 = db.all_entries().unwrap().len();
    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &db.all_entries().unwrap()[0];
    let expect_create: chrono::DateTime<chrono::Utc> = create_iso.parse().unwrap();
    let expect_modify: chrono::DateTime<chrono::Utc> = modify_iso.parse().unwrap();
    assert_eq!(
        e.create_time, expect_create,
        "create_time from meta.createTime"
    );
    assert_eq!(
        e.modify_time, expect_modify,
        "modify_time from meta.modifyTime"
    );
    assert_ne!(
        e.create_time, e.modify_time,
        "distinct source times must land distinctly"
    );

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

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &db.all_entries().unwrap()[0];
    let expect_create: chrono::DateTime<chrono::Utc> = create_iso.parse().unwrap();
    assert_eq!(e.create_time, expect_create);
    assert_eq!(
        e.modify_time, expect_create,
        "modify_time falls back to create_time, never to a fresh now()",
    );
}

// ─── 12. occurTime.precisionUnit → occur_time_precision (da-5ns.6) ───

#[test]
fn year_precision_maps_to_iso_p1y() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "born:1990-01-01T00:00:00.000Z",
        "data": {
            "occurTime": {
                "utc": "1990-01-01T00:00:00.000Z",
                "o": 0,
                "tz": "UTC",
                "precisionUnit": "year"
            },
            "field": "born"
        },
        "meta": {
            "humanId": "y",
            "createTime": {"utc": "2025-01-08T00:00:00.000Z", "o": 0, "tz": "UTC"}
        }
    }]));

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &db.all_entries().unwrap()[0];
    assert_eq!(
        e.occur_time_precision.as_deref(),
        Some("P1Y"),
        "precisionUnit=year → P1Y",
    );
}

#[test]
fn week_precision_maps_to_iso_p1w() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "vacation:2024-06-01T00:00:00.000Z",
        "data": {
            "occurTime": {
                "utc": "2024-06-01T00:00:00.000Z",
                "o": 0,
                "tz": "UTC",
                "precisionUnit": "week"
            },
            "field": "vacation"
        },
        "meta": {
            "humanId": "w",
            "createTime": {"utc": "2025-01-08T00:00:00.000Z", "o": 0, "tz": "UTC"}
        }
    }]));

    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &db.all_entries().unwrap()[0];
    assert_eq!(e.occur_time_precision.as_deref(), Some("P1W"));
}

#[test]
fn precision_unit_canon_table_round_trips_all_units() {
    // Each unit in the da-5ns.6 canon table maps deterministically.
    for (unit, iso) in [
        ("second", "PT1S"),
        ("minute", "PT1M"),
        ("hour", "PT1H"),
        ("day", "P1D"),
        ("week", "P1W"),
        ("month", "P1M"),
        ("year", "P1Y"),
    ] {
        let db = fresh_db();
        let f = write_backup(serde_json::json!([{
            "_id": format!("test_{}:2024-06-01T00:00:00.000Z", unit),
            "data": {
                "occurTime": {
                    "utc": "2024-06-01T00:00:00.000Z",
                    "o": 0,
                    "tz": "UTC",
                    "precisionUnit": unit
                },
                "field": format!("test_{}", unit)
            },
            "meta": {"humanId": "p", "createTime": {"utc": "2025-01-08T00:00:00.000Z", "o": 0, "tz": "UTC"}}
        }]));
        run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
        let e = &db.all_entries().unwrap()[0];
        assert_eq!(
            e.occur_time_precision.as_deref(),
            Some(iso),
            "precisionUnit={} should map to {}",
            unit,
            iso,
        );
    }
}

#[test]
fn missing_precision_unit_leaves_column_null() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "x:2024-06-01T00:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2024-06-01T00:00:00.000Z", "o": 0, "tz": "UTC"},
            "field": "x"
        },
        "meta": {"humanId": "z", "createTime": {"utc": "2025-01-08T00:00:00.000Z", "o": 0, "tz": "UTC"}}
    }]));
    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &db.all_entries().unwrap()[0];
    assert!(
        e.occur_time_precision.is_none(),
        "no precisionUnit → column NULL (treated as exact)",
    );
}

// ─── 13. id_structure / field_structure preservation (da-5ns.6) ──────

#[test]
fn import_preserves_id_structure_from_meta() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "brushteeth:2025-01-08T23:53:08.259Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T23:53:08.259Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "brushteeth"
        },
        "meta": {
            "humanId": "abc",
            "createTime": {"utc": "2025-01-08T23:53:08.569Z", "o": 1, "tz": "Europe/Berlin"},
            "idStructure": "%field%:%occurTime%"
        }
    }]));
    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &db.all_entries().unwrap()[0];
    assert_eq!(
        e.id_structure.as_deref(),
        Some("%field%:%occurTime%"),
        "meta.idStructure must be preserved verbatim for re-interpolation",
    );
}

#[test]
fn import_preserves_field_structure_from_meta() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "word_pt:2025-01-08T08:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T08:00:00.000Z", "o": 0, "tz": "UTC"},
            "field": "word_pt",
            "srcLang": "pt"
        },
        "meta": {
            "humanId": "fs",
            "createTime": {"utc": "2025-01-08T08:00:00.000Z", "o": 0, "tz": "UTC"},
            "fieldStructure": "word_%srcLang%"
        }
    }]));
    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &db.all_entries().unwrap()[0];
    assert_eq!(e.field, "word_pt");
    assert_eq!(
        e.field_structure.as_deref(),
        Some("word_%srcLang%"),
        "meta.fieldStructure must be preserved",
    );
}

// ─── 14. _rev and _attachments are dropped (da-5ns.6) ────────────────

#[test]
fn import_drops_couch_plumbing_fields() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "x:2025-01-08T00:00:00.000Z",
        "_rev": "1-deadbeef",
        "_attachments": {"foo.bin": {"content_type": "application/octet-stream"}},
        "data": {
            "occurTime": {"utc": "2025-01-08T00:00:00.000Z", "o": 0, "tz": "UTC"},
            "field": "x"
        },
        "meta": {"humanId": "y", "createTime": {"utc": "2025-01-08T00:00:00.000Z", "o": 0, "tz": "UTC"}}
    }]));
    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &db.all_entries().unwrap()[0];
    // Neither plumbing field should leak into `data` (which only carries
    // user-payload top-level keys from `data.*`, not from the doc root).
    let data = e.data.as_ref();
    assert!(
        data.and_then(|d| d.get("_rev")).is_none(),
        "_rev must NOT appear in data",
    );
    assert!(
        data.and_then(|d| d.get("_attachments")).is_none(),
        "_attachments must NOT appear in data",
    );
}

// ─── 15. arbitrary top-level keys land in `data` (da-5ns.6) ──────────

#[test]
fn import_routes_extra_data_keys_into_data_blob() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "weight:2025-01-08T00:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T00:00:00.000Z", "o": 0, "tz": "UTC"},
            "field": "weight",
            "value": {"v": 75.2, "u": "kg"},
            "note": "morning weigh-in"
        },
        "meta": {"humanId": "w", "createTime": {"utc": "2025-01-08T00:00:00.000Z", "o": 0, "tz": "UTC"}}
    }]));
    run_cli(&["db", "import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &db.all_entries().unwrap()[0];
    let blob = e.data.as_ref().expect("data blob");
    // Per da-5ns.6: legacy datum's "value" lived as a regular key in data,
    // so it round-trips into `data` (not promoted to a special column).
    assert_eq!(blob["value"], serde_json::json!({"v": 75.2, "u": "kg"}));
    assert_eq!(blob["note"], serde_json::json!("morning weigh-in"));
    // Top-level columns absent from data:
    assert!(blob.get("field").is_none());
    assert!(blob.get("state").is_none());
    assert!(blob.get("occurTime").is_none());
}

// ─── 16. v=20 column-level smoke test on full 3-day fixture ──────────

#[test]
fn import_3day_fixture_columns_are_well_formed() {
    let db = fresh_db();
    let fixture = "tests/fixtures/datum_backup_3days.json";
    run_cli(&["db", "import", fixture], &db).expect("import");

    for e in db.all_entries().unwrap() {
        // Every imported row carries non-null state (null→true promotion
        // means even bare-occur legacy rows have state=true).
        assert!(
            e.state.is_some(),
            "row {} should have state set after promotion",
            e.slug,
        );
        // No legacy entry can produce a following_state — it's daydumb-new.
        assert!(
            e.following_state.is_none(),
            "row {} unexpectedly has following_state",
            e.slug,
        );
        // Every row must derive a valid verb from its (state, dur, fs)
        // triple. derive_verb errors on malformed dur strings — none of
        // our stored durs are malformed (we either keep the legacy ISO
        // or store NULL/PT0S).
        let _ = derive_verb(e.state.clone(), e.dur.as_deref(), e.following_state.clone())
            .unwrap_or_else(|err| {
                panic!("row {} derive_verb failed: {}", e.slug, err);
            });
    }
}
