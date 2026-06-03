use super::core::*;
use crate::cli::*;
use crate::error::Result;
use crate::input::{self, lift, local_switch, local_value, LiftedSpec};

pub(crate) fn parse_now_args(tail: &[&str]) -> Result<NowArgs> {
    // `now` accepts the standard time_args (`-t/-d/-y/-q/-z`) for
    // time-travel (da-tx8j), the disclosure-layer pair (`-a/--tracked`,
    // `-A/--all`), and its own band-rendering flags. We do NOT pull in
    // OutputArgs::FLAG_SPECS — `now` historically only used `-A` from
    // there, and reusing the `--show-all` long name would conflict with
    // the new `--all` spelling, which vividn asked for in the rebind.
    #[derive(Default)]
    struct NowParsing {
        time: input::TimeArgs,
        show_tracked: bool,
        show_all: bool,
        json: bool,
        bare: bool,
        no_color: bool,
        cols: Option<u16>,
    }
    let mut registry: Vec<LiftedSpec<NowParsing>> = Vec::new();
    for spec in input::time_args::FLAG_SPECS {
        registry.push(lift(|p: &mut NowParsing| &mut p.time, spec));
    }
    registry.push(local_switch(Some('a'), Some("tracked"), |p| {
        p.show_tracked = true;
        Ok(())
    }));
    registry.push(local_switch(Some('A'), Some("all"), |p| {
        p.show_all = true;
        Ok(())
    }));
    registry.push(local_switch(None, Some("json"), |p| {
        p.json = true;
        Ok(())
    }));
    registry.push(local_switch(None, Some("bare"), |p| {
        p.bare = true;
        Ok(())
    }));
    registry.push(local_switch(None, Some("no-color"), |p| {
        p.no_color = true;
        Ok(())
    }));
    registry.push(local_value(None, Some("cols"), |p, v| {
        p.cols = Some(parse_f64(v, "--cols")? as u16);
        Ok(())
    }));

    let mut composed = NowParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    if let Some(extra) = pos.first() {
        return Err(unknown_flag(FlagSplit::Positional(extra)));
    }
    Ok(NowArgs {
        show_all: composed.show_all,
        show_tracked: composed.show_tracked,
        json: composed.json,
        bare: composed.bare,
        no_color: composed.no_color,
        cols: composed.cols,
        time: composed.time,
    })
}
