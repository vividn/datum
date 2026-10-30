//! A copy's file changing under its chain: a purge, a hard restore, a rejoin, and a
//! migration between an open session's rounds.

use super::*;

/// `db purge` destroys sync too (da-8v7ua, vividn's reproduction): a copy
/// that synced, revoked the other and was refused `chain leave --alone` keeps
/// nothing after a purge — no op, no chain, no key — and its next invite
/// starts a new chain as a new endpoint.
#[test]
fn a_purge_drops_the_ops_the_chain_and_the_endpoint_key() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "coffee"]);
    let (mut inviting, ticket) = a.invite(&[]);
    assert!(b
        .ok(&["sync", "chain", "join", &ticket])
        .contains("verify ok"));
    assert!(inviting.finish().0);
    b.ok(&["sync", "chain", "expel", &a.id()]);
    let held = b.fails(&["sync", "chain", "leave", "--alone"], "");
    assert!(held.contains("holds other copies' data"), "{held}");
    let (old_id, old_chain) = (b.id(), b.query(CHAIN));

    let purged = b.ok(&["db", "purge", "--leave", "-y"]);
    for table in ["entries", "ops", "sync_local", "sync_peers", "sync_notices"] {
        assert!(purged.contains(table), "{table} not dropped: {purged}");
    }
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
    assert_eq!(b.query("SELECT COUNT(*) FROM sync_local"), "0");
    let list = b.fails(&["sync", "chain", "list"], "");
    assert!(list.contains("in no sync chain"), "{list}");

    let (_inviting, _) = b.invite(&[]);
    assert_ne!(b.id(), old_id);
    assert_ne!(b.query(CHAIN), old_chain);
}

/// `db restore --hard --leave` takes the ops and the chain with the rows,
/// and plain `--hard` refuses a chained copy (da-95y4o):
/// ops kept for a row it dropped fold that row back on every peer, and the
/// two never verify. The copy is a new one, in no chain, and a fresh invite
/// brings it back: both verify ok, and the peer's data comes back to it.
#[test]
fn a_hard_restore_leaves_the_chain_and_rejoins_by_invite() {
    let (a, b) = paired();
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["occur", "tea"]);
    let backup = b.tmp.path().join("b.jsonl");
    b.ok(&["db", "backup", backup.to_str().unwrap()]);
    b.ok(&["occur", "juice"]);
    b.ok(&["sync"]);
    assert!(a.has("juice"));
    let (old_id, old_chain) = (b.id(), b.query(CHAIN));

    let backup = backup.to_str().unwrap();
    // Without --leave it is refused: the chain would keep what it drops.
    let ops = b.query("SELECT COUNT(*) FROM ops");
    let refused = b.fails(&["db", "restore", backup, "--hard", "--yes"], "");
    assert!(
        refused.contains(&format!("in sync chain {old_chain}")) && refused.contains("--leave"),
        "{refused}"
    );
    assert!(b.has("juice"));
    assert_eq!(
        (b.query("SELECT COUNT(*) FROM ops"), b.id()),
        (ops, old_id.clone())
    );

    let restored = b.ok(&["db", "restore", backup, "--hard", "--leave", "--yes"]);
    assert!(
        restored.contains(&format!("left sync chain {old_chain}")),
        "{restored}"
    );
    assert!(!b.has("juice"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
    let list = b.fails(&["sync", "chain", "list"], "");
    assert!(list.contains("in no sync chain"), "{list}");
    assert_ne!(b.id(), old_id);

    drop(_a_runs);
    let (mut inviting, ticket) = a.invite(&[]);
    assert!(b
        .ok(&["sync", "chain", "join", &ticket])
        .contains("verify ok"));
    assert!(inviting.finish().0);
    assert_eq!(b.query(CHAIN), old_chain);
    assert!(b.has("juice"));
    let _a_runs = a.serve(&["sync", "run"]);
    assert!(b.ok(&["sync"]).contains("verify ok"));
    for x in [&a, &b] {
        x.ok(&["check", "sync"]);
    }
}

/// A `--hard --leave` no member can be told of is refused whole, naming `sync
/// chain leave`, whose `--force` tells no one; then it goes through
/// (da-95y4o). Restore's own `--force` is its conflict strategy, and
/// `--hard` refuses it.
#[test]
fn a_hard_restore_no_member_hears_of_is_refused_and_names_chain_leave() {
    let (a, b) = paired();
    b.ok(&["occur", "tea"]);
    let backup = b.tmp.path().join("b.jsonl");
    let backup = backup.to_str().unwrap();
    b.ok(&["db", "backup", backup]);
    b.ok(&["occur", "juice"]);
    drop(a);

    let refused = b.fails(&["db", "restore", backup, "--hard", "--leave", "--yes"], "");
    assert!(
        refused.contains("`datum sync chain leave` again") && refused.contains("`--force`"),
        "{refused}"
    );
    assert!(b.has("juice"));
    b.ok(&["sync", "chain", "leave", "--force"]);
    b.ok(&["db", "restore", backup, "--hard", "--yes"]);
    assert!(!b.has("juice"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
}

const CHAIN: &str =
    "SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1";

/// `db purge --rejoin` over iroh (da-qjxfx): the purge's own first sync
/// refetches everything from the member that serves, the copy keeps its
/// endpoint and its chain, and the next `datum sync` verifies with each.
#[test]
fn a_purge_with_rejoin_refetches_from_the_chain_and_verifies() {
    let (a, b) = paired();
    let c = Copy::new();
    let (mut inviting, ticket) = a.invite(&[]);
    c.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "coffee"]);
    // c is down: each sync fails it, and a's answer is enough.
    let _a_runs = a.serve(&["sync", "run"]);
    b.run(&["sync"]);
    c.run(&["sync"]);
    assert!(c.has("coffee"));
    let (id, chain) = (b.id(), b.query(CHAIN));

    let purged = b.ok(&["db", "purge", "--rejoin", "-y"]);
    assert!(
        purged.contains("verify ok") && !purged.contains("not refetched"),
        "{purged}"
    );
    assert!(b.has("tea") && b.has("coffee"));
    assert_eq!((b.id(), b.query(CHAIN)), (id, chain));
    b.ok(&["occur", "walk"]);
    let _c_runs = c.serve(&["sync", "run"]);
    // c's fresh run may be applying a session of its own (its first dial, or
    // a's dial again) as b dials, and refuses b as busy, which is no
    // failure: b syncs again until c answers (da-10c6e).
    let mut synced = b.ok(&["sync"]);
    for _ in 0..20 {
        if !synced.contains("is busy applying") {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
        synced = b.ok(&["sync"]);
    }
    assert_eq!(synced.matches("verify ok").count(), 2, "{synced}");
    for x in [&a, &b, &c] {
        assert!(x.has("walk"));
        x.ok(&["check", "sync"]);
    }
}

/// A copy purged while `sync run` serves it (the daemon's loop) stops
/// serving the old key, and a member of the old chain that dials it gets
/// nothing and sends nothing (da-8v7ua).
#[test]
fn a_purged_copy_serves_the_old_chain_nothing() {
    let (a, b) = paired();
    let mut b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "tea"]);
    a.ok(&["sync"]);
    assert!(b.has("tea"));

    b.ok(&["db", "purge", "--leave", "--force", "-y"]);
    let start = Instant::now();
    while b_runs.child.try_wait().unwrap().is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "run still serving the old key"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let (ok, _) = b_runs.finish();
    let mut why = String::new();
    b_runs
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut why)
        .unwrap();
    assert!(!ok && why.contains("endpoint key changed"), "{why}");

    let _b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "walk"]);
    let entries = a.query("SELECT COUNT(*) FROM entries");
    // The old key answers nowhere: offline, and nothing synced (da-9d6uy).
    let synced = a.fails(&["sync"], "");
    assert!(synced.contains(": offline, last seen "), "{synced}");
    assert!(!b.has("tea") && !b.has("walk"));
    assert_eq!(b.query("SELECT COUNT(*) FROM ops"), "0");
    assert_eq!(a.query("SELECT COUNT(*) FROM entries"), entries);
}

/// A purge that lands between an open session's rounds leaves the file as
/// empty as it left it (da-otz0d): the session stops at its next round, where
/// it took the peer's whole chain — genesis first, as a copy in no chain
/// does — and wrote the peer's row back. `b` applies slowly, so `a`, which
/// has applied its first round, waits a round on it: the purge lands there.
#[test]
fn a_purge_between_an_open_sessions_rounds_leaves_the_file_empty() {
    let (a, mut b) = paired();
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "juice"]);
    b.env = vec![("DATUM_SYNC_TEST_APPLY_DELAY_MS", "2000")];
    let _b_runs = b.serve(&["sync", "run"]);
    let a_syncs = a.cmd(&["sync"]).spawn().unwrap();
    eventually("a's first round", || a.has("juice"));
    a.ok(&["db", "purge", "--leave", "--force", "-y"]);

    let out = a_syncs.wait_with_output().unwrap();
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success() && said.contains("endpoint key changed"),
        "{said}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for table in ["ops", "entries", "sync_peers", "sync_local"] {
        let rows = a.query(&format!("SELECT COUNT(*) FROM {table}"));
        assert_eq!(rows, "0", "{table}");
    }
}

/// Stamp `copy`'s file one schema on, as a newer datum's migration leaves
/// it. What an older build then says of the two versions.
fn migrate_under(copy: &Copy) -> String {
    let now = CURRENT_SCHEMA_VERSION + 1;
    let stamped = rusqlite::Connection::open(copy.db())
        .unwrap()
        .execute(
            "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
            [now.to_string()],
        )
        .unwrap();
    assert_eq!(stamped, 1);
    format!("at schema v{now} and this datum writes v{CURRENT_SCHEMA_VERSION}")
}

/// A newer datum's migration that lands between an open session's rounds
/// stops the session, which names both versions and writes nothing more
/// into the file (da-1tnup): its rows would be the old shape. `b` applies
/// slowly, so `a` waits a round on it, as for the purge above.
#[test]
fn a_migration_between_an_open_sessions_rounds_stops_it_and_it_writes_nothing_more() {
    let (a, mut b) = paired();
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "juice"]);
    b.env = vec![("DATUM_SYNC_TEST_APPLY_DELAY_MS", "2000")];
    let _b_runs = b.serve(&["sync", "run"]);
    let a_syncs = a.cmd(&["sync"]).spawn().unwrap();
    eventually("a's first round", || a.has("juice"));
    let versions = migrate_under(&a);
    let before = held(&a);

    let out = a_syncs.wait_with_output().unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success() && said.contains(&versions), "{said}");
    assert_eq!(held(&a), before);
}

/// `sync run` and the daemon over it stop, failed and naming both versions,
/// once a newer datum has migrated the file (da-1tnup): the unit then starts
/// the installed binary. Neither writes into it again.
#[test]
fn a_migration_under_sync_run_or_the_daemon_stops_it() {
    for daemon in [false, true] {
        let (a, b) = paired();
        let config = b.tmp.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        let registry = format!("[db.data]\npath = {:?}\n", b.db().display().to_string());
        std::fs::write(config.join("dbs.toml"), registry).unwrap();
        let args: &[&str] = if daemon {
            &["sync", "daemon", "run"]
        } else {
            &["sync", "run"]
        };
        let mut b_runs = b.serve(args);
        a.ok(&["occur", "tea"]);
        eventually("a's sync reaching b", || {
            a.run(&["sync"]).status.success() && b.has("tea")
        });

        let versions = migrate_under(&b);
        let before = held(&b);
        let start = Instant::now();
        while b_runs.child.try_wait().unwrap().is_none() && start.elapsed().as_secs() < 10 {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = b_runs.child.kill();
        let (ok, _) = b_runs.finish();
        let mut why = String::new();
        let mut stderr = b_runs.child.stderr.take().unwrap();
        stderr.read_to_string(&mut why).unwrap();
        assert!(!ok && why.contains(&versions), "daemon {daemon}: {why}");
        assert_eq!(held(&b), before, "daemon {daemon}");
    }
}

/// A purge leaves nothing of the database, as deleting its file would
/// (da-nc8i1), and with `sync run` holding the file open: no page of what it
/// held in the file or its `-wal`, no row but a new copy's own, and none of
/// its pre-migration snapshots, each a whole copy of an earlier state, which
/// the summary names. Another database's snapshot beside it stays.
#[test]
fn a_purge_leaves_nothing_of_the_database_in_its_file_or_beside_it() {
    const MARK: &str = "zebra-marker-77";
    let (a, b) = paired();
    let _b_runs = b.serve(&["sync", "run"]);
    // Long enough to fill more pages than the new schema takes back: a page
    // it reuses is zeroed whether or not the file is rewritten.
    a.ok(&["occur", "tea", &format!("note={}", MARK.repeat(8000))]);
    a.ok(&["sync"]);
    assert!(b.has("tea"));
    let old_id = b.id();
    let own = b
        .tmp
        .path()
        .join("data.db.pre-v1-to-v2-20260101T000000Z.bak");
    std::fs::write(&own, MARK).unwrap();
    std::fs::write(
        b.tmp
            .path()
            .join("other.db.pre-v1-to-v2-20260101T000000Z.bak"),
        MARK,
    )
    .unwrap();
    // The files beside the database that hold the marker, by name.
    let marked = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(b.tmp.path())
            .unwrap()
            .flatten()
            .filter(|e| {
                std::fs::read(e.path())
                    .is_ok_and(|bytes| bytes.windows(MARK.len()).any(|w| w == MARK.as_bytes()))
            })
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    };
    let before = marked();
    assert!(
        before.iter().any(|n| n == "data.db" || n == "data.db-wal") && before.len() >= 3,
        "{before:?}"
    );

    let purged = b.ok(&["db", "purge", "--leave", "--force", "-y"]);
    assert!(
        purged.contains(&format!("removed: {}", own.display())) && !purged.contains("other.db"),
        "{purged}"
    );
    assert_eq!(marked(), ["other.db.pre-v1-to-v2-20260101T000000Z.bak"]);
    // `_meta` is the new schema's stamp, and `sync_local` the key `sync run`
    // mints for the new copy before it stops.
    let tables = b.query(
        "SELECT group_concat(name) FROM sqlite_master WHERE type = 'table' \
         AND name NOT IN ('_meta', 'sync_local')",
    );
    for table in tables.split(',') {
        let rows = b.query(&format!("SELECT COUNT(*) FROM \"{table}\""));
        assert_eq!(rows, "0", "{table}");
    }
    assert_eq!(
        b.query(&format!(
            "SELECT COUNT(*) FROM sync_local WHERE endpoint_id = '{old_id}'"
        )),
        "0"
    );
}

/// A revoked device rejoins by a fresh invite to its own chain (da-j8c4l),
/// as a new device joins: what each holds, both will — the entry it logged
/// while out among them — and it stays in that chain, with its endpoint.
#[test]
fn a_revoked_device_rejoins_by_a_fresh_invite_and_data_goes_both_ways() {
    let (a, b) = paired();
    let chain = a
        .query("SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1");
    let b_id = b.id();
    a.ok(&["sync", "chain", "expel", &b_id]);
    b.ok(&["occur", "out"]);
    a.ok(&["occur", "meanwhile"]);
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.ok(&["sync", "chain", "join", &ticket]);
    assert!(joined.contains("verify ok"), "{joined}");
    assert!(!joined.contains("is replaced by"), "{joined}");
    assert!(inviting.finish().0);
    assert!(a.has("out") && b.has("meanwhile"));
    assert_eq!(a.hash(), b.hash());
    assert_eq!(b.id(), b_id);
    let list = b.ok(&["sync", "chain", "list", "--json"]);
    assert!(list.contains(&chain), "{list}");
    let list = a.ok(&["sync", "chain", "list"]);
    assert!(
        !list.contains("EXPELLED") && list.contains(", again "),
        "{list}"
    );
}
