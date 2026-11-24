//! A backup killed partway leaves its path as it was (da-sovul): absent, or
//! the previous backup, never a cut file that a restore reads as the whole
//! database and deletes what the cut dropped.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const N: usize = 5000;

fn datum(dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
    cmd.current_dir(dir)
        .env("DATUM_DB", dir.join("a.db"))
        .env("HOME", dir)
        .env("NO_COLOR", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// Run `datum db backup b.jsonl [extra]` and SIGKILL it once a file for
/// `b.jsonl` has started filling, whichever name it is written under.
fn kill_partway(dir: &Path, extra: &[&str]) {
    let mut child = datum(dir)
        .args(["db", "backup", "b.jsonl"])
        .args(extra)
        .spawn()
        .unwrap();
    let started = Instant::now();
    let previous = std::fs::read(dir.join("b.jsonl")).ok();
    loop {
        let filling = std::fs::read_dir(dir).unwrap().any(|e| {
            let e = e.unwrap();
            let name = e.file_name().to_string_lossy().into_owned();
            name.contains("b.jsonl")
                && e.metadata().unwrap().len() > 0
                && (name != "b.jsonl"
                    || previous.as_deref() != std::fs::read(e.path()).ok().as_deref())
        });
        if filling || started.elapsed() > Duration::from_secs(30) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn a_backup_killed_partway_leaves_its_path_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let base: chrono::DateTime<chrono::Utc> = "2025-01-01T00:00:00Z".parse().unwrap();
    let docs: Vec<_> = (0..N)
        .map(|i| {
            let utc = (base + chrono::Duration::minutes(i as i64)).to_rfc3339();
            serde_json::json!({
                "_id": format!("coffee:{utc}"),
                "data": {"field": "coffee", "occurTime": {"utc": utc, "o": 0}},
                "meta": {"createTime": {"utc": utc, "o": 0}},
            })
        })
        .collect();
    let backup = serde_json::json!({"backupTime": "2025-02-01T00:00:00Z", "docs": docs});
    let mut import = datum(dir)
        .args(["db", "v2import", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    import
        .stdin
        .take()
        .unwrap()
        .write_all(backup.to_string().as_bytes())
        .unwrap();
    assert!(import.wait().unwrap().success());

    kill_partway(dir, &[]);
    assert!(!dir.join("b.jsonl").exists(), "a cut file at the path");

    // Left alone, it is whole: a header and every entry (the field's spec too).
    assert!(datum(dir)
        .args(["db", "backup", "b.jsonl"])
        .status()
        .unwrap()
        .success());
    let whole = std::fs::read(dir.join("b.jsonl")).unwrap();
    let lines = whole
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .count();
    let header: serde_json::Value =
        serde_json::from_slice(whole.split(|&b| b == b'\n').next().unwrap()).unwrap();
    assert_eq!(lines, header["entry_count"].as_u64().unwrap() as usize + 1);
    assert!(lines > N, "{lines} lines");

    // Killed while replacing it, the previous backup stays byte for byte.
    datum(dir).args(["moment", "tea"]).status().unwrap();
    kill_partway(dir, &["--overwrite"]);
    assert_eq!(std::fs::read(dir.join("b.jsonl")).unwrap(), whole);
}
