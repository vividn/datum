use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{self, lift, local_switch, local_value, LiftedSpec, OutputArgs, Show, TimeArgs};

pub(crate) fn parse_grep_args(tail: &[&str]) -> Result<GrepArgs> {
    #[derive(Default)]
    struct GrepParsing {
        output: OutputArgs,
        field: Option<String>,
    }
    let mut registry: Vec<LiftedSpec<GrepParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        registry.push(lift(|p: &mut GrepParsing| &mut p.output, spec));
    }
    registry.push(local_value(Some('f'), Some("field"), |p, v| {
        p.field = Some(v.to_string());
        Ok(())
    }));

    let mut composed = GrepParsing::default();
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
    if pos.is_empty() {
        return Err(ParseError::InvalidArg(
            "grep requires at least one pattern".into(),
        ));
    }
    Ok(GrepArgs {
        patterns: pos.into_iter().map(|s| s.to_string()).collect(),
        field: composed.field,
        output: composed.output,
    })
}

pub(crate) fn parse_check_args(tail: &[&str]) -> Result<CheckArgs> {
    let mut out = CheckArgs::default();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("fix") => out.fix = true,
            FlagSplit::Long("list") => out.list = true,
            FlagSplit::Positional(name) => {
                if !crate::cli::KNOWN_CHECKERS.contains(&name) {
                    return Err(ParseError::InvalidArg(format!(
                        "unknown checker '{name}'. Known: {known}. Run `datum check --list` to see options.",
                        known = crate::cli::KNOWN_CHECKERS.join(", "),
                    )));
                }
                out.checkers.push(name.to_string());
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

pub(crate) fn parse_count_args(tail: &[&str]) -> Result<CountArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut out = CountArgs::default();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Long("group-by") => {
                let (v, ni) = take_value(tail, i, None, "--group-by")?;
                out.group_by = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("group-by", v) => {
                out.group_by = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("since") => {
                let (v, ni) = take_value(tail, i, None, "--since")?;
                out.since = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("since", v) => {
                out.since = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    out.field = optional_pos(&pos)?;
    Ok(out)
}

/// A flag naming both ends overwrites whatever came before it, the timing
/// flags' composed moment included (`-z` is no window flag and stays).
fn set_stats_window(p: &mut StatsArgs, flag: StatsWindowFlag) {
    if matches!(flag, StatsWindowFlag::Period | StatsWindowFlag::AllTime) {
        p.time = TimeArgs {
            timezone: p.time.timezone.take(),
            ..TimeArgs::default()
        };
    }
    p.window.push(flag);
}

fn set_stats_period(p: &mut StatsArgs, period: StatsPeriod) -> Result<()> {
    set_stats_window(p, StatsWindowFlag::Period);
    p.period = Some(period);
    Ok(())
}

/// `-y/-q/-d/-t/-z` plus the stats-local flags. `-T`/`-D` are suppressed:
/// they shape how an entry is *stored*, which a query has no use for.
fn stats_args_registry() -> Vec<LiftedSpec<StatsArgs>> {
    let mut r: Vec<LiftedSpec<StatsArgs>> = Vec::new();
    for spec in input::time_args::FLAG_SPECS {
        match (spec.short, spec.long) {
            (Some('z'), _) => r.push(lift(|p: &mut StatsArgs| &mut p.time, spec)),
            (Some('y'), _) | (Some('q'), _) | (Some('d'), _) | (Some('t'), _) => {
                let mut lifted = lift(|p: &mut StatsArgs| &mut p.time, spec);
                let apply = lifted.apply;
                lifted.apply = Box::new(move |p, v| {
                    apply(p, v)?;
                    set_stats_window(p, StatsWindowFlag::Time);
                    Ok(())
                });
                r.push(lifted);
            }
            _ => {}
        }
    }
    r.push(local_value(None, Some("since"), |p: &mut StatsArgs, v| {
        set_stats_window(p, StatsWindowFlag::Since);
        p.since = Some(v.to_string());
        Ok(())
    }));
    r.push(local_value(None, Some("until"), |p: &mut StatsArgs, v| {
        set_stats_window(p, StatsWindowFlag::Until);
        p.until = Some(v.to_string());
        Ok(())
    }));
    r.push(local_switch(None, Some("all-time"), |p: &mut StatsArgs| {
        set_stats_window(p, StatsWindowFlag::AllTime);
        Ok(())
    }));
    r.push(local_value(
        None,
        Some("group-by"),
        |p: &mut StatsArgs, v| {
            p.group_by = Some(v.to_string());
            Ok(())
        },
    ));
    r.push(local_switch(
        None,
        Some("include-false"),
        |p: &mut StatsArgs| {
            p.include_false = true;
            Ok(())
        },
    ));
    r.push(local_switch(
        None,
        Some("include-untracked"),
        |p: &mut StatsArgs| {
            p.include_untracked = true;
            Ok(())
        },
    ));
    r.push(local_switch(None, Some("today"), |p: &mut StatsArgs| {
        set_stats_period(p, StatsPeriod::Today)
    }));
    r.push(local_switch(None, Some("week"), |p: &mut StatsArgs| {
        set_stats_period(p, StatsPeriod::Week)
    }));
    r.push(local_switch(
        None,
        Some("last-week"),
        |p: &mut StatsArgs| set_stats_period(p, StatsPeriod::LastWeek),
    ));
    r.push(local_switch(None, Some("month"), |p: &mut StatsArgs| {
        set_stats_period(p, StatsPeriod::Month)
    }));
    r.push(local_switch(
        None,
        Some("last-month"),
        |p: &mut StatsArgs| set_stats_period(p, StatsPeriod::LastMonth),
    ));
    r
}

pub(crate) fn parse_stats_args(tail: &[&str]) -> Result<StatsArgs> {
    let registry = stats_args_registry();
    let mut out = StatsArgs::default();
    let pos = fold_args(tail, &mut out, &registry)?;
    // First positional is the `field[:state]` selector; an optional second
    // positional is the shorthand time-slice (`2026`, `2026-06`, …).
    let mut it = pos.into_iter();
    out.selector = it.next().map(|s| s.to_string());
    out.slice = it.next().map(|s| s.to_string());
    if let Some(extra) = it.next() {
        return Err(ParseError::InvalidArg(format!(
            "stats takes at most a selector and a time-slice; unexpected '{extra}'"
        )));
    }
    Ok(out)
}

/// `fields [<FIELD>...] [--include-ignored | -A] [--sort frecency]` — every
/// field, or the ones named (da-gq232).
///
/// `--json` and `--show` are gone from here: da-qt6z folded the output
/// family into the shared layer, which sweeps them out of `tail` before
/// this runs. What stays is `-A` — lifted the way `tail` lifts it, and
/// the engineering spelling of `--include-ignored` (da-1lnf). Its other
/// spelling, `--show all`, arrives through the shared layer and is ORed
/// in by `apply_show`.
pub(crate) fn parse_fields_args(tail: &[&str]) -> Result<FieldsArgs> {
    #[derive(Default)]
    struct FieldsParsing {
        output: OutputArgs,
        include_ignored: bool,
        by_frecency: bool,
    }
    let mut registry: Vec<LiftedSpec<FieldsParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        registry.push(lift(|p: &mut FieldsParsing| &mut p.output, spec));
    }
    registry.push(local_switch(None, Some("include-ignored"), |p| {
        p.include_ignored = true;
        Ok(())
    }));
    registry.push(local_value(None, Some("sort"), |p, v| match v {
        "frecency" => {
            p.by_frecency = true;
            Ok(())
        }
        other => Err(ParseError::InvalidArg(format!(
            "fields --sort takes `frecency`, not '{other}'"
        ))),
    }));

    let mut composed = FieldsParsing::default();
    let fields = fold_args(tail, &mut composed, &registry)?;
    Ok(FieldsArgs {
        show: Show::default(),
        include_ignored: composed.include_ignored || composed.output.show_all,
        by_frecency: composed.by_frecency,
        fields: fields.into_iter().map(str::to_string).collect(),
    })
}

pub(crate) fn parse_states_args(tail: &[&str]) -> Result<StatesArgs> {
    Ok(StatesArgs {
        field: require_pos(&positionals(tail)?, "FIELD")?,
        show: Show::default(),
    })
}

pub(crate) fn parse_ids_args(tail: &[&str]) -> Result<IdsArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut out = IdsArgs::default();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Short('n') | FlagSplit::Long("limit") => {
                let (v, ni) = take_value(tail, i, None, "-n/--limit")?;
                out.limit = Some(parse_usize(v, "-n/--limit")?);
                i = ni;
            }
            FlagSplit::ShortAttached('n', v) | FlagSplit::LongAttached("limit", v) => {
                out.limit = Some(parse_usize(v, "-n/--limit")?);
                i += 1;
            }
            FlagSplit::Long("oldest-first") => {
                out.oldest_first = true;
                i += 1;
            }
            FlagSplit::Long("since") => {
                let (v, ni) = take_value(tail, i, None, "--since")?;
                out.since = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("since", v) => {
                out.since = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("slug") => {
                out.slug = true;
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    out.field = optional_pos(&pos)?;
    Ok(out)
}
