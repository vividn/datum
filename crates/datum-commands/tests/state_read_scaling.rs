//! da-vmjh: the size curve. vividn's ask was "almost all data queries run
//! in normal operation will be operating on the last 3 days, so it would
//! be good to keep that data fresh and quickly available" — and the
//! corollary, "why would more data make this slower".
//!
//! So this measures the same read on two stores that differ ONLY in how
//! much history sits behind the window, and reports the ratio. Flat is
//! the pass. A tiny store proves nothing (trap 2), which is why the
//! small size is already 2k rows.
//!
//! `#[ignore]` because it is a stopwatch, not an assertion CI should
//! gate on — a loaded runner makes wall-clock lie. The durable gates are
//! the query-plan tests next to the queries themselves
//! (`datum-core`'s `state_reads_stop_at_their_limit`,
//! `distinct_fields_seeks_and_never_scans_entries`,
//! `the_render_window_read_is_index_bounded`). Run it with:
//!
//! ```text
//! cargo test --release -p datum-commands --test state_read_scaling -- --ignored --nocapture
//! ```

use chrono::{DateTime, Duration, TimeZone, Utc};
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;

/// The instant every read below is anchored to. Fixed so the two stores
/// are asked exactly the same question.
fn anchor() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 14, 12, 0, 0).unwrap()
}

fn datum_time(utc: DateTime<Utc>) -> DatumTime {
    DatumTime {
        utc,
        offset_minutes: 0,
        timezone: Some("UTC".into()),
        uncertainty: None,
    }
}

/// `n` rows spread over `fields` fields at one row per 10 minutes,
/// walking backwards from `anchor()`. Same density at both sizes, so the
/// last-few-days window holds the same number of rows either way and the
/// ONLY difference between the stores is the length of the history
/// behind it.
fn store(n: usize, fields: usize) -> Db {
    let db = Db::open_in_memory().unwrap();
    let base = anchor();
    for i in 1..=n {
        let field = format!("f{}", i % fields);
        let t = base - Duration::minutes(10 * i as i64);
        // Alternate a real switch with a bare occur so both the state
        // timeline and the point renderer have something to chew on.
        let (verb, state) = if i % 3 == 2 {
            (Verb::Point, serde_json::json!(true))
        } else if i % 2 == 0 {
            (Verb::Switch, serde_json::json!("on"))
        } else {
            (Verb::Switch, serde_json::json!("off"))
        };
        let dur = (verb == Verb::Point).then(datum_core::dur::Dur::moment);
        db.insert_entry(&Entry::new(
            &field,
            verb,
            Some(state),
            None,
            Some(datum_time(t)),
            dur,
            "bench",
        ))
        .unwrap();
    }
    db
}

/// Median wall-clock of `runs` calls, in milliseconds.
fn median_ms(runs: usize, mut f: impl FnMut()) -> f64 {
    let mut samples: Vec<f64> = (0..runs)
        .map(|_| {
            let t = std::time::Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    samples[runs / 2]
}

#[test]
#[ignore = "stopwatch, not a gate — see the module docs"]
fn the_cost_of_a_normal_read_does_not_grow_with_the_history() {
    const FIELDS: usize = 26;
    let small = store(2_000, FIELDS);
    let large = store(50_000, FIELDS);
    let at = anchor();

    // The commands a click in the browser makes, driven at their real
    // entry points and told to write their SVG to a throwaway file so
    // the measurement covers the whole read path, not a chosen slice of
    // it. Plus the two store-level reads the bead named directly.
    let out = tempfile::tempdir().unwrap();
    let svg = out.path().join("o.svg").to_string_lossy().into_owned();
    let at_str = "2026-09-14T12:00:00Z".to_string();

    let nowview_args = |history: &str| datum_commands::args::NowviewArgs {
        history: Some(history.to_string()),
        time: datum_parse::input::TimeArgs {
            time: Some(at_str.clone()),
            ..Default::default()
        },
        width: Some(400.0),
        height: 300.0,
        output_file: Some(svg.clone()),
        ..Default::default()
    };

    #[allow(clippy::type_complexity)]
    let reads: Vec<(&str, Box<dyn Fn(&Db)>)> = vec![
        (
            "nowview --history 30m",
            Box::new(|db: &Db| {
                datum_commands::commands::nowview::nowview(nowview_args("30m"), db).unwrap();
            }),
        ),
        (
            "nowview --history 0",
            Box::new(|db: &Db| {
                datum_commands::commands::nowview::nowview(nowview_args("0"), db).unwrap();
            }),
        ),
        (
            "dayview",
            Box::new(|db: &Db| {
                datum_commands::commands::dayview::dayview(
                    datum_commands::args::DayviewArgs {
                        end_date: Some("2026-09-14".to_string()),
                        width: Some(400.0),
                        height: Some(300.0),
                        output_file: Some(svg.clone()),
                        ..Default::default()
                    },
                    db,
                )
                .unwrap();
            }),
        ),
        (
            "now snapshot (--json's read)",
            Box::new(|db: &Db| {
                db.current_now(&datum_core::db::NowFilter {
                    include_tracked_false: true,
                    include_ignored: true,
                    snapshot_time: Some(at),
                })
                .unwrap();
            }),
        ),
        (
            "batch_state_at (all fields)",
            Box::new(|db: &Db| {
                let names = db.distinct_fields().unwrap();
                let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                db.batch_state_at(&refs, at).unwrap();
            }),
        ),
        (
            "distinct_fields",
            Box::new(|db: &Db| {
                db.distinct_fields().unwrap();
            }),
        ),
    ];

    println!("\n{:<30} {:>10} {:>10} {:>8}", "read", "2k", "50k", "ratio");
    let mut worst: (f64, &str) = (0.0, "");
    for (label, f) in &reads {
        let s = median_ms(7, || f(&small));
        let l = median_ms(7, || f(&large));
        let ratio = l / s.max(0.001);
        println!("{label:<30} {s:>9.2}ms {l:>9.2}ms {ratio:>7.1}x");
        if ratio > worst.0 {
            worst = (ratio, label);
        }
    }
    println!(
        "\nworst: {} at {:.1}x — 25x more history behind the same window\n",
        worst.1, worst.0
    );
}
