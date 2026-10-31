//! No command takes an entry's row away and keeps its ops (da-smztc): a
//! `sync_orphan_ops` comes from a purge before #491 or a `db restore --hard`
//! before da-95y4o, never from a command run today. `sync_ops`'
//! `check_names_the_entries_whose_ops_have_no_row` is the control: the
//! check names a row dropped under its ops.
//!
//! Beside it, what `check sync` says of a row no op covers, and of an edit
//! a purge left with no create (da-n0ruf).

use super::*;

/// `copy`'s live entries, every field's.
fn shown(copy: &Copy) -> usize {
    let live = copy.query("SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL");
    live.parse().unwrap()
}

/// `check sync` on each copy names no entry whose ops have no row.
fn no_orphans(copies: &[&Copy], after: &str) {
    for copy in copies {
        let out = copy.run(&["check", "sync"], "");
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && !said.contains("sync_orphan_ops"),
            "after `{after}`: {said}"
        );
    }
}

/// `argv` on `copy` takes a row away, and so do its ops on each of `peers`,
/// sent them (a refused op fails the apply, and is no matter here): none of
/// them holds an orphan after.
fn takes(copy: &Copy, peers: &[&Copy], argv: &[&str]) {
    let all = [&[copy][..], peers].concat();
    let before: Vec<usize> = all.iter().map(|c| shown(c)).collect();
    copy.ok(argv);
    for peer in peers {
        copy.send(peer);
    }
    for (n, (c, before)) in all.iter().zip(before).enumerate() {
        assert!(
            shown(c) < before,
            "`{}` took nothing on copy {n}",
            argv.join(" ")
        );
    }
    no_orphans(&all, &argv.join(" "));
}

#[test]
fn no_command_takes_a_row_away_and_keeps_its_ops() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    let kit = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kits/sample.toml");
    a.ok(&["kit", "install", "--yes", kit]);
    a.ok(&["occur", "tea"]);
    a.ok(&["setup", "juice", "--verb", "occur"]);
    a.ok(&["alias", "set", "cof2", "occur coffee cups=2"]);
    assert!(a.send(&b).0);

    takes(&a, &[&b], &["delete", "tea:_"]);
    a.ok(&["occur", "cake"]);
    assert!(a.send(&b).0);
    takes(&a, &[&b], &["undo"]);
    takes(&a, &[&b], &["alias", "remove", "cof2"]);
    takes(&a, &[&b], &["setup", "juice", "--delete"]);
    takes(&a, &[&b], &["kit", "uninstall", "sample"]);

    // A bare restore deletes what the backup lacks.
    let backup = a.key("before.jsonl");
    a.ok(&["db", "backup", &backup]);
    a.ok(&["occur", "extra"]);
    assert!(a.send(&b).0);
    takes(&a, &[&b], &["db", "restore", &backup, "--yes"]);
    // A merge's restored row, sharing a live entry's slug, folds to nothing
    // once undone: its row goes on every copy (da-4ldvb).
    let backup = a.key("pie.jsonl");
    a.ok(&["occur", "pie", "-t", "09:00"]);
    a.ok(&["db", "backup", &backup]);
    a.ok(&["delete", "pie:_"]);
    a.ok(&["occur", "pie", "-t", "09:00"]);
    a.ok(&["db", "restore", &backup, "--merge", "--yes"]);
    assert!(a.send(&b).0);
    no_orphans(&[&a, &b], "db restore --merge");
    takes(&a, &[&b], &["undo"]);

    // A reverted member's entries go from every copy.
    b.ok(&["occur", "bread"]);
    assert!(b.send(&a).0 && a.send(&c).0);
    takes(
        &a,
        &[&c],
        &["sync", "chain", "expel", &b.id(), "--unjoin", "--yes"],
    );

    // A filtered reader drops an entry that moved out of its fields.
    let r = Copy::new();
    a.ok(&["occur", "milk"]);
    admit_filtered(&a, &r, "milk");
    assert!(r.has("milk"));
    a.ok(&["rename", "milk", "latte"]);
    let (ok, text) = feed(&a, &r);
    assert!(ok && !r.has("milk"), "{text}");
    no_orphans(&[&r], "rename milk latte, fed to a reader of milk");

    a.ok(&["occur", "soup"]);
    assert!(a.send(&c).0);
    takes(&a, &[&c], &["sync", "chain", "purge", "--data", "--yes"]);

    let backup = c.key("empty.jsonl");
    c.ok(&["db", "backup", &backup]);
    // The first try signs its departure, and the second leaves once a member
    // holds it.
    c.ok(&["sync", "handshake", &a.id(), "--label", "a", "--addr", "{}"]);
    c.ok(&["occur", "late"]);
    let hard = ["db", "restore", &backup, "--hard", "--leave", "--yes"];
    c.fails(&hard, "");
    assert!(c.send(&a).0);
    let vector = a.ok(&["sync", "pipe", "vector"]);
    c.ok(&["sync", "record-session", &a.id(), "--vector", vector.trim()]);
    takes(&c, &[], &hard);
    c.ok(&["occur", "later"]);
    takes(&c, &[], &["db", "purge", "--yes"]);
    a.ok(&["occur", "last"]);
    takes(&a, &[], &["db", "purge", "--leave", "--force", "--yes"]);
}

/// A row no op covers, on a copy in a chain (da-n0ruf) — a bulk import's,
/// or an entry older than its ops: `check sync` names it as not yet sent,
/// which is no error, and the next sync signs it as it stands.
#[test]
fn check_names_the_rows_no_op_covers_until_a_sync_signs_them() {
    let (a, b) = chained();
    // b's notice is a sync finding too: known, so the one left is the row's.
    a.ok(&["sync", "chain", "ack", "b"]);
    a.ok(&["occur", "tea"]);
    // The entry and the spec its log declared: a's last two ops.
    a.exec("DELETE FROM ops WHERE hid IN (SELECT hid FROM entries WHERE slug LIKE '%tea%')");
    let said = a.ok(&["check", "sync"]);
    assert!(
        said.contains("[WARN ] sync_no_op (x2): 2 entries have no sync op"),
        "{said}"
    );
    assert!(
        said.contains("this copy's next sync signs each as it stands"),
        "{said}"
    );
    for named in ["\n  tea:", "\n  spec:tea ("] {
        assert!(said.contains(named), "{named:?} in {said}");
    }
    assert!(said.contains("Summary: 0 errors, 1 warning."), "{said}");
    // For a program, each by its field, as the other findings name theirs.
    let found = json(&a.ok(&["check", "sync", "--json"]));
    let mut fields: Vec<&str> = found["issues"][0]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["field"].as_str().unwrap())
        .collect();
    fields.sort();
    assert_eq!(fields, ["spec", "tea"], "{found}");

    assert!(a.send(&b).0);
    assert!(b.has("tea"));
    let said = a.ok(&["check", "sync"]);
    assert!(said.contains("No consistency issues found"), "{said}");
    assert_eq!(a.hash(), b.hash());
}

/// An entry a purged device made and another device edited (da-n0ruf): the
/// purge sets its create aside and the edit stays applied, for a hid with no
/// row. Edits with no create fold to nothing, so this is no orphan: there is
/// no row their fold says this copy should hold.
#[test]
fn an_edit_a_purge_left_without_its_create_is_no_orphan() {
    let (a, b) = chained();
    a.ok(&["sync", "chain", "ack", "b"]);
    b.ok(&["occur", "coffee", "cups=1"]);
    assert!(b.send(&a).0);
    let coffee = a.query("SELECT hid FROM entries WHERE field = 'coffee'");
    a.ok(&["update", &coffee, "cups=3"]);
    a.ok(&["sync", "chain", "expel", "b", "--unjoin", "--yes"]);
    assert!(!a.has("coffee"));
    let applied = format!("SELECT kind FROM ops WHERE hid = '{coffee}' AND applied = 1");
    assert_eq!(a.query(&applied), "update");

    let said = a.ok(&["check", "sync"]);
    assert!(said.contains("No consistency issues found"), "{said}");
}
