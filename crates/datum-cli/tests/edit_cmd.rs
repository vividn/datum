//! `datum edit` with no id edits `_LAST` (da-llcb7), and a stale `_LAST`
//! asks first, as `update` does.

use chrono::{Duration, Utc};
use datum_core::db::Db;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `datum <args>` as a process on `dir/e.db`, with `input` on stdin and an
/// editor that copies the file it is handed to `dir/opened.json`.
fn run_bin(dir: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write;
    let editor = dir.join("editor.sh");
    std::fs::write(
        &editor,
        format!(
            "#!/bin/sh\ncp \"$1\" '{}'\n",
            dir.join("opened.json").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.join("e.db"))
        .env("DATUM_HOME", dir.join("home"))
        .env("DATUM_CONFIG_HOME", dir.join("home"))
        .env("DATUM_EDITOR", &editor)
        .env("NO_COLOR", "1")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn datum");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait datum")
}

fn seed(dir: &Path) {
    for args in [&["occur", "coffee"][..], &["switch", "mood", "happy"]] {
        assert!(run_bin(dir, args, "").status.success(), "{args:?}");
    }
}

#[test]
fn a_bare_edit_opens_the_last_entry() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path());

    let out = run_bin(tmp.path(), &["edit"], "");
    assert!(out.status.success(), "{out:?}");
    let opened: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join("opened.json")).unwrap())
            .unwrap();
    assert_eq!(opened["field"], "mood");
    assert_eq!(opened["state"], "happy");
}

#[test]
fn a_stale_last_asks_first_and_a_no_opens_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path());
    Db::open(tmp.path().join("e.db").to_str().unwrap())
        .unwrap()
        .conn()
        .execute(
            "UPDATE last_docs SET time = ?1",
            [(Utc::now() - Duration::minutes(16)).to_rfc3339()],
        )
        .unwrap();

    let out = run_bin(tmp.path(), &["edit"], "n\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[Y/n]"), "a stale _LAST must ask: {stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("Aborted."));
    assert!(!tmp.path().join("opened.json").exists(), "no editor ran");

    // Named, the entry opens without asking.
    let out = run_bin(tmp.path(), &["edit", "coffee"], "");
    assert!(out.status.success(), "{out:?}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("[Y/n]"));
    assert!(tmp.path().join("opened.json").exists());
}
