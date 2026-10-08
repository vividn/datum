//! Every command that writes a SET of entries is one action on the undo
//! stack, so one bare `undo` reverts the batch and one `redo` re-applies it
//! (da-yyjob, da-z0zh3). Each test runs a batch command, then `undo`, and the live
//! entries (specs and aliases among them) are the ones before the command;
//! then `redo`, and they are the ones after. Tombstones may differ. The real
//! binary; the database is read back with SQLite itself.

use std::path::PathBuf;
use std::process::{Command, Output};

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

    fn path(&self, name: &str) -> String {
        self.tmp.path().join(name).to_string_lossy().into_owned()
    }

    fn db(&self) -> PathBuf {
        self.tmp.path().join("data.db")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.db())
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn sql(&self, sql: &str) {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }

    /// The live entries, by what they say rather than when they were written.
    fn live(&self) -> Vec<Vec<Option<String>>> {
        let conn = rusqlite::Connection::open(self.db()).unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT hid, slug, field, state, dur, following_state, data, occur_time
                 FROM entries WHERE deleted_at IS NULL AND superseded_by IS NULL ORDER BY hid",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| (0..8).map(|i| r.get(i)).collect())
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        rows
    }

    /// Run `batch`, then prove one `undo` takes the whole of it back and one
    /// `redo` puts the whole of it back again.
    fn round_trips(&self, batch: &[&str]) {
        let before = self.live();
        self.ok(batch);
        let after = self.live();
        assert_ne!(before, after, "`datum {}` changed nothing", batch.join(" "));
        self.ok(&["undo"]);
        assert_eq!(
            self.live(),
            before,
            "undo after `datum {}`",
            batch.join(" ")
        );
        self.ok(&["redo"]);
        assert_eq!(self.live(), after, "redo after `datum {}`", batch.join(" "));
    }

    /// A backup, then entries inserted, updated and deleted since it.
    fn backed_up_then_changed(&self) -> String {
        let backup = self.path("b.jsonl");
        self.ok(&["occur", "tea"]);
        self.ok(&["occur", "juice", "n=1"]);
        self.ok(&["alias", "set", "zz", "occur tea"]);
        self.ok(&["db", "backup", &backup]);
        self.ok(&["update", "juice:_", "n=2"]);
        self.ok(&["occur", "coffee"]);
        self.ok(&["alias", "remove", "zz"]);
        backup
    }
}

#[test]
fn restore_replace() {
    let env = Env::new();
    let backup = env.backed_up_then_changed();
    env.round_trips(&["db", "restore", &backup, "--yes"]);
}

#[test]
fn restore_merge() {
    let env = Env::new();
    let backup = env.backed_up_then_changed();
    env.ok(&["delete", "tea:_"]);
    env.round_trips(&["db", "restore", &backup, "--merge"]);
}

const KIT: &str = r#"[kit]
name = "k"

[[field]]
name = "mood"
verb = "switch"
color = "yellow"

[[field]]
name = "tea"
verb = "occur"
color = "green"

[[alias]]
name = "wa"
expands_to = "occur water"
"#;

#[test]
fn kit_install() {
    let env = Env::new();
    env.ok(&["occur", "tea"]);
    std::fs::write(env.path("k.toml"), KIT).unwrap();
    env.round_trips(&["kit", "install", &env.path("k.toml"), "--force"]);
}

#[test]
fn kit_uninstall() {
    let env = Env::new();
    std::fs::write(env.path("k.toml"), KIT).unwrap();
    env.ok(&["kit", "install", &env.path("k.toml")]);
    env.ok(&["occur", "tea"]);
    env.round_trips(&["kit", "uninstall", "k"]);
}

#[test]
fn bulk_retime() {
    let env = Env::new();
    env.ok(&["occur", "tea", "-t", "-20"]);
    env.ok(&["occur", "tea", "-t", "-10"]);
    env.ok(&["occur", "juice"]);
    env.round_trips(&["retime", "--field", "tea", "--offset", "PT5M"]);
}

#[test]
fn bulk_delete() {
    let env = Env::new();
    env.ok(&["occur", "tea", "-t", "-20"]);
    env.ok(&["occur", "juice", "-t", "-10"]);
    env.ok(&["occur", "coffee"]);
    env.round_trips(&["delete", "tea:_,juice:_"]);
}

#[test]
fn rekey_over_several() {
    let env = Env::new();
    env.ok(&["occur", "tea", "n=1", "-t", "-20"]);
    env.ok(&["occur", "juice", "n=2", "-t", "-10"]);
    env.ok(&["occur", "coffee"]);
    env.round_trips(&["rekey", "tea:_,juice:_", "n=count"]);
}

#[test]
fn reparse_fix_over_several() {
    let env = Env::new();
    env.ok(&["occur", "tea", "n=1", "-t", "-20"]);
    env.ok(&["occur", "juice", "n=2", "-t", "-10"]);
    env.ok(&["occur", "coffee"]);
    // Rows older than history whose stored shape drifted from what their
    // raw_input parses to.
    env.sql(
        "UPDATE entries SET data = '{\"n\":9}' WHERE field IN ('tea', 'juice');
         DELETE FROM entry_history WHERE entry_hid IN
           (SELECT hid FROM entries WHERE field IN ('tea', 'juice'));",
    );
    env.round_trips(&["reparse", "tea:_,juice:_", "--fix"]);
}

#[test]
fn v2import() {
    let env = Env::new();
    env.ok(&["occur", "tea"]);
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/datum_backup_3days.json"
    );
    env.round_trips(&["db", "v2import", "--from", fixture]);
}

/// da-z0zh3: an imported row gone since (forgotten, say — the row itself,
/// not a tombstone) has nothing to move. The undo skips it and still takes
/// the rest of the import back.
#[test]
fn undo_of_an_import_skips_a_row_gone_since() {
    let env = Env::new();
    env.ok(&["occur", "tea"]);
    let before = env.live();
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/datum_backup_3days.json"
    );
    env.ok(&["db", "v2import", "--from", fixture]);
    env.sql("DELETE FROM entries WHERE hid = 'syn0001'");
    env.ok(&["undo"]);
    assert_eq!(env.live(), before);
}

/// An entry older than its trail — here bulk-imported, so it has none — is
/// reverted by `undo`, not deleted as though the batch had created it.
#[test]
fn restore_over_imported_entries() {
    let env = Env::new();
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/datum_backup_3days.json"
    );
    env.ok(&["db", "v2import", "--from", fixture]);
    let backup = env.path("b.jsonl");
    env.ok(&["db", "backup", &backup]);
    env.sql(
        "UPDATE entries SET data = '{\"n\":9}', content_hash = 'drifted'
         WHERE hid IN ('syn0001', 'syn0002')",
    );
    env.round_trips(&["db", "restore", &backup, "--yes"]);
}

/// A comma-list `delete -c` over rows with no trail: one `undo` brings every
/// row back as it was, without the `deleted: <why>` it wrote onto them.
#[test]
fn bulk_delete_with_a_reason_over_imported_entries() {
    let env = Env::new();
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/datum_backup_3days.json"
    );
    env.ok(&["db", "v2import", "--from", fixture]);
    env.round_trips(&["delete", "syn0001,syn0002", "-c", "wrong day"]);
}

#[test]
fn restore_hard_says_it_cannot_be_undone() {
    let env = Env::new();
    let backup = env.backed_up_then_changed();
    let out = env.ok(&["db", "restore", &backup, "--hard", "--yes"]);
    assert!(out.contains("cannot be undone"), "{out}");
}
