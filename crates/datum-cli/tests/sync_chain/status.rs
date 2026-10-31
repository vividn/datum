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
        // The date, not the bare 23:30: a notice of the real join prints
        // today's clock, which is 23:30 at 21:30 UTC (da-g04cj).
        assert!(!said.contains("2020-01-02"), "{said}");
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

#[test]
fn status_says_never_then_synced_then_pending_then_error_each_with_its_time() {
    let (a, b) = chained();
    // b heard of a over the pipe, never in a session.
    let row = status_of(&b, &a);
    assert_eq!(row["state"], "never", "{row}");
    assert!(row["last_seen"].is_null() && row["last_verify"].is_null());
    let all = json(&a.ok(&["sync", "status", "--json"]));
    assert_eq!(all["this"]["endpoint"], a.id().as_str());
    assert_eq!(all["this"]["client"], client_of(&a).as_str());
    assert!(all["chain"].is_string(), "{all}");

    session(&a, &b);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["to_send"].as_i64()),
        (Some("synced"), Some(0)),
        "{row}"
    );
    assert_eq!(row["last_verify"]["match"], true);
    assert_eq!(
        row["kind"],
        serde_json::Value::Null,
        "b never described itself"
    );

    a.ok(&["occur", "later"]);
    let row = status_of(&a, &b);
    // What a holds past what b last had: the entry, and its field's spec.
    let (mine, theirs) = (
        json(&a.ok(&["sync", "pipe", "vector"])),
        json(&b.ok(&["sync", "pipe", "vector"])),
    );
    let waiting = mine[a.id()].as_i64().unwrap() - theirs[a.id()].as_i64().unwrap();
    assert!(waiting > 0);
    assert_eq!(
        (row["state"].as_str(), row["to_send"].as_i64()),
        (Some("pending"), Some(waiting)),
        "{row}"
    );
    session(&a, &b);
    assert_eq!(status_of(&a, &b)["state"], "synced");

    // A mismatch stands through a write made since; a session clears it.
    a.ok(&["sync", "record-session", &b.id(), "--verify", "mismatch"]);
    a.ok(&["occur", "since"]);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["error"].as_str()),
        (Some("error"), Some("verify MISMATCH")),
        "{row}"
    );
    session(&a, &b);
    assert_eq!(status_of(&a, &b)["state"], "synced");

    // So does a session that ended with no verify to give, one copy ahead
    // of the other (da-t3ey2); one that failed keeps it.
    a.ok(&["sync", "record-session", &b.id(), "--verify", "mismatch"]);
    a.ok(&["sync", "record-session", &b.id(), "--error", "cut"]);
    assert_eq!(status_of(&a, &b)["last_verify"]["match"], false);
    // b being down since is no session either: the mismatch stands (da-ueb4p).
    a.ok(&[
        "sync",
        "record-session",
        &b.id(),
        "--error",
        "gone",
        "--offline",
    ]);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["error"].as_str()),
        (Some("error"), Some("verify MISMATCH")),
        "{row}"
    );
    a.ok(&["sync", "record-session", &b.id()]);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["last_verify"].is_null()),
        (Some("pending"), true),
        "{row}"
    );
    session(&a, &b);

    // One that never reached b, as datum.day's transport says it: offline,
    // its words in the JSON alone (da-ueb4p).
    let why = "could not reach b: timed out";
    a.ok(&[
        "sync",
        "record-session",
        &b.id(),
        "--error",
        why,
        "--offline",
    ]);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["error"].as_str()),
        (Some("offline"), Some(why)),
        "{row}"
    );
    let table = a.ok(&["sync", "status"]);
    assert!(
        table.contains(" offline ") && !table.contains(why),
        "{table}"
    );
    session(&a, &b);

    // b revoked, what it makes is refused, and the transport says so.
    a.ok(&["sync", "chain", "expel", "b"]);
    b.ok(&["occur", "after"]);
    let (ok, why) = b.send(&a);
    assert!(!ok, "{why}");
    let why = why.lines().find(|l| l.starts_with("refused:")).unwrap();
    a.ok(&["sync", "record-session", &b.id(), "--error", why]);
    let row = status_of(&a, &b);
    assert_eq!(
        (row["state"].as_str(), row["revoked"].as_bool()),
        (Some("error"), Some(true)),
        "{row}"
    );
    assert!(
        row["error"]
            .as_str()
            .unwrap()
            .contains("made after its endpoint was expelled"),
        "{row}"
    );
    let table = a.ok(&["sync", "status", "--show-revoked"]);
    assert!(
        table.contains("error") && table.contains("EXPELLED"),
        "{table}"
    );
}

#[test]
fn status_of_a_copy_in_no_chain_is_empty() {
    let a = Copy::new();
    // A read creates no database (da-ov6x5): `db migrate` makes it.
    a.ok(&["db", "migrate"]);
    let all = json(&a.ok(&["sync", "status", "--json"]));
    assert!(all["chain"].is_null(), "{all}");
    assert_eq!(all["members"], serde_json::json!([]));
    assert_eq!(all["this"]["endpoint"], a.id().as_str());
    assert!(a.ok(&["sync", "status"]).contains("no sync chain"));
    // A session with a copy that is no member has nowhere to go.
    let stranger = Copy::new().id();
    let out = a.fails(
        &["sync", "record-session", &stranger, "--verify", "match"],
        "",
    );
    assert!(
        out.contains("not a member of this copy's sync chain"),
        "{out}"
    );
}

/// An open chain says so beside its members, quietly; a locked one does not.
#[test]
fn status_says_a_chain_is_open() {
    let (a, b) = chained();
    let open = "no admin: any device can add devices — `datum sync admin add` to protect the chain";
    assert!(a.ok(&["sync", "status"]).contains(open));
    assert_eq!(json(&a.ok(&["sync", "status", "--json"]))["open"], true);
    a.ok(&["sync", "admin", "add", &a.key("admin.key")]);
    assert!(a.send(&b).0);
    for copy in [&a, &b] {
        assert!(!copy.ok(&["sync", "status"]).contains(open));
        assert_eq!(json(&copy.ok(&["sync", "status", "--json"]))["open"], false);
    }
}
