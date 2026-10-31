//! `datum db v2import` integration tests (da-a8u, rewritten under da-5ns.6).
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

use crate::common;

use common::run_cli;
use datum_core::derive_verb;
use datum_core::entry::Verb;
use datum_core::test_util::fresh_db;
use std::io::Write;

/// Re-derive the verb from the three columns that legitimately encode it.
/// Callers compare against the expected legacy classification.
fn verb_of(e: &datum_core::entry::Entry) -> Verb {
    derive_verb(e.state.clone(), e.dur.as_ref(), e.following_state.clone())
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
// emission of `Dur::moment()` for point entries is da-ju71.5, separate
// bead — until that lands, dur stays NULL here.)

#[test]
fn occur_only_doc_maps_to_state_null() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee"
        },
        "meta": {
            "humanId": "abc123",
            "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = datum_core::test_util::logged_all(&db);
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.field, "coffee");
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

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = datum_core::test_util::logged_all(&db);
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(verb_of(e), Verb::Block);
    assert_eq!(
        e.dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("PT30M"),
        "dur stored verbatim"
    );
    assert!(e.state.is_none(), "null state stays null (no promotion)");
    assert!(e.following_state.is_none());
    // The dur lives in its column alone; until 10-08 its literal was copied
    // into `data` too (vividn: a key v3 parses into a column stays out of data).
    assert!(e.data.is_none(), "dur is a column, not data: {:?}", e.data);
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
        "_id": "read:2025-01-08T07:10:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:10:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "read",
            "state": false,
            "lastState": true
        },
        "meta": {
            "humanId": "ok",
            "createTime": {"utc": "2025-01-08T07:10:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = datum_core::test_util::logged_all(&db);
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(verb_of(e), Verb::Switch);
    assert_eq!(e.state, Some(serde_json::json!(false)));
    assert!(e.dur.is_none());
    // lastState is dropped (changed 10-08, da-w94vu): v2 required it for
    // CouchDB's map/reduce views, so it was never asserted by the person —
    // it goes neither to data (where it was kept) nor to
    // user_asserted_last_state.
    assert!(e.data.is_none(), "lastState left in data: {:?}", e.data);
    assert!(e.user_asserted_last_state.is_none());
}

// ─── 4. explicit dur=null → dur=MOMENT, nothing left in data ────────
// Changed 10-08 (vividn, da-w94vu): legacy `dur: null` was a state held for
// a moment, so it imports as MOMENT — what `switch … dur=MOMENT` stores —
// and renders as one. It used to store PT0S (da-5ns.6, from before v3 had
// MOMENT) and keep the literal null in `data`; a key that becomes a column
// no longer stays in data, and lastState is dropped (see test 3).

#[test]
fn explicit_dur_null_maps_to_moment() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "mood:2025-01-08T07:05:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:05:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "mood",
            "state": "startled",
            "dur": serde_json::Value::Null,
            "lastState": "happy"
        },
        "meta": {
            "humanId": "n",
            "createTime": {"utc": "2025-01-08T07:05:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = datum_core::test_util::logged_all(&db);
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(
        e.dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("MOMENT"),
        "explicit null → MOMENT"
    );
    assert_eq!(verb_of(e), Verb::Point);
    assert_eq!(e.state, Some(serde_json::json!("startled")));
    assert!(e.user_asserted_last_state.is_none());
    assert!(e.data.is_none(), "column keys left in data: {:?}", e.data);
}

// ─── 4b. v2 moment of the state in force is not a repeat ─────────────
// da-0sjt8 (vividn, 2026-10-12): ten years of v2 data log «ran 10k» as
// `switch body run -m distanceKm=10` while running — a `dur: null` doc
// whose state is the one in force. It imports as MOMENT and `check`
// reads it as data of that state, so the history needs no migration.

#[test]
fn v2_same_state_moment_is_not_a_repeat() {
    let db = fresh_db();
    let doc = |at: &str, state: &str, extra: serde_json::Value| {
        let mut data = serde_json::json!({
            "occurTime": {"utc": at, "o": 1, "tz": "Europe/Berlin"},
            "field": "body",
            "state": state,
        });
        data.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::json!({
            "_id": format!("body:{at}"),
            "data": data,
            "meta": {"createTime": {"utc": at, "o": 1, "tz": "Europe/Berlin"}},
        })
    };
    let f = write_backup(serde_json::json!([
        doc("2025-01-08T07:30:00.000Z", "run", serde_json::json!({})),
        doc(
            "2025-01-08T08:40:00.000Z",
            "run",
            serde_json::json!({"dur": null, "distanceKm": 10, "lastState": "run"}),
        ),
        doc("2025-01-08T08:40:01.000Z", "stretch", serde_json::json!({})),
    ]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    assert_eq!(datum_core::test_util::logged_all(&db).len(), 3);
    let hits = datum_core::integrity::list_repeated_states(db.conn()).unwrap();
    assert!(hits.is_empty(), "v2 moment flagged as a repeat: {hits:?}");
}

// ─── 5. missing occurTime → Point with occur_time NULL ───────────────
// state=undefined + dur=undefined → derive_verb: Verb::Point with
// state=null (promotion dropped in da-ju71.2). occur_time stays NULL —
// the column is nominally NOT NULL but the insert path falls back to
// create_time when missing (datum-core/src/db/insert.rs).

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

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = datum_core::test_util::logged_all(&db);
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    // Re-derive: state=None + no dur + no fs → Point.
    assert_eq!(verb_of(e), Verb::Point);
    assert!(e.state.is_none(), "null state stays null (no promotion)");
    assert!(e.dur.is_none());
    let extra = e.data.as_ref().unwrap();
    assert_eq!(extra["note"], serde_json::json!("no time on this entry"));
}

// ─── 6. v2 intent fields are set on imported rows ────────────────────

#[test]
fn imported_rows_carry_sentinel_intent_fields() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee"
        },
        "meta": {
            "humanId": "abc",
            "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(
        e.parser_version, 0,
        "imported rows use sentinel parser_version=0"
    );
    assert_eq!(e.raw_input, None);
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
        serde_json::json!("coffee:2025-01-08T07:00:00.000Z"),
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
            "_id": "coffee:2025-01-08T07:00:00.000Z",
            "data": {
                "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "UTC"},
                "field": "coffee"
            },
            "meta": {"humanId": "b", "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "UTC"}}
        }
    ]));

    run_cli(
        &["db", "v2import", "--dry-run", f.path().to_str().unwrap()],
        &db,
    )
    .unwrap();
    assert_eq!(
        datum_core::test_util::logged_all(&db).len(),
        0,
        "dry-run must not insert"
    );
}

// ─── 8. Integration: import the 3-day fixture from real data ─────────

#[test]
fn import_3day_fixture_produces_blocks_and_points() {
    let db = fresh_db();
    let fixture = "tests/fixtures/datum_backup_3days.json";
    run_cli(&["db", "v2import", fixture], &db).expect("import 3day fixture");

    let entries = datum_core::test_util::logged_all(&db);
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
                .as_ref()
                .is_some_and(|d| !d.is_zero_span() && !d.is_moment() && !d.is_unknown())
        })
        .collect();
    assert!(
        !blocks.is_empty(),
        "fixture must produce at least one block entry"
    );

    for e in &blocks {
        assert!(e.occur_time.is_some(), "block missing occur_time");
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
    // `extra_metadata`, never in user-payload `data` (da-tsw) — and its
    // `data` holds no key v3 keeps in a column (da-w94vu).
    for e in &entries {
        for key in ["field", "state", "occurTime", "dur", "lastState"] {
            assert!(
                e.data.as_ref().and_then(|d| d.get(key)).is_none(),
                "row {} kept column key {key} in data",
                e.slug,
            );
        }
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
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "UTC"},
            "field": "coffee"
        },
        "meta": {"humanId": "a", "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "UTC"}}
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let n1 = datum_core::test_util::logged_all(&db).len();
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let n2 = datum_core::test_util::logged_all(&db).len();
    assert_eq!(n1, n2, "re-importing same backup must be idempotent");
}

// ─── 10. create_time + modify_time + marker preserved (da-1el) ────────

#[test]
fn import_preserves_create_and_modify_times_and_marker() {
    let db = fresh_db();
    let create_iso = "2025-01-08T07:00:00.500Z";
    let modify_iso = "2025-03-15T10:00:00.000Z";
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee"
        },
        "meta": {
            "humanId": "abc",
            "createTime": {"utc": create_iso, "o": 1, "tz": "Europe/Berlin"},
            "modifyTime": {"utc": modify_iso, "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &datum_core::test_util::logged_all(&db)[0];
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
        serde_json::json!("coffee:2025-01-08T07:00:00.000Z"),
    );
}

// ─── 11. modify_time falls back to create_time when source omits it ───

#[test]
fn import_falls_back_to_create_time_when_modify_time_missing() {
    let db = fresh_db();
    let create_iso = "2025-01-08T07:00:00.500Z";
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee"
        },
        "meta": {
            "humanId": "abc",
            "createTime": {"utc": create_iso, "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &datum_core::test_util::logged_all(&db)[0];
    let expect_create: chrono::DateTime<chrono::Utc> = create_iso.parse().unwrap();
    assert_eq!(e.create_time, expect_create);
    assert_eq!(
        e.modify_time, expect_create,
        "modify_time falls back to create_time, never to a fresh now()",
    );
}

// ─── 12. occurTime.precisionUnit → occur_time_uncertainty (da-5ns.6) ───

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

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(
        e.occur_uncertainty(),
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

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(e.occur_uncertainty(), Some("P1W"));
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
        run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
        let e = &datum_core::test_util::logged_all(&db)[0];
        assert_eq!(
            e.occur_uncertainty(),
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
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert!(
        e.occur_uncertainty().is_none(),
        "no precisionUnit → column NULL (treated as exact)",
    );
}

// ─── 13. id_structure / field_structure preservation (da-5ns.6) ──────

/// A one-doc backup of `coffee` at 07:00 UTC with `meta.idStructure`
/// set to `id_structure`.
fn coffee_with_id_structure(id_structure: &str) -> tempfile::NamedTempFile {
    write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee",
            "kind": "flat"
        },
        "meta": {
            "humanId": "abc",
            "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "Europe/Berlin"},
            "idStructure": id_structure
        }
    }]))
}

#[test]
fn import_preserves_id_structure_from_meta() {
    for tmpl in [
        "%occurTime%__%kind%",
        "%occurTime%:%field%",
        "%kind%_%field%_x",
    ] {
        let db = fresh_db();
        let f = coffee_with_id_structure(tmpl);
        run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
        let e = &datum_core::test_util::logged_all(&db)[0];
        assert_eq!(e.id_structure.as_deref(), Some(tmpl), "kept verbatim");
        assert!(
            e.extra_metadata.as_ref().unwrap()["_import"]
                .get("original_id_structure")
                .is_none(),
            "nothing stripped, nothing recorded: {tmpl}",
        );
    }
}

/// da-ipwqb: datum v2 kept the field in its id template; v3's slug
/// prefixes it implicitly, so `%field%:` is stripped and the original kept
/// on the `_import` marker.
#[test]
fn import_strips_a_leading_field_from_id_structure() {
    let db = fresh_db();
    let f = coffee_with_id_structure("%field%:%occurTime%");
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(e.id_structure.as_deref(), Some("%occurTime%"));
    assert_eq!(
        e.extra_metadata.as_ref().unwrap()["_import"]["original_id_structure"],
        "%field%:%occurTime%",
    );

    run_cli(&["update", "abc", "kind=espresso"], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(e.slug.as_str(), "coffee:2025-01-08T07:00:00.000Z");
}

/// A row imported before da-ipwqb still holds `%field%:%occurTime%`; its
/// first edit slugs it with the field once and writes the template back
/// without the lead, in the edit's own op.
#[test]
fn a_row_imported_with_a_leading_field_loses_it_at_its_first_edit() {
    let db = fresh_db();
    let f = coffee_with_id_structure("%field%:%occurTime%");
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    db.conn()
        .execute(
            "UPDATE entries SET id_structure = '%field%:%occurTime%' WHERE hid = 'abc'",
            [],
        )
        .unwrap();

    run_cli(&["update", "abc", "kind=espresso"], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(e.slug.as_str(), "coffee:2025-01-08T07:00:00.000Z");
    assert_eq!(e.id_structure.as_deref(), Some("%occurTime%"));
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
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
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
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
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
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
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
    run_cli(&["db", "v2import", fixture], &db).expect("import");

    for e in datum_core::test_util::logged_all(&db) {
        // No legacy entry can produce a following_state — it's daydumb-new.
        assert!(
            e.following_state.is_none(),
            "row {} unexpectedly has following_state",
            e.slug,
        );
        // Every row must derive a valid verb from its (state, dur, fs)
        // triple. derive_verb errors on malformed dur strings — none of
        // our stored durs are malformed (we either keep the legacy ISO
        // or store NULL/MOMENT).
        let _ = derive_verb(e.state.clone(), e.dur.as_ref(), e.following_state.clone())
            .unwrap_or_else(|err| {
                panic!("row {} derive_verb failed: {}", e.slug, err);
            });
    }
}

// ─── 17. --skip-log writes triage-able list of skipped IDs (da-9oj2) ─

#[test]
fn skip_log_records_duplicates_and_no_field_docs() {
    let db = fresh_db();
    // Two docs: one valid, one missing `field` (must be skipped as
    // `no-field` on first import).
    let f = write_backup(serde_json::json!([
        {
            "_id": "coffee:2025-01-08T07:00:00.000Z",
            "data": {
                "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "UTC"},
                "field": "coffee"
            },
            "meta": {"humanId": "a", "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "UTC"}}
        },
        {
            "_id": "orphan:no-field-here",
            "data": {
                "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "UTC"}
            },
            "meta": {"humanId": "b", "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "UTC"}}
        }
    ]));

    let log = tempfile::NamedTempFile::new().unwrap();
    let log_path = log.path().to_str().unwrap().to_string();
    // First import: 1 valid, 1 no-field skip.
    run_cli(
        &[
            "db",
            "v2import",
            "--skip-log",
            &log_path,
            f.path().to_str().unwrap(),
        ],
        &db,
    )
    .unwrap();
    let body = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        body.contains("no-field\torphan:no-field-here"),
        "skip log must list no-field doc by _id; got:\n{body}",
    );
    assert!(
        !body.contains("duplicate"),
        "no duplicates on first import; got:\n{body}",
    );

    // Second import: original now dedups, no-field still skipped.
    run_cli(
        &[
            "db",
            "v2import",
            "--skip-log",
            &log_path,
            f.path().to_str().unwrap(),
        ],
        &db,
    )
    .unwrap();
    let body = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        body.contains("duplicate\tcoffee:2025-01-08T07:00:00.000Z"),
        "re-import must record dedup skip by source _id; got:\n{body}",
    );
    assert!(
        body.contains("no-field\torphan:no-field-here"),
        "no-field skip must persist across re-import; got:\n{body}",
    );
}

#[test]
fn skip_log_includes_header_and_uses_synthetic_id_when_missing() {
    let db = fresh_db();
    // No `_id` and no `field` — both no-field skip AND synthetic-id path.
    let f = write_backup(serde_json::json!([{
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 0, "tz": "UTC"}
        },
        "meta": {"humanId": "a", "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 0, "tz": "UTC"}}
    }]));

    let log = tempfile::NamedTempFile::new().unwrap();
    let log_path = log.path().to_str().unwrap().to_string();
    run_cli(
        &[
            "db",
            "v2import",
            "--skip-log",
            &log_path,
            f.path().to_str().unwrap(),
        ],
        &db,
    )
    .unwrap();
    let body = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        body.starts_with("# datum db v2import skip log"),
        "got:\n{body}"
    );
    assert!(
        body.contains("<missing-_id at #0>"),
        "synthetic id used when source has no _id; got:\n{body}",
    );
}

// ─── 18. meta.humanId → entry.hid (da-o7o3) ───────────────────────────
// During a parallel-run migration, the user wants `dm get <humanId>` to
// resolve the same conceptual doc in both the legacy datum store and
// daydumb. Importer must lift `meta.humanId` into the entry's `hid`
// column rather than generating a fresh one.

#[test]
fn import_preserves_source_humanid_as_hid() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee"
        },
        "meta": {
            "humanId": "abc123def456",
            "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(
        e.hid, "abc123def456",
        "entry.hid must mirror source meta.humanId",
    );
    // Quick-id lookup by full and prefix forms resolves the imported row.
    // Six characters, not three: the auto-created `spec:coffee` has a random
    // hid, which shares a 3-char prefix about 1 run in 47k (da-grlpj).
    let by_full = db.get_entry("abc123def456").expect("get by full humanId");
    assert_eq!(by_full.slug, e.slug);
    let by_prefix = db
        .resolve_prefix("abc123")
        .expect("resolve by humanId prefix");
    assert_eq!(by_prefix.slug, e.slug);
}

#[test]
fn import_without_humanid_generates_fresh_hid() {
    // Backwards-compat: legacy / corrupt docs lacking meta.humanId still
    // import cleanly with an auto-generated 12-char base36 hid.
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee"
        },
        "meta": {
            "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(
        e.hid.len(),
        datum_core::entry::HID_LEN,
        "missing humanId → freshly generated 12-char hid",
    );
    assert!(
        e.hid.chars().all(|c| c.is_ascii_alphanumeric()),
        "auto-generated hid stays base36",
    );
}

#[test]
fn import_with_empty_humanid_generates_fresh_hid() {
    // Empty string is treated as "no humanId" — same path as the key
    // being absent. Avoids surfacing a corrupt zero-length hid in the DB.
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee"
        },
        "meta": {
            "humanId": "",
            "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(e.hid.len(), datum_core::entry::HID_LEN);
    assert_ne!(e.hid, "", "empty humanId must not become an empty hid");
}

#[test]
fn import_collision_with_existing_row_errors_and_preserves_existing() {
    // Existing entry with hid=X exists in the DB; a fresh import declares
    // meta.humanId=X for a different content. Must error before any write,
    // leaving the existing row intact.
    use datum_core::test_util::entry_builder;

    let db = fresh_db();
    let preexisting = entry_builder("coffee")
        .verb(Verb::Point)
        .state(serde_json::json!(null))
        .insert(&db);
    let stolen_hid = preexisting.hid.clone();

    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee"
        },
        "meta": {
            "humanId": stolen_hid.as_str(),
            "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "Europe/Berlin"}
        }
    }]));

    let err = run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db)
        .expect_err("collision must surface as error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("hid collision"),
        "error message must mention the collision; got: {msg}",
    );
    assert!(
        msg.contains(stolen_hid.as_str()),
        "error message must include the colliding hid; got: {msg}",
    );
    assert!(
        msg.contains("coffee:2025-01-08T07:00:00.000Z"),
        "error message must name the source doc that triggered it; got: {msg}",
    );

    // Existing row untouched, importing row never landed.
    let entries = datum_core::test_util::logged_all(&db);
    assert_eq!(entries.len(), 1, "no new row written on collision");
    assert_eq!(entries[0].slug, preexisting.slug);
    assert_eq!(entries[0].hid, preexisting.hid);
}

#[test]
fn import_in_batch_humanid_duplicate_with_distinct_content_errors() {
    // Two source docs declare the same humanId but produce different
    // content. Must error — even if neither row currently exists in the
    // target DB, only one of them can claim the hid.
    let db = fresh_db();
    let f = write_backup(serde_json::json!([
        {
            "_id": "coffee:2025-01-08T08:00:00.000Z",
            "data": {
                "occurTime": {"utc": "2025-01-08T08:00:00.000Z", "o": 0, "tz": "UTC"},
                "field": "coffee"
            },
            "meta": {"humanId": "dup", "createTime": {"utc": "2025-01-08T08:00:00.500Z", "o": 0, "tz": "UTC"}}
        },
        {
            "_id": "walk:2025-01-08T09:00:00.000Z",
            "data": {
                "occurTime": {"utc": "2025-01-08T09:00:00.000Z", "o": 0, "tz": "UTC"},
                "field": "walk"
            },
            "meta": {"humanId": "dup", "createTime": {"utc": "2025-01-08T09:00:00.500Z", "o": 0, "tz": "UTC"}}
        }
    ]));
    let err = run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db)
        .expect_err("in-batch dup with distinct content must error");
    let msg = format!("{err:#}");
    assert!(msg.contains("hid collision"), "got: {msg}");
    assert!(msg.contains("dup"), "got: {msg}");
    // Nothing should have landed.
    assert_eq!(datum_core::test_util::logged_all(&db).len(), 0);
}

#[test]
fn import_reimport_with_same_humanid_and_same_content_is_idempotent() {
    // Re-importing the same backup file: every doc has the same humanId AND
    // the same content_hash, so the bulk path's content_hash dedup absorbs
    // them. The pre-flight collision check must NOT trip here (same content
    // is benign).
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "UTC"},
            "field": "coffee"
        },
        "meta": {"humanId": "stable42", "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1, "tz": "UTC"}}
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let after_first = datum_core::test_util::logged_all(&db);
    assert_eq!(after_first.len(), 1);
    assert_eq!(after_first[0].hid, "stable42");

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db)
        .expect("re-import must be idempotent, not a collision error");
    let after_second = datum_core::test_util::logged_all(&db);
    assert_eq!(
        after_second.len(),
        1,
        "still exactly one row after re-import"
    );
    assert_eq!(after_second[0].hid, "stable42");
}

// ─── da-vfbq: an absent `tz` is unknown, not UTC ─────────────────────
// Legacy datum types both `o` and `tz` as optional, and older docs carry
// the offset alone. Defaulting the zone to "UTC" beside a non-zero offset
// asserts a zone the backup never contained *and* contradicts the offset
// stored next to it, so `datum check tz` flagged every non-UTC row.

#[test]
fn import_offset_without_tz_leaves_the_zone_unset() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "coffee:2025-01-08T07:00:00.000Z",
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1},
            "field": "coffee"
        },
        "meta": {
            "humanId": "notz01",
            "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 1}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = datum_core::test_util::logged_all(&db);
    assert_eq!(entries.len(), 1);
    let occur = entries[0].occur_time.as_ref().expect("occur_time");
    assert_eq!(occur.offset_minutes, 60, "the offset the backup did carry");
    assert_eq!(
        occur.timezone, None,
        "no `tz` key in the source → no zone claim in the row",
    );
    // The offset still drives local time: 07:00Z at UTC+1 is 08:00 local.
    assert_eq!(occur.local_time().to_rfc3339(), "2025-01-08T08:00:00+01:00");

    // ...and the pair no longer contradicts itself, so `check tz` is clean.
    let report =
        datum_commands::commands::check::collect_issues(&db, &db.list_including_deleted().unwrap())
            .unwrap();
    assert!(
        !report
            .issues
            .iter()
            .any(|i| i.category == "tz_offset_mismatch"),
        "an offset with no zone is not a mismatch: {:?}",
        report.issues,
    );
}

/// da-kvosn: v2 names a fixed offset `UTC+2`. The importer keeps it as
/// written, and it resolves as the offset it says, so `check` does not read
/// a correct offset as a mismatch.
#[test]
fn import_keeps_a_utc_offset_zone_and_check_reads_it() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "walk:2024-06-18T20:42:00.000Z",
        "data": {
            "occurTime": {"utc": "2024-06-18T20:42:00.000Z", "o": 2, "tz": "UTC+2"},
            "field": "walk"
        },
        "meta": {
            "humanId": "utc2ab",
            "createTime": {"utc": "2024-06-18T20:42:00.500Z", "o": 2, "tz": "UTC+2"}
        }
    }]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = datum_core::test_util::logged_all(&db);
    let occur = entries[0].occur_time.as_ref().expect("occur_time");
    assert_eq!(
        occur.timezone.as_deref(),
        Some("UTC+2"),
        "imported as written"
    );
    assert_eq!(occur.offset_minutes, 120);
    assert_eq!(occur.expected_offset_minutes().unwrap(), Some(120));
    let report =
        datum_commands::commands::check::collect_issues(&db, &db.list_including_deleted().unwrap())
            .unwrap();
    assert!(
        !report
            .issues
            .iter()
            .any(|i| ["tz_offset_mismatch", "unknown_timezone"].contains(&i.category)),
        "UTC+2 at +120 is consistent: {:?}",
        report.issues,
    );
}

#[test]
fn import_preserves_a_half_hour_offset() {
    // `o` is signed *hours* and may be fractional (India +5.5, Nepal +5.75,
    // Chatham +12.75). It must round to minutes, not truncate to whole
    // hours — nothing else in the suite pins this.
    let db = fresh_db();
    let f = write_backup(serde_json::json!([
        {
            "_id": "chai:2025-01-08T07:00:00.000Z",
            "data": {
                "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 5.5},
                "field": "chai"
            },
            "meta": {"humanId": "half01", "createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 5.5}}
        },
        {
            "_id": "momo:2025-01-08T08:00:00.000Z",
            "data": {
                "occurTime": {"utc": "2025-01-08T08:00:00.000Z", "o": 5.75, "tz": "Asia/Kathmandu"},
                "field": "momo"
            },
            "meta": {"humanId": "qtr01", "createTime": {"utc": "2025-01-08T08:00:00.500Z", "o": 5.75}}
        }
    ]));

    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();

    let entries = datum_core::test_util::logged_all(&db);
    let occur_of = |field: &str| {
        entries
            .iter()
            .find(|e| e.field == field)
            .and_then(|e| e.occur_time.clone())
            .unwrap_or_else(|| panic!("{field} imported"))
    };
    let chai = occur_of("chai");
    assert_eq!(chai.offset_minutes, 330, "+5:30 survives as 330 minutes");
    assert_eq!(chai.local_time().to_rfc3339(), "2025-01-08T12:30:00+05:30");
    assert_eq!(occur_of("momo").offset_minutes, 345, "+5:45 → 345 minutes");
}

// ─── da-dvyga: timeless and date-only docs are not duplicates ───────
// v2 writes `-D` (and `-d`/`-y` with no time) as `{utc: "YYYY-MM-DD"}`,
// and `-T` with no occurTime at all. Both used to import with no occur
// time and a slug stamped with the import's clock, so all but one per
// field per millisecond collided and were counted as «duplicate».

fn skip_log_lines(docs: &tempfile::NamedTempFile, db: &datum_core::db::Db) -> Vec<String> {
    skipped_by(docs, db, false)
}

fn slugs(db: &datum_core::db::Db) -> Vec<String> {
    let mut s: Vec<String> = datum_core::test_util::logged_all(db)
        .iter()
        .map(|e| e.slug.to_string())
        .collect();
    s.sort();
    s
}

#[test]
fn date_only_docs_import_as_whole_days() {
    let db = fresh_db();
    let docs: Vec<_> = ["07", "08", "09"]
        .iter()
        .map(|d| {
            serde_json::json!({
                "_id": format!("3goals:2025-01-{d}"),
                "data": {"field": "3goals", "occurTime": {"utc": format!("2025-01-{d}")}},
                "meta": {
                    "humanId": format!("goal{d}"),
                    "createTime": {"utc": format!("2025-01-{d}T21:00:00.000Z"), "o": 1, "tz": "Europe/Berlin"}
                }
            })
        })
        .collect();
    let f = write_backup(serde_json::Value::Array(docs));

    assert!(skip_log_lines(&f, &db).is_empty(), "nothing skipped");
    assert_eq!(
        slugs(&db),
        [
            "3goals:2025-01-07",
            "3goals:2025-01-08",
            "3goals:2025-01-09"
        ]
    );
    for e in datum_core::test_util::logged_all(&db) {
        assert_eq!(
            e.occur_uncertainty(),
            Some("P1D"),
            "{} is a whole day",
            e.slug
        );
        let t = e.occur_time.as_ref().unwrap();
        assert_eq!(t.offset_minutes, 60, "midnight in the createTime's zone");
        assert_eq!(t.local_time().format("%H:%M").to_string(), "00:00");
    }

    // Re-importing the same file adds nothing: every row is a real duplicate.
    let again = skip_log_lines(&f, &db);
    assert_eq!(again.len(), 3);
    assert!(
        again.iter().all(|l| l.starts_with("duplicate\t")),
        "{again:?}"
    );
    assert_eq!(datum_core::test_util::logged_all(&db).len(), 3);
}

fn doc_occurring(id: &str, data: serde_json::Value) -> serde_json::Value {
    let mut full = serde_json::json!({"field": "seen"});
    full.as_object_mut()
        .unwrap()
        .extend(data.as_object().unwrap().clone());
    serde_json::json!({
        "_id": id,
        "data": full,
        "meta": {
            "humanId": id,
            "createTime": {"utc": "2025-03-03T12:00:00.000Z", "o": 0, "tz": "UTC"}
        }
    })
}

/// The lines a run lists as skipped, dry or real: the two read one vector.
fn skipped_by(f: &tempfile::NamedTempFile, db: &datum_core::db::Db, dry: bool) -> Vec<String> {
    let log = tempfile::NamedTempFile::new().unwrap();
    let log_path = log.path().to_str().unwrap();
    let mut args = vec![
        "db",
        "v2import",
        f.path().to_str().unwrap(),
        "--skip-log",
        log_path,
    ];
    if dry {
        args.push("--dry-run");
    }
    run_cli(&args, db).unwrap();
    std::fs::read_to_string(log_path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[test]
fn an_unreadable_occur_time_is_listed_and_not_moved_to_the_create_time() {
    let unreadable = [
        ("a-word", serde_json::json!("yesterdayish")),
        ("empty", serde_json::json!("")),
        ("bad-utc", serde_json::json!({"utc": "bad"})),
        ("null-utc", serde_json::json!({"utc": null})),
        ("no-utc", serde_json::json!({"o": 1})),
        ("number", serde_json::json!(1735714800)),
    ];
    // A timeless doc slugs on its create time, so these two need their own.
    let mut null_time = doc_occurring("null", serde_json::json!({"occurTime": null}));
    null_time["meta"]["createTime"]["utc"] = serde_json::json!("2025-03-04T12:00:00.000Z");
    let docs: Vec<_> = unreadable
        .iter()
        .map(|(id, occur)| doc_occurring(id, serde_json::json!({"occurTime": occur})))
        .chain([doc_occurring("absent", serde_json::json!({})), null_time])
        .collect();
    let f = write_backup(serde_json::Value::Array(docs));

    for dry in [true, false] {
        let db = fresh_db();
        let skipped = skipped_by(&f, &db, dry);
        assert_eq!(skipped.len(), unreadable.len(), "dry={dry}: {skipped:?}");
        for ((id, _), line) in unreadable.iter().zip(&skipped) {
            assert!(
                line.starts_with(&format!("invalid-shape\t{id}\toccurTime ")),
                "dry={dry}: {line}"
            );
        }
        let rows = datum_core::test_util::logged_all(&db);
        if dry {
            assert!(rows.is_empty(), "a dry run writes nothing");
            continue;
        }
        // Only the two that named no time are in, at their create time.
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows.iter().all(|e| e.occur_time.is_none()), "{rows:?}");
    }
}

#[test]
fn a_plain_string_occur_time_is_the_instant_v2_wrote() {
    let mut offset_in_meta = doc_occurring(
        "offset-in-meta",
        serde_json::json!({"occurTime": "2025-01-04T07:00:00.000Z"}),
    );
    offset_in_meta["meta"]["utcOffset"] = serde_json::json!(-5);
    let f = write_backup(serde_json::json!([
        doc_occurring(
            "offset-beside",
            serde_json::json!({"occurTime": "2025-01-01T07:00:00.000Z", "occurUtcOffset": 5.5}),
        ),
        doc_occurring(
            "no-offset",
            serde_json::json!({"occurTime": "2025-01-02T07:00:00.000Z"}),
        ),
        doc_occurring("a-date", serde_json::json!({"occurTime": "2025-01-03"})),
        offset_in_meta,
    ]));

    let dry = fresh_db();
    assert_eq!(skipped_by(&f, &dry, true), Vec::<String>::new());
    assert!(datum_core::test_util::logged_all(&dry).is_empty());

    let db = fresh_db();
    assert_eq!(skipped_by(&f, &db, false), Vec::<String>::new());
    let rows = datum_core::test_util::logged_all(&db);
    let row = |hid: &str| {
        rows.iter()
            .find(|e| e.hid.as_str() == hid)
            .unwrap_or_else(|| panic!("{hid} in {rows:?}"))
    };
    let at = |hid: &str| row(hid).occur_time.clone().expect("an occur time");

    let t = at("offset-beside");
    assert_eq!(t.utc.to_rfc3339(), "2025-01-01T07:00:00+00:00");
    assert_eq!(t.offset_minutes, 330);
    assert!(
        row("offset-beside").data.is_none(),
        "the offset has a column: {:?}",
        row("offset-beside").data
    );
    assert_eq!(at("offset-in-meta").offset_minutes, -300);

    let t = at("no-offset");
    assert_eq!(t.utc.to_rfc3339(), "2025-01-02T07:00:00+00:00");
    assert_eq!(t.offset_minutes, 0);

    assert_eq!(row("a-date").occur_uncertainty(), Some("P1D"));
    assert_eq!(
        at("a-date").local_time().format("%F %H:%M").to_string(),
        "2025-01-03 00:00"
    );
}

#[test]
fn timeless_docs_slug_on_their_own_create_time() {
    let db = fresh_db();
    let docs: Vec<_> = (1..=3)
        .map(|i| {
            serde_json::json!({
                "_id": format!("physicalbackup:c{i}"),
                "data": {"field": "physicalbackup", "nextDate": format!("2025-0{i}-01")},
                "meta": {
                    "humanId": format!("phys{i}"),
                    "idStructure": "%?createTime%c",
                    "createTime": {"utc": format!("2025-01-0{i}T10:00:00.000Z"), "o": 1, "tz": "Europe/Berlin"}
                }
            })
        })
        .collect();
    let f = write_backup(serde_json::Value::Array(docs));

    assert!(skip_log_lines(&f, &db).is_empty(), "nothing skipped");
    assert_eq!(
        slugs(&db),
        [
            "physicalbackup:2025-01-01T10:00:00.000Zc",
            "physicalbackup:2025-01-02T10:00:00.000Zc",
            "physicalbackup:2025-01-03T10:00:00.000Zc",
        ]
    );
    for e in datum_core::test_util::logged_all(&db) {
        assert!(e.occur_time.is_none());
        assert!(
            e.data.as_ref().unwrap()["nextDate"].is_string(),
            "{} keeps nextDate",
            e.slug
        );
    }

    let again = skip_log_lines(&f, &db);
    assert_eq!(again.len(), 3);
    assert!(
        again.iter().all(|l| l.starts_with("duplicate\t")),
        "{again:?}"
    );
    assert_eq!(datum_core::test_util::logged_all(&db).len(), 3);
}

/// The timing-dependent case: rows built within one millisecond shared
/// the import clock's slug.
#[test]
fn five_hundred_timeless_docs_import_as_five_hundred_rows() {
    let db = fresh_db();
    let docs: Vec<_> = (0..500)
        .map(|i| {
            serde_json::json!({
                "_id": format!("chore:{i}"),
                "data": {"field": "chore", "n": i},
                "meta": {"createTime": {"utc": format!("2025-01-01T00:00:{:02}.{:03}Z", i / 100, i % 100)}}
            })
        })
        .collect();
    let f = write_backup(serde_json::Value::Array(docs));
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    assert_eq!(datum_core::test_util::logged_all(&db).len(), 500);
}

/// Two docs on one slug with different content are not duplicates: the
/// second is skipped as a slug collision, naming both `_id`s.
#[test]
fn a_slug_collision_is_not_a_duplicate() {
    let db = fresh_db();
    let doc = |id: &str, state: &str| {
        serde_json::json!({
            "_id": id,
            "data": {"field": "mood", "state": state, "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 0}},
            "meta": {"createTime": {"utc": "2025-01-08T07:00:00.000Z"}}
        })
    };
    let f = write_backup(serde_json::json!([
        doc("first", "happy"),
        doc("second", "sad")
    ]));
    let skipped = skip_log_lines(&f, &db);
    assert_eq!(skipped.len(), 1, "{skipped:?}");
    let line = &skipped[0];
    assert!(line.starts_with("slug-collision\tsecond\t"), "{line}");
    assert!(
        line.contains("first"),
        "names the doc holding the slug: {line}"
    );
}

/// Import, delete, import again: the tombstone holds the slug. That is
/// neither a duplicate nor «other content», and the skip names the
/// deleted row (kimi-k3 on #682).
#[test]
fn a_slug_held_by_a_deleted_row_says_so() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "chore:1",
        "data": {"field": "chore"},
        "meta": {"createTime": {"utc": "2025-01-01T10:00:00.000Z"}}
    }]));
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    run_cli(&["delete", "chore:2025-01-01T10:00:00.000Zc"], &db).unwrap();
    assert!(datum_core::test_util::logged_all(&db)[0]
        .deleted_at
        .is_some());

    let skipped = skip_log_lines(&f, &db);
    assert_eq!(
        skipped,
        ["slug-collision\tchore:1\tslug chore:2025-01-01T10:00:00.000Zc held by deleted chore:1"]
    );
}

/// A createTime whose zone name does not parse still has its offset, and
/// a date-only doc takes midnight in it rather than in UTC.
#[test]
fn a_date_only_doc_keeps_the_offset_of_an_unknown_zone() {
    let db = fresh_db();
    let f = write_backup(serde_json::json!([{
        "_id": "3goals:2025-01-08",
        "data": {"field": "3goals", "occurTime": {"utc": "2025-01-08"}},
        "meta": {"createTime": {"utc": "2025-01-08T20:00:00.000Z", "o": 9, "tz": "Not/AZone"}}
    }]));
    run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(e.slug.to_string(), "3goals:2025-01-08");
    let t = e.occur_time.as_ref().unwrap();
    assert_eq!(t.offset_minutes, 540);
    assert_eq!(t.local_time().to_rfc3339(), "2025-01-08T00:00:00+09:00");
    assert_eq!(t.timezone, None, "a name that does not parse is not kept");
}

/// da-s9ngd: identical timeless docs logged on different days are different
/// events, each slugged on its own createTime; re-importing adds nothing.
#[test]
fn identical_timeless_docs_on_different_days_are_two_rows() {
    let db = fresh_db();
    let docs: Vec<_> = (1..=2)
        .map(|i| {
            serde_json::json!({
                "_id": format!("shirk:c{i}"),
                "data": {"field": "shirk"},
                "meta": {"createTime": {"utc": format!("2025-01-0{i}T10:00:00.000Z")}}
            })
        })
        .collect();
    let f = write_backup(serde_json::Value::Array(docs));

    assert!(skip_log_lines(&f, &db).is_empty(), "nothing skipped");
    assert_eq!(datum_core::test_util::logged_all(&db).len(), 2);

    let again = skip_log_lines(&f, &db);
    assert_eq!(again.len(), 2);
    assert!(
        again.iter().all(|l| l.starts_with("duplicate\t")),
        "{again:?}"
    );
    assert_eq!(datum_core::test_util::logged_all(&db).len(), 2);
}

// ─── v2's literal `c` beside its create time (da-hcxdx) ──────────────

/// A timeless `bPlants` doc under `id_structure`, its `_id` as v2 wrote it.
fn plants_with_id_structure(id_structure: &str, id: &str) -> tempfile::NamedTempFile {
    write_backup(serde_json::json!([{
        "_id": id,
        "data": {"field": "bPlants", "watered": true},
        "meta": {
            "humanId": "kz2ki",
            "createTime": {"utc": "2025-11-26T23:32:52.840Z", "o": 1, "tz": "Europe/Berlin"},
            "idStructure": id_structure
        }
    }]))
}

/// v3's `%create_time%` renders its own `c`, so v2's literal one would
/// double: `check` drifted on every such row, and a first edit would write
/// the doubled slug. A real database and binary, since `check`'s verdict is
/// its output.
#[test]
fn import_drops_v2s_literal_c_beside_create_time() {
    for (tmpl, id) in [
        (
            "%field%:%?createTime%c",
            "bPlants:2025-11-26T23:32:52.840Zc",
        ),
        ("%?createTime%c", "2025-11-26T23:32:52.840Zc"),
        (
            "%field%:c%?createTime%",
            "bPlants:c2025-11-26T23:32:52.840Z",
        ),
        ("c%?createTime%", "c2025-11-26T23:32:52.840Z"),
    ] {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("t.db");
        let backup = plants_with_id_structure(tmpl, id);
        let db = datum_core::db::Db::open(path.to_str().unwrap()).unwrap();
        run_cli(&["db", "v2import", backup.path().to_str().unwrap()], &db).unwrap();
        let check = || {
            let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
                .env("DATUM_DB", &path)
                .env("DATUM_HOME", dir.path())
                .env("DATUM_CONFIG_HOME", dir.path())
                .env("NO_COLOR", "1")
                .arg("check")
                .output()
                .unwrap();
            let printed = String::from_utf8_lossy(&out.stdout);
            assert!(
                out.status.success() && printed.contains("No consistency issues found"),
                "{tmpl}: {printed}"
            );
        };
        check();

        let e = &datum_core::test_util::logged_all(&db)[0];
        assert_eq!(
            e.slug.as_str(),
            "bPlants:2025-11-26T23:32:52.840Zc",
            "{tmpl}"
        );
        assert_eq!(e.id_structure.as_deref(), Some("%?createTime%"), "{tmpl}");
        assert_eq!(
            e.extra_metadata.as_ref().unwrap()["_import"]["original_id_structure"],
            tmpl,
        );

        let again = skip_log_lines(&backup, &db);
        assert_eq!(again.len(), 1, "{tmpl}");
        assert!(again[0].starts_with("duplicate\t"), "{tmpl}: {again:?}");

        // An edit keeps the slug, and a new row under the field's spec has
        // one `c`: the import writes the field no id template of v2's.
        run_cli(&["update", "kz2ki", "watered=false"], &db).unwrap();
        assert_eq!(
            db.get_entry("kz2ki").unwrap().slug.as_str(),
            "bPlants:2025-11-26T23:32:52.840Zc",
            "{tmpl}",
        );
        assert_eq!(db.load_spec("bPlants").unwrap().unwrap().id_template, None);
        run_cli(&["occur", "bPlants", "-T"], &db).unwrap();
        let new = datum_core::test_util::logged_all(&db)
            .into_iter()
            .find(|e| e.hid.as_str() != "kz2ki")
            .unwrap();
        let body = new.slug.as_str().strip_prefix("bPlants:").unwrap();
        assert!(
            body.ends_with("Zc") && !body.ends_with("cc"),
            "{tmpl}: {body}"
        );
        check();
    }
}

/// Only a `c` that is the create time's own goes, and nothing else moves.
#[test]
fn import_keeps_every_other_id_structure_beside_create_time() {
    for tmpl in [
        "%?createTime%cups",
        "abc%?createTime%",
        "%?createTime%",
        "%kind%c",
        "%field%",
    ] {
        let db = fresh_db();
        let f = coffee_with_id_structure(tmpl);
        run_cli(&["db", "v2import", f.path().to_str().unwrap()], &db).unwrap();
        let e = &datum_core::test_util::logged_all(&db)[0];
        assert_eq!(e.id_structure.as_deref(), Some(tmpl), "kept verbatim");
        assert!(
            e.extra_metadata.as_ref().unwrap()["_import"]
                .get("original_id_structure")
                .is_none(),
            "nothing stripped, nothing recorded: {tmpl}",
        );
    }
}

/// `check` on the db at `dir`/t.db finds nothing.
fn assert_check_clean(dir: &tempfile::TempDir, label: &str) {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("t.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path())
        .env("NO_COLOR", "1")
        .arg("check")
        .output()
        .unwrap();
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && printed.contains("No consistency issues found"),
        "{label}: {printed}"
    );
}

/// A doc at `07:00` with `kind`, slugged by `id_structure`.
fn coffee_doc(id: &str, human_id: &str, kind: &str, id_structure: &str) -> serde_json::Value {
    serde_json::json!({
        "_id": id,
        "data": {
            "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 1, "tz": "Europe/Berlin"},
            "field": "coffee",
            "kind": kind
        },
        "meta": {
            "humanId": human_id,
            "createTime": {"utc": "2025-01-08T09:30:00.500Z", "o": 1, "tz": "Europe/Berlin"},
            "idStructure": id_structure
        }
    })
}

/// da-pgns7: the slug is what the stored structure renders, whatever else
/// the doc holds, so an occurTime doc with a create-time id keeps the id v2
/// gave it, `check` agrees, an edit moves nothing and a re-import is a
/// duplicate.
#[test]
fn a_docs_slug_is_what_its_id_structure_renders() {
    for (tmpl, slug) in [
        ("%field%:%?createTime%c", "coffee:2025-01-08T09:30:00.500Zc"),
        (
            "%occurTime%__%kind%",
            "coffee:2025-01-08T07:00:00.000Z__flat",
        ),
        ("%kind%_%field%_x", "coffee:flat_coffee_x"),
    ] {
        let dir = tempfile::TempDir::new().unwrap();
        let db = datum_core::db::Db::open(dir.path().join("t.db").to_str().unwrap()).unwrap();
        let backup = write_backup(serde_json::json!([coffee_doc(slug, "abc", "flat", tmpl)]));
        run_cli(&["db", "v2import", backup.path().to_str().unwrap()], &db).unwrap();
        assert_eq!(slugs(&db), [slug], "{tmpl}");
        assert_check_clean(&dir, tmpl);

        run_cli(&["update", "abc", "milk=oat"], &db).unwrap();
        assert_eq!(slugs(&db), [slug], "{tmpl}: an edit keeps it");
        assert_check_clean(&dir, tmpl);
    }

    let db = fresh_db();
    let backup = write_backup(serde_json::json!([coffee_doc(
        "coffee:2025-01-08T09:30:00.500Zc",
        "abc",
        "flat",
        "%field%:%?createTime%c",
    )]));
    assert!(skip_log_lines(&backup, &db).is_empty());
    let again = skip_log_lines(&backup, &db);
    assert_eq!(again.len(), 1);
    assert!(again[0].starts_with("duplicate\t"), "{again:?}");
}

/// Two docs of one instant apart only by their structure's key were one
/// slug and a skipped doc, slugged by the instant alone.
#[test]
fn docs_of_one_instant_their_structure_tells_apart_are_both_kept() {
    let db = fresh_db();
    let tmpl = "%occurTime%__%kind%";
    let backup = write_backup(serde_json::json!([
        coffee_doc("a", "abc", "flat", tmpl),
        coffee_doc("b", "abd", "long", tmpl),
    ]));
    assert_eq!(skip_log_lines(&backup, &db), Vec::<String>::new());
    assert_eq!(
        slugs(&db),
        [
            "coffee:2025-01-08T07:00:00.000Z__flat",
            "coffee:2025-01-08T07:00:00.000Z__long",
        ]
    );
}

/// A structure v3 cannot slug a doc by loses no doc and aborts no import: it
/// is not stored, the original stays on the marker, the doc keeps its default
/// slug, and `check` has nothing to say. So with one rendering to nothing
/// (v2 reads `%?humanId%` from its meta), and the second of two rendering
/// alike.
#[test]
fn a_structure_a_doc_cannot_slug_by_is_not_stored() {
    let at = "coffee:2025-01-08T07:00:00.000Z";
    let tmpl = "%?humanId%";
    let dir = tempfile::TempDir::new().unwrap();
    let db = datum_core::db::Db::open(dir.path().join("t.db").to_str().unwrap()).unwrap();
    let backup = write_backup(serde_json::json!([coffee_doc("a", "abc", "flat", tmpl)]));
    assert_eq!(skip_log_lines(&backup, &db), Vec::<String>::new());
    assert_eq!(slugs(&db), [at]);
    let e = &datum_core::test_util::logged_all(&db)[0];
    assert_eq!(e.id_structure, None);
    assert_eq!(
        e.extra_metadata.as_ref().unwrap()["_import"]["original_id_structure"],
        tmpl,
    );
    assert_check_clean(&dir, tmpl);

    let dir = tempfile::TempDir::new().unwrap();
    let db = datum_core::db::Db::open(dir.path().join("t.db").to_str().unwrap()).unwrap();
    let backup = write_backup(serde_json::json!([
        coffee_doc("a", "abc", "flat", "%kind%"),
        coffee_doc("b", "abd", "flat", "%kind%"),
    ]));
    // Two docs, one fact: the second is the first's duplicate, as ever. Apart
    // in a key the structure does not name, both stay.
    assert_eq!(skip_log_lines(&backup, &db).len(), 1);
    assert_eq!(slugs(&db), ["coffee:flat"]);
    assert_check_clean(&dir, "duplicate");

    let dir = tempfile::TempDir::new().unwrap();
    let db = datum_core::db::Db::open(dir.path().join("t.db").to_str().unwrap()).unwrap();
    let mut second = coffee_doc("b", "abd", "flat", "%kind%");
    second["data"]["milk"] = "oat".into();
    let backup = write_backup(serde_json::json!([
        coffee_doc("a", "abc", "flat", "%kind%"),
        second,
    ]));
    assert_eq!(skip_log_lines(&backup, &db), Vec::<String>::new());
    assert_eq!(slugs(&db), [at, "coffee:flat"]);
    assert_check_clean(&dir, "rendered alike");
}

/// A `::` no id may hold, which only a delimiter other than `:` leaves
/// uncollapsed, costs that doc its structure and not the whole import.
#[test]
fn a_structure_rendering_an_id_no_id_may_be_does_not_abort_the_import() {
    let dir = tempfile::TempDir::new().unwrap();
    let backup = write_backup(serde_json::json!([coffee_doc(
        "a",
        "abc",
        "flat",
        "%kind%::x"
    )]));
    let datum = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", dir.path().join("t.db"))
            .env("DATUM_HOME", dir.path())
            .env("DATUM_CONFIG_HOME", dir.path())
            .env("DATUM_ID_DELIMITER", "/")
            .args(args)
            .output()
            .unwrap()
    };
    let out = datum(&["db", "v2import", backup.path().to_str().unwrap()]);
    assert!(out.status.success(), "{out:?}");
    let db = datum_core::db::Db::open(dir.path().join("t.db").to_str().unwrap()).unwrap();
    assert_eq!(slugs(&db), ["coffee:2025-01-08T07:00:00.000Z"]);
}
