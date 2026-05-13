//! C6: datumrc.toml + env-var + CLI flag layering with source tracking.
//!
//! Each test sandboxes `DATUM_HOME`, `DATUM_CONFIG_HOME`, `DATUM_CONFIG`,
//! and all `DATUM_*` env vars so config resolution is fully isolated.
//! Serialized by a process-wide mutex because env is shared state.

use datum_cli::config::{self, Config, Source};
use std::path::Path;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

const KEYS_TO_CLEAR: &[&str] = &[
    "DATUM_DB",
    "DATUM_CONFIG",
    "DATUM_DEVICE_ID",
    "DATUM_TIMEZONE",
    "DATUM_DEFAULT_FIELD",
    "DATUM_EDITOR",
    "DATUM_ID_DELIMITER",
    "VISUAL",
    "EDITOR",
];

fn sandbox(tmp: &Path) -> std::sync::MutexGuard<'_, ()> {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("DATUM_HOME", tmp.join("data"));
    std::env::set_var("DATUM_CONFIG_HOME", tmp.join("config"));
    for k in KEYS_TO_CLEAR {
        std::env::remove_var(k);
    }
    guard
}

fn write_toml(tmp: &Path, body: &str) -> std::path::PathBuf {
    // DATUM_CONFIG_HOME is the datum-specific config dir (see config_dir()),
    // so the toml lives directly under it — no "/datum" subdir.
    let dir = tmp.join("config");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("datumrc.toml");
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn toml_config_loads_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    write_toml(
        tmp.path(),
        r#"
db_path = "/data/main.db"
timezone = "America/Chicago"
default_field = "mood"
editor = "nvim"
"#,
    );

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.db_path.value, "/data/main.db");
    assert_eq!(cfg.db_path.source, Source::Toml);
    assert_eq!(cfg.timezone.value, "America/Chicago");
    assert_eq!(cfg.timezone.source, Source::Toml);
    assert_eq!(cfg.default_field.value.as_deref(), Some("mood"));
    assert_eq!(cfg.default_field.source, Source::Toml);
    assert_eq!(cfg.editor.value, "nvim");
    assert_eq!(cfg.editor.source, Source::Toml);
    // device_id wasn't set in toml → default layer (auto-generated/persisted)
    assert_eq!(cfg.device_id.source, Source::Default);
    assert!(!cfg.device_id.value.is_empty());
    assert!(cfg.toml_loaded);
}

#[test]
fn env_var_overrides_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    write_toml(
        tmp.path(),
        r#"
db_path = "/toml/path.db"
timezone = "America/Chicago"
default_field = "toml_field"
editor = "nano"
device_id = "toml-device"
"#,
    );

    std::env::set_var("DATUM_DB", "/env/path.db");
    std::env::set_var("DATUM_TIMEZONE", "Europe/Berlin");
    std::env::set_var("DATUM_DEFAULT_FIELD", "env_field");
    std::env::set_var("DATUM_EDITOR", "emacs");
    std::env::set_var("DATUM_DEVICE_ID", "env-device");

    let cfg = Config::load(None).unwrap();

    assert_eq!(cfg.db_path.value, "/env/path.db");
    assert_eq!(cfg.db_path.source, Source::Env);
    assert_eq!(cfg.timezone.value, "Europe/Berlin");
    assert_eq!(cfg.timezone.source, Source::Env);
    assert_eq!(cfg.default_field.value.as_deref(), Some("env_field"));
    assert_eq!(cfg.default_field.source, Source::Env);
    assert_eq!(cfg.editor.value, "emacs");
    assert_eq!(cfg.editor.source, Source::Env);
    assert_eq!(cfg.device_id.value, "env-device");
    assert_eq!(cfg.device_id.source, Source::Env);
}

#[test]
fn cli_flag_overrides_env_var() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    write_toml(tmp.path(), "db_path = \"/toml/path.db\"\n");
    std::env::set_var("DATUM_DB", "/env/path.db");

    let cfg = Config::load(Some("/flag/path.db")).unwrap();

    assert_eq!(cfg.db_path.value, "/flag/path.db");
    assert_eq!(cfg.db_path.source, Source::Flag);
}

#[test]
fn config_show_attributes_sources_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    // toml sets timezone, editor
    write_toml(
        tmp.path(),
        r#"
timezone = "America/Chicago"
editor = "nvim"
"#,
    );
    // env sets default_field
    std::env::set_var("DATUM_DEFAULT_FIELD", "mood");
    // flag sets db_path
    let cfg = Config::load(Some("/flag/path.db")).unwrap();

    let rows = cfg.rows();
    let by_key: std::collections::HashMap<_, _> =
        rows.iter().map(|(k, v, s)| (*k, (v.clone(), *s))).collect();

    assert_eq!(by_key["db_path"].1, Source::Flag);
    assert_eq!(by_key["db_path"].0, "/flag/path.db");

    assert_eq!(by_key["default_field"].1, Source::Env);
    assert_eq!(by_key["default_field"].0, "mood");

    assert_eq!(by_key["timezone"].1, Source::Toml);
    assert_eq!(by_key["timezone"].0, "America/Chicago");

    assert_eq!(by_key["editor"].1, Source::Toml);
    assert_eq!(by_key["editor"].0, "nvim");

    // device_id wasn't set anywhere → default layer
    assert_eq!(by_key["device_id"].1, Source::Default);
}

#[test]
fn datum_config_env_var_overrides_toml_path() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    // Put a toml at the XDG default location — should be IGNORED when
    // DATUM_CONFIG points elsewhere.
    write_toml(tmp.path(), "timezone = \"Europe/Berlin\"\n");

    let alt = tmp.path().join("alt.toml");
    std::fs::write(&alt, "timezone = \"Asia/Tokyo\"\n").unwrap();
    std::env::set_var("DATUM_CONFIG", &alt);

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.timezone.value, "Asia/Tokyo");
    assert_eq!(cfg.timezone.source, Source::Toml);
    assert_eq!(cfg.toml_path, alt);
    assert!(cfg.toml_loaded);
}

#[test]
fn missing_toml_falls_through_to_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    let cfg = Config::load(None).unwrap();

    assert!(!cfg.toml_loaded);
    assert_eq!(cfg.timezone.source, Source::Default);
    assert_eq!(cfg.editor.source, Source::Default);
    assert_eq!(cfg.default_field.source, Source::Default);
    assert_eq!(cfg.default_field.value, None);
    // db_path default is <data_dir>/datum.db
    assert_eq!(cfg.db_path.source, Source::Default);
    assert!(cfg.db_path.value.ends_with("datum.db"));
}

#[test]
fn toml_parse_error_surfaces() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    let bad = write_toml(tmp.path(), "= not valid toml =\n");
    let err = config::load_toml_config(&bad).unwrap_err();
    assert!(
        err.to_string().contains("Parsing config")
            || err
                .chain()
                .any(|e| e.to_string().to_lowercase().contains("toml")),
        "expected a parse-context error, got: {err:#}"
    );
}

#[test]
fn id_delimiter_default_is_colon() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.id_delimiter.value, ":");
    assert_eq!(cfg.id_delimiter.source, Source::Default);
}

#[test]
fn id_delimiter_toml_then_env_then_resolve() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    write_toml(tmp.path(), "id_delimiter = \"/\"\n");

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.id_delimiter.value, "/");
    assert_eq!(cfg.id_delimiter.source, Source::Toml);
    // resolve_id_delimiter (CLI-less) agrees with the Config layer.
    assert_eq!(config::resolve_id_delimiter(None), "/");

    std::env::set_var("DATUM_ID_DELIMITER", "-");
    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.id_delimiter.value, "-");
    assert_eq!(cfg.id_delimiter.source, Source::Env);
    assert_eq!(config::resolve_id_delimiter(None), "-");

    // CLI flag (passed via resolve_id_delimiter) trumps env.
    assert_eq!(config::resolve_id_delimiter(Some("__")), "__");
}

#[test]
fn empty_toml_is_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    write_toml(tmp.path(), "");

    let cfg = Config::load(None).unwrap();
    assert!(cfg.toml_loaded);
    // Everything falls through to defaults.
    assert_eq!(cfg.timezone.source, Source::Default);
    assert_eq!(cfg.editor.source, Source::Default);
}
