use chrono::{Duration, NaiveTime};

use super::*;

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
    assert_eq!(t.absolute.unwrap(), NaiveTime::from_hms_opt(15, 13, 0).unwrap());
}

#[test]
fn test_parse_time_arg_relative() {
    let t = parse_time_arg("-8").unwrap();
    assert_eq!(t.minutes_offset.unwrap(), -8);
    let t = parse_time_arg("+5").unwrap();
    assert_eq!(t.minutes_offset.unwrap(), 5);
}

#[test]
fn test_parse_duration_minutes_default() {
    assert_eq!(parse_duration("5").unwrap(), Duration::minutes(5));
    assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
    assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
}

#[test]
fn test_parse_duration_colon() {
    assert_eq!(parse_duration("1:30").unwrap(), Duration::minutes(90));
    assert_eq!(parse_duration("1:15:30").unwrap(), Duration::seconds(4530));
}

#[test]
fn test_parse_iso_duration() {
    assert_eq!(parse_duration("PT5M").unwrap(), Duration::minutes(5));
    assert_eq!(parse_duration("PT1H30M").unwrap(), Duration::minutes(90));
}

#[test]
fn parse_uncertainty_plus_minus_unicode() {
    let t = parse_time_arg("1513±5").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(15, 13, 0).unwrap()));
    let u = t.uncertainty.expect("uncertainty");
    // Bare number defaults to minutes.
    assert_eq!(u.seconds, 5 * 60);
    assert_eq!(u.unit, PrecisionUnit::Minute);
}

#[test]
fn parse_uncertainty_plus_hyphen_ascii() {
    let t = parse_time_arg("1513+-5").unwrap();
    assert_eq!(t.absolute, Some(NaiveTime::from_hms_opt(15, 13, 0).unwrap()));
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
