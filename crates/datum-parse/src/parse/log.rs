use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{self, lift, local_switch, local_value, LiftedSpec};

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
    // Duration concern (-m, plus the `--dur` tombstone; da-azqq).
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
    // A `-K` is a `-k` whose value is also in the slug (v2's `IdKeyAction`):
    // one list, so keyless values fill the slots in the order the flags
    // appear (da-zwp7g).
    r.push(local_value(Some('K'), Some("slug-key"), |p, v| {
        p.keys.push(v.to_string());
        p.push_slug_key(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('F'), Some("field-key"), |p, v| {
        p.field_keys.push(v.to_string());
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
        // joined by the canonical slug separator `__`, with the `-K`s in the
        // order given (da-14ri0) — e.g. `--id %occurTime% --id %comment%`
        // slugs on BOTH (each `%key%` interpolated, `__`-joined).
        //
        // da-es84: each value is normalized first, so the datum-compatible
        // bare-key form (`--id %occurTime`, missing its closing `%`) becomes
        // a well-formed `%occurTime%` placeholder the slug interpolator
        // resolves. Without this the composed `%occurTime__%comment` is
        // malformed — interpolation drops `%occurTime` and only the literal
        // `comment` survives, collapsing the multi-`--id` to the last key.
        p.push_id_part(&normalize_id_segment(v));
        Ok(())
    }));
    r.push(local_value(None, Some("id-delimiter"), |p, v| {
        p.id_delimiter = Some(v.to_string());
        Ok(())
    }));
    // The value is required, as v2's `-L` (da-n86g3): an optional one left
    // `-L happy` taking nothing and `happy` landing in the duration slot.
    // `-l` records the state being left; `-L` claims it (da-ovnf9).
    r.push(local_value(Some('l'), Some("last-state"), |p, v| {
        p.last_state = Some(state_left("-l/--last-state", v)?);
        p.ensure_last_state = false;
        Ok(())
    }));
    r.push(local_value(Some('L'), Some("ensure-last-state"), |p, v| {
        p.last_state = Some(state_left("-L/--ensure-last-state", v)?);
        p.ensure_last_state = true;
        Ok(())
    }));
    r.push(local_value(None, Some("conflict"), |p, v| {
        p.conflict = Some(parse_conflict_strategy(
            "conflict",
            &["update", "merge", "rekey", "ignore"],
            v,
        )?);
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
    r.push(local_switch(None, Some("setup"), |p| {
        p.setup = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("if-changed"), |p| {
        p.if_changed = true;
        Ok(())
    }));
    r
}

/// The state `-l` / `-L` name: required, as v2's `-L` was.
fn state_left(flag: &str, v: &str) -> Result<String> {
    if v.is_empty() {
        return Err(ParseError::InvalidArg(format!(
            "{flag} needs a value: the state being left"
        )));
    }
    Ok(v.to_string())
}

/// Normalize a single `--id` value into one slug-interpolation segment.
///
/// The datum-compatible bare-key form opens a `%key` placeholder but omits
/// its closing `%` (`--id %occurTime`); complete it so the slug interpolator
/// resolves the key. Everything else is kept verbatim: a plain literal
/// (`rawString`, or empty) carries no leading `%`, and a value already
/// holding balanced `%…%` placeholders (`%occur_time%`, `override-%uid%`,
/// `%state%:%uid%`) is a complete template.
fn normalize_id_segment(v: &str) -> String {
    if v.starts_with('%') && v.matches('%').count() % 2 == 1 {
        format!("{v}%")
    } else {
        v.to_string()
    }
}

pub(crate) fn parse_log_args(tail: &[&str]) -> Result<LogArgs> {
    let registry = log_args_registry();
    let mut out = LogArgs::with_field(String::new());
    let (pos, verbatim) = fold_args_split(tail, &mut out, &registry, |_, _, _| Ok(None))?;
    // `occur -- -x`: a field before the line has one is still its field.
    let (pos, verbatim) = match (pos.is_empty(), verbatim.split_first()) {
        (true, Some((field, rest))) => (vec![*field], rest),
        _ => (pos, verbatim),
    };
    out.verbatim = verbatim.iter().map(|s| s.to_string()).collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    // da-es84: multiple `--id` flags in the datum-compatible bare-key form
    // (`%occurTime` with no closing `%`) must each become a `%key%` segment
    // and join on `__` — not collapse to the last key.
    #[test]
    fn multiple_bare_id_flags_compose_into_key_template() {
        let args = parse_log_args(&[
            "field",
            "--id",
            "%occurTime",
            "--id",
            "%comment",
            "comment=abcdefg",
        ])
        .unwrap();
        assert_eq!(args.id.as_deref(), Some("%occurTime%__%comment%"));
    }

    #[test]
    fn normalize_id_segment_completes_open_key_and_keeps_literals() {
        // datum-compat leading-`%` key missing its close → completed.
        assert_eq!(normalize_id_segment("%occurTime"), "%occurTime%");
        assert_eq!(normalize_id_segment("%comment"), "%comment%");
        // Plain literals (no leading `%`) are kept verbatim — `--id` still
        // accepts a raw slug body and the empty string.
        assert_eq!(normalize_id_segment("rawString"), "rawString");
        assert_eq!(normalize_id_segment(""), "");
        // Complete templates are untouched.
        assert_eq!(normalize_id_segment("%occur_time%"), "%occur_time%");
        assert_eq!(normalize_id_segment("override-%uid%"), "override-%uid%");
        assert_eq!(normalize_id_segment("%state%:%uid%"), "%state%:%uid%");
    }
}
