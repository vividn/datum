//! A time window (da-nflkk): a filtered copy sent the entries of the last
//! while, the one that set each field's state at its start, and with
//! `--specs` every spec and alias.

use chrono::{DateTime, Duration, DurationRound, Utc};

use super::*;

/// `copy` redeeming `member`'s invite with `filter`'s flags, and fed what a
/// session would send it.
fn admit_with(member: &Copy, copy: &Copy, filter: &[&str]) {
    let invite = json(&member.ok(&[&["sync", "new-invite"][..], filter].concat()));
    let secret = invite["secret"].as_str().unwrap();
    let id = copy.id();
    let hello = ["sync", "handshake", &id, "--label", "r", "--addr", "{}"];
    member.ok(&[&hello[..], &["--invite", secret, "--chain", ""]].concat());
    let (ok, text) = feed(member, copy);
    assert!(ok, "{text}");
}

/// `args` on `copy` with its window judged at `now`.
fn at(copy: &Copy, now: DateTime<Utc>, args: &[&str], stdin: &str) -> String {
    let mut child = copy
        .cmd(args)
        .env("DATUM_SYNC_TEST_NOW", now.to_rfc3339())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// [`feed`] with both sides judging at `now`, and the verify both compute.
fn feed_at(from: &Copy, to: &Copy, now: DateTime<Utc>) -> (String, String) {
    for _ in 0..3 {
        let vector = to.ok(&["sync", "pipe", "vector"]);
        let since = [
            "sync",
            "pipe",
            "ops",
            "--since",
            vector.trim(),
            "--peer",
            &to.id(),
        ];
        let ops = at(from, now, &since, "");
        at(to, now, &["sync", "pipe", "apply"], &ops);
    }
    let verify =
        |c: &Copy, p: &Copy| at(c, now, &["sync", "pipe", "verify", "--peer", &p.id()], "");
    (verify(from, to), verify(to, from))
}

/// The hid of `copy`'s entry of `field` at the `n`th oldest time.
fn nth(copy: &Copy, field: &str, n: usize) -> String {
    copy.query(&format!(
        "SELECT hid FROM entries WHERE field = '{field}' ORDER BY at LIMIT 1 OFFSET {n}"
    ))
}

fn holds(copy: &Copy, hid: &str) -> bool {
    copy.query(&format!(
        "SELECT (SELECT COUNT(*) FROM entries WHERE hid = '{hid}')
              + (SELECT COUNT(*) FROM ops WHERE hid = '{hid}')"
    )) != "0"
}

/// A copy with a window of a day holds the last day's entries, every spec
/// and alias, and for a field last switched three days ago that switch, so
/// its state reads right; nothing else is served to it, and a day later the
/// day's oldest entries leave it, its verify agreeing throughout.
#[test]
fn a_day_window_holds_the_day_every_spec_and_alias_and_each_fields_state_at_its_start() {
    let (a, _b) = chained();
    let hour = Utc::now().duration_trunc(Duration::hours(1)).unwrap();
    a.ok(&["switch", "mood", "happy", "-t", "-4320"]);
    // coffee's first is what made it tracked: its state at the start.
    for ago in ["-4400", "-2880", "-600"] {
        a.ok(&["occur", "coffee", "-t", ago]);
    }
    a.ok(&["occur", "coffee"]);
    a.ok(&["setup", "tea", "--verb", "occur"]);
    a.ok(&["alias", "set", "cup", "occur coffee"]);
    let old = nth(&a, "coffee", 1);
    let today = nth(&a, "coffee", 2);
    let r = Copy::new();
    admit_with(&a, &r, &["--window", "1d", "--specs"]);

    assert!(!holds(&r, &old), "two days old, it is outside the window");
    for hid in [
        nth(&a, "mood", 0),
        nth(&a, "coffee", 0),
        today.clone(),
        nth(&a, "coffee", 3),
    ] {
        assert!(holds(&r, &hid), "{hid}");
    }
    assert_eq!(
        r.query("SELECT state FROM entries WHERE field = 'mood'"),
        "\"happy\""
    );
    assert!(r.query("SELECT COUNT(*) FROM entries WHERE slug = 'spec:tea'") != "0");
    assert_eq!(
        r.query("SELECT COUNT(*) FROM entries WHERE field = 'alias'"),
        "1"
    );
    // What it is served from empty is what it holds.
    let served = a.ok(&["sync", "pipe", "ops", "--since", "{}", "--peer", &r.id()]);
    let hids: Vec<String> = served
        .lines()
        .filter(|l| l.starts_with('['))
        .flat_map(|l| json(l).as_array().unwrap().clone())
        .map(|o| o["hid"].as_str().unwrap().to_string())
        .collect();
    assert!(!hids.is_empty() && !hids.contains(&old), "{served}");
    assert!(hids.iter().all(|h| holds(&r, h)), "{served}");
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
    let status = r.ok(&["sync", "status"]);
    assert!(
        status.contains("a reader, filtered: every field, the last 1d, every spec and alias"),
        "{status}"
    );

    // A day later, ten hours ago is outside it, with nothing new sent.
    let (theirs, mine) = feed_at(&a, &r, hour + Duration::days(1));
    assert!(!holds(&r, &today), "aged out");
    assert!(holds(&r, &nth(&a, "coffee", 3)) && holds(&r, &nth(&a, "mood", 0)));
    assert!(r.query("SELECT COUNT(*) FROM entries WHERE slug = 'spec:tea'") != "0");
    assert_eq!(theirs, mine);
}

/// Deleting the entry that set a field's state at the window's start makes
/// the one before it that entry, which is sent whole, so the state still
/// reads right.
#[test]
fn the_entry_before_a_deleted_one_that_set_the_state_is_sent() {
    let (a, _b) = chained();
    a.ok(&["switch", "mood", "happy", "-t", "-4320"]);
    a.ok(&["switch", "mood", "sad", "-t", "-2880"]);
    let (happy, sad) = (nth(&a, "mood", 0), nth(&a, "mood", 1));
    let r = Copy::new();
    admit_with(&a, &r, &["--window", "1d", "--fields", "mood"]);
    assert!(holds(&r, &sad) && !holds(&r, &happy));

    a.ok(&["delete", &sad]);
    let (ok, text) = feed(&a, &r);
    assert!(ok, "{text}");
    assert!(holds(&r, &happy));
    let live = "SELECT state FROM entries WHERE field = 'mood' AND deleted_at IS NULL";
    assert_eq!(r.query(live), "\"happy\"");
    assert_eq!(verify_with(&a, &r), verify_with(&r, &a));
}
