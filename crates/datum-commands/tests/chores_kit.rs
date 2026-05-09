//! End-to-end test for the chores kit (slice 7 / da-d4t.7).
//!
//! Installs `kits/chores.toml` against a fresh DB, logs a couple of
//! chore-shaped entries, and verifies:
//! - the `chore_next_due` index exists on disk,
//! - the `chores_due` view returns one row per field with the latest
//!   next-due value, sorted via the renderer,
//! - the `chorelist` Level-2 command row is present and references the
//!   view + ships the expected color rules,
//! - uninstall removes everything the kit owns.
//!
//! Slice 7 ships the paradigm bundle only — chore/chored alias-creator
//! subcommands are deferred until placeholder substitution lands (see
//! mayor's follow-up beads). This test exercises what the kit ACTUALLY
//! ships against the slice 3-6 primitives.

use std::fs;

use datum_commands::cmd::dispatch_argv;
use datum_core::test_util::fresh_db;
use serde_json::Value as JsonValue;

/// Path to the shipped chores kit, relative to the workspace root.
const CHORES_KIT_PATH: &str = "../../kits/chores.toml";

fn install_chores(db: &datum_core::db::Db) {
    let kit_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(CHORES_KIT_PATH)
        .canonicalize()
        .expect("kits/chores.toml must exist at workspace root");
    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            "--yes".into(),
            kit_path.to_string_lossy().into(),
        ],
        db,
    )
    .unwrap();
}

fn occur(db: &datum_core::db::Db, args: &[&str]) {
    let mut argv: Vec<String> = vec!["datum".into(), "occur".into()];
    argv.extend(args.iter().map(|s| s.to_string()));
    dispatch_argv(argv, db).unwrap();
}

fn index_exists(db: &datum_core::db::Db, on_disk_name: &str) -> bool {
    let n: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            rusqlite::params![on_disk_name],
            |row| row.get(0),
        )
        .unwrap();
    n > 0
}

#[test]
fn chores_kit_parses_and_ships_expected_components() {
    // Sanity check: the shipped TOML is valid and declares the items the
    // bead promises (one index, one view, one Level-2 command). This runs
    // without any DB at all so a parse regression surfaces fast.
    let kit_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(CHORES_KIT_PATH);
    let src = fs::read_to_string(&kit_path).unwrap();
    let kit = datum_core::kit::KitToml::from_str(&src).unwrap();

    assert_eq!(kit.kit.name, "chores");
    assert_eq!(kit.fields.len(), 0, "chores kit ships no field specs");
    assert_eq!(kit.aliases.len(), 0, "chores kit ships no aliases");
    assert_eq!(kit.indices.len(), 1);
    assert_eq!(kit.indices[0].name, "chore_next_due");
    assert_eq!(kit.views.len(), 1);
    assert_eq!(kit.views[0].name, "chores_due");
    assert_eq!(kit.commands.len(), 1);
    assert_eq!(kit.commands[0].name, "chorelist");
    assert_eq!(kit.commands[0].view.as_deref(), Some("chores_due"));
    assert_eq!(kit.commands[0].sort.as_deref(), Some("next_due_raw"));
    assert!(kit.commands[0].flags.iter().any(|f| f == "watch"));
    assert_eq!(kit.commands[0].color_rules.len(), 3);
}

#[test]
fn install_chores_kit_creates_index_view_and_command() {
    let db = fresh_db();
    install_chores(&db);

    // Index is materialised under the kit-prefixed name from slice 5.
    assert!(index_exists(&db, "kit_chores__chore_next_due"));

    // View row is present, tagged to this kit, and the SQL still
    // mentions the COALESCE against all four next-time spellings.
    let view = db
        .get_view("chores_due")
        .unwrap()
        .expect("chores_due view installed");
    assert_eq!(view.kit_id.as_deref(), Some("chores"));
    for needle in ["next_time", "nextTime", "next_date", "nextDate"] {
        assert!(
            view.sql_text.contains(needle),
            "view SQL should reference '{needle}'"
        );
    }

    // Command row references the view and carries the watch flag.
    let cmd = db
        .get_kit_command("chorelist")
        .unwrap()
        .expect("chorelist command installed");
    assert_eq!(cmd.view_name.as_deref(), Some("chores_due"));
    assert_eq!(cmd.sort_col.as_deref(), Some("next_due_raw"));
    assert!(cmd.flags.iter().any(|f| f == "watch"));
    assert_eq!(cmd.color_rules.len(), 3);

    // list_kits surfaces non-zero counts for every shipped layer.
    let kits = db.list_kits().unwrap();
    let chores = kits.iter().find(|k| k.kit_id == "chores").unwrap();
    assert_eq!(chores.field_count, 0);
    assert_eq!(chores.view_count, 1);
    assert_eq!(chores.index_count, 1);
    assert_eq!(chores.command_count, 1);
}

#[test]
fn chores_due_view_returns_latest_entry_per_field() {
    let db = fresh_db();
    install_chores(&db);

    // Two fields, three entries: laundry has an older entry overwritten
    // by a newer one (later create_time wins); dishes has a single entry.
    occur(
        &db,
        &["laundry", "next_date=2026-05-01T00:00:00Z", "done=true"],
    );
    occur(
        &db,
        &["laundry", "next_date=2026-05-08T00:00:00Z", "done=true"],
    );
    occur(
        &db,
        &["dishes", "next_time=2026-05-03T00:00:00Z", "done=true"],
    );

    let view = db.get_view("chores_due").unwrap().unwrap();
    let result = db.execute_view(&view.sql_text).unwrap();

    assert_eq!(
        result.columns,
        vec!["field", "next_due_raw", "last_done", "done_today"]
    );
    assert_eq!(result.rows.len(), 2, "one row per field, latest wins");

    // Build a field → next_due_raw map to assert the MAX(create_time) reduce.
    let mut found: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in &result.rows {
        let field = row[0].as_str().unwrap().to_string();
        let next = row[1].as_str().unwrap().to_string();
        found.insert(field, next);
    }
    assert_eq!(
        found.get("laundry").map(String::as_str),
        Some("2026-05-08T00:00:00Z")
    );
    assert_eq!(
        found.get("dishes").map(String::as_str),
        Some("2026-05-03T00:00:00Z")
    );
}

#[test]
fn entries_without_next_due_are_excluded_from_chores_due() {
    // Ordinary occur entries (no next-time key in `data`) must not show
    // up in chores_due — the view's INNER JOIN against the next-due
    // aggregate filters them out. This is what lets the chorelist
    // command stay focused on chores even when the user logs other
    // verbs/fields against the same DB.
    let db = fresh_db();
    install_chores(&db);

    occur(&db, &["coffee"]);
    // `occur mood switch happy` was a side-effect of the pre-da-k5o6 leak —
    // the trailing positionals fell through to state. Under the v=20
    // contract the verb is the first token, so dispatch the switch
    // directly. Either way: no `nextDate=` key → not a chore.
    dispatch_argv(
        vec!["datum".into(), "switch".into(), "mood".into(), "happy".into()],
        &db,
    )
    .unwrap();

    let view = db.get_view("chores_due").unwrap().unwrap();
    let result = db.execute_view(&view.sql_text).unwrap();
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn nextdate_camelcase_spelling_also_indexes() {
    // The bead's COALESCE accepts four spellings; the camelCase variants
    // exist because legacy datum (Node.js) used them. Verify the
    // view picks them up too.
    let db = fresh_db();
    install_chores(&db);
    occur(
        &db,
        &["taxes", "nextDate=2026-12-31T00:00:00Z", "done=true"],
    );

    let view = db.get_view("chores_due").unwrap().unwrap();
    let result = db.execute_view(&view.sql_text).unwrap();
    assert_eq!(result.rows.len(), 1);
    let next = result.rows[0][1].as_str().unwrap();
    assert_eq!(next, "2026-12-31T00:00:00Z");
}

#[test]
fn uninstall_chores_kit_drops_everything() {
    let db = fresh_db();
    install_chores(&db);
    assert!(index_exists(&db, "kit_chores__chore_next_due"));

    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "uninstall".into(),
            "chores".into(),
        ],
        &db,
    )
    .unwrap();

    assert!(!index_exists(&db, "kit_chores__chore_next_due"));
    assert!(db.get_view("chores_due").unwrap().is_none());
    assert!(db.get_kit_command("chorelist").unwrap().is_none());
    assert!(db.list_kits().unwrap().is_empty());
}

#[test]
fn chorelist_command_renderer_runs_without_panic() {
    // Smoke-test the Level-2 renderer end-to-end. We don't assert on
    // exact stdout (that's covered by kit_command unit tests in
    // datum-commands::commands::kit_command); we just want to confirm
    // the shipped column projection / sort / format hints are mutually
    // consistent so the renderer doesn't bail.
    let db = fresh_db();
    install_chores(&db);
    occur(
        &db,
        &["laundry", "next_date=2030-01-01T00:00:00Z", "done=true"],
    );

    // Calling the renderer directly with no extra args mirrors what
    // `datum chorelist` does in main.rs.
    datum_commands::commands::kit_command::run("chorelist", &[], &db).unwrap();
}

#[test]
fn chorelist_rejects_watch_when_not_declared_does_not_apply() {
    // Sanity: the kit DOES enable --watch, so passing it should be
    // accepted by the renderer's flag check (we don't actually loop —
    // run() entering the watch path would block; instead we just verify
    // get_kit_command's flags list contains "watch").
    let db = fresh_db();
    install_chores(&db);
    let cmd = db.get_kit_command("chorelist").unwrap().unwrap();
    assert!(cmd.flags.iter().any(|f| f == "watch"));
}

/// Pull a representative cell out of a view result for a given field
/// and column name, useful when assertions span multiple rows.
#[allow(dead_code)]
fn cell_for<'a>(
    result: &'a datum_core::db::ViewResult,
    field: &str,
    col: &str,
) -> Option<&'a JsonValue> {
    let field_idx = result.columns.iter().position(|c| c == "field")?;
    let col_idx = result.columns.iter().position(|c| c == col)?;
    for row in &result.rows {
        if row[field_idx].as_str() == Some(field) {
            return Some(&row[col_idx]);
        }
    }
    None
}
