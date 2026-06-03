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
    let mut r: Vec<LiftedSpec<FlagParsing>> = Vec::new();
    // Mirror TailParsing: expose -y/-q/-d/-t/-z; suppress -T/-D
    // (a flag without a moment loses its meaning).
    for spec in input::time_args::FLAG_SPECS {
        match (spec.short, spec.long) {
            (Some('y'), _) | (Some('q'), _) | (Some('d'), _) | (Some('t'), _) | (Some('z'), _) => {
                r.push(lift(|p: &mut FlagParsing| &mut p.time, spec));
            }
            _ => {}
        }
    }
    r
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
