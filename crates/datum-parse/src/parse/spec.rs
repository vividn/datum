use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::Show;

/// The command's name, said once (da-ez40f): `spec` may yet be renamed.
pub const SPEC: &str = "spec";

/// `spec [FIELD] [CHANGES]` (da-ez40f). No field is the listing, a field
/// alone its spec, a field with flags a change, and `--delete` the field's
/// spec document gone.
pub(crate) fn parse_spec_cmd(tail: &[&str]) -> Result<SpecCommand> {
    gone(tail)?;
    let delete = tail.contains(&"--delete");
    let tail: Vec<&str> = tail.iter().copied().filter(|t| *t != "--delete").collect();
    let (mut out, flagged, named) = parse_spec_set(&tail)?;
    let field = out.field.clone();
    match (!named, flagged, delete) {
        (false, false, true) => Ok(SpecCommand::Delete {
            field,
            show: Show::Default,
        }),
        (_, _, true) => Err(ParseError::InvalidArg(format!(
            "--delete takes the field alone: `{SPEC} <FIELD> --delete`"
        ))),
        (false, false, false) => Ok(SpecCommand::Show {
            field,
            show: Show::Default,
        }),
        (false, true, false) => {
            // Declaring a field is the one place a name is chosen, so it is
            // the place to refuse a name nothing can log to (da-b049).
            // Logging stays permissive, and so do reading and deleting: a
            // bad name already in a database can still be put right.
            crate::field_name::check(&field)?;
            Ok(SpecCommand::Set(Box::new(out)))
        }
        (true, _, false) => {
            let mut tags = std::mem::take(&mut out.tags);
            if out != SpecSetArgs::default() {
                return Err(ParseError::MissingRequiredKey("<FIELD>".to_string()));
            }
            if tags.len() > 1 {
                return Err(ParseError::InvalidArg(format!(
                    "`{SPEC}` lists by one --tag"
                )));
            }
            Ok(SpecCommand::List { tag: tags.pop() })
        }
    }
}

/// The subcommands `spec` had, refused by name with the line that does it
/// now. A field may be called any of them, so it is the second positional,
/// which no `spec` line takes, that makes one the old form.
fn gone(tail: &[&str]) -> Result<()> {
    let (Some(&sub), Some(&field)) = (tail.first(), tail.get(1)) else {
        return Ok(());
    };
    if field.starts_with('-') {
        return Ok(());
    }
    let rest = &tail[2..];
    let words = |words: &[&str]| {
        let words: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        crate::alias::expand::shell_join(&words)
    };
    let now = match sub {
        "set" => words(&tail[1..]),
        "show" => words(&[field]),
        "remove" => words(&[field, "--delete"]),
        "add-state" => {
            let name = rest.first().copied().unwrap_or("NAME");
            let mut line = vec![field.to_string(), "--state".into(), "…".into()];
            line.extend(["--state".into(), name.to_string()]);
            // The slots end at the first flag; a lone `-` leaves one alone.
            let slots = rest
                .iter()
                .skip(1)
                .take_while(|w| **w == "-" || !w.starts_with('-'));
            for (flag, v) in ["--state-color", "--state-description", "--state-glyph"]
                .into_iter()
                .zip(slots)
                .filter(|(_, v)| **v != "-")
            {
                line.extend([flag.to_string(), format!("{name}={v}")]);
            }
            crate::alias::expand::shell_join(&line)
        }
        _ => return Ok(()),
    };
    Err(ParseError::InvalidArg(format!(
        "`{SPEC} {sub}` is gone: `{SPEC} {now}`"
    )))
}

/// The flags of a change and the field they change, whether a flag was
/// given, and whether a field was.
pub(crate) fn parse_spec_set(tail: &[&str]) -> Result<(SpecSetArgs, bool, bool)> {
    let mut out = SpecSetArgs::default();
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Long("from-json") => {
                let (v, ni) = take_value(tail, i, None, "--from-json")?;
                out.from_json = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("from-json", v) => {
                out.from_json = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("verb") => {
                let (v, ni) = take_value(tail, i, None, "--verb")?;
                out.verb = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("verb", v) => {
                out.verb = Some(v.to_string());
                i += 1;
            }
            // da-c8mh: the flag is `--verb`. The old spelling is an error
            // that names the new one rather than a silent no-op.
            FlagSplit::Long("default-verb") | FlagSplit::LongAttached("default-verb", _) => {
                return Err(renamed_default_verb())
            }
            FlagSplit::Long("state") => {
                let (v, ni) = take_value(tail, i, None, "--state")?;
                out.states.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state", v) => {
                out.states.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("color") => {
                let (v, ni) = take_value(tail, i, None, "--color")?;
                out.color = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("color", v) => {
                out.color = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state-color") => {
                let (v, ni) = take_value(tail, i, None, "--state-color")?;
                out.state_colors.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state-color", v) => {
                out.state_colors.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state-description") => {
                let (v, ni) = take_value(tail, i, None, "--state-description")?;
                out.state_descriptions.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state-description", v) => {
                out.state_descriptions.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state-key") => {
                let (v, ni) = take_value(tail, i, None, "--state-key")?;
                out.state_keys.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state-key", v) => {
                out.state_keys.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state-lookup-field") => {
                let (v, ni) = take_value(tail, i, None, "--state-lookup-field")?;
                out.state_lookup_field = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state-lookup-field", v) => {
                out.state_lookup_field = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("id-template") => {
                let (v, ni) = take_value(tail, i, None, "--id-template")?;
                out.id_template = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("id-template", v) => {
                out.id_template = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("description") => {
                let (v, ni) = take_value(tail, i, None, "--description")?;
                out.description = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("description", v) => {
                out.description = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("track") => {
                out.track = true;
                i += 1;
            }
            FlagSplit::Long("ignore") => {
                out.ignore = true;
                i += 1;
            }
            FlagSplit::Long("id-key") => {
                let (v, ni) = take_value(tail, i, None, "--id-key")?;
                out.id_keys.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("id-key", v) => {
                out.id_keys.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("data-key") => {
                let (v, ni) = take_value(tail, i, None, "--data-key")?;
                out.data_keys.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("data-key", v) => {
                out.data_keys.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("assign") => {
                let (v, ni) = take_value(tail, i, None, "--assign")?;
                out.assign.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("assign", v) => {
                out.assign.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("id-assign") => {
                let (v, ni) = take_value(tail, i, None, "--id-assign")?;
                out.id_assign.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("id-assign", v) => {
                out.id_assign.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state-assign") => {
                let (v, ni) = take_value(tail, i, None, "--state-assign")?;
                out.state_assign.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state-assign", v) => {
                out.state_assign.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("no-autoalias") => {
                out.no_autoalias = true;
                i += 1;
            }
            FlagSplit::Long("only-listed-keys") => {
                out.only_listed_keys = Some(true);
                i += 1;
            }
            FlagSplit::Long("any-keys") => {
                out.only_listed_keys = Some(false);
                i += 1;
            }
            FlagSplit::Long("only-listed-states") => {
                out.only_listed_states = Some(true);
                i += 1;
            }
            FlagSplit::Long("any-states") => {
                out.only_listed_states = Some(false);
                i += 1;
            }
            FlagSplit::Long("remainder") => {
                let (v, ni) = take_value(tail, i, None, "--remainder")?;
                out.remainder = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("remainder", v) => {
                out.remainder = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("never-false") => {
                out.never_false = Some(true);
                i += 1;
            }
            FlagSplit::Long("allow-false") => {
                out.never_false = Some(false);
                i += 1;
            }
            FlagSplit::Long("y") => {
                let (v, ni) = take_value(tail, i, None, "--y")?;
                out.y = Some(parse_position_f64(v, "--y")?);
                i = ni;
            }
            FlagSplit::LongAttached("y", v) => {
                out.y = Some(parse_position_f64(v, "--y")?);
                i += 1;
            }
            FlagSplit::Long("height") => {
                let (v, ni) = take_value(tail, i, None, "--height")?;
                out.height = Some(parse_position_f64(v, "--height")?);
                i = ni;
            }
            FlagSplit::LongAttached("height", v) => {
                out.height = Some(parse_position_f64(v, "--height")?);
                i += 1;
            }
            FlagSplit::Long("z-index") => {
                let (v, ni) = take_value(tail, i, None, "--z-index")?;
                out.z_index = Some(parse_position_f64(v, "--z-index")?);
                i = ni;
            }
            FlagSplit::LongAttached("z-index", v) => {
                out.z_index = Some(parse_position_f64(v, "--z-index")?);
                i += 1;
            }
            FlagSplit::Long("tag") => {
                let (v, ni) = take_value(tail, i, None, "--tag")?;
                out.tags.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("tag", v) => {
                out.tags.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("glyph") => {
                let (v, ni) = take_value(tail, i, None, "--glyph")?;
                out.glyph = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("glyph", v) => {
                out.glyph = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state-glyph") => {
                let (v, ni) = take_value(tail, i, None, "--state-glyph")?;
                out.state_glyphs.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state-glyph", v) => {
                out.state_glyphs.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("glyph-by") => {
                let (v, ni) = take_value(tail, i, None, "--glyph-by")?;
                out.glyph_by = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("glyph-by", v) => {
                out.glyph_by = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("glyph-map") => {
                let (v, ni) = take_value(tail, i, None, "--glyph-map")?;
                out.glyph_map.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("glyph-map", v) => {
                out.glyph_map.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("icon") => {
                let (v, ni) = take_value(tail, i, None, "--icon")?;
                out.icon = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("icon", v) => {
                out.icon = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("glyph-scale") => {
                let (v, ni) = take_value(tail, i, None, "--glyph-scale")?;
                out.glyph_scale = Some(parse_factor(v, "--glyph-scale")?);
                i = ni;
            }
            FlagSplit::LongAttached("glyph-scale", v) => {
                out.glyph_scale = Some(parse_factor(v, "--glyph-scale")?);
                i += 1;
            }
            FlagSplit::Long("glyph-aspect") => {
                let (v, ni) = take_value(tail, i, None, "--glyph-aspect")?;
                out.glyph_aspect = Some(parse_factor(v, "--glyph-aspect")?);
                i = ni;
            }
            FlagSplit::LongAttached("glyph-aspect", v) => {
                out.glyph_aspect = Some(parse_factor(v, "--glyph-aspect")?);
                i += 1;
            }
            // `-weird` is a field name someone typed, not a flag they
            // misspelled: a leading dash is the only reason it reaches
            // here, and the field slot is still empty. Report the
            // field-name rule so the terminal refuses it the same way the
            // (+) form does — otherwise the three bad names of da-b049 go
            // on failing in three different ways. A `--long` token is a
            // real flag typo and keeps its own message.
            f if pos.is_empty() && !tok.starts_with("--") => {
                crate::field_name::check(tok)?;
                return Err(unknown_flag(f));
            }
            f => return Err(unknown_flag(f)),
        }
    }
    // The spec body used to be `--json <JSON>`; da-qt6z made `--json` the
    // global `--show json`, which takes no value — so the old spelling
    // leaves the body standing here as a second positional. Say where it
    // went rather than drop it on the floor.
    if let Some(extra) = pos.get(1) {
        if extra.trim_start().starts_with('{') {
            return Err(ParseError::InvalidArg(format!(
                "{SPEC}: the spec body is `--from-json <JSON>` now — `--json` \
                 is the global `--show json`, so '{extra}' arrived as an argument"
            )));
        }
        return Err(ParseError::InvalidArg(format!(
            "unexpected argument: '{extra}'"
        )));
    }
    out.field = pos.first().map(|f| f.to_string()).unwrap_or_default();
    Ok((out, tail.len() > pos.len(), !pos.is_empty()))
}

/// A size factor (da-c05v1): a number above zero.
fn parse_factor(s: &str, flag: &str) -> Result<f64> {
    match parse_position_f64(s, flag)? {
        n if n > 0.0 && n.is_finite() => Ok(n),
        _ => Err(ParseError::InvalidArg(format!(
            "{flag} expects a number above 0, got '{s}'"
        ))),
    }
}

pub(crate) fn parse_position_f64(s: &str, flag: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| ParseError::InvalidArg(format!("{flag} expects a number, got '{s}'")))
}
