use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{self, lift, local_switch, local_value, LiftedSpec, OutputArgs};

// Internal composed shape for the read-only `get` command.
#[derive(Default)]
pub(crate) struct GetParsing {
    output: OutputArgs,
    pretty: bool,
}

pub(crate) fn get_args_registry() -> Vec<LiftedSpec<GetParsing>> {
    let mut r: Vec<LiftedSpec<GetParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut GetParsing| &mut p.output, spec));
    }
    r.push(local_switch(None, Some("pretty"), |p| {
        p.pretty = true;
        Ok(())
    }));
    r
}

pub(crate) fn parse_get_args(tail: &[&str]) -> Result<GetArgs> {
    let registry = get_args_registry();
    let mut composed = GetParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;

    // da-yhh1: split positionals into the leading quickId/field plus a
    // tail of `k OP v` matcher tokens. The first positional is always
    // the entry/field target; every later positional must parse as a
    // matcher token. A bare later positional with no operator is a
    // user error — `datum get sleep happy` is meaningless (did they
    // mean `state=happy`?).
    let mut id_or_slug = None;
    let mut matchers: Vec<crate::input::Matcher> = Vec::new();
    for (i, tok) in pos.iter().enumerate() {
        if i == 0 {
            id_or_slug = Some((*tok).to_string());
            continue;
        }
        match crate::input::parse_matcher_token(tok).map_err(ParseError::InvalidArg)? {
            Some(m) => matchers.push(m),
            None => {
                return Err(ParseError::InvalidArg(format!(
                    "`datum get` positional '{tok}' is not a matcher (expected k=v, k>v, k~v, or named forms like k=gt=v)"
                )));
            }
        }
    }

    Ok(GetArgs {
        id_or_slug,
        pretty: composed.pretty,
        output: composed.output,
        matchers,
    })
}

#[derive(Default)]
pub(crate) struct DeleteParsing {
    output: OutputArgs,
    force: bool,
    dry_run: bool,
    strict: bool,
}

pub(crate) fn delete_args_registry() -> Vec<LiftedSpec<DeleteParsing>> {
    let mut r: Vec<LiftedSpec<DeleteParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut DeleteParsing| &mut p.output, spec));
    }
    r.push(local_switch(None, Some("force"), |p| {
        p.force = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("dry-run"), |p| {
        p.dry_run = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("strict"), |p| {
        p.strict = true;
        Ok(())
    }));
    r
}

pub(crate) fn parse_delete_args(tail: &[&str]) -> Result<DeleteArgs> {
    let registry = delete_args_registry();
    let mut composed = DeleteParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(DeleteArgs {
        id_or_slug: pos.first().map(|s| s.to_string()),
        force: composed.force,
        dry_run: composed.dry_run,
        strict: composed.strict,
        output: composed.output,
    })
}

#[derive(Default)]
pub(crate) struct UndoParsing {
    output: OutputArgs,
    to_version: Option<u32>,
    force: bool,
    window: Option<String>,
}

pub(crate) fn undo_args_registry() -> Vec<LiftedSpec<UndoParsing>> {
    let mut r: Vec<LiftedSpec<UndoParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut UndoParsing| &mut p.output, spec));
    }
    r.push(local_value(None, Some("to-version"), |p, v| {
        p.to_version = Some(parse_u32(v, "--to-version")?);
        Ok(())
    }));
    r.push(local_switch(None, Some("force"), |p| {
        p.force = true;
        Ok(())
    }));
    r.push(local_value(None, Some("window"), |p, v| {
        p.window = Some(v.to_string());
        Ok(())
    }));
    r
}

pub(crate) fn parse_undo_args(tail: &[&str]) -> Result<UndoArgs> {
    let registry = undo_args_registry();
    let mut composed = UndoParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(UndoArgs {
        id_or_slug: pos.first().map(|s| s.to_string()),
        to_version: composed.to_version,
        force: composed.force,
        window: composed.window.unwrap_or_else(|| "15m".to_string()),
        output: composed.output,
    })
}
