//! A state declares its own keys, in the four kinds of da-9s98k, which
//! join the field's when an entry is in that state (da-cej7q). vividn,
//! 09-27: «some states might want an extra key or something where other's
//! don't and it would be good to autoalias and validate against that».

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::kit::KitToml;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::{json, Value};

/// `mood` declares `level`; its state `sad` declares a required `reason`
/// of its own, `happy` nothing.
fn mood_db() -> Db {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "mood",
            "--verb",
            "switch",
            "--state",
            "happy",
            "--state",
            "sad",
            "--data-key",
            "level",
            "--state-key",
            "sad=reason",
        ],
        &db,
    )
    .unwrap();
    db
}

/// The typed line, the field-first line and the state's auto-alias.
const HEADS: [&[&str]; 3] = [&["switch", "mood"], &["mood"], &[]];

fn logged(db: &Db) -> Value {
    logged_entries(db, "mood")[0].data.clone().unwrap()
}

#[test]
fn a_state_key_binds_after_the_fields() {
    for head in HEADS {
        let db = mood_db();
        run_cli(&[head, &["sad", "2", "because"]].concat(), &db).unwrap();
        assert_eq!(
            logged(&db),
            json!({"level": 2, "reason": "because"}),
            "{head:?}"
        );
    }
}

#[test]
fn a_required_state_key_refuses_in_that_state_only() {
    for head in HEADS {
        let db = mood_db();
        let err = run_cli(&[head, &["sad", "2"]].concat(), &db).unwrap_err();
        assert!(
            format!("{err:#}").contains("Missing required key: reason"),
            "{head:?}: {err:#}"
        );
        assert!(logged_entries(&db, "mood").is_empty());

        run_cli(&[head, &["happy", "2"]].concat(), &db).unwrap();
        assert_eq!(logged(&db), json!({"level": 2}), "{head:?}");
    }
}

/// A state key named as a field key overrides it for that state, in the
/// field key's place.
#[test]
fn a_state_key_overrides_a_field_key_by_name() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "mood",
            "--verb",
            "switch",
            "--state",
            "happy",
            "--state",
            "sad",
            "--data-key",
            "level=1",
            "--data-key",
            "note=",
            "--state-key",
            "sad=level",
        ],
        &db,
    )
    .unwrap();
    run_cli(&["happy", "-t", "-3"], &db).unwrap();
    assert_eq!(logged(&db), json!({"level": 1}));
    let err = run_cli(&["sad", "-t", "-2"], &db).unwrap_err();
    assert!(format!("{err:#}").contains("Missing required key: level"));
    run_cli(&["sad", "3", "low"], &db).unwrap();
    let data: Vec<_> = logged_entries(&db, "mood")
        .into_iter()
        .map(|e| e.data)
        .collect();
    assert!(
        data.contains(&Some(json!({"level": 3, "note": "low"}))),
        "{data:?}"
    );
}

/// An end, `false` and the other inactive states take none of the state's
/// keys — nor does a switch to another state.
#[test]
fn an_end_ignores_the_states_keys() {
    let db = mood_db();
    run_cli(&["sad", "2", "rain", "-t", "-10"], &db).unwrap();
    for (i, line) in [
        &["end", "mood"][..],
        &["moode"],
        &["switch", "mood", "false"],
        &["switch", "mood", "IGNORED"],
    ]
    .into_iter()
    .enumerate()
    {
        let t = format!("-{}", 8 - i);
        run_cli(&[line, &["-t", &t]].concat(), &db).unwrap_or_else(|e| panic!("{line:?}: {e:#}"));
    }
}

/// `spec set --state-key` and `spec add-state --key` write a state's keys,
/// the stored spec carries them, `--from-json` reads them back, a kit
/// exports and installs them, and `S=` clears.
#[test]
fn state_keys_round_trip() {
    let db = mood_db();
    run_cli(
        &[
            "spec",
            "add-state",
            "mood",
            "calm",
            "--key",
            "why=",
            "--assign",
            "tea=green",
        ],
        &db,
    )
    .unwrap();
    let spec = db.load_spec("mood").unwrap().unwrap();
    let want = json!({"sad": ["reason"], "calm": ["why="]});
    assert_eq!(serde_json::to_value(&spec).unwrap()["state_keys"], want);
    let assign = json!({"calm": {"tea": "green"}});
    assert_eq!(serde_json::to_value(&spec).unwrap()["state_assign"], assign);
    run_cli(&["calm", "2"], &db).unwrap();
    assert_eq!(logged(&db), json!({"level": 2, "tea": "green"}));

    let shown = serde_json::to_string(&spec).unwrap();
    let from_json = fresh_db();
    run_cli(&["spec", "set", "mood", "--from-json", &shown], &from_json).unwrap();
    let back = from_json.load_spec("mood").unwrap().unwrap();
    assert_eq!(back.state_keys, spec.state_keys);
    assert_eq!(back.state_assign, spec.state_assign);

    let dir = tempfile::tempdir().unwrap();
    let kit = dir.path().join("k.toml");
    let toml = KitToml::from_field_specs("k", None, std::slice::from_ref(&spec), &[])
        .to_toml()
        .unwrap();
    std::fs::write(&kit, toml).unwrap();
    let installed = fresh_db();
    run_cli(&["kit", "install", &kit.to_string_lossy()], &installed).unwrap();
    let out = dir.path().join("out.toml");
    run_cli(&["kit", "export", "k", &out.to_string_lossy()], &installed).unwrap();
    let again = fresh_db();
    run_cli(&["kit", "install", &out.to_string_lossy()], &again).unwrap();
    let again = again.load_spec("mood").unwrap().unwrap();
    assert_eq!(again.state_keys, spec.state_keys);
    assert_eq!(again.state_assign, spec.state_assign);

    // An empty flag clears the keys before it, in order.
    let keys = |db: &Db| {
        serde_json::to_value(db.load_spec("mood").unwrap().unwrap()).unwrap()["state_keys"].clone()
    };
    run_cli(
        &[
            "spec",
            "set",
            "mood",
            "--state-key",
            "sad=",
            "--state-key",
            "sad=a",
        ],
        &db,
    )
    .unwrap();
    assert_eq!(keys(&db)["sad"], json!(["a"]));
    run_cli(
        &["spec", "add-state", "mood", "calm", "--key", "b", "--key="],
        &db,
    )
    .unwrap();
    assert_eq!(keys(&db).get("calm"), None);
    run_cli(&["spec", "add-state", "mood", "calm", "--key", "why="], &db).unwrap();
    run_cli(&["spec", "add-state", "mood", "calm", "--key="], &db).unwrap();
    assert_eq!(keys(&db).get("calm"), None);
    run_cli(
        &[
            "spec",
            "add-state",
            "mood",
            "calm",
            "--key",
            "why=",
            "--assign",
            "tea=green",
        ],
        &db,
    )
    .unwrap();
    run_cli(
        &[
            "spec",
            "set",
            "mood",
            "--state-key",
            "sad=reason",
            "--state-key",
            "sad=",
        ],
        &db,
    )
    .unwrap();
    let cleared = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(&cleared).unwrap()["state_keys"],
        json!({"calm": ["why="]})
    );
    // `--state-assign S=K=V` merges into the state's map, `S=K=` removes.
    run_cli(&["spec", "set", "mood", "--state-assign", "calm=tea="], &db).unwrap();
    assert!(db
        .load_spec("mood")
        .unwrap()
        .unwrap()
        .state_assign
        .is_empty());
    run_cli(
        &["spec", "set", "mood", "--state-assign", "sad=tea=black"],
        &db,
    )
    .unwrap();
    run_cli(&["sad", "-t", "-1", "why"], &db).unwrap();
    let teas: Vec<Value> = logged_entries(&db, "mood")
        .into_iter()
        .filter_map(|e| e.data?.get("tea").cloned())
        .collect();
    assert_eq!(teas, [json!("green"), json!("black")]);
}

/// `spec show --text` lists a state's keys under it, and those of a state
/// the `states` list does not name under their own heading.
#[test]
fn text_view_lists_state_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", tmp.path().join("t.db"))
            .env("HOME", tmp.path())
            .env("DATUM_HOME", tmp.path())
            .env("DATUM_CONFIG_HOME", tmp.path())
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    run(&[
        "spec",
        "set",
        "mood",
        "--verb",
        "switch",
        "--state",
        "sad",
        "--state-key",
        "sad=reason",
        "--state-key",
        "gone=why=",
    ]);
    let text = run(&["spec", "show", "mood", "--text"]);
    assert!(
        text.contains("    sad              (auto)\n      reason         required\n"),
        "{text}"
    );
    assert!(
        text.contains("  state keys:\n    gone\n      why            optional\n"),
        "{text}"
    );
}

/// `reparse` replays the state's keys as the line bound them, not the spec
/// as it stands now (da-zv2jr).
#[test]
fn reparse_replays_the_state_keys_the_line_bound() {
    for head in HEADS {
        let db = mood_db();
        run_cli(&[head, &["sad", "2", "because"]].concat(), &db).unwrap();
        let logged = logged_entries(&db, "mood")[0].clone();
        run_cli(&["spec", "set", "mood", "--state-key", "sad=cause"], &db).unwrap();
        run_cli(&["reparse", &logged.hid.0, "--fix"], &db).unwrap();
        let after = db.get_entry(&logged.hid.0).unwrap();
        assert_eq!(after.data, logged.data, "{head:?}");
        assert_eq!(after.parser_version, logged.parser_version, "{head:?}");
    }
}

/// A state alias is no field, so a subcommand's name after it is a value
/// for its slots, not the field-first form: `sad why -t -1` read as
/// `why sad -t -1` and was refused with «unexpected flag: -t» (da-y40jf).
#[test]
fn a_subcommand_word_after_a_state_alias_is_a_value() {
    for word in ["why", "tail", "check"] {
        for head in HEADS {
            let db = fresh_db();
            run_cli(
                &[
                    "spec",
                    "set",
                    "mood",
                    "--verb",
                    "switch",
                    "--state",
                    "sad",
                    "--state-key",
                    "sad=reason",
                ],
                &db,
            )
            .unwrap();
            run_cli(&[head, &["sad", word, "-t", "-1"]].concat(), &db).unwrap();
            assert_eq!(logged(&db), json!({"reason": word}), "{head:?} {word}");
        }
    }
}
