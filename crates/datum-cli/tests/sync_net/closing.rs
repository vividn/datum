//! A session that gives up says why as it closes, so the copy left behind
//! reads it and not «closed by peer: 0» (da-s9rwd).

use super::*;

/// The inviter gives up on a joiner stalled in its apply, which reads why.
#[test]
fn a_session_given_up_on_idle_names_that_to_the_other_copy() {
    let (_a, _b, joined, _served) = slow_first_sync(false);
    let text = String::from_utf8_lossy(&joined.stderr);
    assert!(
        text.contains("closed by peer: timed out (code 0)"),
        "{text}"
    );
}

/// On a kept connection only the stream goes, and a reset carries a code and
/// no words: the copy left stalled reads why all the same. b applies 300-odd
/// ops at 20ms each on a 3s timeout, with no keepalives. Read from its log:
/// the next session on that connection follows at once, and succeeding clears
/// the words from `sync status` within a moment (da-i5tj8).
#[test]
fn a_session_given_up_on_a_kept_connection_names_that_to_the_other_copy() {
    let (mut a, mut b) = paired();
    let quick = [
        ("DATUM_SYNC_TIMEOUT_S", "3"),
        ("DATUM_SYNC_TEST_KEEPALIVE", "0"),
    ];
    a.env = quick.to_vec();
    b.env = quick.to_vec();
    b.env.push(("DATUM_SYNC_TEST_APPLY_DELAY_MS", "20"));
    let _a_runs = a.serve(&["sync", "run"]);
    let _b_runs = logged(&b, &["sync", "run"], "run.log");
    a.ok(&["occur", "first"]);
    until("the first session", Duration::from_secs(60), || {
        a.hash() == b.hash()
    });
    import(&a, 300);
    until("b reads why", Duration::from_secs(60), || {
        log(&b, "run.log").contains("the other copy timed out")
    });
}
