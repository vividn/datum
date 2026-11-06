//! `datum sync chain reenter` (da-vcu8a): a copy a newer datum migrated past
//! its chain comes back at this datum's version by rebuild and replay, never
//! by a down migration.

use super::*;

/// Two copies, one migrated past the other by hand as a newer datum would.
/// The refusal offers the command; the command keeps the file whole beside
/// it, refetches the chain under the same key, replays what the chain never
/// had, and names the one change this datum cannot hold, which stays in the
/// backup.
#[test]
fn a_copy_migrated_past_its_chain_reenters_it_by_rebuild_and_replay() {
    let (a, b) = paired();
    a.ok(&["occur", "tea"]);
    b.ok(&["occur", "coffee"]);
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["sync"]);
    let id = b.id();
    // Made on b after its last sync, so no member holds them.
    b.ok(&["occur", "walk"]);
    b.ok(&["switch", "mood", "calm"]);
    b.ok(&["occur", "swim"]);
    // swim's create carries a column no entry has here, as a newer datum's may.
    let glowing = rusqlite::Connection::open(b.db())
        .unwrap()
        .execute(
            "UPDATE ops SET body = json_set(body, '$.effect.create.glow', 'teal')
             WHERE json_extract(body, '$.effect.create.field') = 'swim'
               AND json_extract(body, '$.kind') = 'create'",
            [],
        )
        .unwrap();
    assert_eq!(glowing, 1);
    super::purge::migrate_under(&b);

    let refused = b.fails(&["tail"], "");
    assert!(refused.contains("datum sync chain reenter"), "{refused}");

    let out = b.ok(&["sync", "chain", "reenter"]);
    assert!(out.contains("verify ok"), "{out}");
    let kept: Vec<&str> = out.lines().filter(|l| l.contains("backup alone")).collect();
    assert_eq!(kept.len(), 1, "{out}");
    assert!(
        kept[0].contains("swim") && kept[0].contains("glow"),
        "{out}"
    );
    assert!(out.contains("replayed"), "{out}");

    // The same member, at this datum's schema, with all it made but swim.
    assert_eq!(b.id(), id);
    assert_eq!(
        b.query("SELECT value FROM _meta WHERE key = 'schema_version'"),
        CURRENT_SCHEMA_VERSION.to_string()
    );
    for field in ["tea", "coffee", "walk", "mood"] {
        assert!(b.has(field), "{field}: {out}");
    }
    assert!(!b.has("swim"), "{out}");
    eventually("a holds what b made", || a.has("walk") && a.has("mood"));

    // The backup is the file as it was: the newer schema, and swim with it.
    let at = out
        .lines()
        .find_map(|l| l.split_once("whole, at "))
        .unwrap()
        .1;
    let backup = rusqlite::Connection::open(at.trim()).unwrap();
    let read = |sql: &str| backup.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
    assert_eq!(
        read("SELECT CAST(value AS INTEGER) FROM _meta WHERE key = 'schema_version'"),
        i64::from(CURRENT_SCHEMA_VERSION) + 1
    );
    assert_eq!(read("SELECT COUNT(*) FROM entries WHERE field = 'swim'"), 1);
    assert_eq!(
        read("SELECT COUNT(*) FROM ops WHERE json_extract(body, '$.effect.create.glow') = 'teal'"),
        1
    );

    b.ok(&["check", "sync"]);
    a.ok(&["check", "sync"]);
    // Once back, nothing is left to re-enter.
    let again = b.fails(&["sync", "chain", "reenter"], "");
    assert!(again.contains("nothing to re-enter"), "{again}");

    // A whole copy of the database, which a purge takes as it takes a
    // migration's snapshots.
    drop(backup);
    let backup = std::path::Path::new(at.trim());
    let purged = b.ok(&["db", "purge", "--rejoin", "-y"]);
    assert!(
        purged.contains(&format!("removed: {}", backup.display())),
        "{purged}"
    );
    assert!(!backup.exists());
}

/// A member holds a change of this copy's that this copy's record of it does
/// not show: the session that carried it was cut before its vector was
/// written. The refetch brings that change back, and its replay lands
/// nothing more: one entry, and the change after it in the spool lands.
#[test]
fn a_change_a_member_holds_unrecorded_is_refetched_and_not_replayed_twice() {
    let (a, b) = paired();
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["occur", "coffee"]);
    b.ok(&["sync"]);
    let id = b.id();
    let vector = format!("$.\"{id}\"");
    let recorded = b.query(&format!(
        "SELECT json_extract(last_vector, '{vector}') FROM sync_peers"
    ));
    b.ok(&["occur", "walk"]);
    b.ok(&["sync"]);
    eventually("a holds walk", || a.has("walk"));
    // The session that carried walk, as if cut before it was recorded.
    rusqlite::Connection::open(b.db())
        .unwrap()
        .execute(
            "UPDATE sync_peers SET last_vector = json_set(last_vector, ?1, ?2)",
            rusqlite::params![vector, recorded.parse::<i64>().unwrap()],
        )
        .unwrap();
    b.ok(&["occur", "swim"]);
    super::purge::migrate_under(&b);

    let out = b.ok(&["sync", "chain", "reenter"]);
    assert!(out.contains("replayed"), "{out}");
    assert!(!out.contains("wait to land"), "{out}");
    assert_eq!(
        b.query("SELECT COUNT(*) FROM entries WHERE field = 'walk'"),
        "1"
    );
    assert!(b.has("swim"), "{out}");
    eventually("a holds swim", || a.has("swim"));
    assert_eq!(
        a.query("SELECT COUNT(*) FROM entries WHERE field = 'walk'"),
        "1"
    );
    // Walk's create is signed once, as it was.
    let signed =
        "SELECT COUNT(*) FROM ops WHERE json_extract(body, '$.effect.create.field') = 'walk'";
    assert_eq!(b.query(signed), "1");
    b.ok(&["check", "sync"]);
    a.ok(&["check", "sync"]);
}

/// A re-entry with nothing to replay, every unsent change one this datum
/// cannot hold, still writes once its chain is back.
#[test]
fn a_reentry_with_nothing_to_replay_writes_once_its_chain_is_back() {
    let (a, b) = paired();
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["occur", "coffee"]);
    b.ok(&["sync"]);
    b.ok(&["occur", "swim"]);
    rusqlite::Connection::open(b.db())
        .unwrap()
        .execute(
            "UPDATE ops SET body = json_set(body, '$.kind', 'frobnicate')
             WHERE json_extract(body, '$.field') = 'swim'",
            [],
        )
        .unwrap();
    super::purge::migrate_under(&b);
    let out = b.ok(&["sync", "chain", "reenter"]);
    assert!(
        out.contains("frobnicate") && !out.contains("replayed"),
        "{out}"
    );
    b.ok(&["occur", "run"]);
    b.ok(&["sync"]);
    eventually("a holds run", || a.has("run"));
    b.ok(&["check", "sync"]);
}
