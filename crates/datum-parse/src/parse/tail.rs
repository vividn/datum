use super::core::*;
use crate::cli::*;
use crate::error::Result;
use crate::input::{
    self, lift, local_optional, local_switch, local_value, LiftedSpec, OutputArgs, TimeArgs,
};

// Internal composed shape for tail/head.
#[derive(Default)]
pub(crate) struct TailParsing {
    time: TimeArgs,
    output: OutputArgs,
    extras: TailExtras,
}

#[derive(Default)]
pub(crate) struct TailExtras {
    count: Option<usize>,
    metric: Option<String>,
    reverse: bool,
    columns: Vec<String>,
    oldest_first: bool,
    table: bool,
    watch: Option<f64>,
    show_meta: bool,
}

pub(crate) fn tail_args_registry() -> Vec<LiftedSpec<TailParsing>> {
    let mut r: Vec<LiftedSpec<TailParsing>> = Vec::new();
    // Time (-y/-q/-d/-t/-z). `-T`/`-D` aren't exposed — tail is a read
    // surface and "no-timestamp" / "full-day" only apply at write time.
    // da-nxm6: `-z`/`--timezone` is now admitted so the day-window
    // builder downstream can compute boundaries in the user's zone
    // rather than the host's `Local`.
    for spec in input::time_args::FLAG_SPECS {
        match (spec.short, spec.long) {
            (Some('y'), _) | (Some('q'), _) | (Some('d'), _) | (Some('t'), _) | (Some('z'), _) => {
                r.push(lift(|p: &mut TailParsing| &mut p.time, spec));
            }
            _ => {}
        }
    }
    // Output.
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut TailParsing| &mut p.output, spec));
    }
    // Tail-scoped extras.
    r.push(local_value(Some('n'), None, |p, v| {
        p.extras.count = Some(parse_usize(v, "-n")?);
        Ok(())
    }));
    r.push(local_value(Some('m'), Some("metric"), |p, v| {
        p.extras.metric = Some(v.to_string());
        Ok(())
    }));
    r.push(local_switch(Some('r'), Some("reverse"), |p| {
        p.extras.reverse = true;
        Ok(())
    }));
    r.push(local_value(None, Some("column"), |p, v| {
        p.extras.columns.push(v.to_string());
        Ok(())
    }));
    r.push(local_switch(None, Some("oldest-first"), |p| {
        p.extras.oldest_first = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("table"), |p| {
        p.extras.table = true;
        Ok(())
    }));
    // da-ghlx: `--show-meta` (alias `--show-all-metadata`) opts into
    // the review-marker / protocol-note rows that the live tail
    // surface hides by default.
    r.push(local_switch(None, Some("show-meta"), |p| {
        p.extras.show_meta = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("show-all-metadata"), |p| {
        p.extras.show_meta = true;
        Ok(())
    }));
    // `-w/--watch` is special — bare or attached or detached non-flag value.
    // The detached-peek form is handled by the caller after the registry walker.
    r.push(local_optional(Some('w'), Some("watch"), |p, v| {
        match v {
            Some(s) => p.extras.watch = Some(parse_f64(s, "-w/--watch")?),
            None => p.extras.watch = Some(1.0),
        }
        Ok(())
    }));
    r
}

pub(crate) fn parse_tail_args(tail: &[&str]) -> Result<TailArgs> {
    let registry = tail_args_registry();
    let mut composed = TailParsing::default();
    // `-w/--watch <N>` (detached value form): when the bare flag is
    // followed by a non-flag token, consume it as the watch interval.
    // The OptionalAttached registry entry handles bare and `=value`.
    let pos = fold_args_with_pre(tail, &mut composed, &registry, |tail, i, p| {
        if matches!(
            split_flag(tail[i]),
            FlagSplit::Short('w') | FlagSplit::Long("watch")
        ) {
            if let Some(next) = tail.get(i + 1).copied().filter(|s| !looks_like_flag(s)) {
                p.extras.watch = Some(parse_f64(next, "-w/--watch")?);
                return Ok(Some(i + 2));
            }
        }
        Ok(None)
    })?;
    // Single positional may be:
    // * `field` — bare field.
    // * `field,project` — comma-separated multi-field list (da-hrnp).
    // * `field:state` — state-block filter (da-plbb); state may itself
    //   carry commas (`field:s1,s2`) for parallel-state shapes.
    // The presence of `:` flips the comma's meaning from field-separator
    // to state-element-separator, so the two forms can't be combined.
    let (fields, state_filter) = match pos.first() {
        None => (Vec::new(), None),
        Some(s) => split_field_state(s),
    };
    Ok(TailArgs {
        fields,
        state_filter,
        count: composed.extras.count,
        metric: composed
            .extras
            .metric
            .unwrap_or_else(|| "hybrid".to_string()),
        reverse: composed.extras.reverse,
        columns: composed.extras.columns,
        time: composed.time,
        oldest_first: composed.extras.oldest_first,
        watch: composed.extras.watch,
        table: composed.extras.table,
        show_meta: composed.extras.show_meta,
        output: composed.output,
    })
}

/// Split a tail positional into `(fields, state_filter)`.
///
/// * Plain `field` or `f1,f2` → comma-split list of fields, no state filter.
/// * `field:state` → single field plus state filter. The `:` flips the
///   comma's role from field-separator to parallel-state-separator.
/// * `field:` (trailing colon, empty state) → bare field, no filter.
pub(crate) fn split_field_state(raw: &str) -> (Vec<String>, Option<Vec<String>>) {
    match raw.split_once(':') {
        None => {
            let fields = raw
                .split(',')
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect::<Vec<_>>();
            (fields, None)
        }
        Some((field, "")) => {
            let f = field.trim().to_string();
            let fields = if f.is_empty() { Vec::new() } else { vec![f] };
            (fields, None)
        }
        Some((field, rest)) => {
            let f = field.trim().to_string();
            let fields = if f.is_empty() { Vec::new() } else { vec![f] };
            let parts: Vec<String> = rest.split(',').map(|s| s.to_string()).collect();
            (fields, Some(parts))
        }
    }
}
