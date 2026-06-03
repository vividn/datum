use super::core::*;
use crate::cli::*;
use crate::error::Result;
use crate::input::{local_value, LiftedSpec};

pub(crate) fn parse_track_args(tail: &[&str]) -> Result<TrackArgs> {
    #[derive(Default)]
    struct TrackParsing {
        color: Option<String>,
        state_colors: Vec<String>,
        tags: Vec<String>,
    }
    let registry: Vec<LiftedSpec<TrackParsing>> = vec![
        local_value(None, Some("color"), |p, v| {
            p.color = Some(v.to_string());
            Ok(())
        }),
        local_value(None, Some("state"), |p, v| {
            p.state_colors.push(v.to_string());
            Ok(())
        }),
        local_value(None, Some("tag"), |p, v| {
            p.tags.push(v.to_string());
            Ok(())
        }),
    ];
    let mut composed = TrackParsing::default();
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        if let Some(ni) = walk_registry(tail, i, &mut composed, &registry)? {
            i = ni;
            continue;
        }
        match split_flag(tail[i]) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(TrackArgs {
        field: require_pos(&pos, "<FIELD>")?,
        color: composed.color,
        state_colors: composed.state_colors,
        tags: composed.tags,
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
