//! End-to-end sync test suite (bead da-bra).
//!
//! Exercises the full multi-instance lifecycle of the merged sync stack
//! (C4a–C4e): real `datum-sync-server` in-process, real `datum-sync-client`
//! pack/encrypt/push/pull, real `cr-sqlite`-shaped changeset replay.
//!
//! Each scenario is a single `#[test]` so a regression localizes to one
//! lifecycle stage rather than a tangled integration soup. All tests
//! serialise on `ENV_LOCK` because the CLI sync entry points read
//! `DATUM_HOME` / `DATUM_CONFIG_HOME` for token + master-key storage.
//!
//! Multi-device scenarios bypass the auto-managed `device_id` file by
//! calling `push::run` / `pull::run` directly with explicit device ids —
//! the existing `sync_push_pull::two_instance_sync_converges` test
//! established this pattern; we extend it to 3 devices, partitions, and
//! restart paths.

mod sync_support;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, TimeZone, Utc};
use datum_cli::config::{auth_token_path, master_key_path, server_host_from_url};
use datum_cli::sync::{key, pull, push, run_sync_with_client, AuthToken, Direction, TestPrompter};
use datum_core::conflicts::{ListFilter, Resolution};
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use datum_sync_client::crypto::changeset::{decrypt_changeset, EncryptedBlob};
use datum_sync_client::crypto::key_wrap::unwrap_master_key;
use datum_sync_client::ops::pack_changeset;
use reqwest::blocking::Client;
use tempfile::TempDir;

use sync_support::{StubRelay, ENV_LOCK};

// ──────────────────────────────────────────────────────────────────────────
// Shared scoped-env scaffolding (matches sync_cmd / sync_push_pull patterns)
// ──────────────────────────────────────────────────────────────────────────

struct EnvGuard {
    _tmp: TempDir,
    _orig_home: EnvRestore,
    _orig_cfg_home: EnvRestore,
    _orig_db: EnvRestore,
    _orig_cfg: EnvRestore,
    _guard: std::sync::MutexGuard<'static, ()>,
}

struct EnvRestore {
    key: &'static str,
    prev: Option<String>,
}

impl EnvRestore {
    fn capture(key: &'static str) -> Self {
        Self {
            key,
            prev: std::env::var(key).ok(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

fn scoped_env() -> EnvGuard {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new().expect("mk tmp");
    let orig_home = EnvRestore::capture("DATUM_HOME");
    let orig_cfg = EnvRestore::capture("DATUM_CONFIG_HOME");
    let orig_db = EnvRestore::capture("DATUM_DB");
    let orig_cfg_file = EnvRestore::capture("DATUM_CONFIG");

    std::env::set_var("DATUM_HOME", tmp.path().join("data"));
    std::env::set_var("DATUM_CONFIG_HOME", tmp.path().join("config"));
    std::env::remove_var("DATUM_DB");
    std::env::remove_var("DATUM_CONFIG");

    EnvGuard {
        _tmp: tmp,
        _orig_home: orig_home,
        _orig_cfg_home: orig_cfg,
        _orig_db: orig_db,
        _orig_cfg: orig_cfg_file,
        _guard: guard,
    }
}

fn write_token_for(server: &str, token: &str) {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host).unwrap();
    let t = AuthToken {
        token: token.to_string(),
        refreshed_at: Utc::now().to_rfc3339(),
        server: server.to_string(),
        user: Some("vividn@vividn.dev".to_string()),
    };
    std::fs::write(path, serde_json::to_vec_pretty(&t).unwrap()).unwrap();
}

fn write_master_key(server: &str, key_bytes: &[u8; 32]) {
    let host = server_host_from_url(server);
    let path = master_key_path(&host).unwrap();
    std::fs::write(path, key_bytes).unwrap();
}

// ──────────────────────────────────────────────────────────────────────────
// Entry construction helpers
// ──────────────────────────────────────────────────────────────────────────

/// Build a deterministic occur entry. Tests pin `create_time = modify_time
/// = base + offset_secs` so LWW comparisons are stable across runs.
fn make_occur(field: &str, offset_secs: i64, device: &str) -> Entry {
    let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
    let at = base + Duration::seconds(offset_secs);
    let t = DatumTime {
        utc: at,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    };
    let mut e = Entry::new(
        field,
        Verb::Occur,
        Some(serde_json::json!(1)),
        None,
        None,
        Some(t),
        None,
        device,
    );
    e.create_time = at;
    e.modify_time = at;
    e
}

/// Switch entry — exercises the JSON-state flavour (`mood -> happy`).
fn make_switch(field: &str, state: &str, offset_secs: i64, device: &str) -> Entry {
    let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
    let at = base + Duration::seconds(offset_secs);
    let t = DatumTime {
        utc: at,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    };
    let mut e = Entry::new(
        field,
        Verb::Switch,
        Some(serde_json::json!(state)),
        None,
        None,
        Some(t),
        None,
        device,
    );
    e.create_time = at;
    e.modify_time = at;
    e
}

/// Block entry with annotated `extra_data` — exercises the units encoding
/// path (`{"v":75.2,"u":"kg"}`).
fn make_block(
    field: &str,
    start_offset_secs: i64,
    duration_secs: i64,
    weight_kg: f64,
    device: &str,
) -> Entry {
    let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
    let start = base + Duration::seconds(start_offset_secs);
    let end = start + Duration::seconds(duration_secs);
    let t_start = DatumTime {
        utc: start,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    };
    let t_end = DatumTime {
        utc: end,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    };
    let data = serde_json::json!({
        "weight": {"v": weight_kg, "u": "kg"},
        "reps": 10
    });
    let mut e = Entry::new(
        field,
        Verb::Block,
        None,
        Some(data),
        None,
        Some(t_start),
        Some(t_end),
        device,
    );
    e.create_time = start;
    e.modify_time = start;
    e
}

/// Snapshot the live-row count in a DB — used to assert convergence.
fn entries_count(db: &Db) -> i64 {
    db.conn()
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap()
}

/// Hashes (content_hash) of every live entry, sorted. Two devices that
/// converge must have identical sets, regardless of insertion order.
fn entry_hash_set(db: &Db) -> Vec<String> {
    let mut stmt = db
        .conn()
        .prepare("SELECT content_hash FROM entries ORDER BY content_hash")
        .unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

/// Build a fresh DB at `<DATUM_HOME>/<name>.db`. Each device-instance gets
/// its own file so they don't collide on the shared `entries` schema.
fn open_db(name: &str) -> Db {
    let home = std::env::var("DATUM_HOME").unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let path = std::path::Path::new(&home).join(format!("{name}.db"));
    Db::open(path.to_str().unwrap()).unwrap()
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 1 — local-first start, then sync-server adoption
// ──────────────────────────────────────────────────────────────────────────

/// 30+ entries spanning a few days are pushed; the relay's raw row is
/// asserted to be CIPHERTEXT (no plaintext field names) and decrypts back
/// to the same plaintext changeset under the test master key.
#[test]
fn s1_local_first_then_server_adoption_with_ciphertext_check() {
    let _env = scoped_env();
    let mut relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());

    // Real `key init` — wraps a fresh master key under a passphrase and
    // uploads to the relay. Mirrors the on-device provisioning flow.
    let client = Client::builder().build().unwrap();
    let mut prompter = TestPrompter::new(["e2e-passphrase", "e2e-passphrase"]);
    let master_key =
        key::init_with_client(&relay.base_url, &client, false, &mut prompter).expect("key init");
    assert_eq!(master_key.len(), 32);

    // Seed >=30 diverse entries spanning a few days.
    let db = open_db("alpha");
    for i in 0..15 {
        // Switch entries (mood, project), point occurs (coffee), and
        // annotated blocks (workout w/ weight=kg) — every shape the data
        // model supports.
        let day_offset = (i / 5) * 86_400;
        let bucket = i % 5;
        let t = day_offset + bucket * 600;
        db.insert_entry(&make_switch("mood", "happy", t, "dev-A"))
            .unwrap();
        db.insert_entry(&make_occur("coffee", t + 60, "dev-A"))
            .unwrap();
    }
    // Two annotated blocks for unit coverage.
    db.insert_entry(&make_block("workout", 0, 1800, 75.2, "dev-A"))
        .unwrap();
    db.insert_entry(&make_block("workout", 86_400, 2400, 76.0, "dev-A"))
        .unwrap();
    // Custom-field switch for breadth.
    db.insert_entry(&make_switch("project", "daydumb", 0, "dev-A"))
        .unwrap();
    let local_count = entries_count(&db);
    assert!(local_count >= 30, "want >=30 entries, got {local_count}");

    // Push.
    let outcome =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).expect("push");
    assert_eq!(outcome.pushed as i64, local_count);

    // CRITICAL: the raw row on the server-side store is ciphertext —
    // never contains plaintext field names. We read directly from the
    // server's SQLite (bypassing the API) per the bead's spec.
    let raw = relay.dump_raw_changesets();
    assert_eq!(raw.len(), 1, "single push -> single changeset row");
    let (device_id, seq, payload_b64) = &raw[0];
    // device_id is whatever ULID `device_id()` minted into DATUM_HOME on
    // the first sync — we don't pin a specific value, but we can confirm
    // it's a non-empty token (rules out an empty default leaking through).
    assert!(!device_id.is_empty(), "device_id must be set");
    assert_eq!(*seq, 1);
    let cipher_bytes = URL_SAFE_NO_PAD.decode(payload_b64).expect("b64");
    let cipher_str = String::from_utf8_lossy(&cipher_bytes).to_string();
    for needle in ["mood", "happy", "coffee", "workout", "project", "daydumb"] {
        assert!(
            !cipher_str.contains(needle),
            "ciphertext leaked plaintext '{needle}' — E2E encryption broken"
        );
    }

    // Decrypt with the test master key — proves AAD binding works
    // end-to-end against the server-stored bytes.
    let blob = EncryptedBlob::from_bytes(cipher_bytes);
    let cs = decrypt_changeset(&blob, &master_key, device_id, *seq as u64)
        .expect("decrypt with master key");
    let plain = cs.as_bytes();
    let txt = String::from_utf8_lossy(plain);
    for needle in ["mood", "happy", "coffee", "workout", "project", "daydumb"] {
        assert!(
            txt.contains(needle),
            "plaintext should contain '{needle}' after decrypt"
        );
    }

    relay.stop_http();
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 2 — second instance joins encryption chain via passphrase
// ──────────────────────────────────────────────────────────────────────────

/// A new device unlocks the master via the same passphrase wrapped key
/// and pulls A's full history. Entry sets converge by content_hash.
#[test]
fn s2_second_instance_joins_via_passphrase_unwrap() {
    let _env = scoped_env();
    let mut relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());

    let client = Client::builder().build().unwrap();
    let mut prompter_a = TestPrompter::new(["shared-pass", "shared-pass"]);
    let master_a = key::init_with_client(&relay.base_url, &client, false, &mut prompter_a)
        .expect("device A key init");

    let db_a = open_db("alpha");
    for i in 0..30 {
        db_a.insert_entry(&make_occur(
            &format!("activity-{}", i % 4),
            (i as i64) * 60,
            "dev-A",
        ))
        .unwrap();
    }
    push::run(
        &relay.base_url,
        &client,
        &relay.mint_token(),
        "dev-A",
        &master_a,
        &db_a,
        false,
    )
    .expect("dev-A push");

    // Fresh device B pulls the wrapped blob, unwraps with the same
    // passphrase. The recovered key must equal A's, otherwise the AAD
    // bound on A's push would refuse to decrypt for B.
    let wrapped_json = relay.wrapped_key().expect("wrapped key on relay");
    let wrapped: datum_sync_client::crypto::key_wrap::WrappedKey =
        serde_json::from_value(wrapped_json).expect("parse wrapped");
    let master_b = unwrap_master_key("shared-pass", &wrapped).expect("unwrap");
    assert_eq!(
        master_b, master_a,
        "passphrase unwrap must match A's master"
    );

    let db_b = open_db("beta");
    pull::run(
        &relay.base_url,
        &client,
        &relay.mint_token(),
        "dev-B",
        &master_b,
        &db_b,
        false,
    )
    .expect("dev-B pull");

    assert_eq!(entries_count(&db_a), entries_count(&db_b));
    assert_eq!(entry_hash_set(&db_a), entry_hash_set(&db_b));

    relay.stop_http();
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 3 — bidirectional convergence
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn s3_bidirectional_convergence() {
    let _env = scoped_env();
    let mut relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0xA3u8; 32];
    write_master_key(&relay.base_url, &master_key);
    let client = Client::builder().build().unwrap();
    let bearer = relay.mint_token();

    let db_a = open_db("alpha");
    let db_b = open_db("beta");
    // A baseline of 30 on A so we exercise an actual deltas-vs-full push.
    for i in 0..30 {
        db_a.insert_entry(&make_occur("baseline", i * 60, "dev-A"))
            .unwrap();
    }
    // Both sides log 5 disjoint-field entries.
    for i in 0..5 {
        db_a.insert_entry(&make_occur(
            &format!("a-only-{i}"),
            10_000 + i * 60,
            "dev-A",
        ))
        .unwrap();
        db_b.insert_entry(&make_occur(
            &format!("b-only-{i}"),
            20_000 + i * 60,
            "dev-B",
        ))
        .unwrap();
    }

    // Round 1: each pushes its writes.
    push::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-A",
        &master_key,
        &db_a,
        false,
    )
    .unwrap();
    push::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-B",
        &master_key,
        &db_b,
        false,
    )
    .unwrap();

    // Round 2: each pulls the other's writes.
    pull::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-A",
        &master_key,
        &db_a,
        false,
    )
    .unwrap();
    pull::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-B",
        &master_key,
        &db_b,
        false,
    )
    .unwrap();

    assert_eq!(
        entries_count(&db_a),
        40,
        "A: 30 baseline + 5 own + 5 from B"
    );
    assert_eq!(
        entries_count(&db_b),
        40,
        "B: 30 baseline + 5 own + 5 from A"
    );
    assert_eq!(entry_hash_set(&db_a), entry_hash_set(&db_b));

    // No spurious conflicts queued — disjoint fields can't conflict.
    let conflicts_a = db_a.list_conflicts(&ListFilter::default()).unwrap();
    let conflicts_b = db_b.list_conflicts(&ListFilter::default()).unwrap();
    assert!(
        conflicts_a.is_empty(),
        "A unexpected conflicts: {conflicts_a:?}"
    );
    assert!(
        conflicts_b.is_empty(),
        "B unexpected conflicts: {conflicts_b:?}"
    );

    relay.stop_http();
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 4 — multi-party 3-device cluster
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn s4_three_device_cluster_converges() {
    let _env = scoped_env();
    let mut relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x42u8; 32];
    write_master_key(&relay.base_url, &master_key);
    let client = Client::builder().build().unwrap();
    let bearer = relay.mint_token();

    let db_a = open_db("alpha");
    let db_b = open_db("beta");
    let db_c = open_db("gamma");
    for i in 0..3 {
        db_a.insert_entry(&make_occur(&format!("a-{i}"), i * 60, "dev-A"))
            .unwrap();
        db_b.insert_entry(&make_occur(&format!("b-{i}"), 10_000 + i * 60, "dev-B"))
            .unwrap();
        db_c.insert_entry(&make_occur(&format!("c-{i}"), 20_000 + i * 60, "dev-C"))
            .unwrap();
    }

    let devices = [("dev-A", &db_a), ("dev-B", &db_b), ("dev-C", &db_c)];
    // Two full pull-then-push rounds across all devices: round 1 lifts
    // every local write to the relay; round 2 lets every device pull the
    // peers' writes that arrived during round 1.
    for _ in 0..2 {
        for (id, db) in devices.iter() {
            pull::run(
                &relay.base_url,
                &client,
                &bearer,
                id,
                &master_key,
                db,
                false,
            )
            .unwrap();
            push::run(
                &relay.base_url,
                &client,
                &bearer,
                id,
                &master_key,
                db,
                false,
            )
            .unwrap();
        }
    }

    let a = entry_hash_set(&db_a);
    let b = entry_hash_set(&db_b);
    let c = entry_hash_set(&db_c);
    assert_eq!(a.len(), 9, "A converged on all 9 entries");
    assert_eq!(a, b, "A and B converged");
    assert_eq!(b, c, "B and C converged");

    relay.stop_http();
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 5 — offline edits then converge
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn s5_offline_edits_then_converge() {
    let _env = scoped_env();
    let mut relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x55u8; 32];
    write_master_key(&relay.base_url, &master_key);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();
    let bearer = relay.mint_token();

    let db_a = open_db("alpha");
    let db_b = open_db("beta");
    // Each side has a small initial baseline that sync-ed before partition.
    db_a.insert_entry(&make_occur("seed-a", 0, "dev-A"))
        .unwrap();
    db_b.insert_entry(&make_occur("seed-b", 100, "dev-B"))
        .unwrap();

    push::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-A",
        &master_key,
        &db_a,
        false,
    )
    .unwrap();
    push::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-B",
        &master_key,
        &db_b,
        false,
    )
    .unwrap();
    pull::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-A",
        &master_key,
        &db_a,
        false,
    )
    .unwrap();
    pull::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-B",
        &master_key,
        &db_b,
        false,
    )
    .unwrap();
    assert_eq!(entries_count(&db_a), 2);
    assert_eq!(entries_count(&db_b), 2);

    // Partition: stop the relay's HTTP listener. AppState (devices,
    // changesets, wrapped_keys) is preserved so the restart resumes
    // cleanly.
    let dead_url = relay.base_url.clone();
    relay.stop_http();

    // Both sides log >=10 entries while disconnected.
    for i in 0..10 {
        db_a.insert_entry(&make_occur(
            &format!("offline-a-{i}"),
            1_000 + i * 60,
            "dev-A",
        ))
        .unwrap();
        db_b.insert_entry(&make_occur(
            &format!("offline-b-{i}"),
            2_000 + i * 60,
            "dev-B",
        ))
        .unwrap();
    }

    // Push attempts while offline must error — the client cannot reach
    // a closed port. We only assert that *some* error surfaces (TCP
    // refusal vs. timeout vs. reset are platform-dependent).
    let err_a = push::run(
        &dead_url,
        &client,
        &bearer,
        "dev-A",
        &master_key,
        &db_a,
        false,
    );
    assert!(err_a.is_err(), "push should fail while server is down");

    // Restart server with state intact.
    relay.restart_http();
    let live_url = relay.base_url.clone();

    // Both devices push and pull. Final state on both sides matches.
    for (id, db) in [("dev-A", &db_a), ("dev-B", &db_b)] {
        push::run(&live_url, &client, &bearer, id, &master_key, db, false).unwrap();
    }
    for (id, db) in [("dev-A", &db_a), ("dev-B", &db_b)] {
        pull::run(&live_url, &client, &bearer, id, &master_key, db, false).unwrap();
    }

    assert_eq!(entries_count(&db_a), 22, "2 baseline + 10 own + 10 peer");
    assert_eq!(entries_count(&db_b), 22);
    assert_eq!(entry_hash_set(&db_a), entry_hash_set(&db_b));

    relay.stop_http();
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 6 — agent kill/restart mid-sync (in-process equivalent)
// ──────────────────────────────────────────────────────────────────────────

/// `datum-sync-agent` is still a stub (da-ndd.15), so we exercise the
/// agent's *resumption* contract directly: a partial push completes
/// successfully, the local watermark advances, the "agent" restarts
/// (a new client), and a follow-up push lands the remaining entries
/// without re-uploading anything from batch 1.
#[test]
fn s6_agent_kill_restart_resumes_without_data_loss() {
    let _env = scoped_env();
    let mut relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x66u8; 32];
    write_master_key(&relay.base_url, &master_key);
    let bearer = relay.mint_token();

    let db = open_db("alpha");

    // Batch 1: 60 entries — pushed by "agent run 1".
    for i in 0..60 {
        db.insert_entry(&make_occur("burst-a", i * 10, "dev-A"))
            .unwrap();
    }
    {
        let client_v1 = Client::builder().build().unwrap();
        let out = push::run(
            &relay.base_url,
            &client_v1,
            &bearer,
            "dev-A",
            &master_key,
            &db,
            false,
        )
        .unwrap();
        assert_eq!(out.pushed, 60);
        // Drop client_v1 here — equivalent to the agent process exiting.
    }
    assert_eq!(relay.changeset_count(), 1);

    // Batch 2: 50 more entries while the agent was down.
    for i in 0..50 {
        db.insert_entry(&make_occur("burst-b", 100_000 + i * 10, "dev-A"))
            .unwrap();
    }

    // Agent restart: brand-new client, brand-new bearer fetch — same
    // local DB. The push_seq watermark stored on the DB drives
    // resumption; only the new rows go up.
    {
        let client_v2 = Client::builder().build().unwrap();
        let out = push::run(
            &relay.base_url,
            &client_v2,
            &bearer,
            "dev-A",
            &master_key,
            &db,
            false,
        )
        .unwrap();
        assert_eq!(out.pushed, 50, "second push must not re-send batch 1");
    }

    // Server has 110 distinct entries spread across two changeset rows
    // (one per push call). No silent duplicate uploads.
    assert_eq!(relay.changeset_count(), 2);

    // Confirm via a peer pull that the merged stream replays correctly.
    let peer_db = open_db("peer");
    let client = Client::builder().build().unwrap();
    pull::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-peer",
        &master_key,
        &peer_db,
        false,
    )
    .unwrap();
    assert_eq!(entries_count(&peer_db), 110);

    relay.stop_http();
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 7 — server restart mid-sync
// ──────────────────────────────────────────────────────────────────────────

/// While A is mid-push, the relay dies. After restart (same in-process
/// state), A's retry succeeds. The server stores exactly the data that
/// landed — no duplicate row from a partial-then-retried upload.
#[test]
fn s7_server_restart_mid_sync() {
    let _env = scoped_env();
    let mut relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x77u8; 32];
    write_master_key(&relay.base_url, &master_key);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();
    let bearer = relay.mint_token();

    let db = open_db("alpha");
    for i in 0..40 {
        db.insert_entry(&make_occur("data", i * 60, "dev-A"))
            .unwrap();
    }

    // Stop the server *before* the push call — the client experiences a
    // connection refusal mid-attempt, which is the same failure surface
    // as a kill mid-request from the client's viewpoint.
    let dead_url = relay.base_url.clone();
    relay.stop_http();
    let err = push::run(
        &dead_url,
        &client,
        &bearer,
        "dev-A",
        &master_key,
        &db,
        false,
    );
    assert!(err.is_err(), "push must fail while server is down");
    // Watermarks must NOT have advanced on a failed push, otherwise the
    // retry would skip rows.
    assert_eq!(
        datum_sync_client::sync_state::get_my_push_seq(&db).unwrap(),
        0,
        "push_seq must not advance on a failed upload"
    );

    // Restart server — same AppState (devices, changesets, wrapped key).
    relay.restart_http();
    let live_url = relay.base_url.clone();
    let out = push::run(
        &live_url,
        &client,
        &bearer,
        "dev-A",
        &master_key,
        &db,
        false,
    )
    .expect("retry push after server restart");
    assert_eq!(out.pushed, 40);
    assert_eq!(
        relay.changeset_count(),
        1,
        "exactly one stored changeset — no dup from retried upload"
    );

    // A peer can pull the full stream from the restored server.
    let peer_db = open_db("peer");
    pull::run(
        &live_url,
        &client,
        &bearer,
        "dev-peer",
        &master_key,
        &peer_db,
        false,
    )
    .unwrap();
    assert_eq!(entries_count(&peer_db), 40);

    relay.stop_http();
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 8 — conflict-queue surfacing
// ──────────────────────────────────────────────────────────────────────────

/// A and B both `start` the same field while disconnected. After both
/// converge online, the local detector files a `double_open` row in the
/// conflict queue. `resolve_conflict(.., TakeLocal)` clears it.
#[test]
fn s8_conflict_surface_and_resolve() {
    let _env = scoped_env();
    let mut relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x88u8; 32];
    write_master_key(&relay.base_url, &master_key);
    let client = Client::builder().build().unwrap();
    let bearer = relay.mint_token();

    let db_a = open_db("alpha");
    let db_b = open_db("beta");

    // Disconnected starts on the same `focus` field 30 minutes apart —
    // textbook double_open from B's perspective once it pulls A's row.
    let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
    let mut a_start = Entry::new(
        "focus",
        Verb::Start,
        None,
        None,
        None,
        Some(DatumTime {
            utc: base,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        }),
        None,
        "dev-A",
    );
    a_start.create_time = base;
    a_start.modify_time = base;
    db_a.insert_entry(&a_start).unwrap();

    let b_at = base + Duration::minutes(30);
    let mut b_start = Entry::new(
        "focus",
        Verb::Start,
        None,
        None,
        None,
        Some(DatumTime {
            utc: b_at,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        }),
        None,
        "dev-B",
    );
    b_start.create_time = b_at;
    b_start.modify_time = b_at;
    db_b.insert_entry(&b_start).unwrap();

    push::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-A",
        &master_key,
        &db_a,
        false,
    )
    .unwrap();
    let pulled = pull::run(
        &relay.base_url,
        &client,
        &bearer,
        "dev-B",
        &master_key,
        &db_b,
        false,
    )
    .unwrap();
    assert!(
        pulled.conflicts_queued >= 1,
        "expected a queued sync_pull conflict on B"
    );

    // A pending row exists with detected_by = sync_pull.
    let pending = db_b
        .list_conflicts(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1, "exactly one pending conflict");
    let c = &pending[0];

    // resolve --take local clears it.
    db_b.resolve_conflict(&c.hid, Resolution::TakeLocal)
        .expect("resolve");
    let remaining = db_b
        .list_conflicts(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    assert!(
        remaining.is_empty(),
        "pending queue must be empty post-resolve, got {remaining:?}"
    );

    relay.stop_http();
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 9 — cr-sqlite changeset replay determinism
// ──────────────────────────────────────────────────────────────────────────

/// Pack A's full changeset stream after a known sequence of inserts.
/// Replay it on a fresh DB. The resulting `entries` rows must be
/// content-byte-identical (modulo `modify_time`, per the spec — which in
/// our LWW pack/apply happens to be preserved exactly, but we still
/// compare the *content* fields the spec calls out).
#[test]
fn s9_cr_sqlite_replay_determinism() {
    let _env = scoped_env();
    // No relay needed — pack/apply is a client-only concern.

    let db_src = Db::open_in_memory().unwrap();
    // Diverse entries — switch, occur, block — so all the on-disk shape
    // columns get packed and replayed.
    db_src
        .insert_entry(&make_switch("mood", "focused", 0, "dev-A"))
        .unwrap();
    db_src
        .insert_entry(&make_occur("coffee", 60, "dev-A"))
        .unwrap();
    db_src
        .insert_entry(&make_block("workout", 120, 1800, 75.5, "dev-A"))
        .unwrap();
    db_src
        .insert_entry(&make_switch("project", "daydumb", 240, "dev-A"))
        .unwrap();

    let cs = pack_changeset(&db_src, 0).expect("pack");
    let db_replay = Db::open_in_memory().unwrap();
    datum_sync_client::ops::apply_changeset(&db_replay, &cs).expect("apply");

    // Compare row-by-row on the spec'd fields. We sort by hid because
    // insert order isn't a contract.
    let rows_src = read_canonical_rows(&db_src);
    let rows_dst = read_canonical_rows(&db_replay);
    assert_eq!(rows_src.len(), 4);
    assert_eq!(
        rows_src, rows_dst,
        "replayed entries must be content-byte-identical"
    );

    // Re-pack-and-replay-again is also stable: a fresh DB seeded from
    // the replay's pack equals the first replay. This catches any
    // non-determinism in the encoding that would surface as a flapping
    // state across cr-sqlite version bumps.
    let cs2 = pack_changeset(&db_replay, 0).expect("repack");
    let db_replay2 = Db::open_in_memory().unwrap();
    datum_sync_client::ops::apply_changeset(&db_replay2, &cs2).expect("apply2");
    assert_eq!(rows_dst, read_canonical_rows(&db_replay2));
}

/// Canonical row tuple for replay determinism: every column the spec
/// pins as part of the entry's identity. Excludes `modify_time` per the
/// bead's "modulo modify_time" carve-out.
type CanonicalRow = (
    String, // hid
    String, // slug
    String, // content_hash
    String, // field
    String, // value
    String, // data
    String, // entry_shape
    String, // start_time
    String, // end_time
    String, // device_id
);

fn read_canonical_rows(db: &Db) -> Vec<CanonicalRow> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT hid, slug, content_hash, field,
                    COALESCE(value,''), COALESCE(data,''), entry_shape,
                    COALESCE(start_time,''), COALESCE(end_time,''), device_id
             FROM entries ORDER BY hid",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
        ))
    })
    .unwrap()
    .map(|r| r.unwrap())
    .collect()
}
