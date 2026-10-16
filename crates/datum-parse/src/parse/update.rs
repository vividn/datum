use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{self, lift, local_switch, local_value, LiftedSpec, OutputArgs};

#[derive(Default)]
pub(crate) struct UpdateParsing {
    output: OutputArgs,
    strategy: Option<UpdateStrategyArg>,
    id: Option<String>,
    id_delimiter: Option<String>,
    keys: Vec<String>,
    comment: Vec<String>,
    dry_run: bool,
    all_or_nothing: bool,
}

pub(crate) fn update_args_registry() -> Vec<LiftedSpec<UpdateParsing>> {
    let mut r: Vec<LiftedSpec<UpdateParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut UpdateParsing| &mut p.output, spec));
    }
    r.push(local_value(None, Some("strategy"), |p, v| {
        p.strategy = Some(parse_update_strategy(v)?);
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
    r.push(local_switch(None, Some("all-or-nothing"), |p| {
        p.all_or_nothing = true;
        Ok(())
    }));
    r
}

/// update, merge and rekey are daydumb's own, and preferNew is merge by v2's
/// name; every other name is its row of the combineData table (datum's own
/// `update` and `merge` are not these).
fn parse_update_strategy(v: &str) -> Result<UpdateStrategyArg> {
    use crate::combine::{Combine, STRATEGIES};
    Ok(match v.to_ascii_lowercase().as_str() {
        "update" => UpdateStrategyArg::Update,
        "merge" | "prefernew" => UpdateStrategyArg::Merge,
        "rekey" => UpdateStrategyArg::Rekey,
        _ => UpdateStrategyArg::Combine(Combine::named(v).ok_or_else(|| {
            let names = STRATEGIES
                .iter()
                .map(|(n, _)| *n)
                .filter(|n| !["update", "merge"].contains(n));
            let all: Vec<&str> = ["update", "merge", "rekey"]
                .into_iter()
                .chain(names)
                .collect();
            ParseError::InvalidArg(format!(
                "invalid --strategy value: '{v}' (expected {})",
                all.join("|")
            ))
        })?),
    })
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
        all_or_nothing: composed.all_or_nothing,
        output: composed.output,
    })
}
