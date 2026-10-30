//! The `timezone` field (da-yw6o2): reserved by name in every database, its
//! states IANA zones or `IGNORED`, its own entry in the zone it names.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use datum_core::db::Db;
use datum_core::entry::Entry;
use serde_json::{json, Value};

struct Env(tempfile::TempDir);

impl Env {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    fn path(&self) -> PathBuf {
        self.0.path().join("t.db")
    }

    fn run_in(&self, tz: &str, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(["--db", self.path().to_str().unwrap()])
            .args(args)
            .env("TZ", tz)
            .env("HOME", self.0.path())
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run_in("Europe/Berlin", args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn refused(&self, args: &[&str]) -> String {
        let out = self.run_in("Europe/Berlin", args);
        assert!(!out.status.success(), "{args:?} was not refused");
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    fn rows(&self) -> Vec<Entry> {
        if !Path::new(&self.path()).exists() {
            return Vec::new();
        }
        let db = Db::open(self.path().to_str().unwrap()).unwrap();
        let hids: Vec<String> = db
            .conn()
            .prepare("SELECT hid FROM entries WHERE field = 'timezone' AND deleted_at IS NULL")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        hids.iter().map(|hid| db.get_entry(hid).unwrap()).collect()
    }

    fn only(&self) -> Entry {
        let mut rows = self.rows();
        assert_eq!(rows.len(), 1);
        rows.remove(0)
    }

    fn fields_row(&self) -> Value {
        let out = self.ok(&["fields", "timezone", "--json"]);
        serde_json::from_str::<Value>(&out).unwrap()[0].clone()
    }
}

fn zone_of(entry: &Entry) -> Option<&str> {
    entry.occur_time.as_ref()?.timezone.as_deref()
}

#[test]
fn a_good_name_logs_on_a_database_that_does_not_exist_yet() {
    let env = Env::new();
    env.ok(&["timezone", "Asia/Tokyo"]);
    let entry = env.only();
    assert_eq!(entry.state, Some(json!("Asia/Tokyo")));
    assert_eq!(zone_of(&entry), Some("Asia/Tokyo"));
}

#[test]
fn a_zone_is_stored_in_its_own_spelling() {
    let env = Env::new();
    env.ok(&["timezone", "asia/tokyo"]);
    assert_eq!(env.only().state, Some(json!("Asia/Tokyo")));
}

#[test]
fn a_bad_name_is_refused_offering_the_nearest_real_ones() {
    let env = Env::new();
    for bad in ["Asia/Tokio", "Tokyo"] {
        let err = env.refused(&["timezone", bad]);
        assert!(err.contains("Asia/Tokyo"), "{bad}: {err}");
    }
    assert!(env.rows().is_empty());
}

/// `-z` takes these; a state does not, being IANA zones only.
#[test]
fn an_offset_is_no_zone() {
    let env = Env::new();
    env.refused(&["timezone", "+4"]);
    env.refused(&["timezone", "UTC+4"]);
    assert!(env.rows().is_empty());
}

#[test]
fn it_holds_a_zone_or_ignored_and_nothing_else() {
    let env = Env::new();
    env.refused(&["timezone"]);
    env.refused(&["end", "timezone"]);
    env.refused(&["occur", "timezone"]);
    env.refused(&["timezone", "true"]);
    assert!(env.rows().is_empty());
    env.ok(&["timezone", "IGNORED"]);
    let entry = env.only();
    assert_eq!(entry.state, Some(json!("IGNORED")));
    assert_eq!(zone_of(&entry), Some("Europe/Berlin"));
}

#[test]
fn its_own_entry_is_at_the_clock_of_the_zone_it_names() {
    let env = Env::new();
    env.ok(&["timezone", "Asia/Tokyo", "-d", "2026-06-01", "-t", "14:00"]);
    let entry = env.only();
    let at = entry.occur_time.as_ref().unwrap();
    assert_eq!(at.utc.to_rfc3339(), "2026-06-01T05:00:00+00:00");
    assert_eq!(at.timezone.as_deref(), Some("Asia/Tokyo"));
    assert_eq!(at.offset_minutes, 9 * 60);
}

#[test]
fn a_z_that_disagrees_is_refused_and_one_that_agrees_is_not() {
    let env = Env::new();
    let err = env.refused(&["timezone", "Asia/Tokyo", "-z", "Europe/Berlin"]);
    assert!(
        err.contains("Europe/Berlin") && err.contains("Asia/Tokyo"),
        "{err}"
    );
    env.refused(&["timezone", "Asia/Tokyo", "-z", "+9"]);
    assert!(env.rows().is_empty());
    env.ok(&["timezone", "Asia/Tokyo", "-z", "asia/tokyo"]);
    assert_eq!(zone_of(&env.only()), Some("Asia/Tokyo"));
}

/// The offset says when, the field says where.
#[test]
fn an_offset_on_t_keeps_its_instant_in_the_zone_named() {
    let env = Env::new();
    env.ok(&["timezone", "Asia/Tokyo", "-t", "2026-06-01T12:00+02:00"]);
    let entry = env.only();
    let at = entry.occur_time.as_ref().unwrap();
    assert_eq!(at.utc.to_rfc3339(), "2026-06-01T10:00:00+00:00");
    assert_eq!(at.timezone.as_deref(), Some("Asia/Tokyo"));
    assert_eq!(at.offset_minutes, 9 * 60);
}

/// A fall-back night is the named zone's, not the host's.
#[test]
fn a_repeated_clock_is_asked_about_in_the_zone_named() {
    let env = Env::new();
    let line = ["-d", "2026-10-25", "-t", "02:30"];
    let named = |zone: &'static str| [&["timezone", zone][..], &line[..]].concat();
    let berlin = env.run_in("Asia/Tokyo", &named("Europe/Berlin"));
    assert!(!berlin.status.success(), "Berlin repeats 02:30 that night");
    assert!(env.rows().is_empty());
    let tokyo = env.run_in("Europe/Berlin", &named("Asia/Tokyo"));
    assert!(tokyo.status.success(), "Tokyo repeats nothing that night");
}

#[test]
fn it_ignores_the_zone_the_neighbors_agree_on() {
    let env = Env::new();
    for t in ["10:00", "11:00", "13:00", "14:00"] {
        env.ok(&[
            "occur",
            "tea",
            "-z",
            "America/Chicago",
            "-d",
            "2026-06-01",
            "-t",
            t,
        ]);
    }
    env.ok(&["timezone", "Asia/Tokyo", "-d", "2026-06-01", "-t", "12:00"]);
    assert_eq!(zone_of(&env.only()), Some("Asia/Tokyo"));
}

#[test]
fn a_block_takes_no_declaration_as_its_following_state() {
    let env = Env::new();
    env.ok(&["timezone", "Asia/Tokyo", "30m"]);
    assert_eq!(env.only().following_state, None);
}

#[test]
fn a_database_that_creates_no_fields_still_has_it() {
    let env = Env::new();
    env.ok(&["db", "config", "auto_setup", "false"]);
    env.refused(&["occur", "tea"]);
    env.ok(&["timezone", "Asia/Tokyo"]);
    assert_eq!(env.rows().len(), 1);
}

#[test]
fn the_word_is_reserved() {
    let env = Env::new();
    env.ok(&["occur", "tea"]);
    env.refused(&["alias", "set", "timezone", "occur tea"]);
}

/// A v2 backup's `timezone` field from before the rule: imported as it is,
/// nothing rewritten, reported by `check`, and otherwise left alone.
#[test]
fn an_imported_state_that_is_no_zone_loads_as_it_is_and_check_reports_it() {
    let env = Env::new();
    let backup = env.0.path().join("backup.json");
    let time = json!({"utc": "2025-01-01T09:34:00.000Z", "o": 1, "tz": "Europe/Berlin"});
    let doc = json!({
        "_id": "timezone:2025-01-01T09:34:00.000Z",
        "_rev": "1-synthetic",
        "data": {"occurTime": time, "field": "timezone", "state": "somewhere"},
        "meta": {
            "humanId": "syn0001",
            "createTime": time,
            "modifyTime": time,
            "idStructure": "%field%:%occurTime%",
        },
    });
    let body = json!({"backupTime": "2025-02-02T12:26:32.488Z", "docs": [doc]});
    std::fs::write(&backup, body.to_string()).unwrap();
    env.ok(&["db", "v2import", "--from", backup.to_str().unwrap()]);
    assert_eq!(env.only().state, Some(json!("somewhere")));

    let report = env.run_in("Europe/Berlin", &["check", "tz"]);
    let said = String::from_utf8_lossy(&report.stdout).into_owned()
        + &String::from_utf8_lossy(&report.stderr);
    assert!(said.contains("no IANA zone"), "{said}");

    // An edit of something else keeps it; one of its state is held to the rule.
    env.ok(&["update", "syn0001", "note=old"]);
    assert_eq!(env.only().state, Some(json!("somewhere")));
    env.refused(&["update", "syn0001", "state=elsewhere"]);
    assert_eq!(env.fields_row()["zone"], Value::Null);
}

#[test]
fn an_update_of_its_state_moves_the_entry_to_that_zone() {
    let env = Env::new();
    env.ok(&["timezone", "Asia/Tokyo"]);
    let before = env.only();
    env.ok(&["update", before.hid.as_str(), "state=Europe/Paris"]);
    let after = env.only();
    assert_eq!(zone_of(&after), Some("Europe/Paris"));
    assert_eq!(
        after.occur_time.as_ref().unwrap().utc,
        before.occur_time.as_ref().unwrap().utc
    );
}

#[test]
fn fields_says_the_zone_it_holds_and_whether_it_is_ignored() {
    let env = Env::new();
    env.ok(&["occur", "tea"]);
    assert_eq!(env.fields_row()["zone"], Value::Null);
    env.ok(&["timezone", "Asia/Tokyo"]);
    let row = env.fields_row();
    assert_eq!(
        (&row["zone"], &row["ignored"]),
        (&json!("Asia/Tokyo"), &json!(false))
    );
    env.ok(&["timezone", "IGNORED"]);
    let row = env.fields_row();
    assert_eq!(
        (&row["zone"], &row["ignored"]),
        (&Value::Null, &json!(true))
    );
}

#[test]
fn its_band_is_thin_at_the_top_until_its_spec_places_it() {
    let env = Env::new();
    env.ok(&["occur", "tea"]);
    let row = env.fields_row();
    assert_eq!(row["y"], json!(0.0));
    assert!(row["height"].as_f64().unwrap() <= 0.05);
    env.ok(&["setup", "timezone", "--y", "0.5", "--height", "0.2"]);
    let row = env.fields_row();
    assert_eq!((&row["y"], &row["height"]), (&json!(0.5), &json!(0.2)));
}

#[test]
fn the_dayview_draws_it_at_the_top_over_every_lane_and_ignored_not_at_all() {
    let env = Env::new();
    let day = ["-d", "2026-06-01"];
    env.ok(&[&["switch", "work", "desk", "-t", "01:00"][..], &day[..]].concat());
    env.ok(&[&["timezone", "Europe/Berlin", "-t", "02:00"][..], &day[..]].concat());
    env.ok(&[&["timezone", "IGNORED", "-t", "06:00"][..], &day[..]].concat());
    let svg = env.ok(&["dayview", "-d", "2026-06-01"]);
    let band = svg.find(r#"data-field="timezone""#).expect("a band");
    let work = svg.find(r#"data-field="work""#).expect("a lane");
    assert!(work < band, "drawn after, so over, every lane");
    assert!(svg.contains(r#"data-state="Europe/Berlin""#), "{svg}");
    assert!(!svg.contains(r#"data-state="IGNORED""#));
}
