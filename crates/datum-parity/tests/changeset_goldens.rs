//! Cross-version changeset goldens (da-4zoa).
//!
//! `goldens/changeset/` holds frozen payloads that real builds put on the
//! wire. This test feeds each to today's
//! [`datum_sync_client::ops::apply_changeset`] and asserts, column by column,
//! what lands in the entries table.
//!
//! # Why this shape and not the obvious one
//!
//! The changeset payload sets no `deny_unknown_fields`, so a build that stops
//! reading a field drops it **in silence**: no error, no count change. Two
//! peers on the same build agree perfectly while both lose the same column,
//! which is why neither a count comparison nor a same-build content comparison
//! can see it — the direction that loses data is a *newer* build reading what
//! an *older* build wrote, and only a frozen artefact can stand in for the
//! older build.
//!
//! So: **nothing regenerates these payloads.** The repo's other goldens
//! (`web/tests/fixtures/cli-parity.json`) have a CI regeneration gate, and that
//! is right for a parity fixture and catastrophic here — regenerating replaces
//! the artefact under test with the thing it was tested against, after which
//! every run passes forever and proves only that today's writer agrees with
//! today's reader. `goldens/changeset/README.md` is the long version.
//!
//! The comparison is column-by-column rather than whole-blob: a whole-blob
//! equality fails on every harmless addition and gets deleted by the third
//! person who hits it. [`COLUMNS`] is the exhaustive list, and the expectation
//! must state every one of them — a column added to `Entry` makes every
//! expectation block fail until someone decides what an old payload means for
//! it, which is the review moment this design wants.

use std::collections::BTreeMap;

use datum_core::db::{row_to_entry_public, Db};
use datum_core::dur::Dur;
use datum_core::entry::Entry;
use datum_sync_client::ops::apply_changeset;
use datum_sync_client::Changeset;
use serde::Deserialize;
use serde_json::{json, Value};

/// Every column the entries table carries, and so every column an
/// expectation must state. Kept in the order `upsert_lww`'s INSERT lists
/// them so the two can be read side by side.
const COLUMNS: [&str; 29] = [
    "hid",
    "slug",
    "content_hash",
    "field",
    "verb",
    "state",
    "dur",
    "dur_uncertainty",
    "following_state",
    "data",
    "occur_time",
    "occur_offset_minutes",
    "occur_timezone",
    "occur_uncertainty",
    "lat",
    "lon",
    "location",
    "create_time",
    "modify_time",
    "device_id",
    "deleted_at",
    "superseded_by",
    "id_structure",
    "field_structure",
    "raw_input",
    "alias_chain",
    "parser_version",
    "user_asserted_last_state",
    "extra_metadata",
];

#[derive(Deserialize)]
struct Manifest {
    #[serde(rename = "_comment")]
    _comment: Vec<String>,
    golden: Vec<Golden>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Golden {
    /// Payload file in `goldens/changeset/`, frozen and never rewritten.
    file: String,
    /// The commit whose build produced it — the record's provenance.
    built_at: String,
    /// The envelope magic those bytes shipped under, recorded per golden
    /// rather than taken from today's constant: a future `DSC2` must still
    /// hand a `DSC1` golden its own four bytes, or the test would be
    /// relabelling old JSON as a format it was never written in.
    magic: String,
    note: String,
    outcome: Outcome,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Outcome {
    Decodes {
        rows: Vec<BTreeMap<String, Value>>,
    },
    Refused {
        message_contains: Vec<String>,
        rows_written: i64,
    },
}

fn goldens_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/changeset")
}

fn manifest() -> Manifest {
    let path = goldens_dir().join("expected.json");
    let text = std::fs::read_to_string(&path).expect("read changeset golden manifest");
    serde_json::from_str(&text).expect("parse changeset golden manifest")
}

/// Rebuild the wire bytes: the golden's own magic, then the payload JSON
/// exactly as it was frozen.
fn wire_bytes(g: &Golden) -> Vec<u8> {
    let json = std::fs::read(goldens_dir().join(&g.file))
        .unwrap_or_else(|e| panic!("read golden {}: {e}", g.file));
    let mut bytes = g.magic.as_bytes().to_vec();
    bytes.extend_from_slice(&json);
    bytes
}

fn opt<T: Into<Value>>(v: Option<T>) -> Value {
    v.map_or(Value::Null, Into::into)
}

/// The stored row as [`COLUMNS`] names it. Derived from the `Entry` the row
/// mapper reconstructs, so anything the apply path failed to write shows up
/// here as a null.
fn stored_columns(e: &Entry) -> BTreeMap<String, Value> {
    let occur = e.occur_time.as_ref();
    let cols = [
        ("hid", json!(e.hid.0)),
        ("slug", json!(e.slug.0)),
        ("content_hash", json!(e.content_hash)),
        ("field", json!(e.field)),
        ("verb", json!(e.verb.to_string())),
        ("state", opt(e.state.clone())),
        (
            "dur",
            opt(e.dur.as_ref().map(Dur::to_storage_string).map(Value::from)),
        ),
        ("dur_uncertainty", opt(e.dur_uncertainty())),
        ("following_state", opt(e.following_state.clone())),
        ("data", opt(e.data.clone())),
        (
            "occur_time",
            opt(occur.map(|t| t.utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))),
        ),
        ("occur_offset_minutes", opt(occur.map(|t| t.offset_minutes))),
        (
            "occur_timezone",
            opt(occur.and_then(|t| t.timezone.clone())),
        ),
        ("occur_uncertainty", opt(e.occur_uncertainty())),
        ("lat", opt(e.lat)),
        ("lon", opt(e.lon)),
        ("location", opt(e.location.clone())),
        (
            "create_time",
            json!(e
                .create_time
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        ),
        (
            "modify_time",
            json!(e
                .modify_time
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        ),
        ("device_id", json!(e.device_id)),
        (
            "deleted_at",
            opt(e
                .deleted_at
                .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))),
        ),
        (
            "superseded_by",
            opt(e.superseded_by.as_ref().map(|h| h.0.clone())),
        ),
        ("id_structure", opt(e.id_structure.clone())),
        ("field_structure", opt(e.field_structure.clone())),
        ("raw_input", opt(e.raw_input.clone())),
        ("alias_chain", opt(e.alias_chain.clone())),
        ("parser_version", json!(e.parser_version)),
        (
            "user_asserted_last_state",
            opt(e.user_asserted_last_state.clone()),
        ),
        ("extra_metadata", opt(e.extra_metadata.clone())),
    ];
    let map: BTreeMap<String, Value> = cols.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let mut want = COLUMNS;
    want.sort_unstable();
    let got: Vec<&str> = map.keys().map(String::as_str).collect();
    assert_eq!(
        got, want,
        "stored_columns must read back exactly the COLUMNS list",
    );
    map
}

fn row_count(db: &Db) -> i64 {
    db.conn()
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .unwrap()
}

/// The whole point, in one loop: every frozen payload still decodes to the
/// row it decoded to when it was recorded — or is still refused for the
/// reason it was recorded as being refused.
#[test]
fn frozen_payloads_still_decode_to_their_recorded_rows() {
    let m = manifest();
    assert!(!m.golden.is_empty(), "manifest lists at least one golden");

    for g in &m.golden {
        let db = Db::open_in_memory().expect("in-memory db");
        let result = apply_changeset(&db, &Changeset::from_bytes(wire_bytes(g)));

        match &g.outcome {
            Outcome::Refused {
                message_contains,
                rows_written,
            } => {
                let err = result.err().unwrap_or_else(|| {
                    panic!(
                        "{} ({}): recorded as refused, but today's build accepted it. \
                         If that is deliberate, change this golden's outcome — do not \
                         touch the payload.",
                        g.file, g.built_at
                    )
                });
                let msg = err.to_string();
                for needle in message_contains {
                    assert!(
                        msg.contains(needle.as_str()),
                        "{}: refusal message lost {needle:?}\n  got: {msg}",
                        g.file,
                    );
                }
                assert_eq!(
                    row_count(&db),
                    *rows_written,
                    "{}: a refused changeset must not leave rows behind",
                    g.file,
                );
            }
            Outcome::Decodes { rows } => {
                result.unwrap_or_else(|e| {
                    panic!(
                        "{} ({}): recorded as decoding, but today's build refuses it: {e}",
                        g.file, g.built_at
                    )
                });
                assert_eq!(
                    row_count(&db),
                    rows.len() as i64,
                    "{}: entry count differs from the record",
                    g.file,
                );

                for expected in rows {
                    let hid = expected
                        .get("hid")
                        .and_then(Value::as_str)
                        .unwrap_or_else(|| panic!("{}: expectation states a hid", g.file));
                    let stored: Entry = db
                        .conn()
                        .query_row(
                            "SELECT * FROM entries WHERE hid = ?1",
                            [hid],
                            row_to_entry_public,
                        )
                        .unwrap_or_else(|e| panic!("{}: row {hid} not stored: {e}", g.file));
                    let actual = stored_columns(&stored);

                    // Both directions, so neither a forgotten column nor a
                    // stale one can hide. A column added to `Entry` lands
                    // here first, as a demand that every golden say what the
                    // old payload means for it.
                    let stated: Vec<&str> = expected.keys().map(String::as_str).collect();
                    let mut want = COLUMNS;
                    want.sort_unstable();
                    assert_eq!(
                        stated,
                        want,
                        "{} row {hid}: the expectation must state exactly the {} columns \
                         in COLUMNS — see goldens/changeset/README.md",
                        g.file,
                        COLUMNS.len(),
                    );

                    for column in COLUMNS {
                        assert_eq!(
                            actual.get(column),
                            expected.get(column),
                            "{} ({}) row {hid}: column `{column}` no longer decodes to \
                             its recorded value.\n  note: {}",
                            g.file,
                            g.built_at,
                            g.note,
                        );
                    }
                }
            }
        }
    }
}

/// Every payload file in the directory is claimed by the manifest, and every
/// claim points at a file that exists. A golden nobody runs is the silence
/// this crate exists to break — `datum-parity`'s text fixtures already
/// proved that an uncompared golden reports green.
#[test]
fn every_payload_file_is_claimed_by_the_manifest() {
    let claimed: Vec<String> = manifest().golden.into_iter().map(|g| g.file).collect();
    let mut on_disk: Vec<String> = std::fs::read_dir(goldens_dir())
        .expect("read goldens/changeset")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into())
        .filter(|n: &String| n.starts_with("changeset_") && n.ends_with(".json"))
        .collect();
    on_disk.sort();
    let mut claimed_sorted = claimed.clone();
    claimed_sorted.sort();
    assert_eq!(
        on_disk, claimed_sorted,
        "every changeset_*.json must appear in expected.json exactly once",
    );
}

/// The goldens' own no-regeneration rule, enforced rather than merely
/// documented: no script in this crate may write into `goldens/changeset/`.
/// The next person's instinct will be to add one — a regeneration path here
/// replaces the artefact under test with the thing it was tested against,
/// after which the suite reports green forever while testing nothing.
#[test]
fn nothing_in_this_crate_regenerates_the_changeset_goldens() {
    let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts");
    for entry in std::fs::read_dir(&scripts).expect("read scripts dir") {
        let path = entry.expect("dir entry").path();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !text.contains("goldens/changeset"),
            "{}: scripts must not touch goldens/changeset — a cross-version \
             golden that any script can rewrite is a test that can never fail \
             (see goldens/changeset/README.md)",
            path.display(),
        );
    }
}
