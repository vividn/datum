use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_meta_cmd(tail: &[&str]) -> Result<MetaCommand> {
    // No flag-level knobs — purely positional. Two forms:
    //   `meta show <field>`  → MetaCommand::Show
    //   `meta <field> <text...>` → MetaCommand::Add (text joined on space)
    let mut pos: Vec<&str> = Vec::new();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            f => return Err(unknown_flag(f)),
        }
    }
    if pos.first().copied() == Some("show") {
        let field = pos
            .get(1)
            .copied()
            .ok_or_else(|| ParseError::InvalidArg("`datum meta show` requires <FIELD>".into()))?;
        if pos.len() > 2 {
            return Err(ParseError::InvalidArg(format!(
                "`datum meta show {field}` takes no further positionals (got {} extra)",
                pos.len() - 2
            )));
        }
        return Ok(MetaCommand::Show {
            field: field.to_string(),
        });
    }
    let field = pos
        .first()
        .copied()
        .ok_or_else(|| ParseError::InvalidArg("`datum meta` requires <FIELD>".into()))?;
    if pos.len() < 2 {
        return Err(ParseError::InvalidArg(format!(
            "`datum meta {field}` requires a note (e.g. `datum meta {field} \"renaming X as Y\"`)"
        )));
    }
    Ok(MetaCommand::Add {
        field: field.to_string(),
        text: pos[1..].join(" "),
    })
}
