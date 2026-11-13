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
    comment_remainder: bool,
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
    r.push(local_switch(Some('C'), Some("comment-remainder"), |p| {
        p.comment_remainder = true;
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

/// update, merge and rekey are `datum update`'s, `datum merge`'s and
/// `datum rekey`'s; every other name is its row of the combineData table
/// (datum's own `update` is not this one).
fn parse_update_strategy(v: &str) -> Result<UpdateStrategyArg> {
    use crate::combine::{Combine, STRATEGIES};
    Ok(match v.to_ascii_lowercase().as_str() {
        "update" => UpdateStrategyArg::Update,
        "merge" => UpdateStrategyArg::Merge,
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
    let (pos, verbatim) = fold_args_split(tail, &mut composed, &registry, |_, _, _| Ok(None))?;
    let mut it = pos.into_iter();
    Ok(UpdateArgs {
        id_or_slug: it.next().map(|s| s.to_string()),
        rest: it.map(|s| s.to_string()).collect(),
        verbatim: verbatim.iter().map(|s| s.to_string()).collect(),
        strategy: composed.strategy,
        id: composed.id,
        id_delimiter: composed.id_delimiter,
        keys: composed.keys,
        comment: composed.comment,
        comment_remainder: composed.comment_remainder,
        dry_run: composed.dry_run,
        all_or_nothing: composed.all_or_nothing,
        output: composed.output,
    })
}

/// `comment <hid> words…`: `update <hid> --strategy append -C words…`,
/// a remark made later about an existing entry (da-7sc0u).
pub(crate) fn parse_comment_args(tail: &[&str]) -> Result<UpdateArgs> {
    let mut args = parse_update_args(tail)?;
    if args.strategy.is_some() {
        return Err(ParseError::InvalidArg(
            "comment takes no --strategy: it is update --strategy append -C".into(),
        ));
    }
    if args.rest.is_empty() && args.comment.is_empty() && args.verbatim.is_empty() {
        return Err(ParseError::InvalidArg(
            "comment takes the words of the comment: comment <ref> words…".into(),
        ));
    }
    args.strategy = Some(UpdateStrategyArg::Combine(crate::combine::Combine::APPEND));
    args.comment_remainder = true;
    Ok(args)
}

/// `datum script`'s line (da-ygfeh).
pub(crate) fn parse_script_args(tail: &[&str]) -> Result<ScriptArgs> {
    let mut args = ScriptArgs::default();
    let mut it = tail.iter();
    while let Some(tok) = it.next() {
        let (name, given) = match split_flag(tok) {
            FlagSplit::Long("atomic") => {
                args.atomic = true;
                continue;
            }
            FlagSplit::Long("allow-partial") => {
                args.allow_partial = true;
                continue;
            }
            FlagSplit::Long("dry-run" | "dryrun") => {
                args.dry_run = true;
                continue;
            }
            FlagSplit::Long(n @ ("now" | "tz" | "db")) => (n, it.next().copied()),
            FlagSplit::LongAttached(n @ ("now" | "tz" | "db"), v) => (n, Some(v)),
            FlagSplit::Positional(f) if args.file.is_none() => {
                args.file = Some(f.into());
                continue;
            }
            FlagSplit::Positional(_) => {
                return Err(ParseError::InvalidArg(
                    "script takes one file: datum script <FILE>".into(),
                ))
            }
            f => return Err(unknown_flag(f)),
        };
        let slot = match name {
            "now" => &mut args.now,
            "tz" => &mut args.tz,
            _ => &mut args.db,
        };
        let given =
            given.ok_or_else(|| ParseError::InvalidArg(format!("--{name} requires a value")))?;
        *slot = Some(given.to_string());
    }
    if args.atomic && args.allow_partial {
        return Err(ParseError::InvalidArg(
            "script: --atomic writes every line or none, and --allow-partial the lines that run: \
             not both"
                .into(),
        ));
    }
    Ok(args)
}

/// `import [FILE|-] [--allow-partial] [--dry-run] [--tz ZONE] [--schema]`.
pub(crate) fn parse_import_args(tail: &[&str]) -> Result<ImportArgs> {
    let mut args = ImportArgs::default();
    let mut it = tail.iter();
    while let Some(tok) = it.next() {
        match split_flag(tok) {
            FlagSplit::Long("allow-partial") => args.allow_partial = true,
            FlagSplit::Long("dry-run" | "dryrun") => args.dry_run = true,
            FlagSplit::Long("schema") => args.schema = true,
            FlagSplit::Long("tz") => {
                let zone = it
                    .next()
                    .ok_or_else(|| ParseError::InvalidArg("--tz requires a value".into()))?;
                args.tz = Some(zone.to_string());
            }
            FlagSplit::LongAttached("tz", v) => args.tz = Some(v.to_string()),
            FlagSplit::Positional(f) if args.file.is_none() => args.file = Some(f.into()),
            FlagSplit::Positional(_) => {
                return Err(ParseError::InvalidArg(
                    "import takes one file: datum import <FILE>, or `-` for stdin".into(),
                ))
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(args)
}

/// `export --jsonl [FIELD…]`: `--jsonl` is the one format yet, and named, so
/// another can come beside it.
pub(crate) fn parse_export_args(tail: &[&str]) -> Result<ExportArgs> {
    let mut args = ExportArgs::default();
    let mut jsonl = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("jsonl") => jsonl = true,
            FlagSplit::Positional(f) => args.fields.push(f.to_string()),
            f => return Err(unknown_flag(f)),
        }
    }
    if !jsonl {
        return Err(ParseError::InvalidArg(
            "export takes a format: datum export --jsonl [FIELD…]".into(),
        ));
    }
    Ok(args)
}

/// `rename OLD NEW [--merge] [--dry-run]`, or `rename FIELD --state OLD NEW`.
pub(crate) fn parse_rename_args(tail: &[&str]) -> Result<RenameArgs> {
    let (mut dry_run, mut merge, mut state) = (false, false, false);
    let mut pos = Vec::new();
    for (i, tok) in tail.iter().enumerate() {
        match split_flag(tok) {
            FlagSplit::Long("dry-run") => dry_run = true,
            FlagSplit::Long("merge") => merge = true,
            FlagSplit::Long("state") => state = true,
            FlagSplit::Positional("--") => {
                pos.extend_from_slice(&tail[i + 1..]);
                break;
            }
            FlagSplit::Positional(s) => pos.push(s),
            f => return Err(unknown_flag(f)),
        }
    }
    let args = |field: Option<&str>, old: &str, new: &str| RenameArgs {
        old: old.into(),
        new: new.into(),
        field: field.map(Into::into),
        dry_run,
        merge,
    };
    match (state, &pos[..]) {
        (false, [old, new]) => Ok(args(None, old, new)),
        (true, [field, old, new]) => Ok(args(Some(field), old, new)),
        (false, _) => Err(ParseError::InvalidArg(
            "rename takes two fields: datum rename <OLD> <NEW>".into(),
        )),
        (true, _) => Err(ParseError::InvalidArg(
            "rename --state takes a field and two states: datum rename <FIELD> --state <OLD> <NEW>"
                .into(),
        )),
    }
}
