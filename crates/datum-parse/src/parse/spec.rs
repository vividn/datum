use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_spec_cmd(tail: &[&str]) -> Result<SpecCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing spec subcommand (show|set|list|remove)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "show" => {
            // `spec show <FIELD> [--text]` — parse a single positional
            // (the field) plus optional --text flag (da-nhzo).
            let mut field: Option<String> = None;
            let mut text = false;
            for arg in rest {
                match split_flag(arg) {
                    FlagSplit::Long("text") => text = true,
                    FlagSplit::LongAttached("text", _) => text = true,
                    FlagSplit::Positional(p) => {
                        if field.is_none() {
                            field = Some(p.to_string());
                        } else {
                            return Err(ParseError::InvalidArg(format!(
                                "unexpected argument: '{p}'"
                            )));
                        }
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let field =
                field.ok_or_else(|| ParseError::MissingRequiredKey("<FIELD>".to_string()))?;
            Ok(SpecCommand::Show { field, text })
        }
        "set" => Ok(SpecCommand::Set(Box::new(parse_spec_set(rest)?))),
        "list" => Ok(SpecCommand::List {
            tag: parse_spec_list_tag(rest)?,
        }),
        "remove" => {
            let field = require_pos(rest, "<FIELD>")?;
            Ok(SpecCommand::Remove { field })
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown spec subcommand: '{other}'"
        ))),
    }
}

pub(crate) fn parse_spec_list_tag(tail: &[&str]) -> Result<Option<String>> {
    let mut tag: Option<String> = None;
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("tag") => {
                let (v, ni) = take_value(tail, i, None, "--tag")?;
                tag = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("tag", v) => {
                tag = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(tag)
}

pub(crate) fn parse_spec_set(tail: &[&str]) -> Result<SpecSetArgs> {
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
            FlagSplit::Long("json") => {
                let (v, ni) = take_value(tail, i, None, "--json")?;
                out.json = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("json", v) => {
                out.json = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("default-verb") => {
                let (v, ni) = take_value(tail, i, None, "--default-verb")?;
                out.default_verb = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("default-verb", v) => {
                out.default_verb = Some(v.to_string());
                i += 1;
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
            FlagSplit::Long("unit") => {
                let (v, ni) = take_value(tail, i, None, "--unit")?;
                out.unit = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("unit", v) => {
                out.unit = Some(v.to_string());
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
            FlagSplit::Long("no-autoalias") => {
                out.no_autoalias = true;
                i += 1;
            }
            FlagSplit::Long("strict") => {
                out.strict = Some(true);
                i += 1;
            }
            FlagSplit::Long("no-strict") => {
                out.strict = Some(false);
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
            f => return Err(unknown_flag(f)),
        }
    }
    out.field = require_pos(&pos.to_vec(), "<FIELD>")?;
    Ok(out)
}

pub(crate) fn parse_position_f64(s: &str, flag: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| ParseError::InvalidArg(format!("{flag} expects a number, got '{s}'")))
}
