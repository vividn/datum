//! End-to-end test capstone for the cluster + multi-DB-per-user epic
//! (da-u6lm, sub-bead 6 / da-35mz).
//!
//! Sub-beads 1–5 landed the wire protocol, server schema, server
//! cluster ops, client cluster registration, and CLI surface (`sync
//! login` cluster adoption, `db purge --cluster`, `db restore
//! --cluster`, GenerationMismatch UX). This file exercises the full
//! flow against a live in-process relay with two distinct local
//! installations ("client A" and "client B").
//!
//! Scenario coverage (matches da-35mz's acceptance):
//! 1. Two-client baseline: both register, push, pull; the two row
//!    sets converge, column for column, under one shared cluster_id.
//! 2. Purge propagation: A's `db purge --cluster` bumps server
//!    generation; B's next sync surfaces the "cluster forked"
//!    message — not the raw protocol mismatch.
//! 3. Restore propagation: same shape, driven through the
//!    `cluster_ops::bump_generation_for_current_server` seam the
//!    `db restore --cluster` command uses internally.
//! 4. Multi-DB-per-user: two DBs under the same user record distinct
//!    cluster_ids on the server; pushes to one don't surface in the
//!    other.
//! 5. Re-init after fork: B follows the cluster-forked recovery
//!    (`db purge` without `--cluster`, then `sync`) and converges
//!    on the post-purge state.
//! 7. The generation gate (da-pjhg): a changeset written under a
//!    superseded generation is never applied, whichever route it
//!    arrives by — so `db purge --cluster` leaves both devices empty,
//!    a batch straddling a bump neither applies its old rows nor
//!    rewinds the device's generation, and a restore-bump reconciles
//!    to the restored snapshot instead of re-merging what preceded it.
//!
//! Convergence here is asserted on row CONTENT, not row count
//! (da-0kmn). `live_rows` says two devices hold the same NUMBER of
//! rows; `assert_same_content` says they hold the same ROWS — every
//! column of every live entry, keyed on `hid`. Loss shows up in a
//! count and corruption does not, and corruption is half the question
//! this suite exists to answer. Scenarios that expect the two sides to
//! DIFFER assert the specific difference instead (the pull changed
//! nothing; the one row that crossed is the other device's own,
//! unchanged). The file's own control,
//! `content_comparison_sees_the_divergence_counts_and_hashes_cannot`,
//! is the pair of databases that satisfies every count *and* every
//! content_hash while holding materially different data.
//!
//! Each "client" gets its own DATUM_HOME (so the auto-managed
//! `device_id` file and the per-host token/master-key live on
//! separate disks). Both DATUM_HOMEs point at the same in-process
//! relay and authenticate as the same user — that's the shape the
//! cluster system was built for.

mod sync_content;
mod sync_support;

use chrono::{Duration, TimeZone, Utc};
use datum_cli::config::{auth_token_path, master_key_path, server_host_from_url};
use datum_cli::sync::{run_sync_with_client, AuthToken, Direction};
use datum_commands::args::DbPurgeArgs;
use datum_commands::commands::db::purge_with_io;
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::sync_state;
use datum_core::time::DatumTime;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use sync_content::{assert_same_content, live_content, live_fields};
use sync_support::{env_lock, StubRelay};

// ──────────────────────────────────────────────────────────────────────────
// Two-client env scaffolding — each "device" has its own DATUM_HOME /
// DATUM_CONFIG_HOME so the device_id, token, and master_key are isolated.
// One shared env_lock guards all env mutation per test (cargo runs tests across
// threads, so we serialise on the same mutex sync_cluster_ops.rs uses).
// ──────────────────────────────────────────────────────────────────────────

struct ClientEnv {
    _tmp: TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
    home_a: PathBuf,
    cfg_a: PathBuf,
    home_b: PathBuf,
    cfg_b: PathBuf,
    /// A third install, used only by the scenarios that need a device
    /// joining a cluster *after* the others have been syncing — the
    /// honest way to measure what the relay actually holds.
    home_c: PathBuf,
    cfg_c: PathBuf,
    orig_home: Option<String>,
    orig_cfg_home: Option<String>,
    orig_db: Option<String>,
    orig_cfg: Option<String>,
}

impl ClientEnv {
    fn new() -> Self {
        let guard = env_lock();
        let tmp = TempDir::new().expect("mk tmp");
        let orig_home = std::env::var("DATUM_HOME").ok();
        let orig_cfg_home = std::env::var("DATUM_CONFIG_HOME").ok();
        let orig_db = std::env::var("DATUM_DB").ok();
        let orig_cfg = std::env::var("DATUM_CONFIG").ok();
        std::env::remove_var("DATUM_DB");
        std::env::remove_var("DATUM_CONFIG");
        let home_a = tmp.path().join("device-A").join("data");
        let cfg_a = tmp.path().join("device-A").join("config");
        let home_b = tmp.path().join("device-B").join("data");
        let cfg_b = tmp.path().join("device-B").join("config");
        let home_c = tmp.path().join("device-C").join("data");
        let cfg_c = tmp.path().join("device-C").join("config");
        for dir in [&home_a, &cfg_a, &home_b, &cfg_b, &home_c, &cfg_c] {
            std::fs::create_dir_all(dir).unwrap();
        }
        Self {
            _tmp: tmp,
            _guard: guard,
            home_a,
            cfg_a,
            home_b,
            cfg_b,
            home_c,
            cfg_c,
            orig_home,
            orig_cfg_home,
            orig_db,
            orig_cfg,
        }
    }

    /// Point all CLI lookups at device A's directories until the next
    /// `use_b()`. Token, master_key, device_id, server config — all
    /// resolved against `home_a` / `cfg_a` for the duration.
    fn use_a(&self) {
        std::env::set_var("DATUM_HOME", &self.home_a);
        std::env::set_var("DATUM_CONFIG_HOME", &self.cfg_a);
    }

    fn use_b(&self) {
        std::env::set_var("DATUM_HOME", &self.home_b);
        std::env::set_var("DATUM_CONFIG_HOME", &self.cfg_b);
    }

    fn use_c(&self) {
        std::env::set_var("DATUM_HOME", &self.home_c);
        std::env::set_var("DATUM_CONFIG_HOME", &self.cfg_c);
    }
}

impl Drop for ClientEnv {
    fn drop(&mut self) {
        for (key, prev) in [
            ("DATUM_HOME", &self.orig_home),
            ("DATUM_CONFIG_HOME", &self.orig_cfg_home),
            ("DATUM_DB", &self.orig_db),
            ("DATUM_CONFIG", &self.orig_cfg),
        ] {
            match prev {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn write_token_for(server: &str, token: &str, user_id: &str) {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host).unwrap();
    let t = datum_sync_client::PersistedAuthToken {
        wire: AuthToken {
            token: token.to_string(),
            refresh_token: "test-refresh".to_string(),
            user_id: user_id.to_string(),
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

fn configure_default_server(server: &str) {
    datum_cli::sync::config::set_server(server.to_string(), true, None)
        .expect("set default sync server");
}

/// Provision an isolated client install: token + master_key keyed on
/// the shared relay's URL, default server pointed at the relay.
/// Returns the fresh DB at `<DATUM_HOME>/datum.db`.
///
/// `master_key` must be IDENTICAL across both clients in a scenario —
/// it's the user's recovery-passphrase-derived key, and a second
/// device joins by unwrapping the same wrapped copy from the relay.
/// Distinct device_id / token / state stay isolated via their
/// per-DATUM_HOME paths.
fn provision_client(env: &ClientEnv, which: &str, relay: &StubRelay, master_key: &[u8; 32]) -> Db {
    match which {
        "A" => env.use_a(),
        "B" => env.use_b(),
        "C" => env.use_c(),
        other => panic!("unknown client tag: {other}"),
    }
    write_token_for(
        &relay.base_url,
        &relay.mint_token(),
        &relay.user_id.to_string(),
    );
    write_master_key(&relay.base_url, master_key);
    configure_default_server(&relay.base_url);
    open_db(Path::new(&std::env::var("DATUM_HOME").unwrap()))
}

fn open_db(home: &Path) -> Db {
    let path = home.join("datum.db");
    Db::open(path.to_str().unwrap()).unwrap()
}

/// Open a DB under `home` with a specific file name, so its file stem
/// drives the da-3fnd default cluster (e.g. `alpha.db` → cluster
/// `alpha`). Used by the merge-flow scenario where the two devices'
/// DBs deliberately differ in name.
fn open_named_db(home: &Path, file: &str) -> Db {
    let path = home.join(file);
    Db::open(path.to_str().unwrap()).unwrap()
}

/// The fixture row every scenario syncs around.
///
/// It deliberately carries substance in the columns the `content_hash`
/// does NOT cover — the zone and its offset, the occur-time uncertainty,
/// the location triple, `raw_input`. The hash preimage is seven fields
/// (`field · occur · dur · state · data · following_state ·
/// deleted-marker`, docs/sync-protocol.md §Content hash); everything else
/// on the row rides outside it. A fixture that left those columns empty
/// would let [`assert_same_content`] agree for the same reason
/// [`live_hashes`] does, and measure nothing the hash had not already
/// settled.
fn make_entry(field: &str, offset_secs: i64, device: &str) -> Entry {
    let base = Utc.with_ymd_and_hms(2026, 5, 22, 12, 0, 0).unwrap();
    let at = base + Duration::seconds(offset_secs);
    // A real zone, not UTC, so `occur_time_offset` and `occur_time_tz`
    // both hold a value that a mangled round-trip could lose.
    let mut t = DatumTime::from_utc_and_tz(at, "America/Chicago").expect("known zone");
    t.uncertainty = Some("PT1M".to_string());
    let mut e = Entry::new(
        field,
        Verb::Point,
        Some(serde_json::json!(1)),
        Some(serde_json::json!({ "cup": { "v": 250, "u": "ml" } })),
        Some(t),
        None,
        device,
    );
    e.lat = Some(41.8781);
    e.lon = Some(-87.6298);
    e.location = Some("Chicago".to_string());
    e.raw_input = Some(format!("{field} 1"));
    // Set, not left unset: the `-L/--last-state=<v>` assertion the user
    // typed rides the wire and must survive apply on the peer (da-nwci).
    // It is outside the content hash, so only `assert_same_content` sees
    // it go missing.
    e.user_asserted_last_state = Some(serde_json::json!("probe"));
    e.create_time = at;
    e.modify_time = at;
    e
}

fn live_rows(db: &Db) -> i64 {
    db.conn()
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn live_hashes(db: &Db) -> Vec<String> {
    let mut stmt = db
        .conn()
        .prepare("SELECT content_hash FROM entries WHERE deleted_at IS NULL ORDER BY content_hash")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

/// The one live row whose `field` is `field`, in full. Panics unless
/// exactly one matches — the scenarios that assert a *difference* between
/// two devices name the row they mean, and a helper that quietly picked
/// the first of several would let the assertion drift off its subject.
fn row_for(db: &Db, field: &str) -> serde_json::Value {
    let mut matches: Vec<serde_json::Value> = live_content(db)
        .into_iter()
        .filter(|row| row["field"].as_str() == Some(field))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one live `{field}` row, found {}",
        matches.len(),
    );
    matches.pop().unwrap()
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

fn cluster_count_on_server(relay: &StubRelay) -> i64 {
    let conn = relay.state.db.lock().unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM clusters WHERE user_id = ?1",
        rusqlite::params![relay.user_id.to_string()],
        |r| r.get(0),
    )
    .unwrap()
}

// ──────────────────────────────────────────────────────────────────────────
// THE CONTROL — the divergence a count, and a hash, cannot see
// ──────────────────────────────────────────────────────────────────────────

/// Two devices, one row each, same `hid`, same count, same
/// `content_hash` — and a different zone, offset, uncertainty and
/// location. That is corruption: the row is still there, still the same
/// entry, and no longer says the same thing. Every count in this file
/// passes on it, and so does the hash the changeset itself agrees on,
/// because none of those four columns are in the hash preimage
/// (docs/sync-protocol.md §Content hash).
///
/// This is why [`live_content`] exists, and the guard that keeps it
/// honest: if the projection ever narrowed back to what the hash already
/// covers, this test is what would notice.
#[test]
fn content_comparison_sees_the_divergence_counts_and_hashes_cannot() {
    let tmp = TempDir::new().expect("mk tmp");
    let db_a = open_named_db(tmp.path(), "a.db");
    let db_b = open_named_db(tmp.path(), "b.db");

    let entry = make_entry("coffee", 0, "dev-A");
    db_a.insert_entry(&entry).unwrap();

    // The same row as it would look after a lossy round-trip: same
    // identity, same hashed substance, four columns quietly changed.
    let mut mangled = entry.clone();
    mangled.location = Some("Berlin".to_string());
    let occur = mangled.occur_time.as_mut().unwrap();
    occur.timezone = Some("Europe/Berlin".to_string());
    occur.offset_minutes = 120;
    occur.uncertainty = None;
    db_b.insert_entry(&mangled).unwrap();

    assert_eq!(live_rows(&db_a), live_rows(&db_b), "the counts agree");
    assert_eq!(
        live_hashes(&db_a),
        live_hashes(&db_b),
        "and so do the content_hashes",
    );

    let (a, b) = (live_content(&db_a), live_content(&db_b));
    assert_eq!(
        a[0]["hid"], b[0]["hid"],
        "one entry, not two — so this is \
         corruption of a row rather than a row gone missing"
    );
    assert_ne!(a, b, "and the content comparison is what sees it");
    assert_ne!(a[0]["location"], b[0]["location"]);
    assert_ne!(a[0]["occur_time"], b[0]["occur_time"]);
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 1 — two-client baseline
// ──────────────────────────────────────────────────────────────────────────

/// Both clients register the same cluster on first sync, push their own
/// writes, and pull each other's. The post-cycle hash sets are
/// identical and the relay tracks one cluster row (gen 0) for the
/// shared user.
#[test]
fn s1_two_client_baseline_converges_under_one_cluster() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x11u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    env.use_a();
    db_a.insert_entry(&make_entry("coffee", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).expect("A push");

    env.use_b();
    db_b.insert_entry(&make_entry("mood", 60, "dev-B")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_b).expect("B push");

    env.use_a();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_a).expect("A pull");
    env.use_b();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_b).expect("B pull");

    assert_eq!(live_rows(&db_a), 2, "A: own + B's row");
    assert_eq!(live_rows(&db_b), 2, "B: own + A's row");
    assert_same_content("A", &db_a, "B", &db_b);

    let cid_a = sync_state::get_cluster_id(&db_a).unwrap().unwrap();
    let cid_b = sync_state::get_cluster_id(&db_b).unwrap().unwrap();
    assert_eq!(cid_a, cid_b, "shared user → shared cluster_id");
    assert_eq!(
        cluster_count_on_server(&relay),
        1,
        "one cluster row on the server for one user",
    );
    assert_eq!(read_server_cluster_generation(&relay, &cid_a), 0);
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 2 — purge propagation
// ──────────────────────────────────────────────────────────────────────────

/// Client A runs `db purge --cluster`: local wipe + server bumps to
/// generation 1. Client B (still on generation 0) sees the actionable
/// "cluster forked" surface on its next sync — not the raw "generation
/// mismatch" line the underlying protocol speaks.
#[test]
fn s2_purge_cluster_propagates_via_generation_mismatch() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x21u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    // Baseline: A pushes a row, B pulls it.
    env.use_a();
    db_a.insert_entry(&make_entry("coffee", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    env.use_b();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_b).unwrap();
    assert_eq!(live_rows(&db_b), 1, "B pulled A's row");
    assert_same_content("A", &db_a, "B", &db_b);

    let cluster_id = sync_state::get_cluster_id(&db_a).unwrap().unwrap();
    assert_eq!(read_server_cluster_generation(&relay, &cluster_id), 0);

    // A purges with --cluster → server now on gen 1, A's local sync_state
    // re-stamped to gen 1.
    env.use_a();
    let mut stdin = Cursor::new(Vec::<u8>::new());
    purge_with_io(
        DbPurgeArgs {
            yes: true,
            cluster: true,
        },
        &db_a,
        &mut stdin,
        false,
        "",
    )
    .expect("A db purge --yes --cluster");
    assert_eq!(read_server_cluster_generation(&relay, &cluster_id), 1);
    assert_eq!(sync_state::get_cluster_generation(&db_a).unwrap(), 1);

    // B is still on gen 0. Next push surfaces "cluster forked" with
    // re-init instructions.
    env.use_b();
    db_b.insert_entry(&make_entry("mood", 60, "dev-B")).unwrap();
    let err = run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_b)
        .expect_err("B's push against bumped server must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("cluster forked"),
        "expected 'cluster forked' surface, got: {msg}",
    );
    assert!(
        msg.contains("datum db purge") && msg.contains("datum sync"),
        "expected re-init guidance, got: {msg}",
    );
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 3 — restore propagation
// ──────────────────────────────────────────────────────────────────────────

/// `db restore --cluster` follows the same propagation shape as
/// purge: it bumps server generation, and peers on the old generation
/// see "cluster forked" on next sync. Driven through the bump seam
/// the restore command itself uses, because the restore's
/// backup/restore plumbing is independently exercised by
/// `sync_cluster_ops.rs`; here the peer-side observation is the
/// load-bearing assertion.
#[test]
fn s3_restore_cluster_propagates_via_generation_mismatch() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x31u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    env.use_a();
    db_a.insert_entry(&make_entry("coffee", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    env.use_b();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_b).unwrap();
    assert_same_content("A", &db_a, "B", &db_b);
    let cluster_id = sync_state::get_cluster_id(&db_a).unwrap().unwrap();

    // Simulate `db restore --cluster`'s server-side effect from A.
    env.use_a();
    let new_gen = datum_commands::commands::sync::cluster_ops::bump_generation_for_current_server(
        &cluster_id,
    )
    .expect("server bump");
    assert_eq!(new_gen, 1);
    assert_eq!(read_server_cluster_generation(&relay, &cluster_id), 1);

    env.use_b();
    db_b.insert_entry(&make_entry("mood", 60, "dev-B")).unwrap();
    let err = run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_b)
        .expect_err("B's push must error after server bump");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("cluster forked"),
        "expected 'cluster forked' surface, got: {msg}",
    );
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 4 — multi-DB-per-user isolation
// ──────────────────────────────────────────────────────────────────────────

/// One user, two distinct local DBs each stamped with its own
/// cluster_id (simulating the future `sync login --cluster <id>`
/// adoption flag). The server registers two cluster rows under the
/// single user, and a pull on DB1 does not surface DB2's data.
///
/// We use a single DATUM_HOME here because the cluster_id is what
/// disambiguates — both DBs share device_id without conflict because
/// the server tracks watermarks per (cluster_id, device_id) pair.
#[test]
fn s4_multi_db_per_user_isolates_clusters() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x41u8; 32];
    // Re-purpose the two DATUM_HOMEs as two "DB installs" for the same
    // user — each has its own device_id file, so the per-(cluster_id,
    // device_id) watermark on the relay doesn't collide. Each DB adopts
    // a distinct cluster_id via the `sync login --cluster <id>` record
    // (da-3fnd); the server has supported N:1 since sub-2.
    let db_personal = provision_client(&env, "A", &relay, &key);
    let db_work = provision_client(&env, "B", &relay, &key);
    sync_state::set_cluster_id(&db_personal, "personal-cluster").unwrap();
    sync_state::set_cluster_id(&db_work, "work-cluster").unwrap();

    env.use_a();
    db_personal
        .insert_entry(&make_entry("coffee", 0, "dev-personal"))
        .unwrap();
    run_sync_with_client(
        Some(&relay.base_url),
        Direction::PushOnly,
        false,
        &db_personal,
    )
    .expect("personal push");

    env.use_b();
    db_work
        .insert_entry(&make_entry("standup", 60, "dev-work"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_work)
        .expect("work push");

    assert_eq!(
        cluster_count_on_server(&relay),
        2,
        "two distinct cluster_ids → two rows on the server",
    );
    assert_eq!(
        read_server_cluster_generation(&relay, "personal-cluster"),
        0
    );
    assert_eq!(read_server_cluster_generation(&relay, "work-cluster"), 0);

    // Pull on the work DB — must NOT see personal's row. The relay's
    // pull-handler filters by (user_id, cluster_id), so even though
    // both DBs auth as the same user, the cluster_id segregates the
    // changesets.
    //
    // The scenario expects the two sides to DIFFER, so the assertion is
    // the specific difference rather than a count: the pull left the
    // work DB exactly as it found it, byte for byte, and nothing of
    // personal's crossed over.
    env.use_b();
    let work_before = live_content(&db_work);
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_work)
        .expect("work pull");
    assert_eq!(
        live_content(&db_work),
        work_before,
        "a pull on the work cluster must change nothing: it holds its own \
         standup row untouched and none of personal's",
    );
    assert_eq!(
        live_fields(&db_work),
        vec!["standup".to_string()],
        "and the row it holds is still its own",
    );
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 6 — db-name default + merge via --cluster (da-3fnd)
// ──────────────────────────────────────────────────────────────────────────

/// The da-3fnd payoff, end-to-end:
///   1. Two devices, two differently-named DBs (`alpha.db`, `beta.db`).
///      With zero config each defaults its cluster to its file stem, so
///      the two stay isolated — neither sees the other's rows.
///   2. The merge escape hatch: device B re-binds beta.db to cluster
///      `alpha` (exactly what `sync login --cluster alpha` records via
///      `set_cluster_id`), then pulls and converges on A's data.
#[test]
fn s6_db_name_default_isolates_then_cluster_override_merges() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x61u8; 32];

    // provision_client wires token/master_key/server for each DATUM_HOME;
    // we ignore its datum.db handle and open distinctly-named DBs so the
    // file-stem default is what segregates the two devices.
    let _ = provision_client(&env, "A", &relay, &key);
    let _ = provision_client(&env, "B", &relay, &key);
    let db_alpha = open_named_db(&env.home_a, "alpha.db");
    let db_beta = open_named_db(&env.home_b, "beta.db");

    // Each device pushes under its file-name default cluster — no flag,
    // no prior cluster recorded.
    env.use_a();
    db_alpha
        .insert_entry(&make_entry("coffee", 0, "dev-alpha"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_alpha)
        .expect("alpha push");
    env.use_b();
    db_beta
        .insert_entry(&make_entry("standup", 60, "dev-beta"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_beta)
        .expect("beta push");

    // The file-stem defaults landed verbatim, server-side and locally.
    assert_eq!(
        cluster_count_on_server(&relay),
        2,
        "two differently-named DBs → two clusters",
    );
    assert_eq!(
        sync_state::get_cluster_id(&db_alpha).unwrap().as_deref(),
        Some("alpha"),
    );
    assert_eq!(
        sync_state::get_cluster_id(&db_beta).unwrap().as_deref(),
        Some("beta"),
    );

    // Isolation: beta pulling its own cluster never sees alpha's coffee.
    // Stated as the specific difference — the pull left beta exactly as
    // it was — rather than as the absence of one field name.
    env.use_b();
    let beta_before = live_content(&db_beta);
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_beta)
        .expect("beta pull pre-merge");
    assert_eq!(
        live_content(&db_beta),
        beta_before,
        "beta's own cluster holds nothing of alpha's, so the pull changes nothing",
    );

    // Merge: re-bind beta.db to cluster `alpha` (the `sync login
    // --cluster alpha` record) and pull. Beta keeps its own standup row
    // and now also pulls alpha's coffee → the two DBs are merged.
    sync_state::set_cluster_id(&db_beta, "alpha").unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_beta)
        .expect("beta pull post-merge");
    let merged = live_fields(&db_beta);
    assert!(
        merged.contains(&"coffee".to_string()) && merged.contains(&"standup".to_string()),
        "after --cluster alpha, beta holds both rows; got {merged:?}",
    );
    assert_eq!(
        row_for(&db_beta, "coffee"),
        row_for(&db_alpha, "coffee"),
        "and the coffee row it gained is alpha's own, unchanged in transit",
    );
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 5 — re-init after fork
// ──────────────────────────────────────────────────────────────────────────

/// B observes the cluster-forked error from S2's shape and follows
/// the recovery the CLI message prescribes: `datum db purge` (without
/// `--cluster`) wipes local sync_state, then `datum sync` re-registers
/// and pulls the post-purge cluster state. After the recovery B's
/// row set matches the post-purge state A wrote.
#[test]
fn s5_peer_recovers_from_fork_via_purge_then_sync() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x51u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    // Baseline + fork (S2's shape).
    env.use_a();
    db_a.insert_entry(&make_entry("coffee", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    env.use_b();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_b).unwrap();

    env.use_a();
    let mut stdin = Cursor::new(Vec::<u8>::new());
    purge_with_io(
        DbPurgeArgs {
            yes: true,
            cluster: true,
        },
        &db_a,
        &mut stdin,
        false,
        "",
    )
    .unwrap();
    // The relay tracks per-device seq watermarks independent of cluster
    // generation — `purge --cluster` resets local sync_state but the
    // server still expects the next push from A's device_id to claim
    // seq `prior_seq+1`, not `1`. A real-world re-init from purge mirrors
    // a fresh install, so drop A's device_id file so the next push lands
    // under a brand-new identifier and matches the server's "first push
    // from this device" assumption.
    let device_id_file_a = env.home_a.join("device_id");
    if device_id_file_a.exists() {
        std::fs::remove_file(&device_id_file_a).unwrap();
    }
    // A writes a post-purge row + pushes — server holds only the new
    // row under generation 1.
    db_a.insert_entry(&make_entry("post-purge", 30, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();

    // B confirms it's wedged on the next sync. A plain pull triggers
    // the generation check on the relay side even with no local edits,
    // so the cluster-forked surface fires on the first contact.
    env.use_b();
    let err = run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_b)
        .expect_err("B must error before the recovery flow");
    assert!(
        format!("{err:#}").contains("cluster forked"),
        "pre-condition: B sees cluster-forked",
    );

    // Apply the prescribed recovery: plain purge (no --cluster) wipes
    // local sync_state, then sync re-registers and pulls. To mirror a
    // fresh re-install we also reset the host's device_id file —
    // the plain purge is DB-scoped and doesn't touch DATUM_HOME, so
    // any per-device seq watermark the relay accrued from B's prior
    // pulls would otherwise wedge the recovery push. Production
    // users follow this path implicitly when the recovery message
    // suggests "back up local-only work" (full reset workflow).
    let mut stdin = Cursor::new(Vec::<u8>::new());
    purge_with_io(
        DbPurgeArgs {
            yes: true,
            cluster: false,
        },
        &db_b,
        &mut stdin,
        false,
        "",
    )
    .expect("plain purge on B");
    assert_eq!(
        sync_state::get_cluster_id(&db_b).unwrap(),
        None,
        "plain purge wipes B's sync_state",
    );
    let device_id_file = env.home_b.join("device_id");
    if device_id_file.exists() {
        std::fs::remove_file(&device_id_file).unwrap();
    }
    // The recovery UX the bead leaves "TBD" (a future `datum sync
    // re-init` or equivalent) must teach this device the current
    // generation before the next sync — without it, B sends gen 0 to a
    // server on gen 1 and the cluster-forked error fires again. Pre-
    // stamp the generation here as the placeholder for that UX; the
    // assertion below verifies pull_once also re-stamps the cluster_id
    // from the server's echo, which is the real "re-init" behaviour
    // the sub-1 protocol design baked in.
    sync_state::set_cluster_generation(&db_b, 1).unwrap();

    // Pull-only recovery: with sync_state wiped the cluster_id re-
    // registers from user_id on first server contact (pull echoes it
    // back), and A's post-purge row downloads. A subsequent Both
    // cycle would push B's freshly-pulled rows back, exercising the
    // sync-amplification edge that this E2E shouldn't double up on
    // — the load-bearing recovery assertion is the data convergence
    // and the cluster_id re-stamp.
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_b)
        .expect("B re-syncs after recovery");
    let b_fields = live_fields(&db_b);
    // da-pjhg: the relay still holds A's pre-purge changesets — it
    // garbage-collects nothing — so B is offered them and must decline.
    // A changeset written under a superseded generation is dead state;
    // applying it merges back exactly what the purge discarded. B ends
    // on A's post-purge row set, which is what "re-init" means.
    assert!(
        b_fields.contains(&"post-purge".to_string()),
        "B must see A's post-purge row after recovery; got {b_fields:?}",
    );
    assert!(
        !b_fields.contains(&"coffee".to_string()),
        "B must NOT re-acquire A's pre-purge row: it was written under \
         the generation the purge retired; got {b_fields:?}",
    );
    assert_same_content("A", &db_a, "B", &db_b);
    assert_eq!(
        sync_state::get_cluster_id(&db_b).unwrap().as_deref(),
        Some("datum"),
        "B re-registers under the client-side default cluster_id \
         (da-3fnd: the DB file-name stem, here datum.db → datum)",
    );
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 7 — the generation gate (da-pjhg)
// ──────────────────────────────────────────────────────────────────────────

/// vividn's scenario, end to end: two devices holding each other's rows,
/// `db purge --cluster` on one of them, then a sync on each. Both must
/// end up empty. The purge says "this cluster's contents are gone"; a
/// device that pulls the pre-bump changesets back has not purged
/// anything, it has round-tripped its data through the relay.
#[test]
fn s7_purge_cluster_leaves_both_devices_empty_after_sync() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x71u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    // Baseline: a row on each device, both sides converged.
    env.use_a();
    db_a.insert_entry(&make_entry("coffee", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    env.use_b();
    db_b.insert_entry(&make_entry("mood", 60, "dev-B")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_b).unwrap();
    env.use_a();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_a).unwrap();
    assert_eq!(live_rows(&db_a), 2, "pre-condition: A holds both rows");
    assert_eq!(live_rows(&db_b), 2, "pre-condition: B holds both rows");
    assert_same_content("A", &db_a, "B", &db_b);

    // A purges the cluster.
    let mut stdin = Cursor::new(Vec::<u8>::new());
    purge_with_io(
        DbPurgeArgs {
            yes: true,
            cluster: true,
        },
        &db_a,
        &mut stdin,
        false,
        "",
    )
    .expect("A db purge --yes --cluster");

    // A syncs. The relay still holds B's pre-bump changeset and offers
    // it; A must not take it back.
    let outcome_a = run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_a)
        .expect("A syncs after its own purge");
    assert_eq!(
        live_rows(&db_a),
        0,
        "A purged the cluster and must stay empty; got {:?}",
        live_fields(&db_a),
    );
    assert!(
        outcome_a.superseded_skipped > 0,
        "A should report the superseded rows it declined, not drop them silently",
    );

    // B syncs. It is still on the old generation, so this is the
    // recovery path: discard local pre-reinit state, adopt the new
    // generation, re-pull — and decline everything the old generation
    // wrote.
    env.use_b();
    let mut prompter = datum_cli::sync::TestPrompter::new::<Vec<String>, String>(vec![]);
    prompter.push_confirm(true);
    datum_cli::sync::run_sync_with_client_and_prompter(
        Some(&relay.base_url),
        Direction::Both,
        false,
        &db_b,
        &mut prompter,
    )
    .expect("B recovers and syncs");
    assert_eq!(
        live_rows(&db_b),
        0,
        "B must converge on the purged cluster's state (empty); got {:?}",
        live_fields(&db_b),
    );
    assert_same_content("A", &db_a, "B", &db_b);
}

/// The relay filters a pull by cluster, device and user — never by
/// generation — so one batch can straddle a bump. Two things must hold
/// for such a batch: the superseded row is not applied, and the client
/// does not assign itself the older generation it carries. The second
/// is the sharper failure: a device that rewinds its own generation
/// 409s on every subsequent push.
#[test]
fn s7_mixed_generation_batch_ignores_the_old_rows_and_never_rewinds() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x72u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    // A pushes two rows. Both land at generation 0.
    env.use_a();
    db_a.insert_entry(&make_entry("pre-bump", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    db_a.insert_entry(&make_entry("post-bump", 60, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();

    // Re-stamp the *second* changeset as generation 1, and the cluster
    // with it: the shape the relay is in immediately after a bump that
    // some rows were written before and some after. Doing it in SQL
    // rather than through a real bump is what makes both generations
    // reachable in one batch.
    let cluster_id = sync_state::get_cluster_id(&db_a).unwrap().unwrap();
    {
        let conn = relay.state.db.lock().unwrap();
        let newest: i64 = conn
            .query_row("SELECT MAX(id) FROM changesets", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "UPDATE changesets SET generation = 1 WHERE id = ?1",
            rusqlite::params![newest],
        )
        .unwrap();
        conn.execute(
            "UPDATE clusters SET generation = 1 WHERE cluster_id = ?1 AND user_id = ?2",
            rusqlite::params![cluster_id, relay.user_id.to_string()],
        )
        .unwrap();
    }

    // B arrives on generation 1 and pulls both rows in one batch.
    env.use_b();
    sync_state::set_cluster_id(&db_b, &cluster_id).unwrap();
    sync_state::set_cluster_generation(&db_b, 1).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_b)
        .expect("B pulls the straddling batch");

    let fields = live_fields(&db_b);
    assert!(
        fields.contains(&"post-bump".to_string()),
        "the current-generation row must still arrive — the gate retires \
         superseded generations, not all history; got {fields:?}",
    );
    assert!(
        !fields.contains(&"pre-bump".to_string()),
        "the superseded row must not be applied; got {fields:?}",
    );
    assert_eq!(
        sync_state::get_cluster_generation(&db_b).unwrap(),
        1,
        "an older row in the batch must not drag the device back to its generation",
    );
    // The two sides differ by exactly one row, so the content claim is
    // made on the row that did cross: it is A's, unchanged.
    assert_eq!(
        row_for(&db_b, "post-bump"),
        row_for(&db_a, "post-bump"),
        "the row the gate let through is A's own, not a mangled copy of it",
    );

    // And the watermark moved past the skipped row: a second pull has
    // nothing left to offer, rather than re-fetching it forever.
    let again = run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_b)
        .expect("B pulls again");
    assert_eq!(again.pulled, 0);
    assert_eq!(again.superseded_skipped, 0);
}

/// The restore half of "reconcile, don't merge". A restore-bump does
/// not mean "discard everything" — it means "the restored snapshot is
/// the truth now". The peer must land on the snapshot, and neither
/// re-merge its own pre-restore rows nor the cluster's.
#[test]
fn s7_restore_bump_reconciles_to_the_restored_snapshot() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x73u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    // Baseline: A and B both have pre-restore rows on the relay.
    env.use_a();
    db_a.insert_entry(&make_entry("a-pre-restore", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    env.use_b();
    db_b.insert_entry(&make_entry("b-pre-restore", 60, "dev-B"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_b).unwrap();

    // A restores a snapshot: the cluster's generation bumps, and A
    // pushes the restored content under the new generation. Driven
    // through the same bump seam `db restore --cluster` uses (s3).
    env.use_a();
    let cluster_id = sync_state::get_cluster_id(&db_a).unwrap().unwrap();
    let new_gen = datum_commands::commands::sync::cluster_ops::bump_generation_for_current_server(
        &cluster_id,
    )
    .expect("server bump");
    sync_state::set_cluster_generation(&db_a, new_gen).unwrap();
    db_a.insert_entry(&make_entry("restored", 120, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();

    // B recovers and syncs.
    env.use_b();
    let mut prompter = datum_cli::sync::TestPrompter::new::<Vec<String>, String>(vec![]);
    prompter.push_confirm(true);
    datum_cli::sync::run_sync_with_client_and_prompter(
        Some(&relay.base_url),
        Direction::PullOnly,
        false,
        &db_b,
        &mut prompter,
    )
    .expect("B recovers and pulls the restored snapshot");

    let fields = live_fields(&db_b);
    assert!(
        fields.contains(&"restored".to_string()),
        "B must adopt the restored state; got {fields:?}",
    );
    assert!(
        !fields.contains(&"a-pre-restore".to_string())
            && !fields.contains(&"b-pre-restore".to_string()),
        "neither side's pre-restore rows may be merged back over the snapshot; got {fields:?}",
    );
    assert_eq!(
        row_for(&db_b, "restored"),
        row_for(&db_a, "restored"),
        "the snapshot B adopted is A's row, unchanged in transit",
    );
}

/// `db restore --hard` as the command runs it (da-rle8g), from a backup
/// file in this device's DATUM_HOME.
fn restore_hard(db: &Db, backup: &str, cluster: bool) {
    datum_commands::commands::backup_restore::cmd_restore(
        datum_commands::args::RestoreArgs {
            path: backup.to_string(),
            force: false,
            dry_run: false,
            merge: false,
            hard: true,
            yes: true,
            show: datum_commands::args::ShowFlag::None,
            cluster,
        },
        None,
        db,
    )
    .expect("restore --hard");
}

fn backup_to(db: &Db, name: &str) -> String {
    let path = std::env::var("DATUM_HOME").unwrap() + "/" + name;
    datum_commands::commands::backup_restore::cmd_backup(
        datum_commands::args::BackupArgs {
            path: path.clone(),
            overwrite: true,
        },
        None,
        db,
    )
    .expect("backup");
    path
}

/// `--hard --cluster` carries the RESTORED rows to the peer, not only the
/// ones written after it: they keep the backup's modify times, so the push
/// has to start over for the new generation to hold them (da-rle8g).
#[test]
fn s7_restore_hard_cluster_hands_the_peer_the_backup() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x74u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    env.use_a();
    db_a.insert_entry(&make_entry("in-backup", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    let backup = backup_to(&db_a, "hard.jsonl");
    db_a.insert_entry(&make_entry("after-backup", 60, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    env.use_b();
    run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_b).unwrap();

    env.use_a();
    restore_hard(&db_a, &backup, true);
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();

    env.use_b();
    let mut prompter = datum_cli::sync::TestPrompter::new::<Vec<String>, String>(vec![]);
    prompter.push_confirm(true);
    datum_cli::sync::run_sync_with_client_and_prompter(
        Some(&relay.base_url),
        Direction::PullOnly,
        false,
        &db_b,
        &mut prompter,
    )
    .expect("B recovers and pulls the restored snapshot");

    assert_eq!(live_fields(&db_b), vec!["in-backup".to_string()]);
    assert_eq!(row_for(&db_b, "in-backup"), row_for(&db_a, "in-backup"));
}

/// `--hard` alone leaves the generation, and the next sync pulls what the
/// other devices pushed back in on top of the restored state (da-rle8g):
/// the pull watermark starts over.
#[test]
fn s7_restore_hard_without_cluster_pulls_the_peers_rows_back() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x75u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    env.use_a();
    db_a.insert_entry(&make_entry("in-backup", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_a).unwrap();
    let backup = backup_to(&db_a, "hard.jsonl");
    env.use_b();
    db_b.insert_entry(&make_entry("from-b", 60, "dev-B"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_b).unwrap();
    env.use_a();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_a).unwrap();
    assert_eq!(live_fields(&db_a).len(), 2);

    restore_hard(&db_a, &backup, false);
    assert_eq!(live_fields(&db_a), vec!["in-backup".to_string()]);
    run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_a).unwrap();

    let mut fields = live_fields(&db_a);
    fields.sort();
    assert_eq!(fields, vec!["from-b".to_string(), "in-backup".to_string()]);
    assert_eq!(sync_state::get_cluster_generation(&db_a).unwrap(), 0);
}

// ──────────────────────────────────────────────────────────────────────────
// SCENARIO 8 — a replaced relay (da-od2t)
// ──────────────────────────────────────────────────────────────────────────

/// Replace the relay's storage the way spinning up a fresh server does:
/// same URL, same user, same auth — but no cluster row, no devices, no
/// changesets, and a row-id space that restarts at 1 (hence the
/// `sqlite_sequence` reset; a bare DELETE leaves AUTOINCREMENT's
/// high-water mark behind and the replacement would not be fresh).
///
/// Keeping the `users` row and the JWT secret is what makes this a
/// *replaced server* rather than a different account: the first version
/// of this scenario used a second `StubRelay` and its precondition assert
/// caught that each relay mints its own user uuid, so it would have
/// measured "the client moved accounts".
fn wipe_relay_storage(relay: &StubRelay) {
    let conn = relay.state.db.lock().unwrap();
    conn.execute_batch(
        "DELETE FROM changesets;
         DELETE FROM clusters;
         DELETE FROM devices;
         DELETE FROM sqlite_sequence WHERE name = 'changesets';",
    )
    .expect("wipe relay storage");
}

/// vividn's spec: "spin up a new fresh server, connect the clients and sync
/// just works again."
///
/// The half that used to fail was not "no client loses data" — nobody was
/// ever prompted and no row was ever dropped. It was the other half: each
/// client's push watermark still claimed the wiped relay held its history,
/// so nobody re-sent it. Both syncs returned Ok and the cluster was
/// silently short, which a device joining later would inherit as an
/// incomplete history with nothing anywhere reporting a problem.
///
/// So the assertion that matters is made by a *third* device joining
/// after the replacement: what it pulls is what the cluster actually
/// holds.
///
/// Two internal controls keep this from passing vacuously:
///   1. the wipe is asserted to have removed the cluster row, so a no-op
///      wipe cannot pass;
///   2. B holds a row A never pulled, so the cluster can only come back
///      complete if *each* client re-sends its own history — one client
///      re-pushing everything it happens to have is not enough.
#[test]
fn s8_replaced_server_must_not_cost_clients_their_history() {
    let env = ClientEnv::new();
    let relay = StubRelay::start();
    let key = [0x81u8; 32];
    let db_a = provision_client(&env, "A", &relay, &key);
    let db_b = provision_client(&env, "B", &relay, &key);

    // Baseline: A and B converge on two rows through the relay.
    env.use_a();
    db_a.insert_entry(&make_entry("coffee", 0, "dev-A"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_a).expect("A sync");
    env.use_b();
    db_b.insert_entry(&make_entry("mood", 60, "dev-B")).unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_b).expect("B sync");
    env.use_a();
    run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_a).expect("A sync");

    // Control 2: B pushes a row A never pulls. After this the cluster's
    // full history exists on no single device.
    env.use_b();
    db_b.insert_entry(&make_entry("walk", 120, "dev-B"))
        .unwrap();
    run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db_b)
        .expect("B pushes walk");
    assert_eq!(live_rows(&db_a), 2, "A holds coffee + mood, not walk");
    assert_eq!(live_rows(&db_b), 3, "B holds all three");

    let cluster_id = sync_state::get_cluster_id(&db_a).unwrap().unwrap();
    assert_eq!(cluster_count_on_server(&relay), 1);
    assert_eq!(read_server_cluster_generation(&relay, &cluster_id), 0);
    let user_before = relay.user_id;

    // ── the server is replaced ────────────────────────────────────────
    wipe_relay_storage(&relay);

    // Control 1: the wipe really did remove the cluster row, so what
    // follows measures a fresh relay and not a no-op.
    assert_eq!(
        cluster_count_on_server(&relay),
        0,
        "the replacement must have no cluster row",
    );
    assert_eq!(relay.changeset_count(), 0, "and no changesets");
    assert_eq!(
        relay.user_id, user_before,
        "same URL, same account — a replaced server, not a different one",
    );

    // ── connect the clients and sync ──────────────────────────────────
    env.use_a();
    let a_out = run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_a)
        .expect("A syncs against the replacement");
    env.use_b();
    let b_out = run_sync_with_client(Some(&relay.base_url), Direction::Both, false, &db_b)
        .expect("B syncs against the replacement");

    // "No client loses data" — the half that already held.
    assert_eq!(live_rows(&db_a), 2, "A kept its rows");
    assert_eq!(live_rows(&db_b), 3, "B kept its rows");

    // The replacement is not silent about it: each client reports the
    // relay it found missing its pushes.
    assert!(
        a_out.relay_rewound.is_some(),
        "A must report the replaced relay, not sync silently",
    );
    assert!(
        b_out.relay_rewound.is_some(),
        "B must report the replaced relay, not sync silently",
    );

    // "Sync just works again" — the half that failed. A device joining
    // after the replacement is the honest measure of what the cluster
    // holds, and it must find the whole history.
    let db_c = provision_client(&env, "C", &relay, &key);
    env.use_c();
    run_sync_with_client(Some(&relay.base_url), Direction::PullOnly, false, &db_c)
        .expect("a device joining after the replacement");

    assert_eq!(
        live_fields(&db_c),
        vec!["coffee".to_string(), "mood".to_string(), "walk".to_string()],
        "the replaced relay must carry the cluster's whole history",
    );
    assert_same_content("C", &db_c, "B", &db_b);
    assert_eq!(
        sync_state::get_cluster_id(&db_c).unwrap().as_deref(),
        Some(cluster_id.as_str()),
        "and under the same cluster it always had",
    );
}
