//! da-uxgv acceptance: what a sync conflict looks like from the outside.
//!
//! The unit tests in `datum-sync-client::ops` prove the merge writes the
//! right pointer. These prove the consequence the bead is actually about:
//! **one winner in the data stream**, the loser kept and addressable but
//! not in it, and none of it leaking into slug derivation.

use chrono::Duration;
use datum_core::db::{Db, ListParams};
use datum_core::entry::{Entry, Verb};
use datum_core::test_util::fresh_db;
use datum_core::time::DatumTime;
use datum_sync_client::ops::{apply_changeset, pack_changeset};

fn at(rfc: &str) -> DatumTime {
    DatumTime {
        utc: rfc.parse().unwrap(),
        offset_minutes: 0,
        timezone: Some("UTC".to_string()),
        uncertainty: None,
    }
}

/// One entry, as a device would record it: same field, same instant, so the
/// two devices' rows derive the same slug and contend.
fn record(state: &str, device: &str) -> Entry {
    let mut e = Entry::new(
        "coffee",
        Verb::Point,
        Some(serde_json::json!(state)),
        None,
        Some(at("2026-08-13T08:00:00Z")),
        None,
        device,
    );
    e.content_hash = e.expected_content_hash();
    e
}

/// Seed a device that holds `local`, then deliver `incoming` to it the way
/// a pull would. Returns the device.
fn merged(local: &Entry, incoming: &Entry) -> Db {
    let db = fresh_db();
    db.insert_entry(local).unwrap();
    let peer = fresh_db();
    peer.insert_entry(incoming).unwrap();
    let changeset = pack_changeset(&peer, 0).unwrap();
    apply_changeset(&db, &changeset).unwrap();
    db
}

/// The headline: two devices recording the same field at the same instant
/// leave **one** row in the stream, and the other still on disk.
#[test]
fn one_winner_in_the_stream_and_the_loser_kept() {
    let local = record("local", "dev-a");
    let mut incoming = record("incoming", "dev-b");
    incoming.modify_time = local.modify_time + Duration::seconds(60);
    assert_eq!(local.slug, incoming.slug, "the two contend for one slug");

    let db = merged(&local, &incoming);

    let stream = db.list_entries(&ListParams::default()).unwrap();
    assert_eq!(stream.len(), 1, "one winner in the tail, got: {stream:?}");
    assert_eq!(stream[0].hid, incoming.hid);

    let all = db.list_including_deleted().unwrap();
    assert_eq!(all.len(), 2, "and the loser is still on disk, not dropped");

    // Addressable by hid — the identity that outlives every edit — but the
    // shared slug resolves to the row that is in the stream.
    let loser = db
        .get_entry_including_deleted(local.hid.as_str())
        .expect("the loser is still addressable by hid");
    assert_eq!(loser.superseded_by.as_ref(), Some(&incoming.hid));
    assert_eq!(
        loser.state,
        Some(serde_json::json!("local")),
        "with its content untouched",
    );
    assert_eq!(
        db.get_entry(local.slug.as_str()).unwrap().hid,
        incoming.hid,
        "the slug names the winner",
    );
}

/// The loser keeps the slug its own template derives — nothing about the
/// merge is smuggled into a derived column. This is the invariant da-uxgv
/// exists to restore.
#[test]
fn the_loser_keeps_its_naturally_derived_slug() {
    let local = record("local", "dev-a");
    let mut incoming = record("incoming", "dev-b");
    incoming.modify_time = local.modify_time + Duration::seconds(60);

    let db = merged(&local, &incoming);
    let loser = db.get_entry_including_deleted(local.hid.as_str()).unwrap();
    assert_eq!(
        loser.slug, local.slug,
        "same slug as the winner, and that is legal now",
    );
}

/// vividn's rule, end to end: deleting the winner does not hand the stream
/// back to the row it beat.
#[test]
fn deleting_the_winner_leaves_the_loser_out_of_the_stream() {
    let local = record("local", "dev-a");
    let mut incoming = record("incoming", "dev-b");
    incoming.modify_time = local.modify_time + Duration::seconds(60);

    let db = merged(&local, &incoming);
    let winner = db.get_entry(incoming.slug.as_str()).unwrap();
    db.soft_delete_entry(&winner.slug).unwrap();

    let stream = db.list_entries(&ListParams::default()).unwrap();
    assert!(
        stream.is_empty(),
        "the deleted winner takes the slug with it; nothing resurfaces: {stream:?}",
    );
    let loser = db.get_entry_including_deleted(local.hid.as_str()).unwrap();
    assert_eq!(loser.superseded_by.as_ref(), Some(&incoming.hid));
}

/// A fresh replica must reach the same two rows from the changeset log
/// alone — no client-local state the log cannot reconstruct, which is the
/// test the bead names when it rejects "leave losing changesets unapplied".
#[test]
fn a_fresh_replica_reconstructs_the_pair_from_the_log() {
    let local = record("local", "dev-a");
    let mut incoming = record("incoming", "dev-b");
    incoming.modify_time = local.modify_time + Duration::seconds(60);

    let db = merged(&local, &incoming);
    let log = pack_changeset(&db, 0).unwrap();

    let replica = fresh_db();
    apply_changeset(&replica, &log).unwrap();

    let shape = |db: &Db| -> Vec<(String, String, Option<String>)> {
        let mut stmt = db
            .conn()
            .prepare("SELECT hid, slug, superseded_by FROM entries ORDER BY hid")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    assert_eq!(shape(&replica), shape(&db));
}
