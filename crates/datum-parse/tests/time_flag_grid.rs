//! The time flags by both ways in, over one grid of inputs (da-m8pjs):
//! every combination of `-y`, `-q`, `-d`, `-t`, `-z`, `-D` and `-T`, at
//! reference times that include both DST nights in Europe/Berlin and a +14
//! and a −11 zone. `TimeArgs::build` (nowview, now), the same two calls in
//! the database's zone (stats), and the build log does
//! (`build_time_anchored` in datum-commands) are one composer,
//! `TimeBuilder`, so they name the same instant; the grid holds them to it,
//! and prints every input where they do not.
//!
//! They were two composers, and on main the grid found 165,352 of these
//! 344,736 inputs built differently. `what_each_disagreement_was_decided_as`
//! is one input from each cause, with the answer it was given.

use chrono::{DateTime, Utc};
use datum_parse::input::time_args::resolve_timezone;
use datum_parse::input::TimeArgs;
use datum_parse::time::{DatumTime, TimeCtx, Zone};

/// The zone the flags are read in: `-z`, else the reference's own.
fn zone_of(a: &TimeArgs, reference: &DatumTime) -> datum_parse::Result<(Zone, Option<String>)> {
    let name = match a.timezone.as_deref() {
        Some(spec) => Some(resolve_timezone(spec)?),
        None => reference.timezone.clone(),
    };
    Ok((Zone::named(name.as_deref().unwrap())?, name))
}

/// What log stores for these flags, with the reference as its clock.
fn as_log_builds(a: &TimeArgs, reference: &DatumTime) -> datum_parse::Result<DatumTime> {
    let (zone, name) = zone_of(a, reference)?;
    let ctx = TimeCtx::at(zone.local_naive(reference.utc));
    Ok(a.builder(ctx)?
        .build_with_uncertainty_at(reference.utc, zone, name))
}

/// Where `TimeArgs::build` and log's build differ on one input, if they do.
/// A date-only moment keeps its clock in the first and is the start of its
/// day, `P1D` wide, in the second.
fn disagreement(a: &TimeArgs, reference: &DatumTime) -> Option<String> {
    let (built, logged) = (a.build(reference), as_log_builds(a, reference));
    let agree = match (&built, &logged) {
        (Err(x), Err(y)) => x.to_string() == y.to_string(),
        (Ok(built), _) if a.omit_timestamp => built.time.is_none(),
        (Ok(built), Ok(logged)) => {
            let t = built.time.as_ref().unwrap();
            // Its own zone, as `strip_to_start_of_day` reads it: `-t`'s
            // offset may be it (da-kvosn).
            let zone = Zone::of(t).unwrap();
            let (at, uncertainty) = if built.only_date {
                let midnight = zone.local_naive(t.utc).date().and_time(Default::default());
                (zone.resolve(midnight).for_boundary(), Some("P1D".into()))
            } else {
                (t.utc, built.uncertainty.as_ref().and_then(|u| u.to_iso()))
            };
            at == logged.utc
                && uncertainty == logged.uncertainty
                && t.timezone == logged.timezone
                && (built.only_date || t.offset_minutes == logged.offset_minutes)
        }
        _ => false,
    };
    (!agree).then(|| format!("{a:?}\n  TimeArgs::build: {built:?}\n  log:             {logged:?}"))
}

#[test]
fn both_ways_in_agree_on_every_combination() {
    let zones = ["Europe/Berlin", "Pacific/Kiritimati", "Pacific/Pago_Pago"];
    let nows = [
        // An ordinary afternoon.
        "2020-05-10T15:25:30Z",
        // Spring forward, 2026-03-29 01:00Z: 01:30 CET before it, 03:30
        // CEST after it, and 00:30 CEST the night after.
        "2026-03-29T00:30:00Z",
        "2026-03-29T01:30:00Z",
        "2026-03-29T22:30:00Z",
        // Fall back, 2026-10-25 01:00Z: the first 02:30, the second, and
        // 23:30 CET at the end of the 25-hour day.
        "2026-10-25T00:30:00Z",
        "2026-10-25T01:30:00Z",
        "2026-10-25T22:30:00Z",
    ];
    let dates = [
        None,
        Some("2026-03-29"),
        Some("2026-10-25"),
        Some("-1"),
        Some("+1"),
        Some("sunday"),
        Some("today"),
        Some("the end of the universe"),
    ];
    let times = [
        None,
        // A clock, and the one each DST night skips or repeats.
        Some("10:15"),
        Some("2:30"),
        // A direction.
        Some("3+"),
        Some("18-"),
        // ISO: an instant, and a wall clock with no zone.
        Some("2026-03-29T00:30:00Z"),
        Some("2026-10-25T02:30"),
        // Relative.
        Some("-8"),
        Some("+90m"),
        Some("-3hrs"),
        Some("-30 minutes"),
        Some("+87.3"),
        // A day, and a day with a clock.
        Some("yesterday"),
        Some("sunday"),
        Some("yesterday 10am"),
        // An uncertainty.
        Some("10:15+-5m"),
        // What neither takes.
        Some("+3H"),
        Some("2500"),
        Some("half past nothing"),
    ];
    let tzs = [
        None,
        Some("Europe/Berlin"),
        Some("Pacific/Kiritimati"),
        Some("Pacific/Pago_Pago"),
    ];

    let mut inputs = 0;
    let mut disagreements = Vec::new();
    for zone in zones {
        for now in nows {
            let reference = DatumTime::from_utc_and_tz(now.parse().unwrap(), zone).unwrap();
            for yesterday in 0..3 {
                for quick in 0..3 {
                    for date in dates {
                        for time in times {
                            for tz in tzs {
                                for (full_day, omit_timestamp) in
                                    [(false, false), (true, false), (false, true)]
                                {
                                    let a = TimeArgs {
                                        date: date.map(str::to_string),
                                        yesterday,
                                        time: time.map(str::to_string),
                                        time_offsets: vec![],
                                        quick,
                                        timezone: tz.map(str::to_string),
                                        full_day,
                                        omit_timestamp,
                                    };
                                    inputs += 1;
                                    if let Some(d) = disagreement(&a, &reference) {
                                        disagreements.push(format!("{now} {zone} {d}"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} of {inputs} inputs disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

/// The flags built in Europe/Berlin at `now`: the wall clock they name,
/// and ` date` after it when the moment is date-only.
fn berlin(now: &str, flags: &[&str]) -> String {
    let mut a = TimeArgs::default();
    for pair in flags.chunks(2) {
        match pair {
            ["-y", n] => a.yesterday = n.parse().unwrap(),
            ["-q", n] => a.quick = n.parse().unwrap(),
            ["-d", d] => a.date = Some(d.to_string()),
            ["-t", t] => a.push_time(t),
            other => panic!("{other:?}"),
        }
    }
    let now: DateTime<Utc> = now.parse().unwrap();
    let reference = DatumTime::from_utc_and_tz(now, "Europe/Berlin").unwrap();
    assert_eq!(disagreement(&a, &reference), None);
    match a.build(&reference) {
        Ok(built) => format!(
            "{}{}",
            built.time.unwrap().local_time().naive_local(),
            if built.only_date { " date" } else { "" }
        ),
        Err(e) => e.to_string(),
    }
}

#[test]
fn what_each_disagreement_was_decided_as() {
    // 17:25:30 CEST on Sunday 2020-05-10.
    let afternoon = "2020-05-10T15:25:30Z";
    // 05:00 CEST on 2026-03-29, whose 02:30 the clock skipped; 02:30 CEST
    // and then 02:30 CET on 2026-10-25, which read it twice.
    let short_day = "2026-03-29T03:00:00Z";
    let (first_0230, second_0230) = ("2026-10-25T00:30:00Z", "2026-10-25T01:30:00Z");

    for (now, flags, want) in [
        // The order is v2's: `-t`, then `-q`, then `-d`, then `-y`.
        // TimeBuilder let a clock swallow `-q` and a date swallow `-y`.
        (
            afternoon,
            &["-t", "10:15", "-q", "3"][..],
            "2020-05-10 10:00:00",
        ),
        (
            afternoon,
            &["-d", "2010-09-01", "-y", "1"],
            "2010-08-31 17:25:30 date",
        ),
        // So a `-d` names the day whatever a shift crossed: 23:30 and 90
        // minutes is 01:00, on the 29th. TimeBuilder shifted last.
        (
            "2026-10-25T22:30:00Z",
            &["-d", "2026-03-29", "-t", "23:30", "-t", "+90m"],
            "2026-03-29 01:00:00",
        ),
        // With no clock the shift is from now, on a day that is not now's:
        // refused (da-e93nr), where it was 01:00 on the 29th as well.
        (
            "2026-10-25T22:30:00Z",
            &["-d", "2026-03-29", "-t", "+90m"],
            "Invalid time: a relative time (-q, -t ±N) needs a clock when the day moves: \
             give one, `-y -t 18:24`, or `-d 2026-10-05 -t 18:30 -t -6`",
        ),
        // A day that moves nothing is still a day. TimeBuilder took
        // `-d today` for no date at all.
        (afternoon, &["-d", "today"], "2020-05-10 17:25:30 date"),
        (
            afternoon,
            &["-y", "1", "-d", "+1"],
            "2020-05-10 17:25:30 date",
        ),
        // A `-t` that names a day is a date. TimeArgs went by the flag.
        (afternoon, &["-t", "yesterday"], "2020-05-09 17:25:30 date"),
        // A weekday is measured from now. TimeArgs measured it from where
        // `-t` had moved to, so this was the Sunday before.
        (
            afternoon,
            &["-d", "sunday", "-t", "18-"],
            "2020-05-10 18:00:00",
        ),
        // A date after a day `-t` named still replaces it, as it did in
        // both; only a `-y` counts back from a date.
        (
            afternoon,
            &["-t", "yesterday", "-d", "2026-03-29"],
            "2026-03-29 17:25:30 date",
        ),
        // A clock a DST night skipped or repeated comes out, at the change
        // or at its first reading. TimeArgs refused it, typed or not.
        (short_day, &["-t", "2:30"], "2026-03-29 03:00:00"),
        (
            first_0230,
            &["-d", "2026-03-29"],
            "2026-03-29 03:00:00 date",
        ),
        (
            afternoon,
            &["-t", "2026-10-25T02:30"],
            "2026-10-25 02:30:00",
        ),
        // And only where it lands: today's 02:30 is yesterday's, and
        // another day's, though today never read it.
        (short_day, &["-t", "2:30", "-y", "1"], "2026-03-28 02:30:00"),
        (
            short_day,
            &["-t", "2:30", "-d", "2026-06-01"],
            "2026-06-01 02:30:00",
        ),
        // TimeArgs clamped at each of two day shifts and kept the clamp.
        (
            "2026-03-29T22:30:00Z",
            &["-t", "2:30", "-d", "-1", "-y", "1"],
            "2026-03-28 02:30:00",
        ),
        // v2's units, which only TimeArgs took.
        (afternoon, &["-t", "-3hrs"], "2020-05-10 14:25:30"),
        (afternoon, &["-t", "-30 minutes"], "2020-05-10 16:55:30"),
        // The parser's own word on a bad `-t`. TimeArgs replaced it.
        (
            afternoon,
            &["-t", "2500"],
            "Invalid time: Cannot parse time: '2500' (must be HH/HHMM/HHMMSS as a valid \
             clock time; use '+2500' or '-2500' for relative minutes)",
        ),
    ] {
        assert_eq!(berlin(now, flags), want, "{flags:?} at {now}");
    }

    // Shifts that add up to nothing leave the instant alone. TimeArgs read
    // the second 02:30 again as the first.
    let reference =
        DatumTime::from_utc_and_tz(second_0230.parse().unwrap(), "Europe/Berlin").unwrap();
    let there_and_back = TimeArgs {
        yesterday: 1,
        date: Some("+1".into()),
        ..TimeArgs::default()
    };
    let built = there_and_back.build(&reference).unwrap();
    assert_eq!(built.time.unwrap().utc, reference.utc);

    // A reference with an offset and no zone name builds in that offset,
    // as it did.
    let nameless = DatumTime {
        timezone: None,
        ..reference.clone()
    };
    let mut clock = TimeArgs::default();
    clock.push_time("10:15");
    let built = clock.build(&nameless).unwrap().time.unwrap();
    assert_eq!(built.local_time().to_rfc3339(), "2026-10-25T10:15:00+01:00");
    assert_eq!(built.timezone, None);

    // `-q` is ±2.5 minutes wide. TimeArgs said it was exact.
    let quick = TimeArgs {
        quick: 1,
        ..TimeArgs::default()
    };
    let built = quick.build(&reference).unwrap();
    assert_eq!(built.uncertainty.unwrap().seconds, 150);
}
