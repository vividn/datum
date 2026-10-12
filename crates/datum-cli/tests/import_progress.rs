//! `db v2import` and `db restore` say how far they are, on stderr (da-h9vj):
//! a ~50 MB import used to run in silence. Stdout stays the command's own —
//! backup and restore pipe JSONL through it.

use std::io::Write;
use std::process::{Command, Output, Stdio};

const N: usize = 3000;

fn datum(db: &std::path::Path, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("NO_COLOR", "1")
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
    // The backup also carries the spec the import declared `coffee` with
    // (da-lc26c), one row more; no TRACK row (da-r6ofr).
    let restored = N + 1;
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

/// The summary lists every skipped doc by reason and v2 `_id`, and a CouchDB
/// design doc is neither imported nor skipped, only counted (da-3clr4).
#[test]
fn v2import_lists_skipped_docs_and_ignores_design_docs() {
    let dir = tempfile::tempdir().unwrap();
    let coffee = serde_json::json!({
        "field": "coffee",
        "occurTime": {"utc": "2025-01-08T07:00:00.000Z", "o": 0},
    });
    let meta = serde_json::json!({"createTime": {"utc": "2025-01-08T07:00:00.500Z", "o": 0}});
    let backup = serde_json::json!({"backupTime": "2025-02-01T00:00:00Z", "docs": [
        {"_id": "coffee:a", "data": coffee, "meta": meta},
        {"_id": "coffee:copy", "data": coffee, "meta": meta},
        {"_id": "orphan", "data": {"occurTime": coffee["occurTime"]}, "meta": meta},
        {"_id": "_design/x", "views": {"by_field": {"map": "function (doc) {}"}}},
    ]})
    .to_string();
    let db = dir.path().join("v2.db");

    let out = datum(&db, &["db", "v2import", "-"], backup.as_bytes());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for want in [
        "1 imported (1 point, 0 block), 2 skipped (1 duplicate, 1 no-field).\n",
        "  duplicate      coffee:copy\n",
        "  no-field       orphan\n",
        "1 CouchDB design doc (_design/…) ignored: views, not entries.\n",
    ] {
        assert!(stdout.contains(want), "want {want:?} in:\n{stdout}");
    }
    assert!(!stdout.contains("_design/x"), "{stdout}");
    assert!(!stdout.contains("--skip-log"), "{stdout}");

    let out = datum(
        &db,
        &["db", "v2import", "--dry-run", "-"],
        backup.as_bytes(),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    for want in [
        "2 would import (2 point, 0 block), 1 skipped (1 no-field).\n",
        "  no-field       orphan\n",
        "1 CouchDB design doc (_design/…) ignored: views, not entries.\n",
    ] {
        assert!(stdout.contains(want), "want {want:?} in:\n{stdout}");
    }
}
