//! Copies a schema apart: a copy behind a member's syncs and warns (da-3zqyv), one
//! below its sync floor is refused both ways (da-ryhmi).

use super::*;

/// A copy behind a member's schema syncs on and warns to upgrade (da-3zqyv):
/// once on stderr, naming the member and both versions, and for as long as
/// it lasts in `sync status` and `datum check`; the newer copy says nothing.
#[test]
fn a_copy_behind_a_members_schema_syncs_and_warns_to_upgrade() {
    let (a, mut b) = paired();
    b.env.push(("DATUM_SYNC_TEST_SCHEMA", "999"));
    let mine = format!(
        "(schema {})",
        datum_core::migrations::CURRENT_SCHEMA_VERSION
    );
    let b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "walk"]);
    // As a names b when it warns: b's describe, which can grow it, arrives after.
    let label = member_label(&a, &b.id());
    let first = a.run(&["sync"]);
    let (out, err) = (
        String::from_utf8(first.stdout).unwrap(),
        String::from_utf8(first.stderr).unwrap(),
    );
    assert!(out.contains("verify ok"), "{out}{err}");
    assert!(b.has("walk"));
    for said in [&format!("⚠ {label} runs datum 3.0."), "(schema 999)", &mine] {
        assert!(err.contains(said), "{said} in: {err}");
    }
    assert!(err.contains("`git pull && scripts/install.sh`"), "{err}");
    let again = a.run(&["sync"]);
    assert!(again.status.success());
    let again = String::from_utf8(again.stderr).unwrap();
    assert!(!again.contains("schema 999"), "warned once: {again}");
    for (what, args) in [("status", &["sync", "status"][..]), ("check", &["check"])] {
        let shown = String::from_utf8(a.run(args).stdout).unwrap();
        assert!(shown.contains("(schema 999)"), "{what}: {shown}");
    }
    let list = a.ok(&["sync", "chain", "list", "--json"]);
    assert_eq!(member_of_list(&list, &b.id())["schema"], 999);

    // The newer copy dials the older one and warns of nothing.
    drop(b_runs);
    let _a_runs = a.serve(&["sync", "run"]);
    a.ok(&["occur", "swim"]);
    let newer = b.run(&["sync"]);
    let err = String::from_utf8(newer.stderr).unwrap();
    assert!(newer.status.success() && b.has("swim"), "{err}");
    assert!(!err.contains("runs datum"), "{err}");
    assert!(!b.ok(&["sync", "status"]).contains("runs datum"));
}

/// A copy below a member's sync floor is refused, both ways, and told to
/// update datum; neither side writes anything of the session, and what the
/// older copy wrote meanwhile arrives once it runs a schema at the floor
/// (da-ryhmi).
#[test]
fn a_copy_below_a_members_sync_floor_is_refused_both_ways_until_it_updates() {
    let (mut a, mut b) = paired();
    a.env.push(("DATUM_SYNC_TEST_SCHEMA", "999"));
    a.env.push(("DATUM_SYNC_TEST_FLOOR", "999"));
    let runs = datum_core::migrations::CURRENT_SCHEMA_VERSION;
    // Each names the other by the label it says in its handshake.
    let update = format!(": it needs schema 999, this copy runs {runs}");
    let behind = format!(
        " runs schema {runs}, below this copy's sync floor 999: it must update datum to sync"
    );
    let list = ["sync", "chain", "list", "--json"];
    let schemas = || {
        let of = |c: &Copy, id: &str| member_of_list(&c.ok(&list), id)["schema"].clone();
        (of(&a, &b.id()), of(&b, &a.id()))
    };
    let before = schemas();
    let up = |copy: &Copy| {
        let mut run = copy.serve(&["sync", "run"]);
        let err = lines(run.child.stderr.take().unwrap());
        let until = move |needle: &str| loop {
            let line = err.recv_timeout(Duration::from_secs(10)).expect(needle);
            if line.contains(needle) {
                break;
            }
        };
        until("is up");
        (run, until)
    };

    // The older copy dials: the newer one refuses it.
    b.ok(&["occur", "walk"]);
    let (a_runs, a_says) = up(&a);
    let refused = b.fails(&["sync"], "");
    assert!(
        refused.contains("\nupdate datum to sync with "),
        "{refused}"
    );
    assert!(refused.contains(&update), "{refused}");
    a_says(&behind);
    drop(a_runs);
    assert!(b.ok(&["sync", "status"]).contains(&update));

    // The newer copy dials: the older one refuses itself, and says why.
    a.ok(&["occur", "swim"]);
    let (b_runs, b_says) = up(&b);
    let refused = a.fails(&["sync"], "");
    assert!(refused.contains(&behind), "{refused}");
    b_says(&update);
    drop(b_runs);
    assert!(!a.has("walk") && !b.has("swim"));
    assert_eq!(schemas(), before, "no version recorded");

    // Updated to the floor, it syncs as before, and its walk arrives.
    b.env.push(("DATUM_SYNC_TEST_SCHEMA", "999"));
    let _a_runs = up(&a);
    let synced = b.ok(&["sync"]);
    assert!(synced.contains("verify ok"), "{synced}");
    assert!(a.has("walk") && b.has("swim"));
}
