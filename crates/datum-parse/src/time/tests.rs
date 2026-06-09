use chrono::{Duration, NaiveTime};

use super::*;
use crate::ParseError;

#[test]
fn test_stacked_flags() {
    assert_eq!(count_stacked_flag("-y", 'y'), Some(1));
    assert_eq!(count_stacked_flag("-yy", 'y'), Some(2));
    assert_eq!(count_stacked_flag("-yyy", 'y'), Some(3));
    assert_eq!(count_stacked_flag("-qqqq", 'q'), Some(4));
    assert_eq!(count_stacked_flag("-d", 'y'), None);
    assert_eq!(count_stacked_flag("y", 'y'), None);
}

#[test]
fn test_parse_time_arg_hhmm() {
    let t = parse_time_arg("1513").unwrap();
    assert_eq!(
        t.absolute.unwrap(),
        NaiveTime::from_hms_opt(15, 13, 0).unwrap()
    );
}

#[test]
fn test_parse_time_arg_hhmmss() {
    // da-xvsz: 6-digit forms must parse as HH:MM:SS, not fall through
    // to minute-offset (which would land 153_045 minutes from now).
    let t = parse_time_arg("153045").unwrap();
    assert_eq!(
        t.absolute.unwrap(),
        NaiveTime::from_hms_opt(15, 30, 45).unwrap()
    );
    assert!(t.minutes_offset.is_none(), "6-digit must not be an offset");
}

#[test]
fn test_parse_time_arg_hmmss_single_hour() {
    // 5-digit HMMSS — single-digit hour with full MM and SS.
    let t = parse_time_arg("93045").unwrap();
    assert_eq!(
        t.absolute.unwrap(),
        NaiveTime::from_hms_opt(9, 30, 45).unwrap()
    );
}

#[test]
fn test_parse_time_arg_relative() {
    let t = parse_time_arg("-8").unwrap();
    assert_eq!(t.minutes_offset.unwrap(), -8);
    let t = parse_time_arg("+5").unwrap();
    assert_eq!(t.minutes_offset.unwrap(), 5);
}

#[test]
fn test_parse_time_arg_unsigned_invalid_time_errors() {
    // da-4h7k: an unsigned integer that isn't a valid clock time must be
    // rejected, not silently treated as a relative minute-offset. `2500`
    // (likely a `2400`/`24:00` typo) used to land 2500 minutes from now.
    assert!(matches!(
        parse_time_arg("2500"),
        Err(ParseError::InvalidTime(_))
    ));
    assert!(matches!(
        parse_time_arg("9999"),
        Err(ParseError::InvalidTime(_))
    ));
    // Valid clock-time forms still parse as absolute times.
    assert_eq!(
        parse_time_arg("0913").unwrap().absolute.unwrap(),
        NaiveTime::from_hms_opt(9, 13, 0).unwrap()
    );
    // The signed form is the explicit opt-in for a (wild) minute offset.
    assert_eq!(parse_time_arg("-2500").unwrap().minutes_offset, Some(-2500));
    assert_eq!(parse_time_arg("+2500").unwrap().minutes_offset, Some(2500));
}

#[test]
fn test_parse_duration_minutes_default() {
    assert_eq!(parse_duration("5").unwrap(), Duration::minutes(5));
    assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
    assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
}

#[test]
fn test_parse_duration_colon() {
    // da-b1q5: the rightmost colon field shifts by part count — minutes for
    // the 2-part HH:MM form, seconds for 3+ parts. A bare `1:30` is 1h30m
    // (one-thirty stopwatch-style), a deliberate 60× divergence from v2's
    // seconds-anchored `1:30` = 1m30s. Lock both readings against drift.
    assert_eq!(parse_duration("1:30").unwrap(), Duration::minutes(90));
    assert_eq!(parse_duration("0:30").unwrap(), Duration::minutes(30));
    assert_eq!(parse_duration("1:15:30").unwrap(), Duration::seconds(4530));
    assert_eq!(parse_duration("0:00:30").unwrap(), Duration::seconds(30));
    assert_eq!(
        parse_duration("1:00:00:00").unwrap(),
        Duration::seconds(86400),
    );
}

#[test]
fn test_parse_iso_duration() {
    assert_eq!(parse_duration("PT5M").unwrap(), Duration::minutes(5));
    assert_eq!(parse_duration("PT1H30M").unwrap(), Duration::minutes(90));
}

#[test]
fn test_parse_duration_compound() {
    // da-ixrk: compound forms must parse, not fall through to "state".
    assert_eq!(
        parse_duration("10m30s").unwrap(),
        Duration::seconds(10 * 60 + 30),
    );
    assert_eq!(parse_duration("1h30m").unwrap(), Duration::minutes(90));
    assert_eq!(
        parse_duration("1h30m45s").unwrap(),
        Duration::seconds(3600 + 30 * 60 + 45),
    );
    assert_eq!(
        parse_duration("2d4h").unwrap(),
        Duration::seconds(2 * 86400 + 4 * 3600),
    );
}

#[test]
fn test_parse_duration_invalid() {
    assert!(parse_duration("abc").is_err());
    assert!(parse_duration("10m30").is_err()); // trailing bare number in compound
    assert!(parse_duration("10x").is_err()); // unknown unit
    assert!(parse_duration("").is_err());
}

#[test]
fn parse_uncertainty_plus_minus_unicode() {
    let t = parse_time_arg("1513±5").unwrap();
    assert_eq!(
        t.absolute,
        Some(NaiveTime::from_hms_opt(15, 13, 0).unwrap())
    );
    let u = t.uncertainty.expect("uncertainty");
    // Bare number defaults to minutes.
    assert_eq!(u.seconds, 5 * 60);
    assert_eq!(u.unit, PrecisionUnit::Minute);
}

#[test]
fn parse_uncertainty_plus_hyphen_ascii() {
    let t = parse_time_arg("1513+-5").unwrap();
    assert_eq!(
        t.absolute,
        Some(NaiveTime::from_hms_opt(15, 13, 0).unwrap())
    );
    let u = t.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 5 * 60);
    assert_eq!(u.unit, PrecisionUnit::Minute);
}

#[test]
fn parse_uncertainty_with_units() {
    // seconds
    let u = parse_uncertainty("30s").unwrap();
    assert_eq!(u.seconds, 30);
    assert_eq!(u.unit, PrecisionUnit::Second);
    // minutes (explicit)
    let u = parse_uncertainty("5m").unwrap();
    assert_eq!(u.seconds, 300);
    assert_eq!(u.unit, PrecisionUnit::Minute);
    // hours
    let u = parse_uncertainty("2h").unwrap();
    assert_eq!(u.seconds, 7200);
    assert_eq!(u.unit, PrecisionUnit::Hour);
    // days
    let u = parse_uncertainty("1d").unwrap();
    assert_eq!(u.seconds, 86400);
    assert_eq!(u.unit, PrecisionUnit::Date);
}

#[test]
fn parse_combined_time_and_uncertainty() {
    // relative offset + unicode ± with minute unit
    let t = parse_time_arg("+5±2m").unwrap();
    assert_eq!(t.minutes_offset, Some(5));
    let u = t.uncertainty.unwrap();
    assert_eq!(u.seconds, 120);
    assert_eq!(u.unit, PrecisionUnit::Minute);

    // relative negative offset + ascii +- with second unit
    let t = parse_time_arg("-30+-10s").unwrap();
    assert_eq!(t.minutes_offset, Some(-30));
    let u = t.uncertainty.unwrap();
    assert_eq!(u.seconds, 10);
    assert_eq!(u.unit, PrecisionUnit::Second);

    // date + day uncertainty via the date parser
    let d = parse_date_arg("2024-03-16±1d").unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2024, 3, 16).unwrap())
    );
    let u = d.uncertainty.unwrap();
    assert_eq!(u.seconds, 86400);
    assert_eq!(u.unit, PrecisionUnit::Date);
}

#[test]
fn q_flags_do_not_increase_precision_below_five_qs() {
    for n in 1..=4u32 {
        let u = quick_precision_for_count(n);
        assert_eq!(u.seconds, 150, "n={n}: expected 150s precision");
        assert_eq!(u.unit, PrecisionUnit::Minute);
    }
}

#[test]
fn five_plus_q_flags_escalate_precision() {
    for n in 5..=10u32 {
        let u = quick_precision_for_count(n);
        assert_eq!(u.seconds, 300, "n={n}: expected 300s precision");
        assert_eq!(u.unit, PrecisionUnit::Minute);
    }
}

#[test]
#[cfg(feature = "native")]
fn explicit_uncertainty_overrides_q_precision() {
    let t = parse_time_arg("+5±30s").unwrap();
    let builder = TimeBuilder::now()
        .with_quick_count(3)
        .with_quick_precision(3)
        .with_time_arg(t);
    let u = builder.precision();
    // Explicit ±30s wins over the -qqq default of 150s/minute.
    assert_eq!(u.seconds, 30);
    assert_eq!(u.unit, PrecisionUnit::Second);
}

#[test]
fn relative_time_sign_future_vs_past() {
    // `+N` = N minutes in the future
    let t = parse_time_arg("+5").unwrap();
    assert_eq!(t.minutes_offset, Some(5));

    // `-N` = N minutes in the past
    let t = parse_time_arg("-8").unwrap();
    assert_eq!(t.minutes_offset, Some(-8));
}

// ── ISO-8601 datetime forms ─────────────────────────────────────────────

#[test]
fn iso_datetime_z_suffix() {
    let t = parse_time_arg("2024-03-16T15:30:00Z").unwrap();
    let dt = t.iso_instant.expect("iso_instant");
    assert_eq!(
        dt,
        chrono::DateTime::parse_from_rfc3339("2024-03-16T15:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    );
    assert!(t.iso_naive.is_none());
    assert!(t.absolute.is_none());
    assert!(t.minutes_offset.is_none());
}

#[test]
fn iso_datetime_offset_suffix() {
    let t = parse_time_arg("2024-03-16T15:30:00+02:00").unwrap();
    let dt = t.iso_instant.expect("iso_instant");
    // 15:30 at +02:00 ↔ 13:30 UTC.
    assert_eq!(
        dt,
        chrono::DateTime::parse_from_rfc3339("2024-03-16T13:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    );
}

#[test]
fn iso_datetime_high_precision_fractional_seconds() {
    let t = parse_time_arg("2024-03-16T15:30:00.123456789Z").unwrap();
    let dt = t.iso_instant.expect("iso_instant");
    // Fractional seconds carry through the parse.
    assert_eq!(
        dt.timestamp_subsec_nanos(),
        123_456_789,
        "expected nanosecond precision to survive parse"
    );
}

#[test]
fn iso_datetime_no_tz_suffix_returns_naive() {
    let t = parse_time_arg("2024-03-16T15:30:00").unwrap();
    let naive = t.iso_naive.expect("iso_naive");
    assert_eq!(
        naive,
        chrono::NaiveDate::from_ymd_opt(2024, 3, 16)
            .unwrap()
            .and_hms_opt(15, 30, 0)
            .unwrap()
    );
    assert!(t.iso_instant.is_none());
}

#[test]
fn iso_datetime_with_uncertainty_suffix() {
    let t = parse_time_arg("2024-03-16T15:30:00Z±5m").unwrap();
    assert!(t.iso_instant.is_some());
    let u = t.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 300);
    assert_eq!(u.unit, PrecisionUnit::Minute);
}

// ── Compact <mmm><day> shorthand ────────────────────────────────────────

#[cfg(feature = "native")]
fn with_mocked_today<F: FnOnce()>(today: chrono::NaiveDate, f: F) {
    use crate::time::clock::{install, uninstall, MockClock};
    use crate::time::DatumTime;
    use chrono::TimeZone;
    use std::sync::Arc;
    let utc = chrono::Utc.from_utc_datetime(&today.and_hms_opt(12, 0, 0).unwrap());
    let dt = DatumTime {
        utc,
        offset_minutes: 0,
        timezone: "UTC".into(),
    };
    install(Arc::new(MockClock::new(dt)));
    f();
    uninstall();
}

#[test]
#[cfg(feature = "native")]
fn parse_date_arg_compact_apr29_uses_current_year_when_passed() {
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("apr29").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    });
}

#[test]
#[cfg(feature = "native")]
fn parse_date_arg_compact_case_insensitive() {
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("APR29").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
        let d = parse_date_arg("Jan5").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap())
        );
    });
}

#[test]
#[cfg(feature = "native")]
fn parse_date_arg_compact_jun7_rolls_back_year() {
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("jun7").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2025, 6, 7).unwrap())
        );
    });
}

#[test]
fn parse_date_arg_compact_explicit_year() {
    let d = parse_date_arg("apr29-2025").unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
    );
    let d = parse_date_arg("apr29@2025").unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
    );
}

#[test]
fn parse_date_arg_compact_unknown_month_errors() {
    assert!(parse_date_arg("nope29").is_err());
}

#[test]
#[cfg(feature = "native")]
fn parse_date_arg_compact_with_uncertainty_suffix() {
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("apr29±1d").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
        let u = d.uncertainty.expect("uncertainty");
        assert_eq!(u.seconds, 86400);
    });
}

// ── compact dates + direction suffix (frozen-clock) ─────────────────────

#[test]
#[cfg(feature = "native")]
fn compact_date_future_picks_this_year_when_upcoming() {
    // 2026-04-05: apr29+ still ahead → 2026-04-29.
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 4, 5).unwrap(), || {
        let d = parse_date_arg("apr29+").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
        assert_eq!(d.direction, Some(Direction::Future));
    });
}

#[test]
#[cfg(feature = "native")]
fn compact_date_future_picks_next_year_when_passed() {
    // 2026-05-05: apr29+ already past → 2027-04-29.
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("apr29+").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2027, 4, 29).unwrap())
        );
    });
}

#[test]
#[cfg(feature = "native")]
fn compact_date_future_at_tie_picks_next_year() {
    // 2026-04-29 + apr29+ → push to 2027-04-29.
    with_mocked_today(
        chrono::NaiveDate::from_ymd_opt(2026, 4, 29).unwrap(),
        || {
            let d = parse_date_arg("apr29+").unwrap();
            assert_eq!(
                d.absolute,
                Some(chrono::NaiveDate::from_ymd_opt(2027, 4, 29).unwrap())
            );
        },
    );
}

#[test]
#[cfg(feature = "native")]
fn compact_date_past_picks_this_year_when_already_passed() {
    // 2026-05-05: apr29- already past → 2026-04-29.
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("apr29-").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
        assert_eq!(d.direction, Some(Direction::Past));
    });
}

#[test]
#[cfg(feature = "native")]
fn compact_date_past_picks_last_year_when_upcoming() {
    // 2026-04-05: apr29- not yet → 2025-04-29.
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 4, 5).unwrap(), || {
        let d = parse_date_arg("apr29-").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
        );
    });
}

#[test]
#[cfg(feature = "native")]
fn compact_date_past_at_tie_picks_last_year() {
    // 2026-04-29 + apr29- → push to 2025-04-29.
    with_mocked_today(
        chrono::NaiveDate::from_ymd_opt(2026, 4, 29).unwrap(),
        || {
            let d = parse_date_arg("apr29-").unwrap();
            assert_eq!(
                d.absolute,
                Some(chrono::NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
            );
        },
    );
}

#[test]
#[cfg(feature = "native")]
fn compact_date_explicit_year_ignores_direction() {
    // Year-pinned compact form: direction marker has nothing to disambiguate.
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("apr29-2025+").unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
        );
        assert_eq!(d.direction, Some(Direction::Future));
    });
}

#[test]
fn iso_datetime_malformed_errors() {
    // Looks like an ISO datetime (date prefix + T) but the time portion is
    // garbage — we surface a structured error instead of falling through to
    // the time-only branches and returning a confusing message.
    assert!(parse_time_arg("2024-03-16Tnotatime").is_err());
}

// ── direction suffix (`+` / `-`) ─────────────────────────────────────────
//
// Parser-level tests verify that the trailing direction marker is
// recognised, surfaces on the parsed shape, and that already-pinned forms
// (ISO instants, ISO naive datetimes) still carry the hint without altering
// the underlying value (the *application* of the hint is exercised under
// the frozen-clock harness in `input::time_args::tests`).

#[test]
fn time_direction_suffix_plus_on_bare_hour() {
    let t = parse_time_arg("3+").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Future));
}

#[test]
fn time_direction_suffix_minus_on_bare_hour() {
    let t = parse_time_arg("3-").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_on_hh_mm() {
    let t = parse_time_arg("18:30+").unwrap();
    assert_eq!(
        t.absolute,
        Some(NaiveTime::from_hms_opt(18, 30, 0).unwrap())
    );
    assert_eq!(t.direction, Some(Direction::Future));
}

#[test]
fn time_direction_suffix_on_hhmm_compact() {
    let t = parse_time_arg("0830-").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(8, 30, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_on_meridian() {
    let t = parse_time_arg("9am+").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Future));
    let t = parse_time_arg("9pm-").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(21, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_on_relative_form() {
    // A relative offset is already directional; the parser still surfaces
    // the hint, but the apply layer ignores it.
    let t = parse_time_arg("+5+").unwrap();
    assert_eq!(t.minutes_offset, Some(5));
    assert_eq!(t.direction, Some(Direction::Future));
    let t = parse_time_arg("-30-").unwrap();
    assert_eq!(t.minutes_offset, Some(-30));
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_after_uncertainty() {
    // Direction marker must be the *very last* character — strip it before
    // peeling the uncertainty body.
    let t = parse_time_arg("3±5m+").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Future));
    let u = t.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 300);

    // ASCII `+-` form for uncertainty + trailing `-` direction.
    let t = parse_time_arg("3+-5m-").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Past));
    let u = t.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 300);
}

#[test]
fn time_direction_suffix_on_iso_instant() {
    // Hint is parsed but the apply layer treats pinned instants as
    // already-resolved. Verify it surfaces on the parse shape and the
    // underlying instant is unchanged.
    let t = parse_time_arg("2024-03-16T15:30:00Z+").unwrap();
    assert!(t.iso_instant.is_some());
    assert_eq!(t.direction, Some(Direction::Future));
    let t = parse_time_arg("2024-03-16T15:30:00-").unwrap();
    assert!(t.iso_naive.is_some());
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_double_marker_errors() {
    // `+-` / `-+` at the tail are explicit conflicts.
    assert!(parse_time_arg("3+-").is_err());
    assert!(parse_time_arg("3-+").is_err());
    assert!(parse_time_arg("3++").is_err());
    assert!(parse_time_arg("3--").is_err());
}

#[test]
fn time_direction_bare_marker_errors() {
    assert!(parse_time_arg("+").is_err());
    assert!(parse_time_arg("-").is_err());
}

#[test]
fn time_no_direction_on_unsuffixed_input() {
    let t = parse_time_arg("3").unwrap();
    assert_eq!(t.direction, None);
    let t = parse_time_arg("15:30").unwrap();
    assert_eq!(t.direction, None);
    let t = parse_time_arg("+5").unwrap();
    assert_eq!(t.direction, None);
    let t = parse_time_arg("2024-03-16T15:30:00Z").unwrap();
    assert_eq!(t.direction, None);
}

#[test]
fn date_direction_suffix_on_iso_date() {
    let d = parse_date_arg("2024-03-16+").unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2024, 3, 16).unwrap())
    );
    assert_eq!(d.direction, Some(Direction::Future));

    let d = parse_date_arg("2024-03-16-").unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2024, 3, 16).unwrap())
    );
    assert_eq!(d.direction, Some(Direction::Past));
}

#[test]
fn date_direction_suffix_on_relative_offset() {
    // Like time, a relative date offset is already directional; parser
    // surfaces the hint, apply layer ignores it.
    let d = parse_date_arg("+1+").unwrap();
    assert_eq!(d.days_offset, 1);
    assert_eq!(d.direction, Some(Direction::Future));
    let d = parse_date_arg("-1-").unwrap();
    assert_eq!(d.days_offset, -1);
    assert_eq!(d.direction, Some(Direction::Past));
}

#[test]
fn date_direction_suffix_after_uncertainty() {
    let d = parse_date_arg("2024-03-16±1d+").unwrap();
    assert_eq!(d.direction, Some(Direction::Future));
    let u = d.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 86400);
}

#[test]
fn date_direction_suffix_double_marker_errors() {
    assert!(parse_date_arg("2024-03-16+-").is_err());
    assert!(parse_date_arg("2024-03-16-+").is_err());
    assert!(parse_date_arg("+1++").is_err());
}

#[test]
fn date_no_direction_on_unsuffixed_input() {
    let d = parse_date_arg("2024-03-16").unwrap();
    assert_eq!(d.direction, None);
    let d = parse_date_arg("+1").unwrap();
    assert_eq!(d.direction, None);
    let d = parse_date_arg("0").unwrap();
    assert_eq!(d.direction, None);
}
