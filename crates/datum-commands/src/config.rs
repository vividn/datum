//! Configuration: locate the database file, device ID, and DB registry.
//!
//! The DB registry lives at `$XDG_CONFIG_HOME/datum/dbs.toml` and maps names
//! to absolute paths. Names that aren't registered resolve to
//! `<XDG_DATA_HOME>/datum/<name>.db` and are auto-added on first use.
//!
//! User preferences live in `$XDG_CONFIG_HOME/datum/datumrc.toml` (overridable
//! via `DATUM_CONFIG`). Loaded through `Config::load`, which layers
//! CLI flag > env var (DATUM_*) > toml > built-in default with per-key
//! source tracking for `datum config show`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A DB reference resolved from CLI arg / env / default. `created_new`
/// indicates that the path didn't exist before this resolution (caller may
/// warn on stderr and persist a registry entry).
#[derive(Debug, Clone)]
pub struct ResolvedDb {
    /// Display name (registry key) when the user passed a bare name. `None`
    /// for explicit paths.
    pub name: Option<String>,
    /// Absolute path to the SQLite file.
    pub path: PathBuf,
    /// True when the underlying file did not exist at resolution time.
    pub created_new: bool,
}

/// Registry entry per DB.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DbEntry {
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default, rename = "db")]
    pub db: BTreeMap<String, DbEntry>,
}

/// Return the path to the datum SQLite database using the legacy single-DB
/// resolution order. Preserved for call sites that haven't migrated yet.
///
/// Resolution order:
///   1. DATUM_DB env var (path or name)
///   2. Default: $XDG_DATA_HOME/datum/datum.db
pub fn db_path() -> Result<String> {
    let resolved = resolve_db(None)?;
    Ok(resolved.path.to_string_lossy().into_owned())
}

/// Resolve a `--db <value>` arg (or fall back to `DATUM_DB` env / default).
///
/// Rules for `value` (applied also to `DATUM_DB`):
/// - Contains `/`, starts with `.`, or ends with `.db` → treat as **path**
///   (absolute or relative, created on demand).
/// - Otherwise → treat as **name** → registry lookup; falls back to
///   `<data_dir>/datum/<name>.db` if unknown. Unknown names are auto-added
///   to the registry after the caller confirms the path was created.
pub fn resolve_db(cli_db: Option<&str>) -> Result<ResolvedDb> {
    let (value, source): (Option<String>, &str) = match cli_db {
        Some(v) => (Some(v.to_string()), "flag"),
        None => match std::env::var("DATUM_DB") {
            Ok(v) if !v.is_empty() => (Some(v), "env"),
            _ => (None, "default"),
        },
    };

    let Some(value) = value else {
        // Default: <data_dir>/datum/datum.db
        let path = data_dir()?.join("datum.db");
        let created_new = !path.exists();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Creating directory {parent:?}"))?;
        }
        return Ok(ResolvedDb {
            name: None,
            path,
            created_new,
        });
    };

    let _ = source;

    if looks_like_path(&value) {
        let path = PathBuf::from(&value);
        let created_new = !path.exists();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Creating directory for DB path: {parent:?}"))?;
            }
        }
        return Ok(ResolvedDb {
            name: None,
            path,
            created_new,
        });
    }

    // Treat as name: registry first, then default dir.
    let registry = load_registry().unwrap_or_default();
    let path = if let Some(entry) = registry.db.get(&value) {
        PathBuf::from(&entry.path)
    } else {
        data_dir()?.join(format!("{value}.db"))
    };
    let created_new = !path.exists();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Creating directory {parent:?}"))?;
    }
    Ok(ResolvedDb {
        name: Some(value),
        path,
        created_new,
    })
}

/// Return true if `s` should be treated as a path (not a name).
/// Rule: contains `/`, starts with `.`, or ends with `.db`.
pub fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.starts_with('.') || s.ends_with(".db")
}

/// Cheap pre-scan of argv for the global `--db <value>` / `--db=<value>`
/// flag, without invoking the full parser. Used by the alias-resolution
/// pre-pass so it can route to the user-selected db before the parser
/// has even seen argv. Stops at a `--` separator: anything after is
/// positional and never carries the global db flag.
pub fn pre_scan_db_flag(argv: &[String]) -> Option<String> {
    extract_global_db_flag(argv).map(|(_, _, value)| value)
}

/// Like [`pre_scan_db_flag`] but also returns the slice of argv that
/// makes up the flag (one token for `--db=X`, two for `--db X`). The
/// alias pre-pass uses this to strip the flag out of argv before
/// alias-expansion runs, so the alias name isn't shadowed by `--db`
/// at argv[1]. Returns `(start_index, end_index_exclusive, value)`.
/// `None` when no `--db` is present before `--`.
pub fn extract_global_db_flag(argv: &[String]) -> Option<(usize, usize, String)> {
    let mut i = 1; // skip argv[0] (binary name)
    while i < argv.len() {
        let tok = &argv[i];
        if tok == "--" {
            return None;
        }
        if let Some(v) = tok.strip_prefix("--db=") {
            return Some((i, i + 1, v.to_string()));
        }
        if tok == "--db" {
            if i + 1 >= argv.len() {
                return None;
            }
            return Some((i, i + 2, argv[i + 1].clone()));
        }
        i += 1;
    }
    None
}

/// Split argv into (argv-without-`--db`-tokens, `--db`-tokens). Used by
/// the alias-expansion pre-pass: the global `--db <X>` flag must not
/// occupy argv[1] or it shadows the alias name. Strip it for expansion,
/// re-prepend it before the parser sees the final argv so the global
/// flag still routes downstream.
pub fn split_off_global_db_flag(argv: Vec<String>) -> (Vec<String>, Vec<String>) {
    let Some((start, end, _)) = extract_global_db_flag(&argv) else {
        return (argv, Vec::new());
    };
    let mut without: Vec<String> = Vec::with_capacity(argv.len() - (end - start));
    let mut flag: Vec<String> = Vec::with_capacity(end - start);
    for (i, tok) in argv.into_iter().enumerate() {
        if i >= start && i < end {
            flag.push(tok);
        } else {
            without.push(tok);
        }
    }
    (without, flag)
}

/// Load the DB registry from `~/.config/datum/dbs.toml` (or via
/// `DATUM_CONFIG_HOME` override). Missing file → empty registry. Parse
/// errors surface as `Err` so callers can show the user what's wrong.
pub fn load_registry() -> Result<Registry> {
    let path = registry_path()?;
    if !path.exists() {
        return Ok(Registry::default());
    }
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Reading DB registry at {path:?}"))?;
    let registry: Registry =
        toml::from_str(&contents).with_context(|| format!("Parsing DB registry at {path:?}"))?;
    Ok(registry)
}

/// Persist the given registry to disk.
pub fn save_registry(registry: &Registry) -> Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Creating config directory {parent:?}"))?;
    }
    let body = toml::to_string_pretty(registry).context("Serializing DB registry")?;
    std::fs::write(&path, body).with_context(|| format!("Writing DB registry to {path:?}"))?;
    Ok(())
}

/// Register a name → path mapping if absent. Explicit paths (no name) are
/// skipped. Safe to call on every CLI invocation; no-op when unchanged.
pub fn ensure_registered(resolved: &ResolvedDb) -> Result<()> {
    let Some(name) = resolved.name.as_deref() else {
        return Ok(());
    };
    let mut registry = load_registry().unwrap_or_default();
    let abs = resolved
        .path
        .canonicalize()
        .unwrap_or_else(|_| resolved.path.clone());
    let path_str = abs.to_string_lossy().into_owned();
    let current = registry.db.get(name).map(|e| e.path.clone());
    if current.as_deref() == Some(path_str.as_str()) {
        return Ok(());
    }
    registry
        .db
        .insert(name.to_string(), DbEntry { path: path_str });
    save_registry(&registry)
}

/// Location of the DB registry file.
pub fn registry_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("dbs.toml"))
}

/// Return (or create) the device ID stored in the data directory.
pub fn device_id() -> Result<String> {
    let path = data_dir()?.join("device_id");
    if path.exists() {
        let id = std::fs::read_to_string(&path)
            .with_context(|| format!("Reading device_id from {path:?}"))?;
        return Ok(id.trim().to_string());
    }
    let id = ulid::Ulid::new().to_string();
    std::fs::write(&path, &id).with_context(|| format!("Writing device_id to {path:?}"))?;
    Ok(id)
}

fn xdg_data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
}

fn xdg_config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

pub fn data_dir() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("DATUM_HOME") {
        let dir = PathBuf::from(home);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Creating DATUM_HOME directory {dir:?}"))?;
        return Ok(dir);
    }
    let base = xdg_data_dir().context("Cannot determine data directory")?;
    let dir = base.join("datum");
    std::fs::create_dir_all(&dir).with_context(|| format!("Creating data directory {dir:?}"))?;
    Ok(dir)
}

pub fn config_dir() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("DATUM_CONFIG_HOME") {
        let dir = PathBuf::from(home);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Creating DATUM_CONFIG_HOME directory {dir:?}"))?;
        return Ok(dir);
    }
    let base = xdg_config_dir().context("Cannot determine config directory")?;
    let dir = base.join("datum");
    std::fs::create_dir_all(&dir).with_context(|| format!("Creating config directory {dir:?}"))?;
    Ok(dir)
}

/// Scan the default data directory for `*.db` files. Used by `info --all`.
pub fn discovered_dbs() -> Result<Vec<(String, PathBuf)>> {
    let dir = data_dir()?;
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(it) => it,
        Err(_) => return Ok(out),
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("db") {
            continue;
        }
        let stem = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if stem.is_empty() {
            continue;
        }
        out.push((stem, p));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Convert an absolute path to a `~`-prefixed display string when it lives
/// under `$HOME`.
pub fn display_path(p: &Path) -> String {
    if let Ok(home) = std::env::var("HOME") {
        if let Ok(rel) = p.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    p.display().to_string()
}

// ─── Layered user config (datumrc.toml) ────────────────────────────────────

/// Where a config value came from. Shown by `datum config show`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Flag,
    Env,
    Toml,
    Default,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Flag => "flag",
            Source::Env => "env",
            Source::Toml => "toml",
            Source::Default => "default",
        }
    }
}

/// A value plus the layer it came from.
#[derive(Debug, Clone, Serialize)]
pub struct SourcedValue<T: Clone + Serialize> {
    pub value: T,
    pub source: Source,
}

impl<T: Clone + Serialize> SourcedValue<T> {
    fn new(value: T, source: Source) -> Self {
        Self { value, source }
    }
}

/// Shape of `datumrc.toml`. All fields optional; missing keys fall through
/// to the env layer or built-in defaults.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    /// Delimiter inserted between empty/collapsed `%key%` segments in
    /// slug templates. CLI `--id-delimiter` and `DATUM_ID_DELIMITER` env
    /// var override this. Built-in default is `":"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_delimiter: Option<String>,
    /// Desktop notifications when the daemon files a new conflict
    /// (C4e). `None` = use the platform default (`true`); explicit
    /// `false` disables them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notifications: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<SyncSection>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub databases: BTreeMap<String, DbSection>,
}

/// Global sync defaults. A per-DB entry under `[databases.*]` may override the
/// `default_server` with its own `sync_server`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct SyncSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_server: Option<String>,
}

/// Per-DB config overrides: can pin a `sync_server` (URL or the literal
/// `"none"` to disable sync for this DB) and toggle `auto_sync`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DbSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_sync: Option<bool>,
}

/// Effective resolved config with per-key source provenance.
#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: SourcedValue<String>,
    pub device_id: SourcedValue<String>,
    pub timezone: SourcedValue<String>,
    pub default_field: SourcedValue<Option<String>>,
    pub editor: SourcedValue<String>,
    pub id_delimiter: SourcedValue<String>,
    /// Path of the toml file that was loaded (or the path we looked at if
    /// missing). Useful for `config show` diagnostics.
    pub toml_path: PathBuf,
    /// Whether the toml file existed and was parsed.
    pub toml_loaded: bool,
}

/// Location of the user's `datumrc.toml`:
/// - `DATUM_CONFIG` env var wins (absolute or relative path).
/// - Otherwise `config_dir()/datumrc.toml`.
pub fn toml_config_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("DATUM_CONFIG") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    Ok(config_dir()?.join("datumrc.toml"))
}

/// Load the toml config as a mutable `RawConfig`. Missing/empty file returns
/// `RawConfig::default()`.
pub fn load_raw_config_or_default() -> Result<RawConfig> {
    let path = toml_config_path()?;
    Ok(load_toml_config(&path)?.unwrap_or_default())
}

/// Persist the raw user config to `datumrc.toml`. Creates the parent
/// directory if needed. Overwrites atomically via `fs::write`.
pub fn save_raw_config(cfg: &RawConfig) -> Result<()> {
    let path = toml_config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Creating config directory {parent:?}"))?;
    }
    let body = toml::to_string_pretty(cfg).context("Serializing datumrc.toml")?;
    std::fs::write(&path, body).with_context(|| format!("Writing config to {path:?}"))?;
    Ok(())
}

/// Where the master key for `server_host` lives on disk.
/// `$XDG_DATA_HOME/datum/keys/<server-host>.key`, file mode 0600.
pub fn master_key_path(server_host: &str) -> Result<PathBuf> {
    let dir = data_dir()?.join("keys");
    std::fs::create_dir_all(&dir).with_context(|| format!("Creating key directory {dir:?}"))?;
    Ok(dir.join(format!("{server_host}.key")))
}

/// Where an auth token for `server_host` lives on disk.
/// `$XDG_CONFIG_HOME/datum/auth/<server-host>.json`.
pub fn auth_token_path(server_host: &str) -> Result<PathBuf> {
    let dir = config_dir()?.join("auth");
    std::fs::create_dir_all(&dir).with_context(|| format!("Creating auth directory {dir:?}"))?;
    Ok(dir.join(format!("{server_host}.json")))
}

/// Extract a stable "host" component for an HTTPS URL — used to key per-server
/// token/key files. Falls back to the URL itself on parse failure.
pub fn server_host_from_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => match (u.host_str(), u.port()) {
            (Some(h), Some(p)) => format!("{h}_{p}"),
            (Some(h), None) => h.to_string(),
            _ => url.to_string(),
        },
        Err(_) => url.to_string(),
    }
}

/// Resolve the effective sync server for a given DB name.
///
/// Precedence: per-DB `sync_server` > global `sync.default_server`. The
/// literal string `"none"` (case-insensitive) means "sync disabled for this
/// DB" and returns `Ok(None)`.
pub fn resolve_sync_server(cfg: &RawConfig, db_name: Option<&str>) -> Option<String> {
    if let Some(name) = db_name {
        if let Some(db_section) = cfg.databases.get(name) {
            if let Some(v) = &db_section.sync_server {
                if v.eq_ignore_ascii_case("none") {
                    return None;
                }
                return Some(v.clone());
            }
        }
    }
    cfg.sync
        .as_ref()
        .and_then(|s| s.default_server.clone())
        .filter(|v| !v.eq_ignore_ascii_case("none"))
}

/// Load and parse the toml config. Missing file → `Ok(None)`. Parse errors
/// surface so users see what's wrong with their file.
pub fn load_toml_config(path: &Path) -> Result<Option<RawConfig>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents =
        std::fs::read_to_string(path).with_context(|| format!("Reading config from {path:?}"))?;
    if contents.trim().is_empty() {
        return Ok(Some(RawConfig::default()));
    }
    let cfg: RawConfig =
        toml::from_str(&contents).with_context(|| format!("Parsing config at {path:?}"))?;
    Ok(Some(cfg))
}

fn read_env(var: &str) -> Option<String> {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn editor_default() -> String {
    read_env("VISUAL")
        .or_else(|| read_env("EDITOR"))
        .unwrap_or_else(|| "vi".to_string())
}

/// Built-in default delimiter for slug-template interpolation. Used when
/// no CLI flag, env var, or `datumrc.toml` value is supplied. Matches the
/// historical hardcoded constant the parser used before R6 layering.
pub fn id_delimiter_default() -> String {
    ":".to_string()
}

fn timezone_default() -> String {
    // Fall through to the IANA local zone if it resolves; otherwise UTC.
    // chrono doesn't expose a portable IANA name, so we accept "UTC" as a
    // conservative default — users who care set `timezone =` in toml or
    // `DATUM_TIMEZONE` in env.
    "UTC".to_string()
}

impl Config {
    /// Load the effective config by layering CLI flag > env > toml > default.
    ///
    /// `cli_db` is the value of `--db` from argv (if any). Other CLI flags
    /// aren't wired up for these keys yet — when they are, extend this
    /// function similarly.
    pub fn load(cli_db: Option<&str>) -> Result<Self> {
        let toml_path = toml_config_path()?;
        let parsed = load_toml_config(&toml_path)?;
        let toml_loaded = parsed.is_some();
        let raw = parsed.unwrap_or_default();

        let db_path = if let Some(v) = cli_db {
            SourcedValue::new(v.to_string(), Source::Flag)
        } else if let Some(v) = read_env("DATUM_DB") {
            SourcedValue::new(v, Source::Env)
        } else if let Some(v) = raw.db_path.clone() {
            SourcedValue::new(v, Source::Toml)
        } else {
            let default_path = data_dir()?.join("datum.db");
            SourcedValue::new(default_path.to_string_lossy().into_owned(), Source::Default)
        };

        let device_id = if let Some(v) = read_env("DATUM_DEVICE_ID") {
            SourcedValue::new(v, Source::Env)
        } else if let Some(v) = raw.device_id.clone() {
            SourcedValue::new(v, Source::Toml)
        } else {
            SourcedValue::new(device_id()?, Source::Default)
        };

        let timezone = if let Some(v) = read_env("DATUM_TIMEZONE") {
            SourcedValue::new(v, Source::Env)
        } else if let Some(v) = raw.timezone.clone() {
            SourcedValue::new(v, Source::Toml)
        } else {
            SourcedValue::new(timezone_default(), Source::Default)
        };

        let default_field = if let Some(v) = read_env("DATUM_DEFAULT_FIELD") {
            SourcedValue::new(Some(v), Source::Env)
        } else if let Some(v) = raw.default_field.clone() {
            SourcedValue::new(Some(v), Source::Toml)
        } else {
            SourcedValue::new(None, Source::Default)
        };

        let editor = if let Some(v) = read_env("DATUM_EDITOR") {
            SourcedValue::new(v, Source::Env)
        } else if let Some(v) = raw.editor.clone() {
            SourcedValue::new(v, Source::Toml)
        } else {
            SourcedValue::new(editor_default(), Source::Default)
        };

        let id_delimiter = if let Some(v) = read_env("DATUM_ID_DELIMITER") {
            SourcedValue::new(v, Source::Env)
        } else if let Some(v) = raw.id_delimiter.clone() {
            SourcedValue::new(v, Source::Toml)
        } else {
            SourcedValue::new(id_delimiter_default(), Source::Default)
        };

        Ok(Config {
            db_path,
            device_id,
            timezone,
            default_field,
            editor,
            id_delimiter,
            toml_path,
            toml_loaded,
        })
    }

    /// Ordered (key, value-as-string, source) tuples for `config show`.
    pub fn rows(&self) -> Vec<(&'static str, String, Source)> {
        let default_field_str = self
            .default_field
            .value
            .clone()
            .unwrap_or_else(|| "(unset)".to_string());
        vec![
            ("db_path", self.db_path.value.clone(), self.db_path.source),
            (
                "device_id",
                self.device_id.value.clone(),
                self.device_id.source,
            ),
            (
                "timezone",
                self.timezone.value.clone(),
                self.timezone.source,
            ),
            (
                "default_field",
                default_field_str,
                self.default_field.source,
            ),
            ("editor", self.editor.value.clone(), self.editor.source),
            (
                "id_delimiter",
                self.id_delimiter.value.clone(),
                self.id_delimiter.source,
            ),
        ]
    }
}

/// Resolve the effective slug `id_delimiter` for a single invocation,
/// layering CLI flag > `DATUM_ID_DELIMITER` env > `datumrc.toml` >
/// built-in default. Standalone helper so per-entry slug-interpolation
/// sites don't have to materialize a full [`Config`] (which would also
/// trigger `device_id()` lazy generation as a side effect).
pub fn resolve_id_delimiter(cli_value: Option<&str>) -> String {
    if let Some(v) = cli_value {
        return v.to_string();
    }
    if let Some(v) = read_env("DATUM_ID_DELIMITER") {
        return v;
    }
    if let Ok(raw) = load_raw_config_or_default() {
        if let Some(v) = raw.id_delimiter {
            return v;
        }
    }
    id_delimiter_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_path_detects_slashes_dots_and_extension() {
        assert!(looks_like_path("/tmp/foo.db"));
        assert!(looks_like_path("./local.db"));
        assert!(looks_like_path("../up.db"));
        assert!(looks_like_path("relative/path"));
        assert!(looks_like_path("name.db"));
        assert!(!looks_like_path("fitness"));
        assert!(!looks_like_path("my-db"));
    }

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn pre_scan_db_flag_finds_separated_form() {
        assert_eq!(
            pre_scan_db_flag(&argv(&["datum", "--db", "fitness", "occur", "msg"])),
            Some("fitness".to_string())
        );
    }

    #[test]
    fn pre_scan_db_flag_finds_equals_form() {
        assert_eq!(
            pre_scan_db_flag(&argv(&["datum", "--db=/tmp/x.db", "msg"])),
            Some("/tmp/x.db".to_string())
        );
    }

    #[test]
    fn pre_scan_db_flag_returns_none_when_absent() {
        assert_eq!(pre_scan_db_flag(&argv(&["datum", "occur", "msg"])), None);
        assert_eq!(pre_scan_db_flag(&argv(&[])), None);
        assert_eq!(pre_scan_db_flag(&argv(&["datum"])), None);
    }

    #[test]
    fn pre_scan_db_flag_stops_at_separator() {
        // Anything after `--` is positional and never a global flag, so
        // a `--db <X>` token after the separator must not be picked up.
        assert_eq!(
            pre_scan_db_flag(&argv(&["datum", "note", "--", "--db", "x"])),
            None
        );
    }

    #[test]
    fn pre_scan_db_flag_skips_argv0() {
        // argv[0] is the binary name; a literal `--db` there shouldn't
        // confuse the scan.
        assert_eq!(pre_scan_db_flag(&argv(&["--db", "occur", "msg"])), None);
    }
}
