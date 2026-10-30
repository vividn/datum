//! `datum sync diff <WHO>` (da-n0ruf): asking a member what it holds, and writing nothing.

use super::*;

/// `datum sync diff <WHO>` (da-n0ruf): two copies whose verify disagrees
/// find the entry and its column by asking the member, which says what it
/// holds and which ops; a pair that agrees says so and how much it compared;
/// and neither side is written. Only a member is answered.
#[test]
fn diff_names_the_entry_and_the_column_two_copies_hold_differently() {
    let (a, b) = paired();
    a.ok(&["occur", "coffee", "cups=2"]);
    let _a_runs = a.serve(&["sync", "run"]);
    let now = b.ok(&["sync"]);
    assert!(now.contains("verify ok"), "{now}");
    let who = &a.id()[..12];
    let label = member_label(&b, &a.id());
    let same = b.ok(&["sync", "diff", who]);
    assert!(
        same.starts_with(&format!("this copy and {label} hold the same data ("))
            && same.ends_with(" entries compared)\n"),
        "{same}"
    );
    let same: serde_json::Value =
        serde_json::from_str(&b.ok(&["sync", "diff", who, "--json"])).unwrap();
    assert_eq!(same["compared"], 2, "{same}");
    assert_eq!(same["differ"], serde_json::json!([]), "{same}");
    assert_eq!(same["peer"]["endpoint"], a.id().as_str(), "{same}");

    // b's row goes stale, as a fold that went wrong leaves it.
    let hid = b.query("SELECT hid FROM entries WHERE field = 'coffee'");
    let sql = |c: &Copy, sql: &str| {
        let db = rusqlite::Connection::open(c.db()).unwrap();
        db.execute_batch(sql).unwrap();
    };
    sql(
        &b,
        "UPDATE entries SET data = '{\"cups\":5}' WHERE field = 'coffee'",
    );
    let state = || {
        let ops = "SELECT COUNT(*) FROM ops";
        (a.hash(), b.hash(), a.query(ops), b.query(ops))
    };
    let before = state();
    assert_ne!(before.0, before.1);
    let said = b.ok(&["sync", "diff", who]);
    assert!(
        said.starts_with(&format!(
            "this copy and {label} hold 1 entry differently, of 2 entries compared:\n\ncoffee:"
        )),
        "{said}"
    );
    assert!(said.contains(&format!(" ({})\n", &hid[..5])), "{said}");
    assert!(
        said.contains(&format!("\n  data.cups: here 5, {label} 2\n")),
        "{said}"
    );
    assert!(said.contains("\n  both hold the same 1 op, "), "{said}");
    let json: serde_json::Value =
        serde_json::from_str(&b.ok(&["sync", "diff", who, "--json"])).unwrap();
    let differs = &json["differ"][0];
    assert_eq!(json["differ"].as_array().unwrap().len(), 1, "{json}");
    assert_eq!(differs["hid"], hid.as_str(), "{json}");
    assert_eq!(
        differs["columns"]["data.cups"],
        serde_json::json!({"here": 5, "there": 2}),
        "{json}"
    );
    assert_eq!(differs["there"]["ops"][0]["kind"], "create", "{json}");
    assert_eq!(state(), before, "a diff writes nothing on either side");

    // The row gone and its ops kept (da-95y4o), seen from beside it.
    sql(&b, "DELETE FROM entries WHERE field = 'coffee'");
    let said = b.ok(&["sync", "diff", who]);
    assert!(
        said.contains(&format!("\n  no row here; {label} has one\n")),
        "{said}"
    );
    assert!(said.contains("\n  both hold the same 1 op, "), "{said}");

    // An entry only one side has, with the op the other lacks.
    a.ok(&["occur", "tea"]);
    let said = b.ok(&["sync", "diff", who]);
    assert!(said.contains("\ntea:"), "{said}");
    assert!(said.contains("\n  ops only here: none\n"), "{said}");
    assert!(
        said.contains(&format!("\n  ops only on {label}: create by ")),
        "{said}"
    );

    // A copy that is no member is told nothing.
    a.ok(&["sync", "chain", "expel", &b.id()[..12]]);
    let refused = b.fails(&["sync", "diff", who], "");
    assert!(refused.contains("refused: "), "{refused}");
}

/// A filtered reader's diff is over what it shares (da-n0ruf, da-u81u1):
/// an entry outside its filter is no difference, and asked for by hid the
/// member tells it neither the row nor its ops.
#[test]
fn a_filtered_readers_diff_is_over_its_fields_alone() {
    let (a, r) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee"]);
    a.ok(&["occur", "tea"]);
    let (mut inviting, ticket) = a.invite(&["--fields", "coffee"]);
    r.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    assert!(!r.has("tea"));

    let _a_runs = a.serve(&["sync", "run"]);
    let same = r.ok(&["sync", "diff", &a.id()[..12]]);
    // The coffee and the spec its log declared.
    assert!(
        same.contains("hold the same data (2 entries compared)"),
        "{same}"
    );

    let tea = a.query("SELECT hid FROM entries WHERE field = 'tea'");
    let held = |peer: &[&str]| -> serde_json::Value {
        let args = [&["sync", "pipe", "held"], peer].concat();
        let out = a.run_in(&args, &format!("[\"{tea}\"]"));
        serde_json::from_slice(&out.stdout).unwrap()
    };
    let told = held(&["--peer", &r.id()]);
    assert_eq!(
        told,
        serde_json::json!([{"hid": tea, "row": null, "ops": []}])
    );
    // A member with no filter is told both.
    let told = held(&[]);
    assert_eq!(told[0]["row"]["field"], "tea", "{told}");
    assert_eq!(told[0]["ops"][0]["kind"], "create", "{told}");

    // The member asking the reader compares the same two entries: the tea
    // is no difference, so its hid is never sent to the reader either.
    drop(_a_runs);
    let _r_runs = r.serve(&["sync", "run"]);
    let same = a.ok(&["sync", "diff", &r.id()[..12]]);
    assert!(
        same.contains("hold the same data (2 entries compared)"),
        "{same}"
    );
}

/// A diff admits no one (da-n0ruf, da-2u95o): a member the answering copy
/// has not heard of is refused, where its sync would show the chain's
/// records and be admitted, and the answering copy still has not heard of
/// it afterwards. Once it has synced, its diff is answered.
#[test]
fn a_diff_from_a_member_not_heard_of_is_refused_and_admits_no_one() {
    let (_a, b, c) = admitted_unheard(&[]);
    let heard = || b.ok(&["sync", "chain", "list"]).contains(&c.id()[..10]);
    let _b_runs = b.serve(&["sync", "run"]);
    let who = &b.id()[..12];
    let out = c.fails(&["sync", "diff", who], "");
    assert!(
        out.contains("refused: not a member of this sync chain"),
        "{out}"
    );
    assert!(!heard(), "a diff admitted its dialer");

    // The inviter is gone, offline; the session with b verifies.
    let synced = c.ok(&["sync"]);
    assert!(synced.contains("verify ok"), "{synced}");
    assert!(heard());
    let same = c.ok(&["sync", "diff", who]);
    assert!(same.contains("hold the same data ("), "{same}");
}
