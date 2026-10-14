//! A past instant with no `-z` takes the zone the entries around it agree
//! on, records that it did, and is left alone where they do not agree
//! (da-h8dvi). Chatham is a zone no test host is in, so the host's never
//! agrees with the neighbours by chance.

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::fresh_db;

const CHATHAM: &str = "Pacific/Chatham";

/// Three `coffee`s in `before` on June 12–14 and three in `after` on 16–18.
fn around_the_15th(before: &str, after: &str) -> Db {
    let db = fresh_db();
    for (day, zone) in [
        (12, before),
        (13, before),
        (14, before),
        (16, after),
        (17, after),
        (18, after),
    ] {
        let date = format!("2024-06-{day}");
        run_cli(
            &["occur", "coffee", "-z", zone, "-d", &date, "-t", "12:00"],
            &db,
        )
        .unwrap();
    }
    db
}

/// `tea`'s zone, its offset and its `extra_metadata`.
fn tea(db: &Db) -> (Option<String>, i64, Option<String>) {
    db.conn()
        .query_row(
            "SELECT occur_time_tz, occur_time_offset, extra_metadata FROM entries WHERE field = 'tea'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
}

#[test]
fn a_backfill_takes_the_zone_around_it_and_records_it() {
    let db = around_the_15th(CHATHAM, CHATHAM);
    run_cli(&["occur", "tea", "-d", "2024-06-15", "-t", "12:00"], &db).unwrap();
    let (zone, offset, meta) = tea(&db);
    assert_eq!(zone.as_deref(), Some(CHATHAM));
    // 12:00 on Chatham's clock, +12:45 in its June.
    let occur: String = db
        .conn()
        .query_row(
            "SELECT occur_time FROM entries WHERE field = 'tea'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!((occur.as_str(), offset), ("2024-06-14T23:15:00+00:00", 765));
    assert_eq!(meta.as_deref(), Some(r#"{"tz_from":"neighbours"}"#));
}

/// One mis-zoned neighbour is outvoted, not copied forward.
#[test]
fn a_lone_neighbour_in_another_zone_is_outvoted() {
    let db = around_the_15th(CHATHAM, CHATHAM);
    run_cli(
        &[
            "occur",
            "coffee",
            "-z",
            "Asia/Tokyo",
            "-d",
            "2024-06-15",
            "-t",
            "18:00",
        ],
        &db,
    )
    .unwrap();
    run_cli(&["occur", "tea", "-d", "2024-06-15", "-t", "12:00"], &db).unwrap();
    assert_eq!(tea(&db).0.as_deref(), Some(CHATHAM));
}

/// A move between the two sides says nothing about which side the instant
/// was on, and nothing after it says nothing either: the host's zone, as
/// before, unrecorded.
#[test]
fn a_step_or_no_later_entry_leaves_the_host_zone() {
    for (before, after, date) in [
        (CHATHAM, "Asia/Tokyo", "2024-06-15"),
        (CHATHAM, CHATHAM, "2024-06-20"),
    ] {
        let db = around_the_15th(before, after);
        run_cli(&["occur", "tea", "-d", date, "-t", "12:00"], &db).unwrap();
        let (zone, _, meta) = tea(&db);
        assert_ne!(zone.as_deref(), Some(CHATHAM), "{before} → {after}, {date}");
        assert_eq!(meta, None, "{before} → {after}, {date}");
    }
}

/// A zone the person named — `-z`, or an offset on `-t` — is theirs.
#[test]
fn a_named_zone_or_offset_is_kept() {
    let named: [&[&str]; 2] = [
        &["-z", "Asia/Tokyo", "-d", "2024-06-15", "-t", "12:00"],
        &["-t", "2024-06-15T12:00+09:00"],
    ];
    for time in named {
        let db = around_the_15th(CHATHAM, CHATHAM);
        let argv: Vec<&str> = ["occur", "tea"].iter().chain(time).copied().collect();
        run_cli(&argv, &db).unwrap();
        let (_, offset, meta) = tea(&db);
        assert_eq!((offset, meta), (540, None), "{time:?}");
    }
}

/// The zone is asked again where the entry lands. 12:00 on the 15th is
/// 23:00 UTC in Pago Pago (UTC−11), and a host's reading of it, from UTC−10
/// to +14, is 22:00 UTC or earlier: three Pago Pago rows between the two
/// make the host's reading bracketed by Pago Pago, and Pago Pago's own
/// reading a step to Tokyo.
#[test]
fn the_zone_must_hold_where_the_entry_lands() {
    let pago = "Pacific/Pago_Pago";
    let db = around_the_15th(pago, "Asia/Tokyo");
    for at in ["22:30", "22:40", "22:50"] {
        let t = format!("2024-06-15T{at}:00Z");
        run_cli(&["occur", "coffee", "-z", pago, "-t", &t], &db).unwrap();
    }
    run_cli(&["occur", "tea", "-d", "2024-06-15", "-t", "12:00"], &db).unwrap();
    let (zone, _, meta) = tea(&db);
    assert_ne!(zone.as_deref(), Some(pago));
    assert_eq!(meta, None);
}

/// An `occur_time=` key with an offset of its own places the entry, so
/// nothing is recorded as the neighbours' doing.
#[test]
fn an_occur_time_key_with_its_own_offset_is_not_recorded_as_inferred() {
    let db = around_the_15th(CHATHAM, CHATHAM);
    let key = "occur_time=2024-06-15T12:00+09:00";
    run_cli(
        &["occur", "tea", "-d", "2024-06-15", "-t", "12:00", key],
        &db,
    )
    .unwrap();
    let (_, offset, meta) = tea(&db);
    assert_eq!((offset, meta), (540, None));
}
