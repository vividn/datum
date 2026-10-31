//! A sync notice: announced once, kept in `check` until acked.

use super::*;

/// The notice is stored once and shown in both `check`, under its sync
/// domain, and `sync status`, and one `ack` clears it from both (da-nq07y).
#[test]
fn a_new_endpoint_is_announced_once_and_stays_in_check_until_acked() {
    let (a, b, _) = locked();
    // a admitted b at a handshake, as any member admits a redeemer: loud,
    // since the invite may have been anyone's. Its own admin key is quiet.
    let admitted = a.ok(&["check", "sync"]);
    assert!(
        admitted.contains("sync:\n[WARN ] sync_new_member (x1)")
            && admitted.contains("⚠ another device joined: b, at ")
            && !admitted.contains("new sync admin"),
        "{admitted}"
    );
    // In plain words (da-o9xdd): by label, on the reader's clock, and what
    // revokes it should it not be theirs.
    let status = a.ok(&["sync", "status"]);
    let line = status
        .lines()
        .find(|l| l.starts_with("⚠ another device joined: b, at "))
        .unwrap_or_else(|| panic!("{status}"));
    assert!(
        line.contains("not yours? `datum sync chain expel ")
            && line.contains("to acknowledge")
            && !line.contains("via")
            && !line.contains('Z'),
        "{line}"
    );
    let report = json(&a.ok(&["check", "--show", "json"]));
    let issue = report["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["category"] == "sync_new_member")
        .unwrap();
    assert_eq!(issue["domain"], "sync");
    a.ok(&["sync", "chain", "ack", "b"]);
    assert!(!a.ok(&["check", "sync"]).contains("new to the sync chain"));
    assert!(!a.ok(&["sync", "status"]).contains("another device joined"));

    // b heard of a, then of the lock: each announced once, on the first
    // command after it arrived.
    let first = b.stderr_of(&["tail"]);
    assert!(first.contains("⚠ new sync admin"), "{first}");
    assert!(!b.stderr_of(&["tail"]).contains("new sync"));

    // a was a member before b joined: named once, plainly, with no ⚠ — not
    // news of a device joining (da-o9xdd).
    let check = b.ok(&["check", "sync"]);
    assert!(check.contains("[WARN ] sync_joined (x1)"), "{check}");
    let a_label = member_of(&b, &a.id())["label"].as_str().unwrap().to_owned();
    let joined = format!("  you joined this sync chain; its members: {a_label} (since ");
    assert!(check.contains(&joined), "{check}");
    assert!(!check.contains("another device joined"), "{check}");
    assert!(check.contains("new to the sync chain"), "{check}");
    b.ok(&["sync", "chain", "ack", "joined"]);
    assert!(!b.ok(&["check", "sync"]).contains("you joined"));
    b.ok(&["sync", "chain", "ack", "ed25519:"]);
    assert!(!b.ok(&["check", "sync"]).contains("new to the sync chain"));
    let none = b.fails(&["sync", "chain", "ack", "nobody"], "");
    assert!(none.contains("nothing to acknowledge"), "{none}");
}

/// The warning's own command, copied as is, acknowledges, and a bare `ack`
/// takes the one notice pending or lists several (da-36xnc).
#[test]
fn the_warning_names_its_ack_and_a_bare_ack_takes_the_only_notice() {
    fn acks(warned: &str) -> Vec<&str> {
        warned
            .split('`')
            .filter(|c| c.starts_with("datum sync chain ack"))
            .collect()
    }
    // The command as printed, less its `datum`.
    let run = |copy: &Copy, command: &str| copy.ok(&command.split(' ').skip(1).collect::<Vec<_>>());

    // A join: the member that admitted b is told of it, and how to say so.
    let (a, b) = (Copy::new(), Copy::new());
    assert!(redeem(&a, &b, "b", None, &["--chain", ""]).status.success());
    let joined = a.stderr_of(&["tail"]);
    assert!(joined.contains("⚠ another device joined: b"), "{joined}");
    let [command] = acks(&joined)[..] else {
        panic!("{joined}")
    };
    run(&a, command);
    assert!(!a.ok(&["check", "sync"]).contains("new to the sync chain"));

    let (_a, b, _) = locked();
    // b heard of the admin key after it joined: the one it is warned of.
    let warned = b.stderr_of(&["tail"]);
    let commands = acks(&warned);
    assert_eq!(commands.len(), 1, "{warned}");

    // With a's endpoint, two are pending: a bare ack lists both, each with
    // its command, and takes neither.
    let pending = |copy: &Copy| {
        copy.ok(&["check", "sync"])
            .matches("`datum sync chain ack ")
            .count()
    };
    let both = b.fails(&["sync", "chain", "ack"], "");
    assert!(both.contains("2 to acknowledge; name one"), "{both}");
    assert!(both.contains(&format!("`{}`", commands[0])), "{both}");
    assert_eq!(
        both.matches("— `datum sync chain ack ").count(),
        2,
        "{both}"
    );
    assert!(both.contains("or `datum sync chain ack --all`"), "{both}");
    assert_eq!(pending(&b), 2);

    run(&b, commands[0]);
    assert_eq!(pending(&b), 1);

    let one = b.ok(&["sync", "chain", "ack"]);
    assert!(
        one.starts_with("acknowledged you joined this sync chain; its members: "),
        "{one}"
    );
    assert!(both.contains(one.trim_start_matches("acknowledged ").trim()));
    assert_eq!(pending(&b), 0);
    let none = b.fails(&["sync", "chain", "ack"], "");
    assert!(none.contains("nothing to acknowledge"), "{none}");
}

/// `ack --all` acknowledges every notice pending, each on its own line, and
/// a second one says nothing is pending (da-s3h3u).
#[test]
fn ack_all_acknowledges_every_notice_and_names_each() {
    let (a, b) = chained();
    let c = Copy::new();
    admit(&a, &c, "c", None);
    assert!(a.send(&b).0);
    // d joins a chain of three: its members named in one line (da-o9xdd),
    // and a device that joins after it is news.
    let d = Copy::new();
    admit(&b, &d, "d", None);
    admit(&b, &Copy::new(), "e", None);
    assert!(b.send(&d).0);
    let check = d.ok(&["check", "sync"]);
    assert_eq!(
        check.matches("you joined this sync chain").count(),
        1,
        "{check}"
    );
    assert!(check.contains("⚠ another device joined: e, at "), "{check}");

    let acked = d.ok(&["sync", "chain", "ack", "--all"]);
    let lines: Vec<&str> = acked.lines().collect();
    assert_eq!(lines.len(), 2, "{acked}");
    assert!(lines[0].starts_with("acknowledged you joined this sync chain; its members: "));
    assert!(lines[1].starts_with("acknowledged another device joined: e, at "));
    for copy in [&a, &b, &c] {
        let label = member_of(&d, &copy.id())["label"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(lines[0].contains(&format!("{label} (since ")), "{acked}");
    }
    assert!(!d.ok(&["check", "sync"]).contains("sync chain ack"));
    assert_eq!(
        d.ok(&["sync", "chain", "ack", "--all"]).trim(),
        "nothing to acknowledge"
    );
}

/// The warning is the last thing a command prints, under its own output
/// (da-36xnc), and one a failing command did not print is the next one's.
#[test]
fn the_warning_comes_after_the_commands_own_output() {
    use std::io::{Read, Seek};
    let joined = || {
        let (a, b) = (Copy::new(), Copy::new());
        a.ok(&["occur", "tea"]);
        assert!(redeem(&a, &b, "b", None, &["--chain", ""]).status.success());
        a
    };
    const WARNING: &str = "⚠ another device joined: b";

    // Both streams into one file, in the order they were written.
    let a = joined();
    let mut both = tempfile::tempfile().unwrap();
    let status = a
        .cmd(&["tail"])
        .stdin(Stdio::null())
        .stdout(both.try_clone().unwrap())
        .stderr(both.try_clone().unwrap())
        .status()
        .unwrap();
    assert!(status.success());
    let mut printed = String::new();
    both.rewind().unwrap();
    both.read_to_string(&mut printed).unwrap();
    let (rows, warned) = printed.split_once(WARNING).expect(&printed);
    assert!(rows.contains("tea"), "{printed}");
    assert_eq!(warned.trim_end().lines().count(), 1, "{printed}");

    // Told once, by a command that fails or by the one after it.
    let a = joined();
    let failed = a.fails(&["sync", "chain", "ack", "nobody"], "");
    let after = a.stderr_of(&["tail"]);
    let told = format!("{failed}{after}");
    assert_eq!(told.matches(WARNING).count(), 1, "{told}");
}
