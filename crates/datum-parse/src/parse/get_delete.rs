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
    all_or_nothing: bool,
    comment: Option<String>,
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
    r.push(local_switch(None, Some("all-or-nothing"), |p| {
        p.all_or_nothing = true;
        Ok(())
    }));
    r.push(local_value(Some('c'), Some("comment"), |p, v| {
        p.comment = Some(v.to_string());
        Ok(())
    }));
    r
}

pub(crate) fn parse_delete_args(tail: &[&str]) -> Result<DeleteArgs> {
    let registry = delete_args_registry();
    let mut composed = DeleteParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(DeleteArgs {
        id_or_slug: optional_pos(&pos)?,
        force: composed.force,
        dry_run: composed.dry_run,
        all_or_nothing: composed.all_or_nothing,
        comment: composed.comment,
        output: composed.output,
    })
}

/// Rejection text for the retired `--window` on `undo`. It took a value that
/// meant something, so a script still passing one is told what replaced it
/// rather than just refused. (`--force` carried no value and is accepted as a
/// no-op — see [`undo_args_registry`].)
pub const UNDO_GATE_REMOVED: &str = "`--window` was removed from `undo` (da-s6no): it no longer refuses on age. It acts on `_LAST`, or on the most recent tombstone when nothing has been touched, warning on stderr when the change is over ten minutes old — and `datum redo` takes the step back.";

#[derive(Default)]
pub(crate) struct UndoParsing {
    output: OutputArgs,
    to_version: Option<u32>,
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
    // The gate these two flags were the halves of is GONE (da-s6no): bare
    // `undo` refused a `_LAST` pointer older than fifteen minutes unless
    // `--force` was passed, and the tombstone fallback looked only inside
    // `--window`. Undo is reversible now that `redo` exists, so the gate cost
    // more than it saved. The two flags part ways here.
    //
    // `--force` is ACCEPTED and does nothing — it is a bare switch, so a
    // script that still carries it means only "don't stop to ask me", and
    // there has been no prompt to skip since da-s6no. `delete --force` is
    // idle for exactly this reason; the two commands stay alike.
    r.push(local_switch(None, Some("force"), |_p| Ok(())));
    // `--window` stays a tombstone: it took a DURATION that meant something,
    // and a script passing one deserves the sentence that says what replaced
    // it rather than a silent change of behaviour.
    r.push(local_value(None, Some("window"), |_p, _v| {
        Err(ParseError::InvalidArg(UNDO_GATE_REMOVED.into()))
    }));
    r
}

pub(crate) fn parse_undo_args(tail: &[&str]) -> Result<UndoArgs> {
    let registry = undo_args_registry();
    let mut composed = UndoParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(UndoArgs {
        id_or_slug: optional_pos(&pos)?,
        to_version: composed.to_version,
        output: composed.output,
    })
}

/// `redo` carries the output family and `--dry-run` — see [`RedoArgs`].
#[derive(Default)]
pub(crate) struct RedoParsing {
    output: OutputArgs,
    dry_run: bool,
}

pub(crate) fn redo_args_registry() -> Vec<LiftedSpec<RedoParsing>> {
    let mut r: Vec<LiftedSpec<RedoParsing>> = input::output_args::FLAG_SPECS
        .iter()
        .map(|spec| lift(|p: &mut RedoParsing| &mut p.output, spec))
        .collect();
    r.push(local_switch(None, Some("dry-run"), |p| {
        p.dry_run = true;
        Ok(())
    }));
    r
}

pub(crate) fn parse_redo_args(tail: &[&str]) -> Result<RedoArgs> {
    let registry = redo_args_registry();
    let mut composed = RedoParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(RedoArgs {
        id_or_slug: optional_pos(&pos)?,
        dry_run: composed.dry_run,
        output: composed.output,
    })
}
