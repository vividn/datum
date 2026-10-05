//! Views, kit commands and kit index definitions are entries (da-fwwtn),
//! following aliases (da-b3752): a kit's `[[view]]`, `[[command]]` and
//! `[[index]]` land as `view:<name>`, `command:<name>` and
//! `index:<kit>/<name>`, so they back up, sync and restore like any doc. Only
//! an index's definition travels; its SQLite index is built on each device.
//!
//! The real binary: what is asserted is what each command PRINTS, and which
//! indices the database file holds.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        }
    }

    fn db(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn run_on(&self, db: &Path, args: &[&str], stdin: &[u8]) -> String {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", db)
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("DATUM_COLOR", "0")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn datum");
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        self.run_on(&self.db("data.db"), args, b"")
    }

    /// Install the kit `name`: a view of its field, a command logging it,
    /// and an index on it.
    fn kit(&self, name: &str) {
        let toml = format!(
            r#"[kit]
name = "{name}"

[[view]]
name = "{name}_log"
sql = "SELECT state FROM entries WHERE field = '{name}' AND deleted_at IS NULL ORDER BY occur_time"

[[command]]
name = "{name}_up"
expands_to = "switch {name} up"

[[index]]
name = "by_state"
on = "entries"
expression = "field, state"
where = "field = '{name}'"
"#
        );
        let path = self.tmp.path().join(format!("{name}.toml"));
        std::fs::write(&path, toml).unwrap();
        self.ok(&["kit", "install", "--yes", path.to_str().unwrap()]);
    }
}

/// The kit indices the database file at `db` holds.
fn kit_indices(db: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'kit_%' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// A kit with a view, a command and an index round-trips: backed up and
/// restored into a fresh database, the view gives what it gave, the
/// command runs, and the index exists there. None of the three shows on
/// the life-data surfaces.
#[test]
fn a_kit_backs_up_and_restores_into_a_fresh_db() {
    let env = Env::new();
    env.kit("mood");
    env.ok(&["mood_up"]);
    let view = env.ok(&["view", "mood_log", "--format", "tsv"]);
    assert!(view.contains("up"), "{view}");
    let tail = env.ok(&["tail"]);
    assert!(
        !["view", "command", "index"]
            .iter()
            .any(|f| tail.contains(&format!("{f}:"))),
        "{tail}"
    );

    let backup = env.ok(&["db", "backup", "-"]);
    let fresh = env.db("fresh.db");
    env.run_on(&fresh, &["db", "restore", "-"], backup.as_bytes());
    assert_eq!(kit_indices(&fresh), ["kit_mood__by_state"]);
    assert_eq!(
        env.run_on(&fresh, &["view", "mood_log", "--format", "tsv"], b""),
        view
    );
    env.run_on(&fresh, &["mood_up"], b"");
    assert_eq!(env.run_on(&fresh, &["count", "mood"], b"").trim(), "2");
    let listed = env.run_on(&fresh, &["kit", "list"], b"");
    assert!(listed.contains("mood"), "{listed}");
}

/// Uninstall takes a kit's own view, command and index — the definition
/// and the SQLite index both — and leaves another kit's alone.
#[test]
fn uninstall_removes_only_its_own() {
    let env = Env::new();
    env.kit("mood");
    env.kit("sleep");
    assert_eq!(
        kit_indices(&env.db("data.db")),
        ["kit_mood__by_state", "kit_sleep__by_state"]
    );

    env.ok(&["kit", "uninstall", "mood"]);
    assert_eq!(kit_indices(&env.db("data.db")), ["kit_sleep__by_state"]);
    let views = env.ok(&["view", "--list"]);
    assert!(
        views.contains("sleep_log") && !views.contains("mood_log"),
        "{views}"
    );
    env.ok(&["sleep_up"]);
    let exported = env.ok(&["kit", "export", "sleep"]);
    assert!(
        exported.contains("expression = \"field, state\""),
        "{exported}"
    );
}

/// `kit show` reads out what the sample kit installed: its fields with verb
/// and colour, its aliases, its view, its command (da-4mu1a).
#[test]
fn kit_show_reads_out_the_sample_kit() {
    let env = Env::new();
    let sample = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kits/sample.toml");
    env.ok(&["kit", "install", "--yes", sample]);
    let shown = env.ok(&["kit", "show", "sample"]);
    for line in [
        "kit 'sample'",
        "fields (8):",
        "  coffee           occur   #8d5524",
        "  mood             switch  (auto)",
        "aliases (4):",
        "  cof              occur coffee",
        "views (1):",
        "  water_by_day",
        "commands (1):",
        "  hydration        Water drunk per day, under 1.5 l in yellow",
    ] {
        assert!(shown.lines().any(|l| l == line), "no `{line}` in:\n{shown}");
    }
    assert!(
        !shown.contains("indices"),
        "the sample kit has none:\n{shown}"
    );
}

/// A kit's `[kit]` table is the entry `kit:<name>` (da-xnzc7): export
/// writes back the version and description it was installed with, with
/// nothing above `[kit]`; `kit list` and `kit show` name them; a reinstall
/// keeps it unless `--force`, as the kit's other rows are kept.
#[test]
fn a_kits_version_and_description_round_trip() {
    let env = Env::new();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kits/sample.toml");
    let sample = std::fs::read_to_string(path).unwrap();
    let meta = datum_core::kit::KitToml::from_str(&sample).unwrap().kit;
    assert!(
        meta.description.is_some(),
        "the sample kit describes itself"
    );
    env.ok(&["kit", "install", "--yes", path]);

    let exported = env.ok(&["kit", "export", "sample"]);
    assert!(exported.starts_with("[kit]\n"), "{exported}");
    let back = datum_core::kit::KitToml::from_str(&exported).unwrap().kit;
    assert_eq!(back.version.as_deref(), Some("0.1"));
    assert_eq!(back.description, meta.description);

    let shown = env.ok(&["kit", "show", "sample"]);
    assert!(shown.lines().any(|l| l == "version 0.1"), "{shown}");
    assert!(
        shown.contains(meta.description.as_deref().unwrap()),
        "{shown}"
    );

    let version = || {
        let listed = env.ok(&["kit", "list"]);
        let row = listed.lines().find(|l| l.starts_with("sample "));
        row.and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or_else(|| panic!("no `sample` row in:\n{listed}"))
            .to_string()
    };
    assert_eq!(version(), "0.1");
    let newer = env.tmp.path().join("sample.toml");
    std::fs::write(
        &newer,
        sample.replace("version = \"0.1\"", "version = \"0.2\""),
    )
    .unwrap();
    let newer = newer.to_str().unwrap();
    env.ok(&["kit", "install", "--yes", newer]);
    assert_eq!(version(), "0.1", "kept without --force");
    env.ok(&["kit", "install", "--yes", "--force", newer]);
    assert_eq!(version(), "0.2");

    env.ok(&["kit", "uninstall", "sample"]);
    assert_eq!(env.ok(&["kit", "list"]), "No kits installed.\n");
}

/// A `unique` index definition from a copy that installed it before kits
/// were refused one (da-9lo90) — here by a merging restore, as it would by
/// sync — is built on no copy, so none refuses a row another accepted:
/// `kit show` and `check` name it, and the kit's other index is built.
#[test]
fn a_unique_index_definition_is_never_built_and_is_named() {
    let env = Env::new();
    let toml = r#"[kit]
name = "k"

[[index]]
name = "one_coffee"
on = "entries"
expression = "field"
where = "field = 'coffee'"

[[index]]
name = "by_field"
on = "entries"
expression = "field"
"#;
    let path = env.tmp.path().join("k.toml");
    std::fs::write(&path, toml).unwrap();
    env.ok(&["kit", "install", "--yes", path.to_str().unwrap()]);
    let backup = env.ok(&["db", "backup", "-"]);
    let unique = backup.replace(
        r#""name":"one_coffee","on":"entries","unique":false"#,
        r#""name":"one_coffee","on":"entries","unique":true"#,
    );
    assert_ne!(unique, backup, "the definition carries `unique`");

    let here = env.db("here.db");
    env.run_on(&here, &["occur", "coffee", "-t", "900"], b"");
    env.run_on(&here, &["db", "restore", "-", "--merge"], unique.as_bytes());
    assert_eq!(kit_indices(&here), ["kit_k__by_field"]);
    env.run_on(&here, &["occur", "coffee", "-t", "1000"], b"");

    let shown = env.run_on(&here, &["kit", "show", "k"], b"");
    let lines: Vec<&str> = shown.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("  ✗ one_coffee"))
        .unwrap_or_else(|| panic!("no failed one_coffee in:\n{shown}"));
    assert!(
        lines[at + 1].contains("not built on this device")
            && lines[at + 1].contains("`unique = true` is not allowed"),
        "{shown}"
    );
    assert!(lines.contains(&"  ✓ by_field ON entries(field)"), "{shown}");

    let checked = env.run_on(&here, &["check"], b"");
    assert!(
        checked.contains("kit_index_unbuilt (x1)") && checked.contains("index:k/one_coffee"),
        "{checked}"
    );
}
