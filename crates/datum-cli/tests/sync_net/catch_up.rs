//! A big catch-up, a round at a time (da-ymgiv).

use super::*;

/// The ops a round carries at most: the wire crate's `ROUND`.
const ROUND: usize = 8192;

/// The ops a copy's log says it sent `to` (a label, or part of one): «sent
/// N» of each session it finished, and «and sent N» of each one lost.
fn sent_in(log: &str, to: &str) -> usize {
    let mut n = 0;
    for line in log.lines().filter(|l| l.contains(to)) {
        for (i, _) in line.match_indices("sent ") {
            let before = &line[..i];
            if before.ends_with(": ") || before.ends_with("and ") {
                let digits: String = line[i + 5..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                n += digits.parse::<usize>().unwrap_or(0);
            }
        }
    }
    n
}

/// The sessions with `to` a copy's log says sent it something, finished or
/// lost: each line [`sent_in`] reads over 0.
fn sessions_in(log: &str, to: &str) -> usize {
    log.lines().filter(|l| sent_in(l, to) > 0).count()
}

/// What `senders`' logs say of `to`, and all of `copy`'s.
fn about(senders: &[&Copy], to: &str, copy: &Copy) -> String {
    let mut all: String = senders
        .iter()
        .flat_map(|c| {
            log(c, "run.log")
                .lines()
                .filter(|l| l.contains(to))
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .collect();
    all += &log(copy, "run.log");
    all
}

/// Once `said` has not changed for 20s: every session it logs has ended,
/// where one a slow copy's apply gate holds says nothing until it does.
fn settled(said: impl Fn() -> String) {
    let (mut was, mut since) = (said(), Instant::now());
    while since.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_secs(1));
        let now = said();
        if now != was {
            (was, since) = (now, Instant::now());
        }
    }
}

fn ops(copy: &Copy) -> usize {
    copy.query("SELECT COUNT(*) FROM ops WHERE applied > 0")
        .parse()
        .unwrap()
}

/// vividn's purge and re-import on a chain of four (da-ymgiv): each copy runs
/// `sync run`, 20,000 entries are imported on a, and d, a slow computer,
/// applies at 1ms an op and loses its sessions every 15s, its `sync run`
/// killed and started again. A round carries at most 1,000 ops here and
/// applies before the next, so a lost session wastes at most a round from
/// each sender, where it wasted all it had been sent and not yet applied
/// (staging: 80,535 sent for 20,003, 7 sessions lost); and no copy dials a
/// peer it is in a session with. Two members in a session with d at once
/// read its vector before either applies, and send it the same round: d
/// ends the session that brought it what it held (da-3l8ay). So a session
/// that sent d anything wastes at most a round: the one lost with it, or the
/// one d answered with its end. Counted once every session with d has
/// ended, not as the total first reaches what d lacked (da-3l8ay). With
/// rounds uncapped, the staging shape, 64,333.
#[test]
fn a_big_import_reaches_a_slow_copy_without_sending_it_many_times() {
    let (mut a, mut b, mut c, mut d) = (Copy::new(), Copy::new(), Copy::new(), Copy::new());
    for joiner in [&b, &c, &d] {
        let (mut inviting, ticket) = a.invite(&[]);
        joiner.ok(&["sync", "chain", "join", &ticket]);
        assert!(inviting.finish().0);
    }
    for copy in [&mut a, &mut b, &mut c, &mut d] {
        copy.env.push(("DATUM_SYNC_TEST_ROUND", "1000"));
    }
    d.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1"));
    let runs: Vec<Logged> = [&a, &b, &c]
        .into_iter()
        .map(|copy| logged(copy, &["sync", "run"], "run.log"))
        .collect();
    let mut slow = logged(&d, &["sync", "run"], "run.log");
    // Each has dialed the others it starts with.
    until("the copies settle", Duration::from_secs(60), || {
        [&a, &b, &c, &d]
            .iter()
            .all(|c| log(c, "run.log").matches("verify ok").count() >= 3)
    });
    let before = ops(&d);
    import(&a, 20_000);
    // A write moves a's vector, and a pushes: its serve signs the baselines.
    a.ok(&["occur", "tea"]);
    until("a signs its baselines", Duration::from_secs(60), || {
        ops(&a) > 20_000
    });
    let all = ops(&a);
    let (start, mut lost) = (Instant::now(), 0);
    while ops(&d) < all {
        assert!(
            start.elapsed() < Duration::from_secs(600),
            "d holds every op within 600s: {} of {all}",
            ops(&d)
        );
        if start.elapsed() > Duration::from_secs(15 * (lost + 1)) {
            drop(slow);
            slow = logged(&d, &["sync", "run"], "run.log");
            lost += 1;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let needed = all - before;
    // What a, b and c say they sent d, once their sessions with it end.
    let to = format!("u{}@", d.port);
    let sent = || -> Vec<usize> {
        [&a, &b, &c]
            .iter()
            .map(|c| sent_in(&log(c, "run.log"), &to))
            .collect()
    };
    // Every session with d has ended once nothing about d has been logged
    // for longer than d takes to apply the rounds queued at its gate.
    settled(|| about(&[&a, &b, &c], &to, &d));
    let seen = sent();
    let total: usize = seen.iter().sum();
    // The sessions each of a, b and c says sent d something: finished, ended
    // by d, cut by a kill, or timed out on a loaded box (da-uakw5).
    let sessions: Vec<usize> = [&a, &b, &c]
        .iter()
        .map(|c| sessions_in(&log(c, "run.log"), &to))
        .collect();
    eprintln!(
        "d lacked {needed} ops, was cut {lost} times, had sessions that sent it something with a {}, b {}, c {} and was sent {total} (a {}, b {}, c {})",
        sessions[0], sessions[1], sessions[2], seen[0], seen[1], seen[2]
    );
    assert!(lost > 0, "d's sessions were never cut");
    // At most a round wasted per session: staging's 80,535 is well over it,
    // and so, with no end, is two members sending d the same catch-up.
    let bound = needed + sessions.iter().sum::<usize>() * 1000;
    assert!(
        total <= bound,
        "d lacked {needed} ops and was sent {total}, over {bound}"
    );
    // And every copy converges.
    let start = Instant::now();
    for (n, copy) in [("b", &b), ("c", &c)] {
        while ops(copy) < all {
            if start.elapsed() > Duration::from_secs(180) {
                eprintln!("{n}'s log:\n{}", log(copy, "run.log"));
                eprintln!("a's log:\n{}", log(&a, "run.log"));
                panic!("{n} holds every op within 180s: {} of {all}", ops(copy));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    eprintln!(
        "every copy holds every op {}s after d",
        start.elapsed().as_secs()
    );
    drop((runs, slow));
    for copy in [&b, &c, &d] {
        assert_eq!(copy.hash(), a.hash());
    }
}

/// Two members in a session with one copy at once read its vector before
/// either applies, and send it the same round (da-3l8ay): d, which lacks an
/// import a and b both hold, ends each session that brought it a round it
/// held, so it is sent the import about once, where it was sent it twice.
#[test]
fn two_members_catching_a_copy_up_at_once_send_it_the_catch_up_once() {
    let (mut a, mut b, mut d) = (Copy::new(), Copy::new(), Copy::new());
    for joiner in [&b, &d] {
        let (mut inviting, ticket) = a.invite(&[]);
        joiner.ok(&["sync", "chain", "join", &ticket]);
        assert!(inviting.finish().0);
    }
    for copy in [&mut a, &mut b, &mut d] {
        copy.env.push(("DATUM_SYNC_TEST_ROUND", "500"));
    }
    d.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1"));
    import(&a, 5000);
    // Its baselines signed now, and b holds them before d is up.
    a.ok(&["sync", "pipe", "ops", "--limit", "1"]);
    let all = ops(&a);
    let runs = [&a, &b].map(|c| logged(c, &["sync", "run"], "run.log"));
    until("b holds a's import", Duration::from_secs(120), || {
        ops(&b) >= all
    });
    let needed = all - ops(&d);
    let _d = logged(&d, &["sync", "run"], "run.log");
    until("d holds every op", Duration::from_secs(300), || {
        ops(&d) >= all
    });
    let to = format!("u{}@", d.port);
    settled(|| about(&[&a, &b], &to, &d));
    let [sa, sb] = [&a, &b].map(|c| sent_in(&log(c, "run.log"), &to));
    let [na, nb] = [&a, &b].map(|c| sessions_in(&log(c, "run.log"), &to));
    eprintln!(
        "d lacked {needed} ops and was sent {} (a {sa} in {na} sessions, b {sb} in {nb})",
        sa + sb
    );
    // A round wasted per session at most, the one d ended it on. Measured:
    // 7,002 in 5 sessions; with no end (DATUM_SYNC_TEST_END=0), 10,004 in 2.
    let bound = needed + (na + nb) * 500;
    assert!(
        sa + sb <= bound,
        "d lacked {needed} ops and was sent {}, over {bound}",
        sa + sb
    );
    drop(runs);
}

/// A session cut mid-catch-up (da-ymgiv) loses at most the round it was
/// cut in: b, applying at 1ms an op, is killed once a round has applied, and
/// the next session is sent only what b still lacks.
#[test]
fn a_catch_up_cut_short_resumes_from_the_last_round_that_applied() {
    let (mut a, mut b) = paired();
    import(&a, 20_000);
    // Its baselines signed now: signing 20,000 sends nothing for longer
    // than the 10s below.
    a.ok(&["sync", "pipe", "ops", "--limit", "1"]);
    let before = ops(&b);
    // a hears the cut session is lost within 10s; b, which paces its apply
    // chunks by its own timeout, has the same one, or a can hear nothing
    // from it for longer than that between chunks. Not 5s: on a loaded
    // runner a closed the resume after 6,866 ops, most likely timing out
    // on one slow chunk (da-tv8ja).
    for copy in [&mut a, &mut b] {
        copy.env.push(("DATUM_SYNC_TIMEOUT_S", "10"));
    }
    b.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1"));
    let _serving = logged(&a, &["sync", "run"], "run.log");
    let cut = logged(&b, &["sync"], "cut.log");
    until("b applies a round", Duration::from_secs(120), || {
        ops(&b) > before + ROUND
    });
    drop(cut);
    let held = ops(&b);
    // a hears the cut session is lost before b dials again.
    until(
        "a says the session is lost",
        Duration::from_secs(30),
        || log(&a, "run.log").contains("then:"),
    );
    let to = format!("u{}@", b.port);
    let first = sent_in(&log(&a, "run.log"), &to);
    b.env
        .retain(|(k, _)| *k != "DATUM_SYNC_TEST_APPLY_DELAY_MS");
    // a's log says which side gave up, should the resume fail (da-tv8ja).
    let out = b.run(&["sync"]);
    assert!(
        out.status.success(),
        "the resume failed: {}{}\na's run.log:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
        log(&a, "run.log"),
    );
    let needed = ops(&b) - before;
    let rest = sent_in(&log(&a, "run.log"), &to) - first;
    eprintln!(
        "b lacked {needed}: the cut session sent {first} and applied {}, the next sent {rest}",
        held - before
    );
    assert_eq!(
        rest,
        needed - (held - before),
        "the next session sends only what b lacks"
    );
    assert!(
        first <= held - before + ROUND,
        "a cut session loses at most a round: sent {first}, applied {}",
        held - before
    );
    assert_eq!(a.hash(), b.hash());
}

/// Two copies that dial each other at once (da-vj3ly): both `sync run`s
/// start together, each holding 2,000 entries the other lacks, and each
/// dials the other as it comes up. The lower endpoint id's session carries
/// both ways and the other copy closes its own, so each op is sent once,
/// where the two sessions sent each twice. The higher id's dial welcomed
/// before the lower id's reached it carries, and refuses that one (da-mghh3).
#[test]
fn two_copies_dialing_each_other_at_once_send_each_op_once() {
    let (mut a, mut b) = paired();
    import_as(&a, "step", 2000);
    import_as(&b, "stair", 2000);
    for copy in [&mut a, &mut b] {
        // Its baselines signed now, before the runs start.
        copy.ok(&["sync", "pipe", "ops", "--limit", "1"]);
        // Each session applies for seconds, so the two overlap.
        copy.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "1"));
    }
    let lacking = ops(&a) + ops(&b);
    let runs = [&a, &b].map(|copy| logged(copy, &["sync", "run"], "run.log"));
    until("both hold every op", Duration::from_secs(120), || {
        ops(&a) == ops(&b) && ops(&a) * 2 > lacking && a.hash() == b.hash()
    });
    let needed = 2 * ops(&a) - lacking;
    // Each has one peer: every session line is with the other.
    let sent = || sent_in(&log(&a, "run.log"), "") + sent_in(&log(&b, "run.log"), "");
    // Until every session has said what it sent, which it does as it ends:
    // the total unchanged for 6s, past either session's apply of 2,000 ops
    // at 1ms each, so a second session still open is not missed.
    let (start, mut seen, mut still) = (Instant::now(), sent(), 0);
    while still < 3 || seen < needed {
        std::thread::sleep(Duration::from_secs(2));
        let now = sent();
        still = if now == seen { still + 1 } else { 0 };
        seen = now;
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the sessions say they sent {needed} within 60s, not {seen}:\n{}\n{}",
            log(&a, "run.log"),
            log(&b, "run.log")
        );
    }
    drop(runs);
    eprintln!("the copies lacked {needed} ops between them and were sent {seen}");
    // Measured: exactly what was lacked; two sessions send twice that.
    assert!(
        seen <= needed + needed / 10,
        "the copies lacked {needed} ops and were sent {seen}:\n{}\n{}",
        log(&a, "run.log"),
        log(&b, "run.log")
    );
    assert_eq!(a.hash(), b.hash());
}

/// The first serve after a bulk import signs its baselines (da-f7mmn):
/// 10,000 here, 5 to 10s in a debug build, on a 6s timeout. a, dialing
/// b's `sync run`, signs them in chunks of the clock's size with a `BUSY`
/// after each, unless `keepalive` is off; it sent nothing until all were
/// signed, and lost b. The sync's output. Not 2s: b applies them in chunks
/// of a sixteenth of the timeout, a `BUSY` between each, and a loaded
/// runner stalled one past 2s (da-3njqh, as da-hd9sx's 3s). With the
/// keepalives off b's apply is silent too, half a minute of it, so the
/// session is lost at 6s however fast a signs.
fn first_sync_after_an_import(keepalive: bool) -> (Copy, Copy, Output) {
    let (mut a, mut b) = paired();
    import(&a, 10_000);
    for c in [&mut a, &mut b] {
        c.env = vec![
            ("DATUM_SYNC_TIMEOUT_S", "6"),
            (
                "DATUM_SYNC_TEST_KEEPALIVE",
                if keepalive { "1" } else { "0" },
            ),
        ];
    }
    let _serving = b.serve(&["sync", "run"]);
    let out = a.run(&["sync"]);
    (a, b, out)
}

#[test]
fn the_first_sync_after_an_import_keeps_its_peer_while_it_signs() {
    let (a, b, out) = first_sync_after_an_import(true);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("verify ok"), "{text}");
    assert!(ops(&b) > 10_000);
    assert_eq!(a.hash(), b.hash());
}

#[test]
fn without_keepalives_the_first_sync_after_an_import_loses_its_peer() {
    let (_a, _b, out) = first_sync_after_an_import(false);
    assert!(
        !out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
