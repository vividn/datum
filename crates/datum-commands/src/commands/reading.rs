//! How a typed time is read: the question a fall-back night's repeated hour
//! asks (da-6i7l1, da-job98), and the note a bare clock far ahead gets
//! (da-qo5in).

use std::collections::BTreeMap;

use anyhow::Result;
use datum_core::{entry::Entry, time::DatumTime};
use datum_parse::input::TimeArgs;
use datum_parse::{ParseError, Readings};

use crate::commands::question::Question;

/// A bare clock (`-t 23:50`, [`TimeArgs::bare_clock`]) is today's, as v2's
/// is, so typed just after midnight for a moment ago it lands almost a day
/// ahead. Over 12 hours ahead it is said in one line, `option` the way the
/// line gave the time; the entry stays as typed.
pub(crate) fn far_ahead(
    time: &TimeArgs,
    entry: &Entry,
    now: chrono::DateTime<chrono::Utc>,
    option: &str,
) -> Option<String> {
    let ahead = entry.at.as_ref()?.utc - now;
    let typed = time.time.as_deref()?;
    (time.bare_clock() && ahead > chrono::TimeDelta::hours(12)).then(|| {
        format!(
            "note: {typed} is today, {} h from now; for last night, -y {option}{typed}",
            ahead.num_hours()
        )
    })
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
