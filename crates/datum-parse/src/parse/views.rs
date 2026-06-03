use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

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
            FlagSplit::Long("n-days") => {
                let (v, ni) = take_value(tail, i, None, "--n-days")?;
                out.n_days = Some(parse_u32(v, "--n-days")?);
                i = ni;
            }
            FlagSplit::LongAttached("n-days", v) => {
                out.n_days = Some(parse_u32(v, "--n-days")?);
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
            FlagSplit::Long("json") => {
                out.json = true;
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
    let mut out = NowviewArgs::default();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Long("history") => {
                let (v, ni) = take_value(tail, i, None, "--history")?;
                out.history = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("history", v) => {
                out.history = Some(v.to_string());
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
                out.height = parse_f64(v, "--height")?;
                i = ni;
            }
            FlagSplit::LongAttached("height", v) => {
                out.height = parse_f64(v, "--height")?;
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
/// Slice 4 (da-d4t.4) — the SQL itself comes from the `views` table at
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
