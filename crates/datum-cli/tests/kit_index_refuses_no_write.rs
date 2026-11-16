//! A kit index never refuses a write (da-9uvmk, F26): one whose expression
//! errors on a row is dropped, named on stderr and by `datum check`, and the
//! write lands — a log, a synced op, a delete and a kit's own uninstall.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

/// SQLite's integer overflow, an error raised as the row is indexed.
const OVERFLOW: &str = "abs(-9223372036854775808)";

/// A copy of a database with its own home, run as the real binary.
struct Copy {
    tmp: TempDir,
}

impl Copy {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn db(&self) -> PathBuf {
        self.tmp.path().join("data.db")
    }

    fn run(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env_remove("DATUM_SCRIPT")
            .env("DATUM_DB", self.db())
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    /// Run `args`, which must succeed; its stderr.
    fn ok(&self, args: &[&str]) -> String {
        self.ok_in(args, "")
    }

    fn ok_in(&self, args: &[&str], stdin: &str) -> String {
        let out = self.run(args, stdin);
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
        err
    }

    /// Install a kit `name` whose indexes are `(index name, expression)`.
    fn install(&self, name: &str, indexes: &[(&str, &str)]) {
        let mut toml = format!("[kit]\nname = \"{name}\"\n");
        for (ix, expr) in indexes {
            toml += &format!(
                "\n[[index]]\nname = \"{ix}\"\non = \"entries\"\nexpression = \"{expr}\"\n"
            );
        }
        let path = self.tmp.path().join(format!("{name}.toml"));
        std::fs::write(&path, toml).unwrap();
        self.ok(&["kit", "install", &path.to_string_lossy(), "--yes"]);
    }

    /// The kit indexes built on this copy, read without opening it as datum.
    fn built(&self) -> Vec<String> {
        let conn = rusqlite::Connection::open(self.db()).unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'kit%' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// The live entries of `field`, by their `x`.
    fn xs(&self, field: &str) -> Vec<String> {
        let conn = rusqlite::Connection::open(self.db()).unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT json_extract(data, '$.x') FROM entries
                 WHERE field = ?1 AND deleted_at IS NULL ORDER BY at",
            )
            .unwrap();
        stmt.query_map([field], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// `datum check`'s words for the kit indexes it finds unbuilt.
    fn check(&self) -> String {
        String::from_utf8(self.run(&["check"], "").stdout).unwrap()
    }
}

/// Errors on a row whose `x` is `boom`.
fn on_boom() -> String {
    format!("(CASE WHEN json_extract(data, '$.x') = 'boom' THEN {OVERFLOW} ELSE 0 END)")
}

const BOTH: [&str; 2] = ["kit_f26probe__filt", "kit_f26probe__plain"];

/// (a) and (d): a log past the index's condition lands, the index is dropped
/// and named, a later log lands, and the kit's index that never errors stays.
#[test]
fn a_log_an_index_errors_on_lands_and_drops_that_index() {
    let c = Copy::new();
    c.install("f26probe", &[("filt", &on_boom()), ("plain", "field")]);
    assert_eq!(c.built(), BOTH);
    assert_eq!(c.ok(&["occur", "probe", "x=ok"]), "");
    let err = c.ok(&["occur", "probe", "x=boom"]);
    assert!(
        err.contains("kit_f26probe__filt") && err.contains("integer overflow"),
        "{err}"
    );
    assert!(!err.contains("plain"), "{err}");
    c.ok(&["occur", "probe", "x=ok2"]);
    assert_eq!(c.xs("probe"), ["ok", "boom", "ok2"]);
    assert_eq!(
        c.built(),
        ["kit_f26probe__plain"],
        "only the one refusing goes"
    );
    assert!(c.check().contains("kit_f26probe__filt"), "{}", c.check());
}

/// (b): a synced op past the condition applies on the copy that built the
/// index, and so do its sender's later ops in the same receive.
#[test]
fn a_synced_op_an_index_errors_on_applies_and_so_do_the_later_ones() {
    let (a, b) = (Copy::new(), Copy::new());
    b.install("f26probe", &[("filt", &on_boom()), ("plain", "field")]);
    for x in ["x=ok", "x=boom", "x=ok2"] {
        a.ok(&["occur", "probe", x]);
    }
    let vector = String::from_utf8(b.run(&["sync", "pipe", "vector"], "").stdout).unwrap();
    let ops = String::from_utf8(
        a.run(&["sync", "pipe", "ops", "--since", vector.trim()], "")
            .stdout,
    )
    .unwrap();
    let err = b.ok_in(&["sync", "pipe", "apply"], &ops);
    assert!(err.contains("kit_f26probe__filt"), "{err}");
    assert_eq!(b.xs("probe"), ["ok", "boom", "ok2"]);
    assert_eq!(b.built(), ["kit_f26probe__plain"]);
}

/// An update past the condition lands as a log does.
#[test]
fn an_update_an_index_errors_on_lands() {
    let c = Copy::new();
    c.install("f26probe", &[("filt", &on_boom()), ("plain", "field")]);
    c.ok(&["occur", "probe", "x=ok"]);
    let err = c.ok(&["update", "probe", "x=boom"]);
    assert!(err.contains("kit_f26probe__filt"), "{err}");
    assert_eq!(c.xs("probe"), ["boom"]);
    assert_eq!(c.built(), ["kit_f26probe__plain"]);
}

/// (c): an index keyed on what a delete sets refuses neither a delete nor
/// the uninstall of its own kit.
fn on_delete() -> String {
    format!("(CASE WHEN deleted_at IS NOT NULL THEN {OVERFLOW} ELSE 0 END)")
}

#[test]
fn a_delete_an_index_errors_on_lands() {
    let c = Copy::new();
    c.install("f26del", &[("del", &on_delete())]);
    c.ok(&["occur", "probe", "x=ok"]);
    let err = c.ok(&["delete", "probe"]);
    assert!(err.contains("kit_f26del__del"), "{err}");
    assert!(c.xs("probe").is_empty(), "the delete landed");
    assert!(c.built().is_empty());
}

#[test]
fn the_uninstall_of_a_kit_whose_index_errors_on_it_lands() {
    let c = Copy::new();
    c.install("f26del", &[("del", &on_delete())]);
    assert_eq!(c.built(), ["kit_f26del__del"]);
    c.ok(&["kit", "uninstall", "f26del"]);
    assert!(c.built().is_empty());
    let kits = String::from_utf8(c.run(&["kit", "list"], "").stdout).unwrap();
    assert!(!kits.contains("f26del"), "{kits}");
}
