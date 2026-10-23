//! `pre_scan_db_flag` against the shared `--db` fixture.
//!
//! The cases live in `tests/fixtures/db-flag.json` at the repo root, and
//! `web/tests/dbflag-pure.test.mjs` reads the same file against datum.day's
//! scanner (`web/src/lib/dbFlag.ts`), so the browser cannot pick another
//! database than the CLI does for the same argv (da-scrml).

use datum_commands::config::pre_scan_db_flag;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    argv: Vec<String>,
    db: Option<String>,
}

#[test]
fn scans_every_shared_case() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/db-flag.json"
    );
    let raw = std::fs::read_to_string(path).expect("shared db-flag fixture");
    let fixture: Fixture = serde_json::from_str(&raw).expect("shared db-flag fixture is JSON");
    assert!(!fixture.cases.is_empty());
    for case in fixture.cases {
        let argv: Vec<String> = std::iter::once("datum".to_string())
            .chain(case.argv)
            .collect();
        assert_eq!(pre_scan_db_flag(&argv), case.db, "{}", case.name);
    }
}
