//! da-n7tp2: `stats` read from its per-day cache answers exactly what it
//! answers computing every day from the entries — for the bare table, a
//! field, a `field:state`, every period flag, slices and `--since/--until`
//! — on thirteen months of history, and still after a backdated write, a
//! retime across midnight, a delete, a spec change and a sync merge, and
//! after a write in another zone, which moves every field's midnights. A
//! `stats` after a write today writes nothing to the file. And `datum check
//! stats` names a cached day that does not agree.

use crate::common;

use std::io::Write;
use std::process::{Command, Stdio};

use chrono::{DateTime, Duration, Utc};
use common::run_cli;
use datum_cli::args::{StatsArgs, StatsPeriod};
use datum_commands::commands::stats::{check_sample, compute_all_fields, compute_stats};
use datum_core::db::{Db, ListParams};
use datum_core::entry::Verb;
use datum_core::test_util::{entry_builder, freeze_time};
use serde_json::json;

const NOW: &str = "2026-06-15T12:00:00Z";

/// A small deterministic generator, so the history is irregular but the
/// same on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

/// Thirteen months of three fields at nanosecond instants: `activity` switches
/// with a block that resumes the state it interrupted every so often,
/// `coffee` dots, and `mood` switches that sometimes read `false` — the
/// bool and the string, whose label is one and whose class is not.
fn seed(db: &Db) {
    let mut rng = Lcg(7);
    let end: DateTime<Utc> = NOW.parse().unwrap();
    let start = end - Duration::days(400);
    let mut step = |t: &mut DateTime<Utc>, hours: u64| {
        *t += Duration::minutes(60 + rng.next(hours * 60) as i64)
            + Duration::nanoseconds(rng.next(1_000_000_000) as i64);
        (*t < end, rng.next(1000))
    };
    let mut t = start;
    loop {
        let (go, r) = step(&mut t, 10);
        if !go {
            break;
        }
        let states = ["coding", "meeting", "reading", "false"];
        if r % 9 == 0 {
            entry_builder("activity")
                .verb(Verb::Block)
                .state(json!("run"))
                .time_start_utc(t - Duration::minutes(40))
                .time_end_utc(t)
                .insert(db);
        } else {
            let state = match states[r as usize % 4] {
                "false" => json!(false),
                s => json!(s),
            };
            let builder = entry_builder("activity").verb(Verb::Switch).state(state);
            builder.time_start_utc(t).insert(db);
        }
    }
    let mut t = start;
    while step(&mut t, 30).0 {
        entry_builder("coffee").time_start_utc(t).insert(db);
    }
    let mut t = start;
    loop {
        let (go, r) = step(&mut t, 72);
        if !go {
            break;
        }
        let state =
            [json!("happy"), json!("sad"), json!(false), json!("false")][r as usize % 4].clone();
        let builder = entry_builder("mood").verb(Verb::Switch).state(state);
        builder.time_start_utc(t).insert(db);
    }
    // One in the future, which no closed day holds.
    entry_builder("activity")
        .verb(Verb::Switch)
        .state(json!("coding"))
        .time_start_utc(end + Duration::days(2))
        .insert(db);
}

/// Every window form, cached or not — or, short of `every`, the windows
/// that hold every write the test makes after the seed.
fn variants(no_cache: bool, every: bool) -> Vec<StatsArgs> {
    let base = |selector: Option<&str>| StatsArgs {
        selector: selector.map(str::to_string),
        no_cache,
        ..StatsArgs::default()
    };
    let mut out = Vec::new();
    for selector in [
        None,
        Some("activity"),
        Some("activity:coding"),
        Some("coffee"),
        Some("mood"),
        Some("mood:false"),
    ] {
        out.push(base(selector));
        out.push(StatsArgs {
            since: Some("400d".into()),
            ..base(selector)
        });
        out.push(StatsArgs {
            include_false: true,
            include_untracked: true,
            ..base(selector)
        });
        if !every {
            continue;
        }
        for slice in ["2025", "2025-08", "2025-08-01"] {
            out.push(StatsArgs {
                slice: Some(slice.into()),
                ..base(selector)
            });
        }
        for period in [
            StatsPeriod::Today,
            StatsPeriod::Week,
            StatsPeriod::LastWeek,
            StatsPeriod::Month,
            StatsPeriod::LastMonth,
        ] {
            out.push(StatsArgs {
                period: Some(period),
                ..base(selector)
            });
        }
        out.push(StatsArgs {
            since: Some("2025-07-01T07:13:00Z".into()),
            until: Some("2025-07-04T13:30:00Z".into()),
            ..base(selector)
        });
        let mut yesterday = base(selector);
        yesterday.time.yesterday = 3;
        out.push(yesterday);
    }
    out
}

fn compute(args: &StatsArgs, db: &Db) -> String {
    match args.selector {
        None => format!("{:?}", compute_all_fields(args, db).unwrap()),
        Some(_) => format!("{:?}", compute_stats(args, db).unwrap()),
    }
}

/// Cached and uncached agree on every variant, and the cache was read.
fn assert_same(db: &Db, after: &str) {
    assert_same_over(db, after, false)
}

fn assert_same_over(db: &Db, after: &str, every: bool) {
    for (cached, uncached) in (variants(false, every).iter()).zip(variants(true, every)) {
        assert_eq!(
            compute(cached, db),
            compute(&uncached, db),
            "after {after}: {cached:?}"
        );
    }
    assert!(
        !db.stats_cache_sample(1).unwrap().is_empty(),
        "after {after}: the cache holds nothing, so nothing was compared"
    );
    assert_eq!(
        check_sample(db, false).unwrap(),
        Vec::<(String, String)>::new()
    );
}

fn hid_near(db: &Db, field: &str, at: &str) -> String {
    let at: DateTime<Utc> = at.parse().unwrap();
    let rows = db
        .list_entries(&ListParams {
            fields: vec![field.into()],
            start_utc: Some(at),
            end_utc: Some(at + Duration::days(3)),
            ..Default::default()
        })
        .unwrap();
    rows[0].hid.to_string()
}

#[test]
fn cached_stats_are_the_uncached_stats_through_every_kind_of_write() {
    let _g = freeze_time(NOW);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.db");
    let db = Db::open(path.to_str().unwrap()).unwrap();
    seed(&db);
    assert_same_over(&db, "the seed", true);

    // Backdated: a switch into a long span, a dot, and a block across a
    // midnight that resumes what it interrupted.
    run_cli(
        &["switch", "activity", "piano", "-t", "2025-11-20 15:00"],
        &db,
    )
    .unwrap();
    run_cli(&["occur", "coffee", "-t", "2025-08-01 08:00"], &db).unwrap();
    entry_builder("activity")
        .verb(Verb::Block)
        .state(json!("run"))
        .time_start_utc("2025-08-01T22:30:00Z".parse().unwrap())
        .time_end_utc("2025-08-02T01:30:00Z".parse().unwrap())
        .insert(&db);
    assert_same(&db, "backdated writes");

    let hid = hid_near(&db, "activity", "2026-03-05T00:00:00Z");
    run_cli(&["retime", &hid, "+1500"], &db).unwrap();
    assert_same(&db, "a retime across midnight");

    let hid = hid_near(&db, "mood", "2026-02-01T00:00:00Z");
    run_cli(&["del", &hid, "--force"], &db).unwrap();
    assert_same(&db, "a delete");

    run_cli(&["spec", "add-state", "activity", "piano"], &db).unwrap();
    assert_same(&db, "a spec change");

    // A sync merge: another copy's backdated switch, applied by the real
    // binary to this file.
    drop(db);
    let other = dir.path().join("other.db");
    let datum = |db: &std::path::Path, home: &str, args: &[&str], stdin: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", db)
            .env("DATUM_HOME", dir.path().join(home))
            .env("DATUM_CONFIG_HOME", dir.path().join(home))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    datum(
        &other,
        "b",
        &["switch", "activity", "chess", "-t", "2026-01-09 20:00"],
        "",
    );
    let vector = datum(&path, "a", &["sync", "pipe", "vector"], "");
    let ops = datum(
        &other,
        "b",
        &["sync", "pipe", "ops", "--since", vector.trim()],
        "",
    );
    let applied = datum(&path, "a", &["sync", "pipe", "apply"], &ops);
    assert!(applied.starts_with("applied "), "{applied}");
    let db = Db::open(path.to_str().unwrap()).unwrap();
    let activity = StatsArgs {
        selector: Some("activity".into()),
        ..StatsArgs::default()
    };
    assert!(
        compute(&activity, &db).contains("chess"),
        "the merge landed"
    );
    assert_same(&db, "a sync merge");
}

#[test]
fn check_names_a_corrupted_cached_day_and_fix_drops_it() {
    let _g = freeze_time(NOW);
    let db = Db::open_in_memory().unwrap();
    for (t, state) in [
        ("2026-06-10T09:00:00Z", "coding"),
        ("2026-06-11T09:00:00Z", "meeting"),
        ("2026-06-12T09:00:00Z", "coding"),
    ] {
        let builder = entry_builder("activity")
            .verb(Verb::Switch)
            .state(json!(state));
        builder.time_start_utc(t.parse().unwrap()).insert(&db);
    }
    let all = |no_cache| StatsArgs {
        no_cache,
        ..StatsArgs::default()
    };
    let truth = compute(&all(true), &db);
    assert_eq!(compute(&all(false), &db), truth);
    assert_eq!(check_sample(&db, false).unwrap(), vec![]);

    db.conn()
        .execute(
            "UPDATE stats_cache SET seconds = seconds + 60
              WHERE field = 'activity' AND day = '2026-06-11' AND state = 'meeting'",
            [],
        )
        .unwrap();
    let corrupt = vec![("activity".to_string(), "2026-06-11".to_string())];
    assert_ne!(compute(&all(false), &db), truth, "the corruption is read");
    assert_eq!(check_sample(&db, false).unwrap(), corrupt);
    assert_eq!(check_sample(&db, true).unwrap(), corrupt);
    assert_eq!(
        compute(&all(false), &db),
        truth,
        "--fix dropped it, stats refilled it"
    );
    assert_eq!(check_sample(&db, false).unwrap(), vec![]);
}

/// datum.day hears any write to the file as a change and re-runs every pane
/// in every tab, so a `stats` after a write today must not write: the day it
/// changed is not cached. A backdated one is refilled, once.
#[test]
fn a_stats_after_a_write_today_writes_nothing() {
    let _g = freeze_time(NOW);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.db");
    let db = Db::open(path.to_str().unwrap()).unwrap();
    // Through the CLI, so every row is in the host's zone, as a real
    // history is: a row in another zone is a zone change (see above).
    for day in 10..15 {
        let at = format!("2026-06-{day} 09:00");
        run_cli(&["switch", "activity", "coding", "-t", &at], &db).unwrap();
        run_cli(&["occur", "coffee", "-t", &at], &db).unwrap();
    }
    let all = |no_cache| StatsArgs {
        no_cache,
        ..StatsArgs::default()
    };
    compute(&all(false), &db);
    let watch = rusqlite::Connection::open(&path).unwrap();
    let version = || -> i64 {
        watch
            .query_row("PRAGMA data_version", [], |r| r.get(0))
            .unwrap()
    };

    run_cli(&["occur", "coffee"], &db).unwrap();
    run_cli(&["switch", "activity", "reading", "-t", "-90"], &db).unwrap();
    let before = version();
    let cached = compute(&all(false), &db);
    assert_eq!(
        version(),
        before,
        "a stats after a write today wrote to the file"
    );
    assert_eq!(cached, compute(&all(true), &db));

    run_cli(&["occur", "coffee", "-t", "2026-06-12 15:00"], &db).unwrap();
    let before = version();
    let cached = compute(&all(false), &db);
    assert_ne!(version(), before, "a backdated write refills its days");
    assert_eq!(cached, compute(&all(true), &db));
    let before = version();
    compute(&all(false), &db);
    assert_eq!(version(), before, "and only once");
}

/// A row in another zone moves the midnights after it in every field, not
/// only its own: `ZoneTimeline::resolve_local` reads the newest zone stretch
/// first, so Tokyo's midnight (15:00 UTC) is the day's edge when no row in
/// UTC falls between the Tokyo row and that midnight's UTC reading.
#[test]
fn a_write_in_another_zone_moves_every_fields_midnights() {
    let _g = freeze_time(NOW);
    let db = Db::open_in_memory().unwrap();
    for day in 1..10 {
        for (time, state) in [("06:00", "coding"), ("12:00", "false")] {
            let at = format!("2026-06-0{day} {time}");
            let args = ["switch", "activity", state, "-z", "UTC", "-t", &at];
            run_cli(&args, &db).unwrap();
        }
    }
    let day = |slice: &str, no_cache| StatsArgs {
        selector: Some("activity".into()),
        slice: Some(slice.into()),
        no_cache,
        ..StatsArgs::default()
    };
    let total = |args: &StatsArgs| compute_stats(args, &db).unwrap().total_duration_seconds;
    assert_eq!(total(&day("2026-06-04", false)), 86_400);

    let tokyo = [
        "occur",
        "water",
        "-z",
        "Asia/Tokyo",
        "-t",
        "2026-06-04 23:50",
    ];
    run_cli(&tokyo, &db).unwrap();
    assert_eq!(
        total(&day("2026-06-04", true)),
        15 * 3600,
        "the fixture moves the 5th's midnight to 15:00 UTC on the 4th"
    );
    for slice in ["2026-06-04", "2026-06-05", "2026-06"] {
        let (cached, uncached) = (day(slice, false), day(slice, true));
        assert_eq!(compute(&cached, &db), compute(&uncached, &db), "{slice}");
    }
}

/// A window caches back only as far as it starts, so `stats --month` costs
/// the month and not the history (da-cwpr1); a later all-time read extends
/// the range to the field's first day.
#[test]
fn a_window_caches_its_own_days_and_all_time_extends_them() {
    let _g = freeze_time(NOW);
    let db = Db::open_in_memory().unwrap();
    seed(&db);
    let table = |period, no_cache| StatsArgs {
        period,
        no_cache,
        ..StatsArgs::default()
    };
    let month = Some(StatsPeriod::Month);
    assert_eq!(
        compute(&table(month, false), &db),
        compute(&table(month, true), &db)
    );
    let (lo, _) = db.stats_cache_through("activity").unwrap().unwrap();
    assert_eq!(lo, "2026-06-01", "the month cached from its first day");

    assert_eq!(
        compute(&table(None, false), &db),
        compute(&table(None, true), &db)
    );
    assert_eq!(db.stats_cache_through("activity").unwrap().unwrap().0, "");
    assert_eq!(
        compute(&table(month, false), &db),
        compute(&table(month, true), &db)
    );
}

/// Nor does the first `stats` a database ever runs, on a day's entries: the
/// cache's stamp, and the zone the first entry brings, go in with the first
/// days it caches, not with a read that caches none (`refresh-runs.test.mjs`).
#[test]
fn the_first_stats_writes_nothing_until_it_caches_a_day() {
    let _g = freeze_time(NOW);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.db");
    let db = Db::open(path.to_str().unwrap()).unwrap();
    run_cli(&["occur", "coffee"], &db).unwrap();
    run_cli(&["switch", "mood", "calm", "-t", "-30"], &db).unwrap();
    let watch = rusqlite::Connection::open(&path).unwrap();
    let version = || -> i64 {
        watch
            .query_row("PRAGMA data_version", [], |r| r.get(0))
            .unwrap()
    };
    let before = version();
    for selector in [None, Some("mood")] {
        let args = StatsArgs {
            selector: selector.map(str::to_string),
            ..StatsArgs::default()
        };
        compute(&args, &db);
    }
    assert_eq!(
        version(),
        before,
        "a read with no closed day to cache wrote"
    );
}
