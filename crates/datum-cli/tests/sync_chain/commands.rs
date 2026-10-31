//! The command tree: what is hidden, what is gone, what bare `sync` says, and
//! `pause`.

use super::*;

#[test]
fn the_network_workers_commands_are_hidden_and_endpoint_key_is_the_secret() {
    let a = Copy::new();
    let key = a.ok(&["sync", "endpoint-key"]);
    assert_eq!(
        key.trim(),
        a.query("SELECT endpoint_secret FROM sync_local")
    );
    let help = format!("{}{}", a.ok(&["sync", "--help"]), a.ok(&["help", "sync"]));
    assert!(help.contains("rename") && help.contains("status"), "{help}");
    for hidden in [
        "endpoint-key",
        "datum sync holds",
        "record-session",
        "handshake",
        "new-invite",
        "sync pipe",
        "datum sync run",
    ] {
        assert!(!help.contains(hidden), "{hidden} in help");
    }
    let found = a.ok(&["__complete", "2", "--", "datum", "sync", ""]);
    assert!(
        found.contains("status") && found.contains("chain"),
        "{found}"
    );
    for hidden in [
        "endpoint-key",
        "holds",
        "record-session",
        "handshake",
        "new-invite",
        "pipe",
        "run",
    ] {
        assert!(!found.contains(hidden), "{hidden}: {found}");
    }
}

/// The tree regrouped (da-aam46), after device-to-device pairing, `init`
/// and `add-endpoint` went (da-6177s): every old spelling is gone from
/// help, from completion, and as a command; the groups complete, and the
/// hidden commands do not.
#[test]
fn the_old_spellings_are_gone_and_the_groups_complete() {
    let a = Copy::new();
    let help = format!("{}{}", a.ok(&["sync", "--help"]), a.ok(&["help", "sync"]));
    let complete = |words: &[&str]| {
        let at = (words.len() + 2).to_string();
        let argv = [
            &["__complete", &at, "--", "datum", "sync"][..],
            words,
            &[""],
        ]
        .concat();
        let found = a.ok(&argv);
        found
            .split_whitespace()
            .map(String::from)
            .collect::<Vec<_>>()
    };
    let top = complete(&[]);
    for gone in [
        "init",
        "add-endpoint",
        "add-client",
        "peers",
        "forget",
        "now",
        "invite",
        "connect",
        "members",
        "rename",
        "ack",
        "revoke",
        "add-admin",
        "remove-admin",
        "rekey",
        "vector",
        "ops",
        "apply",
        "verify",
    ] {
        assert!(!help.contains(&format!("sync {gone}")), "{gone} in help");
        let out = a.fails(&["sync", gone, "x"], "");
        assert!(out.contains("unknown sync subcommand"), "{gone}: {out}");
    }
    assert_eq!(
        top,
        ["status", "grid", "pause", "resume", "diff", "refold", "chain", "admin", "daemon"]
    );
    assert_eq!(
        complete(&["chain"]),
        [
            "invite", "join", "leave", "list", "rename", "expel", "unjoin", "wipe", "purge",
            "role", "ack", "rekey"
        ]
    );
    // `revoke` and `revert` were renamed `expel` and `unjoin`, and cut
    // (da-n0uj1.1): each is unknown, naming the verbs there are.
    // `chain copy` is `chain invite --like`, and cut too (da-pvax0).
    let copy = a.fails(&["sync", "chain", "copy", "--from", "x"], "");
    assert!(copy.contains("unknown sync chain subcommand"), "{copy}");
    // `grant` is `chain invite <DEVICE>` or `--like`, and cut too (da-p76eu).
    let grant = a.fails(&["sync", "grant", "x", "member"], "");
    assert!(grant.contains("unknown sync subcommand"), "{grant}");
    // `connect` is `join`, and cut too (da-ka4iw).
    let connect = a.fails(&["sync", "chain", "connect", "x"], "");
    assert!(
        connect.contains("unknown sync chain subcommand"),
        "{connect}"
    );
    for gone in ["revoke", "revert"] {
        let out = a.fails(&["sync", "chain", gone, "x"], "");
        assert!(
            out.contains("unknown sync chain subcommand")
                && out.contains("expel")
                && out.contains("unjoin"),
            "{gone}: {out}"
        );
    }
    let purge = a.fails(&["sync", "chain", "purge", "x"], "");
    assert!(purge.contains("`sync chain unjoin <WHO>`"), "{purge}");
    assert_eq!(complete(&["admin"]), ["list", "add", "remove"]);
    assert_eq!(complete(&["daemon"]), ["install", "uninstall", "status"]);
    // `datum sync help` is the page.
    assert_eq!(a.ok(&["sync", "help"]), a.ok(&["help", "sync"]));
}

/// Bare `sync` in no chain says how to join one, and is no failure.
#[test]
fn bare_sync_in_no_chain_says_how_to_start_one() {
    let out = Copy::new().ok(&["sync"]);
    assert!(
        out.contains("`datum sync chain invite` on this device")
            && out.contains("`datum sync chain join <TICKET>`"),
        "{out}"
    );
}

/// `datum sync pause` (da-wlbyx): the copy neither sends nor receives — its
/// pipe and its handshake refuse, and bare `sync` names when and the fix —
/// `status` leads with it, and only this copy is paused. `resume` lets the
/// same ops through.
#[test]
fn pause_stops_the_pipe_and_resume_lets_it_through() {
    let (a, b) = chained();
    let b_id = b.id();
    a.ok(&["occur", "tea"]);
    assert!(a.ok(&["sync", "pause"]).contains("sync paused"));
    let at = a.query("SELECT paused_at FROM sync_local");
    let refusal = format!("sync is paused on this copy since {at}");
    for why in [
        a.fails(&["sync", "pipe", "ops"], ""),
        b.send(&a).1,
        a.fails(
            &["sync", "handshake", &b_id, "--label", "b", "--addr", "{}"],
            "",
        ),
    ] {
        assert!(
            why.contains(&refusal) && why.contains("`datum sync resume`"),
            "{why}"
        );
    }
    let status = a.ok(&["sync", "status"]);
    let named = a.fails(&["sync"], "");
    assert!(
        named.contains("sync is paused on this copy since "),
        "{named}"
    );
    assert!(status.starts_with("⚠ paused since "), "{status}");
    assert_eq!(
        json(&a.ok(&["sync", "status", "--json"]))["paused"],
        at.as_str()
    );
    assert!(json(&b.ok(&["sync", "status", "--json"]))["paused"].is_null());
    assert!(a.ok(&["sync", "pause"]).contains("already paused, since "));

    assert!(a.ok(&["sync", "resume"]).contains("sync resumed"));
    assert!(a.send(&b).0);
    assert!(b.has("tea"));
    assert!(!a.ok(&["sync", "status"]).contains("paused"));
}
