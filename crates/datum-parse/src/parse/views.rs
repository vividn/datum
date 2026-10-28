use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::LiftedSpec;

pub(crate) fn parse_dayview_args(tail: &[&str]) -> Result<DayviewArgs> {
    let mut out = DayviewArgs::default();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Long("start-date") => {
                let (v, ni) = take_value(tail, i, None, "--start-date")?;
                out.start_date = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("start-date", v) => {
                out.start_date = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("end-date") => {
                let (v, ni) = take_value(tail, i, None, "--end-date")?;
                out.end_date = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("end-date", v) => {
                out.end_date = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('d') => {
                let (v, ni) = take_value(tail, i, None, "-d")?;
                out.end_date = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('d', v) => {
                out.end_date = Some(v.to_string());
                i += 1;
            }
            // `--n-days` is the old spelling, kept unlisted: saved layouts store argv.
            FlagSplit::Long("days" | "n-days") => {
                let (v, ni) = take_value(tail, i, None, "--days")?;
                out.n_days = Some(parse_u32(v, "--days")?);
                i = ni;
            }
            FlagSplit::LongAttached("days" | "n-days", v) => {
                out.n_days = Some(parse_u32(v, "--days")?);
                i += 1;
            }
            FlagSplit::Short('n') => {
                let (v, ni) = take_value(tail, i, None, "-n")?;
                out.n_days = Some(parse_u32(v, "-n")?);
                i = ni;
            }
            FlagSplit::ShortAttached('n', v) => {
                out.n_days = Some(parse_u32(v, "-n")?);
                i += 1;
            }
            FlagSplit::Long("output-file") => {
                let (v, ni) = take_value(tail, i, None, "--output-file")?;
                out.output_file = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("output-file", v) => {
                out.output_file = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('o') => {
                let (v, ni) = take_value(tail, i, None, "-o")?;
                out.output_file = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('o', v) => {
                out.output_file = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("watch") | FlagSplit::Short('w') => {
                out.watch = true;
                i += 1;
            }
            FlagSplit::Long("width") => {
                let (v, ni) = take_value(tail, i, None, "--width")?;
                out.width = Some(parse_f64(v, "--width")?);
                i = ni;
            }
            FlagSplit::LongAttached("width", v) => {
                out.width = Some(parse_f64(v, "--width")?);
                i += 1;
            }
            FlagSplit::Long("height") => {
                let (v, ni) = take_value(tail, i, None, "--height")?;
                out.height = Some(parse_f64(v, "--height")?);
                i = ni;
            }
            FlagSplit::LongAttached("height", v) => {
                out.height = Some(parse_f64(v, "--height")?);
                i += 1;
            }
            FlagSplit::Long("day-height") => {
                let (v, ni) = take_value(tail, i, None, "--day-height")?;
                out.day_height = Some(parse_f64(v, "--day-height")?);
                i = ni;
            }
            FlagSplit::LongAttached("day-height", v) => {
                out.day_height = Some(parse_f64(v, "--day-height")?);
                i += 1;
            }
            FlagSplit::Long("time-axis-height") => {
                let (v, ni) = take_value(tail, i, None, "--time-axis-height")?;
                out.time_axis_height = parse_f64(v, "--time-axis-height")?;
                i = ni;
            }
            FlagSplit::LongAttached("time-axis-height", v) => {
                out.time_axis_height = parse_f64(v, "--time-axis-height")?;
                i += 1;
            }
            FlagSplit::Long("label-width") => {
                let (v, ni) = take_value(tail, i, None, "--label-width")?;
                out.label_width = Some(parse_f64(v, "--label-width")?);
                i = ni;
            }
            FlagSplit::LongAttached("label-width", v) => {
                out.label_width = Some(parse_f64(v, "--label-width")?);
                i += 1;
            }
            FlagSplit::Long("margin") => {
                let (v, ni) = take_value(tail, i, None, "--margin")?;
                out.margin = parse_f64(v, "--margin")?;
                i = ni;
            }
            FlagSplit::LongAttached("margin", v) => {
                out.margin = parse_f64(v, "--margin")?;
                i += 1;
            }
            FlagSplit::Long("orientation") => {
                let (v, ni) = take_value(tail, i, None, "--orientation")?;
                out.orientation = parse_orientation(v)?;
                i = ni;
            }
            FlagSplit::LongAttached("orientation", v) => {
                out.orientation = parse_orientation(v)?;
                i += 1;
            }
            FlagSplit::Long("horizontal") => {
                out.orientation = DayviewOrientation::Horizontal;
                i += 1;
            }
            FlagSplit::Long("vertical") => {
                out.orientation = DayviewOrientation::Vertical;
                i += 1;
            }
            FlagSplit::Long("sample") => {
                let (v, ni) = take_value(tail, i, None, "--sample")?;
                out.sample = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("sample", v) => {
                out.sample = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("digests") => {
                let (v, ni) = take_value(tail, i, None, "--digests")?;
                out.digests = parse_days_back(v, "--digests")?;
                i = ni;
            }
            FlagSplit::LongAttached("digests", v) => {
                out.digests = parse_days_back(v, "--digests")?;
                i += 1;
            }
            FlagSplit::Long("pictures") => {
                let (v, ni) = take_value(tail, i, None, "--pictures")?;
                out.pictures = parse_days_back(v, "--pictures")?;
                i = ni;
            }
            FlagSplit::LongAttached("pictures", v) => {
                out.pictures = parse_days_back(v, "--pictures")?;
                i += 1;
            }
            FlagSplit::Long("data-layer") => {
                out.data_layer = true;
                i += 1;
            }
            FlagSplit::Long("context") => {
                let (v, ni) = take_value(tail, i, None, "--context")?;
                out.context = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("context", v) => {
                out.context = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Positional(_) => {
                return Err(ParseError::InvalidArg(
                    "dayview takes no positional arguments".into(),
                ));
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

/// `--digests`' and `--pictures`' list: days back, comma-separated.
fn parse_days_back(v: &str, flag: &str) -> Result<Vec<u32>> {
    v.split(',').map(|n| parse_u32(n, flag)).collect()
}

pub(crate) fn parse_orientation(s: &str) -> Result<DayviewOrientation> {
    match s {
        "horizontal" | "h" => Ok(DayviewOrientation::Horizontal),
        "vertical" | "v" => Ok(DayviewOrientation::Vertical),
        other => Err(ParseError::InvalidArg(format!(
            "--orientation must be 'horizontal' or 'vertical', got '{other}'"
        ))),
    }
}

pub(crate) fn parse_nowview_args(tail: &[&str]) -> Result<NowviewArgs> {
    // da-5flt: the shared time flags centre the window, as they aim a verb
    // or `tail`. `-T`/`-D` shape an entry, not a moment, and are refused.
    let time: Vec<LiftedSpec<NowviewArgs>> = crate::input::time_args::FLAG_SPECS
        .iter()
        .filter(|s| !matches!(s.short, Some('T') | Some('D')))
        .map(|s| crate::input::lift(|a: &mut NowviewArgs| &mut a.time, s))
        .collect();
    let mut out = NowviewArgs::default();
    let mut i = 0;
    while i < tail.len() {
        if let Some(ni) = walk_registry(tail, i, &mut out, &time)? {
            i = ni;
            continue;
        }
        let tok = tail[i];
        match split_flag(tok) {
            // da-7q8r: `--context` is the name, because a time flag can
            // put the window anywhere and "history" only reads as the
            // past. `--history` is the same flag and keeps working.
            FlagSplit::Long("context" | "history") => {
                let (v, ni) = take_value(tail, i, None, "--context")?;
                out.history = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("context" | "history", v) => {
                out.history = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("labels") => {
                out.labels = true;
                i += 1;
            }
            FlagSplit::Long("now-label") => {
                out.now_label = true;
                i += 1;
            }
            FlagSplit::Long("overlay") => {
                out.overlay = true;
                i += 1;
            }
            FlagSplit::Long("surround") => {
                let (v, ni) = take_value(tail, i, None, "--surround")?;
                out.surround = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("surround", v) => {
                out.surround = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("ahead") => {
                let (v, ni) = take_value(tail, i, None, "--ahead")?;
                out.ahead = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("ahead", v) => {
                out.ahead = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("center") => {
                let (v, ni) = take_value(tail, i, None, "--center")?;
                out.center = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("center", v) => {
                out.center = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("width") => {
                let (v, ni) = take_value(tail, i, None, "--width")?;
                out.width = Some(parse_f64(v, "--width")?);
                i = ni;
            }
            FlagSplit::LongAttached("width", v) => {
                out.width = Some(parse_f64(v, "--width")?);
                i += 1;
            }
            FlagSplit::Long("height") => {
                let (v, ni) = take_value(tail, i, None, "--height")?;
                out.height = Some(parse_f64(v, "--height")?);
                i = ni;
            }
            FlagSplit::LongAttached("height", v) => {
                out.height = Some(parse_f64(v, "--height")?);
                i += 1;
            }
            FlagSplit::Long("margin") => {
                let (v, ni) = take_value(tail, i, None, "--margin")?;
                out.margin = parse_f64(v, "--margin")?;
                i = ni;
            }
            FlagSplit::LongAttached("margin", v) => {
                out.margin = parse_f64(v, "--margin")?;
                i += 1;
            }
            FlagSplit::Long("time-axis-height") => {
                let (v, ni) = take_value(tail, i, None, "--time-axis-height")?;
                out.time_axis_height = parse_f64(v, "--time-axis-height")?;
                i = ni;
            }
            FlagSplit::LongAttached("time-axis-height", v) => {
                out.time_axis_height = parse_f64(v, "--time-axis-height")?;
                i += 1;
            }
            FlagSplit::Long("orientation") => {
                let (v, ni) = take_value(tail, i, None, "--orientation")?;
                out.orientation = parse_orientation(v)?;
                i = ni;
            }
            FlagSplit::LongAttached("orientation", v) => {
                out.orientation = parse_orientation(v)?;
                i += 1;
            }
            FlagSplit::Long("horizontal") => {
                out.orientation = DayviewOrientation::Horizontal;
                i += 1;
            }
            FlagSplit::Long("vertical") => {
                out.orientation = DayviewOrientation::Vertical;
                i += 1;
            }
            FlagSplit::Long("output-file") => {
                let (v, ni) = take_value(tail, i, None, "--output-file")?;
                out.output_file = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("output-file", v) => {
                out.output_file = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('o') => {
                let (v, ni) = take_value(tail, i, None, "-o")?;
                out.output_file = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('o', v) => {
                out.output_file = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("watch") | FlagSplit::Short('w') => {
                out.watch = true;
                i += 1;
            }
            // ── Terminal-mode flags (only meaningful when no `--output-file`).
            // Per da-57dj `dm nowview` defaults to a pure visual band-stack
            // in the terminal; these knobs control that renderer.
            FlagSplit::Long("window") => {
                let (v, ni) = take_value(tail, i, None, "--window")?;
                out.window = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("window", v) => {
                out.window = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("cols") => {
                let (v, ni) = take_value(tail, i, None, "--cols")?;
                out.cols = Some(parse_f64(v, "--cols")? as u16);
                i = ni;
            }
            FlagSplit::LongAttached("cols", v) => {
                out.cols = Some(parse_f64(v, "--cols")? as u16);
                i += 1;
            }
            FlagSplit::Long("rows") => {
                let (v, ni) = take_value(tail, i, None, "--rows")?;
                out.rows = Some(parse_f64(v, "--rows")? as u16);
                i = ni;
            }
            FlagSplit::LongAttached("rows", v) => {
                out.rows = Some(parse_f64(v, "--rows")? as u16);
                i += 1;
            }
            FlagSplit::Long("no-color") => {
                out.no_color = true;
                i += 1;
            }
            FlagSplit::Positional(_) => {
                return Err(ParseError::InvalidArg(
                    "nowview takes no positional arguments".into(),
                ));
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

/// `datum view <name> [--format pretty|tsv|csv|json] [--watch]`.
///
/// One required positional (the view name); everything else is a flag.
/// Slice 4 (da-d4t.4) — the SQL itself comes from the view's entry at
/// dispatch time, not from argv.
pub(crate) fn parse_view_args(tail: &[&str]) -> Result<ViewArgs> {
    let mut name: Option<String> = None;
    let mut out = ViewArgs::default();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                if name.is_some() {
                    return Err(ParseError::InvalidArg(format!(
                        "view takes a single name positional, got extra '{s}'"
                    )));
                }
                name = Some(s.to_string());
                i += 1;
            }
            FlagSplit::Long("format") | FlagSplit::Short('f') => {
                let (v, ni) = take_value(tail, i, None, "--format")?;
                out.format = parse_view_format(v)?;
                i = ni;
            }
            FlagSplit::LongAttached("format", v) | FlagSplit::ShortAttached('f', v) => {
                out.format = parse_view_format(v)?;
                i += 1;
            }
            FlagSplit::Long("watch") | FlagSplit::Short('w') => {
                out.watch = true;
                i += 1;
            }
            FlagSplit::Long("list") => {
                out.list = true;
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if out.list {
        // --list short-circuits in the command layer; a stray positional
        // is a typo signal, not silently ignored.
        if let Some(extra) = name {
            return Err(ParseError::InvalidArg(format!(
                "view --list does not take a name positional, got '{extra}'"
            )));
        }
        return Ok(out);
    }
    let name = name.ok_or_else(|| {
        ParseError::InvalidArg("view requires a name (e.g. `datum view chores_due`)".into())
    })?;
    out.name = name;
    Ok(out)
}

pub(crate) fn parse_view_format(s: &str) -> Result<ViewFormat> {
    match s {
        "auto" => Ok(ViewFormat::Auto),
        "pretty" => Ok(ViewFormat::Pretty),
        "tsv" => Ok(ViewFormat::Tsv),
        "csv" => Ok(ViewFormat::Csv),
        "json" => Ok(ViewFormat::Json),
        other => Err(ParseError::InvalidArg(format!(
            "--format must be one of pretty|tsv|csv|json (or auto), got '{other}'"
        ))),
    }
}
