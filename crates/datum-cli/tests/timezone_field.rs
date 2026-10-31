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

/// da-wpgrm: `datum check tz` warns where an entry's zone is not the one the
/// field holds at its instant.
mod off_declared {
    use super::*;

    const T0: &str = "2026-03-01T12:00:00+00:00";
    const T1: &str = "2026-03-02T12:00:00+00:00";
    const T2: &str = "2026-03-03T12:00:00+00:00";

    /// `timezone <zone>` at `T0`, and a `work` entry in `own` at `T1`.
    fn declared(zone: &str, own: &str) -> Env {
        let env = Env::new();
        env.ok(&["timezone", zone, "-t", T0]);
        env.ok(&["occur", "work", "-t", T1, "-z", own]);
        env
    }

    /// What `datum check tz` finds, as `(entry's zone, what the field holds)`.
    fn found(env: &Env) -> Vec<(String, String)> {
        let out = env.run_in("Europe/Berlin", &["check", "tz", "--json"]);
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        let issues = report["issues"].as_array().unwrap();
        let Some(issue) = issues.iter().find(|i| i["category"] == "tz_off_declared") else {
            return vec![];
        };
        assert_eq!(issue["severity"], "warning", "{issue}");
        let detail = issue["example"]["detail"].as_str().unwrap();
        let (own, declared) = detail.split_once("  declared=").unwrap();
        vec![(
            own.trim_start_matches("tz=").to_string(),
            declared.to_string(),
        )]
    }

    fn pair(own: &str, declared: &str) -> Vec<(String, String)> {
        vec![(own.to_string(), declared.to_string())]
    }

    #[test]
    fn a_zone_other_than_the_declared_one_warns_and_shows_it() {
        let env = declared("Asia/Tokyo", "Europe/Berlin");
        assert_eq!(found(&env), pair("Europe/Berlin", "Asia/Tokyo"));
        let shown = env.ok(&["check", "tz"]);
        let row = (shown.lines())
            .find(|l| l.contains("work"))
            .unwrap_or_else(|| panic!("{shown}"));
        assert!(row.contains("declared Asia/Tokyo"), "{shown}");
    }

    /// Same rules are no excuse, same offset today less so, and a spelling of
    /// the zone is the zone.
    #[test]
    fn it_compares_names_and_not_rules() {
        let env = declared("Europe/Paris", "Europe/Berlin");
        assert_eq!(found(&env), pair("Europe/Berlin", "Europe/Paris"));
        let env = declared("Africa/Lagos", "Europe/Berlin");
        assert_eq!(found(&env), pair("Europe/Berlin", "Africa/Lagos"));
        assert_eq!(found(&declared("Asia/Kolkata", "Asia/Calcutta")), vec![]);
        assert_eq!(found(&declared("Asia/Calcutta", "Asia/Kolkata")), vec![]);
    }

    #[test]
    fn no_field_and_no_zone_held_claim_nothing() {
        let env = Env::new();
        env.ok(&["occur", "work", "-t", T1, "-z", "Asia/Tokyo"]);
        assert_eq!(found(&env), vec![]);
        // Before the field's first row, and under IGNORED.
        env.ok(&["timezone", "Europe/Berlin", "-t", T2]);
        assert_eq!(found(&env), vec![]);
        let env = declared("IGNORED", "Asia/Tokyo");
        assert_eq!(found(&env), vec![]);
    }

    /// The entry on the instant of a switch is under it, and a later switch
    /// ends what the earlier one claims.
    #[test]
    fn each_entry_is_read_against_the_zone_at_its_instant() {
        let env = declared("Asia/Tokyo", "Asia/Tokyo");
        env.ok(&["timezone", "Europe/Paris", "-t", T2]);
        env.ok(&["occur", "walk", "-t", T2, "-z", "Asia/Tokyo"]);
        assert_eq!(found(&env), pair("Asia/Tokyo", "Europe/Paris"));
    }

    /// A second run reads what a retroactive switch reaches, though the
    /// entries it reaches did not change, and what a later one takes back.
    #[test]
    fn a_second_run_reads_what_a_change_to_the_field_reaches() {
        let env = declared("Europe/Berlin", "Europe/Berlin");
        env.ok(&["occur", "walk", "-t", T2, "-z", "Europe/Berlin"]);
        assert_eq!(found(&env), vec![]);
        let at = "2026-03-02T00:00:00+00:00";
        env.ok(&["timezone", "Asia/Tokyo", "-t", at]);
        let out = env.ok(&["check", "tz", "--json"]);
        let report: Value = serde_json::from_str(&out).unwrap();
        let issues = report["issues"].as_array().unwrap();
        let off = (issues.iter())
            .find(|i| i["category"] == "tz_off_declared")
            .unwrap_or_else(|| panic!("{report}"));
        assert_eq!(off["count"], 2, "{off}");
        env.ok(&[
            "timezone",
            "Europe/Berlin",
            "-t",
            "2026-03-02T06:00:00+00:00",
        ]);
        assert_eq!(found(&env), vec![]);
    }
}

/// da-uhpql: `_` reads the same clock `timezone X` would: the zone it names.
#[test]
fn the_previous_state_is_at_the_clock_of_the_zone_it_names() {
    let env = Env::new();
    env.ok(&["timezone", "Asia/Tokyo", "-t", "2026-06-01T10:00"]);
    env.ok(&["timezone", "America/Chicago", "-t", "2026-06-02T10:00"]);
    let out = env.ok(&["timezone", "_", "-t", "2026-06-03T10:00", "--show", "json"]);
    assert!(out.contains("timezone:2026-06-03T01:00:00.000Z"), "{out}");
    let err = env.refused(&["timezone", "_", "-z", "Europe/Berlin"]);
    assert!(err.contains("disagrees"), "{err}");
}

/// da-uhpql: a line naming the field by its key is the line naming it by word.
#[test]
fn a_field_named_by_key_is_at_the_clock_of_the_zone_it_names() {
    let env = Env::new();
    env.ok(&[
        "switch",
        "field=timezone",
        "Asia/Tokyo",
        "-t",
        "2026-06-01T14:00",
    ]);
    let at = env.only().occur_time.unwrap();
    assert_eq!(at.utc.to_rfc3339(), "2026-06-01T05:00:00+00:00");
    assert_eq!(at.timezone.as_deref(), Some("Asia/Tokyo"));
    let err = env.refused(&["switch", "field=timezone", "Asia/Seoul", "-z", "Asia/Tokyo"]);
    assert!(err.contains("disagrees"), "{err}");
    assert_eq!(env.rows().len(), 1);
    // A `field=` naming another field is that field's, its `-z` its own.
    env.ok(&["switch", "field=mood", "Asia/Seoul", "-z", "Asia/Tokyo"]);
}

/// da-uhpql: as a log's `-z` is, by entry: `IGNORED` names no zone.
#[test]
fn retime_refuses_a_z_that_disagrees() {
    let env = Env::new();
    env.ok(&["timezone", "Asia/Tokyo", "-t", "2026-06-01T10:00"]);
    let before = env.only().occur_time.unwrap();
    for line in [
        &["retime", "_LAST", "-z", "Europe/Berlin"][..],
        &["retime", "--field", "timezone", "-z", "Europe/Berlin"],
    ] {
        let err = env.refused(line);
        assert!(
            err.contains("Europe/Berlin") && err.contains("Asia/Tokyo"),
            "{err}"
        );
    }
    let after = env.only().occur_time.unwrap();
    assert_eq!((after.utc, &after.timezone), (before.utc, &before.timezone));
    env.ok(&["retime", "_LAST", "-t", "11:00", "-z", "Asia/Tokyo"]);
    let at = env.only().occur_time.unwrap();
    assert_eq!(at.utc.to_rfc3339(), "2026-06-01T02:00:00+00:00");

    env.ok(&["timezone", "IGNORED", "-t", "2026-06-02T10:00"]);
    env.ok(&["retime", "_LAST", "-z", "Europe/Berlin"]);
    // Another field's entries are no concern of it.
    env.ok(&["occur", "tea", "-t", "2026-06-02T11:00"]);
    env.ok(&["retime", "_LAST", "-z", "Europe/Berlin"]);
}

/// da-uhpql: the picker reads it, for a field `end` is refused on.
#[test]
fn fields_says_it_has_no_off() {
    let env = Env::new();
    env.ok(&["timezone", "Asia/Tokyo"]);
    assert_eq!(env.fields_row()["never_false"], json!(true));
    env.refused(&["end", "timezone"]);
    env.ok(&["switch", "mood", "calm"]);
    let row = &serde_json::from_str::<Value>(&env.ok(&["fields", "mood", "--json"])).unwrap()[0];
    assert_eq!(row.get("never_false"), None);
}
