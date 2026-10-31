//! A write warning of changes no other device has (da-u10rl).

use super::*;

/// Age this copy's ops past its last session with `b` by `days`, as though
/// it made them then.
fn age_unsent(a: &Copy, days: u32) {
    let seq = a.query(&format!(
        "SELECT COALESCE(json_extract(last_vector, '$.\"{}\"'), 0) FROM sync_peers",
        a.id()
    ));
    a.exec(&format!(
        "UPDATE ops SET hlc = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-{days} days') || '/0000'
         WHERE endpoint = (SELECT endpoint_id FROM sync_local) AND seq > {seq}"
    ));
}

/// A write warns, at most once a day, when changes made here have reached
/// no other member for over a day, naming how many, since when, and the
/// pause when it knows it (da-u10rl). Another device being off is no reason,
/// a read never warns, nor does a lone member.
#[test]
fn a_write_warns_once_a_day_of_changes_no_other_device_has() {
    let (a, b) = chained();
    a.ok(&["occur", "tea"]);
    session(&a, &b);
    let stale = "reached no other device";

    // b off for a week, a holding nothing it lacks: no warning.
    a.exec("UPDATE sync_peers SET last_seen = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-7 days')");
    assert!(
        !a.stderr_of(&["occur", "tea"]).contains(stale),
        "b off is no reason"
    );

    // What a wrote has reached no one for three days: the next write warns.
    a.exec("UPDATE sync_local SET warned_at = NULL");
    age_unsent(&a, 3);
    assert!(!a.stderr_of(&["tail"]).contains(stale), "a read is quiet");
    let since = a.query("SELECT date('now', '-3 days', 'localtime')");
    let warned = a.stderr_of(&["occur", "tea"]);
    assert!(
        warned.contains(&format!(
            "⚠ 2 changes made here since {since} have reached no other device — `datum sync status`"
        )),
        "{warned}"
    );
    assert!(
        !a.stderr_of(&["occur", "tea"]).contains(stale),
        "once a day"
    );
    // Shown, past the day's warning, wherever sync's question is asked, from
    // one source (da-nq07y).
    let unsent = format!("3 changes made here since {since} have reached no other device");
    let status = a.ok(&["sync", "status"]);
    assert!(status.contains(&format!("⚠ {unsent}\n")), "{status}");
    let check = a.ok(&["check", "sync"]);
    assert!(
        check.contains(&format!("[WARN ] sync_unsent (x1): ⚠ {unsent}")),
        "{check}"
    );
    assert_eq!(
        json(&a.ok(&["sync", "status", "--json"]))["unsent"],
        unsent.as_str()
    );

    // A day later it warns again, and names the pause.
    a.ok(&["sync", "pause"]);
    a.exec("UPDATE sync_local SET warned_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-25 hours')");
    let at = a.query("SELECT strftime('%H:%M', paused_at, 'localtime') FROM sync_local");
    let warned = a.stderr_of(&["occur", "tea"]);
    assert!(
        warned.contains(&format!(
            "⚠ 4 changes made here since {since} have reached no other device (paused since {at})"
        )),
        "{warned}"
    );

    // Once b has them, b off for a week again: no warning.
    a.ok(&["sync", "resume"]);
    session(&a, &b);
    a.exec("UPDATE sync_local SET warned_at = NULL");
    a.exec("UPDATE sync_peers SET last_seen = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-7 days')");
    assert!(
        !a.stderr_of(&["occur", "tea"]).contains(stale),
        "b has them"
    );

    // With b revoked, a is its chain's lone member: never a warning.
    a.ok(&["sync", "chain", "expel", "b"]);
    age_unsent(&a, 3);
    a.exec("UPDATE sync_local SET warned_at = NULL");
    assert!(!a.stderr_of(&["occur", "tea"]).contains(stale));
    assert!(!a.ok(&["sync", "status"]).contains(stale));
    assert!(!a.ok(&["check", "sync"]).contains(stale));
}

/// A multi-side write commits after every side has run, and warns as a
/// single write does (da-wlbyx).
#[test]
fn a_multi_side_write_warns_too() {
    let (a, b) = chained();
    a.ok(&["occur", "tea", ",", "occur", "coffee"]);
    session(&a, &b);
    a.ok(&["occur", "water"]);
    age_unsent(&a, 3);
    let warned = a.stderr_of(&["occur", "tea", ",", "occur", "coffee"]);
    // water and the spec it declared, tea, coffee.
    assert!(warned.contains("⚠ 4 changes made here since"), "{warned}");
}

/// Only a member holding everything counts toward what has reached another
/// device (da-u10rl): a copy whose only other member is a filtered reader
/// never warns, as a lone member never does, since no sync could carry all
/// of it off.
#[test]
fn a_copy_whose_only_other_is_a_filtered_reader_never_warns() {
    let (a, r) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee"]);
    admit_filtered(&a, &r, "coffee");
    let rows = a.query(&format!(
        "SELECT COUNT(*) FROM sync_peers WHERE endpoint_id = '{}'",
        r.id()
    ));
    assert_eq!(rows, "1", "the reader is a peer a could read a vector from");
    a.ok(&["occur", "tea"]);
    a.exec("UPDATE ops SET hlc = strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-3 days') || '/0000'");
    let out = a.stderr_of(&["occur", "tea"]);
    assert!(!out.contains("reached no other device"), "{out}");
}
