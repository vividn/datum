//! A device taken out of the chain with all it signed (`purge` records), and
//! one brought back.

use super::*;

/// How `copy` holds the three entries the purge test is about: whether it
/// has coffee, tea's cups, whether juice is deleted.
fn purge_view(copy: &Copy) -> (bool, String, bool) {
    (
        copy.has("coffee"),
        copy.query("SELECT json_extract(data, '$.cups') FROM entries WHERE field = 'tea'"),
        copy.query("SELECT deleted_at IS NOT NULL FROM entries WHERE field = 'juice'") == "1",
    )
}

/// Nothing in `datum check` but the notices of members it knows of.
fn check_clean(copy: &Copy, others: &[&Copy]) {
    for o in others {
        copy.run(&["sync", "chain", "ack", &o.id()[..10]], "");
    }
    let check = copy.ok(&["check"]);
    assert!(check.contains("No consistency issues found"), "{check}");
}

/// A device that only made specs signed no change to an entry, and its
/// preview says so rather than a span of zeroes — and that its specs go
/// too, which a purge does (da-sivjb).
#[test]
fn a_purge_preview_of_a_device_that_only_made_specs_says_they_go() {
    let (a, b) = chained();
    b.ok(&["setup", "coffee", "--verb", "occur"]);
    assert!(b.send(&a).0);
    let spec = || a.ok(&["setup", "coffee", "--json"]);
    assert!(!spec().contains("No spec defined"), "{}", spec());
    let preview = a.ok(&["sync", "chain", "unjoin", "b"]);
    assert!(
        preview.contains("signed no change to an entry, only 1 definition: unjoining it expels it and sets that aside"),
        "{preview}"
    );
    let p = json(&a.ok(&["sync", "chain", "unjoin", "b", "--json"]));
    assert!(
        p["first"].is_null() && p["last"].is_null() && p["defined"] == 1,
        "{p}"
    );
    a.ok(&["sync", "chain", "unjoin", "b", "--yes"]);
    assert!(spec().contains("No spec defined"), "{}", spec());
}

/// A purge (da-ry11c, vividn 10-07: «if you accidentally add a poisoned
/// device … you can scrape out all the bad data easily») sets aside every
/// op its endpoint signed, on every copy: what it created goes, its edit of
/// another device's entry and its delete are undone. A copy that hears of
/// it later — and the purged copy itself — does the same.
#[test]
fn a_purge_sets_aside_a_devices_ops_on_every_copy() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    a.ok(&["occur", "tea", "cups=1"]);
    a.ok(&["occur", "juice"]);
    assert!(a.send(&b).0);
    let tea = a.query("SELECT hid FROM entries WHERE field = 'tea'");
    let juice = a.query("SELECT hid FROM entries WHERE field = 'juice'");
    b.ok(&["occur", "coffee"]);
    b.ok(&["update", &tea, "cups=3"]);
    b.ok(&["delete", &juice]);
    assert!(b.send(&a).0);
    let poisoned = (true, "3".to_string(), true);
    assert_eq!(purge_view(&a), poisoned);

    // The old spelling is refused, naming the new (da-c9fiy).
    let old = a.fails(&["sync", "chain", "purge", "b", "--yes"], "");
    assert!(old.contains("`sync chain unjoin <WHO>`"), "{old}");
    // The preview names what goes, and changes nothing.
    let preview = a.ok(&["sync", "chain", "unjoin", "b"]);
    assert!(
        preview.contains("b (")
            && preview.contains("1 entry it created goes,")
            && preview.contains("1 it edited and 1 it deleted")
            && preview.contains("nothing changed: `datum sync chain unjoin b --yes`"),
        "{preview}"
    );
    let p = json(&a.ok(&["sync", "chain", "unjoin", "b", "--json"]));
    assert_eq!(
        (&p["created"], &p["edited"], &p["deleted"], &p["purged"]),
        (&1.into(), &1.into(), &1.into(), &serde_json::Value::Null),
        "{p}"
    );
    assert!(p["first"].is_string() && p["last"].is_string(), "{p}");
    assert_eq!(purge_view(&a), poisoned);

    let purged = a.ok(&["sync", "chain", "unjoin", "b", "--yes"]);
    assert!(purged.starts_with("unjoined: b ("), "{purged}");
    let clean = (false, "1".to_string(), false);
    assert_eq!(purge_view(&a), clean);
    let again = a.fails(&["sync", "chain", "unjoin", "b", "--yes"], "");
    assert!(again.contains("unjoined already"), "{again}");
    assert!(a
        .ok(&["sync", "chain", "list", "--show-revoked"])
        .contains("UNJOINED"));

    // c syncs after the purge, from a: b's ops arrive with it, set aside.
    let (_, text) = a.send(&c);
    assert!(text.contains("its endpoint was unjoined"), "{text}");
    assert_eq!(purge_view(&c), clean);
    // b itself hears of it, and its own ops go.
    a.send(&b);
    assert_eq!(purge_view(&b), clean);
    // b's ops straight from b change nothing on c.
    b.send(&c);
    assert_eq!(purge_view(&c), clean);
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), c.hash());
    check_clean(&a, &[&b, &c]);
    check_clean(&c, &[&a, &b]);

    // A copy does not unjoin itself.
    let own = a.fails(&["sync", "chain", "unjoin", &a.id()[..10], "--yes"], "");
    assert!(own.contains("does not unjoin itself"), "{own}");
}

/// A PURGED device that rejoins by a fresh invite connects as if for the
/// first time (da-q4rau, vividn 10-08: «It should be like that device is
/// connecting for the first time»): its purge goes with its revoke, so
/// everything it made — before the purge, and while it was out — folds back
/// in on every copy, itself included. There is no unpurge.
#[test]
fn a_purged_device_that_rejoins_brings_all_its_ops_back() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    a.ok(&["occur", "tea", "cups=1"]);
    assert!(a.send(&b).0);
    let tea = a.query("SELECT hid FROM entries WHERE field = 'tea'");
    b.ok(&["occur", "coffee"]);
    b.ok(&["update", &tea, "cups=3"]);
    assert!(b.send(&a).0 && a.send(&c).0);
    a.ok(&["sync", "chain", "unjoin", "b", "--yes"]);
    a.send(&b);
    a.send(&c);
    let cups = "SELECT json_extract(data, '$.cups') FROM entries WHERE field = 'tea'";
    for copy in [&a, &b, &c] {
        assert!(!copy.has("coffee"));
        assert_eq!(copy.query(cups), "1");
    }
    b.ok(&["occur", "out"]);

    let out = redeem(&a, &b, "b", None, &["--chain", &chain_of(&a)]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    a.send(&b);
    b.send(&a);
    a.send(&c);
    for copy in [&a, &b, &c] {
        assert!(copy.has("coffee") && copy.has("out"));
        assert_eq!(copy.query(cups), "3");
        let m = member_of(copy, &b.id());
        assert!(m["purged"].is_null() && m["revoked"].is_null(), "{m}");
        assert!(m["rejoined"].is_string(), "{m}");
    }
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), c.hash());
    check_clean(&a, &[&b, &c]);
    check_clean(&c, &[&a, &b]);
}

/// Two copies logging one coffee at one minute are one entry, the
/// earlier-created row showing both (da-04glw). Purging the device that
/// made it leaves the other's row standing on its own, not merged into a
/// row that is gone.
#[test]
fn a_purge_frees_the_rows_merged_into_the_purged_devices_row() {
    let (a, b) = chained();
    b.ok(&["occur", "coffee", "-t", "1200", "cups=2"]);
    a.ok(&["occur", "coffee", "-t", "1200", "cups=2"]);
    assert!(b.send(&a).0);
    let shown = "SELECT COUNT(*) FROM entries WHERE field = 'coffee' AND superseded_by IS NULL";
    assert_eq!(a.query(shown), "1");
    // b's, the earlier, shows both.
    let b_row = b.query("SELECT hid FROM entries WHERE field = 'coffee'");
    let shown_hid = "SELECT hid FROM entries WHERE field = 'coffee' AND superseded_by IS NULL";
    assert_eq!(a.query(shown_hid), b_row);
    a.ok(&["sync", "chain", "unjoin", "b", "--yes"]);
    assert_eq!(a.query(shown), "1");
    assert_eq!(
        a.query("SELECT COUNT(*) FROM entries WHERE field = 'coffee'"),
        "1"
    );
    check_clean(&a, &[&b]);
}

/// Three copies logging that coffee are one entry with two rows merged in.
/// Freed together they would both claim its slug at once; freed one at a
/// time, the earlier-created of the two shows it and the other merges in.
#[test]
fn a_purge_frees_two_rows_merged_into_the_purged_devices_row() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    b.ok(&["occur", "coffee", "-t", "1200", "cups=2"]);
    a.ok(&["occur", "coffee", "-t", "1200", "cups=2"]);
    c.ok(&["occur", "coffee", "-t", "1200", "cups=2"]);
    assert!(b.send(&a).0);
    assert!(c.send(&a).0);
    // c holds all three too: the purge reaches it by sync, below.
    assert!(a.send(&c).0);
    let rows = "SELECT COUNT(*) FROM entries WHERE field = 'coffee'";
    let shown = "SELECT hid FROM entries WHERE field = 'coffee' AND superseded_by IS NULL";
    assert_eq!(a.query(rows), "3");
    assert_eq!(
        a.query(shown),
        b.query("SELECT hid FROM entries WHERE field = 'coffee'")
    );
    a.ok(&["sync", "chain", "unjoin", "b", "--yes"]);
    assert_eq!(a.query(rows), "2");
    assert_eq!(a.query(shown), a.query("SELECT hid FROM entries WHERE field = 'coffee' AND client_id = (SELECT client_id FROM sync_local)"));
    let live = "SELECT COUNT(*) FROM entries WHERE field = 'coffee'
                AND superseded_by IS NULL AND deleted_at IS NULL";
    assert_eq!(a.query(live), "1");
    // At c the purge arrives by sync, and the session goes through.
    let (_, text) = a.send(&c);
    assert!(text.contains("its endpoint was unjoined"), "{text}");
    assert!(!text.contains("UNIQUE"), "{text}");
    assert_eq!((c.query(rows), c.query(live)), ("2".into(), "1".into()));
    assert_eq!(a.hash(), c.hash());
    check_clean(&a, &[&b, &c]);
    check_clean(&c, &[&a, &b]);
}
