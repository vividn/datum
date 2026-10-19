//! An op the chain refuses: a sync notice, not an integrity warning
//! (da-0c2ne).

use super::*;

/// A refusal is a sync notice, not an integrity warning (da-0c2ne): one per
/// endpoint however many of its ops, in `check sync` and not `check
/// integrity`, gone once the chain admits the endpoint, with no ack.
#[test]
fn a_non_members_ops_are_refused_until_it_is_admitted() {
    let a = Copy::new();
    a.ok(&["sync", "new-invite"]);
    let stranger = Copy::new();
    stranger.ok(&["occur", "intrusion"]);
    stranger.ok(&["occur", "intrusion", "-q"]);

    let (ok, text) = stranger.send(&a);
    assert!(!ok, "{text}");
    assert!(text.contains("not a member of the sync chain"), "{text}");
    assert!(!a.has("intrusion"));
    let refused = |c: &Copy| {
        c.query("SELECT COUNT(*) FROM sync_notices WHERE kind = 'refused' AND acked = 0")
    };
    assert_eq!(refused(&a), "1");
    assert_eq!(a.query("SELECT COUNT(*) FROM integrity_warnings"), "0");
    let integrity = a.ok(&["check", "integrity"]);
    assert!(integrity.contains("No consistency issues"), "{integrity}");
    let sync = a.ok(&["check", "sync"]);
    assert!(sync.contains("sync_refused"), "{sync}");
    assert!(sync.contains(&stranger.id()[..18]), "{sync}");

    // Held, they count as received: not asked for again (the pipe's one
    // overlap op aside), and no second notice.
    let (_, again) = stranger.send(&a);
    assert!(again.contains("applied 0 · already had 1"), "{again}");
    assert_eq!(refused(&a), "1");

    // Admitted, what it had sent applies without being sent again, and
    // nothing is left to acknowledge.
    admit(&a, &stranger, "s", None);
    assert!(a.has("intrusion"));
    assert_eq!(
        a.query("SELECT COUNT(*) FROM sync_notices WHERE kind = 'refused'"),
        "0"
    );
    assert!(!a.ok(&["check"]).contains("sync_refused"));
}

/// A refused device is named by the host it says it is, and `sync chain
/// ack` takes that label, as it takes a member's (da-0c2ne).
#[test]
fn a_refusal_is_acknowledged_by_its_label() {
    let a = Copy::new();
    a.ok(&["sync", "new-invite"]);
    let stranger = Copy::new();
    stranger.ok(&["occur", "intrusion"]);
    assert!(!stranger.send(&a).0);
    let label = a.query("SELECT label FROM sync_notices WHERE kind = 'refused'");
    assert!(!stranger.id().starts_with(&label), "{label}");
    a.ok(&["sync", "chain", "ack", &label]);
    assert_eq!(
        a.query("SELECT acked FROM sync_notices WHERE kind = 'refused'"),
        "1"
    );
    assert!(!a.ok(&["check", "sync"]).contains("sync_refused"));
}

/// A database from before da-0c2ne kept its refusals in
/// `integrity_warnings`: upgraded, each endpoint's is its `refused` notice,
/// pending or acked as the row was, and integrity holds none.
#[test]
fn the_upgrade_moves_refusals_into_sync_notices() {
    let a = Copy::new();
    a.ok(&["sync", "new-invite"]);
    let row = |id: &str, ep: char, resolution: &str| {
        let ep = ep.to_string().repeat(64);
        format!(
            "('{id}', '{id}', 'intrusion', '{{\"extra_metadata\":{{\"sync_op\":{{\
             \"hlc\":\"2026-10-01T08:00:00.000Z/0000\",\"op\":\"{ep}/1\",\
             \"refused\":\"not a member of the sync chain\"}}}}}}',
             '2026-10-01T09:00:00Z', 'sync_pull', 'sync_unauthorised', {resolution})"
        )
    };
    a.exec(&format!(
        "INSERT INTO integrity_warnings
             (id, hid, entry_slug, local_entry, detected_at, detected_by, kind, resolution)
         VALUES {}, {};
         UPDATE _meta SET value = '66' WHERE key = 'schema_version';",
        row("w1", 'a', "NULL"),
        row("w2", 'b', "'keep_both'")
    ));
    let sync = a.ok(&["check", "sync"]);
    assert!(sync.contains("sync_refused (x1)"), "{sync}");
    assert!(sync.contains("aaaaaaaaaa refused"), "{sync}");
    let integrity = a.ok(&["check", "integrity"]);
    assert!(integrity.contains("No consistency issues"), "{integrity}");
    assert_eq!(a.query("SELECT COUNT(*) FROM integrity_warnings"), "0");
    assert_eq!(
        a.query(
            "SELECT group_concat(subject || ' ' || acked || ' ' || at, ', ')
             FROM sync_notices WHERE kind = 'refused'"
        ),
        format!(
            "refused:{} 0 2026-10-01T08:00:00.000Z, refused:{} 1 2026-10-01T08:00:00.000Z",
            "a".repeat(64),
            "b".repeat(64)
        )
    );
}
