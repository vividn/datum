//! `shell_split` against the shared tokenizer fixture.
//!
//! The cases are not written here. They live in
//! `tests/fixtures/shell-split.json` at the repo root, and
//! `web/tests/shellsplit.test.mjs` reads the same file — because two
//! hand-written test files drift exactly the way two implementations do,
//! which is what let U+0085 and U+FEFF tokenize differently in the terminal
//! and the browser for as long as they did (da-rx63). A fixture is the one
//! thing that cannot disagree with itself.

use datum_parse::shell_split;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    line: String,
    argv: Vec<String>,
}

fn fixture() -> Fixture {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/shell-split.json"
    );
    let raw = std::fs::read_to_string(path).expect("shared shell-split fixture");
    serde_json::from_str(&raw).expect("shared shell-split fixture is JSON")
}

#[test]
fn splits_every_shared_case() {
    let fixture = fixture();
    assert!(!fixture.cases.is_empty());
    for case in &fixture.cases {
        assert_eq!(shell_split(&case.line), case.argv, "{}", case.name);
    }
}

/// The two codepoints that actually diverged. A fixture can be edited; this
/// says out loud that dropping them from it is the regression, not a tidy-up.
#[test]
fn the_fixture_still_covers_the_two_divergent_codepoints() {
    let fixture = fixture();
    for c in ['\u{0085}', '\u{FEFF}'] {
        assert!(
            fixture.cases.iter().any(|case| case.line.contains(c)),
            "no case exercises U+{:04X}",
            c as u32
        );
    }
}
