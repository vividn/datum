//! `datum spec set` integration tests for da-dqb position flags
//! (`--y`, `--height`, `--z-index`).

use crate::common;

use common::run_cli;
use datum_core::field_spec::DEFAULT_FIELD_HEIGHT;
use datum_core::test_util::fresh_db;

#[test]
fn spec_set_persists_position_flags() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "sleep",
            "--verb",
            "start",
            "--y",
            "0.05",
            "--height",
            "0.20",
            "--z-index",
            "1",
        ],
        &db,
    )
    .expect("spec set with position flags");

    let spec = db.load_spec("sleep").unwrap().expect("spec persisted");
    assert_eq!(spec.presentation.y, Some(0.05));
    assert_eq!(spec.presentation.height, Some(0.20));
    assert_eq!(spec.presentation.z_index, Some(1.0));

    let pos = spec.presentation.resolve_position("sleep");
    assert_eq!(pos.y, 0.05);
    assert_eq!(pos.height, 0.20);
    assert_eq!(pos.z_index, 1.0);
}

#[test]
fn spec_set_position_flags_merge_into_existing_spec() {
    let db = fresh_db();

    // First call: set the verb only. Position fields stay None.
    run_cli(&["spec", "set", "focus", "--verb", "switch"], &db).expect("first spec set");
    let spec = db.load_spec("focus").unwrap().unwrap();
    assert!(spec.presentation.y.is_none());
    assert!(spec.presentation.height.is_none());
    assert!(spec.presentation.z_index.is_none());

    // Second call: only --y. Existing default_verb must survive; the other
    // position fields must remain None (so the resolver continues to apply
    // defaults).
    run_cli(&["spec", "set", "focus", "--y", "0.42"], &db).expect("merge --y");
    let spec = db.load_spec("focus").unwrap().unwrap();
    assert_eq!(spec.presentation.y, Some(0.42));
    assert!(spec.presentation.height.is_none());
    assert!(spec.presentation.z_index.is_none());

    // Resolver applies defaults for the missing fields: a `y` with no
    // height is the default band's centre (da-1r506).
    let pos = spec.presentation.resolve_position("focus");
    assert_eq!(pos.y, 0.42 - DEFAULT_FIELD_HEIGHT / 2.0);
    assert_eq!(pos.height, DEFAULT_FIELD_HEIGHT);
    assert_eq!(pos.z_index, pos.y);
}

#[test]
fn spec_set_overlap_two_fields_share_y_with_distinct_z_index() {
    let db = fresh_db();

    run_cli(
        &[
            "spec",
            "set",
            "sleep",
            "--verb",
            "start",
            "--y",
            "0.05",
            "--height",
            "0.20",
            "--z-index",
            "1",
        ],
        &db,
    )
    .unwrap();
    run_cli(
        &[
            "spec",
            "set",
            "focus",
            "--verb",
            "switch",
            "--y",
            "0.05",
            "--height",
            "0.20",
            "--z-index",
            "2",
        ],
        &db,
    )
    .unwrap();

    let sleep = db.load_spec("sleep").unwrap().unwrap();
    let focus = db.load_spec("focus").unwrap().unwrap();
    let s = sleep.presentation.resolve_position("sleep");
    let f = focus.presentation.resolve_position("focus");
    // Both fields occupy the same vertical region (intentional, artistic).
    assert_eq!(s.y, f.y);
    assert_eq!(s.height, f.height);
    // focus stacks above sleep.
    assert!(f.z_index > s.z_index);
}

#[test]
fn spec_set_attached_value_form_works_for_position_flags() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "energy",
            "--verb",
            "switch",
            "--y=0.30",
            "--height=0.10",
            "--z-index=3",
        ],
        &db,
    )
    .expect("attached-value form");

    let spec = db.load_spec("energy").unwrap().unwrap();
    assert_eq!(spec.presentation.y, Some(0.30));
    assert_eq!(spec.presentation.height, Some(0.10));
    assert_eq!(spec.presentation.z_index, Some(3.0));
}

// ─── da-zwqk: per-state descriptions ──────────────────────────────────

#[test]
fn spec_set_persists_state_descriptions() {
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
            "--state-description",
            "happy=feeling good",
            "--state",
            "sad",
            "--state-description=sad=down or flat",
        ],
        &db,
    )
    .expect("spec set with state descriptions");

    let spec = db.load_spec("mood").unwrap().expect("spec persisted");
    assert_eq!(
        spec.state_descriptions.get("happy").map(String::as_str),
        Some("feeling good"),
    );
    assert_eq!(
        spec.state_descriptions.get("sad").map(String::as_str),
        Some("down or flat"),
    );
}

// ─── da-t9ps: safe incremental `spec add-state` ───────────────────────

#[test]
fn spec_add_state_merges_positionally_without_dropping_others() {
    let db = fresh_db();
    // Seed a two-state field.
    run_cli(
        &[
            "spec", "set", "mood", "--verb", "switch", "--state", "happy", "--state", "sad",
        ],
        &db,
    )
    .expect("seed mood");

    // add-state <FIELD> <NAME> <COLOR> <DESCRIPTION> — all four positionals.
    run_cli(
        &["spec", "add-state", "mood", "meh", "yellow", "so-so"],
        &db,
    )
    .expect("add-state with color + description");

    let spec = db.load_spec("mood").unwrap().expect("spec persisted");
    // Existing states survive; the new one is appended.
    assert_eq!(
        spec.allowed_states.as_ref().unwrap(),
        &["happy".to_string(), "sad".to_string(), "meh".to_string()],
    );
    assert_eq!(
        spec.state_colors.get("meh"),
        datum_core::kit_color::resolve_color(Some("yellow"), "meh")
            .unwrap()
            .as_ref()
    );
    assert_eq!(
        spec.state_descriptions.get("meh").map(String::as_str),
        Some("so-so"),
    );
}

/// IGNORED and TRACKED are statuses (da-h1l5o): listed as states, completion
/// offered them, and picking IGNORED ignored the field. `false` is listable,
/// a never-false field's `end` (da-eqb30).
#[test]
fn spec_refuses_a_status_as_a_state() {
    let db = fresh_db();
    for (status, does) in [
        ("IGNORED", "datum ignore mood"),
        ("TRACKED", "datum track mood"),
    ] {
        let json = format!(r#"{{"field":"mood","verb":"switch","allowed_states":["{status}"]}}"#);
        for line in [
            &["spec", "add-state", "mood", status][..],
            &["spec", "set", "mood", "--state", "calm", "--state", status],
            &["spec", "set", "mood", "--from-json", &json],
            &["track", "mood", "--verb", "switch", "--state", status],
        ] {
            let msg = run_cli(line, &db).unwrap_err().to_string();
            assert!(
                msg.contains("is a status, not a state") && msg.contains(does),
                "{line:?}: {msg}"
            );
        }
    }
    assert!(db.load_spec("mood").unwrap().is_none(), "nothing written");

    for name in ["calm", "false"] {
        run_cli(&["spec", "add-state", "mood", name], &db).unwrap();
    }
    let spec = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(spec.allowed_states.unwrap(), ["calm", "false"]);
}

#[test]
fn spec_add_state_dash_color_sets_description_only() {
    let db = fresh_db();
    run_cli(
        &["spec", "add-state", "mood", "grumpy", "-", "irritable"],
        &db,
    )
    .expect("dash color, description only");

    let spec = db.load_spec("mood").unwrap().expect("spec persisted");
    assert!(!spec.state_colors.contains_key("grumpy"));
    assert_eq!(
        spec.state_descriptions.get("grumpy").map(String::as_str),
        Some("irritable"),
    );
}

#[test]
fn spec_set_state_description_with_empty_text_clears_entry() {
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
            "--state-description",
            "happy=initial meaning",
        ],
        &db,
    )
    .expect("seed description");
    run_cli(
        &["spec", "set", "mood", "--state-description", "happy="],
        &db,
    )
    .expect("clear with STATE=");

    let spec = db.load_spec("mood").unwrap().expect("spec persisted");
    assert!(
        !spec.state_descriptions.contains_key("happy"),
        "STATE= form should clear the description",
    );
}

/// da-cwieh: `--never-false` marks a field that is never off, a later
/// `spec set` without it keeps it, and `--allow-false` clears it.
#[test]
fn spec_set_never_false_sets_keeps_and_clears() {
    let db = fresh_db();
    let never = || db.load_spec("env").unwrap().unwrap().never_false;
    let set = |args: &[&str]| run_cli(&[&["spec", "set", "env"], args].concat(), &db).unwrap();
    set(&["--verb", "switch", "--state", "home", "--never-false"]);
    assert!(never());
    set(&["--state", "office"]);
    assert!(never());
    set(&["--allow-false"]);
    assert!(!never());
}

/// da-59amk: a never-false field has no end, so `spec set` makes it no
/// `<field>e` alias: the flag takes the saved one away, `--allow-false`
/// or a listed `false` brings it back.
#[test]
fn spec_set_never_false_drops_the_end_alias_and_allow_false_restores_it() {
    let db = fresh_db();
    let end_alias = || db.lookup_alias("enve").unwrap().map(|a| a.expands_to);
    let set = |args: &[&str]| run_cli(&[&["spec", "set", "env"], args].concat(), &db).unwrap();
    set(&["--verb", "switch", "--state", "home"]);
    assert_eq!(end_alias().as_deref(), Some("end env"));
    set(&["--never-false"]);
    assert_eq!(end_alias(), None);
    assert!(db.lookup_alias("env").unwrap().is_some());
    assert!(db.lookup_alias("home").unwrap().is_some());
    set(&["--state", "home", "--state", "false"]);
    assert_eq!(end_alias().as_deref(), Some("end env"));
    set(&["--state", "home"]);
    assert_eq!(end_alias(), None);
    set(&["--allow-false"]);
    assert_eq!(end_alias().as_deref(), Some("end env"));
}

/// `datum spec --help` names `--color` and its clearing form, and its
/// example runs as the page spells it (da-44oe7: the flag worked and the
/// page never said so).
#[test]
fn the_spec_help_names_color_and_its_example_runs() {
    let page = datum_parse::help::command_help("spec");
    assert!(page.contains("--color C"), "{page}");
    assert!(page.contains("`--color ''` clears it"), "{page}");
    assert!(page.contains("--state-color S=C"), "{page}");

    let line = page
        .lines()
        .find(|l| l.trim().starts_with("datum spec set mood --color"))
        .expect("no `datum spec set mood --color` example on spec --help");
    // `datum` dropped, and the shell's quotes around `#…`
    let argv: Vec<&str> = line
        .split_whitespace()
        .skip(1)
        .map(|w| w.trim_matches('\''))
        .collect();
    let db = fresh_db();
    run_cli(&["spec", "set", "mood", "--verb", "switch"], &db).unwrap();
    run_cli(&argv, &db).unwrap();
    let color = || db.load_spec("mood").unwrap().unwrap().color;
    assert_eq!(color().as_deref(), Some("#b87333"));
    run_cli(&["spec", "set", "mood", "--color", ""], &db).unwrap();
    // cleared, however it is stored: no colour of its own is left
    assert_eq!(color().filter(|c| !c.is_empty()), None);
}

/// One colour rule (da-xw1rx): `spec set --color`, `--state-color`,
/// `add-state`'s COLOR and `track`'s `--color` / `--state S=C` resolve a colour as a kit's `color` does, so a hue
/// name stores the hex `kit install` would, and what the renderer cannot
/// draw is refused and changes nothing.
#[test]
fn spec_set_color_takes_a_kits_colours_and_refuses_the_rest() {
    let db = fresh_db();
    let set = |args: &[&str]| run_cli(&[&["spec", "set", "mood"][..], args].concat(), &db);
    let hue = |c, name| datum_core::kit_color::resolve_color(Some(c), name).unwrap();
    let spec = || db.load_spec("mood").unwrap().unwrap();

    set(&["--verb", "switch", "--color", "red"]).unwrap();
    assert_eq!(spec().color, hue("red", "mood"));
    set(&["--state-color", "happy=Blue"]).unwrap();
    assert_eq!(
        spec().state_colors.get("happy"),
        hue("blue", "happy").as_ref()
    );
    run_cli(&["spec", "add-state", "mood", "sad", "violet"], &db).unwrap();
    assert_eq!(
        spec().state_colors.get("sad"),
        hue("violet", "sad").as_ref()
    );

    set(&["--color", "196"]).unwrap();
    assert_eq!(spec().color.as_deref(), Some("196"));
    set(&["--color", "#B87333"]).unwrap();
    assert_eq!(spec().color.as_deref(), Some("#b87333"));

    let before = spec();
    for args in [
        &["--color", "nonsense"][..],
        &["--color", "300"],
        &["--state-color", "happy=abc"],
    ] {
        let err = format!("{:#}", set(args).unwrap_err());
        assert!(
            err.contains("is not a colour") && err.contains("hue name") && err.contains("pink"),
            "{args:?}: {err}"
        );
    }
    assert!(run_cli(&["spec", "add-state", "mood", "meh", "beige"], &db).is_err());
    assert!(run_cli(&["track", "mood", "--color", "beige"], &db).is_err());
    assert!(run_cli(&["track", "mood", "--state", "happy=beige"], &db).is_err());
    assert!(run_cli(&["track", "mood", "--state", "happy="], &db).is_err());
    let after = spec();
    assert_eq!(after.color, before.color);
    assert_eq!(after.state_colors, before.state_colors);
    assert_eq!(after.allowed_states, before.allowed_states);
}

/// datum.day's field form saves an edited spec whole, as `spec set
/// --from-json`, and a colour stored before the rule (da-xw1rx), which it
/// sends back untouched, saves with it: the rule guards a colour a write
/// changes, and leaves alone one nobody touched (da-eciuy).
#[test]
fn the_field_forms_save_keeps_a_colour_stored_before_the_rule() {
    let db = fresh_db();
    run_cli(
        &[
            "spec", "set", "mood", "--verb", "switch", "--state", "happy",
        ],
        &db,
    )
    .unwrap();
    let mut spec = db.load_spec("mood").unwrap().unwrap();
    spec.color = Some("#0f0".into());
    spec.state_colors.insert("happy".into(), "beige".into());
    db.save_field_spec(&spec).unwrap();
    spec.description = Some("edited".into());
    let json = serde_json::to_string(&spec).unwrap();
    run_cli(&["spec", "set", "mood", "--from-json", &json], &db).unwrap();
    let spec = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(spec.description.as_deref(), Some("edited"));
    assert_eq!(spec.color.as_deref(), Some("#0f0"));
    assert_eq!(spec.state_colors["happy"], "beige");
}

/// `update spec:F color=…` and `spec set --from-json` hold a colour they
/// change to `--color`'s rule (da-eciuy): a hue resolves as the kit's
/// does, and anything else is refused, naming the key, with nothing
/// written.
#[test]
fn update_and_from_json_hold_a_changed_colour_to_the_rule() {
    let db = fresh_db();
    run_cli(
        &[
            "spec", "set", "mood", "--verb", "switch", "--state", "happy",
        ],
        &db,
    )
    .unwrap();
    let spec = || db.load_spec("mood").unwrap().unwrap();
    let hue = |c, name| datum_core::kit_color::resolve(c, name).unwrap();

    run_cli(&["update", "spec:mood", "color=red"], &db).unwrap();
    assert_eq!(spec().color, Some(hue("red", "mood")));
    let mut json = serde_json::to_value(spec()).unwrap();
    json["color"] = "00FF00".into();
    json["state_colors"]["happy"] = "pink".into();
    run_cli(
        &["spec", "set", "mood", "--from-json", &json.to_string()],
        &db,
    )
    .unwrap();
    assert_eq!(spec().color.as_deref(), Some("#00ff00"));
    assert_eq!(spec().state_colors["happy"], hue("pink", "happy"));

    let before = spec();
    let err = format!(
        "{:#}",
        run_cli(&["update", "spec:mood", "color=nonsense"], &db).unwrap_err()
    );
    assert!(err.contains("color: 'nonsense' is not a colour"), "{err}");
    json["state_colors"]["happy"] = "beige".into();
    let err = format!(
        "{:#}",
        run_cli(
            &["spec", "set", "mood", "--from-json", &json.to_string()],
            &db
        )
        .unwrap_err()
    );
    assert!(
        err.contains("state_colors.happy: 'beige' is not a colour"),
        "{err}"
    );
    assert_eq!(spec().color, before.color);
    assert_eq!(spec().state_colors, before.state_colors);
}
