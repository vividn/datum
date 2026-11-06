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
    // The version is the build's, whose minor a sync floor moves (da-ec8y8).
    let runs = format!(
        "⚠ {label} runs datum {}.{}.",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR")
    );
    for said in [&runs, "(schema 999)", &mine] {
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

/// The knobs that make a copy say it runs schema 999 with its floor there:
/// every copy that does not say so is below its floor.
pub(super) const OVER: [(&str, &str); 2] = [
    ("DATUM_SYNC_TEST_SCHEMA", "999"),
    ("DATUM_SYNC_TEST_FLOOR", "999"),
];

/// How many of `of`'s ops `copy` holds, chain records (`~chain`) or the rest.
fn ops_of(copy: &Copy, of: &Copy, records: bool) -> usize {
    let hid = if records { "=" } else { "!=" };
    let sql = format!(
        "SELECT COUNT(*) FROM ops WHERE endpoint = '{}' AND hid {hid} '~chain'",
        of.id()
    );
    copy.query(&sql).parse().unwrap()
}

/// `copy`'s `sync run`, up, and a wait for a line of its stderr.
pub(super) fn up(copy: &Copy) -> (Serving, impl Fn(&str)) {
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
}

/// `copy`'s `sync chain list --json`.
fn listed(copy: &Copy) -> serde_json::Value {
    serde_json::from_str(&copy.ok(&["sync", "chain", "list", "--json"])).unwrap()
}

/// The cell `copy`'s grid draws for endpoint `id`, in its one column.
fn grid_cell(copy: &Copy, id: &str) -> serde_json::Value {
    let grid: serde_json::Value =
        serde_json::from_str(&copy.ok(&["sync", "grid", "--json"])).unwrap();
    let chain = grid["chains"][0]["chain"].as_str().unwrap().to_string();
    let rows = grid["devices"].as_array().unwrap();
    let cell = rows
        .iter()
        .map(|r| r["cells"][&chain].clone())
        .find(|c| c["endpoint"] == id);
    cell.unwrap_or_else(|| panic!("{id} in no cell of {grid}"))
}

/// Two members across a sync floor (da-ryhmi): no entry moves either way, and
/// the one below is told to update, from either side; what the older copy
/// wrote meanwhile arrives once it runs a schema at the floor. Membership
/// crosses (da-52vdj): each session swaps the chain's records and records
/// what the other runs, and is no sync, so `datum sync` fails.
#[test]
fn a_copy_below_a_members_sync_floor_syncs_no_entry_both_ways_until_it_updates() {
    let (mut a, mut b) = paired();
    a.env.extend(OVER);
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
    let entry_ops = || (ops_of(&a, &b, false), ops_of(&b, &a, false));
    let held = entry_ops();

    // The older copy dials: the newer one tells it to update.
    b.ok(&["occur", "walk"]);
    let (a_runs, a_says) = up(&a);
    let refused = b.fails(&["sync"], "");
    assert!(refused.contains("update datum to sync with "), "{refused}");
    assert!(refused.contains(&update), "{refused}");
    assert!(
        refused.contains("Only the chain's membership crossed"),
        "{refused}"
    );
    a_says(&behind);
    drop(a_runs);
    assert!(b.ok(&["sync", "status"]).contains(&update));

    // The newer copy dials: it says the older must update.
    a.ok(&["occur", "swim"]);
    let (b_runs, b_says) = up(&b);
    let refused = a.fails(&["sync"], "");
    assert!(refused.contains(&behind), "{refused}");
    b_says(&update);
    drop(b_runs);
    assert!(!a.has("walk") && !b.has("swim"));
    assert_eq!(entry_ops(), held, "no entry op crossed");
    assert_eq!(
        schemas(),
        (runs.into(), 999.into()),
        "each recorded what the other runs"
    );
    // Each grid marks the other, saying which must update.
    assert_eq!(grid_cell(&a, &b.id())["apart"]["updates"], "it");
    assert_eq!(grid_cell(&b, &a.id())["apart"]["updates"], "this");
    assert_eq!(grid_cell(&b, &a.id())["apart"]["floor"], 999);

    // Updated to the floor, it syncs as before, and its walk arrives.
    b.env.extend(OVER);
    let _a_runs = up(&a);
    let synced = b.ok(&["sync"]);
    assert!(synced.contains("verify ok"), "{synced}");
    assert!(a.has("walk") && b.has("swim"));
    assert!(grid_cell(&b, &a.id())["apart"].is_null());
}

/// A join across a sync floor joins (da-52vdj, vividn: «Membership can cross
/// the floor even if ops cannot»): the joiner is a member at once, with both
/// copies in its chain, its grid marks the inviter, and no entry op crosses
/// either way. Then a third member, on the joiner's side of the floor, hears
/// of it in its first session with it, by the chain's records it shows.
#[test]
fn a_join_across_a_sync_floor_joins_and_moves_no_entry() {
    let (mut a, c) = paired();
    a.env.extend(OVER);
    let mut b = Copy::new();
    a.ok(&["occur", "walk"]);
    b.ok(&["occur", "swim"]);
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0, "the invite was redeemed");
    let runs = datum_core::migrations::CURRENT_SCHEMA_VERSION;
    assert!(
        joined.contains(&format!(": it needs schema 999, this copy runs {runs}")),
        "{joined}"
    );

    // A member, with every copy in its chain: the inviter's records wait on
    // its entry ops, and none of them came.
    assert_eq!(listed(&b)["chain"], listed(&a)["chain"]);
    let shown = listed(&b);
    for id in [a.id(), b.id(), c.id()] {
        let m = shown["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["endpoint"] == id);
        assert!(m, "{id} in {shown}: {}", b.query("SELECT group_concat(endpoint || '/' || seq || '/' || applied || '/' || substr(body, 1, 400), ' ;; ') FROM ops WHERE hid = '~chain'"));
    }
    let status: serde_json::Value =
        serde_json::from_str(&b.ok(&["sync", "status", "--json"])).unwrap();
    assert!(status["chain"].is_string(), "{status}");
    assert_eq!(grid_cell(&b, &b.id())["state"], "member");
    assert_eq!(grid_cell(&b, &a.id())["apart"]["updates"], "this");
    assert_eq!(grid_cell(&a, &b.id())["apart"]["updates"], "it");
    assert!(!a.has("swim") && !b.has("walk"));
    assert_eq!(ops_of(&b, &a, false), 0, "no entry op of the inviter's");
    assert!(ops_of(&b, &a, true) > 0, "the inviter's records");
    assert_eq!(ops_of(&a, &b, false), 0, "no entry op of the joiner's");

    // The third member has not heard of it, and does in its first session
    // with it, before any entry op: the joiner shows the records it holds
    // waiting, which the third keeps. The inviter is offline meanwhile.
    let unheard = format!(
        "SELECT COUNT(*) FROM sync_peers WHERE endpoint_id = '{}'",
        b.id()
    );
    assert_eq!(c.query(&unheard), "0");
    assert!(!c.ok(&["sync", "grid", "--json"]).contains(&b.id()));
    let _c_runs = up(&c);
    b.env.push(("DATUM_SYNC_DIAL_TIMEOUT_S", "2"));
    let synced = b.run(&["sync"]);
    let said = String::from_utf8_lossy(&synced.stdout);
    let grid = c.ok(&["sync", "grid", "--json"]);
    assert!(
        grid.contains(&b.id()),
        "{said}{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert_eq!(grid_cell(&c, &b.id())["state"], "member", "{said}");
    let status = c.ok(&["sync", "status", "--json"]);
    assert!(status.contains(&b.id()), "{status}");
}

/// A join to a copy from before floors (da-52vdj; GitHub's `latest` release,
/// at schema 68, is one): it admits the joiner and welcomes it into a sync,
/// reading no floor and taking no session of records alone. The joiner sends
/// it nothing, so it writes nothing it cannot read, and records where it is
/// joining through; once the inviter is updated, a bare `sync` finishes the
/// join there.
#[test]
fn a_join_to_a_copy_from_before_floors_sends_it_nothing_and_finishes_once_it_updates() {
    let (mut a, b) = (Copy::new(), Copy::new());
    a.env.extend(OVER);
    a.env.push(("DATUM_SYNC_TEST_BEFORE_FLOORS", "1"));
    a.ok(&["occur", "walk"]);
    b.ok(&["occur", "swim"]);
    let (inviting, ticket) = a.invite(&[]);
    let joined = b.ok(&["sync", "chain", "join", &ticket]);
    drop(inviting);
    assert!(joined.contains(": it needs schema 999"), "{joined}");

    // The inviter admitted it; the joiner holds where it joins through, and
    // nothing of the inviter's, nor the inviter anything of the joiner's.
    member_of_list(&a.ok(&["sync", "chain", "list", "--json"]), &b.id());
    let through = format!(
        "SELECT COUNT(*) FROM sync_peers WHERE endpoint_id = '{}' AND redeemed_at IS NOT NULL",
        a.id()
    );
    assert_eq!(b.query(&through), "1");
    assert_eq!(listed(&b)["joining"][0]["endpoint"], a.id());
    assert_eq!((ops_of(&b, &a, false), ops_of(&b, &a, true)), (0, 0));
    assert_eq!(ops_of(&a, &b, false), 0);
    assert!(!a.has("swim") && !b.has("walk"));

    // Updated, the inviter syncs with it, and the join is done.
    a.env.truncate(1);
    let _a_runs = up(&a);
    let synced = b.ok(&["sync"]);
    assert!(synced.contains("verify ok"), "{synced}");
    assert!(a.has("swim") && b.has("walk"));
    member_of_list(&b.ok(&["sync", "chain", "list", "--json"]), &b.id());
}
