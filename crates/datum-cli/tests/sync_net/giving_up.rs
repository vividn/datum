//! A dial a person waits on gives up on an offline member in the short limit,
//! a session connected and a run's own rounds not (da-75rbi).

use super::*;

/// a, b serving, and c, a member that is off. Nothing here sets a dial's
/// limit: what these time is the one a person gets (da-75rbi).
fn one_up_one_off() -> (Copy, Copy, Serving, String) {
    let (a, b, off) = one_off();
    let b_runs = b.serve(&["sync", "run"]);
    (a, b, b_runs, off)
}

/// a and b, not yet running, and the label of c, a member that is off.
fn one_off() -> (Copy, Copy, String) {
    let (a, b) = paired();
    let c = Copy::new();
    let (mut inviting, ticket) = a.invite(&[]);
    c.ok(&["sync", "chain", "join", &ticket]);
    assert!(inviting.finish().0);
    let off = member_label(&a, &c.id());
    (a, b, off)
}

/// Five seconds, and a margin for a loaded runner; the 60 s it was is far off.
const GIVES_UP_BY: Duration = Duration::from_secs(15);

/// What `datum sync` said, offline beside a live member, in how long.
fn sync_with_one_off(a: &Copy, off: &str) -> (String, Duration) {
    let start = Instant::now();
    let out = a.run(&["sync"]);
    let took = start.elapsed();
    let said = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "the live one synced: {said}");
    assert!(said.contains("verify ok"), "{said}");
    assert!(said.contains(&format!("{off}: offline")), "{said}");
    (said, took)
}

/// An offline member is given up on in the short limit, the live one beside
/// it synced, the exit what it was.
#[test]
fn sync_gives_up_on_an_offline_member_quickly() {
    let (a, _b, _b_runs, off) = one_up_one_off();
    let (said, took) = sync_with_one_off(&a, &off);
    assert!(took < GIVES_UP_BY, "{took:?}: {said}");
}

/// The same through a run, a person waiting on its answer: a member never
/// reached, and one gone since with the connection kept to it.
#[test]
fn an_ask_of_the_run_gives_up_on_an_offline_member_quickly() {
    let (a, _b, mut b_runs, off) = one_up_one_off();
    let _a_runs = a.serve(&["sync", "run"]);
    let socket = PathBuf::from(format!("{}.sync", a.db().display()));
    eventually("a's run taking asks", || socket.exists());
    let (said, took) = sync_with_one_off(&a, &off);
    assert!(took < GIVES_UP_BY, "{took:?}: {said}");

    b_runs.child.kill().unwrap();
    b_runs.child.wait().unwrap();
    let start = Instant::now();
    let said = a.fails(&["sync"], "");
    let took = start.elapsed();
    assert!(said.contains("none of 2 peer(s) synced"), "{said}");
    assert!(took < GIVES_UP_BY, "{took:?}: {said}");
}

/// An ask while the run applies another session still dials each member, so
/// one that is off reads offline, not this copy busy (da-ahivt). a's dials at
/// start reach no run and back off, so what a applies, slowly, is b's run's.
#[test]
fn an_ask_while_the_run_applies_still_finds_a_member_offline() {
    let (mut a, b, off) = one_off();
    import_as(&b, "stair", 300);
    b.ok(&["sync", "pipe", "ops", "--limit", "1"]);
    a.env.extend([
        ("DATUM_SYNC_TIMEOUT_S", "2"),
        ("DATUM_SYNC_TEST_APPLY_DELAY_MS", "10"),
    ]);
    let _a_runs = logged(&a, &["sync", "run"], "run.log");
    until(
        "a's dials at start failing",
        Duration::from_secs(20),
        || log(&a, "run.log").matches("next try in").count() >= 2,
    );
    let _b_runs = b.serve(&["sync", "run"]);
    until("a applying b's dial", Duration::from_secs(20), || {
        a.has("stair")
    });
    sync_with_one_off(&a, &off);
}

/// A run's own round is no person waiting: its dial keeps the longer limit,
/// here 8 s by the idle limit it follows, past the 5 an ask has.
#[test]
fn a_runs_own_round_keeps_its_longer_limit() {
    let (mut a, _b) = paired();
    a.env.push(("DATUM_SYNC_TIMEOUT_S", "8"));
    let start = Instant::now();
    let _a_runs = logged(&a, &["sync", "run"], "run.log");
    until("its dial failing", Duration::from_secs(30), || {
        log(&a, "run.log").contains("next try in")
    });
    assert!(
        start.elapsed() >= Duration::from_secs(8),
        "{:?}",
        start.elapsed()
    );
}

/// The limit is the dial's: a session connected runs past it, here by the
/// member taking 6 s over its verify.
#[test]
fn a_session_past_the_short_limit_is_not_cut() {
    let (mut a, b) = paired();
    a.env.push(("DATUM_SYNC_TEST_VERIFY_DELAY_S", "6"));
    let _a_runs = a.serve(&["sync", "run"]);
    b.ok(&["occur", "walk"]);
    let start = Instant::now();
    let said = b.ok(&["sync"]);
    assert!(said.contains("verify ok"), "{said}");
    assert!(
        start.elapsed() >= Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    assert!(a.has("walk"));
}
