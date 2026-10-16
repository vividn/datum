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
    // An entry made before it is kept on its copy, and the way back named.
    assert!(
        said.contains("kept on that copy in <db>.wiped-<time>.jsonl, which `datum db restore <it> --merge` brings back"),
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

/// datum.day's way in a passkey-locked chain (da-sd7xt): the backup is on
/// stdin, where the CLI would ask for the approval code. The dry run names
/// the order to approve, and the run takes it back signed — refused when it
/// is not this chain's restore order or not signed by the admin, with
/// nothing changed.
#[test]
fn a_passkey_approves_a_chain_restore_named_by_its_dry_run() {
    let (a, b) = chained();
    a.ok(&["sync", "admin", "add", &passkey_json()]);
    assert!(a.send(&b).0);
    b.ok(&["occur", "tea"]);
    let backup = b.key("good.jsonl");
    b.ok(&["db", "backup", &backup]);
    b.ok(&["occur", "poison"]);
    let bytes = std::fs::read_to_string(&backup).unwrap();
    let piped = ["db", "restore", "-", "--hard", "--chain"];
    let dry = json(&b.ok_in(&[&piped[..], &["--dry-run", "--json"]].concat(), &bytes));
    let link = dry["approve"].as_str().unwrap().to_string();
    let record = BASE64URL_NOPAD
        .decode(link.rsplit('#').next().unwrap().as_bytes())
        .unwrap();
    let code = approval_code(&Sha256::digest(&record));
    let run = |link: &str, code: &str| {
        let approval = ["--yes", "--approve-link", link, "--approval-code", code];
        b.run(&[&piped[..], &approval].concat(), &bytes)
    };
    // A purge's order, signed, restores nothing: it is not a restore's.
    let mut purge: serde_json::Value = serde_json::from_slice(&record).unwrap();
    purge.as_object_mut().unwrap().remove("restore");
    let purge = format!(
        "https://datum.day/sync/approve#{}",
        BASE64URL_NOPAD.encode(purge.to_string().as_bytes())
    );
    for (link, code, why) in [
        (
            purge.as_str(),
            code.as_str(),
            "not an order restoring sync chain",
        ),
        (
            &link,
            &approval_code(&Sha256::digest(b"another")),
            "the sync chain refuses it",
        ),
    ] {
        let out = run(link, code);
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && err.contains(why), "{err}");
        assert_eq!(fields(&b), "poison spec spec tea");
    }
    let out = run(&link, &code);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fields(&b), "spec tea");
    assert!(b.send(&a).0);
    assert_eq!(fields(&a), "spec tea");
    assert_eq!(a.hash(), b.hash());
}

/// da-uk23v, vividn: «you should never be afraid of data loss regardless
/// which client you're entering into». c, offline, logs before a chain
/// restore: as it syncs the entry goes from its database, as the restore
/// says, and into a backup beside it, which the next command, `check` and
/// `sync status` name until acknowledged; `db restore --merge` of it brings
/// the entry back, to every copy. The ordering copy keeps only its own.
#[test]
fn a_chain_restore_leaves_an_offline_copy_a_backup_of_what_it_drops() {
    let (a, c) = chained();
    a.ok(&["occur", "tea"]);
    assert!(a.send(&c).0);
    let backup = a.key("good.jsonl");
    a.ok(&["db", "backup", &backup]);
    c.ok(&["occur", "before"]);
    a.ok(&["db", "restore", &backup, "--hard", "--chain", "--yes"]);
    assert!(kept(&a).is_empty(), "{:?}", kept(&a));

    assert!(a.send(&c).0);
    assert!(!c.has("before") && c.has("tea"));
    let [file] = kept(&c).try_into().unwrap();
    let file = file.display().to_string();
    let lines = std::fs::read_to_string(&file).unwrap();
    assert!(
        lines.lines().next().unwrap().contains("daydumb_backup"),
        "{lines}"
    );
    assert!(lines.contains(r#""field":"before""#), "{lines}");

    let next = c.run(&["tail"], "");
    let said = String::from_utf8_lossy(&next.stderr);
    let merge = format!("`datum db restore {file} --merge` brings it back");
    assert!(said.contains(&merge), "{said}");
    assert!(c.ok(&["sync", "status"]).contains(&merge));
    let check = c.run(&["check"], "");
    assert!(String::from_utf8_lossy(&check.stdout).contains(&merge));

    c.ok(&["db", "restore", &file, "--merge"]);
    assert!(c.has("before"));
    assert!(c.send(&a).0);
    assert!(a.has("before") && a.has("tea"));
    assert_eq!(a.hash(), c.hash());
    c.ok(&["sync", "chain", "ack", "--all"]);
    assert!(!c.ok(&["sync", "status"]).contains(&merge));

    // A second restore, as soon as the first: each wipe its own file.
    a.ok(&["db", "restore", &backup, "--hard", "--chain", "--yes"]);
    assert!(a.send(&c).0);
    assert!(!c.has("before"));
    assert_eq!(kept(&c).len(), 2, "{:?}", kept(&c));
}
