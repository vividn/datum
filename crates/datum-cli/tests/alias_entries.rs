//! Aliases are entries (da-b3752), following specs (da-lc26c): a user's or
//! kit's alias is the entry `alias:<name>` of field `alias`, so it backs up,
//! syncs and edits like any doc, while staying off the life-data surfaces.
//!
//! The real binary: what is asserted is what each command PRINTS.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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

    fn run_on(&self, db: &Path, args: &[&str], stdin: &[u8]) -> Output {
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
        out
    }

    fn ok(&self, args: &[&str]) -> String {
        String::from_utf8(self.run_on(&self.db("data.db"), args, b"").stdout).unwrap()
    }

    fn kit(&self, name: &str, aliases: &[(&str, &str)]) {
        let mut toml = format!("[kit]\nname = \"{name}\"\n");
        for (a, to) in aliases {
            toml += &format!("\n[[alias]]\nname = \"{a}\"\nexpands_to = \"{to}\"\n");
        }
        let path = self.tmp.path().join(format!("{name}.toml"));
        std::fs::write(&path, toml).unwrap();
        self.ok(&["kit", "install", path.to_str().unwrap()]);
    }
}

/// `alias set` is an entry write: `undo` steps an overwrite back and a new
/// alias away, `alias remove` is undone too, and `history alias:vbp` lists
/// the versions.
#[test]
fn setting_an_alias_then_undo() {
    let env = Env::new();
    env.ok(&["alias", "set", "vbp", "note word pos=verb"]);
    env.ok(&["alias", "set", "vbp", "note word pos=noun"]);
    env.ok(&["undo"]);
    assert_eq!(
        env.ok(&["alias", "show", "vbp"]).trim(),
        "vbp → datum note word pos=verb"
    );
    assert!(env.ok(&["history", "alias:vbp"]).contains("undo"));

    env.ok(&["alias", "remove", "vbp"]);
    assert!(env.ok(&["alias", "show", "vbp"]).contains("No alias"));
    env.ok(&["undo"]);
    assert!(env.ok(&["alias", "show", "vbp"]).contains("pos=verb"));

    env.ok(&["alias", "set", "cof", "occur coffee"]);
    env.ok(&["undo"]);
    assert!(env.ok(&["alias", "show", "cof"]).contains("No alias"));
}

/// A backup carries the aliases: restored into a fresh database, each one
/// resolves again. An alias entry stays off the life-data surfaces.
#[test]
fn backup_then_restore_into_a_fresh_db_keeps_aliases() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    env.ok(&["alias", "set", "cof", "occur coffee"]);
    let tail = env.ok(&["tail"]);
    assert!(!tail.contains("alias"), "{tail}");
    assert!(env.ok(&["tail", "alias"]).contains("alias"));

    let backup = env
        .run_on(&env.db("data.db"), &["db", "backup", "-"], b"")
        .stdout;
    let fresh = env.db("fresh.db");
    env.run_on(&fresh, &["db", "restore", "-"], &backup);
    let shown = env.run_on(&fresh, &["alias", "show", "cof"], b"").stdout;
    assert_eq!(
        String::from_utf8(shown).unwrap().trim(),
        "cof → datum occur coffee"
    );
    env.run_on(&fresh, &["cof"], b"");
    let count = env.run_on(&fresh, &["count", "coffee"], b"").stdout;
    assert_eq!(String::from_utf8(count).unwrap().trim(), "2");
}

/// A kit's aliases are its: install marks them, export reads them, and
/// uninstall takes only its own — never the person's, never another kit's.
#[test]
fn a_kit_round_trip_removes_only_its_own() {
    let env = Env::new();
    env.ok(&["alias", "set", "mine", "occur mine"]);
    env.kit("drinks", &[("cof", "occur coffee"), ("tea", "occur tea")]);
    env.kit("water", &[("wat", "occur water")]);

    assert!(env.ok(&["kit", "list"]).contains("drinks"));
    let exported = env.ok(&["kit", "export", "drinks"]);
    assert!(
        exported.contains("cof") && exported.contains("tea") && !exported.contains("wat"),
        "{exported}"
    );

    env.ok(&["kit", "uninstall", "drinks"]);
    for gone in ["cof", "tea"] {
        assert!(
            env.ok(&["alias", "show", gone]).contains("No alias"),
            "{gone}"
        );
    }
    assert!(env.ok(&["alias", "show", "wat"]).contains("occur water"));
    assert!(env.ok(&["alias", "show", "mine"]).contains("occur mine"));
}
