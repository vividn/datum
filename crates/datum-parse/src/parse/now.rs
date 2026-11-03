use super::core::*;
use super::tail::field_list;
use crate::cli::*;
use crate::error::Result;
use crate::input::{self, lift, local_switch, local_value, LiftedSpec, Show};

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
        bare: bool,
        no_color: bool,
        context: Option<String>,
        watch: bool,
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
    registry.push(local_switch(None, Some("bare"), |p| {
        p.bare = true;
        Ok(())
    }));
    registry.push(local_switch(None, Some("no-color"), |p| {
        p.no_color = true;
        Ok(())
    }));
    // da-apbrm: the band's length and its redraw, which `nowview --window`
    // and `-w` gave the terminal band before nowview became the SVG alone.
    registry.push(local_value(None, Some("context"), |p, v| {
        p.context = Some(v.to_string());
        Ok(())
    }));
    registry.push(local_switch(Some('w'), Some("watch"), |p| {
        p.watch = true;
        Ok(())
    }));

    let mut composed = NowParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(NowArgs {
        fields: optional_pos(&pos)?
            .as_deref()
            .map(field_list)
            .unwrap_or_default(),
        show_all: composed.show_all,
        show_tracked: composed.show_tracked,
        show: Show::default(),
        bare: composed.bare,
        no_color: composed.no_color,
        context: composed.context,
        watch: composed.watch,
        time: composed.time,
    })
}
