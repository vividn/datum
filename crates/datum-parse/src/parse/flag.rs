use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{self, lift, LiftedSpec, TimeArgs};

/// `datum flag <field> [comment]` — field positional + optional
/// comment positionals joined on space + standard timing flags
/// (`-y/-q/-d/-t/-z`). Honors back-dated time so a flag can be placed
/// at a specific past moment.
#[derive(Default)]
pub(crate) struct FlagParsing {
    time: TimeArgs,
}

pub(crate) fn flag_args_registry() -> Vec<LiftedSpec<FlagParsing>> {
    moment_time_flags(|p: &mut FlagParsing| &mut p.time)
}

/// The time flags of a command that places one moment, as `flag`, `track`
/// and `ignore` do: `-y/-q/-d/-t/-z`, without `-T/-D` (a moment with no
/// time, or a whole day, loses its meaning).
pub(crate) fn moment_time_flags<P: 'static>(
    time: fn(&mut P) -> &mut TimeArgs,
) -> Vec<LiftedSpec<P>> {
    input::time_args::FLAG_SPECS
        .iter()
        .filter(|spec| matches!(spec.short, Some('y' | 'q' | 'd' | 't' | 'z')))
        .map(|spec| lift(time, spec))
        .collect()
}

pub(crate) fn parse_flag_cmd(tail: &[&str]) -> Result<FlagArgs> {
    let registry = flag_args_registry();
    let mut composed = FlagParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    let field = pos
        .first()
        .copied()
        .ok_or_else(|| ParseError::InvalidArg("`datum flag` requires <FIELD>".into()))?;
    let comment = if pos.len() > 1 {
        pos[1..].join(" ")
    } else {
        String::new()
    };
    Ok(FlagArgs {
        field: field.to_string(),
        comment,
        time: composed.time,
    })
}
