//! `datum edit` with no id edits `_LAST` (da-llcb7), and a stale `_LAST`
//! asks first, as `update` does.

use chrono::{Duration, Utc};
use datum_core::db::Db;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// `datum <args>` as a process on `dir/e.db`, with `input` on stdin and an
/// editor that copies the file it is handed to `dir/opened.json`.
fn run_bin(dir: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write;
    let editor = dir.join("editor.sh");
    std::fs::write(
        &editor,
        format!(
            "#!/bin/sh\ncp \"$1\" '{}'\n",
            dir.join("opened.json").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.join("e.db"))
        .env("DATUM_HOME", dir.join("home"))
        .env("DATUM_CONFIG_HOME", dir.join("home"))
        .env("DATUM_EDITOR", &editor)
        .env("NO_COLOR", "1")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn datum");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait datum")
}

fn seed(dir: &Path) {
    for args in [&["occur", "coffee"][..], &["switch", "mood", "happy"]] {
        assert!(run_bin(dir, args, "").status.success(), "{args:?}");
    }
}

/// `datum edit tea` with an editor that runs `sed` on the file.
fn edit_tea(dir: &Path, sed: &str) -> Output {
    let editor = dir.join("sed.sh");
    std::fs::write(&editor, format!("#!/bin/sh\nsed -i '{sed}' \"$1\"\n")).unwrap();
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.join("e.db"))
        .env("DATUM_HOME", dir.join("home"))
        .env("DATUM_CONFIG_HOME", dir.join("home"))
        .env("DATUM_EDITOR", &editor)
        .args(["edit", "tea"])
        .stdin(Stdio::null())
        .output()
        .expect("spawn datum")
}

/// [`edit_tea`] setting the zone to `zone`, and the row's time after it.
fn edit_zone(dir: &Path, zone: &str) -> (Output, (String, Option<String>, i32)) {
    let sed = format!("s|\"timezone\": [^,]*|\"timezone\": \"{zone}\"|");
    (edit_tea(dir, &sed), tea_time(dir))
}

fn tea_time(dir: &Path) -> (String, Option<String>, i32) {
    Db::open(dir.join("e.db").to_str().unwrap())
        .unwrap()
        .conn()
        .query_row(
            "SELECT occur_time, occur_time_tz, occur_time_offset FROM entries WHERE field = 'tea'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
}

/// `tea` at 10:00 in Berlin, whose +2 no other zone below shares.
fn seed_tea(dir: &Path) -> String {
    let args = [
        "occur",
        "tea",
        "-t",
        "2026-06-02T10:00",
        "-z",
        "Europe/Berlin",
    ];
    assert!(run_bin(dir, &args, "").status.success());
    tea_time(dir).0
}

/// `tea` as a v2 import holds it, in a zone that is none, at +1.
fn import_tea_in_mars(dir: &Path) {
    let time = serde_json::json!({"utc": "2025-01-01T09:34:00.000Z", "o": 1, "tz": "Mars"});
    let doc = serde_json::json!({
        "_id": "tea:2025-01-01T09:34:00.000Z",
        "_rev": "1-synthetic",
        "data": {"occurTime": time, "field": "tea"},
        "meta": {
            "humanId": "syn0001",
            "createTime": time,
            "modifyTime": time,
            "idStructure": "%field%:%occurTime%",
        },
    });
    let backup = dir.join("backup.json");
    let body = serde_json::json!({"backupTime": "2025-02-02T12:26:32.488Z", "docs": [doc]});
    std::fs::write(&backup, body.to_string()).unwrap();
    let import = ["db", "v2import", "--from", backup.to_str().unwrap()];
    assert!(run_bin(dir, &import, "").status.success());
}

#[test]
fn a_name_that_is_no_zone_is_refused_and_nothing_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let utc = seed_tea(tmp.path());

    for bad in ["Mars", ""] {
        let (out, row) = edit_zone(tmp.path(), bad);
        assert!(!out.status.success(), "{bad:?} must be refused: {out:?}");
        assert_eq!(row, (utc.clone(), Some("Europe/Berlin".into()), 120));
    }
    let (out, _) = edit_zone(tmp.path(), "Asia/Tokio");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Asia/Tokyo"),
        "offers the nearest: {stderr}"
    );
}

#[test]
fn a_zone_is_stored_as_z_stores_it_and_the_instant_stands() {
    let tmp = tempfile::tempdir().unwrap();
    let utc = seed_tea(tmp.path());

    for (typed, zone, offset) in [
        ("asia/tokyo", Some("Asia/Tokyo"), 540),
        ("UTC+5:30", None, 330),
        ("-3", None, -180),
        ("UTC", Some("UTC"), 0),
    ] {
        let (out, row) = edit_zone(tmp.path(), typed);
        assert!(out.status.success(), "{typed}: {out:?}");
        assert_eq!(
            row,
            (utc.clone(), zone.map(String::from), offset),
            "{typed}"
        );
    }
}

#[test]
fn a_row_already_holding_a_bad_zone_can_be_edited_and_repaired() {
    let tmp = tempfile::tempdir().unwrap();
    import_tea_in_mars(tmp.path());
    let utc = tea_time(tmp.path()).0;

    // Left as it is, the edit of something else goes through.
    let out = edit_tea(tmp.path(), "s|\"state\": [^,]*|\"state\": \"hot\"|");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(tea_time(tmp.path()).1.as_deref(), Some("Mars"));

    let (out, row) = edit_zone(tmp.path(), "Europe/Paris");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(row, (utc, Some("Europe/Paris".into()), 60));
}

/// da-i35uc: a moved instant takes its zone's offset there, with the zone
/// changed in the same edit or not, and `check tz` finds nothing.
#[test]
fn a_moved_instant_takes_its_zones_offset_there() {
    let tmp = tempfile::tempdir().unwrap();
    seed_tea(tmp.path());

    let out = edit_tea(tmp.path(), "s|\"utc\": \"2026-06-02|\"utc\": \"2026-12-02|");
    assert!(out.status.success(), "{out:?}");
    let (utc, zone, offset) = tea_time(tmp.path());
    assert!(utc.starts_with("2026-12-02"), "{utc}");
    assert_eq!((zone.as_deref(), offset), (Some("Europe/Berlin"), 60));
    let check = run_bin(tmp.path(), &["check", "tz"], "");
    assert!(check.status.success(), "{check:?}");

    let out = edit_tea(
        tmp.path(),
        "s|\"utc\": \"2026-12-02|\"utc\": \"2026-06-02|; \
         s|\"timezone\": [^,]*|\"timezone\": \"America/Chicago\"|",
    );
    assert!(out.status.success(), "{out:?}");
    let (utc, zone, offset) = tea_time(tmp.path());
    assert!(utc.starts_with("2026-06-02"), "{utc}");
    assert_eq!((zone.as_deref(), offset), (Some("America/Chicago"), -300));
}

/// A zone the edit left stays even where it cannot be read, so a row holding a
/// bad one still takes a new instant, keeping the offset it has.
#[test]
fn a_bad_zone_left_keeps_its_offset_as_the_instant_moves() {
    let tmp = tempfile::tempdir().unwrap();
    import_tea_in_mars(tmp.path());

    let out = edit_tea(tmp.path(), "s|\"utc\": \"2025-01-01|\"utc\": \"2025-07-01|");
    assert!(out.status.success(), "{out:?}");
    let (utc, zone, offset) = tea_time(tmp.path());
    assert!(utc.starts_with("2025-07-01"), "{utc}");
    assert_eq!((zone.as_deref(), offset), (Some("Mars"), 60));
}

/// da-i35uc: a zone the neighbours gave is a guess until it is stated, and
/// an edit changing it states it; any other edit leaves the marker.
#[test]
fn an_edit_changing_the_zone_drops_the_neighbours_marker() {
    let tmp = tempfile::tempdir().unwrap();
    for day in [12, 13, 14, 16, 17, 18] {
        let date = format!("2024-06-{day}");
        let z = ["occur", "coffee", "-z", "Pacific/Chatham", "-d", &date];
        assert!(
            run_bin(tmp.path(), &[&z[..], &["-t", "12:00"]].concat(), "")
                .status
                .success()
        );
    }
    let tea = ["occur", "tea", "-d", "2024-06-15", "-t", "12:00"];
    assert!(run_bin(tmp.path(), &tea, "").status.success());
    let meta = || -> Option<String> {
        Db::open(tmp.path().join("e.db").to_str().unwrap())
            .unwrap()
            .conn()
            .query_row(
                "SELECT extra_metadata FROM entries WHERE field = 'tea'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(meta().as_deref(), Some(r#"{"tz_from":"neighbours"}"#));

    let out = edit_tea(tmp.path(), "s|\"utc\": \"2024-06-14|\"utc\": \"2024-06-13|");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(meta().as_deref(), Some(r#"{"tz_from":"neighbours"}"#));

    let (out, _) = edit_zone(tmp.path(), "Asia/Tokyo");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(meta(), None);
}

#[test]
fn a_bare_edit_opens_the_last_entry() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path());

    let out = run_bin(tmp.path(), &["edit"], "");
    assert!(out.status.success(), "{out:?}");
    let opened: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join("opened.json")).unwrap())
            .unwrap();
    assert_eq!(opened["field"], "mood");
    assert_eq!(opened["state"], "happy");
}

#[test]
fn a_stale_last_asks_first_and_a_no_opens_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    seed(tmp.path());
    Db::open(tmp.path().join("e.db").to_str().unwrap())
        .unwrap()
        .conn()
        .execute(
            "UPDATE last_docs SET time = ?1",
            [(Utc::now() - Duration::minutes(16)).to_rfc3339()],
        )
        .unwrap();

    let out = run_bin(tmp.path(), &["edit"], "n\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[Y/n]"), "a stale _LAST must ask: {stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("Aborted."));
    assert!(!tmp.path().join("opened.json").exists(), "no editor ran");

    // Named, the entry opens without asking.
    let out = run_bin(tmp.path(), &["edit", "coffee"], "");
    assert!(out.status.success(), "{out:?}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("[Y/n]"));
    assert!(tmp.path().join("opened.json").exists());
}
