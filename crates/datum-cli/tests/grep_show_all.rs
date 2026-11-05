//! `datum grep -A` shows each hit's whole document, as `datum get -A` does,
//! where the default prints a table (da-j8m7s).

use std::path::Path;
use std::process::Command;

fn ok(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .env("TZ", "UTC")
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn grep_show_all_prints_each_hit_whole() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    ok(&db, &["moment", "coffee", "kind=strong"]);
    ok(&db, &["moment", "coffee", "kind=weak"]);

    let table = ok(&db, &["grep", "-f", "coffee", "strong"]);
    let whole = ok(&db, &["grep", "-A", "-f", "coffee", "strong"]);

    for section in ["── Identity", "── Bookkeeping", "content_hash"] {
        assert!(!table.contains(section), "{table}");
        assert!(whole.contains(section), "{whole}");
    }
    assert!(
        whole.contains("strong") && !whole.contains("weak"),
        "{whole}"
    );
    assert_eq!(whole.matches("── Identity").count(), 1, "{whole}");
}
