//! `sync chain list` and `sync admin list` print each instant on the
//! reader's clock, as `sync status` does (da-awd9c); `--json` keeps the
//! stored instants.

use super::*;

/// `at` as a reader at +02 reads it: the time alone today.
fn at_plus_two(at: &str) -> String {
    let zone = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
    let t = chrono::DateTime::parse_from_rfc3339(at)
        .unwrap()
        .with_timezone(&zone);
    let today = chrono::Utc::now().with_timezone(&zone).date_naive();
    let format = if t.date_naive() == today {
        "%H:%M"
    } else {
        "%Y-%m-%d %H:%M"
    };
    t.format(format).to_string()
}

#[test]
fn chain_list_prints_its_instants_on_the_readers_clock() {
    let (a, b) = chained();
    let admin = a.key("admin.key");
    a.ok(&["sync", "admin", "add", &admin]);
    a.ok(&["sync", "chain", "expel", "b", "--admin-key", &admin]);
    let at_two = |args: &[&str]| {
        let out = a.cmd(args).env("TZ", "Etc/GMT-2").output().unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let list = at_two(&["sync", "chain", "list", "--show-revoked"]);
    let all = json(&a.ok(&["sync", "chain", "list", "--json"]));
    let raw = |v: &serde_json::Value| v.as_str().unwrap().to_string();
    let added = raw(&all["admins"][0]["added"]);
    let b_row = member_of(&a, &b.id());
    let shown = [
        ("admitted", raw(&member_of(&a, &a.id())["admitted"])),
        ("admitted", raw(&b_row["admitted"])),
        ("EXPELLED", raw(&b_row["revoked"])),
        ("added", added.clone()),
    ];
    for (word, instant) in shown {
        assert!(instant.ends_with('Z'), "--json keeps {instant}");
        assert!(!list.contains(&instant), "{list}");
        let local = format!("{word} {}", at_plus_two(&instant));
        assert!(list.contains(&local), "{local} in {list}");
    }
    let admins = at_two(&["sync", "admin", "list"]);
    assert!(
        admins.contains(&format!("added {}", at_plus_two(&added))),
        "{admins}"
    );
}

/// This copy's departure is LEAVING in its own row while no member holds
/// it, as `leave` and `status` say, and LEFT once one does (da-9bxhr).
#[test]
fn chain_list_says_a_departure_no_member_holds_is_pending() {
    let (a, b) = chained();
    b.ok(&["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"]);
    b.fails(&["sync", "chain", "leave"], "");
    let own_row = || {
        let list = b.ok(&["sync", "chain", "list"]);
        list.lines()
            .find(|l| l.contains("(this copy)"))
            .unwrap()
            .to_string()
    };
    let row = own_row();
    assert!(
        row.contains("  LEAVING: signed ") && row.contains(", finishes once a member holds it"),
        "{row}"
    );
    assert!(!row.contains("LEFT"), "{row}");
    assert!(b.send(&a).0);
    let vector = a.ok(&["sync", "pipe", "vector"]);
    b.ok(&["sync", "record-session", &a.id(), "--vector", vector.trim()]);
    let row = own_row();
    assert!(row.contains("  LEFT ") && !row.contains("LEAVING"), "{row}");
}

/// One install is one device (da-n0uj1.2): each of its databases, in a
/// chain of its own, binds its own endpoint to the one device key in its
/// data directory, made at the first need and kept for every later process.
/// Another install is another device, and its binding reaches the chain's
/// other members with its describe.
#[test]
fn an_installs_databases_bind_their_endpoints_to_one_device_key() {
    let (a, b) = chained();
    let in_db = |args: &[&str]| {
        let out = a.cmd(args).env("DATUM_DB", a.key("other.db")).output();
        let out = out.unwrap();
        assert!(out.status.success(), "{out:?}");
        json(&String::from_utf8(out.stdout).unwrap())
    };
    in_db(&["sync", "new-invite"]);
    let other = in_db(&["sync", "chain", "list", "--json"]);
    let device = member_of(&a, &a.id())["device"].clone();
    assert!(device.as_str().is_some_and(|d| d.len() == 64), "{device}");
    assert_eq!(other["members"][0]["device"], device, "{other}");
    assert_ne!(other["members"][0]["endpoint"], a.id().as_str());
    assert_ne!(
        other["chain"],
        json(&a.ok(&["sync", "chain", "list", "--json"]))["chain"]
    );
    assert_eq!(
        json(&a.ok(&["sync", "status", "--json"]))["this"]["device"],
        device
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = std::fs::metadata(a.tmp.path().join("home/device_key")).unwrap();
        assert_eq!(file.permissions().mode() & 0o777, 0o600);
    }
    // b is admitted and has not described itself: bound to nothing, until it does.
    assert_eq!(member_of(&a, &b.id())["device"], serde_json::Value::Null);
    b.ok(&["sync", "chain", "rename", "bee"]);
    assert!(b.send(&a).0);
    let theirs = member_of(&a, &b.id())["device"].clone();
    assert_eq!(
        theirs,
        json(&b.ok(&["sync", "status", "--json"]))["this"]["device"]
    );
    assert!(theirs.is_string() && theirs != device, "{theirs}");
}

/// A copy chained before the binding is bound at its next sync, its label
/// kept (da-n0uj1.2): it describes itself again once, and not after. A
/// device key file, or a `DATUM_DEVICE_KEY`, that holds no key is said, by
/// name, and the sync goes on.
#[test]
fn a_copy_chained_unbound_is_bound_at_its_next_sync() {
    let a = Copy::new();
    for args in [
        &["sync", "new-invite"][..],
        &["sync", "chain", "rename", "lap"],
    ] {
        let out = a
            .cmd(args)
            .env("DATUM_DEVICE_KEY", "none")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }
    let row = || member_of(&a, &a.id());
    assert_eq!(row()["device"], serde_json::Value::Null);
    let records = || a.query("SELECT COUNT(*) FROM ops WHERE kind = 'chain'");
    a.ok(&["sync"]);
    let (bound, held) = (row(), records());
    assert_eq!(bound["label"], "lap");
    assert_eq!(
        bound["device"],
        json(&a.ok(&["sync", "status", "--json"]))["this"]["device"]
    );
    a.ok(&["sync"]);
    assert_eq!(records(), held, "bound, a sync describes nothing more");
    std::fs::write(a.tmp.path().join("home/device_key"), "").unwrap();
    let said = a.stderr_of(&["sync"]);
    assert!(said.contains("device_key holds no device key"), "{said}");
    let out = a
        .cmd(&["sync"])
        .env("DATUM_DEVICE_KEY", "none")
        .output()
        .unwrap();
    let said = String::from_utf8(out.stderr).unwrap();
    assert!(said.contains("DATUM_DEVICE_KEY is no device key"), "{said}");
}
