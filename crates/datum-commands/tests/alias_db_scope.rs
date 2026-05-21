//! Regression coverage for da-5hqu: `parse_invocation` must resolve
//! aliases against the db named by a global `--db <path>` argv flag,
//! not the default db. Before the fix, a user with `--db /tmp/foo.db`
//! who had defined `vbp` only in `/tmp/foo.db` would see "unknown
//! subcommand: 'vbp'" because the resolver opened the default db.

use std::path::Path;
use std::sync::Mutex;

use datum_commands::args::{parse_invocation, peek_intent_context};
use datum_core::db::Db;
use datum_core::field_spec::Alias;
use tempfile::TempDir;

/// `parse_invocation` reads `HOME` / `XDG_DATA_HOME` / `DATUM_DB` from
/// the process env to derive the default db path. Integration tests
/// run in parallel by default — serialize on a mutex so one test's
/// env doesn't bleed into another's.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVar {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvVar {
    fn set(key: &'static str, value: &std::ffi::OsStr) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, prev }
    }

    fn remove(key: &'static str) -> Self {
        let prev = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, prev }
    }
}

impl Drop for EnvVar {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

fn isolated_home() -> (TempDir, Vec<EnvVar>) {
    let tmp = tempfile::tempdir().expect("create isolated XDG home");
    let path = tmp.path();
    let guards = vec![
        EnvVar::set("HOME", path.as_os_str()),
        EnvVar::set("XDG_DATA_HOME", path.as_os_str()),
        EnvVar::set("XDG_CONFIG_HOME", path.as_os_str()),
        EnvVar::remove("DATUM_DB"),
        EnvVar::remove("DATUM_HOME"),
    ];
    (tmp, guards)
}

fn create_db_with_alias(path: &Path, name: &str, expands_to: &str) {
    let db =
        Db::open(path.to_str().expect("utf-8 db path")).expect("open target db for alias seed");
    db.save_alias(&Alias {
        name: name.to_string(),
        expands_to: expands_to.to_string(),
        description: None,
    })
    .expect("save alias to target db");
}

#[test]
fn parse_invocation_resolves_alias_in_db_flag_target() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (tmp, _guards) = isolated_home();

    let target = tmp.path().join("target.db");
    create_db_with_alias(&target, "vbp", "note word srcLang=pt pos=verb");

    let target_str = target.to_str().expect("utf-8 target path");
    let inv = parse_invocation(argv(&[
        "datum", "--db", target_str, "vbp", "correr", "to run",
    ]))
    .expect("parse_invocation should succeed against --db target");

    // The alias is only defined in `target.db`. The default db under
    // the isolated home does not exist, so under the pre-fix code the
    // resolver opened nothing (default) and the chain stayed empty —
    // the user saw "unknown subcommand: 'vbp'". Post-fix the resolver
    // opens `target.db` and the alias expands.
    let ctx = peek_intent_context().expect("intent context populated");
    assert!(
        ctx.alias_chain.iter().any(|s| s.name == "vbp"),
        "expected alias 'vbp' to expand via --db target, got chain: {:?}",
        ctx.alias_chain
    );

    drop(inv);
}

#[test]
fn parse_invocation_resolves_alias_in_db_flag_equals_form() {
    // `--db=<value>` (no separating space) must work the same as the
    // separated form — the pre-scan helper handles both.
    let _lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (tmp, _guards) = isolated_home();

    let target = tmp.path().join("equals.db");
    create_db_with_alias(&target, "vbp", "note word srcLang=pt pos=verb");

    let target_str = target.to_str().expect("utf-8 target path");
    let argv = vec![
        "datum".to_string(),
        format!("--db={target_str}"),
        "vbp".to_string(),
        "correr".to_string(),
        "to run".to_string(),
    ];
    parse_invocation(argv).expect("parse_invocation with --db=<path>");

    let ctx = peek_intent_context().expect("intent context populated");
    assert!(
        ctx.alias_chain.iter().any(|s| s.name == "vbp"),
        "expected alias 'vbp' to expand via --db=<path> target, got chain: {:?}",
        ctx.alias_chain
    );
}
