//! A spec's declared keys are the field's `-k`/`-K` slots, in four kinds —
//! required, optional, default, assign — so logging a field behaves as an
//! alias with those slots would (da-9s98k).
//!
//! The rows each test reproduces were measured on main a32b82c7 with the
//! alias `jr` = `occur juice -k cups -k size= -k temp=cold brand=acme`.

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::field_spec::{DeclaredKey, KeyKind};
use datum_core::kit::KitToml;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::{json, Value};

/// `juice` declares the alias's four keys, one of each kind.
fn juice_db() -> Db {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "juice",
            "--verb",
            "occur",
            "--data-key",
            "cups",
            "--data-key",
            "size=",
            "--data-key",
            "temp=cold",
            "--data-key",
            "brand:=acme",
        ],
        &db,
    )
    .unwrap();
    db
}

/// The data of the entry `line` writes, through the typed line
/// (`occur juice …`) and through the field's own alias (`juice …`).
fn both_ways(line: &[&str]) -> [Value; 2] {
    [&["occur", "juice"][..], &["juice"][..]].map(|head| {
        let db = juice_db();
        run_cli(&[head, line].concat(), &db).unwrap();
        logged_entries(&db, "juice")[0].data.clone().unwrap()
    })
}

#[test]
fn required_key_refuses_a_bare_log() {
    for head in [&["occur", "juice"][..], &["juice"][..]] {
        let db = juice_db();
        let err = run_cli(head, &db).unwrap_err();
        assert!(
            format!("{err:#}").contains("Missing required key: cups"),
            "{err:#}"
        );
        assert!(logged_entries(&db, "juice").is_empty());
    }
}

#[test]
fn default_and_assign_fill_and_optional_stays_absent() {
    for data in both_ways(&["2"]) {
        assert_eq!(data, json!({"cups": 2, "temp": "cold", "brand": "acme"}));
    }
}

#[test]
fn positionals_bind_in_declared_order_over_a_default() {
    for data in both_ways(&["2", "large", "warm"]) {
        assert_eq!(
            data,
            json!({"cups": 2, "size": "large", "temp": "warm", "brand": "acme"})
        );
    }
}

#[test]
fn a_key_value_at_the_call_site_overrides_an_assigned_one() {
    for data in both_ways(&["2", "brand=other"]) {
        assert_eq!(data["brand"], json!("other"));
    }
}

/// The alias declares `cups` too: one slot, and the alias's form (a
/// default of 5, where the spec's is required) wins.
#[test]
fn an_aliases_own_form_of_a_key_wins() {
    let db = juice_db();
    run_cli(&["alias", "set", "jx", "occur juice -k cups=5"], &db).unwrap();
    run_cli(&["jx"], &db).unwrap();
    run_cli(&["jx", "3", "large", "-t", "-5"], &db).unwrap();
    let data: Vec<Value> = logged_entries(&db, "juice")
        .into_iter()
        .map(|e| e.data.unwrap())
        .collect();
    assert!(data.contains(&json!({"cups": 5, "temp": "cold", "brand": "acme"})));
    assert!(data.contains(&json!({"cups": 3, "size": "large", "temp": "cold", "brand": "acme"})));
}

/// An id key's value goes into the slug, an assigned one's too.
#[test]
fn id_keys_are_slug_slots() {
    let db = fresh_db();
    run_cli(
        &[
            "spec", "set", "word", "--verb", "occur", "--id-key", "w", "--id-key", "lang:=pt",
        ],
        &db,
    )
    .unwrap();
    run_cli(&["word", "correr"], &db).unwrap();
    assert_eq!(logged_entries(&db, "word")[0].slug, "word:correr__pt");
}

/// An add field's keys bind too (vividn, 09-28: «add is the primary use
/// case for spec based fields»); an add needs an id, so its title is one.
#[test]
fn an_add_binds_its_keys() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "book",
            "--verb",
            "add",
            "--id-key",
            "title",
            "--data-key",
            "author=",
            "--data-key",
            "shelf:=home",
        ],
        &db,
    )
    .unwrap();
    run_cli(&["book", "Dune", "Herbert"], &db).unwrap();
    run_cli(&["add", "book", "Emma"], &db).unwrap();
    let data: Vec<Value> = logged_entries(&db, "book")
        .into_iter()
        .map(|e| e.data.unwrap())
        .collect();
    assert!(data.contains(&json!({"title": "Dune", "author": "Herbert", "shelf": "home"})));
    assert!(data.contains(&json!({"title": "Emma", "shelf": "home"})));
}

/// A multi-side alias gives each side its field's keys, and the call site's
/// positionals walk them side by side; a key two sides declare is one slot
/// (docs/trailing-and-slots.md rule 6; vividn, 09-28).
fn morning_db() -> Db {
    let db = fresh_db();
    for line in [
        &[
            "spec",
            "set",
            "coffee",
            "--verb",
            "occur",
            "--data-key",
            "cups",
        ][..],
        &[
            "spec",
            "set",
            "work",
            "--verb",
            "start",
            "--data-key",
            "project=",
        ],
        &[
            "spec",
            "set",
            "bike",
            "--verb",
            "start",
            "--data-key",
            "project=",
        ],
        &["alias", "set", "morning", "occur coffee , start work"],
    ] {
        run_cli(line, &db).unwrap();
    }
    db
}

#[test]
fn a_multi_side_alias_binds_each_sides_keys() {
    let db = morning_db();
    run_cli(&["morning", "2", "acme", "2"], &db).unwrap();
    assert_eq!(
        logged_entries(&db, "coffee")[0].data,
        Some(json!({"cups": 2}))
    );
    let work = &logged_entries(&db, "work")[0];
    assert_eq!(work.data, Some(json!({"project": "acme"})));
    assert_eq!(work.duration_seconds(), Some(2 * 60));

    let db = morning_db();
    run_cli(&["morning", "2"], &db).unwrap();
    assert_eq!(
        logged_entries(&db, "coffee")[0].data,
        Some(json!({"cups": 2}))
    );
    assert_eq!(logged_entries(&db, "work")[0].data, None);
}

#[test]
fn a_key_two_sides_declare_is_one_slot() {
    let db = morning_db();
    run_cli(&["alias", "set", "both", "start work , start bike"], &db).unwrap();
    run_cli(&["both", "acme", "30m"], &db).unwrap();
    for field in ["work", "bike"] {
        let e = &logged_entries(&db, field)[0];
        assert_eq!(e.data, Some(json!({"project": "acme"})), "{field}");
        assert_eq!(e.duration_seconds(), Some(30 * 60), "{field}");
    }
}

/// An alias's own `-k` on one side and a spec key on another bind in side
/// order: the first side's slots first.
#[test]
fn alias_and_spec_slots_walk_in_side_order() {
    let db = morning_db();
    run_cli(
        &["alias", "set", "mix", "occur coffee -k size , start work"],
        &db,
    )
    .unwrap();
    run_cli(&["mix", "big", "2", "acme"], &db).unwrap();
    let coffee = logged_entries(&db, "coffee")[0].data.clone();
    assert_eq!(coffee, Some(json!({"size": "big", "cups": 2})));
    assert_eq!(
        logged_entries(&db, "work")[0].data,
        Some(json!({"project": "acme"}))
    );
}

/// `spec set --data-key` and `track --key` write different lists; a name in
/// both is one key — the first place, the last kind — as repeated `-k` are.
#[test]
fn a_key_declared_twice_is_one_key() {
    let db = juice_db();
    run_cli(&["track", "juice", "--key", "cups=5"], &db).unwrap();
    let spec = db.load_spec("juice").unwrap().unwrap();
    let cups: Vec<_> = spec
        .declared_keys(None)
        .into_iter()
        .filter(|k| k.name == "cups")
        .collect();
    assert_eq!(cups.len(), 1);
    assert_eq!(cups[0].kind, KeyKind::Default("5".into()));
    run_cli(&["juice"], &db).unwrap();
    assert_eq!(
        logged_entries(&db, "juice")[0].data.as_ref().unwrap()["cups"],
        json!(5)
    );
}

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
            "--data-key",
            "level",
        ],
        &db,
    )
    .unwrap();
    db
}

#[test]
fn a_switch_binds_after_its_state() {
    for line in [
        &["switch", "mood", "happy", "2"][..],
        &["mood", "happy", "2"],
        &["happy", "2"],
    ] {
        let db = mood_db();
        run_cli(line, &db).unwrap();
        let e = &logged_entries(&db, "mood")[0];
        assert_eq!(e.state, Some(json!("happy")), "{line:?}");
        assert_eq!(e.data, Some(json!({"level": 2})), "{line:?}");
    }
}

/// Keys apply to active states only (vividn, 09-27): an end, `false`, `null`,
/// IGNORED and the track marker log without mood's required `level`.
#[test]
fn inactive_entries_ignore_the_specs_keys() {
    let db = mood_db();
    for (i, line) in [
        &["end", "mood"][..],
        &["stop", "mood"],
        &["moode"],
        &["switch", "mood", "false"],
        &["switch", "mood", "null"],
        &["switch", "mood", "IGNORED"],
        &["switch", "mood", "TRACKED"],
        &["ignore", "mood"],
        &["track", "mood"],
    ]
    .into_iter()
    .enumerate()
    {
        let t = format!("-{}", 20 - i);
        let timed = [line, &["-t", &t]].concat();
        let line = if line[0] == "ignore" || line[0] == "track" {
            // Both write at now, with no `-t`: in the same millisecond
            // they share a slug and the second is a duplicate.
            std::thread::sleep(std::time::Duration::from_millis(2));
            line
        } else {
            &timed
        };
        run_cli(line, &db).unwrap_or_else(|e| panic!("{line:?}: {e:#}"));
    }
}

/// Every route keeps the kind: `spec set` and `track --key` write it, the
/// stored spec (what `spec show` prints) carries it, `--from-json` reads it
/// back, and a kit exports and installs it.
#[test]
fn the_kind_round_trips() {
    let db = juice_db();
    run_cli(&["spec", "set", "juice", "--id-key", "lang:=pt"], &db).unwrap();
    run_cli(
        &[
            "track",
            "juice",
            "--key",
            "origin:=farm",
            "--required-key",
            "n",
        ],
        &db,
    )
    .unwrap();
    let spec = db.load_spec("juice").unwrap().unwrap();
    let key = |name: &str, kind, id| DeclaredKey {
        name: name.into(),
        kind,
        id,
    };
    let want = vec![
        key("lang", KeyKind::Assign("pt".into()), true),
        key("cups", KeyKind::Required, false),
        key("size", KeyKind::Optional, false),
        key("temp", KeyKind::Default("cold".into()), false),
        key("brand", KeyKind::Assign("acme".into()), false),
        key("n", KeyKind::Required, false),
        key("origin", KeyKind::Assign("farm".into()), false),
    ];
    assert_eq!(spec.declared_keys(None), want);

    let shown = serde_json::to_string(&spec).unwrap();
    let from_json = fresh_db();
    run_cli(&["spec", "set", "juice", "--from-json", &shown], &from_json).unwrap();
    assert_eq!(
        from_json
            .load_spec("juice")
            .unwrap()
            .unwrap()
            .declared_keys(None),
        want
    );

    let dir = tempfile::tempdir().unwrap();
    let kit = dir.path().join("k.toml");
    let toml = KitToml::from_field_specs("k", None, &[spec], &[])
        .to_toml()
        .unwrap();
    std::fs::write(&kit, toml).unwrap();
    let installed = fresh_db();
    run_cli(&["kit", "install", &kit.to_string_lossy()], &installed).unwrap();
    let out = dir.path().join("out.toml");
    run_cli(&["kit", "export", "k", &out.to_string_lossy()], &installed).unwrap();
    let again = fresh_db();
    run_cli(&["kit", "install", &out.to_string_lossy()], &again).unwrap();
    assert_eq!(
        again
            .load_spec("juice")
            .unwrap()
            .unwrap()
            .declared_keys(None),
        want
    );
}
