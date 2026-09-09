//! da-uxgv acceptance: what a sync conflict looks like from the outside.
//!
//! The unit tests in `datum-sync-client::ops` prove the merge writes the
//! right pointer. These prove the consequence the bead is actually about:
//! **one winner in the data stream**, the loser kept and addressable but
//! not in it, and none of it leaking into slug derivation.

use chrono::Duration;
use datum_core::db::{Db, ListParams};
use datum_core::entry::{Entry, Verb};
use datum_core::ids::Slug;
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

// ─── da-2fkr — the same-instant pair that is NOT a collision ─────────

/// One transaction, as a device would record it: stateless, dated to the
/// day, and given a distinct slug by the field's `%comment%` id template.
fn transaction(comment: &str, device: &str) -> Entry {
    let mut e = Entry::new(
        "tx",
        Verb::Point,
        None,
        Some(serde_json::json!({ "comment": comment })),
        Some(at("2026-09-01T00:00:00Z")),
        None,
        device,
    );
    e.slug = Slug(format!("tx:{comment}"));
    e.id_structure = Some("%comment%".to_string());
    e.content_hash = e.expected_content_hash();
    e
}

/// da-2fkr, the trap the bead names: narrowing only the index would leave
/// the sync apply superseding one of two transactions recorded on one day by
/// two devices — data loss in the reader's eyes. Both devices must end up
/// holding both rows, both live.
#[test]
fn two_devices_two_transactions_on_one_day_both_survive_a_full_sync() {
    let rent = transaction("rent", "dev-a");
    let electricity = transaction("electricity", "dev-b");
    assert_ne!(
        rent.slug, electricity.slug,
        "the id template separates them"
    );
    assert_eq!(
        rent.occur_time.as_ref().unwrap().utc,
        electricity.occur_time.as_ref().unwrap().utc,
        "and they sit at one instant",
    );

    let a = fresh_db();
    a.insert_entry(&rent).unwrap();
    let b = fresh_db();
    b.insert_entry(&electricity).unwrap();

    // A full sync: each side receives the other's log.
    apply_changeset(&a, &pack_changeset(&b, 0).unwrap()).unwrap();
    apply_changeset(&b, &pack_changeset(&a, 0).unwrap()).unwrap();

    for (name, db) in [("dev-a", &a), ("dev-b", &b)] {
        let mut stream: Vec<(String, Option<String>)> = db
            .list_entries(&ListParams::default())
            .unwrap()
            .into_iter()
            .map(|e| (e.slug.to_string(), e.superseded_by.map(|h| h.to_string())))
            .collect();
        stream.sort();
        assert_eq!(
            stream,
            vec![
                ("tx:electricity".to_string(), None),
                ("tx:rent".to_string(), None),
            ],
            "{name} must hold both transactions live and unsuperseded",
        );
        assert!(
            datum_core::integrity::list(
                db.conn(),
                &datum_core::integrity::ListFilter {
                    kind: Some(datum_core::integrity::IntegrityKind::IdentityCollision),
                    pending: None,
                },
            )
            .unwrap()
            .is_empty(),
            "{name} must file no identity_collision",
        );
    }
}
