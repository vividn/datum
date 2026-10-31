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

/// da-h8k9q: the entrances that move a logged entry hold it to the same rule.
mod moved {
    use super::*;

    const LOGGED: &[&str] = &["timezone", "Asia/Tokyo", "-t", "2026-06-02T10:00+02:00"];

    /// The entry at 10:00+02:00 on 2026-06-03, in Tokyo as 17:00.
    fn assert_in_tokyo(entry: &Entry) {
        let at = entry.occur_time.as_ref().unwrap();
        assert_eq!(at.utc.to_rfc3339(), "2026-06-03T08:00:00+00:00");
        assert_eq!(at.timezone.as_deref(), Some("Asia/Tokyo"));
        assert_eq!(at.offset_minutes, 9 * 60);
    }

    #[test]
    fn an_offset_on_retime_t() {
        let env = Env::new();
        env.ok(LOGGED);
        let hid = env.only().hid;
        env.ok(&["retime", hid.as_str(), "-t", "2026-06-03T10:00+02:00"]);
        assert_in_tokyo(&env.only());
    }

    #[test]
    fn an_offset_on_update_occur_time() {
        let env = Env::new();
        env.ok(LOGGED);
        let hid = env.only().hid;
        env.ok(&["update", hid.as_str(), "occur_time=2026-06-03T10:00+02:00"]);
        assert_in_tokyo(&env.only());
    }

    /// Its instant in another's spelling changes nothing, and writes nothing.
    #[test]
    fn the_same_instant_on_update_is_no_change() {
        let env = Env::new();
        env.ok(LOGGED);
        let hid = env.only().hid;
        let out = env.ok(&["update", hid.as_str(), "occur_time=2026-06-02T10:00+02:00"]);
        assert!(out.contains("NODIFF"), "{out}");
    }

    fn edit(env: &Env, hid: &str, sed: &str) -> Output {
        let script = env.0.path().join("editor.sh");
        std::fs::write(&script, format!("sed -i -E '{sed}' \"$1\"\n")).unwrap();
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(["--db", env.path().to_str().unwrap(), "edit", hid])
            .env("TZ", "Europe/Berlin")
            .env("HOME", env.0.path())
            .env("EDITOR", format!("sh {}", script.display()))
            .output()
            .unwrap()
    }

    #[test]
    fn a_time_and_a_state_on_edit() {
        let env = Env::new();
        env.ok(LOGGED);
        let hid = env.only().hid;
        let moved = edit(
            &env,
            hid.as_str(),
            r#"s/"utc": .*/"utc": "2026-06-03T08:00:00Z",/; s/"offset_minutes": .*/"offset_minutes": 120,/; s/"timezone": .*/"timezone": "Europe\/Berlin"/"#,
        );
        assert!(moved.status.success(), "{moved:?}");
        assert_in_tokyo(&env.only());
        let bad = edit(&env, hid.as_str(), r#"s/"state": .*/"state": "Tokyo",/"#);
        assert!(!bad.status.success(), "{bad:?}");
        assert_in_tokyo(&env.only());
    }

    /// `IGNORED` names no zone to go to, and is no reason to refuse a new time.
    #[test]
    fn a_row_naming_no_zone_keeps_the_offset_it_is_given() {
        let env = Env::new();
        env.ok(&["timezone", "IGNORED", "-t", "2026-06-02T10:00+02:00"]);
        let hid = env.only().hid;
        env.ok(&["retime", hid.as_str(), "-t", "2026-06-03T10:00+05:00"]);
        assert_eq!(
            env.only().occur_time.unwrap().offset_minutes,
            5 * 60,
            "IGNORED names no zone"
        );
    }
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
    env.ok(&["update", "syn0001", "occur_time=2025-01-02T10:00+02:00"]);
    assert_eq!(env.only().occur_time.unwrap().offset_minutes, 2 * 60);
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

/// An entry follows the field at its instant, and a device in another zone
/// asks to move it (da-mvt5i).
mod follow {
    use super::*;
    use rusqlite::Connection;
    use std::io::Write;
    use std::process::Stdio;

    const BERLIN: &str = "Europe/Berlin";
    const PARIS: &str = "Europe/Paris";
    const LISBON: &str = "Europe/Lisbon";

    /// The line as datum.day runs it, `answers` its stdin.
    fn page(env: &Env, tz: &str, args: &[&str], answers: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(["--db", env.path().to_str().unwrap()])
            .args(args)
            .env("TZ", tz)
            .env("HOME", env.0.path())
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .env("DATUM_NO_DEFAULT_ANSWER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(answers.as_bytes()).unwrap();
        drop(stdin);
        child.wait_with_output().unwrap()
    }

    fn asked(out: &Output) -> bool {
        let said = String::from_utf8_lossy(&out.stderr);
        out.status.code() == Some(12) && said.contains("DATUM_ASK=")
    }

    fn sql(env: &Env, query: &str) -> Vec<(String, String)> {
        let conn = Connection::open(env.path()).unwrap();
        let mut stmt = conn.prepare(query).unwrap();
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)));
        let rows = rows.unwrap().collect::<Result<_, _>>();
        rows.unwrap()
    }

    /// Each `coffee`'s time and zone.
    fn coffees(env: &Env) -> Vec<(String, String)> {
        sql(
            env,
            "SELECT occur_time, IFNULL(occur_time_tz, '') FROM entries WHERE field = 'coffee' ORDER BY rowid",
        )
    }

    fn zones_logged(env: &Env) -> Vec<String> {
        let rows = sql(
            env,
            "SELECT json_extract(state, '$'), '' FROM entries WHERE field = 'timezone' ORDER BY rowid",
        );
        rows.into_iter().map(|r| r.0).collect()
    }

    fn holding(zone: &str) -> Env {
        let env = Env::new();
        env.ok(&["timezone", zone, "-t", "-100"]);
        env
    }

    /// Berlin until 08:00 UTC on the 10th, Lisbon until 17:00 UTC (18:00
    /// Berlin) on the 12th, Berlin again; March, so an hour between them.
    fn trip() -> Env {
        let env = Env::new();
        for (zone, at) in [
            (BERLIN, "2026-03-01T00:00:00+00:00"),
            (LISBON, "2026-03-10T08:00:00+00:00"),
            (BERLIN, "2026-03-12T17:00:00+00:00"),
        ] {
            env.ok(&["timezone", zone, "-t", at]);
        }
        env
    }

    /// `-d day -t clock` on a host in Berlin: where it lands, and in what.
    fn placed(env: &Env, day: &str, clock: &str) -> (String, String) {
        env.ok(&["occur", "coffee", "-d", day, "-t", clock]);
        coffees(env).pop().unwrap()
    }

    fn assert_instant(got: &(String, String), utc: &str, zone: &str) {
        let at: chrono::DateTime<chrono::Utc> = got.0.parse().unwrap();
        let want: chrono::DateTime<chrono::Utc> = utc.parse().unwrap();
        assert_eq!((at, got.1.as_str()), (want, zone));
    }

    #[test]
    fn a_clock_inside_a_trip_is_the_trips_and_past_the_switch_is_home() {
        let env = trip();
        let inside = placed(&env, "2026-03-11", "8:00");
        assert_instant(&inside, "2026-03-11T08:00:00Z", LISBON);
        let past = placed(&env, "2026-03-13", "8:00");
        assert_instant(&past, "2026-03-13T07:00:00Z", BERLIN);
        let before = placed(&env, "2026-03-05", "8:00");
        assert_instant(&before, "2026-03-05T07:00:00Z", BERLIN);
    }

    /// The instant the trip begins, by its own clock, is the trip: read by
    /// Berlin's, 8:00 is an hour before it, and that used to be its reading.
    #[test]
    fn at_a_trips_first_instant_its_clock_wins() {
        let env = trip();
        let first = placed(&env, "2026-03-10", "8:00");
        assert_instant(&first, "2026-03-10T08:00:00Z", LISBON);
        // The hour the move west repeats goes to the stretch being begun.
        let repeated = placed(&env, "2026-03-10", "8:30");
        assert_instant(&repeated, "2026-03-10T08:30:00Z", LISBON);
        // Before it, only Berlin's clock reads it.
        let before = placed(&env, "2026-03-10", "7:30");
        assert_instant(&before, "2026-03-10T06:30:00Z", BERLIN);
    }

    #[test]
    fn at_a_trips_last_edge_each_side_reads_its_own_clock() {
        let env = trip();
        let last = placed(&env, "2026-03-12", "16:30");
        assert_instant(&last, "2026-03-12T16:30:00Z", LISBON);
        let home = placed(&env, "2026-03-12", "18:00");
        assert_instant(&home, "2026-03-12T17:00:00Z", BERLIN);
        // The hour the move east skipped: the clock typed, `check tz` naming it.
        let skipped = placed(&env, "2026-03-12", "17:30");
        assert_instant(&skipped, "2026-03-12T17:30:00Z", LISBON);
        let out = env.ok(&["check", "tz", "--json"]);
        assert!(out.contains("tz_off_declared"), "{out}");
    }

    #[test]
    fn a_relative_time_lands_in_the_stretch_it_names() {
        let env = Env::new();
        env.ok(&["timezone", LISBON, "-t", "-3000"]);
        env.ok(&["timezone", BERLIN, "-t", "-1000"]);
        env.ok(&["occur", "coffee", "-t", "-2000"]);
        assert_eq!(coffees(&env).pop().unwrap().1, LISBON);
    }

    /// A trip two days back, `-yy` read in the trip's own zone.
    #[test]
    fn yy_is_the_trips_clock() {
        let env = Env::new();
        env.ok(&["timezone", LISBON, "-t", "-5000"]);
        env.ok(&["timezone", BERLIN, "-t", "-1000"]);
        env.ok(&["occur", "coffee", "-yy", "-t", "8"]);
        let db = Db::open(env.path().to_str().unwrap()).unwrap();
        let (hid, _) = sql(&env, "SELECT hid, '' FROM entries WHERE field = 'coffee'").remove(0);
        let at = db.get_entry(&hid).unwrap().occur_time.unwrap();
        assert_eq!(at.timezone.as_deref(), Some(LISBON));
        let clock = at.utc + chrono::TimeDelta::minutes(at.offset_minutes.into());
        assert_eq!(clock.format("%H:%M").to_string(), "08:00", "{at:?}");
    }

    #[test]
    fn z_and_an_offset_win_over_the_field() {
        let env = trip();
        env.ok(&[
            "occur",
            "coffee",
            "-d",
            "2026-03-11",
            "-t",
            "8",
            "-z",
            "Asia/Tokyo",
        ]);
        assert_eq!(coffees(&env).pop().unwrap().1, "Asia/Tokyo");
        env.ok(&["occur", "coffee", "-t", "2026-03-11T09:00:00+09:00"]);
        assert_ne!(coffees(&env).pop().unwrap().1, LISBON);
    }

    /// Where the field claims nothing, the entries around it still decide.
    #[test]
    fn under_ignored_the_neighbours_decide() {
        let env = Env::new();
        for at in [
            "2026-03-01T08:00:00+00:00",
            "2026-03-01T09:00:00+00:00",
            "2026-03-01T10:00:00+00:00",
            "2026-03-01T12:00:00+00:00",
            "2026-03-01T22:00:00+00:00",
        ] {
            env.ok(&["occur", "walk", "-t", at, "-z", "Asia/Tokyo"]);
        }
        env.ok(&["timezone", "IGNORED", "-t", "2026-02-01T00:00:00+00:00"]);
        env.ok(&["occur", "coffee", "-d", "2026-03-01", "-t", "20:30"]);
        assert_eq!(coffees(&env).pop().unwrap().1, "Asia/Tokyo");
    }

    /// The key is one more `-t` (da-87wvg), so it is read as `-t` is.
    fn keyed(env: &Env, line: &[&str]) -> (String, String) {
        env.ok(&[&["occur", "coffee"], line].concat());
        coffees(env).pop().unwrap()
    }

    #[test]
    fn an_occur_time_key_is_in_the_zone_held_at_its_instant() {
        let env = trip();
        let inside = keyed(&env, &["occur_time=2026-03-11T08:00"]);
        assert_instant(&inside, "2026-03-11T08:00:00Z", LISBON);
        let first = keyed(&env, &["occur_time=2026-03-10T08:00"]);
        assert_instant(&first, "2026-03-10T08:00:00Z", LISBON);
        let before = keyed(&env, &["occur_time=2026-03-10T07:30"]);
        assert_instant(&before, "2026-03-10T06:30:00Z", BERLIN);
        let past = keyed(&env, &["occur_time=2026-03-13T08:00"]);
        assert_instant(&past, "2026-03-13T07:00:00Z", BERLIN);
    }

    #[test]
    fn an_occur_time_key_counts_from_the_flags_and_z_and_its_offset_win() {
        let env = trip();
        let relative = keyed(&env, &["-d", "2026-03-11", "-t", "8", "occur_time=-30"]);
        assert_instant(&relative, "2026-03-11T07:30:00Z", LISBON);
        let named = keyed(&env, &["-z", "Asia/Tokyo", "occur_time=2026-03-11T09:00"]);
        assert_instant(&named, "2026-03-11T00:00:00Z", "Asia/Tokyo");
        let offset = keyed(&env, &["occur_time=2026-03-11T10:00:00+09:00"]);
        assert_instant(&offset, "2026-03-11T01:00:00Z", "");
    }

    /// `-D` makes the key's whole day, midnight by the trip's clock.
    #[test]
    fn a_whole_day_key_is_that_stretchs_midnight() {
        let env = trip();
        let day = keyed(&env, &["-D", "occur_time=2026-03-11T08:00"]);
        assert_instant(&day, "2026-03-11T00:00:00Z", LISBON);
    }

    /// The field claims nothing at the key's instant, whatever it holds now.
    #[test]
    fn before_the_first_stretch_a_key_takes_nothing_from_the_field() {
        let env = trip();
        let early = keyed(&env, &["occur_time=2026-01-11T08:00"]);
        assert_instant(&early, "2026-01-11T07:00:00Z", BERLIN);
        let env = Env::new();
        env.ok(&["timezone", LISBON, "-t", "-100"]);
        let early = keyed(&env, &["occur_time=2026-01-11T08:00"]);
        assert_instant(&early, "2026-01-11T07:00:00Z", BERLIN);
    }

    /// Only an entry at about now asks: the key's time decides that, as `-t`'s.
    #[test]
    fn a_key_in_the_past_does_not_ask_and_one_at_now_does() {
        let env = holding(BERLIN);
        let past = page(
            &env,
            PARIS,
            &["occur", "coffee", "occur_time=2026-01-11T08:00"],
            "",
        );
        assert!(past.status.success(), "{past:?}");
        assert_eq!(zones_logged(&env), [BERLIN]);
        assert!(asked(&page(
            &env,
            PARIS,
            &["occur", "coffee", "occur_time=-1"],
            ""
        )));
    }

    #[test]
    fn a_device_in_another_zone_is_asked_and_yes_moves_the_field_and_the_entry() {
        let env = holding(BERLIN);
        let unanswered = page(&env, PARIS, &["occur", "coffee"], "");
        assert!(asked(&unanswered), "{unanswered:?}");
        assert!(coffees(&env).is_empty(), "asked before anything is written");
        assert_eq!(zones_logged(&env), [BERLIN]);

        let yes = page(&env, PARIS, &["occur", "coffee"], "y\n");
        assert!(yes.status.success(), "{yes:?}");
        assert_eq!(coffees(&env).pop().unwrap().1, PARIS);
        assert_eq!(zones_logged(&env), [BERLIN, PARIS]);
        let raw = sql(
            &env,
            "SELECT field, raw_input FROM entries WHERE raw_input != '' ORDER BY rowid",
        );
        assert!(raw[1].1.starts_with("timezone Europe/Paris -t "), "{raw:?}");
        assert!(raw[2].1.ends_with("occur coffee"), "{raw:?}");

        let again = page(&env, PARIS, &["occur", "tea"], "");
        assert!(again.status.success(), "the two agree now: {again:?}");
    }

    #[test]
    fn no_keeps_the_stored_zone_and_is_not_asked_again_until_something_changes() {
        let env = holding(BERLIN);
        let no = page(&env, PARIS, &["occur", "coffee"], "n\n");
        assert!(no.status.success(), "{no:?}");
        assert_eq!(coffees(&env).pop().unwrap().1, BERLIN);
        assert_eq!(zones_logged(&env), [BERLIN]);

        let quiet = page(&env, PARIS, &["occur", "tea"], "");
        assert!(quiet.status.success(), "the same pair: {quiet:?}");
        assert!(asked(&page(
            &env,
            "Asia/Tokyo",
            &["occur", "tea", "-t", "-1"],
            ""
        )));

        // Home, and the same trip again, is asked again.
        assert!(page(&env, BERLIN, &["occur", "tea", "-t", "-2"], "")
            .status
            .success());
        assert!(asked(&page(&env, PARIS, &["occur", "tea", "-t", "-3"], "")));
    }

    /// The very zone named two ways, whichever the field and the host use.
    #[test]
    fn a_tzdb_spelling_is_not_another_zone() {
        for (stored, device) in [
            ("Asia/Kolkata", "Asia/Calcutta"),
            ("Asia/Calcutta", "Asia/Kolkata"),
        ] {
            let env = holding(stored);
            let out = page(&env, device, &["occur", "coffee"], "");
            assert!(out.status.success(), "{out:?}");
            assert_eq!(coffees(&env).pop().unwrap().1, stored);
        }
    }

    #[test]
    fn auto_logs_the_switch_with_no_question_and_says_so() {
        let env = holding(BERLIN);
        env.ok(&["db", "config", "timezone_follow", "auto"]);
        assert_eq!(
            env.ok(&["db", "config", "timezone_follow"]),
            "timezone_follow auto\n"
        );
        let out = page(&env, PARIS, &["occur", "coffee"], "");
        assert!(out.status.success(), "{out:?}");
        assert_eq!(coffees(&env).pop().unwrap().1, PARIS);
        assert_eq!(zones_logged(&env), [BERLIN, PARIS]);
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(said.contains("timezone") && said.contains(PARIS), "{said}");
    }

    #[test]
    fn nothing_is_asked_where_the_field_claims_nothing() {
        let none = Env::new();
        let out = page(&none, PARIS, &["occur", "coffee"], "");
        assert!(out.status.success(), "{out:?}");
        assert_eq!(coffees(&none).pop().unwrap().1, PARIS);

        let ignored = holding("IGNORED");
        let out = page(&ignored, PARIS, &["occur", "coffee"], "");
        assert!(out.status.success(), "{out:?}");
        assert_eq!(coffees(&ignored).pop().unwrap().1, PARIS);
        assert_eq!(zones_logged(&ignored), ["IGNORED"]);
    }

    /// Only at about now, on a write, and never in place of `-z`, the field's
    /// own line, or a line with sides writing together.
    #[test]
    fn it_asks_only_for_a_write_at_about_now() {
        let env = holding(BERLIN);
        for line in [
            &["occur", "coffee", "-y"][..],
            &["occur", "coffee", "-t", "-60"],
            &["occur", "coffee", "-z", "Asia/Tokyo"],
            &["add", "coffee", "-T", "a=1"],
            &["tail"],
            &["timezone", "Asia/Tokyo", "-t", "-1"],
            &["timezone", "IGNORED"],
        ] {
            let out = page(&env, PARIS, line, "");
            assert!(out.status.success(), "{line:?}: {out:?}");
        }
        let env = holding(BERLIN);
        assert!(asked(&page(&env, PARIS, &["occur", "coffee", "-q"], "")));
        assert!(page(&env, PARIS, &["occur", "a", ",", "occur", "b"], "")
            .status
            .success());
        assert_eq!(zones_logged(&env), [BERLIN]);
    }

    /// No person there: stdin is the script's own, so nothing reads it, says
    /// a prompt, or moves the field, `auto` or not.
    #[test]
    fn a_pipe_is_no_person() {
        let env = holding(BERLIN);
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(["--db", env.path().to_str().unwrap(), "occur", "coffee"])
            .env("TZ", PARIS)
            .env("HOME", env.0.path())
            .env_remove("DATUM_NO_DEFAULT_ANSWER")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let held_open = child.stdin.take();
        let started = std::time::Instant::now();
        let out = child.wait_with_output().unwrap();
        drop(held_open);
        assert!(out.status.success(), "{out:?}");
        assert!(
            started.elapsed().as_secs() < 4,
            "it waited on a silent pipe"
        );
        assert_eq!(String::from_utf8_lossy(&out.stderr), "");
        assert_eq!(coffees(&env).pop().unwrap().1, BERLIN);

        env.ok(&["db", "config", "timezone_follow", "auto"]);
        assert!(env.run_in(PARIS, &["occur", "tea"]).status.success());
        assert_eq!(zones_logged(&env), [BERLIN]);
    }

    /// A terminal's answer; and a log at no mismatch runs on the database with
    /// no snapshot, spool or `_meta` row.
    #[cfg(unix)]
    #[test]
    fn a_terminal_is_asked_and_its_answer_taken() {
        use crate::ambiguous_time::pty;
        use std::io::Read;

        let env = holding(BERLIN);
        let (mut master, slave) = pty();
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(["--db", env.path().to_str().unwrap(), "occur", "coffee"])
            .env("TZ", PARIS)
            .env("HOME", env.0.path())
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .env_remove("DATUM_NO_DEFAULT_ANSWER")
            .stdin(slave)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (mut heard, mut buf) = (String::new(), [0u8; 256]);
        while !heard.contains("[y/N]") {
            let n = stderr.read(&mut buf).unwrap();
            assert!(n > 0, "never asked: {heard}");
            heard.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        master.write_all(b"y\n").unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(coffees(&env).pop().unwrap().1, PARIS);
        assert_eq!(zones_logged(&env), [BERLIN, PARIS]);
    }

    #[test]
    fn a_log_with_no_mismatch_leaves_nothing_behind() {
        let env = holding(BERLIN);
        assert!(page(&env, BERLIN, &["occur", "coffee"], "")
            .status
            .success());
        assert!(!env.path().with_extension("db.spool").exists());
        let held = sql(
            &env,
            "SELECT key, value FROM _meta WHERE key = 'zone_declined'",
        );
        assert!(held.is_empty(), "{held:?}");
    }

    /// `follow-zone`, what the page runs as it loads: the same question.
    #[test]
    fn the_pages_load_asks_the_same_question() {
        let env = holding(BERLIN);
        let line = ["follow-zone"];
        assert!(asked(&page(&env, PARIS, &line, "")));
        assert!(page(&env, PARIS, &line, "n\n").status.success());
        assert!(page(&env, PARIS, &line, "").status.success(), "once only");
        assert_eq!(zones_logged(&env), [BERLIN]);
        assert!(asked(&page(&env, "Asia/Tokyo", &line, "")));
        let yes = page(&env, "Asia/Tokyo", &line, "y\n");
        assert!(yes.status.success(), "{yes:?}");
        assert_eq!(zones_logged(&env), [BERLIN, "Asia/Tokyo"]);

        env.ok(&["db", "config", "timezone_follow", "auto"]);
        assert!(page(&env, PARIS, &line, "").status.success());
        assert_eq!(zones_logged(&env), [BERLIN, "Asia/Tokyo", PARIS]);

        let none = Env::new();
        assert!(page(&none, PARIS, &line, "").status.success());
        assert!(zones_logged(&none).is_empty());
    }

    #[test]
    fn timezone_follow_takes_ask_or_auto() {
        let env = Env::new();
        assert_eq!(
            env.ok(&["db", "config", "timezone_follow"]),
            "timezone_follow ask (default)\n"
        );
        let refused = env.refused(&["db", "config", "timezone_follow", "off"]);
        assert!(refused.contains("IGNORED"), "{refused}");
        env.refused(&["db", "config", "auto_setup", "auto"]);
    }
}
