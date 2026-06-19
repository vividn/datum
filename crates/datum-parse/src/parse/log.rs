use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{self, lift, local_optional, local_switch, local_value, LiftedSpec};

// ─── Subcommand parsers ──────────────────────────────────────────────────

/// Build the per-command flag registry for logging verbs (occur, start,
/// stop, switch, add). Lifts the timing/duration/output module specs onto
/// the public `LogArgs` shape directly — no intermediate parsing struct
/// since da-e2e exposed the composed bags publicly.
pub(crate) fn log_args_registry() -> Vec<LiftedSpec<LogArgs>> {
    let mut r: Vec<LiftedSpec<LogArgs>> = Vec::new();
    // Time concern (-y/-q/-d/-t/-z/-T/-D).
    for spec in input::time_args::FLAG_SPECS {
        r.push(lift(|p: &mut LogArgs| &mut p.time, spec));
    }
    // Duration concern (--dur, -m).
    for spec in input::duration_args::FLAG_SPECS {
        r.push(lift(|p: &mut LogArgs| &mut p.duration, spec));
    }
    // Output concern (-A, --show, -o).
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut LogArgs| &mut p.output, spec));
    }
    // Command-scoped extras. We don't lift DataArgs's FLAG_SPECS wholesale
    // because log tracks slug-keys separately from keys; -R/-S/-C are
    // surfaced individually below so daydumb's PRIMARY_VALUE_KEY sentinel
    // can stay in charge when none of them are supplied.
    r.push(local_switch(Some('u'), Some("undo"), |p| {
        p.undo = true;
        Ok(())
    }));
    r.push(local_value(Some('k'), Some("key"), |p, v| {
        p.keys.push(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('K'), Some("slug-key"), |p, v| {
        p.slug_keys.push(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('c'), Some("comment"), |p, v| {
        p.comment.push(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('R'), Some("remainder"), |p, v| {
        p.remainder = Some(v.to_string());
        Ok(())
    }));
    r.push(local_switch(Some('S'), Some("string-remainder"), |p| {
        p.string_remainder = true;
        Ok(())
    }));
    r.push(local_switch(Some('C'), Some("comment-remainder"), |p| {
        p.comment_remainder = true;
        Ok(())
    }));
    r.push(local_switch(Some('I'), Some("string-state"), |p| {
        p.string_state = true;
        Ok(())
    }));
    r.push(local_value(None, Some("id"), |p, v| {
        // da-r17h (task 7): repeated `--id` flags compose into one template,
        // joined by the canonical slug separator `__` — e.g.
        // `--id %occurTime% --id %comment%` slugs on BOTH (each `%key%`
        // interpolated, `__`-joined). A single `--id` is unchanged.
        match &mut p.id {
            Some(existing) => {
                existing.push_str("__");
                existing.push_str(v);
            }
            None => p.id = Some(v.to_string()),
        }
        Ok(())
    }));
    r.push(local_value(None, Some("id-delimiter"), |p, v| {
        p.id_delimiter = Some(v.to_string());
        Ok(())
    }));
    r.push(local_optional(Some('L'), Some("last-state"), |p, v| {
        // Bare → Some(""), `=value` → Some(value). The empty-string sentinel
        // is the trigger for Stop's close-most-recent behaviour.
        p.last_state = Some(v.unwrap_or("").to_string());
        Ok(())
    }));
    r.push(local_value(None, Some("conflict"), |p, v| {
        p.conflict = Some(parse_conflict_strategy(v)?);
        Ok(())
    }));
    r.push(local_switch(None, Some("update"), |p| {
        if p.merge {
            return Err(ParseError::InvalidArg(
                "--update conflicts with --merge".into(),
            ));
        }
        p.update = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("merge"), |p| {
        if p.update {
            return Err(ParseError::InvalidArg(
                "--merge conflicts with --update".into(),
            ));
        }
        p.merge = true;
        Ok(())
    }));
    r.push(local_switch(Some('M'), Some("no-metadata"), |p| {
        p.no_metadata = true;
        Ok(())
    }));
    r.push(local_value(None, Some("alias"), |p, v| {
        if v.is_empty() {
            return Err(ParseError::InvalidArg(
                "--alias requires a non-empty name".into(),
            ));
        }
        p.alias_name = Some(v.to_string());
        Ok(())
    }));
    r.push(local_switch(None, Some("alias-overwrite"), |p| {
        p.alias_overwrite = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("dry-run"), |p| {
        p.dry_run = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("dryrun"), |p| {
        p.dry_run = true;
        Ok(())
    }));
    r
}

pub(crate) fn parse_log_args(tail: &[&str]) -> Result<LogArgs> {
    let registry = log_args_registry();
    let mut out = LogArgs::with_field(String::new());
    let pos = fold_args(tail, &mut out, &registry)?;
    let mut it = pos.into_iter();
    out.field_or_verb = it
        .next()
        .ok_or_else(|| {
            ParseError::InvalidArg("missing required positional: <FIELD_OR_VERB>".into())
        })?
        .to_string();
    out.rest = it.map(|s| s.to_string()).collect();
    Ok(out)
}
