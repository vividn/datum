//! Configuration: locate the database file, client ID, and DB registry.
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
use chrono::Weekday;
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

/// Resolve a `--db <value>` arg (or fall back to `DATUM_DB` env / default).
///
/// Rules for `value` (applied also to `DATUM_DB`):
/// - Contains `/`, starts with `.`, or ends with `.db` → treat as **path**
///   (absolute or relative, created on demand).
/// - Otherwise → treat as **name** → registry lookup; falls back to
///   `<data_dir>/datum/<name>.db` if unknown. Unknown names are auto-added
///   to the registry after the caller confirms the path was created.
pub fn resolve_db(cli_db: Option<&str>) -> Result<ResolvedDb> {
    let (name, path) = locate(cli_db)?;
    let created_new = !path.exists();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Creating directory {parent:?}"))?;
        }
    }
    Ok(ResolvedDb {
        name,
        path,
        created_new,
    })
}

/// Where [`resolve_db`] would put the database, creating nothing — for a
/// reader that must leave no trace, such as shell completion (da-bu09u).
pub fn locate_db(cli_db: Option<&str>) -> Option<PathBuf> {
    locate(cli_db).ok().map(|(_, path)| path)
}

/// The db's registry name (`None` for a path) and its path, touching nothing.
fn locate(cli_db: Option<&str>) -> Result<(Option<String>, PathBuf)> {
    let value = match cli_db {
        Some(v) => Some(v.to_string()),
        None => std::env::var("DATUM_DB").ok().filter(|v| !v.is_empty()),
    };
    let dir = || data_dir_path().context("Cannot determine data directory");
    let Some(value) = value else {
        return Ok((None, dir()?.join("datum.db")));
    };
    if looks_like_path(&value) {
        return Ok((None, PathBuf::from(&value)));
    }
    // Treat as name: registry first, then default dir.
    let registry = load_registry().unwrap_or_default();
    let path = match registry.db.get(&value) {
        Some(entry) => PathBuf::from(&entry.path),
        None => dir()?.join(format!("{value}.db")),
    };
    Ok((Some(value), path))
}

/// `DATUM_SHOW`: a `--show` asked for from outside argv (da-vx31e), so a
/// caller that wants the echo as data — datum.day — leaves `raw_input` the
/// line that was typed. Unset or empty is none; a value `--show` would
/// refuse is an error, as the flag's is.
pub fn env_show() -> Result<Option<datum_parse::input::Show>> {
    match std::env::var("DATUM_SHOW") {
        Ok(v) if !v.is_empty() => Ok(Some(
            v.parse().map_err(|e| anyhow::anyhow!("DATUM_SHOW: {e}"))?,
        )),
        _ => Ok(None),
    }
}

/// Return true if `s` should be treated as a path (not a name).
/// Rule: contains `/`, starts with `.`, or ends with `.db`.
pub fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.starts_with('.') || s.ends_with(".db")
}

/// Resolve the active *named* DB for per-DB config scoping (da-cyn4):
/// the explicit `--db` flag wins over the `DATUM_DB` env var, mirroring
/// [`resolve_db`]. Returns `None` when the active target is a bare path
/// rather than a registered name — paths have no name to scope by, and
/// a path-valued flag does NOT fall back to the env var (the flag has
/// already selected the active DB).
pub fn active_db_name(cli_db: Option<&str>) -> Option<String> {
    cli_db
        .map(str::to_string)
        .or_else(|| std::env::var("DATUM_DB").ok())
        .filter(|v| !v.is_empty() && !looks_like_path(v))
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
/// at `argv[1]`. Returns `(start_index, end_index_exclusive, value)`.
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
/// occupy `argv[1]` or it shadows the alias name. Strip it for expansion,
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

/// Split the global `--color WHEN` / `--color=WHEN` off argv, as
/// [`split_off_global_db_flag`] does `--db`. Only in front of the command
/// and of any `--`: after the command, `--color` is `track`'s and `setup`'s.
pub fn split_off_global_color_flag(mut argv: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut i = 1;
    while let Some(tok) = argv.get(i) {
        if tok == "--color" || tok.starts_with("--color=") {
            let end = (i + 1 + usize::from(tok == "--color")).min(argv.len());
            let flag = argv.drain(i..end).collect();
            return (argv, flag);
        }
        if !tok.starts_with('-') || tok == "--" {
            break;
        }
        i += 1 + usize::from(tok == "--show");
    }
    (argv, Vec::new())
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

/// Keep a database opened by path among [`known_dbs`], so the sync daemon
/// syncs it (da-zrl7n): a registry entry named by its absolute path, which
/// `--db` reads as that path. A no-op for one known already.
pub fn ensure_known(path: &Path) -> Result<()> {
    let abs = path.canonicalize()?;
    if known_dbs()
        .values()
        .any(|p| p.canonicalize().is_ok_and(|p| p == abs))
    {
        return Ok(());
    }
    let mut registry = load_registry()?;
    let path = abs.to_string_lossy().into_owned();
    registry.db.insert(path.clone(), DbEntry { path });
    save_registry(&registry)
}

/// Location of the DB registry file.
/// Reading it creates nothing; [`save_registry`] makes the directory.
pub fn registry_path() -> Result<PathBuf> {
    Ok(config_dir_path()
        .context("Cannot determine config directory")?
        .join("dbs.toml"))
}

/// This client's id — one per installation, a data directory or a browser
/// profile, not per machine (da-lfsei): `DATUM_CLIENT_ID`, then the toml `client_id` key, then
/// the data directory's file. The one resolver — entries, sync and `config
/// show` all read it, so an entry's `client_id` is its op's (da-mmkre). A
/// toml that does not parse is `config show`'s to report, not a log's.
pub fn client_id() -> Result<String> {
    let toml = load_raw_config_or_default().ok().and_then(|r| r.client_id);
    Ok(resolve_client_id(toml)?.value)
}

fn resolve_client_id(toml: Option<String>) -> Result<SourcedValue<String>> {
    Ok(if let Some(v) = read_env("DATUM_CLIENT_ID") {
        SourcedValue::new(v, Source::Env)
    } else if let Some(v) = toml {
        SourcedValue::new(v, Source::Toml)
    } else {
        SourcedValue::new(file_client_id()?, Source::Default)
    })
}

/// Return (or create) the client ID stored in the data directory. An
/// install from before da-lfsei has it under `device_id`, moved here once.
fn file_client_id() -> Result<String> {
    let dir = data_dir()?;
    let path = dir.join("client_id");
    let old = dir.join("device_id");
    if !path.exists() && old.exists() {
        std::fs::rename(&old, &path).with_context(|| format!("Moving {old:?} to {path:?}"))?;
    }
    if path.exists() {
        let id = std::fs::read_to_string(&path)
            .with_context(|| format!("Reading client_id from {path:?}"))?;
        return Ok(id.trim().to_string());
    }
    let id = ulid::Ulid::new().to_string();
    std::fs::write(&path, &id).with_context(|| format!("Writing client_id to {path:?}"))?;
    Ok(id)
}

fn xdg_data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
}

pub(crate) fn xdg_config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

pub fn data_dir() -> Result<PathBuf> {
    let dir = data_dir_path().context("Cannot determine data directory")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("Creating data directory {dir:?}"))?;
    Ok(dir)
}

/// [`data_dir`] without creating it.
pub fn data_dir_path() -> Option<PathBuf> {
    match std::env::var("DATUM_HOME") {
        Ok(home) => Some(PathBuf::from(home)),
        Err(_) => Some(xdg_data_dir()?.join("datum")),
    }
}

pub fn config_dir() -> Result<PathBuf> {
    let dir = config_dir_path().context("Cannot determine config directory")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("Creating config directory {dir:?}"))?;
    Ok(dir)
}

/// [`config_dir`] without creating it.
pub fn config_dir_path() -> Option<PathBuf> {
    match std::env::var("DATUM_CONFIG_HOME") {
        Ok(home) => Some(PathBuf::from(home)),
        Err(_) => Some(xdg_config_dir()?.join("datum")),
    }
}

/// Every database datum knows of: the registry's names, and every `*.db`
/// in the data directory, the registry's path winning a name collision.
pub fn known_dbs() -> BTreeMap<String, PathBuf> {
    let mut seen: BTreeMap<String, PathBuf> = load_registry()
        .unwrap_or_default()
        .db
        .into_iter()
        .map(|(name, entry)| (name, PathBuf::from(entry.path)))
        .collect();
    for (name, path) in discovered_dbs().unwrap_or_default() {
        seen.entry(name).or_insert(path);
    }
    seen
}

/// Scan the default data directory for `*.db` files.
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
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    /// Delimiter inserted between empty/collapsed `%key%` segments in
    /// slug templates. CLI `--id-delimiter` and `DATUM_ID_DELIMITER` env
    /// var override this. Built-in default is `":"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_delimiter: Option<String>,
    /// Which weekday opens a week, for `stats --week` / `--last-week`.
    /// A weekday name (`"monday"`, `"sun"`) or an ISO number (`1` =
    /// Monday … `7` = Sunday). `DATUM_WEEK_START` overrides it; the
    /// built-in default is Monday.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub week_start: Option<String>,
    /// `true`: `datum check` asks the release whether a newer datum is out
    /// (da-uphgn). Off by default: datum makes no network call of its own
    /// unless you sync, upgrade, or turn this on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_check: Option<bool>,
}

/// Effective resolved config with per-key source provenance.
#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: SourcedValue<String>,
    pub client_id: SourcedValue<String>,
    pub default_field: SourcedValue<Option<String>>,
    pub editor: SourcedValue<String>,
    pub id_delimiter: SourcedValue<String>,
    /// Raw (unparsed) `week_start`, so `config show` can display a value
    /// even when it is misspelled — [`resolve_week_start`] is where a bad
    /// one is rejected, and only for the commands that need a week.
    pub week_start: SourcedValue<String>,
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

/// The editor `datum edit` runs: DATUM_EDITOR, then the toml `editor` key,
/// then $VISUAL, then $EDITOR, then `vi`. The one precedence order, shared by
/// `Config::load` (and so `config show`) and `datum edit` (da-tkkz3).
fn resolve_editor(toml: Option<String>) -> SourcedValue<String> {
    if let Some(v) = read_env("DATUM_EDITOR") {
        SourcedValue::new(v, Source::Env)
    } else if let Some(v) = toml {
        SourcedValue::new(v, Source::Toml)
    } else {
        let v = read_env("VISUAL")
            .or_else(|| read_env("EDITOR"))
            .unwrap_or_else(|| "vi".to_string());
        SourcedValue::new(v, Source::Default)
    }
}

/// [`resolve_editor`] without the rest of [`Config::load`], which would also
/// resolve the database and write a `client_id` file.
pub fn editor() -> Result<String> {
    Ok(resolve_editor(load_raw_config_or_default()?.editor).value)
}

/// Built-in default delimiter for slug-template interpolation. Used when
/// no CLI flag, env var, or `datumrc.toml` value is supplied. Matches the
/// historical hardcoded constant the parser used before R6 layering.
pub fn id_delimiter_default() -> String {
    ":".to_string()
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

        let client_id = resolve_client_id(raw.client_id.clone())?;

        let default_field = if let Some(v) = read_env("DATUM_DEFAULT_FIELD") {
            SourcedValue::new(Some(v), Source::Env)
        } else if let Some(v) = raw.default_field.clone() {
            SourcedValue::new(Some(v), Source::Toml)
        } else {
            SourcedValue::new(None, Source::Default)
        };

        let editor = resolve_editor(raw.editor.clone());

        let id_delimiter = if let Some(v) = read_env("DATUM_ID_DELIMITER") {
            SourcedValue::new(v, Source::Env)
        } else if let Some(v) = raw.id_delimiter.clone() {
            SourcedValue::new(v, Source::Toml)
        } else {
            SourcedValue::new(id_delimiter_default(), Source::Default)
        };

        let week_start = if let Some(v) = read_env("DATUM_WEEK_START") {
            SourcedValue::new(v, Source::Env)
        } else if let Some(v) = raw.week_start.clone() {
            SourcedValue::new(v, Source::Toml)
        } else {
            SourcedValue::new(week_start_default().to_string(), Source::Default)
        };

        Ok(Config {
            db_path,
            client_id,
            default_field,
            editor,
            id_delimiter,
            week_start,
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
                "client_id",
                self.client_id.value.clone(),
                self.client_id.source,
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
            (
                "week_start",
                self.week_start.value.clone(),
                self.week_start.source,
            ),
        ]
    }
}

/// The weekday a week opens on when nothing says otherwise. Monday — the
/// ISO week start, and what `date`/`chrono` call weekday 1.
pub fn week_start_default() -> Weekday {
    Weekday::Mon
}

/// Read a `week_start` value: a weekday name (`"monday"`, `"Mon"`, any
/// case) or an ISO number (`1` = Monday … `7` = Sunday).
///
/// The name is canonical — errors and `datum config show` speak names —
/// but a number is accepted because plenty of config formats spell a
/// weekday that way.
pub fn parse_week_start(s: &str) -> Result<Weekday> {
    let t = s.trim();
    if let Ok(n) = t.parse::<u8>() {
        return match n {
            1 => Ok(Weekday::Mon),
            2 => Ok(Weekday::Tue),
            3 => Ok(Weekday::Wed),
            4 => Ok(Weekday::Thu),
            5 => Ok(Weekday::Fri),
            6 => Ok(Weekday::Sat),
            7 => Ok(Weekday::Sun),
            _ => anyhow::bail!(
                "invalid week_start '{s}': a number must be 1-7 (1 = monday … 7 = sunday)"
            ),
        };
    }
    t.parse::<Weekday>().map_err(|_| {
        anyhow::anyhow!(
            "invalid week_start '{s}': expected a weekday name \
             (monday … sunday, or the three-letter form) or 1-7"
        )
    })
}

/// Resolve the effective week start for a single invocation, layering
/// `DATUM_WEEK_START` env > `datumrc.toml` > Monday. Standalone for the
/// same reason as [`resolve_id_delimiter`]: materializing a full
/// [`Config`] would lazily generate a client id as a side effect.
pub fn resolve_week_start() -> Result<Weekday> {
    if let Some(v) = read_env("DATUM_WEEK_START") {
        return parse_week_start(&v);
    }
    if let Ok(raw) = load_raw_config_or_default() {
        if let Some(v) = raw.week_start {
            return parse_week_start(&v);
        }
    }
    Ok(week_start_default())
}

/// Resolve the effective slug `id_delimiter` for a single invocation,
/// layering CLI flag > `DATUM_ID_DELIMITER` env > `datumrc.toml` >
/// built-in default. Standalone helper so per-entry slug-interpolation
/// sites don't have to materialize a full [`Config`] (which would also
/// trigger `client_id()` lazy generation as a side effect).
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

    #[test]
    fn active_db_name_flag_wins_and_paths_are_nameless() {
        // The `Some` branches never read DATUM_DB, so these are safe to
        // assert without env isolation (da-cyn4).
        assert_eq!(active_db_name(Some("work")), Some("work".to_string()));
        // A path-valued flag selects the DB but yields no name — and
        // must NOT fall back to the env var.
        assert_eq!(active_db_name(Some("/tmp/x.db")), None);
        assert_eq!(active_db_name(Some("")), None);
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

    #[test]
    fn week_start_accepts_weekday_names_in_any_case() {
        for (input, expected) in [
            ("monday", Weekday::Mon),
            ("Monday", Weekday::Mon),
            ("MON", Weekday::Mon),
            ("sun", Weekday::Sun),
            ("  saturday  ", Weekday::Sat),
        ] {
            assert_eq!(parse_week_start(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn week_start_accepts_iso_numbers() {
        // ISO: 1 = Monday … 7 = Sunday.
        assert_eq!(parse_week_start("1").unwrap(), Weekday::Mon);
        assert_eq!(parse_week_start("7").unwrap(), Weekday::Sun);
    }

    #[test]
    fn week_start_rejects_nonsense_by_name() {
        // The name is canonical, so the error says so rather than echoing
        // a number range the user didn't use.
        for bad in ["0", "8", "funday", ""] {
            let err = parse_week_start(bad).unwrap_err().to_string();
            assert!(err.contains("week_start"), "{bad}: {err}");
        }
    }

    #[test]
    fn week_start_defaults_to_monday() {
        assert_eq!(week_start_default(), Weekday::Mon);
    }
}
