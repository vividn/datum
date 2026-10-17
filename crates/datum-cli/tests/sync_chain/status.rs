//! `sync status` as a person reads it.

use super::*;

/// A member's row in `sync status` is on the reader's clock, as the notices
/// above it are; `--json` keeps the instant as stored (da-jb4ln).
#[test]
fn a_members_time_in_sync_status_is_on_the_readers_clock() {
    let (a, b) = chained();
    let sql = rusqlite::Connection::open(a.tmp.path().join("data.db")).unwrap();
    let pin = "UPDATE sync_peers SET last_seen = '2020-01-02T23:30:00Z', \
               last_verify_at = NULL WHERE endpoint_id = ?1";
    assert_eq!(sql.execute(pin, [b.id()]).unwrap(), 1);
    let out = a
        .cmd(&["sync", "status"])
        .env("TZ", "Etc/GMT-2")
        .output()
        .unwrap();
    let status = String::from_utf8(out.stdout).unwrap();
    let row = status
        .lines()
        .find(|l| !l.starts_with('⚠') && l.contains(&b.id()[..10]))
        .unwrap_or_else(|| panic!("{status}"));
    assert!(
        row.contains("  2020-01-03 01:30") && !row.contains('Z'),
        "{row}"
    );
    let status = json(&a.ok(&["sync", "status", "--json"]));
    let member = status["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["endpoint"] == b.id())
        .unwrap();
    assert_eq!(member["last_seen"], "2020-01-02T23:30:00Z");
}
