//! Acceptance tests for `datum sync` push / pull / bidirectional / status
//! (bead da-ndd.14 = C4c).
//!
//! Tests mutate `DATUM_HOME` and `DATUM_CONFIG_HOME` so they're serialised
//! through the shared env lock. Each test constructs its own stub relay which
//! exposes the wire protocol from `docs/sync-protocol.md` (`/v0/push`,
//! `/v0/pull`, `/v0/keys/wrapped`, `/v0/auth/device-code*`).

mod sync_support;

use chrono::{Duration as ChronoDuration, Utc};
use datum_cli::config::{auth_token_path, master_key_path, server_host_from_url};
use datum_cli::sync::{run_sync_with_client, AuthToken, Direction, SyncOutcome};
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use datum_sync_client::sync_state;
use tempfile::TempDir;

use sync_support::{env_lock, ScopedEnv, StubRelay};

struct EnvGuard {
    // Fields drop in declaration order: restore env vars first, then
    // release the mutex. Putting `_guard` last means no other test can
    // observe a partially-restored environment.
    _tmp: TempDir,
    _orig_home: ScopedEnv,
    _orig_cfg_home: ScopedEnv,
    _orig_db: ScopedEnv,
    _orig_cfg: ScopedEnv,
    _guard: std::sync::MutexGuard<'static, ()>,
}

fn scoped_env() -> EnvGuard {
    let guard = env_lock();
    let tmp = TempDir::new().expect("mk tmp");
    let orig_home = ScopedEnv::set("DATUM_HOME", tmp.path().join("data"));
    let orig_cfg = ScopedEnv::set("DATUM_CONFIG_HOME", tmp.path().join("config"));
    let orig_db = ScopedEnv::remove("DATUM_DB");
    let orig_cfg_file = ScopedEnv::remove("DATUM_CONFIG");

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
    let t = datum_sync_client::PersistedAuthToken {
        wire: AuthToken {
            token: token.to_string(),
            refresh_token: "test-refresh".to_string(),
            user_id: "vividn@vividn.dev".to_string(),
            cluster_id: None,
        },
        refreshed_at: Utc::now().to_rfc3339(),
        server: server.to_string(),
    };
    std::fs::write(path, serde_json::to_vec_pretty(&t).unwrap()).unwrap();
}

fn write_master_key(server: &str, key: &[u8; 32]) {
    let host = server_host_from_url(server);
    let path = master_key_path(&host).unwrap();
    std::fs::write(path, key).unwrap();
}

fn make_entry(field: &str, offset_secs: i64, device: &str) -> Entry {
    use chrono::{Duration, TimeZone};
    let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
    let at = base + Duration::seconds(offset_secs);
    let t = DatumTime {
        utc: at,
        offset_minutes: 0,
        timezone: Some("UTC".to_string()),
    };
    let mut e = Entry::new(
        field,
        Verb::Point,
        Some(serde_json::json!(1)),
        None,
        Some(t),
        None,
        device,
    );
    e.create_time = at;
    e.modify_time = at;
    e
}

fn fresh_db(env: &EnvGuard) -> Db {
    let _ = env;
    let path = std::env::var("DATUM_HOME").unwrap();
    std::fs::create_dir_all(&path).unwrap();
    let db_path = std::path::Path::new(&path).join("datum.db");
    Db::open(db_path.to_str().unwrap()).unwrap()
}

// ──────────────────────────────────────────────────────────────────────────
// Push
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn sync_push_uploads_new_changes_since_last_seq() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0x11u8; 32]);
    let db = fresh_db(&env);

    // Two new entries.
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    db.insert_entry(&make_entry("mood", 60, "dev-1")).unwrap();

    let outcome =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();

    assert_eq!(outcome.pushed, 2);
    assert_eq!(outcome.pulled, 0);

    // push_mark advanced past the entries' modify_time.
    assert!(sync_state::get_push_mark(&db).unwrap() > 0);
    assert_eq!(sync_state::get_device_seq(&db).unwrap(), 1);

    // A second push with no new rows sends nothing.
    let again =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    assert_eq!(again.pushed, 0);
    // Counter didn't tick because nothing was uploaded.
    assert_eq!(sync_state::get_device_seq(&db).unwrap(), 1);
}

/// da-60gf: a seq desync (here: device_seq rolled back, as after a DB
/// restore from backup) recovers in-band and surfaces as a typed
/// `SeqRebase` event on the outcome — no library-side printing.
#[test]
fn sync_push_seq_rebase_surfaces_in_outcome() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0x11u8; 32]);
    let db = fresh_db(&env);

    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    let first =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    assert_eq!(
        first.seq_rebased, None,
        "clean push must not report a rebase"
    );
    assert_eq!(sync_state::get_device_seq(&db).unwrap(), 1);

    // Roll the local counter back — the relay has seq 1 and expects 2,
    // but the next push will predict 1.
    sync_state::set_device_seq(&db, 0).unwrap();
    db.insert_entry(&make_entry("mood", 60, "dev-1")).unwrap();

    let outcome =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    assert_eq!(outcome.pushed, 1);
    let rebase = outcome
        .seq_rebased
        .expect("desynced push must report the rebase");
    assert_eq!(rebase.predicted, 1);
    assert_eq!(rebase.expected, 2);
    assert_eq!(rebase.device_id, datum_cli::config::device_id().unwrap());
    // Local counter re-based to the relay's authoritative value.
    assert_eq!(sync_state::get_device_seq(&db).unwrap(), 2);
}

/// da-9xst / da-ca4q: the relay's seq advances mid-flight (a "twin" insert
/// under our device_id lands just before our push — simulated via the
/// server's test-only `collide_pushes_remaining` knob). The client must
/// re-base to the relay's authoritative seq AND re-encrypt — the AAD binds
/// the seq, so replaying the old ciphertext at the new seq would store a
/// blob no peer can unseal. Proven here by decrypting the stored blob
/// under the adopted seq.
#[test]
fn seq_recovery_reencrypts_with_relay_authoritative_aad() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0xb1u8; 32];
    write_master_key(&relay.base_url, &master_key);
    let db = fresh_db(&env);
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();

    // One twin insert races ahead of the first push attempt only.
    *relay.state.collide_pushes_remaining.lock().unwrap() = 1;

    let outcome =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    assert_eq!(outcome.pushed, 1);
    let rebase = outcome
        .seq_rebased
        .expect("raced push must report the rebase");
    assert_eq!(rebase.predicted, 1);
    assert_eq!(rebase.expected, 2);
    // Local counter adopted the relay's authoritative value.
    assert_eq!(sync_state::get_device_seq(&db).unwrap(), 2);

    // The retried blob landed at the relay's expected seq and unseals
    // under the NEW AAD (device_id, seq=2, cluster) — i.e. the recovery
    // re-encrypted rather than replaying the seq-1 ciphertext. Cluster is
    // the da-3fnd file-stem default for `datum.db`.
    let device_id = datum_cli::config::device_id().unwrap();
    let (_, _, payload_b64) = relay
        .dump_raw_changesets()
        .into_iter()
        .find(|(dev, seq, _)| *dev == device_id && *seq == 2)
        .expect("recovered push stored at relay's expected seq");
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let raw = URL_SAFE_NO_PAD.decode(payload_b64.as_bytes()).unwrap();
    let blob = datum_sync_client::crypto::changeset::EncryptedBlob::from_bytes(raw);
    datum_sync_client::crypto::changeset::decrypt_changeset(
        &blob,
        &master_key,
        &device_id,
        2,
        "datum",
    )
    .expect("re-encrypted blob must unseal under the adopted seq");
}

/// da-9xst / da-ca4q: a seq mismatch on the retry as well means a second
/// device is actively pushing under our device_id — the push must refuse
/// with the collision error rather than loop on a moving target, and must
/// leave every local watermark untouched.
#[test]
fn second_seq_mismatch_refuses_as_device_id_collision() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0xb2u8; 32]);
    let db = fresh_db(&env);
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();

    // A twin insert races ahead of BOTH the first attempt and the retry.
    *relay.state.collide_pushes_remaining.lock().unwrap() = 2;

    let err = run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db)
        .expect_err("second mismatch must be fatal, not retried again");
    let msg = format!("{err:#}");
    let device_id = datum_cli::config::device_id().unwrap();
    assert!(
        msg.contains("another device is using device_id") && msg.contains(&device_id),
        "expected device_id-collision refusal naming the device, got: {msg}",
    );
    assert!(
        msg.contains("refusing to push"),
        "expected refusal surface, got: {msg}",
    );

    // No watermark corruption: the failed push must not adopt the moving
    // seq, advance the push mark, or register a cluster.
    assert_eq!(sync_state::get_device_seq(&db).unwrap(), 0);
    assert_eq!(sync_state::get_push_mark(&db).unwrap(), 0);
    assert_eq!(sync_state::get_cluster_id(&db).unwrap(), None);
    // The relay holds only the two twin rows — our blob never landed.
    assert_eq!(relay.changeset_count(), 2);
}

/// da-hu5y: two DBs on one machine share the per-machine `device_id`
/// (config.rs writes one ULID per machine) but count `device_seq`
/// independently — sync_state lives in each DB. The relay scopes its
/// seq counter per (device_id, cluster_id) to match, so alternating
/// pushes from the two DBs must all be clean: no SeqMismatch re-base
/// (extra round-trip + re-encrypt) in the multi-DB steady state.
#[test]
fn multi_db_one_device_alternating_pushes_never_rebase() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0x33u8; 32]);

    let home = std::env::var("DATUM_HOME").unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let db_a = Db::open(&format!("{home}/alpha.db")).unwrap();
    let db_b = Db::open(&format!("{home}/beta.db")).unwrap();
    // Two DBs = two clusters (da-3fnd defaults each to its file-name
    // stem; pinned explicitly here for clarity).
    sync_state::set_cluster_id(&db_a, "alpha").unwrap();
    sync_state::set_cluster_id(&db_b, "beta").unwrap();

    // The bead's walk-through: alternate A, B, A, B. Under a device-
    // global server counter every push after the first would predict
    // low and re-base; per-cluster scoping makes each predict cleanly.
    let rounds: [(&Db, &str); 4] = [(&db_a, "a1"), (&db_b, "b1"), (&db_a, "a2"), (&db_b, "b2")];
    for (round, (db, field)) in rounds.into_iter().enumerate() {
        db.insert_entry(&make_entry(field, round as i64 * 60, "dev-1"))
            .unwrap();
        let outcome =
            run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, db).unwrap();
        assert_eq!(outcome.pushed, 1);
        assert_eq!(
            outcome.seq_rebased,
            None,
            "push #{} ({field}) must not re-base",
            round + 1
        );
    }

    // Per-DB counters advanced independently — each cluster is at 2.
    assert_eq!(sync_state::get_device_seq(&db_a).unwrap(), 2);
    assert_eq!(sync_state::get_device_seq(&db_b).unwrap(), 2);
}

#[test]
fn sync_pull_applies_changes_and_advances_seq() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x22u8; 32];
    write_master_key(&relay.base_url, &master_key);

    // Peer device pushes a changeset into the relay directly.
    let peer_db = Db::open_in_memory().unwrap();
    peer_db
        .insert_entry(&make_entry("peer-field", 0, "dev-peer"))
        .unwrap();

    let peer_changeset = datum_sync_client::ops::pack_changeset(&peer_db, 0).unwrap();
    let peer_blob = datum_sync_client::crypto::changeset::encrypt_changeset(
        &peer_changeset,
        &master_key,
        "dev-peer",
        1,
        "datum", // matches the StubRelay's injected cluster_id (file-stem default)
    )
    .unwrap();
    relay.inject_changeset("dev-peer", 1, peer_blob.as_bytes());

    // Our local DB starts empty.
    let db = fresh_db(&env);

    let outcome =
        run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db).unwrap();

    assert_eq!(outcome.pushed, 0);
    assert_eq!(outcome.pulled, 1);

    // Peer entry is now in the local DB.
    let count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE field = 'peer-field'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);

    // pull_seq advanced.
    assert!(sync_state::get_pull_seq(&db).unwrap() > 0);
}

#[test]
fn sync_bidirectional_pulls_first() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x33u8; 32];
    write_master_key(&relay.base_url, &master_key);

    // Stash a peer changeset.
    let peer_db = Db::open_in_memory().unwrap();
    peer_db
        .insert_entry(&make_entry("peer-field", 0, "dev-peer"))
        .unwrap();
    let cs = datum_sync_client::ops::pack_changeset(&peer_db, 0).unwrap();
    let blob = datum_sync_client::crypto::changeset::encrypt_changeset(
        &cs,
        &master_key,
        "dev-peer",
        1,
        "datum",
    )
    .unwrap();
    relay.inject_changeset("dev-peer", 1, blob.as_bytes());

    // Local has its own entry.
    let db = fresh_db(&env);
    db.insert_entry(&make_entry("local-field", 0, "dev-1"))
        .unwrap();

    let outcome = run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db).unwrap();

    assert_eq!(outcome.pulled, 1, "should pull 1 peer change");
    // After pulling the peer entry, a fresh pack sees both rows in the DB.
    // Since push_mark starts at 0, BOTH rows go up the wire.
    assert_eq!(outcome.pushed, 2);
}

// ──────────────────────────────────────────────────────────────────────────
// Dry-run
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn sync_dry_run_does_not_mutate() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0x44u8; 32]);

    let db = fresh_db(&env);
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();

    let outcome = run_sync_with_client(
        Some(&relay.base_url),
        Direction::PushOnly,
        true, // dry_run
        &db,
    )
    .unwrap();

    assert_eq!(outcome.pushed, 1);
    assert!(outcome.dry_run);
    // Watermarks stay at 0 — nothing actually shipped.
    assert_eq!(sync_state::get_push_mark(&db).unwrap(), 0);
    assert_eq!(sync_state::get_device_seq(&db).unwrap(), 0);
    // Relay state is empty.
    assert_eq!(relay.changeset_count(), 0);
}

// ──────────────────────────────────────────────────────────────────────────
// Status + error paths
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn sync_status_reports_per_db() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0x55u8; 32]);
    let db = fresh_db(&env);
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();

    // `datum sync status` prints human output; we invoke it for side
    // effects (no panics / errors) and check that watermarks are what the
    // status renderer reads from.
    datum_cli::sync::status::run(
        datum_cli::args::SyncStatusArgs {
            server: Some(relay.base_url.clone()),
        },
        None,
    )
    .unwrap();

    let last_push = sync_state::get_last_push_at(&db).unwrap();
    assert!(
        last_push.is_some(),
        "status should have a real last_push_at"
    );
}

#[test]
fn sync_without_login_errors_cleanly() {
    let env = scoped_env();
    let relay = StubRelay::start();
    // Deliberately skip writing a token.
    write_master_key(&relay.base_url, &[0x66u8; 32]);
    let db = fresh_db(&env);

    let err = run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("DATUM_SYNC_TOKEN"),
        "error should point at the working bootstrap (DATUM_SYNC_TOKEN), got: {msg}"
    );
}

#[test]
fn sync_without_unlocked_key_errors_cleanly() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    // Deliberately skip writing a master key.
    let db = fresh_db(&env);

    let err = run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("master key not unlocked")
            || msg.contains("key init")
            || msg.contains("key unlock"),
        "expected key-not-unlocked error, got: {msg}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Conflict queue on pull
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn pull_with_conflict_queues_to_conflict_table() {
    use chrono::{Duration, TimeZone};
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x77u8; 32];
    write_master_key(&relay.base_url, &master_key);

    // Local: start() a block at 12:00.
    let db = fresh_db(&env);
    let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
    let mut local_start = Entry::new(
        "focus",
        Verb::Switch,
        None,
        None,
        Some(DatumTime {
            utc: base,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
        }),
        None,
        "dev-1",
    );
    local_start.create_time = base;
    local_start.modify_time = base;
    db.insert_entry(&local_start).unwrap();

    // Peer: also start() on the same field 30 min later — double_open.
    let peer_db = Db::open_in_memory().unwrap();
    let peer_at = base + Duration::minutes(30);
    let mut peer_start = Entry::new(
        "focus",
        Verb::Switch,
        None,
        None,
        Some(DatumTime {
            utc: peer_at,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
        }),
        None,
        "dev-peer",
    );
    peer_start.create_time = peer_at;
    peer_start.modify_time = peer_at;
    peer_db.insert_entry(&peer_start).unwrap();

    let cs = datum_sync_client::ops::pack_changeset(&peer_db, 0).unwrap();
    let blob = datum_sync_client::crypto::changeset::encrypt_changeset(
        &cs,
        &master_key,
        "dev-peer",
        1,
        "datum",
    )
    .unwrap();
    relay.inject_changeset("dev-peer", 1, blob.as_bytes());

    let outcome =
        run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db).unwrap();

    assert_eq!(outcome.pulled, 1);
    assert!(
        outcome.integrity_warnings_queued >= 1,
        "expected at least one conflict queued, got {}",
        outcome.integrity_warnings_queued,
    );

    // Row is in integrity_warnings with detected_by='sync_pull'.
    let n: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM integrity_warnings WHERE detected_by = 'sync_pull'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(n >= 1, "sync_pull conflict row should exist");
}

// ──────────────────────────────────────────────────────────────────────────
// Integration: end-to-end two-device convergence + push idempotency
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn two_instance_sync_converges() {
    // Two local DBs share a master key, sync through the same relay; after
    // a handful of rounds they see the same entry set.
    let _env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    let master_key = [0x88u8; 32];
    write_master_key(&relay.base_url, &master_key);

    let path_a = std::env::var("DATUM_HOME").unwrap() + "/alpha.db";
    let path_b = std::env::var("DATUM_HOME").unwrap() + "/beta.db";
    std::fs::create_dir_all(std::env::var("DATUM_HOME").unwrap()).unwrap();
    let db_a = Db::open(&path_a).unwrap();
    let db_b = Db::open(&path_b).unwrap();

    // The `alpha.db` / `beta.db` names only exist to give the two
    // simulated devices distinct local files under one DATUM_HOME —
    // semantically this is one logical DB synced across two devices. Per
    // da-3fnd a no-cluster DB defaults its cluster to its file-name stem,
    // which would split these two into `alpha` / `beta` and stop them
    // converging; pin both to one shared cluster as a real same-DB pair
    // (or a `sync login --cluster` user) would.
    sync_state::set_cluster_id(&db_a, "shared").unwrap();
    sync_state::set_cluster_id(&db_b, "shared").unwrap();

    // Device A writes two rows.
    db_a.insert_entry(&make_entry("a-field-1", 0, "dev-A"))
        .unwrap();
    db_a.insert_entry(&make_entry("a-field-2", 60, "dev-A"))
        .unwrap();

    // Device B writes a different row.
    db_b.insert_entry(&make_entry("b-field-1", 30, "dev-B"))
        .unwrap();

    // Each device needs its own device_id — write a device_id file under
    // a per-device DATUM_HOME. Because this test shares a single
    // DATUM_HOME, we stub device_id via the existing files.
    // `datum_commands::config::device_id()` writes a fresh ULID on first
    // call and reuses it thereafter. Both DBs read the same id here,
    // which is NOT realistic but forces us to scope the two runs via
    // environment — simplification for this test: write two device_id
    // files by toggling DATUM_HOME.
    //
    // Simpler: we call run_sync_inner directly with explicit device_ids.
    use datum_cli::sync::{preflight_checks, run_sync_inner};
    let pre = preflight_checks(&relay.base_url).unwrap();
    let client = reqwest::blocking::Client::builder().build().unwrap();

    // Round 1: A pushes, B pulls.
    let _ = run_sync_inner_for_device(
        &relay.base_url,
        &client,
        &pre.token.token,
        "dev-A",
        &master_key,
        &db_a,
        Direction::PushOnly,
    );
    let _ = run_sync_inner_for_device(
        &relay.base_url,
        &client,
        &pre.token.token,
        "dev-B",
        &master_key,
        &db_b,
        Direction::PullOnly,
    );

    // Round 2: B pushes (now also has A's entries via pull), A pulls.
    let _ = run_sync_inner_for_device(
        &relay.base_url,
        &client,
        &pre.token.token,
        "dev-B",
        &master_key,
        &db_b,
        Direction::PushOnly,
    );
    let _ = run_sync_inner_for_device(
        &relay.base_url,
        &client,
        &pre.token.token,
        "dev-A",
        &master_key,
        &db_a,
        Direction::PullOnly,
    );

    let a_count = entries_count(&db_a);
    let b_count = entries_count(&db_b);
    assert_eq!(a_count, b_count, "devices should converge to same count");
    assert_eq!(a_count, 3, "all three entries converged");

    // Keep `run_sync_inner` referenced so its import isn't flagged as dead.
    let _ = run_sync_inner;
}

fn run_sync_inner_for_device(
    server: &str,
    client: &reqwest::blocking::Client,
    bearer: &str,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    direction: Direction,
) -> SyncOutcome {
    // Wrap the bare bearer in an AuthToken + Session so callers can keep
    // their existing "give me a string" signature. No on-disk persistence
    // here — the tests don't exercise refresh.
    let token = AuthToken {
        token: bearer.to_string(),
        refresh_token: "test-refresh".to_string(),
        user_id: "test-user".to_string(),
        cluster_id: None,
    };
    let session = datum_sync_client::http::Session::new(server, &token, None);
    let mut outcome = SyncOutcome::default();
    if direction.does_pull() {
        let pulled =
            datum_cli::sync::pull::run(server, client, &session, device_id, master_key, db, false)
                .unwrap();
        outcome.pulled = pulled.applied;
        outcome.integrity_warnings_queued = pulled.integrity_warnings_queued;
    }
    if direction.does_push() {
        let pushed =
            datum_cli::sync::push::run(server, client, &session, device_id, master_key, db, false)
                .unwrap();
        outcome.pushed = pushed.pushed;
    }
    outcome
}

fn entries_count(db: &Db) -> i64 {
    db.conn()
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap()
}

// ──────────────────────────────────────────────────────────────────────────
// Cluster registration (da-eu7o)
// ──────────────────────────────────────────────────────────────────────────

/// da-eu7o sub-4: on first sync the relay echoes back a server-assigned
/// cluster_id (the user's default cluster_id, currently their user_id
/// per the da-og24 transitional mapping). The client must persist it
/// into local sync_state so subsequent syncs send the same cluster_id
/// and the server's per-cluster scoping behaves predictably.
#[test]
fn first_push_registers_cluster_id_locally() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0xa1u8; 32]);
    let db = fresh_db(&env);
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();

    // Before sync the DB has no recorded cluster.
    assert_eq!(sync_state::get_cluster_id(&db).unwrap(), None);
    assert_eq!(sync_state::get_cluster_generation(&db).unwrap(), 0);

    let outcome =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    assert_eq!(outcome.pushed, 1);

    // The server's default cluster_id is the user's UUID (da-og24
    // transitional mapping). We don't assert the exact value to avoid
    // coupling tests to the server's choice — just that *some*
    // non-empty cluster_id round-tripped.
    let stored = sync_state::get_cluster_id(&db).unwrap();
    assert!(
        stored.as_deref().is_some_and(|s| !s.is_empty()),
        "expected cluster_id to be persisted, got {stored:?}",
    );
    // Generation is 0 on a freshly-registered cluster.
    assert_eq!(sync_state::get_cluster_generation(&db).unwrap(), 0);
}

/// da-eu7o sub-4: after the server bumps the cluster generation (e.g.
/// via the `/v0/clusters/{id}/bump-generation` endpoint a la da-tat1),
/// the next push from a client still sitting on the old generation must
/// be rejected with a typed error — NOT collapsed into an opaque "push
/// failed: 409" string.
#[test]
fn push_after_server_bump_surfaces_generation_mismatch_error() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0xa2u8; 32]);
    let db = fresh_db(&env);

    // First sync: registers the local cluster at generation 0.
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    let registered_cluster = sync_state::get_cluster_id(&db).unwrap().unwrap();
    assert_eq!(sync_state::get_cluster_generation(&db).unwrap(), 0);

    // Simulate a server-side cluster bump (the da-tat1 endpoint path,
    // exercised here directly against the shared AppState). The client
    // is now sitting on generation 0 but the server is on 1.
    {
        let conn = relay.state.db.lock().unwrap();
        let (new_gen, _started_at) =
            datum_sync_server::cluster::bump_generation(&conn, &registered_cluster, &relay.user_id)
                .unwrap()
                .unwrap();
        assert_eq!(new_gen, 1);
    }

    // A new local write triggers a push — the server must reject with
    // GenerationMismatch. da-eu7o surfaces it as a typed
    // SessionError::GenerationMismatch; da-ws1q sub-5 then rewrites
    // that into a "cluster forked" CLI surface at the run_sync_inner
    // boundary, so the message a user sees here is the friendly form.
    db.insert_entry(&make_entry("mood", 60, "dev-1")).unwrap();
    let err = run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db)
        .expect_err("push against bumped server must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("cluster forked"),
        "expected 'cluster forked' surface from sub-5 rewrite, got: {msg}",
    );
    assert!(
        msg.contains("generation 1") && msg.contains("sent 0"),
        "expected expected/found generations in surface, got: {msg}",
    );

    // Watermarks must NOT advance after a mismatch — peers depend on
    // never seeing a row we never committed.
    assert_eq!(
        sync_state::get_device_seq(&db).unwrap(),
        1,
        "device_seq must stay pinned on the previous push when the new push fails",
    );
}

#[test]
fn push_idempotent_on_replay() {
    // Pushing the same DB twice with no new writes is a no-op on the
    // second call — the relay doesn't see a duplicate changeset.
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, &relay.mint_token());
    write_master_key(&relay.base_url, &[0x99u8; 32]);
    let db = fresh_db(&env);
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();

    let first =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    assert_eq!(first.pushed, 1);
    assert_eq!(relay.changeset_count(), 1);

    // Replay: no new local writes → nothing to upload.
    let second =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    assert_eq!(second.pushed, 0);
    assert_eq!(relay.changeset_count(), 1, "replay must not double-push");

    // Silence unused warning on the guard placeholder in helpers.
    let _ = ChronoDuration::seconds(0);
}

// ──────────────────────────────────────────────────────────────────────────
// --db flag server resolution (da-cyn4)
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn sync_db_flag_resolves_per_db_server() {
    // da-cyn4: the sync tree used to detect the active DB from DATUM_DB
    // alone, so `datum --db work sync` with a per-DB server pinned for
    // "work" silently pushed to the GLOBAL relay. The --db flag must win.
    let env = scoped_env();
    let global_relay = StubRelay::start();
    let work_relay = StubRelay::start();
    for relay in [&global_relay, &work_relay] {
        write_token_for(&relay.base_url, &relay.mint_token());
        write_master_key(&relay.base_url, &[0x33u8; 32]);
    }
    datum_cli::sync::config::set_server(global_relay.base_url.clone(), true, None).unwrap();
    datum_cli::sync::config::set_server(work_relay.base_url.clone(), false, Some("work")).unwrap();

    let db = fresh_db(&env);
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();

    // DATUM_DB is unset (scoped_env clears it) — only the `--db work`
    // flag names the active DB.
    datum_cli::sync::dispatch(
        datum_cli::args::SyncArgs {
            command: None,
            push: true,
            pull: false,
            dry_run: false,
            server: None,
        },
        Some("work"),
        &db,
    )
    .unwrap();

    assert_eq!(
        work_relay.changeset_count(),
        1,
        "push must land on work's per-DB relay"
    );
    assert_eq!(
        global_relay.changeset_count(),
        0,
        "global relay must stay untouched under --db work"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Paging (da-5ua6): no request carries a whole history
// ──────────────────────────────────────────────────────────────────────────

/// Session over the raw bearer, mirroring `run_sync_inner_for_device` —
/// the paging tests drive `push_once_with` / `pull_once_with` directly so
/// they can shrink the page size without building a huge DB.
fn session_for(server: &str, bearer: &str) -> datum_sync_client::http::Session {
    let token = AuthToken {
        token: bearer.to_string(),
        refresh_token: "test-refresh".to_string(),
        user_id: "test-user".to_string(),
        cluster_id: None,
    };
    datum_sync_client::http::Session::new(server, &token, None)
}

/// Shrink both page sizes to `n`. Useful for driving the loops without a
/// huge DB — but note that a test built only on this never sees the
/// shipped configuration, which is how da-blcb hid: see
/// [`default_page_size_pulls_a_history_past_the_relay_row_ceiling`].
fn paged_opts(n: usize) -> datum_sync_client::sync_once::SyncOpts<'static> {
    datum_sync_client::sync_once::SyncOpts {
        push_page_entries: n,
        pull_page_rows: n,
        progress: None,
    }
}

/// The acceptance case behind the bead: a first sync of a database bigger
/// than one request completes in bounded requests, and the peer pulls it
/// down in bounded requests too — the mirror failure, not just the push
/// half.
#[test]
fn first_sync_pages_push_and_pull_in_bounded_requests() {
    use datum_sync_client::sync_once::{pull_once_with, push_once_with};

    let env = scoped_env();
    let relay = StubRelay::start();
    let bearer = relay.mint_token();
    write_token_for(&relay.base_url, &bearer);
    let master_key = [0x5au8; 32];
    write_master_key(&relay.base_url, &master_key);

    let home = std::env::var("DATUM_HOME").unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let db_a = Db::open(&format!("{home}/alpha.db")).unwrap();
    let db_b = Db::open(&format!("{home}/beta.db")).unwrap();
    // Two devices, one logical DB — pin both to one cluster (da-3fnd
    // would otherwise split them by file-name stem).
    sync_state::set_cluster_id(&db_a, "shared").unwrap();
    sync_state::set_cluster_id(&db_b, "shared").unwrap();
    let _ = &env;

    for i in 0..10 {
        db_a.insert_entry(&make_entry(&format!("f{i}"), i * 60, "dev-A"))
            .unwrap();
    }

    let client = reqwest::blocking::Client::builder().build().unwrap();
    let session = session_for(&relay.base_url, &bearer);

    // Push in pages of 3: 10 entries → 4 requests, none of them the whole
    // history.
    let pushed = push_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-A",
        &master_key,
        &db_a,
        false,
        &paged_opts(3),
    )
    .unwrap();
    assert_eq!(pushed.pushed, 10);
    assert_eq!(pushed.pages, 4, "10 entries at 3/page is 4 requests");
    assert_eq!(relay.changeset_count(), 4, "one relay row per page");

    // Pull in pages of 2 relay rows: the peer walks the same history in
    // bounded responses and lands every entry exactly once.
    let pulled = pull_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-B",
        &master_key,
        &db_b,
        false,
        &paged_opts(2),
    )
    .unwrap();
    assert_eq!(pulled.applied, 10);
    assert_eq!(pulled.pages, 2, "4 rows at 2/page is 2 requests");
    assert_eq!(entries_count(&db_b), 10, "peer converged on every entry");

    // Both watermarks are caught up: re-running either direction is a
    // no-op, so paging didn't leave a row behind or re-send one.
    let again = push_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-A",
        &master_key,
        &db_a,
        false,
        &paged_opts(3),
    )
    .unwrap();
    assert_eq!((again.pushed, again.pages), (0, 0));
    let again = pull_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-B",
        &master_key,
        &db_b,
        false,
        &paged_opts(2),
    )
    .unwrap();
    assert_eq!((again.applied, again.pages), (0, 0));
}

/// A pull page carries whole push pages, so one page can be an entire
/// first sync — page-granular progress would then say nothing for the
/// length of the run. Progress must arrive per changeset (da-j8cu).
#[test]
fn pull_reports_progress_within_a_single_page() {
    use datum_sync_client::sync_once::{pull_once_with, push_once_with, SyncOpts, SyncProgress};

    let env = scoped_env();
    let relay = StubRelay::start();
    let bearer = relay.mint_token();
    write_token_for(&relay.base_url, &bearer);
    let master_key = [0x5cu8; 32];
    write_master_key(&relay.base_url, &master_key);

    let home = std::env::var("DATUM_HOME").unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let db_a = Db::open(&format!("{home}/alpha.db")).unwrap();
    let db_b = Db::open(&format!("{home}/beta.db")).unwrap();
    sync_state::set_cluster_id(&db_a, "shared").unwrap();
    sync_state::set_cluster_id(&db_b, "shared").unwrap();
    let _ = &env;

    for i in 0..10 {
        db_a.insert_entry(&make_entry(&format!("f{i}"), i * 60, "dev-A"))
            .unwrap();
    }

    let client = reqwest::blocking::Client::builder().build().unwrap();
    let session = session_for(&relay.base_url, &bearer);

    // 10 entries at 3/page → 4 relay rows.
    push_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-A",
        &master_key,
        &db_a,
        false,
        &paged_opts(3),
    )
    .unwrap();

    // Pull them all in ONE page — the shape that used to be silent.
    let seen = std::cell::RefCell::new(Vec::new());
    let record = |p: SyncProgress| seen.borrow_mut().push(p);
    let pulled = pull_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-B",
        &master_key,
        &db_b,
        false,
        &SyncOpts {
            pull_page_rows: 50,
            progress: Some(&record),
            ..SyncOpts::default()
        },
    )
    .unwrap();
    assert_eq!((pulled.applied, pulled.pages), (10, 1));

    let events = seen.into_inner();
    let within_page: Vec<_> = events
        .iter()
        .filter_map(|p| match p {
            SyncProgress::PullChangeset {
                changeset,
                entries,
                total,
            } => Some((*changeset, *entries, *total)),
            _ => None,
        })
        .collect();
    assert_eq!(
        within_page,
        vec![(1, 3, 3), (2, 3, 6), (3, 3, 9), (4, 1, 10)],
        "every changeset reports, with a running total"
    );
}

/// A push that dies mid-history resumes at the first page that never
/// landed: completed pages persisted their watermark, so the re-run
/// re-sends nothing and skips nothing.
#[test]
fn interrupted_push_resumes_from_the_last_completed_page() {
    use datum_sync_client::sync_once::{pull_once_with, push_once_with, SyncOpts, SyncProgress};

    let env = scoped_env();
    let relay = StubRelay::start();
    let bearer = relay.mint_token();
    write_token_for(&relay.base_url, &bearer);
    let master_key = [0x5bu8; 32];
    write_master_key(&relay.base_url, &master_key);

    let home = std::env::var("DATUM_HOME").unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let db_a = Db::open(&format!("{home}/alpha.db")).unwrap();
    let db_b = Db::open(&format!("{home}/beta.db")).unwrap();
    sync_state::set_cluster_id(&db_a, "shared").unwrap();
    sync_state::set_cluster_id(&db_b, "shared").unwrap();
    let _ = &env;

    for i in 0..9 {
        db_a.insert_entry(&make_entry(&format!("f{i}"), i * 60, "dev-A"))
            .unwrap();
    }

    let client = reqwest::blocking::Client::builder().build().unwrap();
    let session = session_for(&relay.base_url, &bearer);

    // Break the run after page 2 by handing dev-A to another user on the
    // relay: the next page comes back 403 and the push errors out
    // mid-history, exactly like a dropped connection would.
    let hijack = |p: SyncProgress| {
        if let SyncProgress::PushPage { page: 2, .. } = p {
            let conn = relay.state.db.lock().unwrap();
            conn.execute(
                "UPDATE devices SET user_id = ?1 WHERE id = 'dev-A'",
                [uuid::Uuid::new_v4().to_string()],
            )
            .unwrap();
        }
    };
    let err = push_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-A",
        &master_key,
        &db_a,
        false,
        &SyncOpts {
            push_page_entries: 3,
            progress: Some(&hijack),
            ..SyncOpts::default()
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("403"),
        "expected the injected mid-run failure, got: {err}"
    );
    assert_eq!(
        relay.changeset_count(),
        2,
        "two pages landed before the break"
    );

    // Undo the hijack and re-run: the resume sends only what never
    // landed — 3 entries, one page — not the whole history again.
    {
        let conn = relay.state.db.lock().unwrap();
        conn.execute(
            "UPDATE devices SET user_id = ?1 WHERE id = 'dev-A'",
            [relay.user_id.to_string()],
        )
        .unwrap();
    }
    let resumed = push_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-A",
        &master_key,
        &db_a,
        false,
        &paged_opts(3),
    )
    .unwrap();
    assert_eq!(resumed.pushed, 3, "only the un-landed page re-sent");
    assert_eq!(relay.changeset_count(), 3);

    // And the history is whole on the far side — nothing skipped by the
    // interruption.
    let pulled = pull_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-B",
        &master_key,
        &db_b,
        false,
        &paged_opts(2),
    )
    .unwrap();
    assert_eq!(pulled.applied, 9);
    assert_eq!(entries_count(&db_b), 9);
}

/// The bug da-5ua6's paging loop shipped with: at the default page size
/// the client asked for ten times the relay's row ceiling, the relay
/// answered with its own smaller number, and the client read the short
/// page as end-of-stream. A history longer than `MAX_PULL_ROWS` relay rows
/// came down as a prefix and reported a successful sync (da-blcb).
///
/// Every other test in this section overrides both page sizes, which puts
/// client and relay in agreement by construction — the regime where the
/// loop was never broken. This one takes `SyncOpts::default()` for the
/// pull, so it exercises the only configuration that ships.
#[test]
fn default_page_size_pulls_a_history_past_the_relay_row_ceiling() {
    use datum_sync_client::sync_once::{
        pull_once_with, push_once_with, SyncOpts, SyncProgress, MAX_PULL_ROWS,
    };

    let env = scoped_env();
    let relay = StubRelay::start();
    let bearer = relay.mint_token();
    write_token_for(&relay.base_url, &bearer);
    let master_key = [0x5cu8; 32];
    write_master_key(&relay.base_url, &master_key);

    let home = std::env::var("DATUM_HOME").unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let db_a = Db::open(&format!("{home}/alpha.db")).unwrap();
    let db_b = Db::open(&format!("{home}/beta.db")).unwrap();
    sync_state::set_cluster_id(&db_a, "shared").unwrap();
    sync_state::set_cluster_id(&db_b, "shared").unwrap();
    let _ = &env;

    // One entry per push page ⇒ one relay row per entry, so the relay
    // ends up holding one row more than a single pull response may carry.
    let rows = MAX_PULL_ROWS + 1;
    for i in 0..rows {
        db_a.insert_entry(&make_entry(&format!("f{i}"), i as i64 * 60, "dev-A"))
            .unwrap();
    }

    let client = reqwest::blocking::Client::builder().build().unwrap();
    let session = session_for(&relay.base_url, &bearer);

    let pushed = push_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-A",
        &master_key,
        &db_a,
        false,
        &paged_opts(1),
    )
    .unwrap();
    assert_eq!(pushed.pushed, rows);
    assert_eq!(relay.changeset_count(), rows, "one relay row per entry");

    let seen: std::cell::RefCell<Vec<usize>> = std::cell::RefCell::new(Vec::new());
    let sink = |p: SyncProgress| {
        if let SyncProgress::PullPage { page, .. } = p {
            seen.borrow_mut().push(page);
        }
    };
    let pulled = pull_once_with(
        &relay.base_url,
        &client,
        &session,
        "dev-B",
        &master_key,
        &db_b,
        false,
        &SyncOpts {
            progress: Some(&sink),
            ..SyncOpts::default()
        },
    )
    .unwrap();

    assert_eq!(
        pulled.applied, rows,
        "the whole history, not a MAX_PULL_ROWS-row prefix reported as success",
    );
    assert_eq!(entries_count(&db_b), rows as i64);
    assert_eq!(pulled.pages, 2, "{MAX_PULL_ROWS} rows, then the remainder");
    // A second page means `dm sync`'s `page > 1` guard fires, so a long
    // pull is no longer silent.
    assert_eq!(*seen.borrow(), vec![1, 2], "per-page progress is reported");
}
