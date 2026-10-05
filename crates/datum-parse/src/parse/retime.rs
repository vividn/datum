use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{self, lift, local_switch, local_value, LiftedSpec, TimeArgs};

pub(crate) fn parse_retime_args(tail: &[&str]) -> Result<RetimeArgs> {
    #[derive(Default)]
    struct RetimeParsing {
        offset: Option<String>,
        time: TimeArgs,
        field: Option<String>,
        after: Option<String>,
        before: Option<String>,
        relative: bool,
    }
    let mut registry: Vec<LiftedSpec<RetimeParsing>> = Vec::new();
    // Time-arg group (-t/-y/-q/-d/-z/-D/-T): lifted onto `time` so
    // retime composes the same time inputs as occur/start/stop. da-rorg
    // promotes -t/--time off the old custom value-slot to keep the
    // semantics unified across mutating commands.
    for spec in input::time_args::FLAG_SPECS {
        registry.push(lift(|p: &mut RetimeParsing| &mut p.time, spec));
    }
    registry.push(local_value(None, Some("offset"), |p, v| {
        p.offset = Some(v.to_string());
        Ok(())
    }));
    registry.push(local_value(Some('f'), Some("field"), |p, v| {
        p.field = Some(v.to_string());
        Ok(())
    }));
    registry.push(local_value(None, Some("after"), |p, v| {
        p.after = Some(v.to_string());
        Ok(())
    }));
    registry.push(local_value(None, Some("before"), |p, v| {
        p.before = Some(v.to_string());
        Ok(())
    }));
    // da-rorg: `-r`/`--relative` flips the reference frame for the
    // accompanying time args (and matches the implicit semantics of
    // a bare numeric positional). Note: `-r` is also `--reverse` on
    // tail; flag namespaces are per-command so they don't collide.
    registry.push(local_switch(Some('r'), Some("relative"), |p| {
        p.relative = true;
        Ok(())
    }));

    let mut composed = RetimeParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;

    // da-rorg: positionals split into a slug and at most one bare
    // numeric (minutes-offset shorthand for `-r -t <N>`). A short hid can
    // be all digits (da-q1k4p), so a number is the offset only when it is
    // signed or follows the slug: `retime 15437` names entry 15437, and
    // `retime +30` / `retime -5` / `retime <id> 30` shift.
    let mut slug: Option<String> = None;
    let mut bare_offset_min: Option<i64> = None;
    for p in &pos {
        let signed = p.starts_with(['-', '+']);
        if let Some(n) = p.parse::<i64>().ok().filter(|_| signed || slug.is_some()) {
            if bare_offset_min.is_some() {
                return Err(ParseError::InvalidArg(
                    "retime accepts at most one bare numeric positional (minutes offset)".into(),
                ));
            }
            bare_offset_min = Some(n);
        } else if slug.is_none() {
            slug = Some(p.to_string());
        } else {
            return Err(ParseError::InvalidArg(format!(
                "unexpected positional after slug: '{p}'"
            )));
        }
    }

    if composed.offset.is_some() && bare_offset_min.is_some() {
        return Err(ParseError::InvalidArg(
            "--offset and a bare numeric positional are mutually exclusive".into(),
        ));
    }
    // Either shift form returns before the time args are read, so beside
    // them it would silently drop them (da-k5oa5).
    let t = &composed.time;
    let time_args = t.time.is_some()
        || t.yesterday > 0
        || t.quick > 0
        || t.date.is_some()
        || t.full_day
        || t.omit_timestamp
        || t.timezone.is_some();
    if time_args || composed.relative {
        let shift = (composed.offset.as_ref().map(|_| "--offset".to_string()))
            .or(bare_offset_min.map(|n| format!("a bare minutes offset ({n})")));
        if let Some(shift) = shift {
            return Err(ParseError::InvalidArg(format!(
                "{shift} is mutually exclusive with time-arg flags"
            )));
        }
    }

    Ok(RetimeArgs {
        slug,
        offset: composed.offset,
        time: composed.time,
        field: composed.field,
        after: composed.after,
        before: composed.before,
        relative: composed.relative,
        bare_offset_min,
    })
}
