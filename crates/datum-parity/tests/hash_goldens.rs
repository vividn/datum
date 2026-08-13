//! Byte-level `content_hash` goldens (da-yira).
//!
//! The sync merge order is `(modify_time, content_hash, device_id, hid)`.
//! It is only deterministic across implementations if every implementation
//! derives the *same bytes* for `content_hash` — and the preimage embeds
//! arbitrary JSON, whose serialization is a choice, not a given.
//!
//! `goldens/hash/content_hash.toml` pins that choice: for each entry, the
//! seven canonical preimage segments, the hex of the NUL-joined preimage,
//! and the expected digest. This test closes the loop in both directions:
//!
//! 1. the fixture is self-consistent (`parts` → `preimage_hex` → `content_hash`),
//!    so a foreign implementation can check itself against the file alone; and
//! 2. `datum_core`'s `expected_content_hash` reproduces it from the entry's
//!    columns, so the file cannot drift away from what daydumb actually writes.
//!
//! The specification these encode lives in `docs/sync-protocol.md`
//! § "Canonical serialization".

use datum_core::dur::Dur;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Goldens {
    fixture: Vec<Fixture>,
}

/// One entry, its canonical preimage, and its digest. `state` / `data` /
/// `following_state` are raw JSON text as it might arrive — deliberately
/// *not* in canonical key order, so the canonicalization is what's tested.
#[derive(Deserialize)]
struct Fixture {
    name: String,
    field: String,
    occur: Option<String>,
    dur: Option<String>,
    state: Option<String>,
    data: Option<String>,
    following_state: Option<String>,
    #[serde(default)]
    deleted: bool,
    parts: Vec<String>,
    preimage_hex: String,
    content_hash: String,
}

fn goldens() -> Vec<Fixture> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/goldens/hash/content_hash.toml"
    );
    let text = std::fs::read_to_string(path).expect("read hash goldens");
    toml::from_str::<Goldens>(&text)
        .expect("parse hash goldens")
        .fixture
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn json(text: &Option<String>) -> Option<serde_json::Value> {
    text.as_ref()
        .map(|t| serde_json::from_str(t).expect("fixture JSON parses"))
}

/// The fixture stands on its own: joining `parts` with a single `0x00`
/// byte yields `preimage_hex`, whose `SHA-256` is `content_hash`. An
/// implementation with no access to this source tree can verify itself
/// against exactly this.
#[test]
fn fixture_preimage_hashes_to_expected_digest() {
    for f in goldens() {
        assert_eq!(f.parts.len(), 7, "{}: preimage has seven segments", f.name);
        let preimage = f.parts.join("\0").into_bytes();
        assert_eq!(hex(&preimage), f.preimage_hex, "{}: preimage bytes", f.name);
        let digest = format!("{:x}", Sha256::digest(&preimage));
        assert_eq!(digest, f.content_hash, "{}: digest", f.name);
    }
}

/// ...and daydumb reproduces it from the entry's columns. This is the half
/// that fails if the canonical form drifts — a different timestamp
/// rendering, a different JSON serialization (notably `serde_json`'s
/// `preserve_order` feature turning up in the workspace graph), a changed
/// separator, or a new hashed column.
#[test]
fn core_reproduces_fixture_hashes() {
    for f in goldens() {
        let occur = f.occur.as_ref().map(|t| {
            let utc = chrono::DateTime::parse_from_rfc3339(t)
                .expect("fixture occur parses")
                .with_timezone(&chrono::Utc);
            DatumTime::from_utc_and_tz(utc, "UTC").expect("UTC is a zone")
        });
        let dur = f
            .dur
            .as_ref()
            .map(|d| Dur::parse(d).expect("fixture dur parses"));
        let mut entry = Entry::new(
            f.field.clone(),
            // `verb` is outside the hash — it is fully determined by the
            // hashed `(state, dur, following_state)` triple.
            Verb::Point,
            json(&f.state),
            json(&f.data),
            occur,
            dur,
            "fixture-device",
        );
        entry.following_state = json(&f.following_state);
        // Only deleted-ness is hashed, so any instant serves here.
        entry.deleted_at = f.deleted.then(chrono::Utc::now);

        assert_eq!(
            entry.expected_content_hash(),
            f.content_hash,
            "{}: datum-core disagrees with the pinned preimage {:?}",
            f.name,
            f.parts
        );
    }
}
