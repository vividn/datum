//! Acceptance tests for `datum sync` push / pull / bidirectional / status
//! (bead da-ndd.14 = C4c).
//!
//! Tests mutate `DATUM_HOME` and `DATUM_CONFIG_HOME` so they're serialised
//! through the `ENV_LOCK`. Each test constructs its own stub relay which
//! exposes the wire protocol from `docs/sync-protocol.md` (`/v1/push`,
//! `/v1/pull`, `/v1/keys/wrapped`, `/v1/auth/device-code*`).

mod sync_support;

use chrono::{Duration as ChronoDuration, Utc};
use datum_cli::config::{auth_token_path, master_key_path, server_host_from_url};
use datum_cli::sync::{run_sync_with_client, AuthToken, Direction, SyncOutcome};
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use datum_sync_client::sync_state;
use tempfile::TempDir;

use sync_support::{StubRelay, ENV_LOCK};

struct EnvGuard {
    // Fields drop in declaration order: restore env vars first, then
    // release the mutex. Putting `_guard` last means no other test can
    // observe a partially-restored environment.
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
        timezone: "UTC".to_string(),
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

    // push_seq advanced past the entries' modify_time.
    assert!(sync_state::get_push_seq(&db).unwrap() > 0);
    assert_eq!(sync_state::get_my_push_seq(&db).unwrap(), 1);

    // A second push with no new rows sends nothing.
    let again =
        run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    assert_eq!(again.pushed, 0);
    // Counter didn't tick because nothing was uploaded.
    assert_eq!(sync_state::get_my_push_seq(&db).unwrap(), 1);
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
    let blob =
        datum_sync_client::crypto::changeset::encrypt_changeset(&cs, &master_key, "dev-peer", 1)
            .unwrap();
    relay.inject_changeset("dev-peer", 1, blob.as_bytes());

    // Local has its own entry.
    let db = fresh_db(&env);
    db.insert_entry(&make_entry("local-field", 0, "dev-1"))
        .unwrap();

    let outcome = run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db).unwrap();

    assert_eq!(outcome.pulled, 1, "should pull 1 peer change");
    // After pulling the peer entry, a fresh pack sees both rows in the DB.
    // Since push_seq starts at 0, BOTH rows go up the wire.
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
    assert_eq!(sync_state::get_push_seq(&db).unwrap(), 0);
    assert_eq!(sync_state::get_my_push_seq(&db).unwrap(), 0);
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
    datum_cli::sync::status::run(datum_cli::args::SyncStatusArgs {
        server: Some(relay.base_url.clone()),
    })
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
        msg.contains("not logged in"),
        "expected not-logged-in error, got: {msg}"
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
            timezone: "UTC".to_string(),
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
            timezone: "UTC".to_string(),
        }),
        None,
        "dev-peer",
    );
    peer_start.create_time = peer_at;
    peer_start.modify_time = peer_at;
    peer_db.insert_entry(&peer_start).unwrap();

    let cs = datum_sync_client::ops::pack_changeset(&peer_db, 0).unwrap();
    let blob =
        datum_sync_client::crypto::changeset::encrypt_changeset(&cs, &master_key, "dev-peer", 1)
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
/// via the `/v1/clusters/{id}/bump-generation` endpoint a la da-tat1),
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
        let new_gen =
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
        sync_state::get_my_push_seq(&db).unwrap(),
        1,
        "my_push_seq must stay pinned on the previous push when the new push fails",
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
