//! A changeset written by a peer on an OLDER build must arrive whole.
//!
//! `sync_cluster_e2e` (da-0kmn) compares content between two devices
//! running the SAME binary. A dropped-unknown-field bug is invisible to
//! that shape by construction: both sides serialise and deserialise with
//! the same field set, so the two databases agree perfectly — on the
//! wrong content. The axis missing there, and supplied here, is VERSION.
//!
//! The concrete failure this file defends against is stated in
//! `datum-sync-client/src/ops.rs` above `adopt_legacy_uncertainty`:
//! before da-b8rs an entry carried `occur_time_precision` and
//! `dur_precision` as its own top-level keys; now those values live on
//! the time and the duration they describe. Nothing in the entry types
//! sets `deny_unknown_fields`, so a newer peer reading an older peer's
//! payload parses it cleanly and *silently* drops both uncertainties
//! unless the fold puts them back.
//!
//! No old binary is built and no relay is involved: the pre-da-b8rs wire
//! shape is a literal below, because the shape IS what is under test and
//! a reader has to be able to see it. Pack/apply is a client-only
//! concern, so the test applies that payload to one database and compares
//! it — through the same `assert_same_content` the cluster suite uses —
//! against the entry the shape was meant to encode, built natively in a
//! second database.
//!
//! The next field to move house can copy this: build the entry the new
//! way, spell the old payload out, and assert nothing was lost between
//! them.

mod sync_content;

use chrono::{Duration, TimeZone, Utc};
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use datum_parse::Dur;
use datum_sync_client::crypto::changeset::Changeset;
use datum_sync_client::ops::apply_changeset;

use sync_content::{assert_same_content, live_content};

/// The entry the legacy payload below encodes, built the way current code
/// builds one: both uncertainties stated in their da-b8rs homes — on the
/// `occur_time` and inside the `dur`.
///
/// Carries substance in columns the `content_hash` does not cover (the
/// zone and its offset, the location triple, `raw_input`) for the same
/// reason the cluster suite's fixture does: a comparison over an
/// emptied-out row would agree for reasons the hash had already settled.
/// Both uncertainties are outside the preimage too, which is exactly why
/// losing them is silent.
fn expected_entry() -> Entry {
    let at = Utc.with_ymd_and_hms(2026, 5, 22, 12, 0, 0).unwrap();
    let mut t = DatumTime::from_utc_and_tz(at, "America/Chicago").expect("known zone");
    t.uncertainty = Some("PT1M".to_string());
    let dur = Dur::of(Duration::minutes(30)).with_uncertainty(Some("PT5M".to_string()));
    let mut e = Entry::new(
        "workout",
        Verb::Point,
        Some(serde_json::json!(1)),
        Some(serde_json::json!({ "cup": { "v": 250, "u": "ml" } })),
        Some(t),
        Some(dur),
        "dev-old",
    );
    e.lat = Some(41.8781);
    e.lon = Some(-87.6298);
    e.location = Some("Chicago".to_string());
    e.raw_input = Some("workout 1".to_string());
    e.create_time = at;
    e.modify_time = at;
    e
}

/// The same entry as a pre-da-b8rs peer would have put it on the wire:
/// `occur_time` with no `uncertainty` key, a bare `dur`, and the two
/// uncertainties sitting beside them as top-level `occur_time_precision`
/// / `dur_precision`. Current code knows neither top-level key.
///
/// Only the three identity columns are interpolated — `hid` is the
/// merge key, and `slug` / `content_hash` are derived from substance the
/// two shapes share — so that the comparison is over one entry rather
/// than two. Every field that carries the shape under test is literal.
///
/// `b"DSC1"` is the v1 changeset magic, spelled out here because it is
/// part of what the old peer wrote; a future magic bump makes this
/// payload fail loudly at the envelope rather than pass on a
/// mis-parse (`payload_json`, da-pl2o).
fn legacy_payload(entry: &Entry) -> Changeset {
    let doc = serde_json::json!({
        // The peer is old on the axis under test — the uncertainty keys —
        // and current on every other. `hash_version` is stated because a
        // changeset that declares none is refused outright (da-engu), and
        // conflating "predates da-b8rs" with "predates hash versioning"
        // would make this file test the refusal instead of the fold. The
        // refusal has its own tests in `datum-sync-client::ops`.
        "hash_version": datum_sync_protocol::CONTENT_HASH_VERSION,
        "entries": [{
            "hid": entry.hid,
            "slug": entry.slug,
            "content_hash": entry.content_hash,
            "field": "workout",
            "verb": "block",
            "state": 1,
            "data": { "cup": { "v": 250, "u": "ml" } },
            "following_state": null,
            "dur": "PT30M",
            "dur_precision": "PT5M",
            "occur_time": {
                "utc": "2026-05-22T12:00:00Z",
                "offset_minutes": -300,
                "timezone": "America/Chicago",
            },
            "occur_time_precision": "PT1M",
            "lat": 41.8781,
            "lon": -87.6298,
            "location": "Chicago",
            "create_time": "2026-05-22T12:00:00Z",
            "modify_time": "2026-05-22T12:00:00Z",
            "device_id": "dev-old",
            "id_structure": null,
            "field_structure": null,
            "deleted_at": null,
            "raw_input": "workout 1",
            "alias_chain": null,
            "parser_version": 2,
            "user_asserted_last_state": null,
            "extra_metadata": null,
        }]
    });
    let json = serde_json::to_vec(&doc).expect("serialise legacy payload");
    let mut bytes = b"DSC1".to_vec();
    bytes.extend_from_slice(&json);
    Changeset::from_bytes(bytes)
}

/// Apply the pre-da-b8rs payload with current code. The row that lands
/// must equal, column for column, the entry that payload encodes.
///
/// Made to go red by commenting out the `adopt_legacy_uncertainty` call
/// in `parse_payload`: the failure names both dropped fields —
/// `occur_time: native={...,"uncertainty":"PT1M"} legacy={...}` and
/// `dur: native="PT30M±PT5M" legacy="PT30M"`.
#[test]
fn legacy_wire_shape_keeps_its_uncertainty() {
    let db_native = Db::open_in_memory().expect("open native db");
    let db_legacy = Db::open_in_memory().expect("open legacy db");

    let entry = expected_entry();
    db_native.insert_entry(&entry).expect("insert native");
    apply_changeset(&db_legacy, &legacy_payload(&entry)).expect("apply legacy changeset");

    // Guard against a vacuous pass: the expected row has to actually
    // state both uncertainties, or the comparison below proves nothing.
    let native = live_content(&db_native);
    assert_eq!(native.len(), 1, "one entry to compare");
    assert_eq!(
        native[0]["occur_time"]["uncertainty"], "PT1M",
        "the expected row states its occur-time uncertainty",
    );
    assert_eq!(
        native[0]["dur"], "PT30M±PT5M",
        "the expected row states its duration uncertainty",
    );

    assert_same_content("native", &db_native, "legacy", &db_legacy);
}
