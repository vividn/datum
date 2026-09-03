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
    let t = parse_time_arg("1513", TimeCtx::now()).unwrap();
    assert_eq!(
        t.absolute.unwrap(),
        NaiveTime::from_hms_opt(15, 13, 0).unwrap()
    );
}

#[test]
fn test_parse_time_arg_hhmmss() {
    // da-xvsz: 6-digit forms must parse as HH:MM:SS, not fall through
    // to minute-offset (which would land 153_045 minutes from now).
    let t = parse_time_arg("153045", TimeCtx::now()).unwrap();
    assert_eq!(
        t.absolute.unwrap(),
        NaiveTime::from_hms_opt(15, 30, 45).unwrap()
    );
    assert!(t.offset.is_none(), "6-digit must not be an offset");
}

#[test]
fn test_parse_time_arg_hmmss_single_hour() {
    // 5-digit HMMSS — single-digit hour with full MM and SS.
    let t = parse_time_arg("93045", TimeCtx::now()).unwrap();
    assert_eq!(
        t.absolute.unwrap(),
        NaiveTime::from_hms_opt(9, 30, 45).unwrap()
    );
}

#[test]
fn test_parse_time_arg_relative() {
    let t = parse_time_arg("-8", TimeCtx::now()).unwrap();
    assert_eq!(t.offset.unwrap(), Duration::minutes(-8));
    let t = parse_time_arg("+5", TimeCtx::now()).unwrap();
    assert_eq!(t.offset.unwrap(), Duration::minutes(5));
}

#[test]
fn test_parse_time_arg_relative_decimal_and_units() {
    // da-5722: signed relative offsets must accept decimals and unit
    // suffixes, not just bare integers. The default unit stays minutes.
    assert_eq!(
        parse_time_arg("-3.5", TimeCtx::now())
            .unwrap()
            .offset
            .unwrap(),
        Duration::seconds(-210), // 3.5 min in the past
    );
    assert_eq!(
        parse_time_arg("+3.5", TimeCtx::now())
            .unwrap()
            .offset
            .unwrap(),
        Duration::seconds(210),
    );
    assert_eq!(
        parse_time_arg("-0.5", TimeCtx::now())
            .unwrap()
            .offset
            .unwrap(),
        Duration::seconds(-30),
    );
    assert_eq!(
        parse_time_arg("-10m", TimeCtx::now())
            .unwrap()
            .offset
            .unwrap(),
        Duration::minutes(-10),
    );
    assert_eq!(
        parse_time_arg("-1.5h", TimeCtx::now())
            .unwrap()
            .offset
            .unwrap(),
        Duration::minutes(-90),
    );
    assert_eq!(
        parse_time_arg("+100s", TimeCtx::now())
            .unwrap()
            .offset
            .unwrap(),
        Duration::seconds(100),
    );
    // Unsigned decimals are NOT relative offsets (the sign is the opt-in,
    // per da-4h7k) and aren't valid clocks → rejected.
    assert!(matches!(
        parse_time_arg(".5", TimeCtx::now()),
        Err(ParseError::InvalidTime(_))
    ));
}

#[test]
fn test_parse_time_arg_unsigned_invalid_time_errors() {
    // da-4h7k: an unsigned integer that isn't a valid clock time must be
    // rejected, not silently treated as a relative minute-offset. `2500`
    // (likely a `2400`/`24:00` typo) used to land 2500 minutes from now.
    assert!(matches!(
        parse_time_arg("2500", TimeCtx::now()),
        Err(ParseError::InvalidTime(_))
    ));
    assert!(matches!(
        parse_time_arg("9999", TimeCtx::now()),
        Err(ParseError::InvalidTime(_))
    ));
    // Valid clock-time forms still parse as absolute times.
    assert_eq!(
        parse_time_arg("0913", TimeCtx::now())
            .unwrap()
            .absolute
            .unwrap(),
        NaiveTime::from_hms_opt(9, 13, 0).unwrap()
    );
    // The signed form is the explicit opt-in for a (wild) minute offset.
    assert_eq!(
        parse_time_arg("-2500", TimeCtx::now()).unwrap().offset,
        Some(Duration::minutes(-2500))
    );
    assert_eq!(
        parse_time_arg("+2500", TimeCtx::now()).unwrap().offset,
        Some(Duration::minutes(2500))
    );
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
    let t = parse_time_arg("1513±5", TimeCtx::now()).unwrap();
    assert_eq!(
        t.absolute,
        Some(NaiveTime::from_hms_opt(15, 13, 0).unwrap())
    );
    let u = t.uncertainty.expect("uncertainty");
    // Bare number defaults to minutes.
    assert_eq!(u.seconds, 5 * 60);
    assert_eq!(u.unit, UncertaintyUnit::Minute);
}

#[test]
fn parse_uncertainty_plus_hyphen_ascii() {
    let t = parse_time_arg("1513+-5", TimeCtx::now()).unwrap();
    assert_eq!(
        t.absolute,
        Some(NaiveTime::from_hms_opt(15, 13, 0).unwrap())
    );
    let u = t.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 5 * 60);
    assert_eq!(u.unit, UncertaintyUnit::Minute);
}

#[test]
fn parse_uncertainty_with_units() {
    // seconds
    let u = parse_uncertainty("30s").unwrap();
    assert_eq!(u.seconds, 30);
    assert_eq!(u.unit, UncertaintyUnit::Second);
    // minutes (explicit)
    let u = parse_uncertainty("5m").unwrap();
    assert_eq!(u.seconds, 300);
    assert_eq!(u.unit, UncertaintyUnit::Minute);
    // hours
    let u = parse_uncertainty("2h").unwrap();
    assert_eq!(u.seconds, 7200);
    assert_eq!(u.unit, UncertaintyUnit::Hour);
    // days
    let u = parse_uncertainty("1d").unwrap();
    assert_eq!(u.seconds, 86400);
    assert_eq!(u.unit, UncertaintyUnit::Date);
}

#[test]
fn parse_combined_time_and_uncertainty() {
    // relative offset + unicode ± with minute unit
    let t = parse_time_arg("+5±2m", TimeCtx::now()).unwrap();
    assert_eq!(t.offset, Some(Duration::minutes(5)));
    let u = t.uncertainty.unwrap();
    assert_eq!(u.seconds, 120);
    assert_eq!(u.unit, UncertaintyUnit::Minute);

    // relative negative offset + ascii +- with second unit
    let t = parse_time_arg("-30+-10s", TimeCtx::now()).unwrap();
    assert_eq!(t.offset, Some(Duration::minutes(-30)));
    let u = t.uncertainty.unwrap();
    assert_eq!(u.seconds, 10);
    assert_eq!(u.unit, UncertaintyUnit::Second);

    // date + day uncertainty via the date parser
    let d = parse_date_arg("2024-03-16±1d", TimeCtx::now()).unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2024, 3, 16).unwrap())
    );
    let u = d.uncertainty.unwrap();
    assert_eq!(u.seconds, 86400);
    assert_eq!(u.unit, UncertaintyUnit::Date);
}

#[test]
fn q_flags_do_not_increase_uncertainty_below_five_qs() {
    for n in 1..=4u32 {
        let u = quick_uncertainty_for_count(n);
        assert_eq!(u.seconds, 150, "n={n}: expected 150s uncertainty");
        assert_eq!(u.unit, UncertaintyUnit::Minute);
    }
}

#[test]
fn five_plus_q_flags_escalate_uncertainty() {
    for n in 5..=10u32 {
        let u = quick_uncertainty_for_count(n);
        assert_eq!(u.seconds, 300, "n={n}: expected 300s uncertainty");
        assert_eq!(u.unit, UncertaintyUnit::Minute);
    }
}

#[test]
#[cfg(feature = "native")]
fn explicit_uncertainty_overrides_quick_uncertainty() {
    let t = parse_time_arg("+5±30s", TimeCtx::now()).unwrap();
    let builder = TimeBuilder::now()
        .with_quick_count(3)
        .with_quick_uncertainty(3)
        .with_time_arg(t);
    let u = builder.uncertainty();
    // Explicit ±30s wins over the -qqq default of 150s/minute.
    assert_eq!(u.seconds, 30);
    assert_eq!(u.unit, UncertaintyUnit::Second);
}

#[test]
fn relative_time_sign_future_vs_past() {
    // `+N` = N minutes in the future
    let t = parse_time_arg("+5", TimeCtx::now()).unwrap();
    assert_eq!(t.offset, Some(Duration::minutes(5)));

    // `-N` = N minutes in the past
    let t = parse_time_arg("-8", TimeCtx::now()).unwrap();
    assert_eq!(t.offset, Some(Duration::minutes(-8)));
}

// ── ISO-8601 datetime forms ─────────────────────────────────────────────

#[test]
fn iso_datetime_z_suffix() {
    let t = parse_time_arg("2024-03-16T15:30:00Z", TimeCtx::now()).unwrap();
    let dt = t.iso_instant.expect("iso_instant");
    assert_eq!(
        dt,
        chrono::DateTime::parse_from_rfc3339("2024-03-16T15:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    );
    assert!(t.iso_naive.is_none());
    assert!(t.absolute.is_none());
    assert!(t.offset.is_none());
}

#[test]
fn iso_datetime_offset_suffix() {
    let t = parse_time_arg("2024-03-16T15:30:00+02:00", TimeCtx::now()).unwrap();
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
    let t = parse_time_arg("2024-03-16T15:30:00.123456789Z", TimeCtx::now()).unwrap();
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
    let t = parse_time_arg("2024-03-16T15:30:00", TimeCtx::now()).unwrap();
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
    let t = parse_time_arg("2024-03-16T15:30:00Z±5m", TimeCtx::now()).unwrap();
    assert!(t.iso_instant.is_some());
    let u = t.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 300);
    assert_eq!(u.unit, UncertaintyUnit::Minute);
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
        timezone: Some("UTC".into()),
        uncertainty: None,
    };
    install(Arc::new(MockClock::new(dt)));
    f();
    uninstall();
}

#[test]
#[cfg(feature = "native")]
fn parse_date_arg_compact_apr29_uses_current_year_when_passed() {
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("apr29", TimeCtx::now()).unwrap();
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
        let d = parse_date_arg("APR29", TimeCtx::now()).unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
        let d = parse_date_arg("Jan5", TimeCtx::now()).unwrap();
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
        let d = parse_date_arg("jun7", TimeCtx::now()).unwrap();
        assert_eq!(
            d.absolute,
            Some(chrono::NaiveDate::from_ymd_opt(2025, 6, 7).unwrap())
        );
    });
}

#[test]
fn parse_date_arg_compact_explicit_year() {
    let d = parse_date_arg("apr29-2025", TimeCtx::now()).unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
    );
    let d = parse_date_arg("apr29@2025", TimeCtx::now()).unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
    );
}

#[test]
fn parse_date_arg_compact_unknown_month_errors() {
    assert!(parse_date_arg("nope29", TimeCtx::now()).is_err());
}

#[test]
#[cfg(feature = "native")]
fn parse_date_arg_compact_with_uncertainty_suffix() {
    with_mocked_today(chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(), || {
        let d = parse_date_arg("apr29±1d", TimeCtx::now()).unwrap();
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
        let d = parse_date_arg("apr29+", TimeCtx::now()).unwrap();
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
        let d = parse_date_arg("apr29+", TimeCtx::now()).unwrap();
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
            let d = parse_date_arg("apr29+", TimeCtx::now()).unwrap();
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
        let d = parse_date_arg("apr29-", TimeCtx::now()).unwrap();
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
        let d = parse_date_arg("apr29-", TimeCtx::now()).unwrap();
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
            let d = parse_date_arg("apr29-", TimeCtx::now()).unwrap();
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
        let d = parse_date_arg("apr29-2025+", TimeCtx::now()).unwrap();
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
    assert!(parse_time_arg("2024-03-16Tnotatime", TimeCtx::now()).is_err());
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
    let t = parse_time_arg("3+", TimeCtx::now()).unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Future));
}

#[test]
fn time_direction_suffix_minus_on_bare_hour() {
    let t = parse_time_arg("3-", TimeCtx::now()).unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_on_hh_mm() {
    let t = parse_time_arg("18:30+", TimeCtx::now()).unwrap();
    assert_eq!(
        t.absolute,
        Some(NaiveTime::from_hms_opt(18, 30, 0).unwrap())
    );
    assert_eq!(t.direction, Some(Direction::Future));
}

#[test]
fn time_direction_suffix_on_hhmm_compact() {
    let t = parse_time_arg("0830-", TimeCtx::now()).unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(8, 30, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_on_meridian() {
    let t = parse_time_arg("9am+", TimeCtx::now()).unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Future));
    let t = parse_time_arg("9pm-", TimeCtx::now()).unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(21, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_on_relative_form() {
    // A relative offset is already directional; the parser still surfaces
    // the hint, but the apply layer ignores it.
    let t = parse_time_arg("+5+", TimeCtx::now()).unwrap();
    assert_eq!(t.offset, Some(Duration::minutes(5)));
    assert_eq!(t.direction, Some(Direction::Future));
    let t = parse_time_arg("-30-", TimeCtx::now()).unwrap();
    assert_eq!(t.offset, Some(Duration::minutes(-30)));
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_after_uncertainty() {
    // Direction marker must be the *very last* character — strip it before
    // peeling the uncertainty body.
    let t = parse_time_arg("3±5m+", TimeCtx::now()).unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
    assert_eq!(t.direction, Some(Direction::Future));
    let u = t.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 300);

    // ASCII `+-` form for uncertainty + trailing `-` direction.
    let t = parse_time_arg("3+-5m-", TimeCtx::now()).unwrap();
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
    let t = parse_time_arg("2024-03-16T15:30:00Z+", TimeCtx::now()).unwrap();
    assert!(t.iso_instant.is_some());
    assert_eq!(t.direction, Some(Direction::Future));
    let t = parse_time_arg("2024-03-16T15:30:00-", TimeCtx::now()).unwrap();
    assert!(t.iso_naive.is_some());
    assert_eq!(t.direction, Some(Direction::Past));
}

#[test]
fn time_direction_suffix_double_marker_errors() {
    // `+-` / `-+` at the tail are explicit conflicts.
    assert!(parse_time_arg("3+-", TimeCtx::now()).is_err());
    assert!(parse_time_arg("3-+", TimeCtx::now()).is_err());
    assert!(parse_time_arg("3++", TimeCtx::now()).is_err());
    assert!(parse_time_arg("3--", TimeCtx::now()).is_err());
}

#[test]
fn time_direction_bare_marker_errors() {
    assert!(parse_time_arg("+", TimeCtx::now()).is_err());
    assert!(parse_time_arg("-", TimeCtx::now()).is_err());
}

#[test]
fn time_no_direction_on_unsuffixed_input() {
    let t = parse_time_arg("3", TimeCtx::now()).unwrap();
    assert_eq!(t.direction, None);
    let t = parse_time_arg("15:30", TimeCtx::now()).unwrap();
    assert_eq!(t.direction, None);
    let t = parse_time_arg("+5", TimeCtx::now()).unwrap();
    assert_eq!(t.direction, None);
    let t = parse_time_arg("2024-03-16T15:30:00Z", TimeCtx::now()).unwrap();
    assert_eq!(t.direction, None);
}

#[test]
fn date_direction_suffix_on_iso_date() {
    let d = parse_date_arg("2024-03-16+", TimeCtx::now()).unwrap();
    assert_eq!(
        d.absolute,
        Some(chrono::NaiveDate::from_ymd_opt(2024, 3, 16).unwrap())
    );
    assert_eq!(d.direction, Some(Direction::Future));

    let d = parse_date_arg("2024-03-16-", TimeCtx::now()).unwrap();
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
    let d = parse_date_arg("+1+", TimeCtx::now()).unwrap();
    assert_eq!(d.days_offset, 1);
    assert_eq!(d.direction, Some(Direction::Future));
    let d = parse_date_arg("-1-", TimeCtx::now()).unwrap();
    assert_eq!(d.days_offset, -1);
    assert_eq!(d.direction, Some(Direction::Past));
}

#[test]
fn date_direction_suffix_after_uncertainty() {
    let d = parse_date_arg("2024-03-16±1d+", TimeCtx::now()).unwrap();
    assert_eq!(d.direction, Some(Direction::Future));
    let u = d.uncertainty.expect("uncertainty");
    assert_eq!(u.seconds, 86400);
}

#[test]
fn date_direction_suffix_double_marker_errors() {
    assert!(parse_date_arg("2024-03-16+-", TimeCtx::now()).is_err());
    assert!(parse_date_arg("2024-03-16-+", TimeCtx::now()).is_err());
    assert!(parse_date_arg("+1++", TimeCtx::now()).is_err());
}

#[test]
fn date_no_direction_on_unsuffixed_input() {
    let d = parse_date_arg("2024-03-16", TimeCtx::now()).unwrap();
    assert_eq!(d.direction, None);
    let d = parse_date_arg("+1", TimeCtx::now()).unwrap();
    assert_eq!(d.direction, None);
    let d = parse_date_arg("0", TimeCtx::now()).unwrap();
    assert_eq!(d.direction, None);
}

// ── Day words ──────────────────────────────────────────────────────────

#[test]
fn date_day_words_are_relative_offsets() {
    assert_eq!(
        parse_date_arg("today", TimeCtx::now()).unwrap().days_offset,
        0
    );
    assert_eq!(
        parse_date_arg("yesterday", TimeCtx::now())
            .unwrap()
            .days_offset,
        -1
    );
    assert_eq!(
        parse_date_arg("Tomorrow", TimeCtx::now())
            .unwrap()
            .days_offset,
        1
    );
}

/// Day words name one specific day, so — like `±N` offsets — a trailing
/// direction marker is accepted but inert.
#[test]
fn date_day_words_ignore_direction_marker() {
    let d = parse_date_arg("yesterday-", TimeCtx::now()).unwrap();
    assert_eq!(d.days_offset, -1);
    assert_eq!(d.direction, Some(Direction::Past));
    assert!(d.absolute.is_none());
}

/// Day words need no clock: they resolve to an offset, not a date.
#[test]
fn date_day_words_need_no_reference() {
    let d = parse_date_arg("yesterday", TimeCtx::default()).unwrap();
    assert_eq!(d.days_offset, -1);
}

// ── Weekday names ──────────────────────────────────────────────────────

/// 2026-05-05 is a Tuesday. With the default backward-looking direction,
/// a bare weekday is the most recent one — vividn's decision for the vast
/// majority of cases (`--since sunday`).
#[test]
fn weekday_bare_resolves_to_most_recent_past() {
    let tue = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let ctx = TimeCtx::at(tue.and_hms_opt(12, 0, 0).unwrap());
    assert_eq!(
        parse_date_arg("sunday", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
    );
    assert_eq!(
        parse_date_arg("sun", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
    );
    assert_eq!(
        parse_date_arg("FRIDAY", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 1)
    );
}

/// The reference day answers for its own weekday when no marker forces a
/// strict search — mirroring `apr29` on April 29.
#[test]
fn weekday_bare_at_tie_picks_the_reference_day() {
    let tue = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let ctx = TimeCtx::at(tue.and_hms_opt(12, 0, 0).unwrap());
    assert_eq!(parse_date_arg("tuesday", ctx).unwrap().absolute, Some(tue));
}

/// A trailing marker is strict in both directions, so on a Tuesday
/// `tuesday-` is a week back and `tuesday+` a week on.
#[test]
fn weekday_direction_marker_is_strict() {
    let tue = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let ctx = TimeCtx::at(tue.and_hms_opt(12, 0, 0).unwrap());
    assert_eq!(
        parse_date_arg("tuesday-", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 4, 28)
    );
    assert_eq!(
        parse_date_arg("tuesday+", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 12)
    );
    assert_eq!(
        parse_date_arg("sunday+", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
    );
}

/// `Nearest` — the data-arg default — takes whichever side is closer.
/// From Tuesday, Sunday is 2 days back and Friday 3 days on.
#[test]
fn weekday_nearest_default_picks_the_closer_side() {
    let tue = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let ctx = TimeCtx::at(tue.and_hms_opt(12, 0, 0).unwrap()).facing(DefaultDirection::Nearest);
    assert_eq!(
        parse_date_arg("sunday", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
    );
    assert_eq!(
        parse_date_arg("friday", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 8)
    );
}

#[test]
fn weekday_future_default_looks_forward() {
    let tue = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let ctx = TimeCtx::at(tue.and_hms_opt(12, 0, 0).unwrap()).facing(DefaultDirection::Future);
    assert_eq!(
        parse_date_arg("sunday", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 10)
    );
}

/// Without an anchor there is no "most recent Sunday" to name, so the
/// parse fails loudly rather than guessing an epoch.
#[test]
fn weekday_without_reference_errors() {
    assert!(matches!(
        parse_date_arg("sunday", TimeCtx::default()),
        Err(ParseError::InvalidTime(_))
    ));
}

// ── Reference time drives the year-less compact form ────────────────────

/// The caller's reference, not the wall clock, decides the year — this is
/// what lets a data key resolve against its entry's occur-time.
#[test]
fn compact_date_resolves_against_the_supplied_reference() {
    let ctx = TimeCtx::at(
        chrono::NaiveDate::from_ymd_opt(2026, 5, 5)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap(),
    );
    assert_eq!(
        parse_date_arg("jun7", ctx).unwrap().absolute,
        chrono::NaiveDate::from_ymd_opt(2025, 6, 7)
    );
    assert_eq!(
        parse_date_arg("jun7", ctx.facing(DefaultDirection::Nearest))
            .unwrap()
            .absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 6, 7)
    );
}

// ── parse_time_arg falls back to date parsing ───────────────────────────

/// Half one of da-0gs4: a caller wanting "either a time or a date" makes
/// one call. Day-shaped input comes back in `TimeArg::date`.
#[test]
fn time_arg_falls_back_to_date_parsing() {
    let tue = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let ctx = TimeCtx::at(tue.and_hms_opt(12, 0, 0).unwrap());

    let t = parse_time_arg("sunday", ctx).unwrap();
    assert_eq!(
        t.date.expect("date fallback").absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 5, 3)
    );

    let t = parse_time_arg("yesterday", ctx).unwrap();
    assert_eq!(t.date.expect("date fallback").days_offset, -1);

    let t = parse_time_arg("2024-03-16", ctx).unwrap();
    assert_eq!(
        t.date.expect("date fallback").absolute,
        chrono::NaiveDate::from_ymd_opt(2024, 3, 16)
    );

    let t = parse_time_arg("apr29", ctx).unwrap();
    assert_eq!(
        t.date.expect("date fallback").absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 4, 29)
    );
}

/// Clock forms must keep winning: `+1` is one minute, never one day.
#[test]
fn time_arg_clock_forms_beat_the_date_fallback() {
    let ctx = TimeCtx::now();
    assert_eq!(
        parse_time_arg("+1", ctx).unwrap().offset,
        Some(Duration::minutes(1))
    );
    assert!(parse_time_arg("+1", ctx).unwrap().date.is_none());
    assert!(parse_time_arg("1513", ctx).unwrap().date.is_none());
}

/// da-4h7k's guard survives the fallback: an out-of-range bare integer
/// must not slide through to the date parser, where `2500` would read as
/// a 2500-day offset.
#[test]
fn time_arg_invalid_clock_integer_does_not_reach_date_fallback() {
    assert!(parse_time_arg("2500", TimeCtx::now()).is_err());
    assert!(parse_time_arg("9999", TimeCtx::now()).is_err());
}

/// Both suffixes survive the hand-off to the date parser, in the usual
/// order (uncertainty, then direction at the very tail).
#[test]
fn time_arg_date_fallback_keeps_direction_and_uncertainty() {
    let tue = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let ctx = TimeCtx::at(tue.and_hms_opt(12, 0, 0).unwrap());
    let t = parse_time_arg("tuesday±1d-", ctx).unwrap();
    assert_eq!(t.direction, Some(Direction::Past));
    assert_eq!(t.uncertainty.expect("uncertainty").seconds, 86_400);
    assert_eq!(
        t.date.expect("date fallback").absolute,
        chrono::NaiveDate::from_ymd_opt(2026, 4, 28)
    );
}
