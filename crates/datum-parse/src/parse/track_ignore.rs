use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{local_switch, local_value, LiftedSpec};

pub(crate) fn parse_track_args(tail: &[&str]) -> Result<TrackArgs> {
    #[derive(Default)]
    struct TrackParsing {
        verb: Option<String>,
        color: Option<String>,
        states: Vec<String>,
        tags: Vec<String>,
        keys: Vec<String>,
        required_keys: Vec<String>,
        data_keys: Vec<String>,
        id_keys: Vec<String>,
        remainder: Option<String>,
        string_remainder: bool,
        comment_remainder: bool,
    }
    let registry: Vec<LiftedSpec<TrackParsing>> = vec![
        local_value(None, Some("verb"), |p, v| {
            p.verb = Some(v.to_string());
            Ok(())
        }),
        local_value(None, Some("color"), |p, v| {
            p.color = Some(v.to_string());
            Ok(())
        }),
        local_value(None, Some("state"), |p, v| {
            p.states.push(v.to_string());
            Ok(())
        }),
        local_value(None, Some("tag"), |p, v| {
            p.tags.push(v.to_string());
            Ok(())
        }),
        local_value(None, Some("key"), |p, v| {
            p.keys.push(v.to_string());
            Ok(())
        }),
        local_value(None, Some("required-key"), |p, v| {
            p.required_keys.push(v.to_string());
            Ok(())
        }),
        local_value(Some('k'), None, |p, v| {
            p.data_keys.push(v.to_string());
            Ok(())
        }),
        local_value(Some('K'), None, |p, v| {
            p.id_keys.push(v.to_string());
            Ok(())
        }),
        local_value(Some('R'), Some("remainder"), |p, v| {
            p.remainder = Some(v.to_string());
            Ok(())
        }),
        local_switch(Some('S'), Some("string-remainder"), |p| {
            p.string_remainder = true;
            Ok(())
        }),
        local_switch(Some('C'), Some("comment-remainder"), |p| {
            p.comment_remainder = true;
            Ok(())
        }),
    ];
    let mut composed = TrackParsing::default();
    let mut pos: Vec<&str> = Vec::new();
    let mut assign: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        if let Some(ni) = walk_registry(tail, i, &mut composed, &registry)? {
            i = ni;
            continue;
        }
        match split_flag(tail[i]) {
            // After the field, a `KEY=VALUE` assigns, as on an alias line.
            FlagSplit::Positional(s) if !pos.is_empty() && s.contains('=') => {
                assign.push(s.to_string());
                i += 1;
            }
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Long("default-verb") | FlagSplit::LongAttached("default-verb", _) => {
                return Err(renamed_default_verb())
            }
            f => return Err(unknown_flag(f)),
        }
    }
    // One route for the leftover words (da-zqnt3): `-S` joins what `-R K`
    // sends, and `-C` is the other route, not a second one.
    if composed.string_remainder && composed.remainder.is_none() {
        return Err(ParseError::InvalidArg(
            "-S joins the words -R KEY takes: give -R KEY too (-SR KEY)".into(),
        ));
    }
    if composed.comment_remainder && composed.remainder.is_some() {
        return Err(ParseError::InvalidArg(
            "-C and -R both say where the leftover words go: give one".into(),
        ));
    }
    Ok(TrackArgs {
        field: require_pos(&pos, "<FIELD>")?,
        verb: composed.verb,
        color: composed.color,
        states: composed.states,
        tags: composed.tags,
        keys: composed.keys,
        required_keys: composed.required_keys,
        data_keys: composed.data_keys,
        id_keys: composed.id_keys,
        assign,
        remainder: composed.remainder,
        string_remainder: composed.string_remainder,
        comment_remainder: composed.comment_remainder,
    })
}

pub(crate) fn parse_ignore_args(tail: &[&str]) -> Result<IgnoreArgs> {
    // `ignore` takes only a positional field — no flags. Skip the
    // registry walker and just collect positionals.
    let mut pos: Vec<&str> = Vec::new();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(IgnoreArgs {
        field: require_pos(&pos, "<FIELD>")?,
    })
}
