//! `db purge` on a chained copy (da-n0zx1, da-qjxfx): refused with no flag,
//! `--leave`, `--rejoin`, and a purge under an open session.

use super::*;

/// A purge between two rounds of datum.day's session leaves the file as
/// empty as it left it (da-otz0d). Its store runs each round as `sync --as
/// <the endpoint the session began as> …`, a process a command: after the
/// purge the commands that write fail, where the same apply with no `--as`
/// takes the peer's whole chain, as a copy in no chain does.
#[test]
fn a_purge_between_the_rounds_of_a_piped_session_leaves_the_file_empty() {
    let (a, b) = chained();
    let (me, peer) = (a.id(), b.id());
    fn session<'a>(me: &'a str, cmd: &[&'a str]) -> Vec<&'a str> {
        [&["sync", "--as", me], cmd].concat()
    }
    b.ok(&["occur", "juice"]);
    let vector = a.ok(&session(&me, &["pipe", "vector"]));
    let ops = b.ok(&["sync", "pipe", "ops", "--since", vector.trim()]);
    a.ok_in(&session(&me, &["pipe", "apply", "--json"]), &ops);
    let done = ["record-session", &peer, "--verify", "match"];
    a.ok(&session(&me, &done));
    assert!(a.has("juice"));

    a.ok(&["db", "purge", "--leave", "--force", "--yes"]);
    let all = b.ok(&["sync", "pipe", "ops"]);
    let hello = ["handshake", &peer, "--label", "b", "--addr", "{}"];
    let rounds: [(&[&str], &str); 5] = [
        (&["pipe", "ops", "--since", "{}", "--peer", &peer], ""),
        (&["pipe", "apply", "--json"], &all),
        (&[&hello[..], &["--dialed"]].concat(), ""),
        (&hello, ""),
        (&done, ""),
    ];
    for (cmd, stdin) in rounds {
        let why = a.fails(&session(&me, cmd), stdin);
        assert!(why.contains("endpoint key changed"), "{cmd:?}: {why}");
    }
    for table in ["ops", "entries", "sync_peers", "sync_local"] {
        let rows = a.query(&format!("SELECT COUNT(*) FROM {table}"));
        assert_eq!(rows, "0", "{table}");
    }

    a.ok_in(&["sync", "pipe", "apply"], &all);
    assert!(a.has("juice"));
}

/// `db purge` on a chained copy leaves its chain, purges all of it or
/// rejoins it (da-qjxfx): with none of the three said it refuses, naming
/// them, and changes nothing, as `db restore --hard` does (da-n0zx1).
#[test]
fn a_chained_purge_without_a_flag_is_refused_naming_all_three() {
    let (a, _b) = chained();
    a.ok(&["occur", "tea"]);
    let ops = a.query("SELECT COUNT(*) FROM ops");
    for argv in [&["db", "purge"][..], &["db", "purge", "--yes"]] {
        let refused = a.fails(argv, "");
        assert!(
            ["--leave", "--chain", "--rejoin"]
                .iter()
                .all(|f| refused.contains(f)),
            "{argv:?}: {refused}"
        );
    }
    assert!(a.has("tea"));
    assert_eq!(a.query("SELECT COUNT(*) FROM ops"), ops);
    let unchained = Copy::new();
    unchained.ok(&["occur", "tea"]);
    for flag in ["--leave", "--rejoin"] {
        let refused = unchained.fails(&["db", "purge", flag, "--yes"], "");
        assert!(refused.contains("in no sync chain"), "{flag}: {refused}");
    }
    assert!(unchained.has("tea"));
}

/// `--leave` signs the departure first, as `chain leave` does, and wipes
/// only once a member holds it: the other members list the copy as left,
/// and it is in no chain, a new copy.
#[test]
fn a_purge_with_leave_tells_the_chain_then_wipes() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    b.ok(&["occur", "tea"]);
    let old_id = b.id();
    b.ok(&["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"]);
    let refused = b.fails(&["db", "purge", "--leave", "--yes"], "");
    assert!(
        refused.contains("no member holds this copy's departure")
            && refused.contains("`datum db purge --leave` again"),
        "{refused}"
    );
    assert!(b.has("tea"));
    assert!(b.send(&a).0);
    let vector = a.ok(&["sync", "pipe", "vector"]);
    b.ok(&["sync", "record-session", &a.id(), "--vector", vector.trim()]);

    let purged = b.ok(&["db", "purge", "--leave", "--yes"]);
    assert!(purged.contains("its departure is with: "), "{purged}");
    assert!(!b.has("tea"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
    assert!(b
        .fails(&["sync", "chain", "list"], "")
        .contains("in no sync chain"));
    assert_ne!(b.id(), old_id);
    assert!(a.send(&c).0);
    for x in [&a, &c] {
        let list = x.ok(&["sync", "chain", "list", "--show-revoked"]);
        let row = list.lines().find(|l| l.contains(&old_id[..10])).unwrap();
        assert!(row.contains("  LEFT "), "{list}");
        assert!(x.has("tea"));
    }
}

/// `--rejoin` wipes the copy and keeps its endpoint key, so it stays a
/// member and refetches everything, its own changes too, which come back
/// signed by the key it still holds. Refused, changing nothing, while one
/// of them has reached no other member; and until they are back it writes
/// nothing, which would number a new op from a seq it used.
#[test]
fn a_purge_with_rejoin_refetches_everything_and_stays_a_member() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    a.ok(&["occur", "coffee"]);
    b.ok(&["occur", "tea"]);
    b.ok(&["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"]);
    let ops = b.query("SELECT COUNT(*) FROM ops");
    let refused = b.fails(&["db", "purge", "--rejoin", "--yes"], "");
    assert!(
        refused.contains("made here") && refused.contains("reached no other member"),
        "{refused}"
    );
    assert!(b.has("tea"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), ops);

    assert!(b.send(&a).0 && a.send(&b).0 && a.send(&c).0);
    let vector = a.ok(&["sync", "pipe", "vector"]);
    b.ok(&["sync", "record-session", &a.id(), "--vector", vector.trim()]);
    // An entry no op covers yet (a bulk import's) is one more the wipe
    // would lose.
    let coffee = "(SELECT hid FROM entries WHERE field = 'coffee')";
    b.exec(&format!(
        "UPDATE ops SET hid = 'x' || hid WHERE hid = {coffee}"
    ));
    let refused = b.fails(&["db", "purge", "--rejoin", "--yes"], "");
    assert!(refused.contains("1 change made here"), "{refused}");
    b.exec(&format!(
        "UPDATE ops SET hid = substr(hid, 2) WHERE hid = 'x' || {coffee}"
    ));
    let id = b.id();
    // A change queued before the purge is the old copy's, and goes with it
    // (da-owy96).
    let holder = rusqlite::Connection::open(b.tmp.path().join("data.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(b.ok(&["occur", "queued"]).contains("QUEUED"));
    drop(holder);
    let purged = b.ok(&["db", "purge", "--rejoin", "--yes"]);
    assert!(
        purged.contains("kept: this copy's endpoint key")
            && purged.contains("removed: 1 queued change(s)"),
        "{purged}"
    );
    assert!(
        purged.contains("refetching from ") && purged.contains("not refetched yet"),
        "{purged}"
    );
    assert!(!b.has("tea") && !b.has("coffee"));
    assert_eq!(b.id(), id);
    let write = b.fails(&["occur", "late"], "");
    assert!(write.contains("refetching its sync chain"), "{write}");

    let (ok, out) = a.send(&b);
    assert!(ok, "{out}");
    assert!(b.has("tea") && b.has("coffee"));
    assert_eq!(chain_of(&b), chain_of(&a));
    assert_eq!(b.hash(), a.hash());
    assert_eq!(c.hash(), a.hash());
    b.ok(&["occur", "late"]);
    assert!(b.send(&a).0 && b.send(&c).0);
    assert!(a.has("late") && c.has("late"));
    assert!(!a.has("queued") && !b.has("queued"));
    assert_eq!(b.hash(), a.hash());
    assert_eq!(c.hash(), a.hash());
}
