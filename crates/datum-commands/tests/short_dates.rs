//! A date in a table is `Nov 6` in this year, its month in the locale's
//! language, and ISO in another; `--iso-times` prints every time and date
//! cell in full ISO 8601; `--json` keeps ISO (da-9rda8).

use chrono::{DateTime, TimeZone, Utc};
use datum_commands::cmd::dispatch_argv;
use datum_commands::render::dates::IsoTimes;
use datum_commands::render::{colors::strip_ansi, format_table_with, TableArgs, TimeMetricCol};
use datum_core::entry::{Entry, Verb};
use datum_core::test_util::{fresh_db, set_time};
use datum_core::time::DatumTime;
use datum_test_util::{no_color_scope, ScopedEnv};
use serde_json::Value as JsonValue;

fn at(y: i32, mo: u32, d: u32, h: u32, m: u32, s: u32, offset_minutes: i32) -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(y, mo, d, h, m, s).unwrap();
    DatumTime {
        utc,
        offset_minutes,
        timezone: None,
        uncertainty: None,
    }
}

fn day(y: i32, mo: u32, d: u32) -> Entry {
    let mut e = Entry::new(
        "due",
        Verb::Point,
        None,
        None,
        Some(at(y, mo, d, 12, 0, 0, 0)),
        None,
        "t",
    );
    e.set_at_uncertainty(Some("P1D".into()));
    e
}

fn table(entries: &[Entry]) -> String {
    let args = TableArgs {
        metric: TimeMetricCol::Occur,
        ..Default::default()
    };
    let now = at(2026, 11, 3, 9, 0, 0, 0);
    strip_ansi(&format_table_with(entries, None, Some(&now), None, &args).unwrap())
}

#[test]
fn a_day_this_year_is_its_month_and_day_and_another_year_iso() {
    let _c = no_color_scope();
    let (_a, _t, _l) = (
        ScopedEnv::remove("LC_ALL"),
        ScopedEnv::remove("LC_TIME"),
        ScopedEnv::set("LANG", "C.UTF-8"),
    );
    let out = table(&[day(2026, 11, 6), day(2027, 11, 6), day(2026, 10, 3)]);
    assert!(out.contains("Nov 6 "), "{out}");
    assert!(out.contains("2027-11-06"), "{out}");
    assert!(!out.contains("2026-11-06"), "{out}");
    {
        let _de = ScopedEnv::set("LANG", "de_DE.UTF-8");
        assert!(table(&[day(2026, 10, 3)]).contains("Okt 3"));
        let _all = ScopedEnv::set("LC_ALL", "C");
        assert!(table(&[day(2026, 10, 3)]).contains("Oct 3"), "LC_ALL wins");
    }
    let _iso = IsoTimes::set();
    let out = table(&[day(2026, 11, 6)]);
    assert!(out.contains("2026-11-06"), "{out}");
}

/// vividn, 2026-11-03: «--iso-times should then put the time into ISO
/// format for feeding into other programs or spreadsheets».
#[test]
fn iso_times_prints_an_instant_that_parses_back_offset_and_all() {
    let _c = no_color_scope();
    let t = at(2026, 11, 3, 8, 24, 25, 60);
    let e = Entry::new("tea", Verb::Point, None, None, Some(t.clone()), None, "t");
    let plain = table(std::slice::from_ref(&e));
    assert!(!plain.contains("2026-11-03T"), "{plain}");
    let _iso = IsoTimes::set();
    let out = table(&[e]);
    // After the row's `ref`, here its hid (da-kjikt).
    let cell = out
        .lines()
        .nth(1)
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap();
    assert_eq!(cell, "2026-11-03T09:24:25+01:00", "{out}");
    let back = DateTime::parse_from_rfc3339(cell).unwrap();
    assert_eq!(back.with_timezone(&Utc), t.utc);
    assert_eq!(back.offset().local_minus_utc(), 3600);
    assert_eq!(
        out.lines().next().unwrap().split_whitespace().nth(1),
        Some("time"),
        "{out}"
    );
    assert!(!out.lines().next().unwrap().contains("(+1)"), "{out}");
}

fn run(db: &datum_core::db::Db, args: &[&str]) {
    let argv = std::iter::once("datum").chain(args.iter().copied());
    dispatch_argv(argv.map(String::from).collect(), db).unwrap();
}

/// `chorelist`'s Due column, the motivating one: its cell `Nov 6`, the
/// value `--json` carries still the ISO date.
#[test]
fn a_kit_commands_due_date_is_short_and_its_json_value_iso() {
    let _c = no_color_scope();
    let (_a, _t, _l) = (
        ScopedEnv::remove("LC_ALL"),
        ScopedEnv::remove("LC_TIME"),
        ScopedEnv::set("LANG", "en_US.UTF-8"),
    );
    let _g = set_time("2026-11-03T12:00:00Z");
    let db = fresh_db();
    let kit = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kits/chores.toml");
    run(&db, &["kit", "install", "--yes", kit]);
    run(
        &db,
        &["occur", "vacuum", "-z", "UTC", "next_date=2026-11-06"],
    );
    run(
        &db,
        &["occur", "gutters", "-z", "UTC", "next_date=2027-11-06"],
    );
    let json = |extra: &[&str]| -> Vec<JsonValue> {
        let mut argv = vec!["--all".to_string(), "--json".to_string()];
        argv.extend(extra.iter().map(|s| s.to_string()));
        let out = datum_commands::commands::kit_command::output("chorelist", &argv, &db).unwrap();
        serde_json::from_str(&out).unwrap()
    };
    let rows = json(&[]);
    let row = |f: &str| rows.iter().find(|r| r["field"] == f).unwrap().clone();
    let due = |r: &JsonValue| r["row_cells"][2].as_str().unwrap().to_string();
    assert_eq!(row("vacuum")["due"], "2026-11-06");
    assert_eq!(due(&row("vacuum")), "Nov 6");
    assert_eq!(row("gutters")["due"], "2027-11-06");
    assert_eq!(due(&row("gutters")), "2027-11-06");
    let _iso = IsoTimes::set();
    let rows = json(&[]);
    let vacuum = rows.iter().find(|r| r["field"] == "vacuum").unwrap();
    assert_eq!(due(vacuum), "2026-11-06");
    assert!(
        DateTime::parse_from_rfc3339(vacuum["row_cells"][3].as_str().unwrap())
            .is_ok_and(|t| t.timestamp_subsec_nanos() == 0),
        "Last Occur is an instant, not «Nm ago»: {vacuum}"
    );
}
