//! A copy that took another's history by `db ops export | import` joins that
//! copy's chain (da-tllzg): the chain's records, which the import held
//! dormant, apply once its first sync brings the chain's newer ones, and the
//! two hold the same records, entries and state.

use super::*;

#[test]
fn a_copy_that_imported_a_chained_copys_ops_joins_its_chain() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["moment", "coffee", "-y", "-t", "9:00"]);
    a.ok(&["switch", "mood", "happy", "-y", "-t", "10:00"]);
    let (mut inviting, ticket) = a.invite(&[]);
    a.ok(&["moment", "tea", "-y", "-t", "11:00"]);
    let ops = a.ok(&["db", "ops", "export"]);
    assert!(ops.contains("\"~chain\""), "the chain's records go too");
    let imported = b.run_in(&["db", "ops", "import"], &ops);
    assert!(imported.status.success(), "{imported:?}");

    let joined = b.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    assert!(joined.contains("verify ok"), "{joined}");
    // Each of a's records, applied; b's own describe is a's at its next sync.
    let records = |of: &Copy| {
        of.query(&format!(
            "SELECT group_concat(endpoint || '/' || seq, ' ') FROM (SELECT * FROM ops
               WHERE hid = '~chain' AND applied = 1 AND endpoint != '{}' ORDER BY endpoint, seq)",
            b.id()
        ))
    };
    assert_eq!(records(&b), records(&a), "the chain's records");
    b.ok(&["sync", "chain", "list"]);
    assert_eq!(b.ok(&["export", "--jsonl"]), a.ok(&["export", "--jsonl"]));
    assert_eq!(a.hash(), b.hash());
}

/// A copy in no chain that syncs from one holding dormant records (by the
/// pipe, two copies in no chain) holds them dormant too: it stays in no
/// chain, with the entries, and can join that chain after.
#[test]
fn a_copy_synced_from_one_holding_dormant_records_stays_in_no_chain_and_can_join() {
    let (a, b, c) = (Copy::new(), Copy::new(), Copy::new());
    a.ok(&["moment", "coffee", "-y", "-t", "9:00"]);
    let (mut inviting, ticket) = a.invite(&[]);
    a.ok(&["moment", "tea", "-y", "-t", "11:00"]);
    let ops = a.ok(&["db", "ops", "export"]);
    let imported = b.run_in(&["db", "ops", "import"], &ops);
    assert!(imported.status.success(), "{imported:?}");

    let vector = c.ok(&["sync", "pipe", "vector"]);
    let sent = b.ok(&["sync", "pipe", "ops", "--since", vector.trim()]);
    let applied = c.run_in(&["sync", "pipe", "apply"], &sent);
    assert!(applied.status.success(), "{applied:?}");
    assert!(
        String::from_utf8_lossy(&applied.stdout).contains("held 0"),
        "{applied:?}"
    );
    c.fails(&["sync", "chain", "list"], "");
    assert_eq!(c.ok(&["export", "--jsonl"]), a.ok(&["export", "--jsonl"]));

    let joined = c.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    assert!(joined.contains("verify ok"), "{joined}");
    let records = |of: &Copy| {
        of.query(&format!(
            "SELECT group_concat(endpoint || '/' || seq, ' ') FROM (SELECT * FROM ops
               WHERE hid = '~chain' AND applied = 1 AND endpoint != '{}' ORDER BY endpoint, seq)",
            c.id()
        ))
    };
    assert_eq!(records(&c), records(&a), "the chain's records");
    assert_eq!(c.ok(&["export", "--jsonl"]), a.ok(&["export", "--jsonl"]));
}
