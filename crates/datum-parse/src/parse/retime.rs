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
    // da-rorg: `-r`/`--relative` reads the accompanying time args as a
    // pure shift. Note: `-r` is also `--reverse` on tail; flag namespaces
    // are per-command so they don't collide.
    registry.push(local_switch(Some('r'), Some("relative"), |p| {
        p.relative = true;
        Ok(())
    }));

    let mut composed = RetimeParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;

    // A slug, then at most one time in `-t`'s own grammar (da-jix52). A
    // short hid can be all digits (da-q1k4p), so a positional alone is the
    // time only when it is signed: `retime 15437` names entry 15437, and
    // `retime +30` / `retime -1h` / `retime <id> 1040` place `_LAST` or <id>.
    let (slug, when) = match pos[..] {
        [] => (None, None),
        [p] if p.starts_with(['-', '+']) => (None, Some(p)),
        [slug] => (Some(slug), None),
        [slug, when] => (Some(slug), Some(when)),
        [_, _, extra, ..] => {
            return Err(ParseError::InvalidArg(format!(
                "unexpected positional after the time: '{extra}'"
            )))
        }
    };
    // It IS a `-t` value from here on, so the two cannot disagree.
    if let Some(when) = when {
        composed.time.push_time(when);
    }

    // `--offset` returns before the time args are read, so beside them it
    // would silently drop them (da-k5oa5).
    let t = &composed.time;
    let placing = t.time.is_some()
        || t.yesterday > 0
        || t.quick > 0
        || t.date.is_some()
        || t.full_day
        || t.timezone.is_some()
        || composed.relative;
    if composed.offset.is_some() && (placing || t.omit_timestamp) {
        return Err(ParseError::InvalidArg(
            "--offset is mutually exclusive with time-arg flags".into(),
        ));
    }

    // `-T` takes the time away (da-1d0v2), so a time it would be moved to
    // has nowhere to go.
    if t.omit_timestamp && placing {
        return Err(ParseError::InvalidArg(
            "-T/--no-timestamp removes the time; it cannot be combined with time flags or -r"
                .into(),
        ));
    }

    Ok(RetimeArgs {
        slug: slug.map(str::to_string),
        offset: composed.offset,
        time: composed.time,
        field: composed.field,
        after: composed.after,
        before: composed.before,
        relative: composed.relative,
    })
}
