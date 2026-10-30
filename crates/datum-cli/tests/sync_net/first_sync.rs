//! A first sync: keepalives through a slow apply or verify, and how far it says it is.

use super::*;

#[test]
fn a_first_sync_longer_than_the_timeout_completes_on_keepalives() {
    let (a, b, joined, (ok, served)) = slow_first_sync(true);
    let text = String::from_utf8_lossy(&joined.stdout);
    assert!(
        joined.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&joined.stderr)
    );
    assert!(text.contains("verify ok"), "{text}");
    assert!(ok && served.contains("verify ok"), "{served}");
    assert_eq!(a.hash(), b.hash());
}

#[test]
fn without_keepalives_it_fails_says_what_landed_and_the_invite_still_exits() {
    let (a, b, joined, (ok, served)) = slow_first_sync(false);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&joined.stdout),
        String::from_utf8_lossy(&joined.stderr)
    );
    assert!(!joined.status.success(), "{text}");
    assert!(text.contains("applied 30"), "{text}");
    assert!(text.contains("paired with"), "{text}");
    // The redeemer is told it was admitted, and what finishes it (da-0tawk):
    // cut short before the chain's records, it is only joining (da-idw54).
    assert!(
        text.contains("this copy was admitted to its sync chain")
            && text.contains("`datum sync` finishes it"),
        "{text}"
    );
    // The admission held although the session failed: the invite exits 0.
    assert!(ok, "{served}");
    assert!(
        served.contains("paired with") && served.contains("run `datum sync`"),
        "{served}"
    );

    // And it is a pairing: the next sync finishes what the first began.
    let (mut a, mut b) = (a, b);
    a.env.clear();
    b.env.clear();
    let _a_runs = a.serve(&["sync", "run"]);
    let now = b.ok(&["sync"]);
    assert!(now.contains("verify ok"), "{now}");
    assert_eq!(a.hash(), b.hash());
}

/// A first sync whose verify outlasts the progress timeout (da-0mwws): the
/// joiner's or the inviter's hash holds its thread for 6s on a 3s timeout,
/// and the peer, whose own is done, hears a `BUSY` every 0.75s unless
/// `keepalive` is off. Whether the join and the invite succeeded, and what
/// each printed on stdout and stderr.
fn slow_verify(joiner: bool, keepalive: bool) -> ((bool, String), (bool, String)) {
    let (mut a, mut b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea"]);
    for c in [&mut a, &mut b] {
        c.env = vec![
            ("DATUM_SYNC_TIMEOUT_S", "3"),
            (
                "DATUM_SYNC_TEST_KEEPALIVE",
                if keepalive { "1" } else { "0" },
            ),
        ];
    }
    let slow = if joiner { &mut b } else { &mut a };
    slow.env.push(("DATUM_SYNC_TEST_VERIFY_DELAY_S", "6"));
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.run(&["sync", "chain", "join", &ticket]);
    let (served_ok, mut served) = inviting.finish();
    let err = inviting.child.stderr.as_mut().unwrap();
    err.read_to_string(&mut served).unwrap();
    let joined_text = format!(
        "{}{}",
        String::from_utf8_lossy(&joined.stdout),
        String::from_utf8_lossy(&joined.stderr)
    );
    ((joined.status.success(), joined_text), (served_ok, served))
}

#[test]
fn a_verify_longer_than_the_timeout_completes_on_keepalives_and_says_so() {
    for joiner in [true, false] {
        let ((ok, joined), (served_ok, served)) = slow_verify(joiner, true);
        assert!(ok && joined.contains("verify ok"), "{joined}");
        assert!(served_ok && served.contains("verify ok"), "{served}");
        let slow = if joiner { &joined } else { &served };
        assert!(
            slow.contains(": verifying that both copies agree ("),
            "{slow}"
        );
    }
}

#[test]
fn without_keepalives_a_slow_verify_times_out() {
    let ((ok, joined), (_, served)) = slow_verify(true, false);
    assert!(!ok && !joined.contains("verify ok"), "{joined}");
    assert!(served.contains("nothing moved for 3s"), "{served}");
}

/// A first sync says how far it is on both sides (da-nyg6u): vividn's joiner
/// went silent for a minute and then printed ten lines that all said «87s»,
/// and his inviter said nothing until it was done. b applies a's 300-odd ops
/// at 40ms each: 12s, in chunks paced to hold its thread for 3.75s.
#[test]
fn a_first_sync_says_how_far_it_is_on_both_sides() {
    let (a, mut b) = (Copy::new(), Copy::new());
    for i in 0..300 {
        a.ok(&["occur", "step", &format!("n={i}")]);
    }
    b.env = vec![("DATUM_SYNC_TEST_APPLY_DELAY_MS", "40")];
    let (mut inviting, ticket) = a.invite(&[]);
    let mut joining = b.cmd(&["sync", "chain", "join", &ticket]);
    let mut joining = joining.spawn().expect("spawn datum");

    // The inviter names the joiner while the joiner is still at it.
    let mut said = BufReader::new(inviting.child.stderr.take().unwrap());
    let mut named = String::new();
    while !named.contains(" is joining: sending") && said.read_line(&mut named).unwrap() > 0 {}
    assert!(named.contains(" is joining: sending"), "{named}");
    assert!(joining.try_wait().unwrap().is_none(), "the join had ended");

    let joined = joining.wait_with_output().unwrap();
    let (out, err) = (
        String::from_utf8_lossy(&joined.stdout),
        String::from_utf8_lossy(&joined.stderr),
    );
    assert!(
        joined.status.success() && out.contains("verify ok"),
        "{out}{err}"
    );
    // The joiner says how many ops are applied, and each line it says is an
    // interval on from the last: none held back, then said together.
    let at: Vec<u64> = err
        .lines()
        .filter_map(|l| l.strip_suffix("s)")?.rsplit_once('(')?.1.parse().ok())
        .collect();
    assert!(
        at.len() > 1 && at.windows(2).all(|w| w[0] + 5 <= w[1]),
        "{err}"
    );
    assert!(err.contains(" ops applied ("), "{err}");

    let (ok, served) = inviting.finish();
    assert!(ok && served.contains("verify ok"), "{served}");
    let mut rest = String::new();
    said.read_to_string(&mut rest).unwrap();
    assert!(
        rest.contains(" ops sent, waiting for it to apply them ("),
        "{rest}"
    );
}

/// An invite serves every member while its joiner joins (da-dwm2y): the
/// daemon steps aside for it, so its endpoint is the copy's, and it served
/// the joiner alone: a member dialing it through a long join reached no
/// one, and said it was offline. c applies a's 300-odd ops at 50ms each.
#[test]
fn an_invite_serves_the_other_members_while_its_joiner_joins() {
    let (a, mut b) = paired();
    let mut c = Copy::new();
    import(&a, 300);
    b.env.push(("DATUM_SYNC_TIMEOUT_S", "5"));
    c.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "50"));
    let (mut inviting, ticket) = a.invite(&[]);
    let mut joining = c.cmd(&["sync", "chain", "join", &ticket]).spawn().unwrap();
    until("c applies a's first ops", Duration::from_secs(60), || {
        // Read as c writes: a lock or a table not there yet is «not yet».
        rusqlite::Connection::open(c.db())
            .and_then(|db| {
                db.query_row("SELECT COUNT(*) FROM ops WHERE applied > 0", [], |r| {
                    r.get::<_, i64>(0)
                })
            })
            .is_ok_and(|n| n > 0)
    });
    let synced = b.run(&["sync"]);
    let said = String::from_utf8_lossy(&synced.stdout);
    assert!(
        joining.try_wait().unwrap().is_none(),
        "the join had ended: {said}"
    );
    assert!(
        said.contains("verify ok"),
        "{said}{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    assert!(joining.wait().unwrap().success());
    assert!(inviting.finish().0);
}

/// A first sync of many rounds says how far it is in all, not in the round
/// (da-dwm2y): vividn's phone, joining 287k entries in rounds of 8,192, said
/// «of 8,192» some 35 times over. Here a round is 200 ops of a's 600-odd,
/// applied at 30ms each: six seconds a round, so a five-second line lands in
/// the first. The inviter says the same of what it sends (da-ija0z):
/// vividn's said «3418 of 8192 ops sent», then «1057 of 8192», a round's ops
/// read as the whole. An import's baselines are signed in the first round,
/// after the vectors are swapped, so the joiner's first lines know of one
/// round's ops alone and name it, and the inviter's know them all
/// (da-y8yde): its first said «8192 ops sent» and no round. Between rounds
/// the joiner says what it applied, or has received, of the next: a reads
/// each round's ops as slowly as b applies them, so a line lands there too
/// (da-impts), which a loaded runner alone had found. The inviter, reading
/// them and signing the import's baselines, says it is preparing the round
/// (da-la01w).
#[test]
fn a_first_sync_of_many_rounds_counts_every_round() {
    let (mut a, mut b) = (Copy::new(), Copy::new());
    import(&a, 600);
    a.env = vec![
        ("DATUM_SYNC_TEST_ROUND", "200"),
        ("DATUM_SYNC_TEST_SEND_DELAY_MS", "30"),
    ];
    b.env = vec![
        ("DATUM_SYNC_TEST_ROUND", "200"),
        ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "30"),
    ];
    let (mut inviting, ticket) = a.invite(&[]);
    let joined = b.cmd(&["sync", "chain", "join", &ticket]).output().unwrap();
    let err = String::from_utf8_lossy(&joined.stderr);
    assert!(joined.status.success(), "{err}");
    assert!(inviting.finish().0);
    let mut sent = String::new();
    let mut e = inviting.child.stderr.take().unwrap();
    e.read_to_string(&mut sent).unwrap();
    // Its round, and how many it has sent in all, never go back.
    let rounds: Vec<(usize, usize, usize)> = sent
        .lines()
        .filter_map(|l| {
            let (round, rest) = l.split_once(": round ")?.1.split_once(": ")?;
            let (_, rest) = rest.split_once(" ops sent, ")?;
            let (all, rest) = rest.split_once(" of about ")?;
            let (total, _) = rest.split_once(" in all")?;
            Some((round.parse().ok()?, all.parse().ok()?, total.parse().ok()?))
        })
        .collect();
    assert!(rounds.len() > 1 && rounds.last().unwrap().0 > 1, "{sent}");
    let on = |w: &[(usize, usize, usize)]| w[0].0 <= w[1].0 && w[0].1 <= w[1].1;
    assert!(rounds.windows(2).all(on), "{sent}");
    // In all, not a round's: a count of the round alone never passes 200.
    assert!(rounds.iter().any(|r| r.1 > 200), "{sent}");
    assert!(!sent.contains(" of 200 ops sent ("), "{sent}");
    // Every line says so from the first round, with the whole of it.
    let first = sent.lines().find(|l| l.contains(" ops sent")).unwrap_or("");
    assert!(first.contains(": round 1: "), "{sent}");
    assert!(rounds.iter().all(|r| r.2 >= 600), "{sent}");
    // Nor is a line before its first round's ops a bare «syncing cantown»:
    // it reads them for six seconds (da-la01w).
    let bare = |l: &&str| l.starts_with("syncing ") && !l.contains(": ");
    assert!(!sent.lines().any(|l| bare(&l)), "{sent}");
    let signs = ": preparing round 1: reading what to send, 600 baselines signed (";
    assert!(sent.contains(signs), "{sent}");
    let said: Vec<(usize, usize, usize)> = err
        .lines()
        .filter_map(|l| {
            let (round, rest) = l.split_once(": round ")?.1.split_once(": ")?;
            let (done, rest) = rest.split_once(" of ")?;
            let (of, _) = rest.split_once(" ops applied")?;
            Some((round.parse().ok()?, done.parse().ok()?, of.parse().ok()?))
        })
        .collect();
    // The joiner names its round from the first, where it knows of no more.
    assert!(said.first().is_some_and(|r| r.0 == 1), "{err}");
    let applied = err.lines().filter(|l| l.contains(" ops applied")).count();
    assert_eq!(applied, said.len(), "{err}");
    // Nor is a line between its rounds a bare «syncing cantown» (da-impts).
    assert!(!err.lines().any(|l| bare(&l)), "{err}");
    let waits = |l: &str| l.contains(": round 2: 0 ops received (");
    assert!(err.lines().any(waits), "{err}");
    assert!(said.last().is_some_and(|r| r.2 >= 600), "{err}");
    let on = |w: &[(usize, usize, usize)]| w[0].0 <= w[1].0 && w[0].1 <= w[1].1;
    assert!(said.windows(2).all(on), "{err}");
}

/// A slow copy catching up from three members at once (da-h31wt): vividn's,
/// after a purge, lost every session but one to «connection lost: timed
/// out» or «database is locked». x applies at 80 ms an op, on a 12 s timeout
/// (a loaded runner stalled 3 s, then 6 s: da-hd9sx, da-lhbzr), while three
/// members running `sync run` each bring it 100 entries, and a command is
/// typed on it meanwhile: all four converge, and none says a session failed,
/// giving way to a write being none. With `DATUM_SYNC_TEST_KEEPALIVE=0` it fails.
#[test]
fn a_slow_copy_catches_up_from_three_members_at_once() {
    let mut x = Copy::new();
    let mut peers: Vec<Copy> = (0..3).map(|_| Copy::new()).collect();
    for p in &peers {
        let (mut inviting, ticket) = x.invite(&[]);
        p.ok(&["sync", "chain", "join", &ticket]);
        assert!(inviting.finish().0);
    }
    for (i, p) in peers.iter().enumerate() {
        for n in 0..100 {
            p.ok(&["occur", &format!("p{i}"), &format!("n={n}")]);
        }
    }
    let said = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut runs = Vec::new();
    let mut up = |c: &Copy| {
        let mut child = Killed(c.cmd(&["sync", "run"]).spawn().unwrap());
        for rx in [
            lines(child.0.stdout.take().unwrap()),
            lines(child.0.stderr.take().unwrap()),
        ] {
            let (said, who) = (said.clone(), c.env[0].1);
            std::thread::spawn(move || {
                rx.iter()
                    .for_each(|l| said.lock().unwrap().push(format!("{who}: {l}")))
            });
        }
        runs.push(child);
    };
    let timeout = ("DATUM_SYNC_TIMEOUT_S", "12");
    x.env
        .extend([timeout, ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "80")]);
    up(&x);
    for p in &mut peers {
        p.env.extend([timeout, ("DATUM_SYNC_TEST_RETRY_MS", "200")]);
        up(p);
    }
    let count = || x.query("SELECT COUNT(*) FROM entries WHERE field LIKE 'p_'");
    let start = Instant::now();
    while count() == "0" {
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "x applied nothing"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    x.ok(&["occur", "typed"]);
    assert_ne!(count(), "300", "the command was typed after the catch-up");
    while !peers.iter().all(|p| p.has("typed") && p.hash() == x.hash()) {
        assert!(
            start.elapsed() < Duration::from_secs(480),
            "converged: {:#?}",
            said.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    drop(runs);
    let said = said.lock().unwrap();
    let failed: Vec<&String> = said
        .iter()
        .filter(|l| !l.contains("gave way to a write"))
        .filter(|l| {
            ["locked", "timed out", "connection lost", "failed"]
                .iter()
                .any(|w| l.contains(w))
        })
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}
