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

/// `datum sync` and `sync status` name a pause on the reader's clock, and so
/// do `pause` and `resume`; `--json` keeps the instant (da-49tq4).
#[test]
fn a_pause_is_named_on_the_readers_clock() {
    let (a, _b) = chained();
    a.ok(&["sync", "pause"]);
    a.exec("UPDATE sync_local SET paused_at = '2020-01-02T23:30:00Z'");
    let at = |args: &[&str]| {
        let out = a.cmd(args).env("TZ", "Etc/GMT-2").output().unwrap();
        let said = [out.stdout, out.stderr].concat();
        let said = String::from_utf8(said).unwrap();
        assert!(!said.contains("23:30"), "{said}");
        said
    };
    let status = at(&["sync", "status"]);
    assert!(
        status.starts_with("⚠ paused since 2020-01-03 01:30: "),
        "{status}"
    );
    let sync = at(&["sync"]);
    assert!(
        sync.contains("sync is paused on this copy since 2020-01-03 01:30: "),
        "{sync}"
    );
    let again = at(&["sync", "pause"]);
    assert!(
        again.contains("already paused, since 2020-01-03 01:30\n"),
        "{again}"
    );
    assert_eq!(
        json(&a.ok(&["sync", "status", "--json"]))["paused"],
        "2020-01-02T23:30:00Z"
    );
    let resumed = at(&["sync", "resume"]);
    assert!(
        resumed.contains("(paused since 2020-01-03 01:30)"),
        "{resumed}"
    );
}

/// A departure no member holds yet is named in `sync status` on the
/// reader's clock; `chain list --json` keeps the instant (da-49tq4).
#[test]
fn a_departure_is_named_on_the_readers_clock() {
    let (_a, b) = chained();
    b.fails(&["sync", "chain", "leave"], "");
    let list = json(&b.ok(&["sync", "chain", "list", "--json"]));
    let left = list["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["this"] == true)
        .and_then(|m| m["left"].as_str())
        .unwrap_or_else(|| panic!("{list}"))
        .to_string();
    let east = chrono::FixedOffset::east_opt(5 * 3600 + 45 * 60).unwrap();
    let clock = chrono::DateTime::parse_from_rfc3339(&left)
        .unwrap()
        .with_timezone(&east)
        .format("%H:%M:")
        .to_string();
    let out = b
        .cmd(&["sync", "status"])
        .env("TZ", "Asia/Kathmandu")
        .output()
        .unwrap();
    let status = String::from_utf8(out.stdout).unwrap();
    let line = status
        .lines()
        .find(|l| l.contains("signed its departure at "))
        .unwrap_or_else(|| panic!("{status}"));
    // The clock alone today, its date before it should a midnight fall
    // between the leave and the read.
    assert!(
        line.contains(&clock) && !line.contains('Z'),
        "{clock} in {line}"
    );
}
