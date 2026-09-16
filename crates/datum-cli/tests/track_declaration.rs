//! da-c8mh: the first log of a field writes its TRACK document beside it.
//!
//! Driven through the real binary: the declaration is defined by what the
//! *process* writes and echoes, and the bead's acceptance list is phrased
//! that way — a fresh db, one command, two documents.
//!
//! `show_json.rs` covers the shape of a doc and `e2e_subprocess.rs` the
//! table's `action` column. What lives here is the matrix the other two
//! leave out: one case per verb, each asserting its own glyph, plus the
//! `_LAST` group `undo` takes back whole and the import that declares
//! every field it brings.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

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
        let db = tmp.path().join("track_declaration.db");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        Self {
            _tmp: tmp,
            home,
            config_home,
            db,
        }
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum");
        assert!(
            out.status.success(),
            "{args:?} exited {:?}\n  stdout: {}\n  stderr: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).expect("utf-8 stdout")
    }

    /// The `action` of every doc `args` echoed, in printed order.
    fn actions(&self, args: &[&str]) -> Vec<String> {
        let argv: Vec<&str> = args.iter().copied().chain(["--show", "json"]).collect();
        let stdout = self.run(&argv);
        let doc: Value = serde_json::from_str(stdout.trim_end())
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"));
        doc["docs"]
            .as_array()
            .expect("docs array")
            .iter()
            .map(|d| d["action"].as_str().expect("action string").to_string())
            .collect()
    }
}

/// One case per verb: the FIRST log of a field emits two documents, the
/// declaration first, and its action word carries the glyph for the verb
/// the log implies — `occur` → `●`, `start` and `end` → `█`, `switch` →
/// `▞`, `note` → `¢`.
///
/// `end` on a field that was never started is in here on purpose: vividn
/// settled it at 19:13 — the end brings the field into being too, rather
/// than refusing to close what does not exist.
#[test]
fn the_first_log_of_a_field_declares_it_with_its_verbs_glyph() {
    for (argv, glyph) in [
        (vec!["occur", "coffee"], "TRACK\u{25cf}"),
        (vec!["start", "work"], "TRACK\u{2588}"),
        (vec!["end", "chore"], "TRACK\u{2588}"),
        (vec!["switch", "mood", "happy"], "TRACK\u{259e}"),
        (vec!["note", "idea", "words"], "TRACK\u{a2}"),
    ] {
        // A fresh db per case: what is under test is the FIRST log.
        let env = Env::new();
        let actions = env.actions(&argv);
        assert_eq!(
            actions,
            vec![glyph.to_string(), "CREATE".to_string()],
            "{argv:?} should echo its declaration, then the entry",
        );
    }
}

/// Six characters, like `CREATE` — the column the CLI aligns by display
/// width has one cell per row, and a glyph that measured two would push
/// every later cell out of true.
#[test]
fn a_track_action_word_is_six_columns_wide() {
    let env = Env::new();
    for glyph in env.actions(&["occur", "coffee"]) {
        assert_eq!(
            datum_cli::output::visible_width(&glyph),
            6,
            "{glyph:?} is not six columns"
        );
    }
}

/// A field is declared ONCE. The second log of the same field is one
/// document, because the declaration is already written down.
#[test]
fn only_the_first_log_declares() {
    let env = Env::new();
    assert_eq!(env.actions(&["occur", "coffee"]).len(), 2, "the first log");
    assert_eq!(
        env.actions(&["occur", "coffee"]),
        vec!["CREATE".to_string()],
        "the second log has nothing left to declare",
    );
}

/// The pair is one `_LAST` group: `get _LAST` names both, and `undo`
/// takes both back. This is the whole reason the declaration rides in
/// the entry's own atomic write — a half-undone field would leave a spec
/// vouching for a history that is no longer there.
#[test]
fn the_declaration_and_the_entry_undo_together() {
    let env = Env::new();
    env.run(&["occur", "coffee", "--show", "none"]);

    let last = env.run(&["get", "_LAST"]);
    let headers: Vec<&str> = last
        .lines()
        .filter(|l| l.starts_with("TRACK") || l.starts_with("EXISTS "))
        .collect();
    assert_eq!(
        headers.len(),
        2,
        "`_LAST` is the declaration and the entry: {last:?}"
    );
    assert!(headers[0].starts_with("TRACK"), "{headers:?}");

    let undone = env.run(&["undo"]);
    assert_eq!(
        undone.lines().filter(|l| l.starts_with("DELETE ")).count(),
        2,
        "one `undo` takes the whole group: {undone:?}"
    );
    assert!(
        env.run(&["tail"]).contains("[No data]"),
        "nothing should be left standing",
    );
}

/// An imported database is whole under the same rule a typed one is: a
/// v2 backup carrying three fields in three different shapes comes out
/// with a declaration and a spec for each, with the verb its entries
/// imply.
#[test]
fn v2import_declares_every_field_it_brings() {
    let env = Env::new();
    let backup = env._tmp.path().join("backup.json");
    std::fs::write(
        &backup,
        r#"{"backupTime":"2026-01-01T00:00:00.000Z","docs":[
{"_id":"coffee:2026-01-01T09:00:00.000Z","data":{"field":"coffee","occurTime":{"utc":"2026-01-01T09:00:00.000Z","o":0}},"meta":{"createTime":{"utc":"2026-01-01T09:00:00.000Z","o":0}}},
{"_id":"mood:2026-01-01T10:00:00.000Z","data":{"field":"mood","state":"happy","occurTime":{"utc":"2026-01-01T10:00:00.000Z","o":0}},"meta":{"createTime":{"utc":"2026-01-01T10:00:00.000Z","o":0}}},
{"_id":"work:2026-01-01T11:00:00.000Z","data":{"field":"work","state":true,"occurTime":{"utc":"2026-01-01T11:00:00.000Z","o":0}},"meta":{"createTime":{"utc":"2026-01-01T11:00:00.000Z","o":0}}}
]}"#,
    )
    .unwrap();

    env.run(&["db", "v2import", "--from", backup.to_str().unwrap()]);

    let conn = rusqlite::Connection::open(&env.db).expect("open db");
    let mut stmt = conn
        .prepare(
            "SELECT field, json_extract(extra_metadata, '$.track') FROM entries
             WHERE deleted_at IS NULL
               AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NOT NULL
             ORDER BY field",
        )
        .unwrap();
    let declared: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        declared,
        vec![
            ("coffee".to_string(), "occur".to_string()),
            ("mood".to_string(), "switch".to_string()),
            ("work".to_string(), "start".to_string()),
        ],
        "one declaration per imported field, with the verb its entries imply",
    );

    // The spec is written too, so the next log of an imported field has a
    // verb to read rather than a log to guess from.
    for (field, verb) in [("coffee", "occur"), ("mood", "switch"), ("work", "start")] {
        let spec: String = conn
            .query_row(
                "SELECT spec_json FROM field_specs WHERE field = ?1",
                [field],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("no spec for {field}: {e}"));
        let spec: Value = serde_json::from_str(&spec).unwrap();
        assert_eq!(spec["verb"], verb, "{field}'s spec");
        assert!(spec["default_verb"].is_null(), "{field} kept the old key");
    }

    // The declaration precedes the history it declares.
    let (earliest_track, earliest_entry): (String, String) = conn
        .query_row(
            "SELECT
               (SELECT MIN(occur_time) FROM entries
                 WHERE field = 'coffee'
                   AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NOT NULL),
               (SELECT MIN(occur_time) FROM entries
                 WHERE field = 'coffee'
                   AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NULL)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(
        earliest_track < earliest_entry,
        "the declaration should sit ahead of the field's first entry: \
         {earliest_track} vs {earliest_entry}",
    );
}
