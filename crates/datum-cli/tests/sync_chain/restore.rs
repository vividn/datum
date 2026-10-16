//! `db restore --hard --chain` (da-ncmmj): every copy in the chain restored
//! to a backup, by a chain wipe and the backup signed after it.

use super::*;

/// `copy`'s live fields, sorted: a spec is a field's, restored with it.
fn fields(copy: &Copy) -> String {
    copy.query(
        "SELECT group_concat(field, ' ') FROM
           (SELECT field FROM entries WHERE deleted_at IS NULL ORDER BY field)",
    )
}

/// vividn 10-15: poisoned data, and the whole chain put back to a good backup.
/// a restores it; b, online, holds exactly the backup at its next sync; c
/// was offline, and loses the entry it made before the restore and keeps
/// the one it made after. Every copy then verifies with every other.
#[test]
fn a_chain_restore_puts_every_copy_back_to_the_backup() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    a.ok(&["occur", "tea"]);
    for other in [&b, &c] {
        assert!(a.send(other).0);
    }
    let backup = a.key("good.jsonl");
    a.ok(&["db", "backup", &backup]);
    a.ok(&["occur", "poison"]);
    assert!(a.send(&b).0);
    c.ok(&["occur", "before"]);

    // Plain --hard names both ways; off a TTY it asks for --yes; a dry run
    // names each member and changes nothing.
    let refused = a.fails(&["db", "restore", &backup, "--hard", "--yes"], "");
    assert!(
        refused.contains("--chain") && refused.contains("--leave"),
        "{refused}"
    );
    let refused = a.fails(&["db", "restore", &backup, "--hard", "--chain"], "");
    assert!(refused.contains("--yes"), "{refused}");
    let said = a.ok(&["db", "restore", &backup, "--hard", "--chain", "--dry-run"]);
    for who in [&b, &c] {
        assert!(said.contains(&who.id()[..10]), "{said}");
    }
    assert!(
        said.contains("restored to the backup (2 written)"),
        "{said}"
    );
    assert_eq!(fields(&a), "poison spec spec tea");
    // datum.day's question reads the JSON, and its yes is pinned to it: a
    // write since refuses with nothing wiped.
    let restore = ["db", "restore", &backup, "--hard", "--chain"];
    let dry = json(&a.ok(&[&restore[..], &["--dry-run", "--json"]].concat()));
    assert_eq!(dry["would_insert"], 2, "{dry}");
    let pin = dry["fingerprint"].as_str().unwrap();
    a.ok(&["occur", "late"]);
    let stale = a.run(
        &[&restore[..], &["--yes", "--if-unchanged", pin]].concat(),
        "",
    );
    assert_eq!(stale.status.code(), Some(12));
    assert!(a.has("late") && a.has("poison"));

    let done = a.ok(&[&restore[..], &["--yes"]].concat());
    assert!(done.contains("2 written"), "{done}");
    assert_eq!(fields(&a), "spec tea");
    assert!(a.send(&b).0);
    assert_eq!(fields(&b), "spec tea");

    c.ok(&["occur", "after"]);
    for _ in 0..2 {
        for (from, to) in [(&c, &b), (&b, &c), (&c, &a), (&a, &c), (&b, &a)] {
            let (ok, text) = from.send(to);
            assert!(ok, "{text}");
        }
    }
    for copy in [&a, &b, &c] {
        assert_eq!(fields(copy), "after spec spec tea");
    }
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), c.hash());
}

/// In a locked chain the restore is an admin act, as the purge it orders
/// is: refused without the admin key, with nothing changed, and done with
/// it — here from stdin, as datum.day hands it the backup.
#[test]
fn in_a_locked_chain_a_chain_restore_takes_the_admin_key() {
    let (a, b, admin) = locked();
    b.ok(&["occur", "tea"]);
    let backup = b.key("good.jsonl");
    b.ok(&["db", "backup", &backup]);
    b.ok(&["occur", "poison"]);
    assert!(b.send(&a).0);
    let restore = ["db", "restore", &backup, "--hard", "--chain", "--yes"];
    let refused = b.fails(&restore, "");
    assert!(refused.contains("no approval code"), "{refused}");
    assert_eq!(fields(&b), "poison spec spec tea");

    let piped = ["db", "restore", "-", "--hard", "--chain", "--yes"];
    let bytes = std::fs::read_to_string(&backup).unwrap();
    b.ok_in(&[&piped[..], &["--admin-key", &admin]].concat(), &bytes);
    assert_eq!(fields(&b), "spec tea");
    assert!(b.send(&a).0);
    assert_eq!(fields(&a), "spec tea");
    assert_eq!(a.hash(), b.hash());
}
