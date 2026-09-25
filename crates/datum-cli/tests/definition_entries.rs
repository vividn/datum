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
