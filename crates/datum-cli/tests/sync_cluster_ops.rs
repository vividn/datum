//! Integration tests for da-ws1q (sub-5 of multi-DB epic):
//!   - `datum sync login` records the cluster_id from the auth token's
//!     user_id into local `sync_state`.
//!   - `datum db purge --cluster` bumps the cluster's generation on the
//!     server after the local wipe.
//!   - `datum db restore --cluster` bumps generation after a non-dry
//!     restore.
//!   - `datum sync` rewrites a server-side
//!     `SyncProtocolError::GenerationMismatch` into a CLI-actionable
//!     "cluster forked" error.
//!
//! Driver style mirrors `sync_push_pull.rs` and `sync_e2e.rs`: real
//! `datum-sync-server` in-process behind the `StubRelay` facade, real
//! `datum-sync-client` HTTP plumbing.

mod sync_support;

use chrono::{Duration, TimeZone, Utc};
use datum_cli::config::{auth_token_path, master_key_path, server_host_from_url};
use datum_cli::sync::{run_sync_with_client, AuthToken, Direction};
use datum_commands::args::{DbPurgeArgs, RestoreArgs};
use datum_commands::commands::backup_restore;
use datum_commands::commands::db::purge_with_io;
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::sync_state;
use datum_core::time::DatumTime;
use std::io::Cursor;
use tempfile::TempDir;

use sync_support::{env_lock, ScopedEnv, StubRelay};

// ──────────────────────────────────────────────────────────────────────────
// Shared scoped-env scaffolding (matches sync_push_pull.rs)
// ──────────────────────────────────────────────────────────────────────────

struct EnvGuard {
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

fn write_token_for(server: &str, token: &str, user_id: Option<&str>) {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host).unwrap();
    let t = datum_sync_client::PersistedAuthToken {
        wire: AuthToken {
            token: token.to_string(),
            refresh_token: "test-refresh".to_string(),
            user_id: user_id.unwrap_or("test-user").to_string(),
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
    let base = Utc.with_ymd_and_hms(2026, 5, 22, 12, 0, 0).unwrap();
    let at = base + Duration::seconds(offset_secs);
    let t = DatumTime {
        utc: at,
        offset_minutes: 0,
        timezone: Some("UTC".to_string()),
        uncertainty: None,
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

/// Set the relay URL as the global default server so
/// `cluster_ops::bump_generation_for_current_server` (and `run_sync`)
/// resolve to the test stub without a per-DB binding.
fn configure_default_server(server: &str) {
    datum_cli::sync::config::set_server(server.to_string(), true, None)
        .expect("set default sync server");
}

fn read_server_cluster_generation(relay: &StubRelay, cluster_id: &str) -> i64 {
    let conn = relay.state.db.lock().unwrap();
    conn.query_row(
        "SELECT generation FROM clusters WHERE cluster_id = ?1 AND user_id = ?2",
        rusqlite::params![cluster_id, relay.user_id.to_string()],
        |r| r.get::<_, i64>(0),
    )
    .expect("cluster row must exist after registration")
}

// ──────────────────────────────────────────────────────────────────────────
// `datum sync login` adopts cluster_id locally
// ──────────────────────────────────────────────────────────────────────────

/// da-3fnd: `sync login` with no `--cluster` flag binds `sync_state.
/// cluster_id` to the DB's file-name stem (`datum.db` → `datum`), so the
/// very first push/pull sends a stable cluster identifier instead of
/// None — and two devices on a same-named DB auto-share a cluster.
#[test]
fn sync_login_records_cluster_id_from_db_name() {
    let env = scoped_env();
    let relay = StubRelay::start();
    configure_default_server(&relay.base_url);
    let db = fresh_db(&env);

    // Stub-relay device-code loop authorizes on the 2nd poll.
    let mut prompter = datum_cli::sync::TestPrompter::new(Vec::<String>::new());

    // `run` is the production path — it logs in AND stamps the cluster.
    let args = datum_commands::args::SyncLoginArgs {
        server: Some(relay.base_url.clone()),
        cluster: None,
    };
    datum_cli::sync::login::run(args, None, &db, &mut prompter).expect("login::run should succeed");

    // fresh_db opens `<DATUM_HOME>/datum.db`, so the default cluster is
    // its file stem `datum`.
    let stored = sync_state::get_cluster_id(&db).expect("read cluster_id");
    assert_eq!(stored.as_deref(), Some("datum"));
}

/// da-3fnd: `sync login --cluster <name>` overrides the file-name
/// default — the merge/split escape hatch for putting two differently-
/// named DBs onto one cluster.
#[test]
fn sync_login_cluster_flag_overrides_db_name() {
    let env = scoped_env();
    let relay = StubRelay::start();
    configure_default_server(&relay.base_url);
    let db = fresh_db(&env);

    let mut prompter = datum_cli::sync::TestPrompter::new(Vec::<String>::new());
    let args = datum_commands::args::SyncLoginArgs {
        server: Some(relay.base_url.clone()),
        cluster: Some("shared-household".to_string()),
    };
    datum_cli::sync::login::run(args, None, &db, &mut prompter).expect("login::run should succeed");

    let stored = sync_state::get_cluster_id(&db).expect("read cluster_id");
    assert_eq!(stored.as_deref(), Some("shared-household"));
}

// ──────────────────────────────────────────────────────────────────────────
// `datum db purge --cluster` bumps server generation
// ──────────────────────────────────────────────────────────────────────────

/// End-to-end: first sync registers cluster (server gen 0), then
/// `db purge --cluster` calls the bump-generation endpoint so the
/// server is on gen 1 — peers will see GenerationMismatch on next sync.
#[test]
fn db_purge_with_cluster_flag_bumps_server_generation() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(
        &relay.base_url,
        &relay.mint_token(),
        Some(&relay.user_id.to_string()),
    );
    write_master_key(&relay.base_url, &[0xb1u8; 32]);
    configure_default_server(&relay.base_url);
    let db = fresh_db(&env);

    // Initial sync registers the cluster server-side and writes cluster_id
    // into local sync_state (via da-eu7o, sub-4 wiring).
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db)
        .expect("initial sync registers cluster");
    let cluster_id = sync_state::get_cluster_id(&db)
        .unwrap()
        .expect("cluster_id recorded by first sync");
    assert_eq!(read_server_cluster_generation(&relay, &cluster_id), 0);

    // Now purge with --cluster: local wipe + server bump.
    let mut stdin = Cursor::new(Vec::<u8>::new());
    purge_with_io(
        DbPurgeArgs {
            yes: true,
            cluster: true,
        },
        &db,
        &mut stdin,
        false,
        "",
    )
    .expect("db purge --yes --cluster");

    // Server is on generation 1 now.
    assert_eq!(read_server_cluster_generation(&relay, &cluster_id), 1);

    // Local sync_state was re-stamped post-purge so next push from this
    // device carries cluster_id + the new generation.
    assert_eq!(
        sync_state::get_cluster_id(&db).unwrap().as_deref(),
        Some(cluster_id.as_str()),
        "purge must restore cluster_id after wiping sync_state",
    );
    assert_eq!(
        sync_state::get_cluster_generation(&db).unwrap(),
        1,
        "purge must record the new generation locally",
    );
}

/// `db purge` without `--cluster` keeps the existing single-cluster
/// backward-compat: local wipe only, no HTTP call. Verified by
/// asserting the relay's recorded generation stays at 0.
#[test]
fn db_purge_without_cluster_flag_does_not_touch_server() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(
        &relay.base_url,
        &relay.mint_token(),
        Some(&relay.user_id.to_string()),
    );
    write_master_key(&relay.base_url, &[0xb2u8; 32]);
    configure_default_server(&relay.base_url);
    let db = fresh_db(&env);

    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    let cluster_id = sync_state::get_cluster_id(&db).unwrap().unwrap();
    assert_eq!(read_server_cluster_generation(&relay, &cluster_id), 0);

    let mut stdin = Cursor::new(Vec::<u8>::new());
    purge_with_io(
        DbPurgeArgs {
            yes: true,
            cluster: false,
        },
        &db,
        &mut stdin,
        false,
        "",
    )
    .expect("plain db purge --yes");

    assert_eq!(
        read_server_cluster_generation(&relay, &cluster_id),
        0,
        "plain purge must NOT bump generation (backward compat)",
    );
    // sync_state was wiped — no cluster_id locally either.
    assert_eq!(sync_state::get_cluster_id(&db).unwrap(), None);
}

// ──────────────────────────────────────────────────────────────────────────
// `datum db restore --cluster` bumps server generation
// ──────────────────────────────────────────────────────────────────────────

/// Restore --cluster: replay backup file, then bump generation. The
/// local cluster_generation tracks the new server value.
#[test]
fn db_restore_with_cluster_flag_bumps_server_generation() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(
        &relay.base_url,
        &relay.mint_token(),
        Some(&relay.user_id.to_string()),
    );
    write_master_key(&relay.base_url, &[0xb3u8; 32]);
    configure_default_server(&relay.base_url);
    let db = fresh_db(&env);

    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    let cluster_id = sync_state::get_cluster_id(&db).unwrap().unwrap();

    // Produce a backup of the current DB, then restore it back with
    // --cluster. --force keeps the (deduped) entries and exercises the
    // post-restore bump path.
    let backup_path = std::env::var("DATUM_HOME").unwrap() + "/snapshot.jsonl";
    backup_restore::cmd_backup(
        datum_commands::args::BackupArgs {
            path: backup_path.clone(),
            overwrite: true,
        },
        &db,
    )
    .expect("backup");

    backup_restore::cmd_restore(
        RestoreArgs {
            path: backup_path,
            force: true,
            dry_run: false,
            json: false,
            cluster: true,
        },
        &db,
    )
    .expect("restore --cluster");

    assert_eq!(read_server_cluster_generation(&relay, &cluster_id), 1);
    assert_eq!(sync_state::get_cluster_generation(&db).unwrap(), 1);
}

/// `db restore --cluster --dry-run` MUST NOT bump generation — dry-run
/// is supposed to be side-effect-free.
#[test]
fn db_restore_dry_run_with_cluster_does_not_bump() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(
        &relay.base_url,
        &relay.mint_token(),
        Some(&relay.user_id.to_string()),
    );
    write_master_key(&relay.base_url, &[0xb4u8; 32]);
    configure_default_server(&relay.base_url);
    let db = fresh_db(&env);

    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    let cluster_id = sync_state::get_cluster_id(&db).unwrap().unwrap();

    let backup_path = std::env::var("DATUM_HOME").unwrap() + "/dry.jsonl";
    backup_restore::cmd_backup(
        datum_commands::args::BackupArgs {
            path: backup_path.clone(),
            overwrite: true,
        },
        &db,
    )
    .unwrap();

    backup_restore::cmd_restore(
        RestoreArgs {
            path: backup_path,
            force: false,
            dry_run: true,
            json: false,
            cluster: true,
        },
        &db,
    )
    .expect("dry-run restore --cluster");

    assert_eq!(
        read_server_cluster_generation(&relay, &cluster_id),
        0,
        "dry-run must NOT bump generation",
    );
}

// ──────────────────────────────────────────────────────────────────────────
// GenerationMismatch friendly error
// ──────────────────────────────────────────────────────────────────────────

/// After the server's generation has been bumped (e.g. by another
/// device's `db purge --cluster`), a sync from a still-on-old-gen
/// client must surface a CLI-actionable "cluster forked" message — not
/// the raw protocol "generation mismatch: client X, server Y" line.
#[test]
fn sync_after_bump_emits_cluster_forked_message() {
    let env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(
        &relay.base_url,
        &relay.mint_token(),
        Some(&relay.user_id.to_string()),
    );
    write_master_key(&relay.base_url, &[0xb5u8; 32]);
    configure_default_server(&relay.base_url);
    let db = fresh_db(&env);

    // First sync registers cluster at gen 0.
    db.insert_entry(&make_entry("coffee", 0, "dev-1")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db).unwrap();
    let cluster_id = sync_state::get_cluster_id(&db).unwrap().unwrap();

    // Simulate another device bumping the server-side generation
    // (equivalent to `datum db purge --cluster` from a peer).
    {
        let conn = relay.state.db.lock().unwrap();
        datum_sync_server::cluster::bump_generation(&conn, &cluster_id, &relay.user_id)
            .unwrap()
            .unwrap();
    }

    // Next push: client is on gen 0, server on gen 1 → mismatch.
    db.insert_entry(&make_entry("mood", 60, "dev-1")).unwrap();
    let err = run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db)
        .expect_err("push against bumped server must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("cluster forked"),
        "expected 'cluster forked' surface, got: {msg}",
    );
    assert!(
        msg.contains("datum db purge") && msg.contains("datum sync"),
        "expected re-init instructions, got: {msg}",
    );
}
