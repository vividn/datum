//! The field-name rule against the shared fixture.
//!
//! The cases are not written here. They live in
//! `tests/fixtures/field-name.json` at the repo root, and
//! `web/tests/fieldname.test.mjs` reads the same file — because two
//! hand-written test files drift exactly the way two implementations do,
//! and this rule has two implementations by necessity: the parser's, which
//! enforces, and the browser form's, which explains before the user
//! submits (da-b049).

use datum_parse::field_name::{check, suggest, RULE};
use datum_parse::parse_argv;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    field: String,
    ok: bool,
    suggest: Option<String>,
}

fn fixture() -> Fixture {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/field-name.json"
    );
    let raw = std::fs::read_to_string(path).expect("shared field-name fixture");
    serde_json::from_str(&raw).expect("shared field-name fixture is JSON")
}

#[test]
fn judges_every_shared_case() {
    let fixture = fixture();
    assert!(!fixture.cases.is_empty());
    for case in &fixture.cases {
        assert_eq!(check(&case.field).is_ok(), case.ok, "{}", case.name);
        assert_eq!(suggest(&case.field), case.suggest, "{}", case.name);
    }
}

/// The rule is enforced where names are created, so `spec set` itself has
/// to refuse — and refuse by *saying the rule*. `-weird` used to come back
/// as "unexpected flag: -w", which is a third way of failing for a person
/// who typed a name, so the message is asserted and not just the error
/// (da-b049). This is the terminal's half; `web/tests/fieldname.test.mjs`
/// drives the browser's.
#[test]
fn spec_set_refuses_every_name_the_fixture_calls_bad() {
    for case in &fixture().cases {
        let argv = ["datum", "spec", "set", &case.field, "--verb", "occur"];
        let parsed = parse_argv(&argv.map(str::to_string));
        assert_eq!(parsed.is_ok(), case.ok, "{}: {parsed:?}", case.name);
        if let Err(e) = parsed {
            let message = e.to_string();
            assert!(message.contains(RULE), "{}: {message}", case.name);
        }
    }
}

/// A misspelled flag is still a misspelled flag. The leading-dash arm above
/// must not swallow `--defualt-verb` into a field-name complaint.
#[test]
fn a_mistyped_long_flag_keeps_its_own_message() {
    let argv = ["datum", "spec", "set", "--defualt-verb", "occur", "coffee"];
    let e = parse_argv(&argv.map(str::to_string))
        .unwrap_err()
        .to_string();
    assert!(e.contains("--defualt-verb"), "{e}");
    assert!(!e.contains(RULE), "{e}");
}

/// Every suggestion the fixture offers has to be a name the rule accepts,
/// or the (+) form would talk a user into a second refusal.
#[test]
fn every_suggestion_is_itself_acceptable() {
    for case in &fixture().cases {
        if let Some(fixed) = &case.suggest {
            assert!(check(fixed).is_ok(), "{}: '{fixed}'", case.name);
        }
    }
}

/// Unicode names work today and must keep working. A `[a-z0-9_]` rule
/// would pass every other test in this file while quietly making `café`
/// undeclarable, so say out loud that the fixture still covers them.
#[test]
fn the_fixture_still_covers_non_ascii_names_that_are_allowed() {
    let fixture = fixture();
    for name in ["café", "日本語", "emoji☕", "très-bien"] {
        assert!(
            fixture
                .cases
                .iter()
                .any(|case| case.field == name && case.ok),
            "no case declares {name} acceptable"
        );
    }
}
