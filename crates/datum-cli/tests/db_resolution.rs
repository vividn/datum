//! `--db` resolution and registry semantics (da-ndd.11).
//!
//! Each test sets `DATUM_HOME` / `DATUM_CONFIG_HOME` to a temp dir and
//! clears `DATUM_DB` so resolution is fully sandboxed. Serialized by a
//! process-wide mutex because the underlying env is shared state.

use datum_cli::config::{self, looks_like_path, ResolvedDb};

// Env mutations in the resolver aren't safe to parallelize; serialize.
use datum_test_util::env_lock;

fn sandbox_env(tmp: &std::path::Path) -> std::sync::MutexGuard<'static, ()> {
    let guard = env_lock();
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
    let body = format!("[db.body]\npath = \"{}\"\n", custom.display());
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

/// How many files `datum <args>` opened, run as `main` runs it.
fn opens(args: &[&str]) -> usize {
    let argv = std::iter::once("datum")
        .chain(args.iter().copied())
        .map(String::from)
        .collect();
    let before = datum_core::db::Db::files_opened();
    datum_cli::run(argv).unwrap_or_else(|e| panic!("datum {args:?}: {e:#}"));
    datum_core::db::Db::files_opened() - before
}

/// A process opens its database once (da-z75fs): the alias pass's `Db` is the
/// one the dispatch runs on, where it opens the same file the same way.
#[test]
fn a_command_opens_its_database_once() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox_env(tmp.path());
    let dir = tmp.path().join("dbs");
    std::fs::create_dir(&dir).unwrap();
    std::env::set_var("DATUM_DB", dir.join("once.db"));

    // No file yet: the alias pass opens nothing, and the write makes it.
    assert_eq!(opens(&["moment", "coffee"]), 1, "the first write");
    assert_eq!(opens(&["moment", "coffee"]), 1, "a write");
    assert_eq!(opens(&["tail"]), 1, "a read by the owner");
    assert_eq!(opens(&["alias", "set", "cf", "moment coffee"]), 1);
    assert_eq!(opens(&["cf"]), 1, "an alias");
    assert_eq!(
        opens(&["moment", "tea", ",", "moment", "cake"]),
        1,
        "two sides"
    );
    let other = tmp.path().join("other.db");
    assert_eq!(opens(&["--db", other.to_str().unwrap(), "moment", "x"]), 1);
    assert_eq!(opens(&["--db", other.to_str().unwrap(), "tail"]), 1, "--db");

    // A reader that cannot write the file opens it read-only, once.
    let writable = std::fs::metadata(&dir).unwrap().permissions();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let read_only = opens(&["tail"]);
    std::fs::set_permissions(&dir, writable).unwrap();
    assert_eq!(read_only, 1, "a read-only read");
}
