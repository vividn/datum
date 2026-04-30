//! End-to-end tests for slice-5 kit indices (da-d4t.5).
//!
//! Covers:
//! - `[[index]]` blocks materialize as real SQLite indices
//! - URL kit installs are blocked
//! - Trust prompt: `--yes` approves; scripted-deny aborts
//! - Re-install drops obsolete indices and recreates current ones
//! - Uninstall drops the kit's indices
//! - DDL allowlist rejects malicious expressions before any DB write

use std::fs;

use anyhow::Result;
use datum_commands::cmd::dispatch_argv;
use datum_commands::commands::kit::{install, TrustGate};
use datum_core::kit::IndexEntry;
use datum_core::test_util::fresh_db;
use datum_parse::cli::KitInstallArgs;

/// Scripted trust gate for tests. Returns the queued answer for each
/// `approve_indices` call; runs out → panics.
struct ScriptedTrust {
    answers: std::collections::VecDeque<bool>,
    pub seen: Vec<(String, usize, bool, bool)>, // (kit, n_indices, is_reinstall, assume_yes)
}

impl ScriptedTrust {
    fn new<I: IntoIterator<Item = bool>>(answers: I) -> Self {
        Self {
            answers: answers.into_iter().collect(),
            seen: Vec::new(),
        }
    }
}

impl TrustGate for ScriptedTrust {
    fn approve_indices(
        &mut self,
        kit_name: &str,
        indices: &[IndexEntry],
        is_reinstall: bool,
        assume_yes: bool,
    ) -> Result<bool> {
        self.seen
            .push((kit_name.into(), indices.len(), is_reinstall, assume_yes));
        Ok(self
            .answers
            .pop_front()
            .expect("scripted trust ran out of answers"))
    }
}

fn write_chores_kit(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("chores.toml");
    fs::write(
        &p,
        r##"[kit]
name = "chores"
version = "0.1"

[[field]]
name = "trash"
default_verb = "occur"

[[index]]
name = "by_field_start"
on = "entries"
expression = "field, start_time DESC"
where = "deleted_at IS NULL"
"##,
    )
    .unwrap();
    p
}

fn index_exists(db: &datum_core::db::Db, on_disk_name: &str) -> bool {
    let n: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            rusqlite::params![on_disk_name],
            |row| row.get(0),
        )
        .unwrap();
    n > 0
}

#[test]
fn install_with_yes_creates_real_index() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_chores_kit(dir.path());
    let db = fresh_db();

    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            "--yes".into(),
            path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();

    // The on-disk index exists in sqlite_master with the expected name.
    assert!(index_exists(&db, "kit_chores__by_field_start"));

    // kit_indices row recorded.
    let installed = db.load_indices_by_kit("chores").unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].index_name, "by_field_start");
    assert_eq!(installed[0].on_disk_name, "kit_chores__by_field_start");
    assert!(installed[0]
        .sql_text
        .contains("ON entries(field, start_time DESC)"));

    // list_kits sees both fields and indices.
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].kit_id, "chores");
    assert_eq!(kits[0].field_count, 1);
    assert_eq!(kits[0].index_count, 1);
}

#[test]
fn install_without_approval_aborts_without_db_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_chores_kit(dir.path());
    let db = fresh_db();

    let mut trust = ScriptedTrust::new([false]);
    let err = install(
        KitInstallArgs {
            path: path.to_string_lossy().into_owned(),
            force: false,
            assume_yes: false,
        },
        &db,
        &mut trust,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("declined"), "msg = {err}");

    // Trust gate was consulted exactly once with the right shape.
    assert_eq!(trust.seen.len(), 1);
    assert_eq!(trust.seen[0].0, "chores");
    assert_eq!(trust.seen[0].1, 1);
    assert!(!trust.seen[0].2, "first install is not a re-install");

    // Nothing was written: no fields, no indices, no rows.
    assert!(db.list_kits().unwrap().is_empty());
    assert!(db.load_indices_by_kit("chores").unwrap().is_empty());
    assert!(!index_exists(&db, "kit_chores__by_field_start"));
}

#[test]
fn install_with_approval_via_scripted_gate() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_chores_kit(dir.path());
    let db = fresh_db();

    let mut trust = ScriptedTrust::new([true]);
    install(
        KitInstallArgs {
            path: path.to_string_lossy().into_owned(),
            force: false,
            assume_yes: false,
        },
        &db,
        &mut trust,
    )
    .unwrap();

    assert!(index_exists(&db, "kit_chores__by_field_start"));
    assert_eq!(db.load_indices_by_kit("chores").unwrap().len(), 1);
}

#[test]
fn url_install_is_blocked_with_clear_error() {
    let db = fresh_db();
    let mut trust = ScriptedTrust::new([true]);
    let err = install(
        KitInstallArgs {
            path: "https://example.com/some.toml".into(),
            force: false,
            assume_yes: true, // even with --yes, URL is blocked
        },
        &db,
        &mut trust,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("URL kit installs"), "msg = {err}");
    assert!(err.contains("signed-registry"), "msg = {err}");
    // Trust gate not even consulted for URLs (we bail before parsing).
    assert!(trust.seen.is_empty());
}

#[test]
fn reinstall_drops_obsolete_index_and_recreates_current() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("k.toml");
    let db = fresh_db();

    // First install: ix_a.
    fs::write(
        &path,
        r##"[kit]
name = "k"

[[index]]
name = "ix_a"
on = "entries"
expression = "field"
"##,
    )
    .unwrap();
    let mut trust = ScriptedTrust::new([true, true]);
    install(
        KitInstallArgs {
            path: path.to_string_lossy().into_owned(),
            force: false,
            assume_yes: false,
        },
        &db,
        &mut trust,
    )
    .unwrap();
    assert!(index_exists(&db, "kit_k__ix_a"));

    // Edit the kit to drop ix_a and add ix_b.
    fs::write(
        &path,
        r##"[kit]
name = "k"

[[index]]
name = "ix_b"
on = "entries"
expression = "slug"
"##,
    )
    .unwrap();
    install(
        KitInstallArgs {
            path: path.to_string_lossy().into_owned(),
            force: false,
            assume_yes: false,
        },
        &db,
        &mut trust,
    )
    .unwrap();

    // Old index gone, new index live.
    assert!(!index_exists(&db, "kit_k__ix_a"));
    assert!(index_exists(&db, "kit_k__ix_b"));

    let installed = db.load_indices_by_kit("k").unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].index_name, "ix_b");

    // Second call was correctly flagged as a re-install.
    assert!(
        trust.seen[1].2,
        "second call should be flagged is_reinstall"
    );
}

#[test]
fn uninstall_drops_kit_indices() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_chores_kit(dir.path());
    let db = fresh_db();

    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            "--yes".into(),
            path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();
    assert!(index_exists(&db, "kit_chores__by_field_start"));

    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "uninstall".into(),
            "chores".into(),
        ],
        &db,
    )
    .unwrap();

    assert!(!index_exists(&db, "kit_chores__by_field_start"));
    assert!(db.load_indices_by_kit("chores").unwrap().is_empty());
    assert!(db.list_kits().unwrap().is_empty());
}

#[test]
fn malicious_ddl_rejected_before_trust_prompt() {
    // A kit author who tries to slip an ATTACH past the validator
    // should fail at parse-validation time. The trust gate is never
    // consulted because the DDL builder bails first.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("evil.toml");
    fs::write(
        &path,
        r##"[kit]
name = "evil"

[[index]]
name = "harmless_looking"
on = "entries"
expression = "field"
where = "1=1; ATTACH DATABASE '/tmp/x' AS evil"
"##,
    )
    .unwrap();
    let db = fresh_db();
    let mut trust = ScriptedTrust::new([true]);
    let err = install(
        KitInstallArgs {
            path: path.to_string_lossy().into_owned(),
            force: false,
            assume_yes: true,
        },
        &db,
        &mut trust,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("';'") || err.contains("ATTACH"), "msg = {err}");
    assert!(
        trust.seen.is_empty(),
        "trust gate must not see invalid DDL — {:?}",
        trust.seen
    );
}

#[test]
fn install_with_only_indices_no_fields_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("idxonly.toml");
    fs::write(
        &path,
        r##"[kit]
name = "idxonly"

[[index]]
on = "entries"
name = "by_slug"
expression = "slug"
"##,
    )
    .unwrap();
    let db = fresh_db();
    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            "--yes".into(),
            path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();

    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].field_count, 0);
    assert_eq!(kits[0].index_count, 1);
    assert!(index_exists(&db, "kit_idxonly__by_slug"));
}
