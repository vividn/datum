use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_spec_cmd(tail: &[&str]) -> Result<SpecCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing spec subcommand (show|add-state|set|list|remove)".into(),
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
        "add-state" => parse_spec_add_state(rest),
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

/// `spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION] [GLYPH]` — up to
/// five positionals, no flags. The last three are optional; a lone dash
/// (`-`) in a slot leaves it unset while still supplying the ones after it
/// (`add-state mood meh - "so-so"`, `add-state mood meh - - ▼`).
pub(crate) fn parse_spec_add_state(tail: &[&str]) -> Result<SpecCommand> {
    let mut pos: Vec<&str> = Vec::new();
    for arg in tail {
        match split_flag(arg) {
            FlagSplit::Positional(p) => pos.push(p),
            other => return Err(unknown_flag(other)),
        }
    }
    let mut it = pos.into_iter();
    let field = it
        .next()
        .ok_or_else(|| ParseError::MissingRequiredKey("<FIELD>".to_string()))?
        .to_string();
    let name = it
        .next()
        .ok_or_else(|| ParseError::MissingRequiredKey("<NAME>".to_string()))?
        .to_string();
    // `-` in any slot leaves that slot alone.
    let mut slot = || it.next().filter(|v| *v != "-").map(str::to_string);
    let color = slot();
    let description = slot();
    let glyph = slot();
    if let Some(extra) = it.next() {
        return Err(ParseError::InvalidArg(format!(
            "unexpected argument: '{extra}'"
        )));
    }
    Ok(SpecCommand::AddState {
        field,
        name,
        color,
        description,
        glyph,
    })
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
                "spec set: the spec body is `--from-json <JSON>` now — `--json` \
                 is the global `--show json`, so '{extra}' arrived as an argument"
            )));
        }
        return Err(ParseError::InvalidArg(format!(
            "unexpected argument: '{extra}'"
        )));
    }
    out.field = require_pos(&pos.to_vec(), "<FIELD>")?;
    // Declaring a field is the one place a name is chosen, so it is the
    // place to refuse a name nothing can log to (da-b049). Logging stays
    // permissive — a quoted `occur "my coffee"` still works, so nothing
    // already in a database becomes unreachable.
    crate::field_name::check(&out.field)?;
    Ok(out)
}

pub(crate) fn parse_position_f64(s: &str, flag: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| ParseError::InvalidArg(format!("{flag} expects a number, got '{s}'")))
}
