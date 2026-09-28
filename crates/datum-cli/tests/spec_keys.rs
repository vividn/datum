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
use datum_core::test_util::{advance_time, fresh_db, logged_entries, set_time};
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
            "--assign",
            "brand=acme",
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
            "spec",
            "set",
            "word",
            "--verb",
            "occur",
            "--id-key",
            "w",
            "--id-assign",
            "lang=pt",
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
            "--assign",
            "shelf=home",
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
    // Each line logs at now (`ignore` and `track` take no -t): step the
    // clock between them so no two rows share a millisecond's slug.
    let _clock = set_time("2026-01-15T12:00:00Z");
    let db = mood_db();
    for line in [
        &["end", "mood"][..],
        &["stop", "mood"],
        &["moode"],
        &["switch", "mood", "false"],
        &["switch", "mood", "null"],
        &["switch", "mood", "IGNORED"],
        &["switch", "mood", "TRACKED"],
        &["ignore", "mood"],
        &["track", "mood"],
    ] {
        advance_time("1s");
        run_cli(line, &db).unwrap_or_else(|e| panic!("{line:?}: {e:#}"));
    }
}

/// Every route keeps the kind: `spec set` and `track` write it, the
/// stored spec (what `spec show` prints) carries it, `--from-json` reads it
/// back, and a kit exports and installs it.
#[test]
fn the_kind_round_trips() {
    let db = juice_db();
    run_cli(&["spec", "set", "juice", "--id-assign", "lang=pt"], &db).unwrap();
    run_cli(
        &["track", "juice", "origin=farm", "--required-key", "n"],
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
        key("cups", KeyKind::Required, false),
        key("size", KeyKind::Optional, false),
        key("temp", KeyKind::Default("cold".into()), false),
        key("n", KeyKind::Required, false),
        key("brand", KeyKind::Assign("acme".into()), false),
        key("origin", KeyKind::Assign("farm".into()), false),
        key("lang", KeyKind::Assign("pt".into()), true),
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

/// `track`'s alias-style tail (da-ulc0v): `-k` declares a slot, a bare
/// `KEY=VALUE` an assign. The control is the same field tracked without the
/// tail, which logs neither.
#[test]
fn track_takes_an_alias_style_tail() {
    // Every `track` writes a row at now: step the clock between commands so
    // no two rows share a millisecond's slug.
    let _clock = set_time("2026-01-15T12:00:00Z");
    let run = |args: &[&str], db: &Db| {
        advance_time("1s");
        run_cli(args, db).unwrap_or_else(|e| panic!("{args:?}: {e:#}"));
    };
    let db = fresh_db();
    run(&["track", "plain", "--verb", "occur"], &db);
    run(
        &[
            "track",
            "vacuum",
            "--verb",
            "occur",
            "-k",
            "next_date=+4",
            "proj=household",
        ],
        &db,
    );
    run(&["occur", "vacuum", "-d", "2026-03-01"], &db);
    run(&["occur", "plain"], &db);
    let spec = db.load_spec("vacuum").unwrap().unwrap();
    assert_eq!(spec.data_keys, ["next_date=+4"]);
    assert_eq!(spec.assign["proj"], "household");
    let data = logged_entries(&db, "vacuum")[0].data.clone().unwrap();
    assert_eq!(data["proj"], json!("household"));
    assert!(
        data["next_date"].as_str().is_some_and(|d| d.len() == 10),
        "{data}"
    );
    assert_eq!(logged_entries(&db, "plain")[0].data, None);

    // `-K` is an id slot, and a slot named again takes its new kind in place.
    run(&["track", "vacuum", "-K", "room", "-k", "next_date="], &db);
    let spec = db.load_spec("vacuum").unwrap().unwrap();
    assert_eq!(spec.id_keys, ["room"]);
    assert_eq!(spec.data_keys, ["next_date="]);
}

/// `spec set --assign` round-trips through `spec show` (text and JSON), a
/// kit's export and install, and `--assign KEY=` removes it. The control:
/// `brand:=acme` is no spelling of an assign any more — `:=` is gone.
#[test]
fn assign_round_trips_through_show_and_kits() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "f",
            "--verb",
            "occur",
            "--assign",
            "brand=acme",
        ],
        &db,
    )
    .unwrap();
    let spec = db.load_spec("f").unwrap().unwrap();
    let shown = serde_json::to_value(&spec).unwrap();
    assert_eq!(shown["assign"], json!({"brand": "acme"}));
    assert_eq!(shown["data_keys"], json!([]));

    let dir = tempfile::tempdir().unwrap();
    let kit = dir.path().join("k.toml");
    let toml = KitToml::from_field_specs("k", None, std::slice::from_ref(&spec), &[])
        .to_toml()
        .unwrap();
    assert!(toml.contains("brand = \"acme\""), "{toml}");
    std::fs::write(&kit, toml).unwrap();
    let installed = fresh_db();
    run_cli(&["kit", "install", &kit.to_string_lossy()], &installed).unwrap();
    let out = dir.path().join("out.toml");
    run_cli(&["kit", "export", "k", &out.to_string_lossy()], &installed).unwrap();
    let again = fresh_db();
    run_cli(&["kit", "install", &out.to_string_lossy()], &again).unwrap();
    run_cli(&["f"], &again).unwrap();
    assert_eq!(
        logged_entries(&again, "f")[0].data,
        Some(json!({"brand": "acme"}))
    );

    run_cli(&["spec", "set", "f", "--assign", "brand="], &db).unwrap();
    assert!(db.load_spec("f").unwrap().unwrap().assign.is_empty());

    run_cli(&["spec", "set", "f", "--data-key", "brand:=acme"], &db).unwrap();
    let kinds = db.load_spec("f").unwrap().unwrap().declared_keys(None);
    assert!(
        kinds.iter().all(|k| !matches!(k.kind, KeyKind::Assign(_))),
        "{kinds:?}"
    );
}

/// A key lives in one place, and the latest declaration wins (da-ulc0v,
/// review 12132): a slot declared after an assign of the same name takes
/// the positional, and an assign declared after a slot drops it — through
/// `track`, `spec set` and a state's keys alike.
#[test]
fn a_key_lives_in_one_place_and_the_latest_declaration_wins() {
    // Every `track` writes a row at now: step the clock between commands so
    // no two rows share a millisecond's slug.
    let _clock = set_time("2026-01-15T12:00:00Z");
    let run = |args: &[&str], db: &Db| {
        advance_time("1s");
        run_cli(args, db).unwrap_or_else(|e| panic!("{args:?}: {e:#}"));
    };
    let db = fresh_db();
    run(&["track", "x2", "--verb", "occur", "proj=household"], &db);
    run(&["track", "x2", "-k", "proj="], &db);
    let spec = db.load_spec("x2").unwrap().unwrap();
    assert_eq!(spec.data_keys, ["proj="]);
    assert!(spec.assign.is_empty(), "{:?}", spec.assign);
    run(&["occur", "x2", "5"], &db);
    assert_eq!(logged_entries(&db, "x2")[0].data, Some(json!({"proj": 5})));

    // The reverse order ends with the assign only.
    run(&["track", "x2", "proj=household"], &db);
    let spec = db.load_spec("x2").unwrap().unwrap();
    assert!(spec.data_keys.is_empty(), "{:?}", spec.data_keys);
    assert_eq!(spec.assign["proj"], "household");

    // A bare `KEY=` removes an assign and leaves a slot alone, as `spec set
    // --assign KEY=` does.
    run(&["track", "y", "--verb", "occur", "-k", "proj="], &db);
    run(&["track", "y", "proj="], &db);
    assert_eq!(db.load_spec("y").unwrap().unwrap().data_keys, ["proj="]);

    // `spec set` and `track --key` follow the same rule.
    run(&["spec", "set", "x2", "--data-key", "proj"], &db);
    assert!(db.load_spec("x2").unwrap().unwrap().assign.is_empty());
    run(&["spec", "set", "x2", "--id-assign", "proj=a"], &db);
    let spec = db.load_spec("x2").unwrap().unwrap();
    assert!(spec.data_keys.is_empty(), "{:?}", spec.data_keys);
    run(&["track", "x2", "--key", "proj="], &db);
    assert!(db.load_spec("x2").unwrap().unwrap().id_assign.is_empty());

    // A state's keys too.
    let db = fresh_db();
    run(
        &["spec", "set", "mood", "--verb", "switch", "--state", "sad"],
        &db,
    );
    run(
        &["spec", "add-state", "mood", "sad", "--assign", "why=x"],
        &db,
    );
    run(&["spec", "add-state", "mood", "sad", "--key", "why="], &db);
    let spec = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(spec.state_keys["sad"], ["why="]);
    assert!(spec.state_assign.is_empty(), "{:?}", spec.state_assign);
    run(&["spec", "set", "mood", "--state-assign", "sad=why=y"], &db);
    let spec = db.load_spec("mood").unwrap().unwrap();
    assert!(spec.state_keys.is_empty(), "{:?}", spec.state_keys);
    assert_eq!(spec.state_assign["sad"]["why"], "y");
}

/// `reparse` replays the keys the line bound, not the spec as it stands
/// now (da-zv2jr): a spec edited since the log leaves the row alone.
#[test]
fn reparse_replays_the_keys_the_line_bound() {
    for head in [&["occur", "juice"][..], &["juice"][..]] {
        let db = juice_db();
        run_cli(&[head, &["2", "large"]].concat(), &db).unwrap();
        let logged = logged_entries(&db, "juice")[0].clone();
        run_cli(&["spec", "set", "juice", "--data-key", "size"], &db).unwrap();
        let now = db.load_spec("juice").unwrap().unwrap().declared_keys(None);
        assert_eq!(now[0].name, "size", "{now:?}");
        run_cli(&["reparse", &logged.hid.0, "--fix"], &db).unwrap();
        let after = db.get_entry(&logged.hid.0).unwrap();
        assert_eq!(after.data, logged.data, "{head:?}");
        assert_eq!(after.parser_version, logged.parser_version, "{head:?}");
    }
}
