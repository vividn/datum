//! How a typed time is read: the question a fall-back night's repeated hour
//! asks (da-6i7l1, da-job98), and the hint an evening logged before noon
//! gets (da-85b0d).

use std::collections::BTreeMap;

use anyhow::Result;
use datum_core::time::DatumTime;
use datum_parse::{ParseError, Readings};

use crate::commands::question::Question;

/// The hint a time this evening gets when logged before noon (da-85b0d):
/// most likely last night's, so it says how to move it there. The entry
/// stays as typed, and its echo says FUTURE.
pub(crate) fn last_night(
    at: &DatumTime,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<&'static str> {
    use chrono::Timelike;
    let at = at.local_time();
    let now = now.with_timezone(at.offset());
    (at.date_naive() == now.date_naive() && at.hour() >= 20 && now.hour() < 12)
        .then_some("note: `retime -y` to move to yesterday")
}

/// Which of the two instants the flags name the line means, where they name
/// a wall time a fall-back night repeats (da-6i7l1): `Some(later)`, `false`
/// when there is one, or `None` when the person asked declined. `read` is
/// the line's build with `TimeArgs::later` as given. No reading is a safe
/// default, so with no answer the question is [`Unanswered`], naming both
/// full spellings, each after `option`, how the line gave the time (`-t `,
/// `end_time=`).
///
/// [`Unanswered`]: crate::commands::question::Unanswered
pub(crate) fn which_reading(
    read: impl Fn(bool) -> Option<DatumTime>,
    option: &str,
) -> Result<Option<bool>> {
    let (Some(first), Some(second)) = (read(false), read(true)) else {
        return Ok(Some(false));
    };
    if first.utc == second.utc {
        return Ok(Some(false));
    }
    ask_reading(&Readings::new(option, &first, &second))
}

/// [`which_reading`]'s question, of a `*_time` key too (da-job98): `None`
/// when the person asked declined.
fn ask_reading(readings: &Readings) -> Result<Option<bool>> {
    let [a, b] = readings.options();
    let question = Question::choose(
        format!("{}. Which one?", readings.said()),
        vec![a.clone(), b.clone()],
    );
    let answer = question.ask()?;
    let option = readings.option.trim_end();
    let spelled = |o: &str| o.strip_prefix(option).unwrap_or(o).trim().to_string();
    let answer = spelled(&answer);
    Ok(match answer.as_str() {
        "1" => Some(false),
        "2" => Some(true),
        _ if answer == spelled(&a) => Some(false),
        _ if answer == spelled(&b) => Some(true),
        _ => None,
    })
}

/// `parse` again until every `*_time` key naming a wall time a fall-back
/// night repeats has its reading in `later`, asking of each as `-t` is
/// asked (da-job98): `None` when the person asked declined.
pub(crate) fn asking<T>(
    later: &mut BTreeMap<String, bool>,
    mut parse: impl FnMut(&BTreeMap<String, bool>) -> Result<T>,
) -> Result<Option<T>> {
    loop {
        let err = match parse(later) {
            Ok(parsed) => return Ok(Some(parsed)),
            Err(err) => err,
        };
        let Some(ParseError::RepeatedTime(readings)) = err.downcast_ref() else {
            return Err(err);
        };
        let key = readings.option.trim_end_matches('=').to_string();
        match ask_reading(readings)? {
            Some(reading) => later.insert(key, reading),
            None => return Ok(None),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hint for an entry at `at`, logged at `now`, both in Berlin on
    /// 2026-11-09 unless `now` names its day.
    fn hint(at: &str, now: &str) -> bool {
        let read = |s: &str| {
            let s = if s.contains('T') {
                s.to_string()
            } else {
                format!("2026-11-09T{s}")
            };
            let utc = chrono::DateTime::parse_from_rfc3339(&format!("{s}:00+01:00")).unwrap();
            DatumTime::from_utc_and_tz(utc.into(), "Europe/Berlin").unwrap()
        };
        last_night(&read(at), read(now).utc).is_some()
    }

    #[test]
    fn an_evening_logged_before_noon_is_hinted_at_its_edges() {
        assert!(!hint("19:59", "09:00"));
        assert!(hint("20:00", "09:00"));
        assert!(hint("23:59", "09:00"));
        assert!(hint("23:30", "11:59"));
        assert!(!hint("23:30", "12:00"));
        // Last night's own evening, and tomorrow's, are where they belong.
        assert!(!hint("2026-11-08T23:30", "09:00"));
        assert!(!hint("2026-11-10T23:30", "09:00"));
    }
}
