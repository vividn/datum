//! A spec on a field template covers every field the template writes and
//! every field whose name parses against it; a field's own spec overrides
//! it key by key (da-38obu).

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::fresh_db;

const BOARD: &str = "[kit]\nname = \"board\"\n\n[[field]]\nname = \"issue__%hid:8%\"\n\
                     verb = \"switch\"\nstates = { queued = {}, in_progress = {}, done = {} }\n";

fn board() -> Db {
    let db = fresh_db();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.toml");
    std::fs::write(&path, BOARD).unwrap();
    run_cli(&["kit", "install", &path.to_string_lossy()], &db).unwrap();
    db
}

fn states(db: &Db, field: &str) -> Option<Vec<String>> {
    db.load_spec(field).unwrap().unwrap().allowed_states
}

fn spec_rows(db: &Db) -> Vec<String> {
    let mut rows: Vec<String> = db
        .conn()
        .prepare("SELECT slug FROM entries WHERE field = 'spec' AND deleted_at IS NULL")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    rows.sort();
    rows
}

#[test]
fn an_issue_written_through_the_template_and_one_switched_directly_both_get_its_states() {
    let db = board();
    run_cli(&["add", "issue__%hid:8%", "--id", "doc", "title=x"], &db).unwrap();
    let written = db.distinct_fields().unwrap();
    let written = written.iter().find(|f| f.starts_with("issue__")).unwrap();
    run_cli(&["switch", written, "in_progress"], &db).unwrap();
    run_cli(&["switch", "issue__2qvk0dk4", "queued"], &db).unwrap();
    // The covered field's own word, as a specced field's.
    run_cli(&["issue__2qvk0dk4", "done"], &db).unwrap();

    let all = ["queued", "in_progress", "done"].map(String::from).to_vec();
    for (field, now) in [
        (written.as_str(), "in_progress"),
        ("issue__2qvk0dk4", "done"),
    ] {
        assert_eq!(states(&db, field), Some(all.clone()), "{field}");
        let state = db.state_at(field, chrono::Utc::now()).unwrap();
        assert_eq!(state, Some(serde_json::json!(now)), "{field}");
    }
    assert_eq!(
        spec_rows(&db),
        ["spec:issue__%hid:8%"],
        "no covered field has an automatic spec"
    );
    // `issue__notes` is five characters, not eight.
    run_cli(&["switch", "issue__notes", "queued"], &db).unwrap();
    assert_eq!(states(&db, "issue__notes"), None);
}

#[test]
fn a_fields_own_spec_overrides_the_template_key_by_key() {
    let db = board();
    run_cli(&["switch", "issue__2qvk0dk4", "queued"], &db).unwrap();
    run_cli(&["ignore", "issue__2qvk0dk4"], &db).unwrap();
    run_cli(&["setup", "issue__2qvk0dk4", "--color", "#abcdef"], &db).unwrap();

    let spec = db.load_spec("issue__2qvk0dk4").unwrap().unwrap();
    assert_eq!(spec.color.as_deref(), Some("#abcdef"));
    assert_eq!(spec.allowed_states.unwrap().len(), 3, "ignore keeps them");
    let covered = spec.template.unwrap();
    assert_eq!(covered.template, "issue__%hid:8%");
    assert_eq!(covered.own, ["color"]);
    // The own spec stores what it sets and nothing of the template's.
    let own = db.spec_entry("issue__2qvk0dk4").unwrap().unwrap().data;
    assert_eq!(
        own,
        Some(serde_json::json!({"field": "issue__2qvk0dk4", "color": "#abcdef"}))
    );

    // A change to the template reaches the key the field does not set.
    run_cli(
        &["setup", "issue__%hid:8%", "--add-state", "in_review"],
        &db,
    )
    .unwrap();
    assert_eq!(states(&db, "issue__2qvk0dk4").unwrap().len(), 4);
}

#[test]
fn a_bare_spec_from_before_the_template_sets_nothing() {
    let db = fresh_db();
    run_cli(&["occur", "issue__2qvk0dk4"], &db).unwrap();
    assert!(db.spec_entry("issue__2qvk0dk4").unwrap().is_some());
    run_cli(
        &[
            "setup",
            "issue__%hid:8%",
            "--verb",
            "switch",
            "--states",
            "a,b",
        ],
        &db,
    )
    .unwrap();
    let spec = db.load_spec("issue__2qvk0dk4").unwrap().unwrap();
    assert_eq!(spec.allowed_states.unwrap(), ["a", "b"]);
    assert!(spec.template.unwrap().own.is_empty());
}

#[test]
fn a_template_that_ties_another_is_refused_and_the_longer_literal_wins() {
    let db = board();
    let err = run_cli(&["setup", "issue__%issue_id%", "--verb", "occur"], &db).unwrap_err();
    assert!(format!("{err:#}").contains("ties the template"), "{err:#}");

    run_cli(&["setup", "issue__x%hid:7%", "--verb", "occur"], &db).unwrap();
    assert_eq!(states(&db, "issue__x2qvk0dk"), None, "the longer one");
    assert_eq!(states(&db, "issue__y2qvk0dk").unwrap().len(), 3);
}

#[test]
fn a_template_spec_undoes_like_any_spec() {
    let db = board();
    run_cli(&["setup", "issue__%hid:8%", "--color", "#abcdef"], &db).unwrap();
    run_cli(&["undo"], &db).unwrap();
    let spec = db.load_spec("issue__2qvk0dk4").unwrap().unwrap();
    assert_eq!(spec.color, None);
    assert!(
        spec.derived_aliases.is_empty(),
        "a template derives no aliases"
    );
}

/// da-j04cq: the board, with its template saying a write creates no issue.
fn strict_board() -> Db {
    let db = board();
    run_cli(&["setup", "issue__%hid:8%", "--auto-setup", "false"], &db).unwrap();
    db
}

#[test]
fn a_template_with_auto_setup_off_refuses_a_never_written_name_unless_setup() {
    let db = strict_board();
    let err = run_cli(&["switch", "issue__abcdefgh", "queued"], &db).unwrap_err();
    assert!(err.to_string().contains("--setup"), "{err}");
    assert!(
        !db.field_exists("issue__abcdefgh").unwrap(),
        "nothing written"
    );
    let err = run_cli(&["add", "issue__%hid:8%", "--id", "doc"], &db).unwrap_err();
    assert!(
        err.to_string().contains("--setup"),
        "a new hid is a new field: {err}"
    );

    run_cli(&["switch", "issue__abcdefgh", "queued", "--setup"], &db).unwrap();
    // Now it exists, and logs as before.
    run_cli(&["switch", "issue__abcdefgh", "done"], &db).unwrap();
    let state = db.state_at("issue__abcdefgh", chrono::Utc::now()).unwrap();
    assert_eq!(state, Some(serde_json::json!("done")));
    run_cli(&["add", "issue__%hid:8%", "--id", "doc", "--setup"], &db).unwrap();
    // A field no template covers is the db's, which creates it.
    run_cli(&["occur", "coffee"], &db).unwrap();
    assert!(db.field_exists("coffee").unwrap());
}

#[test]
fn auto_setup_is_a_templates_alone() {
    let db = strict_board();
    let err = run_cli(
        &["setup", "mood", "--verb", "switch", "--auto-setup", "false"],
        &db,
    )
    .unwrap_err();
    assert!(err.to_string().contains("not one"), "{err}");
    assert!(db.load_spec("mood").unwrap().is_none());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.toml");
    let kit = "[kit]\nname = \"plain\"\n\n[[field]]\nname = \"tea\"\nverb = \"occur\"\nauto_setup = false\n";
    std::fs::write(&path, kit).unwrap();
    let err = run_cli(&["kit", "install", &path.to_string_lossy()], &db).unwrap_err();
    assert!(format!("{err:#}").contains("not one"), "{err:#}");

    run_cli(&["setup", "issue__%hid:8%", "--auto-setup", ""], &db).unwrap();
    run_cli(&["switch", "issue__abcdefgh", "queued"], &db).unwrap();
}

#[test]
fn a_kits_template_sets_auto_setup() {
    let db = fresh_db();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.toml");
    std::fs::write(&path, format!("{BOARD}auto_setup = false\n")).unwrap();
    run_cli(&["kit", "install", &path.to_string_lossy()], &db).unwrap();
    assert!(run_cli(&["switch", "issue__abcdefgh", "queued"], &db).is_err());
    run_cli(&["switch", "issue__abcdefgh", "queued", "--setup"], &db).unwrap();
}

/// da-w6pp2: the database's own `auto_setup`, an entry: off, a write naming
/// a field that does not exist refuses unless `--setup`, and an existing
/// field logs as before.
#[test]
fn a_db_with_auto_setup_off_refuses_a_new_field_unless_setup() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).unwrap();
    run_cli(&["db", "config", "auto_setup", "false"], &db).unwrap();
    assert_eq!(db.setting("auto_setup").unwrap(), Some(false));

    let err = run_cli(&["occur", "newfield"], &db).unwrap_err();
    let err = err.to_string();
    assert!(
        err.contains("'newfield'") && err.contains("--setup"),
        "{err}"
    );
    assert!(!db.field_exists("newfield").unwrap(), "nothing written");

    run_cli(&["occur", "newfield", "--setup"], &db).unwrap();
    assert!(db.field_exists("newfield").unwrap());
    run_cli(&["occur", "newfield"], &db).unwrap();
    run_cli(&["occur", "coffee"], &db).unwrap();

    // `true` opens it again.
    run_cli(&["db", "config", "auto_setup", "true"], &db).unwrap();
    run_cli(&["occur", "tea"], &db).unwrap();
}

/// A template's own `auto_setup` overrides the database's either way.
#[test]
fn a_templates_auto_setup_overrides_the_dbs() {
    let db = board();
    run_cli(&["db", "config", "auto_setup", "false"], &db).unwrap();
    // The template says nothing: the database's closes it.
    assert!(run_cli(&["switch", "issue__abcdefgh", "queued"], &db).is_err());
    run_cli(&["setup", "issue__%hid:8%", "--auto-setup", "true"], &db).unwrap();
    run_cli(&["switch", "issue__abcdefgh", "queued"], &db).unwrap();
    // A name no template covers stays closed.
    assert!(run_cli(&["occur", "newfield"], &db).is_err());
}

/// The setting is an entry the database keeps off the life surfaces, and
/// `setting` takes no other.
#[test]
fn a_setting_is_a_meta_entry() {
    let db = fresh_db();
    run_cli(&["db", "config", "auto_setup", "false"], &db).unwrap();
    assert!(!db
        .distinct_fields()
        .unwrap()
        .contains(&"setting".to_string()));
    assert!(run_cli(&["occur", "setting"], &db).is_err());
    assert!(run_cli(&["db", "config", "nope", "false"], &db).is_err());
    assert!(run_cli(&["db", "config", "auto_setup", "maybe"], &db).is_err());
    run_cli(&["undo"], &db).unwrap();
    assert_eq!(db.setting("auto_setup").unwrap(), None);
}

/// da-iov64: the board, taking only its listed states.
fn listed_board() -> Db {
    let db = board();
    run_cli(&["setup", "issue__%hid:8%", "--only-listed-states"], &db).unwrap();
    db
}

fn refused(db: &Db, line: &[&str]) -> String {
    format!("{:#}", run_cli(line, db).expect_err("refused"))
}

#[test]
fn a_typod_state_on_a_covered_field_advises_setting_up_the_template() {
    let db = listed_board();
    let msg = refused(&db, &["switch", "issue__2qvk0dk4", "in_prgress"]);
    assert!(
        msg.contains("`datum setup 'issue__%hid:8%' --add-state in_prgress`")
            && msg.contains("`datum setup 'issue__%hid:8%' --any-states`"),
        "{msg}"
    );
    assert!(!msg.contains("setup issue__2qvk0dk4"), "{msg}");
}

#[test]
fn ending_a_never_false_covered_field_advises_setting_up_the_template() {
    let db = listed_board();
    run_cli(&["setup", "issue__%hid:8%", "--never-false"], &db).unwrap();
    run_cli(&["switch", "issue__2qvk0dk4", "queued"], &db).unwrap();
    let msg = refused(&db, &["end", "issue__2qvk0dk4"]);
    assert!(
        msg.contains("`datum setup 'issue__%hid:8%' --add-state false`")
            && msg.contains("`datum setup 'issue__%hid:8%' --allow-false`"),
        "{msg}"
    );
    assert!(!msg.contains("setup issue__2qvk0dk4"), "{msg}");
}

#[test]
fn a_covered_field_whose_own_spec_takes_only_its_own_states_is_advised_by_name() {
    let db = board();
    let own = ["--states", "queued,blocked", "--only-listed-states"];
    run_cli(&[&["setup", "issue__2qvk0dk4"][..], &own].concat(), &db).unwrap();
    let msg = refused(&db, &["switch", "issue__2qvk0dk4", "done"]);
    assert!(
        msg.contains("`datum setup issue__2qvk0dk4 --add-state done`")
            && msg.contains("`datum setup issue__2qvk0dk4 --any-states`"),
        "{msg}"
    );
    assert!(!msg.contains("issue__%hid:8%"), "{msg}");
}

#[test]
fn a_plain_fields_refusal_advises_setting_up_the_field() {
    let db = fresh_db();
    let spec = ["setup", "env", "--verb", "switch", "--states", "home"];
    run_cli(
        &[&spec[..], &["--only-listed-states", "--never-false"]].concat(),
        &db,
    )
    .unwrap();
    let msg = refused(&db, &["switch", "env", "park"]);
    assert!(
        msg.contains("`datum setup env --add-state park`, or `datum setup env --any-states`"),
        "{msg}"
    );
    run_cli(&["switch", "env", "home"], &db).unwrap();
    let msg = refused(&db, &["end", "env"]);
    assert!(
        msg.contains("(`datum setup env --add-state false`), or `datum setup env --allow-false`"),
        "{msg}"
    );
}

#[test]
fn each_suggestion_names_the_spec_that_owns_the_key_it_changes() {
    // The template lists the states; the field's own spec takes only them.
    let db = board();
    run_cli(&["setup", "issue__2qvk0dk4", "--only-listed-states"], &db).unwrap();
    let msg = refused(&db, &["switch", "issue__2qvk0dk4", "in_prgress"]);
    assert!(
        msg.contains("`datum setup 'issue__%hid:8%' --add-state in_prgress`")
            && msg.contains("`datum setup issue__2qvk0dk4 --any-states`"),
        "{msg}"
    );

    // The template takes only its states; the field's own spec is never false.
    let db = listed_board();
    run_cli(&["setup", "issue__2qvk0dk4", "--never-false"], &db).unwrap();
    run_cli(&["switch", "issue__2qvk0dk4", "queued"], &db).unwrap();
    let msg = refused(&db, &["end", "issue__2qvk0dk4"]);
    assert!(
        msg.contains("`datum setup 'issue__%hid:8%' --add-state false`")
            && msg.contains("`datum setup issue__2qvk0dk4 --allow-false`"),
        "{msg}"
    );
}

/// Every `datum setup …` a refusal advises, as argv, split as a shell does.
fn advised(msg: &str) -> Vec<Vec<String>> {
    let lines: Vec<_> = msg
        .split('`')
        .filter_map(|s| s.strip_prefix("datum "))
        .map(datum_parse::alias::shell_split)
        .collect();
    assert!(!lines.is_empty(), "{msg}");
    lines
}

#[test]
fn every_advised_setup_runs_and_then_takes_the_refused_line() {
    // da-9afg5: the advice said `--state`, which setup refuses.
    let covered = || listed_board();
    let covered_never_false = || {
        let db = listed_board();
        run_cli(&["setup", "issue__%hid:8%", "--never-false"], &db).unwrap();
        run_cli(&["switch", "issue__2qvk0dk4", "queued"], &db).unwrap();
        db
    };
    let plain = || {
        let db = fresh_db();
        let spec = ["setup", "env", "--verb", "switch", "--states", "home"];
        run_cli(
            &[&spec[..], &["--only-listed-states", "--never-false"]].concat(),
            &db,
        )
        .unwrap();
        run_cli(&["switch", "env", "home"], &db).unwrap();
        db
    };
    let cases: [(&dyn Fn() -> Db, &[&str]); 6] = [
        (&covered, &["switch", "issue__2qvk0dk4", "in_prgress"]),
        (&covered_never_false, &["end", "issue__2qvk0dk4"]),
        (&plain, &["switch", "env", "park"]),
        (&plain, &["end", "env"]),
        // A state a shell would split or read is quoted as one word.
        (&plain, &["switch", "env", "very tired"]),
        (&plain, &["switch", "env", "it's $HOME"]),
    ];
    for (make, line) in cases {
        let msg = refused(&make(), line);
        assert!(!msg.contains("--state "), "{msg}");
        for setup in advised(&msg) {
            let db = make();
            let setup: Vec<&str> = setup.iter().map(String::as_str).collect();
            run_cli(&setup, &db).unwrap_or_else(|e| panic!("{setup:?}: {e:#}"));
            run_cli(line, &db).unwrap_or_else(|e| panic!("{line:?} after {setup:?}: {e:#}"));
        }
    }
}
