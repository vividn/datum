//! `track` and `ignore` echo the row they write as any log does (da-te4id).
//! vividn, 2026-10-31: «they should instead use the standard render pipeline
//! with the ACTION verb (IGNORE) and (TRACK<Symbol>) as well as showing the
//! state change into IGNORED or TRACKED».

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn run(db: &Path, show: &str, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("NO_COLOR", "1")
        .env("DATUM_SHOW", show)
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "datum {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// The echo's two lines: the header's action and id's field, and the info
/// line past its time.
fn echoed(out: &str) -> (String, String, String) {
    let [header, info] = out.lines().collect::<Vec<_>>()[..] else {
        panic!("not two lines: {out}");
    };
    let header: Vec<&str> = header.split(' ').collect();
    let (field, _) = header[2].split_once(':').expect(header[2]);
    let (_, what) = info.split_once(' ').expect(info);
    (
        header[0].to_string(),
        field.to_string(),
        what.trim().to_string(),
    )
}

fn says(out: &str, action: &str, field: &str, state: &str) {
    let (a, f, what) = echoed(out);
    assert_eq!((a.as_str(), f.as_str()), (action, field), "{out}");
    let words: Vec<&str> = what.split_whitespace().collect();
    assert_eq!(words, [field, state], "{out}");
}

#[test]
fn each_echoes_its_action_and_state_on_an_occur_and_a_stateful_field() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    run(&db, "", &["setup", "tea", "--verb", "occur"]);
    run(&db, "", &["track", "mood", "--verb", "switch"]);
    run(&db, "", &["track", "nap", "--verb", "start"]);
    run(&db, "", &["track", "jot", "--verb", "add"]);

    for (field, track) in [
        ("tea", "TRACK●"),
        ("mood", "TRACK▞"),
        ("nap", "TRACK█"),
        ("jot", "TRACK¢"),
    ] {
        says(
            &run(&db, "", &["ignore", field]),
            "IGNORE",
            field,
            "IGNORED",
        );
        says(&run(&db, "", &["track", field]), track, field, "TRACKED");
        // Each six columns, as CREATE.
        assert_eq!(track.chars().count(), 6);
    }
}

/// Already there, nothing is written and it says so; the same under JSON as
/// `setup`'s NODIFF, naming the row that holds the field.
#[test]
fn already_tracked_or_ignored_writes_nothing_and_says_no_change() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    run(&db, "", &["switch", "mood", "happy"]);
    let count = || run(&db, "", &["count", "mood"]);
    let before = count();

    assert_eq!(run(&db, "", &["track", "mood"]), "no change\n");
    assert_eq!(count(), before, "a running field gets no TRACK row");
    let doc: Value = serde_json::from_str(&run(&db, "json", &["track", "mood"])).unwrap();
    assert_eq!(doc["docs"][0]["action"], "NODIFF");
    assert_eq!(doc["docs"][0]["state"], "happy");
    assert_eq!(count(), before);

    run(&db, "", &["ignore", "mood"]);
    let before = count();
    assert_eq!(run(&db, "", &["ignore", "mood"]), "no change\n");
    let doc: Value = serde_json::from_str(&run(&db, "json", &["ignore", "mood"])).unwrap();
    assert_eq!(doc["docs"][0]["action"], "NODIFF");
    assert_eq!(doc["docs"][0]["state"], "IGNORED");
    assert_eq!(count(), before);
}

/// The JSON echo is what a log's is, its `action`, `shape` and the state it
/// came from.
#[test]
fn json_carries_action_shape_and_the_state_left() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    run(&db, "", &["switch", "mood", "happy"]);
    let happy: Value = serde_json::from_str(&run(&db, "json", &["get", "mood"])).unwrap();
    let happy = happy["docs"][0]["state_color"]
        .as_str()
        .unwrap()
        .to_string();

    let doc: Value = serde_json::from_str(&run(&db, "json", &["ignore", "mood"])).unwrap();
    let doc = &doc["docs"][0];
    assert_eq!(doc["action"], "IGNORE");
    assert_eq!(doc["shape"], "SWITCH");
    assert_eq!(doc["state"], "IGNORED");
    // The LEAD slot, painted with what the field left.
    let lead = doc["info_segments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["text"] == "  " && s["color"] == happy.as_str());
    assert!(lead, "no {happy} LEAD in {doc}");

    let doc: Value = serde_json::from_str(&run(&db, "json", &["track", "mood"])).unwrap();
    let doc = &doc["docs"][0];
    assert_eq!(doc["action"], "TRACK▞");
    assert_eq!(doc["shape"], "SWITCH");
    assert_eq!(doc["state"], "TRACKED");
}

/// A declaration writes two rows, the spec and the status, and echoes both.
#[test]
fn a_declaration_echoes_the_spec_then_the_row() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    let doc: Value =
        serde_json::from_str(&run(&db, "json", &["track", "tea", "--verb", "occur"])).unwrap();
    let actions: Vec<&str> = doc["docs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["CREATE", "TRACK●"]);

    let out = run(&db, "", &["track", "nap", "--verb", "start"]);
    let last_two = out.lines().rev().take(2).collect::<Vec<_>>();
    assert!(last_two[1].starts_with("TRACK█ "), "{out}");
    assert!(last_two[0].contains("TRACKED"), "{out}");
    assert!(out.starts_with("CREATE "), "{out}");
}
