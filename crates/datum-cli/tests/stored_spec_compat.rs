//! da-c8mh's compatibility half: a database whose specs were
//! written BEFORE this bead is still a database this binary reads.
//!
//! The break this guards against was measured, not imagined: with
//! `FieldSpec.verb` required and no alias, every command that loads a
//! spec answered `Serialization error: missing field `verb`` on a real
//! database — `switch`, `occur`, `now`, `fields`, `spec show` — and the
//! post-merge hook installs this binary over the one those databases
//! were written by. So the old spellings are read:
//!
//! - `"default_verb": "switch"` — the key the verb used to live under
//!   ([`FieldSpec::verb`]'s serde alias reads it);
//! - `"default_verb": null` — one of da-emze's verbless specs, which
//!   carries no answer at all and takes the verb its own log implies.
//!
//! (Only the STORED key is forgiven. The FLAG `--default-verb` is still
//! an error naming `--verb` — `track_cmd.rs` holds it to that.)
//!
//! The spec rows below are verbatim output of `datum spec set` from the
//! binary that preceded this bead.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

/// `spec set activity --default-verb switch --color blue`, as the
/// previous binary stored it.
const SPEC_WITH_OLD_KEY: &str = r#"{"field":"activity","default_verb":"switch","subfields":{},"description":null,"unit":null,"id_template":null,"color":"blue","allowed_states":null,"state_colors":{},"derived_aliases":[{"state":"activity","expands_to":"switch activity","kind":"bare_field"},{"state":"activitye","expands_to":"end activity","kind":"terminal_end"}],"id_keys":[],"data_keys":[],"no_autoalias":false,"strict":false}"#;

/// `spec set <field> --color red` — a spec that was never asked about
/// the verb (da-emze), stored with the key present and null.
fn verbless_spec(field: &str) -> String {
    format!(
        r#"{{"field":"{field}","default_verb":null,"subfields":{{}},"description":null,"unit":null,"id_template":null,"color":"red","allowed_states":null,"state_colors":{{}},"derived_aliases":[],"id_keys":[],"data_keys":[],"no_autoalias":false,"strict":false}}"#
    )
}

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    config_home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let db = tmp.path().join("stored_spec_compat.db");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        Self {
            _tmp: tmp,
            home,
            config_home,
            db,
        }
    }

    fn try_run(&self, args: &[&str]) -> (bool, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    fn run(&self, args: &[&str]) -> String {
        let (ok, text) = self.try_run(args);
        assert!(ok, "{args:?} failed:\n{text}");
        text
    }

    /// Replace `field`'s spec with one in the old on-disk shape, the way
    /// a database written by the previous binary holds it.
    fn store_old_spec(&self, field: &str, spec_json: &str) {
        let conn = rusqlite::Connection::open(&self.db).expect("open db");
        let changed = conn
            .execute(
                "UPDATE entries SET data = ?2 WHERE slug = 'spec:' || ?1",
                rusqlite::params![field, spec_json],
            )
            .expect("store the old spec shape");
        assert_eq!(changed, 1, "no spec row for {field}");
        // A spec is an entry (da-lc26c): keep its content hash honest, as
        // the binary that wrote the old shape would have.
        let db = datum_core::db::Db::open(self.db.to_str().unwrap()).unwrap();
        let slug = format!("spec:{field}");
        let spec = db.get_entry(&slug).unwrap();
        db.set_entry_content_hash(spec.hid.as_str(), &spec.expected_content_hash())
            .unwrap();
    }

    fn spec_verb(&self, field: &str) -> String {
        let shown = self.run(&["spec", "show", field]);
        let spec: Value = serde_json::from_str(&shown).expect("spec show prints json");
        assert!(
            spec["default_verb"].is_null(),
            "{field} is shown with the old key: {shown}"
        );
        spec["verb"].as_str().expect("a verb").to_string()
    }
}

/// The acceptance run: every command vremya measured erroring, on a
/// database holding both old spec shapes.
#[test]
fn a_database_written_before_this_bead_still_reads() {
    let env = Env::new();

    // The log first — these fields have a history, as the databases that
    // broke did.
    env.run(&["switch", "activity", "researching", "-t", "-30"]);
    env.run(&["switch", "mood", "happy", "-t", "-20"]);
    env.run(&["occur", "tea", "-t", "-10"]);

    // …and then their specs, in the shapes the previous binary wrote.
    env.store_old_spec("activity", SPEC_WITH_OLD_KEY);
    env.store_old_spec("mood", &verbless_spec("mood"));
    env.store_old_spec("tea", &verbless_spec("tea"));

    for args in [
        vec!["switch", "activity", "reading"],
        vec!["switch", "mood", "calm"],
        vec!["occur", "tea"],
        vec!["now"],
        vec!["fields"],
        vec!["fields", "--json"],
        vec!["spec", "show", "activity"],
        vec!["spec", "show", "tea"],
        vec!["spec", "list"],
        vec!["tail", "-n", "5"],
        vec!["check"],
    ] {
        let (ok, text) = env.try_run(&args);
        assert!(
            ok && !text.contains("missing field"),
            "{args:?} should read a database from before da-c8mh:\n{text}"
        );
    }

    // The old key IS the verb; a spec that never had one takes the verb
    // its own log implies — named states are a switch, a bare occur is an
    // occur — and the spec still shows everything else it carried.
    assert_eq!(env.spec_verb("activity"), "switch");
    assert_eq!(env.spec_verb("mood"), "switch");
    assert_eq!(env.spec_verb("tea"), "occur");

    let spec: Value = serde_json::from_str(&env.run(&["spec", "show", "activity"])).unwrap();
    assert_eq!(spec["color"], "blue");

    // The widget bar reads the same verb the CLI does (da-c8mh: the web
    // takes the shape from the spec, not from the log).
    let listing: Value = serde_json::from_str(&env.run(&["fields", "--json"])).unwrap();
    let verbs: Vec<(&str, &str)> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["field"].as_str().unwrap(),
                f["spec"]["verb"].as_str().unwrap_or("-"),
            )
        })
        .collect();
    assert_eq!(
        verbs,
        vec![("activity", "switch"), ("mood", "switch"), ("tea", "occur")],
    );
}

/// A spec that is broken for some OTHER reason is still an error — the
/// fallback reads a MISSING verb, it does not paper over a bad one.
#[test]
fn a_spec_with_an_unreadable_verb_is_still_an_error() {
    let env = Env::new();
    env.run(&["occur", "tea"]);
    env.store_old_spec("tea", r#"{"field":"tea","default_verb":"pirouette"}"#);

    let (ok, text) = env.try_run(&["spec", "show", "tea"]);
    assert!(!ok, "an unreadable verb should error, got:\n{text}");
}

/// da-6v7z1: datum has no units. A spec stored while it still did — a
/// field-level `unit` and a subfield's — loads with both ignored, shows
/// neither, and the next write stores it without them.
#[test]
fn a_stored_unit_is_ignored_and_dropped_on_the_next_write() {
    let env = Env::new();
    env.run(&["occur", "tea"]);
    env.store_old_spec(
        "tea",
        r#"{"field":"tea","verb":"occur","unit":"cups","subfields":{"dose":{"required":false,"value_type":"number","in_slug":false,"default":null,"unit":"mg"}}}"#,
    );

    for args in [
        vec!["spec", "show", "tea"],
        vec!["spec", "show", "tea", "--text"],
        vec!["fields", "--json"],
    ] {
        let text = env.run(&args);
        assert!(!text.contains("unit"), "{args:?} shows a unit:\n{text}");
    }
    let spec: Value = serde_json::from_str(&env.run(&["spec", "show", "tea"])).unwrap();
    assert_eq!(spec["subfields"]["dose"]["value_type"], "number");

    env.run(&["spec", "set", "tea", "--description", "green"]);
    let conn = rusqlite::Connection::open(&env.db).unwrap();
    let stored: String = conn
        .query_row(
            "SELECT data FROM entries WHERE slug = 'spec:tea'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(stored.contains("green"), "the write landed: {stored}");
    assert!(
        !stored.contains("unit"),
        "a unit survived the write: {stored}"
    );
}

/// da-eqb30: `strict` is now `only_listed_keys`. A spec stored as `strict`
/// reads as the new key — shown under it, still refusing an unlisted key
/// — and the next write stores the new name.
#[test]
fn a_stored_strict_reads_as_only_listed_keys() {
    let env = Env::new();
    env.run(&["occur", "tea", "cups=2"]);
    env.store_old_spec(
        "tea",
        r#"{"field":"tea","verb":"occur","data_keys":["cups"],"strict":true}"#,
    );

    let spec: Value = serde_json::from_str(&env.run(&["spec", "show", "tea"])).unwrap();
    assert_eq!(spec["only_listed_keys"], true, "{spec}");
    assert!(
        spec.get("strict").is_none(),
        "shown with the old key: {spec}"
    );
    env.run(&["occur", "tea", "3"]);
    let (ok, text) = env.try_run(&["occur", "tea", "3", "brand=x"]);
    assert!(
        !ok && text.contains("'brand'"),
        "an unlisted key was taken:\n{text}"
    );

    env.run(&["spec", "set", "tea", "--description", "green"]);
    let conn = rusqlite::Connection::open(&env.db).unwrap();
    let stored: String = conn
        .query_row(
            "SELECT data FROM entries WHERE slug = 'spec:tea'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        stored.contains(r#""only_listed_keys":true"#) && !stored.contains("strict"),
        "the write kept the old name: {stored}"
    );
}
