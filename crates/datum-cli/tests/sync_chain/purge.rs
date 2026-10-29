//! `sync chain purge --data | --all` (da-qzrj9): the chain-wide hard purge,
//! carried out by each copy as it comes to hold the order.

use super::*;

/// What `copy` holds of data: entries, entry ops with a body, history rows.
fn data_of(copy: &Copy) -> [String; 3] {
    [
        "SELECT COUNT(*) FROM entries",
        "SELECT COUNT(*) FROM ops WHERE hid != '~chain' AND body != ''",
        "SELECT COUNT(*) FROM entry_history",
    ]
    .map(|sql| copy.query(sql))
}

const EMPTY: [&str; 3] = ["0", "0", "0"];

/// Each member's endpoint and whether it is revoked, as `copy` holds them.
fn members_of(copy: &Copy) -> Vec<(String, bool)> {
    let list = json(&copy.ok(&["sync", "chain", "list", "--json"]));
    let members = list["members"].as_array().unwrap().iter();
    members
        .map(|m| {
            (
                m["endpoint"].as_str().unwrap().into(),
                m["revoked"].is_string(),
            )
        })
        .collect()
}

/// `sync chain purge --data` (da-qzrj9, vividn 10-12): a signed order each
/// copy carries out as it comes to hold it. a orders it and b, online,
/// empties at its next sync. c was offline: it missed a's last entry and
/// logged one of its own, and empties when it next connects — to b, which
/// carried the order out, so the seqs b took of a's ops reach c as stubs it
/// would otherwise wait on forever. The confirmation said who learns when;
/// d, revoked, is never told. The members stay, a's backup restores what
/// went, and what is written afterwards syncs as ever.
#[test]
fn a_data_purge_empties_every_copy_as_it_connects_and_the_chain_syncs_on() {
    let (a, b) = chained();
    let (c, d) = (Copy::new(), Copy::new());
    admit(&a, &c, "c", None);
    admit(&a, &d, "d", None);
    a.ok(&["occur", "tea"]);
    for other in [&b, &c, &d] {
        assert!(a.send(other).0);
    }
    a.ok(&["sync", "chain", "expel", &d.id()[..10]]);
    a.ok(&["sync", "record-session", &b.id(), "--verify", "match"]);
    // As if b had admitted c: a never met it.
    a.exec(&format!(
        "DELETE FROM sync_peers WHERE endpoint_id = '{}'",
        c.id()
    ));
    a.ok(&["occur", "coffee"]);
    assert!(a.send(&b).0);
    c.ok(&["occur", "juice"]);
    let members = members_of(&a);

    // Without --yes: who learns of it when, on the reader's clock, and
    // nothing changes.
    a.exec(&format!(
        "UPDATE sync_peers SET last_seen = '2020-01-02T03:04:05Z' WHERE endpoint_id = '{}'",
        b.id()
    ));
    let out = a
        .cmd(&["sync", "chain", "purge", "--data"])
        .env("TZ", "Asia/Kathmandu")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8(out.stdout).unwrap();
    for line in [
        format!(
            "b ({}) last synced with this copy at 2020-01-02 08:49: it purges",
            &b.id()[..10]
        ),
        format!(
            "c ({}) has not synced with this copy: it purges when it next connects",
            &c.id()[..10]
        ),
        format!(
            "d ({}) is expelled: it is never told, and keeps what it holds",
            &d.id()[..10]
        ),
        "nothing changed: `datum sync chain purge --data --yes` does it".into(),
    ] {
        assert!(said.contains(&line), "{line}\n{said}");
    }
    assert!(a.has("tea") && a.has("coffee"));

    let done = a.ok(&["sync", "chain", "purge", "--data", "--yes"]);
    assert_eq!(data_of(&a), EMPTY, "{done}");
    // A whole earlier copy beside b's file goes with what it holds.
    let snapshot = b
        .tmp
        .path()
        .join("data.db.pre-v1-to-v2-20260101T000000Z.bak");
    std::fs::write(&snapshot, "tea").unwrap();
    assert!(a.send(&b).0);
    assert_eq!(data_of(&b), EMPTY);
    assert!(!snapshot.exists());
    // c's entry, made before the order, is taken as it reaches b.
    assert!(c.send(&b).0);
    assert_eq!(data_of(&b), EMPTY);
    let (ok, text) = b.send(&c);
    assert!(ok, "{text}");
    assert_eq!(data_of(&c), EMPTY, "c's own entry goes too");
    for (from, to) in [(&c, &a), (&c, &b), (&a, &c)] {
        let (ok, text) = from.send(to);
        assert!(ok, "{text}");
    }
    for copy in [&a, &b, &c] {
        assert_eq!(members_of(copy), members);
        assert_eq!(data_of(copy), EMPTY);
    }
    // Each copy the order reached kept what it held, as a restore's does
    // (da-acwey); a, which ordered it, its own backup alone.
    assert!(kept(&a).is_empty(), "{:?}", kept(&a));
    assert_eq!((kept(&b).len(), kept(&c).len()), (1, 1));

    // Afterwards a new entry on any copy syncs to the others.
    c.ok(&["occur", "later"]);
    for (from, to) in [(&c, &b), (&b, &a)] {
        assert!(from.send(to).0);
    }
    assert!(a.has("later") && b.has("later"));
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), c.hash());

    // d is refused a session, so never told, and keeps what it holds.
    let hello = ["sync", "handshake", &d.id(), "--label", "d", "--addr", "{}"];
    a.fails(&hello, "");
    assert!(d.has("tea"));

    // a's backup, taken before it purged, restores what went.
    let backup = done.rsplit("backup: ").next().unwrap().trim();
    let fresh = Copy::new();
    fresh.ok(&["db", "restore", backup, "--yes"]);
    assert!(fresh.has("tea") && fresh.has("coffee") && !fresh.has("juice"));
}

/// da-acwey, vividn: «every member keeps one». c, offline, logs before a
/// chain purge it learns of later: what it held goes into a backup beside
/// it, which the next command, `check` and `sync status` name, saying
/// purged, until acknowledged, and `db restore --merge` of it brings the
/// entry back. The ordering copy keeps only its own, and a second purge
/// leaves the first one's backup.
#[test]
fn a_chain_purge_leaves_an_offline_copy_a_backup_of_what_it_drops() {
    let (a, c) = chained();
    a.ok(&["occur", "tea"]);
    assert!(a.send(&c).0);
    c.ok(&["occur", "before"]);
    a.ok(&["sync", "chain", "purge", "--data", "--yes"]);
    assert!(kept(&a).is_empty(), "{:?}", kept(&a));

    assert!(a.send(&c).0);
    assert_eq!(data_of(&c), EMPTY);
    let [file] = kept(&c).try_into().unwrap();
    let file = file.display().to_string();
    let lines = std::fs::read_to_string(&file).unwrap();
    assert!(
        lines.lines().next().unwrap().contains("daydumb_backup"),
        "{lines}"
    );
    assert!(lines.contains(r#""field":"before""#), "{lines}");
    assert!(lines.contains(r#""field":"tea""#), "{lines}");

    let next = c.run(&["tail"], "");
    let said = String::from_utf8_lossy(&next.stderr);
    let merge = format!("what this copy held is in {file} — `datum db restore {file} --merge`");
    assert!(
        said.contains("sync chain purged by ") && said.contains(&merge),
        "{said}"
    );
    assert!(c.ok(&["sync", "status"]).contains(&merge));
    let check = c.run(&["check", "sync"], "");
    let check = String::from_utf8_lossy(&check.stdout);
    assert!(
        check.contains("a restore or purge of the sync chain") && check.contains(&merge),
        "{check}"
    );

    c.ok(&["db", "restore", &file, "--merge"]);
    assert!(c.has("before") && c.has("tea"));
    c.ok(&["sync", "chain", "ack", "--all"]);
    assert!(!c.ok(&["sync", "status"]).contains(&merge));

    // A second purge: its own file, the first one's left in place.
    a.ok(&["sync", "chain", "purge", "--data", "--yes"]);
    assert!(a.send(&c).0);
    assert_eq!(data_of(&c), EMPTY);
    let both = kept(&c);
    assert!(
        both.len() == 2 && both.iter().any(|f| f.display().to_string() == file),
        "{both:?}"
    );
    assert!(kept(&a).is_empty(), "{:?}", kept(&a));
}

/// `sync chain purge --all` (da-qzrj9): the data goes and the chain
/// dissolves — every copy reads «in no sync chain», the offline one too once
/// it connects, here to b, which passes the order on. What one writes
/// afterwards reaches no other, the fold takes no record after it, and an
/// invite starts a new chain with a new key.
#[test]
fn an_all_purge_dissolves_the_chain_on_every_copy_as_it_connects() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    a.ok(&["occur", "tea"]);
    for other in [&b, &c] {
        assert!(a.send(other).0);
    }
    let said = a.ok(&["sync", "chain", "purge", "--all"]);
    assert!(said.contains("the chain dissolves"), "{said}");
    a.ok(&["sync", "chain", "purge", "--all", "--yes"]);
    assert!(b.has("tea") && c.has("tea"));
    assert!(a.send(&b).0);
    assert!(b.send(&c).0);
    for copy in [&a, &b, &c] {
        assert_eq!(data_of(copy), EMPTY);
        for cmd in [
            &["sync", "chain", "list"][..],
            &["sync", "admin", "list"],
            &["sync", "status"],
        ] {
            let out = copy.ok(cmd);
            assert!(out.contains("this copy is in no sync chain"), "{out}");
        }
        for cmd in [
            &["sync", "chain", "list", "--json"][..],
            &["sync", "admin", "list", "--json"],
        ] {
            let out = json(&copy.ok(cmd));
            assert!(
                out["chain"].is_null() && out["dissolved"].is_string(),
                "{out}"
            );
            assert_eq!(out["admins"], serde_json::json!([]), "{out}");
        }
        let status = json(&copy.ok(&["sync", "status", "--json"]));
        assert!(status["chain"].is_null() && status["dissolved"].is_string());
        assert_eq!(
            (&status["open"], &status["members"]),
            (&serde_json::json!(false), &serde_json::json!([]))
        );
    }

    a.ok(&["occur", "later"]);
    assert!(a.send(&b).0);
    assert!(!b.has("later"));
    let refused = a.fails(&["sync", "chain", "expel", &b.id()[..10]], "");
    assert!(refused.contains("dissolved by a purge"), "{refused}");

    let old = a.id();
    a.ok(&["sync", "new-invite"]);
    assert_ne!(a.id(), old);
    assert!(a.ok(&["sync", "chain", "list"]).contains("sync chain"));
    assert!(a.has("later"));
    // A dissolved copy is in no chain to `db purge` either: nothing to
    // leave, nothing to rejoin.
    c.ok(&["db", "purge", "--yes"]);
}

/// In a locked chain a purge is an admin act (da-qzrj9): any member orders
/// it with the admin key, and none without.
#[test]
fn in_a_locked_chain_a_purge_takes_the_admin_key() {
    let (a, b, admin) = locked();
    b.ok(&["occur", "tea"]);
    let refused = b.fails(&["sync", "chain", "purge", "--data", "--yes"], "");
    assert!(refused.contains("no approval code"), "{refused}");
    assert!(b.has("tea"));
    // Refused, it leaves no backup of a purge that never happened.
    let beside = std::fs::read_dir(b.tmp.path()).unwrap();
    let names: Vec<_> = beside.map(|f| f.unwrap().file_name()).collect();
    assert!(
        !names
            .iter()
            .any(|n| n.to_string_lossy().contains(".purge-")),
        "{names:?}"
    );
    b.ok(&[
        "sync",
        "chain",
        "purge",
        "--data",
        "--yes",
        "--admin-key",
        &admin,
    ]);
    assert_eq!(data_of(&b), EMPTY);
    assert!(b.send(&a).0);
    assert_eq!(data_of(&a), EMPTY);
}

/// datum.day's purge in a passkey-locked chain (da-459uh): the order is
/// refused at its approve link, leaving no backup, and `sync approve`
/// writes the wipe — after the backup the order would have written.
#[test]
fn a_passkey_approved_purge_leaves_its_backup() {
    let a = Copy::new();
    a.ok(&["sync", "new-invite"]);
    a.ok(&["sync", "admin", "add", &passkey_json()]);
    a.ok(&["occur", "tea"]);
    let out = a.run(&["db", "purge", "--chain", "--yes"], "");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    let link = err
        .split_whitespace()
        .find(|w| w.contains("/sync/approve#"))
        .unwrap();
    let record = BASE64URL_NOPAD
        .decode(link.rsplit('#').next().unwrap().as_bytes())
        .unwrap();
    let said = a.ok_in(
        &["sync", "approve", link],
        &approval_code(&Sha256::digest(&record)),
    );
    assert!(!a.has("tea"));
    let backup = said
        .split("approved: wipe; backup: ")
        .nth(1)
        .unwrap()
        .trim();
    let fresh = Copy::new();
    fresh.ok(&["db", "restore", backup, "--yes"]);
    assert!(fresh.has("tea"));
}

/// A filtered reader's holes lie below what it holds, by design: a purge
/// empties it all the same, with no stub sent or waited on, and what is
/// written afterwards reaches it (da-qzrj9).
#[test]
fn a_filtered_reader_with_holes_is_purged_and_reads_on() {
    let (a, _b) = chained();
    let r = Copy::new();
    admit_filtered(&a, &r, "coffee");
    for field in ["coffee", "tea", "coffee", "tea", "coffee"] {
        a.ok(&["occur", field]);
    }
    assert!(feed(&a, &r).0);
    let held = |sql: &str| r.query(&format!("SELECT COUNT(*) FROM ops WHERE {sql}"));
    let gaps = "seq < (SELECT MAX(seq) FROM ops o WHERE o.endpoint = ops.endpoint)";
    assert_ne!(held(&format!("hid != '~chain' AND {gaps}")), "0");

    a.ok(&["sync", "chain", "purge", "--data", "--yes"]);
    a.ok(&["occur", "tea"]);
    a.ok(&["occur", "coffee", "note=after"]);
    assert!(feed(&a, &r).0);
    let rows = |c: &Copy, of: &str| {
        c.query(&format!(
            "SELECT group_concat(slug, ' ') FROM (SELECT slug FROM entries {of} ORDER BY slug)"
        ))
    };
    let coffee = "WHERE field = 'coffee' OR slug = 'spec:coffee'";
    assert_eq!(rows(&a, coffee).split(' ').count(), 2);
    assert_eq!(rows(&r, ""), rows(&a, coffee));
    assert!(!holds_any(&r, "tea"));
    assert_eq!(held("applied = 0"), "0");
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));

    // Its filter changing reads past the stubs it holds, and keeps them.
    let stubs = held("applied = 3");
    assert_ne!(stubs, "0");
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
    assert!(holds_any(&r, "tea"));
    assert_eq!(held("applied = 3"), stubs);
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
}

/// A change queued before a wipe is the old copy's data (da-59hwx): landed
/// after the wipe, signed again as this copy's next, it would outlive the
/// order and reach every member. Here one waits on the ordering copy and
/// one on a copy the order reaches, each behind a drain that stepped aside
/// (another holds the spool's lock), and each drain after the wipe drops
/// its line and says so. A change queued after the order still lands.
#[test]
fn a_change_queued_before_a_wipe_does_not_outlive_it() {
    let (a, b) = chained();
    a.ok(&["occur", "tea"]);
    assert!(a.send(&b).0);
    // Queued behind the write lock, then kept queued behind the spool's.
    let queue = |copy: &Copy, field: &str| {
        let db = copy.tmp.path().join("data.db");
        let holder = rusqlite::Connection::open(&db).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(copy.ok(&["occur", field]).contains("QUEUED"));
        let spool = std::fs::File::create(copy.key("data.db.spool.lock")).unwrap();
        spool.lock().unwrap();
        spool
    };
    let held = (queue(&a, "old_a"), queue(&b, "old_b"));
    a.ok(&["sync", "chain", "purge", "--data", "--yes"]);
    assert!(a.send(&b).0);
    assert_eq!(data_of(&b), EMPTY);
    drop(held);
    for copy in [&a, &b] {
        let out = copy.run(&["tail"], "");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains("dropped 1 queued change(s) made before the sync chain's purge"),
            "{said}"
        );
        assert!(!said.contains("written"), "{said}");
        assert_eq!(data_of(copy), EMPTY);
    }
    let db = b.tmp.path().join("data.db");
    let holder = rusqlite::Connection::open(&db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(b.ok(&["occur", "new"]).contains("QUEUED"));
    drop(holder);
    b.ok(&["tail"]);
    for (from, to) in [(&a, &b), (&b, &a)] {
        assert!(from.send(to).0);
    }
    for copy in [&a, &b] {
        assert!(copy.has("new") && !copy.has("old_a") && !copy.has("old_b"));
    }
    assert_eq!(a.hash(), b.hash());
}

/// A change queued before a wipe but landed before the order reaches its
/// copy is the old copy's data too (da-m6c4j): signed again as it drains,
/// it carries when its command ran, and the wipe reads that. Here it
/// reaches the ordering copy after the wipe, and the order reaches it
/// after; both take it.
#[test]
fn a_change_queued_before_a_wipe_and_landed_before_it_arrives_does_not_outlive_it() {
    let (a, b) = chained();
    a.ok(&["occur", "tea"]);
    assert!(a.send(&b).0);
    let db = b.tmp.path().join("data.db");
    let holder = rusqlite::Connection::open(&db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(b.ok(&["occur", "old_b"]).contains("QUEUED"));
    a.ok(&["sync", "chain", "purge", "--data", "--yes"]);
    drop(holder);
    b.ok(&["tail"]);
    assert!(b.has("old_b"));
    assert!(b.send(&a).0);
    assert!(!a.has("old_b") && data_of(&a) == EMPTY);
    assert!(a.send(&b).0);
    assert!(!b.has("old_b") && data_of(&b) == EMPTY);
    assert_eq!(a.hash(), b.hash());
}

/// da-a2wyw (vividn 10-16): without --yes, a chain purge and a revert say what
/// goes, then on a TTY ask for the database's name, as `db purge` does: a
/// wrong answer writes nothing, the right one does it. Off a TTY they change
/// nothing and name the command as it was typed, with --yes.
#[test]
fn a_chain_purge_and_a_revert_ask_on_a_tty_and_name_the_command_as_typed() {
    let (a, b) = chained();
    a.ok(&["occur", "tea"]);
    assert!(a.send(&b).0);
    b.ok(&["occur", "coffee"]);
    assert!(b.send(&a).0);
    for (argv, typed) in [
        (
            &["db", "purge", "--chain"][..],
            "datum db purge --chain --yes",
        ),
        (
            &["sync", "chain", "purge", "--all"],
            "datum sync chain purge --all --yes",
        ),
        (
            &["sync", "chain", "unjoin", "b"],
            "datum sync chain unjoin b --yes",
        ),
    ] {
        let said = a.ok(argv);
        assert!(
            said.contains(&format!("nothing changed: `{typed}` does it")),
            "{said}"
        );
    }
    assert!(a.has("tea") && a.has("coffee"));

    let path = a.tmp.path().join("data.db");
    let path = path.to_str().unwrap();
    let db = datum_core::db::Db::open(path).unwrap();
    // This thread signs as a's client, so it is a's endpoint.
    thread_local!(static CLIENT: std::cell::RefCell<String> = Default::default());
    CLIENT.with(|c| *c.borrow_mut() = a.query("SELECT client_id FROM sync_local"));
    datum_core::db::set_client_source(|| Some(CLIENT.with(|c| c.borrow().clone())));
    let on_a_tty = |words: &[&str], answer: &str| {
        let argv: Vec<String> = std::iter::once("datum")
            .chain(words.iter().copied())
            .map(Into::into)
            .collect();
        let datum_parse::Command::Sync(cmd) = datum_parse::parse_argv(&argv).unwrap().command
        else {
            unreachable!("{words:?} is a sync command")
        };
        datum_commands::commands::sync::sync_with_io(
            cmd,
            Some(path),
            &db,
            &mut answer.as_bytes(),
            true,
        )
    };
    for words in [
        &["sync", "chain", "unjoin", "b"][..],
        &["db", "purge", "--chain"],
    ] {
        let e = on_a_tty(words, "nope\n").unwrap_err();
        assert!(
            e.to_string().contains("aborted (input did not match"),
            "{e}"
        );
        assert!(a.has("tea") && a.has("coffee"), "{words:?} wrote nothing");
    }
    // da-3aq7i: the hint, pasteable, keeps the global flags typed; the
    // prompt and its abort line name the command without them.
    let said = a.ok(&["--db", path, "--color", "never", "db", "purge", "--chain"]);
    assert!(
        said.contains(&format!(
            "nothing changed: `datum --db {path} --color never db purge --chain --yes` does it"
        )),
        "{said}"
    );
    let raw_argv: Vec<String> = ["--db", "p", "--color=never", "sync", "chain", "unjoin", "b"]
        .map(Into::into)
        .into();
    datum_commands::args::set_intent_context(datum_commands::args::IntentContext {
        raw_input: raw_argv.join(" "),
        raw_argv,
        alias_chain: Vec::new(),
    });
    let e = on_a_tty(&["sync", "chain", "unjoin", "b"], "nope\n").unwrap_err();
    assert!(
        e.to_string().starts_with("sync chain unjoin b: aborted"),
        "{e}"
    );
    datum_commands::args::take_intent_context();
    on_a_tty(&["sync", "chain", "unjoin", "b"], &format!("{path}\n")).unwrap();
    assert!(a.has("tea") && !a.has("coffee"));
    on_a_tty(&["db", "purge", "--chain"], &format!("{path}\n")).unwrap();
    assert_eq!(data_of(&a), EMPTY);
}

/// A revert's preview names the span of what goes on the reader's clock, as
/// `sync status` does, and --json keeps the instants (da-rzxnr).
#[test]
fn a_revert_preview_names_its_span_on_the_readers_clock() {
    let (a, b) = chained();
    b.ok(&["occur", "coffee"]);
    assert!(b.send(&a).0);
    let p = json(&a.ok(&["sync", "chain", "unjoin", "b", "--json"]));
    let kathmandu = |at: &serde_json::Value| {
        let at = chrono::DateTime::parse_from_rfc3339(at.as_str().unwrap()).unwrap();
        let east = chrono::FixedOffset::east_opt(5 * 3600 + 45 * 60).unwrap();
        at.with_timezone(&east).format("%H:%M").to_string()
    };
    let out = a
        .cmd(&["sync", "chain", "unjoin", "b"])
        .env("TZ", "Asia/Kathmandu")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8(out.stdout).unwrap();
    let span = format!(
        "every op it signed, {} to {}: 1 entry it created goes",
        kathmandu(&p["first"]),
        kathmandu(&p["last"])
    );
    assert!(said.contains(&span), "{span} in {said}");
}
