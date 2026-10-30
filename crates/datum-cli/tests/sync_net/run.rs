//! `sync run` and the daemon: serving, pushing, a bare `sync` asking the run, giving
//! way to a write, and when an unreachable member is tried again.

use super::*;

#[test]
fn run_serves_members_and_pushes_new_ops() {
    let (a, b) = paired();
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["occur", "walk"]);
    let now = b.ok(&["sync"]);
    // The walk, the spec its first log declares, and the describe b wrote
    // of itself once admitted.
    assert!(
        now.contains("sent 3, got 0, conflicts 0, verify ok"),
        "{now}"
    );
    assert!(a.has("walk"));

    let _b_runs = b.serve(&["sync", "run"]);
    a.ok(&["occur", "swim"]);
    eventually("a's write reaching b", || b.has("swim"));
    eventually("equal hashes", || a.hash() == b.hash());
}

/// `datum sync` on a copy a `sync run` serves asks the run to sync
/// (da-tltjj): it binds nothing of its own — here the copy's one port, which
/// the run holds, so a bind would fail — and prints each member's session as
/// the run's, failing when one did. With the run killed, its socket left
/// beside the database, it dials as it always did.
#[test]
fn sync_asks_the_run_serving_its_copy_and_binds_nothing_of_its_own() {
    let (a, mut b) = paired();
    b.env.push(("DATUM_SYNC_DIAL_TIMEOUT_S", "2"));
    let mut a_runs = a.serve(&["sync", "run"]);
    let b_runs = b.serve(&["sync", "run"]);
    let socket = PathBuf::from(format!("{}.sync", b.db().display()));
    eventually("b's run taking asks", || socket.exists());

    b.ok(&["occur", "walk"]);
    let out = b.run(&["sync"]);
    let (said, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{said}{err}");
    assert!(
        err.contains("this copy's sync daemon is syncing it"),
        "{err}"
    );
    assert!(said.contains("verify ok"), "{said}");
    assert!(a.has("walk"));

    // A member it cannot reach is named offline, its words in `-v`'s line,
    // which the run sends too (da-a4fxr); the only one, it fails the sync
    // as none synced (da-9d6uy).
    a_runs.child.kill().unwrap();
    a_runs.child.wait().unwrap();
    let said = b.fails(&["sync", "-v"], "");
    assert!(
        said.contains("this copy's sync daemon is syncing it"),
        "{said}"
    );
    assert!(said.contains(": offline, last seen "), "{said}");
    assert!(said.contains("s · could not reach"), "{said}");
    assert!(said.contains("none of 1 peer(s) synced"), "{said}");

    let _a_runs = a.serve(&["sync", "run"]);
    // Killed and waited for (`Serving`'s drop): SIGKILL runs no destructor,
    // so the socket stays, as a crashed daemon's would.
    drop(b_runs);
    assert!(socket.exists(), "a killed run leaves its socket");
    b.ok(&["occur", "swim"]);
    let out = b.run(&["sync"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("sync daemon"), "{err}");
    assert!(a.has("swim"));
}

/// An ask the member refuses as busy applying a session the run dialed it
/// waits that session out and dials again, so `datum sync` says this one
/// verified, not that none synced (da-sdrol, as da-221fe for one served).
/// a's dial to b at start reaches no run and backs off, so the session a
/// applies, slowly, is b's run's dial at start.
#[test]
fn an_ask_waits_out_the_session_the_run_dialed_the_member_applies() {
    let (mut a, b) = paired();
    import_as(&b, "stair", 300);
    b.ok(&["sync", "pipe", "ops", "--limit", "1"]);
    a.env.extend([
        ("DATUM_SYNC_TIMEOUT_S", "2"),
        ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "10"),
    ]);
    let _a_runs = logged(&a, &["sync", "run"], "run.log");
    until("a's dial at start failing", Duration::from_secs(20), || {
        log(&a, "run.log").contains("next try in")
    });
    let _b_runs = b.serve(&["sync", "run"]);
    until("a applying b's dial", Duration::from_secs(20), || {
        a.has("stair")
    });
    let out = b.run(&["sync"]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{said}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(said.contains("verify ok"), "{said}");
    assert_eq!(
        a.query("SELECT COUNT(*) FROM entries WHERE field = 'stair'"),
        "300"
    );
}

/// An answer cut short — the run stopped with a session open — fails the
/// ask rather than reads as done, and one this build cannot read says so.
#[test]
fn an_answer_cut_short_or_unreadable_fails_the_ask() {
    let (_a, b) = paired();
    let socket = PathBuf::from(format!("{}.sync", b.db().display()));
    for (answer, says) in [
        (
            "2\n{\"ok\":true,\"line\":\"a: verify ok\"}\n",
            "stopped before it finished",
        ),
        ("two\n", "cannot read"),
        ("1\n{\"ok\":true}\n", "cannot read"),
    ] {
        let run = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let answering = std::thread::spawn(move || {
            run.accept()
                .unwrap()
                .0
                .write_all(answer.as_bytes())
                .unwrap();
        });
        let said = b.fails(&["sync"], "");
        answering.join().unwrap();
        assert!(said.contains(says), "{answer:?}: {said}");
        std::fs::remove_file(&socket).unwrap();
    }
}

/// `sync daemon run` (da-vd2sn) finds a chained database datum knows of —
/// here by the registry — and serves it as `sync run` does.
#[test]
fn daemon_run_serves_every_chained_database() {
    let (a, b) = paired();
    let config = b.tmp.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let registry = format!("[db.data]\npath = {:?}\n", b.db().display().to_string());
    std::fs::write(config.join("dbs.toml"), registry).unwrap();
    let mut daemon = b.cmd(&["sync", "daemon", "run"]);
    let _b_runs = Serving::of(daemon.stderr(Stdio::inherit()));
    a.ok(&["occur", "swim"]);
    eventually("a's sync reaching b's daemon", || {
        a.run(&["sync"]).status.success() && b.has("swim")
    });
}

/// `sync run` is the one that gives way (da-lk1s2): with a write holding its
/// file, it fails a session within its short busy timeout, where any other
/// connection waits out 5 s, and tries every peer again at its next tick
/// rather than after a failed push's backoff — quietly: the peer it refused
/// records no error (da-h31wt).
#[test]
fn run_gives_way_to_a_write_holding_its_file_and_tries_again_at_once() {
    let (a, b) = paired();
    let mut a_runs = a.serve(&["sync", "run"]);
    let (tx, rx) = std::sync::mpsc::channel();
    let err = BufReader::new(a_runs.child.stderr.take().unwrap());
    std::thread::spawn(move || {
        err.lines()
            .map_while(Result::ok)
            .try_for_each(|l| tx.send(l))
    });
    let until = |needle: &str| loop {
        let line = rx.recv_timeout(Duration::from_secs(10)).expect(needle);
        if line.contains(needle) {
            break;
        }
    };
    // Up before b first pushes, or that push is unreachable and waits a minute.
    until("is up");
    let _b_runs = b.serve(&["sync", "run"]);
    let write = rusqlite::Connection::open(a.db()).unwrap();
    write.execute_batch("BEGIN IMMEDIATE").unwrap();

    b.ok(&["occur", "walk"]);
    let start = Instant::now();
    until("gave way to a write");
    let took = start.elapsed();
    assert!(took < Duration::from_secs(4), "a gave way after {took:?}");
    assert!(!a.has("walk"));

    // Giving way failed nobody: b, refused while a's file is held, does
    // not say so in its status (da-h31wt).
    std::thread::sleep(Duration::from_millis(500));
    let said = b.query("SELECT COUNT(*) FROM sync_peers WHERE last_error IS NOT NULL");
    assert_eq!(said, "0", "{}", b.ok(&["sync", "status"]));

    write.execute_batch("COMMIT").unwrap();
    // A miss says what a's run said since: CI's log has nothing else (da-le37o).
    let start = Instant::now();
    while !a.has("walk") {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "b's walk reaching a within 10s; a's run said:\n{}",
            rx.try_iter().collect::<Vec<_>>().join("\n")
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// a's dial given way to b's session with it (da-vj3ly, da-mghh3), heard
/// only once that session gave way to a write on a's file, waits no
/// minute: b, refused by the give-way, waits on a, and a pushes once the
/// write ends (da-le37o). On a loaded box that order is rare; the knob makes
/// a slow on purpose, and b's `datum sync`s dial a while a dials b, so a's
/// dial is welcomed while a serves b's, and a hears it closed only after.
#[test]
fn a_dial_given_way_to_a_session_that_gave_way_to_a_write_waits_no_minute() {
    let (x, y) = paired();
    // Only the higher endpoint id closes its own dial for the other's.
    let (mut a, b) = if x.id() > y.id() { (x, y) } else { (y, x) };
    a.env.push(("DATUM_SYNC_TEST_LATE_MS", "600"));
    let mut a_runs = a.serve(&["sync", "run"]);
    let a_err = lines(a_runs.child.stderr.take().unwrap());
    while !a_err
        .recv_timeout(Duration::from_secs(10))
        .expect("a up")
        .contains("is up")
    {}
    let mut b_runs = b.serve(&["sync", "run"]);
    b_runs.lines_until(" got ");
    let write = rusqlite::Connection::open(a.db()).unwrap();
    write.execute_batch("BEGIN IMMEDIATE").unwrap();

    b.ok(&["occur", "walk"]);
    let start = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                while start.elapsed() < Duration::from_secs(6) {
                    b.run(&["sync"]);
                }
            });
        }
    });
    write.execute_batch("COMMIT").unwrap();
    let start = Instant::now();
    while !a.has("walk") {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "b's walk reaching a within 10s; a's run said:\n{}",
            a_err.try_iter().collect::<Vec<_>>().join("\n")
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A peer whose file a write holds refuses as busy, and this copy waits the
/// applying gap, a minute, to push to it again. The peer gives way itself
/// and pushes once the write ends, which is what brings it the change, well
/// inside [`eventually`]'s ten seconds.
#[test]
fn a_peer_held_by_a_write_comes_back_for_the_change_itself() {
    let (a, b) = paired();
    let mut a_runs = a.serve(&["sync", "run"]);
    let a_err = lines(a_runs.child.stderr.take().unwrap());
    while !a_err
        .recv_timeout(Duration::from_secs(10))
        .expect("a up")
        .contains("is up")
    {}
    // b pushes to a as it starts, so it has pushed all it holds and pushes
    // again only once it gives way.
    let mut b_runs = b.serve(&["sync", "run"]);
    let b_err = lines(b_runs.child.stderr.take().unwrap());
    b_runs.lines_until(" got ");
    let until = |needle: &str| loop {
        let line = b_err.recv_timeout(Duration::from_secs(10)).expect(needle);
        if line.contains(needle) {
            break;
        }
    };
    let write = rusqlite::Connection::open(b.db()).unwrap();
    write.execute_batch("BEGIN IMMEDIATE").unwrap();

    a.ok(&["occur", "walk"]);
    until("gave way to a write");
    assert!(!b.has("walk"));

    write.execute_batch("COMMIT").unwrap();
    eventually("a's walk reaching b", || b.has("walk"));
}

/// A write that takes `sync run`'s file in the middle of a session it
/// serves ends the session as one given way to, as at its handshake
/// (da-2l0ha): the dialer is told it was busy and records no failure.
#[test]
fn a_write_mid_session_is_given_way_to_and_fails_nobody() {
    let (mut a, b) = paired();
    // A second an op before a applies it: the write takes a's file
    // meanwhile, past the handshake.
    a.env = vec![("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1000")];
    let mut a_runs = a.serve(&["sync", "run"]);
    let err = lines(a_runs.child.stderr.take().unwrap());
    let until = |needle: &str| loop {
        let line = err.recv_timeout(Duration::from_secs(10)).expect(needle);
        if line.contains(needle) {
            break;
        }
    };
    until("is up");
    b.ok(&["occur", "walk"]);
    let syncing = b.cmd(&["sync"]).spawn().unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let write = rusqlite::Connection::open(a.db()).unwrap();
    write.execute_batch("BEGIN IMMEDIATE").unwrap();
    until("gave way to a write");
    let out = syncing.wait_with_output().unwrap();
    let said = b.query("SELECT COUNT(*) FROM sync_peers WHERE last_error IS NOT NULL");
    assert_eq!(said, "0", "{}", String::from_utf8_lossy(&out.stdout));

    write.execute_batch("COMMIT").unwrap();
    b.ok(&["sync"]);
    assert!(a.has("walk"));
}

/// And one that takes the dialer's file: it gives way, telling the copy it
/// dialed, which records no failure either (da-2l0ha).
#[test]
fn a_dial_a_write_cuts_short_tells_its_peer_it_was_busy() {
    let (a, mut b) = paired();
    let _a_runs = a.serve(&["sync", "run"]);
    a.ok(&["occur", "walk"]);
    b.env = vec![("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1000")];
    let syncing = b.cmd(&["sync"]).spawn().unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let write = rusqlite::Connection::open(b.db()).unwrap();
    write.execute_batch("BEGIN IMMEDIATE").unwrap();
    let out = syncing.wait_with_output().unwrap();
    write.execute_batch("COMMIT").unwrap();
    let said = a.query("SELECT COUNT(*) FROM sync_peers WHERE last_error IS NOT NULL");
    assert_eq!(
        said,
        "0",
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        a.ok(&["sync", "status"])
    );
    b.ok(&["sync"]);
    assert!(b.has("walk"));
}

/// A session's verify hashes on a connection of its own, which waits for a
/// write as the session's does: `sync run`'s quarter second, not 5 s
/// (da-2l0ha), so a write that takes the file under it holds it no longer.
#[test]
fn a_verify_under_a_write_waits_as_its_session_does() {
    let (mut a, b) = paired();
    // a's holds of the write lock said, and a second before it hashes.
    a.env = vec![
        ("DATUM_SYNC_LONG_HOLD_MS", "0"),
        ("DATUM_SYNC_TEST_VERIFY_DELAY_S", "1"),
    ];
    let mut a_runs = a.serve(&["sync", "run"]);
    let err = lines(a_runs.child.stderr.take().unwrap());
    let until = |needle: &str| loop {
        let line = err.recv_timeout(Duration::from_secs(10)).expect(needle);
        if line.contains(needle) {
            break;
        }
    };
    until("is up");
    // A change, so the verify has a row's hash to keep.
    b.ok(&["occur", "walk"]);
    let syncing = b.cmd(&["sync"]).spawn().unwrap();
    // Applied: the write takes a's file before its verify keeps the hash.
    until("entries");
    let write = rusqlite::Connection::open(a.db()).unwrap();
    write.execute_batch("BEGIN IMMEDIATE").unwrap();
    let start = Instant::now();
    let out = syncing.wait_with_output().unwrap();
    let took = start.elapsed();
    write.execute_batch("COMMIT").unwrap();
    let out = String::from_utf8_lossy(&out.stdout);
    assert!(out.contains("verify ok"), "{out}");
    assert!(
        took < Duration::from_secs(4),
        "b's sync ended {took:?} after the write"
    );
}

/// A member that is down is tried again after a gap that doubles with each
/// failure up to thirty times the first, every failure after the first
/// writes nothing, and the member, up again, is heard and pushed to at once
/// — not at the next try (da-u1ul1). The first gap is 200 ms here, a minute for real.
#[test]
fn an_unreachable_member_is_tried_less_often_and_heard_at_once_when_it_is_back() {
    let (mut a, b) = paired();
    a.env.extend([
        ("DATUM_SYNC_DIAL_TIMEOUT_S", "1"),
        ("DATUM_SYNC_TEST_RETRY_MS", "200"),
    ]);
    let mut a_runs = Killed(a.cmd(&["sync", "run"]).spawn().unwrap());
    let (err, out) = (
        lines(a_runs.0.stderr.take().unwrap()),
        lines(a_runs.0.stdout.take().unwrap()),
    );
    let mut gaps = Vec::new();
    let mut first = None;
    while gaps.len() < 6 {
        let line = err
            .recv_timeout(Duration::from_secs(10))
            .expect("a failure");
        if let Some((_, gap)) = line.split_once("; next try in ") {
            gaps.push(gap.to_string());
            first.get_or_insert_with(|| held(&a));
        }
    }
    assert_eq!(gaps, ["200ms", "400ms", "800ms", "1.6s", "3.2s", "6s"]);
    assert_eq!(Some(held(&a)), first, "a failure alike wrote");

    let start = Instant::now();
    let _b_runs = b.serve(&["sync", "run"]);
    let heard = out
        .recv_timeout(Duration::from_secs(10))
        .expect("b's session");
    assert!(heard.contains(": sent "), "{heard}");
    // And pushes to it again at once, its gap of 6 s forgotten.
    a.ok(&["occur", "tea"]);
    eventually("a's write reaching b", || b.has("tea"));
    let took = start.elapsed();
    assert!(took < Duration::from_secs(4), "a reached b after {took:?}");
}

/// `sync run` logs a hold of its write lock past `DATUM_SYNC_LONG_HOLD_MS`
/// (a second unless set) with what it wrote (da-hyem1): here every hold,
/// and the apply of b's walk among them.
#[test]
fn run_logs_its_long_holds_of_the_write_lock_with_what_they_wrote() {
    let (a, b) = paired();
    let mut a_runs = Serving::of(a.cmd(&["sync", "run"]).env("DATUM_SYNC_LONG_HOLD_MS", "0"));
    let err = BufReader::new(a_runs.child.stderr.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        err.lines()
            .map_while(Result::ok)
            .try_for_each(|l| tx.send(l))
    });
    let until = |needle: &str| loop {
        let line = rx.recv_timeout(Duration::from_secs(10)).expect(needle);
        if line.contains(needle) {
            break line;
        }
    };
    until("is up");
    b.ok(&["occur", "walk"]);
    b.ok(&["sync"]);
    let held = until("entries");
    assert!(held.contains(" held the write lock 0."), "{held}");
}
