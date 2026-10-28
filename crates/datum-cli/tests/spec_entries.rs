//! Field specs are entries (da-lc26c): a field's spec is the entry
//! `spec:<field>` of field `spec`, so it backs up, syncs and edits like any
//! doc, while staying off the life-data surfaces.
//!
//! vividn, 2026-09-25: "specs, aliases, views should all very much be part of
//! the db they are describing. Ideally in the same data format as everything
//! else, so they sync and can even be edited like normal datum docs."
//!
//! The real binary: what is asserted is what each command PRINTS.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value as Json;
use tempfile::TempDir;

struct Env {
    tmp: TempDir,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        Self {
            db: tmp.path().join("data.db"),
            tmp,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn err(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            !out.status.success(),
            "`datum {}` succeeded",
            args.join(" ")
        );
        String::from_utf8(out.stderr).unwrap()
    }

    fn spec(&self, field: &str) -> Json {
        serde_json::from_str(&self.ok(&["setup", field, "--json"])).unwrap()
    }

    fn kit(&self, name: &str, fields: &[&str]) {
        let mut toml = format!("[kit]\nname = \"{name}\"\n");
        for f in fields {
            toml += &format!("\n[[field]]\nname = \"{f}\"\nverb = \"occur\"\ncolor = \"green\"\n");
        }
        let path = self.tmp.path().join(format!("{name}.toml"));
        std::fs::write(&path, toml).unwrap();
        self.ok(&["kit", "install", path.to_str().unwrap()]);
    }
}

/// A spec edit is an entry edit: `undo` steps it back and `redo` forward,
/// `history spec:mood` lists the versions, and `update spec:mood` edits it
/// as a doc.
#[test]
fn a_spec_edits_and_undoes_like_a_doc() {
    let env = Env::new();
    env.ok(&["switch", "mood", "happy"]);
    env.ok(&["setup", "mood", "--color", "#0000ff"]);
    env.ok(&["setup", "mood", "--color", "#ff0000"]);

    env.ok(&["undo"]);
    assert_eq!(env.spec("mood")["color"], "#0000ff");
    env.ok(&["redo"]);
    assert_eq!(env.spec("mood")["color"], "#ff0000");

    env.ok(&["update", "spec:mood", "color=green"]);
    let green = datum_core::kit_color::resolve("green", "mood").unwrap();
    assert_eq!(env.spec("mood")["color"], green.as_str());
    let history = env.ok(&["history", "spec:mood"]);
    assert!(
        history.contains("undo") && history.contains("redo"),
        "{history}"
    );
}

/// A write is validated as a spec: a malformed one is refused with the
/// reason and never stored, and a plain log of the reserved field too.
#[test]
fn a_malformed_spec_is_refused_and_never_stored() {
    let env = Env::new();
    env.ok(&["switch", "mood", "happy"]);
    let err = env.err(&["update", "spec:mood", "verb=pirouette"]);
    assert!(err.contains("not a valid field spec"), "{err}");
    assert_eq!(env.spec("mood")["verb"], "switch");

    let err = env.err(&["occur", "spec"]);
    assert!(err.contains("reserved for field specs"), "{err}");
}

/// An anchor triple is saved whole or not at all (da-gm80m): a half one
/// through `--from-json`, datum.day's write, is refused naming the field and
/// the keys set, and stores nothing; a whole one and an empty one save.
#[test]
fn a_half_anchor_triple_is_refused_and_a_whole_or_empty_one_saves() {
    let env = Env::new();
    env.ok(&["occur", "body"]);
    let spec = |json: &str| format!(r#"{{"field":"contacts","verb":"occur"{json}}}"#);
    let half = spec(r#","relative_to":"body","anchor_pos_ratio":0.1"#);
    let err = env.err(&["setup", "contacts", "--from-json", &half]);
    assert!(
        err.contains("field 'contacts': anchor triple inconsistent")
            && err.contains("anchor_size_ratio=false"),
        "{err}"
    );
    assert!(!env.ok(&["tail", "spec"]).contains("contacts"));

    let whole = spec(r#","relative_to":"body","anchor_pos_ratio":0.1,"anchor_size_ratio":0.5"#);
    env.ok(&["setup", "contacts", "--from-json", &whole]);
    assert_eq!(env.spec("contacts")["anchor_size_ratio"], 0.5);
    env.err(&["setup", "contacts", "--from-json", &half]);
    assert_eq!(env.spec("contacts")["anchor_size_ratio"], 0.5);

    env.ok(&["setup", "contacts", "--from-json", &spec("")]);
    assert!(env.spec("contacts").get("relative_to").is_none());
}

/// Specs and layouts are not life data: they stay off tail, now, stats,
/// fields and count, and come back when named or with `-A`.
#[test]
fn meta_entries_stay_off_the_life_data_surfaces() {
    let env = Env::new();
    env.ok(&["switch", "mood", "happy"]);
    env.ok(&["add", "layout", "--id", "%name%", "name=main", "root=1"]);
    // A spec for a meta field, as an older datum minted on a layout's first
    // log: still not a field to list.
    env.ok(&["setup", "layout", "--verb", "occur"]);
    let hidden = |out: String| {
        assert!(!out.contains("spec") && !out.contains("layout"), "{out}");
        out
    };

    assert!(hidden(env.ok(&["tail"])).contains("happy"));
    hidden(env.ok(&["now"]));
    hidden(env.ok(&["stats"]));
    hidden(env.ok(&["fields"]));
    assert_eq!(hidden(env.ok(&["count"])).trim(), "1");

    assert!(env.ok(&["tail", "spec"]).contains("spec"));
    let all = env.ok(&["tail", "-A"]);
    assert!(all.contains("spec") && all.contains("layout"), "{all}");
}

/// A kit's specs are its: install writes them marked with the kit, list and
/// export count and read them, and uninstall takes only its own.
#[test]
fn kit_install_and_uninstall_touch_only_the_kits_specs() {
    let env = Env::new();
    env.ok(&["setup", "sleep", "--verb", "start"]);
    env.kit("drinks", &["coffee", "tea"]);
    env.kit("water", &["water"]);

    let listed = env.ok(&["kit", "list"]);
    assert!(listed.contains("drinks"), "{listed}");
    let exported = env.ok(&["kit", "export", "drinks"]);
    assert!(
        exported.contains("coffee") && exported.contains("tea"),
        "{exported}"
    );
    assert!(!exported.contains("water"), "{exported}");
    assert!(
        env.spec("coffee")["color"].is_string(),
        "the kit wrote its colour"
    );

    env.ok(&["kit", "uninstall", "drinks"]);
    for gone in ["coffee", "tea"] {
        assert!(
            env.ok(&["setup", gone, "--json"]).contains("No spec"),
            "{gone}"
        );
    }
    assert!(
        env.spec("water")["color"].is_string(),
        "another kit's spec stays"
    );
    assert_eq!(env.spec("sleep")["verb"], "start");
}
