//! `db v2import` and `db restore` say how far they are, on stderr (da-h9vj):
//! a ~50 MB import used to run in silence. Stdout stays the command's own —
//! backup and restore pipe JSONL through it.

use std::io::Write;
use std::process::{Command, Output, Stdio};

const N: usize = 3000;

fn datum(db: &std::path::Path, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "0")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run datum binary");
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

#[test]
fn v2import_and_restore_report_progress_on_stderr_only() {
    let dir = tempfile::tempdir().unwrap();
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

    let first = dir.path().join("first.db");
    let out = datum(
        &first,
        &["db", "v2import", "-"],
        backup.to_string().as_bytes(),
    );
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        stderr.contains(&format!("v2import: reading {N}/{N}\n")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("v2import: inserting {N}/{N}\n")),
        "{stderr}"
    );
    assert!(stdout.contains(&format!("{N} imported")), "{stdout}");
    assert!(!stdout.contains("v2import:"), "{stdout}");

    // Backup's stdout is the JSONL restore reads: nothing else in it.
    let jsonl = datum(&first, &["db", "backup", "-"], b"").stdout;
    for line in String::from_utf8_lossy(&jsonl).lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("a JSON line");
    }

    let out = datum(
        &dir.path().join("second.db"),
        &["db", "restore", "-"],
        &jsonl,
    );
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    // The backup also carries the TRACK document the import declared
    // `coffee` with (da-c8mh) and its spec (da-lc26c), two rows more.
    let restored = N + 2;
    assert!(
        stderr.contains(&format!("restore: reading {restored}/{restored}\n")),
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("restore: inserting {restored}/{restored}\n")),
        "{stderr}"
    );
    assert!(!stdout.contains("restore:"), "{stdout}");
    assert!(stdout.contains(&format!("{restored} inserted")), "{stdout}");

    // Importing the same backup again is still a no-op.
    let out = datum(
        &first,
        &["db", "v2import", "-"],
        backup.to_string().as_bytes(),
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("0 imported"));
}
