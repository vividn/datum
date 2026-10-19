//! No command takes an entry's row away and keeps its ops (da-smztc): a
//! `sync_orphan_ops` comes from a purge before #491 or a `db restore --hard`
//! before da-95y4o, never from a command run today. `sync_ops`'
//! `check_names_the_entries_whose_ops_have_no_row` is the control: the
//! check names a row dropped under its ops.

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
    takes(&a, &[&c], &["sync", "chain", "revert", &b.id(), "--yes"]);

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
