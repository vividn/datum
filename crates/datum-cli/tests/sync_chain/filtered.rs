//! Filtered readers (da-u81u1): a reader sent only the entries of its fields.

use super::*;

/// A reader invite, filtered or not, refuses a redeemer that holds entries
/// of its own, saying how many, and admits nothing (da-tti3w): the invite is
/// unused after, so the same redeemer holding none is admitted. A member
/// invite admits a redeemer that holds some.
#[test]
fn a_reader_invite_refuses_a_redeemer_holding_entries() {
    let (a, _b) = chained();
    for caps in [&["--reader"][..], &["--fields", "coffee"]] {
        let invite = json(&a.ok(&[&["sync", "new-invite"], caps].concat()));
        let secret = invite["secret"].as_str().unwrap();
        let r = Copy::new();
        let id = r.id();
        let hello = ["sync", "handshake", &id, "--label", "r", "--addr", "{}"];
        let redeem = [&hello[..], &["--invite", secret, "--chain", ""]].concat();
        let refused = a.fails(&[&redeem[..], &["--holds", "42"]].concat(), "");
        assert!(
            refused.contains(
                "r holds 42 entries of its own; a reader writes nothing, so joining as one \
                 would hide them. Join from a fresh database (`--db`), or ask for a member invite"
            ),
            "{refused}"
        );
        let list = a.ok(&["sync", "chain", "list", "--json"]);
        assert!(!list.contains(&id), "refused, admitted nothing: {list}");
        a.ok(&redeem);
        assert_eq!(member_of(&a, &id)["role"], "reader");
    }
    let invite = json(&a.ok(&["sync", "new-invite"]));
    let r = Copy::new();
    let id = r.id();
    let hello = ["sync", "handshake", &id, "--label", "m", "--addr", "{}"];
    let secret = invite["secret"].as_str().unwrap();
    a.ok(&[
        &hello[..],
        &["--invite", secret, "--chain", "", "--holds", "42"],
    ]
    .concat());
    assert_eq!(member_of(&a, &id)["role"], "member");
}

/// A reader filtered to `coffee` gets coffee's entries and spec, from any
/// member that serves it, and nothing of `tea` — not even from a member that
/// holds everything; its verify agrees over the filtered set; and it and
/// the members say «filtered: n of m fields».
#[test]
fn a_filtered_reader_gets_its_fields_and_their_specs_and_nothing_else() {
    let (a, b) = chained();
    for (f, copy) in [("coffee", &a), ("tea", &a)] {
        copy.ok(&["setup", f, "--verb", "occur"]);
        copy.ok(&["occur", f]);
    }
    b.ok(&["occur", "coffee", "cups=2"]);
    assert!(b.send(&a).0 && a.send(&b).0);
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'coffee'"),
        "2"
    );
    // b's first coffee declared a spec of its own: the older one stands.
    let spec = "SELECT COUNT(*) FROM entries WHERE slug = 'spec:coffee' AND superseded_by IS NULL";
    assert_eq!(r.query(spec), "1");
    assert!(!holds_any(&r, "tea"));
    // b holds everything too, and, once a's records say what r is, sends
    // it nothing of tea.
    assert!(a.send(&b).0);
    b.ok(&["occur", "tea", "-t", "-2"]);
    b.ok(&["occur", "coffee"]);
    let served = b.ok(&["sync", "pipe", "ops", "--since", "{}", "--peer", &r.id()]);
    let ops: Vec<serde_json::Value> = served
        .lines()
        .filter(|l| l.starts_with('['))
        .flat_map(|l| json(l).as_array().unwrap().clone())
        .collect();
    assert!(!ops.is_empty(), "{served}");
    assert!(ops.iter().all(|o| o["field"] == "coffee"), "{served}");
    assert!(feed(&b, &r).0);
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'coffee'"),
        "3"
    );
    assert!(!holds_any(&r, "tea"));

    assert!(b.send(&a).0);
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
    assert_eq!(verify_with(&b, &r), verify_with(&r, &b));
    assert_ne!(
        verify_with(&a, &r),
        a.hash(),
        "the filtered hash is its own"
    );

    let status = r.ok(&["sync", "status"]);
    assert!(
        status.contains("a reader, filtered: 1 of 2 fields: coffee"),
        "{status}"
    );
    assert!(a
        .ok(&["sync", "chain", "list"])
        .contains("reader (filtered: 1 of 2 fields)"));
    let filter = &member_of(&a, &r.id())["filter"];
    assert_eq!(filter, &serde_json::json!({"fields": ["coffee"], "of": 2}));
    let this = &json(&r.ok(&["sync", "status", "--json"]))["this"];
    assert_eq!(this["filter"]["fields"], serde_json::json!(["coffee"]));
}

/// Widening sends the reader the whole history of the field it adds;
/// narrowing makes its copy delete it — rows, ops and history.
#[test]
fn widening_sends_a_fields_history_and_narrowing_deletes_it() {
    let (a, _b) = chained();
    for (field, at) in [("tea", "-3"), ("coffee", "-2"), ("tea", "-1")] {
        a.ok(&["occur", field, "-t", at]);
    }
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    assert!(!holds_any(&r, "tea"));

    a.ok(&[
        "sync",
        "chain",
        "role",
        "r",
        "reader",
        "--fields",
        "coffee,tea",
    ]);
    let (ok, text) = feed(&a, &r);
    assert!(ok, "{text}");
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'tea'"),
        "2"
    );
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
    let out = a.fails(
        &[
            "sync",
            "chain",
            "role",
            "r",
            "reader",
            "--fields",
            "tea,coffee",
        ],
        "",
    );
    assert!(
        out.contains("r is already a reader of those fields"),
        "{out}"
    );

    a.ok(&["sync", "chain", "role", "r", "reader", "--fields", "coffee"]);
    assert!(feed(&a, &r).0);
    assert!(!holds_any(&r, "tea"));
    assert!(r.has("coffee"));
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));

    // Unfiltered, it is sent the rest, and with no gap left its marks go
    // — on the apply, not on a read: status and the vector leave them.
    a.ok(&["sync", "chain", "role", "r", "reader"]);
    let marks = |c: &Copy| c.query("SELECT COUNT(*) FROM sync_caught_up");
    assert!(feed(&a, &r).0);
    assert_eq!(a.hash(), r.hash());
    assert_eq!(marks(&r), "0");
    a.exec("INSERT INTO sync_caught_up VALUES ('e', '[\"coffee\"]', 1)");
    a.ok(&["sync", "status"]);
    a.ok(&["sync", "pipe", "vector"]);
    assert_eq!(marks(&a), "1");
    a.ok_in(&["sync", "pipe", "apply"], "");
    assert_eq!(marks(&a), "0");
}

/// An entry whose field changes into the filter arrives whole — its create
/// under the old field too — and one that changes out of it is removed.
#[test]
fn an_entry_that_moves_into_the_filter_arrives_whole_and_one_that_leaves_is_removed() {
    let (a, _b) = chained();
    a.ok(&["occur", "tea", "note=first"]);
    let tea = a.query("SELECT hid FROM entries WHERE field = 'tea'");
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    assert!(!holds_any(&r, "tea"));

    a.ok(&["update", &tea, "field=coffee"]);
    let (ok, text) = feed(&a, &r);
    assert!(ok, "{text}");
    assert_eq!(
        r.query(&format!("SELECT field FROM entries WHERE hid = '{tea}'")),
        "coffee"
    );
    assert_eq!(
        r.query(&format!("SELECT COUNT(*) FROM ops WHERE hid = '{tea}'")),
        "2"
    );
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));

    a.ok(&["update", &tea, "field=tea"]);
    assert!(feed(&a, &r).0);
    assert_eq!(
        r.query(&format!("SELECT COUNT(*) FROM entries WHERE hid = '{tea}'")),
        "0"
    );
    assert_eq!(
        r.query(&format!("SELECT COUNT(*) FROM ops WHERE hid = '{tea}'")),
        "0"
    );
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
}

/// The reader's vector is «caught up to #N»: past the ops of a field it is
/// never sent, so a member's status says synced across the gaps, and
/// pending only for what it would send.
#[test]
fn a_filtered_readers_vector_is_caught_up_across_gaps_and_status_counts_what_it_is_sent() {
    let (a, _b) = chained();
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    for (field, at) in [("coffee", "-3"), ("tea", "-2"), ("tea", "-1")] {
        a.ok(&["occur", field, "-t", at]);
    }
    let record = |r: &Copy| {
        let (ok, text) = feed(&a, r);
        assert!(ok, "{text}");
        let vector = r.ok(&["sync", "pipe", "vector"]);
        let verify = match verify_with(&a, r) == verify_with(r, &a) {
            true => "match",
            false => "mismatch",
        };
        a.ok(&[
            "sync",
            "record-session",
            &r.id(),
            "--vector",
            vector.trim(),
            "--verify",
            verify,
        ]);
    };
    record(&r);
    let a_id = a.id();
    let theirs = json(&r.ok(&["sync", "pipe", "vector"]));
    let mine = json(&a.ok(&["sync", "pipe", "vector"]));
    assert_eq!(theirs[&a_id], mine[&a_id], "caught up past the tea ops");
    let row = status_of(&a, &r);
    assert_eq!(
        (row["state"].as_str(), row["to_send"].as_i64()),
        (Some("synced"), Some(0)),
        "{row}"
    );

    a.ok(&["occur", "tea"]);
    assert_eq!(status_of(&a, &r)["to_send"], 0, "tea is not waiting for it");
    assert_eq!(status_of(&a, &r)["state"], "synced");
    a.ok(&["occur", "coffee"]);
    assert_eq!(status_of(&a, &r)["to_send"], 1);
    assert_eq!(status_of(&a, &r)["state"], "pending");
    record(&r);
    assert_eq!(status_of(&a, &r)["state"], "synced");

    // An entry no op covers yet goes as a baseline, to r only if it is
    // coffee: an import's rows, as its ops were never written.
    let uncover = |field: &str| {
        let hid = a.query(&format!(
            "SELECT MIN(hid) FROM entries WHERE field = '{field}'"
        ));
        a.exec(&format!("DELETE FROM ops WHERE hid = '{hid}'"));
    };
    uncover("tea");
    assert_eq!(status_of(&a, &r)["to_send"], 0);
    uncover("coffee");
    assert_eq!(status_of(&a, &r)["to_send"], 1);
}

/// An op outside the filter that reaches a filtered reader anyway — a
/// member's whole log, unfiltered — is refused, and nothing of it is kept.
#[test]
fn an_op_outside_its_filter_sent_to_a_filtered_reader_is_refused() {
    let (a, _b) = chained();
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    a.ok(&["occur", "tea", "-t", "-1"]);
    a.ok(&["occur", "coffee"]);
    let (ok, text) = a.send(&r);
    assert!(!ok, "{text}");
    assert!(text.contains("outside this reader's filter"), "{text}");
    assert!(r.has("coffee"));
    assert!(!holds_any(&r, "tea"));
}

/// `--kit` stands for the fields a kit holds specs for, written into the
/// chain as that list; a filter is a reader's alone.
#[test]
fn a_kit_names_its_fields_and_only_a_reader_is_filtered() {
    let (a, _b) = chained();
    a.ok(&["setup", "coffee", "--verb", "occur"]);
    a.exec(
        "UPDATE entries SET data = json_set(data, '$.kit_id', 'drinks') WHERE slug = 'spec:coffee'",
    );
    let r = Copy::new();
    let invite = json(&a.ok(&["sync", "new-invite", "--kit", "drinks"]));
    let secret = invite["secret"].as_str().unwrap();
    let hello = ["sync", "handshake", &r.id(), "--label", "r", "--addr", "{}"];
    a.ok(&[&hello[..], &["--invite", secret, "--chain", ""]].concat());
    assert_eq!(
        member_of(&a, &r.id())["filter"]["fields"],
        serde_json::json!(["coffee"])
    );
    let out = a.fails(&["sync", "new-invite", "--kit", "nope"], "");
    assert!(
        out.contains("kit nope has no fields in this database"),
        "{out}"
    );
    let out = a.fails(
        &["sync", "chain", "role", "b", "member", "--fields", "coffee"],
        "",
    );
    assert!(out.contains("only a reader is filtered"), "{out}");
}

/// A filtered reader made a member holds its gaps until they are sent: it
/// says so in its vector, the highest contiguous seq, and a filtered reader
/// it serves is marked caught up that far and no further.
#[test]
fn a_filtered_reader_made_a_member_claims_only_what_it_holds_without_gaps() {
    let (a, _b) = chained();
    for (field, at) in [("coffee", "-3"), ("tea", "-2"), ("coffee", "-1")] {
        a.ok(&["occur", field, "-t", at]);
    }
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    a.ok(&["sync", "chain", "role", "r", "member"]);
    // Only the chain's records reach it: it is a member, the tea not yet.
    let records: String = a
        .ops_for(&r)
        .lines()
        .filter(|l| json(l)["hid"] == "~chain")
        .map(|l| format!("{l}\n"))
        .collect();
    r.ok_in(&["sync", "pipe", "apply"], &records);
    let a_id = a.id();
    let head = |c: &Copy| {
        json(&c.ok(&["sync", "pipe", "vector"]))[&a_id]
            .as_i64()
            .unwrap()
    };
    assert!(
        head(&r) < head(&a),
        "r lacks a's tea: {} of {}",
        head(&r),
        head(&a)
    );

    let r2 = Copy::new();
    admit_filtered(&r, &r2, "coffee");
    assert_eq!(head(&r2), head(&r));
    // Once a holds r's records admitting r2, it sends r2 the rest.
    assert!(r.send(&a).0);
    assert!(feed(&a, &r2).0);
    assert_eq!(head(&r2), head(&a));
}
