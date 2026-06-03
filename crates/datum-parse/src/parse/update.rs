use super::core::*;
use crate::cli::*;
use crate::error::Result;
use crate::input::{self, lift, local_switch, local_value, LiftedSpec, OutputArgs};

#[derive(Default)]
pub(crate) struct UpdateParsing {
    output: OutputArgs,
    strategy: Option<ConflictStrategyArg>,
    id: Option<String>,
    id_delimiter: Option<String>,
    keys: Vec<String>,
    comment: Vec<String>,
    dry_run: bool,
    strict: bool,
}

pub(crate) fn update_args_registry() -> Vec<LiftedSpec<UpdateParsing>> {
    let mut r: Vec<LiftedSpec<UpdateParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut UpdateParsing| &mut p.output, spec));
    }
    r.push(local_value(None, Some("strategy"), |p, v| {
        p.strategy = Some(parse_conflict_strategy(v)?);
        Ok(())
    }));
    r.push(local_value(None, Some("id"), |p, v| {
        p.id = Some(v.to_string());
        Ok(())
    }));
    r.push(local_value(None, Some("id-delimiter"), |p, v| {
        p.id_delimiter = Some(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('k'), Some("key"), |p, v| {
        p.keys.push(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('c'), Some("comment"), |p, v| {
        p.comment.push(v.to_string());
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

pub(crate) fn parse_update_args(tail: &[&str]) -> Result<UpdateArgs> {
    let registry = update_args_registry();
    let mut composed = UpdateParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    let mut it = pos.into_iter();
    Ok(UpdateArgs {
        id_or_slug: it.next().map(|s| s.to_string()),
        rest: it.map(|s| s.to_string()).collect(),
        strategy: composed.strategy,
        id: composed.id,
        id_delimiter: composed.id_delimiter,
        keys: composed.keys,
        comment: composed.comment,
        dry_run: composed.dry_run,
        strict: composed.strict,
        output: composed.output,
    })
}
