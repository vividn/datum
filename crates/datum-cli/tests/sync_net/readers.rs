//! Readers over the link: one ahead of a member, one refused for holding entries, and
//! a filtered one.

use super::*;

/// A reader ahead of the member it syncs with (da-t3ey2): r holds an entry
/// of a's that b, which knows r, lacks, and a reader never relays, so no
/// session between them can make them equal. Each says who is ahead and by
/// how much, and neither a MISMATCH, in its session or its status; the same
/// ops and different rows are still one.
#[test]
fn a_reader_ahead_of_a_member_says_so_and_the_same_ops_apart_are_a_mismatch() {
    let (a, mut b, r) = admitted_unheard(&["--reader"]);
    // A dial that reaches no one fails the command: what it printed.
    let sync = |c: &Copy| String::from_utf8(c.run(&["sync"]).stdout).unwrap();

    // b hears of r from a; r alone gets a's next entry; a goes.
    let a_runs = a.serve(&["sync", "run"]);
    let heard = sync(&b);
    assert!(heard.contains("verify ok"), "{heard}");
    a.ok(&["occur", "later"]);
    let got = sync(&r);
    assert!(got.contains("verify ok") && r.has("later"), "{got}");
    drop(a_runs);

    // b's run dials r in its turn, and reaches no one: its status would say
    // that, so not while this looks.
    b.env = vec![("DATUM_SYNC_TIMEOUT_S", "30")];
    let mut b_runs = b.serve(&["sync", "run"]);
    let dialed = sync(&r);
    let served = b_runs.lines_until(" got ");
    drop(b_runs);
    // As each names the other now: r's describe reached b in that session.
    let (b_label, r_label) = (member_label(&r, &b.id()), member_label(&b, &r.id()));
    b.env = r.env.clone();
    // The entry, and the spec its first log declared.
    let ahead = format!("not verified: this copy is 2 changes ahead of {b_label}");
    assert!(dialed.contains(&ahead), "{dialed}");
    let ahead = format!("not verified: {r_label} is 2 changes ahead of this copy");
    assert!(served.contains(&ahead), "{served}");
    for (x, y) in [(&b, &r), (&r, &b)] {
        let row = entry_of(x, "status", y);
        assert_eq!(
            (row["state"].as_str(), row["error"].as_str()),
            (Some("pending"), None),
            "{row}"
        );
    }

    // b catches up from a, so the two hold the same ops and verify; then r
    // loses a row.
    let a_runs = a.serve(&["sync", "run"]);
    assert!(sync(&b).contains("verify ok") && b.has("later"));
    drop(a_runs);
    let _b_runs = b.serve(&["sync", "run"]);
    let level = sync(&r);
    assert!(level.contains("verify ok"), "{level}");
    assert_eq!(entry_of(&r, "status", &b)["state"], "synced");
    rusqlite::Connection::open(r.db())
        .unwrap()
        .execute("DELETE FROM entries WHERE field = 'later'", [])
        .unwrap();
    let lost = sync(&r);
    assert!(lost.contains("verify MISMATCH"), "{lost}");
    assert_eq!(entry_of(&r, "status", &b)["error"], "verify MISMATCH");
}

/// A reader writes nothing, so a copy holding entries of its own is refused
/// a reader invite, filtered or not, by count (da-tti3w): it is in no chain
/// after and its entries are as they were, and the invite, unused, admits a
/// fresh copy. A copy holding entries takes a member invite as before
/// (`an_invite_pairs_two_copies_and_their_data_converges`).
#[test]
fn a_copy_holding_entries_is_refused_a_reader_invite() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    for caps in [&["--reader"][..], &["--fields", "coffee"]] {
        let r = Copy::new();
        r.ok(&["occur", "tea"]);
        r.ok(&["occur", "tea", "-t", "-5"]);
        let (mut inviting, ticket) = a.invite(caps);
        let refused = r.fails(&["sync", "chain", "join", &ticket], "");
        assert!(
            refused.contains("holds 2 entries of its own; a reader writes nothing"),
            "{refused}"
        );
        // A refusal of no kind of its own (da-b6xh6).
        assert!(
            refused.contains("Next: `datum sync status` shows where this copy stands"),
            "{refused}"
        );
        assert_eq!(
            r.query("SELECT COUNT(*) FROM ops WHERE hid = '~chain'"),
            "0"
        );
        assert_eq!(r.ok(&["count"]).trim(), "2");
        assert!(!r.has("coffee"));

        let fresh = Copy::new();
        let joined = fresh.ok(&["sync", "chain", "join", &ticket]);
        assert!(joined.contains("verify ok"), "{joined}");
        assert!(fresh.has("coffee"));
        assert!(inviting.finish().0);
    }
}

/// What a copy holds is more than `count` counts: a flag marker and a meta
/// note are entries a reader could not send either, and `count`, as `tail`,
/// leaves them out (da-lkh1j).
#[test]
fn a_copy_holding_a_flag_and_a_meta_note_is_refused_a_reader_invite() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    let r = Copy::new();
    r.ok(&["flag", "tea", "come back to this"]);
    r.ok(&["meta", "tea", "rate it"]);
    assert_eq!(r.ok(&["count"]).trim(), "0");
    assert_eq!(r.ok(&["sync", "holds"]).trim(), "2");
    let (_inviting, ticket) = a.invite(&["--reader"]);
    let refused = r.fails(&["sync", "chain", "join", &ticket], "");
    assert!(
        refused.contains("holds 2 entries of its own; a reader writes nothing"),
        "{refused}"
    );
}

/// A filtered reader over iroh (da-u81u1): the member serving it knows who
/// it is and sends its fields alone — more than one apply chunk of them —
/// the verify over that set agrees, and a widened filter brings the rest
/// on the next session, in step again.
#[test]
fn a_filtered_reader_over_the_link_gets_its_fields_and_verifies() {
    let (a, r) = (Copy::new(), Copy::new());
    for m in 1..=70 {
        a.ok(&["occur", "coffee", "-t", &format!("-{m}")]);
    }
    a.ok(&["occur", "tea", "-t", "-100"]);
    let (mut inviting, ticket) = a.invite(&["--fields", "coffee"]);
    let joined = r.ok(&["sync", "chain", "join", &ticket]);
    assert!(joined.contains("verify ok"), "{joined}");
    assert!(inviting.finish().0);
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'coffee'"),
        "70"
    );
    assert!(!r.has("tea"));

    let who = r.id()[..10].to_string();
    a.ok(&[
        "sync",
        "chain",
        "role",
        &who,
        "reader",
        "--fields",
        "coffee,tea",
    ]);
    let _a_runs = a.serve(&["sync", "run"]);
    let now = r.ok(&["sync"]);
    assert!(now.contains("verify ok"), "{now}");
    assert!(r.has("tea"));
}
