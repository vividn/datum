//! `datum allday <field> [value]` — a value for a whole day (da-rvvce).

use anyhow::Result;
use datum_core::{action::Action, db::Db};

use crate::args::LogArgs;

pub fn allday(args: LogArgs, db: &Db) -> Result<()> {
    crate::commands::log_core::log_entry(Action::AllDay, args, db)
}

/// An `allday` line is a `change … -m -D`, a point carrying the
/// day's value on today or the day `-d`/`-y` names, which no later day
/// inherits as a state would. A clock has no place in it, so `-t` and `-q`
/// are refused, and so is `-T`.
pub(crate) fn all_day(action: Action, args: &mut LogArgs) -> Result<Action> {
    if action != Action::AllDay {
        return Ok(action);
    }
    let t = &args.time;
    if t.time.is_some() || !t.time_offsets.is_empty() || t.quick > 0 {
        anyhow::bail!(
            "`{}` is logged as allday, which takes a date and no time: drop -t/-q, and name the day with -d or -y",
            args.field_or_verb
        );
    }
    if t.omit_timestamp {
        anyhow::bail!(
            "`{}` is logged as allday, which is on a day: drop -T",
            args.field_or_verb
        );
    }
    args.time.full_day = true;
    args.duration.moment = true;
    Ok(Action::Switch)
}
