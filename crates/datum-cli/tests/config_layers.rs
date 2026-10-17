//! C6: datumrc.toml + env-var + CLI flag layering with source tracking.
//!
//! Each test sandboxes `DATUM_HOME`, `DATUM_CONFIG_HOME`, `DATUM_CONFIG`,
//! and all `DATUM_*` env vars so config resolution is fully isolated.
//! Serialized by a process-wide mutex because env is shared state.

use chrono::Weekday;
use datum_cli::config::{self, Config, Source};
use datum_test_util::env_lock;
use std::path::Path;

const KEYS_TO_CLEAR: &[&str] = &[
    "DATUM_DB",
    "DATUM_CONFIG",
    "DATUM_CLIENT_ID",
    "DATUM_DEFAULT_FIELD",
    "DATUM_EDITOR",
    "DATUM_ID_DELIMITER",
    "DATUM_WEEK_START",
    "VISUAL",
    "EDITOR",
];

fn sandbox(tmp: &Path) -> std::sync::MutexGuard<'static, ()> {
    let guard = env_lock();
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
default_field = "mood"
editor = "nvim"
"#,
    );

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.db_path.value, "/data/main.db");
    assert_eq!(cfg.db_path.source, Source::Toml);
    assert_eq!(cfg.default_field.value.as_deref(), Some("mood"));
    assert_eq!(cfg.default_field.source, Source::Toml);
    assert_eq!(cfg.editor.value, "nvim");
    assert_eq!(cfg.editor.source, Source::Toml);
    // client_id wasn't set in toml → default layer (auto-generated/persisted)
    assert_eq!(cfg.client_id.source, Source::Default);
    assert!(!cfg.client_id.value.is_empty());
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
default_field = "toml_field"
editor = "nano"
client_id = "toml-device"
"#,
    );

    std::env::set_var("DATUM_DB", "/env/path.db");
    std::env::set_var("DATUM_DEFAULT_FIELD", "env_field");
    std::env::set_var("DATUM_EDITOR", "emacs");
    std::env::set_var("DATUM_CLIENT_ID", "env-device");

    let cfg = Config::load(None).unwrap();

    assert_eq!(cfg.db_path.value, "/env/path.db");
    assert_eq!(cfg.db_path.source, Source::Env);
    assert_eq!(cfg.default_field.value.as_deref(), Some("env_field"));
    assert_eq!(cfg.default_field.source, Source::Env);
    assert_eq!(cfg.editor.value, "emacs");
    assert_eq!(cfg.editor.source, Source::Env);
    assert_eq!(cfg.client_id.value, "env-device");
    assert_eq!(cfg.client_id.source, Source::Env);
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

    // toml sets editor
    write_toml(
        tmp.path(),
        r#"
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

    assert_eq!(by_key["editor"].1, Source::Toml);
    assert_eq!(by_key["editor"].0, "nvim");

    // client_id wasn't set anywhere → default layer
    assert_eq!(by_key["client_id"].1, Source::Default);
}

#[test]
fn datum_config_env_var_overrides_toml_path() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    // Put a toml at the XDG default location — should be IGNORED when
    // DATUM_CONFIG points elsewhere.
    write_toml(tmp.path(), "editor = \"nano\"\n");

    let alt = tmp.path().join("alt.toml");
    std::fs::write(&alt, "editor = \"nvim\"\n").unwrap();
    std::env::set_var("DATUM_CONFIG", &alt);

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.editor.value, "nvim");
    assert_eq!(cfg.editor.source, Source::Toml);
    assert_eq!(cfg.toml_path, alt);
    assert!(cfg.toml_loaded);
}

#[test]
fn missing_toml_falls_through_to_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    let cfg = Config::load(None).unwrap();

    assert!(!cfg.toml_loaded);
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
    assert_eq!(cfg.editor.source, Source::Default);
}

#[test]
fn week_start_defaults_then_layers_toml_under_env() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());

    // Default: Monday, and `config show` says so.
    assert_eq!(config::resolve_week_start().unwrap(), Weekday::Mon);
    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.week_start.value, "Mon");
    assert_eq!(cfg.week_start.source, Source::Default);

    write_toml(tmp.path(), "week_start = \"sunday\"\n");
    assert_eq!(config::resolve_week_start().unwrap(), Weekday::Sun);
    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.week_start.value, "sunday");
    assert_eq!(cfg.week_start.source, Source::Toml);

    std::env::set_var("DATUM_WEEK_START", "3");
    assert_eq!(config::resolve_week_start().unwrap(), Weekday::Wed);
    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.week_start.source, Source::Env);
    std::env::remove_var("DATUM_WEEK_START");
}

#[test]
fn a_misspelled_week_start_still_shows_in_config_show() {
    // `config show` is where a user goes to diagnose a bad value, so it
    // must render one rather than fail; only the commands that need a
    // week reject it.
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());
    write_toml(tmp.path(), "week_start = \"funday\"\n");

    let cfg = Config::load(None).unwrap();
    assert_eq!(cfg.week_start.value, "funday");
    assert!(config::resolve_week_start().is_err());
}

/// `stats --week` / `--last-week` open on the configured `week_start`.
/// Lives here rather than in `stats_cmd.rs` because it sets config, and
/// only this binary serialises every reader of it (da-s99o3: a `funday`
/// set there failed a neighbouring stats test that took no lock).
#[test]
fn the_week_flags_read_the_week_start_config() {
    use datum_cli::args::{StatsArgs, StatsPeriod};
    use datum_commands::commands::stats::compute_stats;
    use datum_core::entry::Verb;
    use datum_core::test_util::{entry_builder, freeze_time, fresh_db};

    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());
    let _t = freeze_time("2026-06-17T09:00:00Z"); // a Wednesday
    let db = fresh_db();
    entry_builder("activity")
        .verb(Verb::Switch)
        .state(serde_json::json!("coding"))
        .time_start(
            datum_core::time::DatumTime::from_utc_and_tz(
                "2026-05-20T10:00:00Z".parse().unwrap(),
                "UTC",
            )
            .unwrap(),
        )
        .insert(&db);
    let week = |period| {
        compute_stats(
            &StatsArgs {
                selector: Some("activity".to_string()),
                period: Some(period),
                ..StatsArgs::default()
            },
            &db,
        )
        .map(|r| r.total_duration_seconds)
    };

    write_toml(tmp.path(), "week_start = \"monday\"\n");
    assert_eq!(
        week(StatsPeriod::Week).unwrap(),
        2 * 86400 + 9 * 3600,
        "Monday midnight through Wednesday 09:00"
    );
    assert_eq!(
        week(StatsPeriod::LastWeek).unwrap(),
        7 * 86400,
        "a whole closed week"
    );

    write_toml(tmp.path(), "week_start = \"sunday\"\n");
    assert_eq!(
        week(StatsPeriod::Week).unwrap(),
        3 * 86400 + 9 * 3600,
        "a Sunday week opened a day earlier"
    );
    assert_eq!(week(StatsPeriod::LastWeek).unwrap(), 7 * 86400);

    write_toml(tmp.path(), "week_start = \"funday\"\n");
    let err = week(StatsPeriod::Week).unwrap_err().to_string();
    assert!(err.contains("week_start"), "{err}");
}

/// `datum config --help` tables the keys from `ENV_VARS`' `key`s (da-6szdc),
/// so they have to be the keys `config show` prints, in its order.
#[test]
fn config_help_keys_are_the_keys_config_show_prints() {
    let tmp = tempfile::tempdir().unwrap();
    let _g = sandbox(tmp.path());
    let shown: Vec<&str> = Config::load(None)
        .unwrap()
        .rows()
        .into_iter()
        .map(|(k, _, _)| k)
        .collect();
    let documented: Vec<&str> = datum_parse::help::ENV_VARS
        .iter()
        .filter(|v| !v.key.is_empty())
        .map(|v| v.key)
        .collect();
    assert_eq!(documented, shown);
}
