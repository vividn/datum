//! The premade examples (da-rky8h): each spec `setup --premade --json`
//! prints is one `setup --from-json` takes, and stores as printed.

use crate::common;

use common::run_cli;
use datum_commands::commands::{premade::load, spec::spec_json};
use datum_core::db::Db;
use datum_core::test_util::fresh_db;
use serde_json::Value;

/// `spec` saved as setup saves one, then read back as `setup F --json` prints it.
fn saved(db: &Db, spec: &Value) -> Value {
    let field = spec["field"].as_str().unwrap();
    let json = serde_json::to_string(spec).unwrap();
    run_cli(&["setup", field, "--from-json", &json, "--track"], db)
        .unwrap_or_else(|e| panic!("{field}: {e:#}"));
    serde_json::to_value(spec_json(&db.load_spec(field).unwrap().unwrap())).unwrap()
}

#[test]
fn every_example_sets_up_as_it_is_printed() {
    let premade = load().unwrap();
    // Every example in one database, as a person taking them all would.
    let db = fresh_db();
    for e in &premade.examples {
        assert_eq!(saved(&db, &e.spec), e.spec, "{}", e.field);
    }
    // Each choice and variant on a database of its own: the field is the same.
    for e in &premade.examples {
        let other = e.choices.values().map(|c| &c.spec);
        for spec in other.chain(e.variants.iter().map(|v| &v.spec)) {
            assert_eq!(saved(&fresh_db(), spec), *spec, "{}", e.field);
        }
    }
}
