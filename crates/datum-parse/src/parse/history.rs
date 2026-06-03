use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{self, lift, local_switch, local_value, LiftedSpec, OutputArgs};

#[derive(Default)]
pub(crate) struct HistoryParsing {
    version: Option<u32>,
    diff: Option<Vec<u32>>,
    json: bool,
}

pub(crate) fn history_args_registry() -> Vec<LiftedSpec<HistoryParsing>> {
    let mut r: Vec<LiftedSpec<HistoryParsing>> = Vec::new();
    r.push(local_value(None, Some("version"), |p, v| {
        p.version = Some(parse_u32(v, "--version")?);
        Ok(())
    }));
    r.push(local_switch(None, Some("json"), |p| {
        p.json = true;
        Ok(())
    }));
    // `--diff` consumes TWO detached values — handled by the caller as a
    // pre-walker special case; not represented here.
    r
}

pub(crate) fn parse_history_args(tail: &[&str]) -> Result<HistoryArgs> {
    let registry = history_args_registry();
    let mut composed = HistoryParsing::default();
    // `--diff A B` — two-arg form, no central-registry support today.
    let pos = fold_args_with_pre(tail, &mut composed, &registry, |tail, i, p| {
        if matches!(split_flag(tail[i]), FlagSplit::Long("diff")) {
            let a = tail
                .get(i + 1)
                .ok_or_else(|| ParseError::InvalidArg("--diff requires two values".into()))?;
            let b = tail
                .get(i + 2)
                .ok_or_else(|| ParseError::InvalidArg("--diff requires two values".into()))?;
            p.diff = Some(vec![parse_u32(a, "--diff")?, parse_u32(b, "--diff")?]);
            return Ok(Some(i + 3));
        }
        Ok(None)
    })?;
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(HistoryArgs {
        id_or_slug,
        version: composed.version,
        diff: composed.diff,
        json: composed.json,
    })
}

pub(crate) fn parse_why_args(tail: &[&str]) -> Result<WhyArgs> {
    #[derive(Default)]
    struct WhyParsing {
        full: bool,
    }
    let registry: Vec<LiftedSpec<WhyParsing>> = vec![local_switch(None, Some("full"), |p| {
        p.full = true;
        Ok(())
    })];
    let mut composed = WhyParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(WhyArgs {
        id_or_slug,
        full: composed.full,
    })
}

pub(crate) fn parse_edit_args(tail: &[&str]) -> Result<EditArgs> {
    let mut composed = OutputArgs::default();
    let registry: Vec<LiftedSpec<OutputArgs>> = input::output_args::FLAG_SPECS
        .iter()
        .map(|s| lift(|p: &mut OutputArgs| p, s))
        .collect();
    let pos = fold_args(tail, &mut composed, &registry)?;
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(EditArgs {
        id_or_slug,
        output: composed,
    })
}
