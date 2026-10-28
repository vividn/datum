use crate::cli::*;
use crate::error::{ParseError, Result};

/// `track F [CHANGES]` is `setup F [CHANGES] --track`, and `ignore` the same
/// with `--ignore` (da-ijf3p): `setup`'s flags, and none of their own.
pub(crate) fn parse_status(tail: &[&str], track: bool) -> Result<Box<SpecSetArgs>> {
    let (mut out, flagged, named) = super::spec::parse_spec_set(tail)?;
    if !named {
        return Err(ParseError::MissingRequiredKey("<FIELD>".to_string()));
    }
    if !out.list_tags.is_empty() {
        return Err(super::spec::gone_list("tag", "T", "--tags a,b"));
    }
    // A change declares the field, as `setup`'s does (da-b049); a bare
    // `track` or `ignore` of a bad name already in the database still puts
    // it right. `a,b` is a list, each name held to the rule alone, and an
    // empty one is none (da-klzka).
    for name in out.field.split(',') {
        if flagged || name.is_empty() {
            crate::field_name::check(name)?;
        }
    }
    out.track |= track;
    out.ignore |= !track;
    Ok(Box::new(out))
}

/// One route for the leftover words (da-zqnt3): `-S` joins what `-R K`
/// sends, and `-C` is the other route, not a second one.
pub(crate) fn check_remainder_route(remainder: bool, string: bool, comment: bool) -> Result<()> {
    if string && !remainder {
        return Err(ParseError::InvalidArg(
            "-S joins the words -R KEY takes: give -R KEY too (-SR KEY)".into(),
        ));
    }
    if comment && remainder {
        return Err(ParseError::InvalidArg(
            "-C and -R both say where the leftover words go: give one".into(),
        ));
    }
    Ok(())
}
