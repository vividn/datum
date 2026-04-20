//! `--db` resolution and registry semantics (da-ndd.11).
//!
//! Each test sets `DATUM_HOME` / `DATUM_CONFIG_HOME` to a temp dir and
//! clears `DATUM_DB` so resolution is fully sandboxed. Serialized by a
//! process-wide mutex because the underlying env is shared state.

use datum_cli::config::{self, looks_like_path, ResolvedDb};
use std::sync::Mutex;

// Env mutations in the resolver aren't safe to parallelize; serialize.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn sandbox_env(tmp: &std::path::Path) -> std::sync::MutexGuard<'_, ()> {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("DATUM_HOME", tmp.join("data"));
    std::env::set_var("DATUM_CONFIG_HOME", tmp.join("config"));
    std::env::remove_var("DATUM_DB");
    guard
}

#[test]
fn looks_like_path_rules() {
    assert!(looks_like_path("/abs/path.db"));
    assert!(looks_like_path("./rel.db"));
    assert!(looks_like_path("dir/name"));
    assert!(looks_like_path("local.db"));
    assert!(!looks_like_path("fitness"));
    assert!(!looks_like_path("my_db"));
}

#[test]
fn explicit_path_is_used_verbatim() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox_env(tmp.path());
    let p = tmp.path().join("custom.db");
    let resolved = config::resolve_db(Some(p.to_str().unwrap())).unwrap();
    assert_eq!(resolved.path, p);
    assert!(resolved.name.is_none());
    assert!(resolved.created_new);
}

#[test]
fn name_resolves_to_xdg_data_default_when_not_registered() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox_env(tmp.path());
    let resolved = config::resolve_db(Some("fitness")).unwrap();
    assert_eq!(resolved.name.as_deref(), Some("fitness"));
    assert_eq!(resolved.path, tmp.path().join("data").join("fitness.db"));
    assert!(resolved.created_new);
}

#[test]
fn name_resolves_via_registry_when_present() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox_env(tmp.path());

    // Seed registry with a custom path.
    let registry_dir = tmp.path().join("config");
    std::fs::create_dir_all(&registry_dir).unwrap();
    let custom = tmp.path().join("elsewhere").join("body.db");
    std::fs::create_dir_all(custom.parent().unwrap()).unwrap();
    let body = format!(
        "[db.body]\npath = \"{}\"\n",
        custom.display()
    );
    std::fs::write(registry_dir.join("dbs.toml"), body).unwrap();

    let resolved = config::resolve_db(Some("body")).unwrap();
    assert_eq!(resolved.path, custom);
    assert_eq!(resolved.name.as_deref(), Some("body"));
}

#[test]
fn ensure_registered_persists_named_db_to_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox_env(tmp.path());

    // First resolution: auto-create.
    let resolved = config::resolve_db(Some("fitness")).unwrap();
    // Simulate the open + ensure_registered pair from main().
    {
        let _ = datum_core::db::Db::open(resolved.path.to_str().unwrap()).unwrap();
    }
    config::ensure_registered(&resolved).unwrap();

    // Second resolution sees the registry entry.
    let registry = config::load_registry().unwrap();
    assert!(
        registry.db.contains_key("fitness"),
        "registry should carry auto-created name: {:?}",
        registry.db
    );
}

#[test]
fn datum_db_env_var_is_respected() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox_env(tmp.path());
    let p = tmp.path().join("env.db");
    std::env::set_var("DATUM_DB", p.to_str().unwrap());
    let resolved: ResolvedDb = config::resolve_db(None).unwrap();
    assert_eq!(resolved.path, p);
    std::env::remove_var("DATUM_DB");
}

#[test]
fn flag_overrides_env() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox_env(tmp.path());
    let env_p = tmp.path().join("env.db");
    let flag_p = tmp.path().join("flag.db");
    std::env::set_var("DATUM_DB", env_p.to_str().unwrap());
    let resolved = config::resolve_db(Some(flag_p.to_str().unwrap())).unwrap();
    assert_eq!(resolved.path, flag_p);
    std::env::remove_var("DATUM_DB");
}

#[test]
fn default_db_path_used_when_no_flag_or_env() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox_env(tmp.path());
    let resolved = config::resolve_db(None).unwrap();
    assert_eq!(resolved.path, tmp.path().join("data").join("datum.db"));
    assert!(resolved.name.is_none());
}
