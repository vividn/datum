use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{LiftedSpec, ValueMode};

// ─── Flag tokenisation ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlagSplit<'a> {
    Positional(&'a str),
    Long(&'a str),                  // --name (no attached value)
    LongAttached(&'a str, &'a str), // --name=value
    Short(char),                    // -c
    ShortAttached(char, &'a str),   // -c=value, or -cVALUE (only `-c=` form used here)
}

pub(crate) fn split_flag(tok: &str) -> FlagSplit<'_> {
    if tok == "-" || tok == "--" {
        return FlagSplit::Positional(tok);
    }
    if let Some(rest) = tok.strip_prefix("--") {
        match rest.split_once('=') {
            Some((n, v)) => FlagSplit::LongAttached(n, v),
            None => FlagSplit::Long(rest),
        }
    } else if let Some(rest) = tok.strip_prefix('-') {
        // A leading digit, `.`, or other non-letter means "this is a value
        // like `-1` or `-5.3`, not a flag".
        let first = rest.chars().next().unwrap();
        if !first.is_ascii_alphabetic() {
            return FlagSplit::Positional(tok);
        }
        if let Some((f, v)) = rest.split_once('=') {
            if f.chars().count() == 1 {
                return FlagSplit::ShortAttached(f.chars().next().unwrap(), v);
            }
            // `--foo=v` was already handled; `-foo=v` isn't a shape we use.
            return FlagSplit::Positional(tok);
        }
        if rest.chars().count() == 1 {
            FlagSplit::Short(first)
        } else {
            // `-dVALUE` form: attached value for a short flag.
            // We emit it as ShortAttached so callers can decide.
            FlagSplit::ShortAttached(first, &rest[first.len_utf8()..])
        }
    } else {
        FlagSplit::Positional(tok)
    }
}

/// Take the value following a flag at `i` (which points *at* the flag token).
/// Returns (value, new_i).
pub(crate) fn take_value<'a>(
    tail: &'a [&'a str],
    i: usize,
    attached: Option<&'a str>,
    flag_label: &str,
) -> Result<(&'a str, usize)> {
    if let Some(v) = attached {
        return Ok((v, i + 1));
    }
    match tail.get(i + 1) {
        Some(v) => Ok((v, i + 2)),
        None => Err(ParseError::InvalidArg(format!(
            "missing value for {flag_label}"
        ))),
    }
}

// ─── Central flag registry walker ────────────────────────────────────────
//
// The walker matches one token at `i` against every spec in `registry`,
// applying the first match. Specs are pulled from per-module FLAG_SPECS
// arrays via `lift()` and combined into a per-command registry. Compound
// shorts (`-yyt 3`) are pre-expanded by `alias::expand_stacked_flags` so
// the walker only sees single-letter forms.

/// Try to consume one flag from `tail[i]` against `registry`. Returns the
/// new index when consumed; `None` when the token isn't a flag (i.e.
/// positional) or doesn't match any spec.
pub(crate) fn walk_registry<P: 'static>(
    tail: &[&str],
    i: usize,
    target: &mut P,
    registry: &[LiftedSpec<P>],
) -> Result<Option<usize>> {
    let tok = tail[i];
    let split = split_flag(tok);

    let (spec, attached) = match split {
        FlagSplit::Long(name) => match registry.iter().find(|s| s.long == Some(name)) {
            Some(s) => (s, None),
            None => return Ok(None),
        },
        FlagSplit::LongAttached(name, v) => match registry.iter().find(|s| s.long == Some(name)) {
            Some(s) => (s, Some(v)),
            None => return Ok(None),
        },
        FlagSplit::Short(c) => match registry.iter().find(|s| s.short == Some(c)) {
            Some(s) => (s, None),
            None => return Ok(None),
        },
        FlagSplit::ShortAttached(c, v) => match registry.iter().find(|s| s.short == Some(c)) {
            Some(s) => (s, Some(v)),
            None => return Ok(None),
        },
        FlagSplit::Positional(_) => return Ok(None),
    };

    match (spec.value_mode, attached) {
        (ValueMode::None, _) => {
            // Switch flag — attached `=value` form is silently ignored,
            // matching pre-registry behaviour for things like `--show-all=foo`.
            (spec.apply)(target, None)?;
            Ok(Some(i + 1))
        }
        (ValueMode::Required, Some(v)) => {
            (spec.apply)(target, Some(v))?;
            Ok(Some(i + 1))
        }
        (ValueMode::Required, None) => {
            let label = spec_label(spec);
            let (v, ni) = take_value(tail, i, None, &label)?;
            (spec.apply)(target, Some(v))?;
            Ok(Some(ni))
        }
        (ValueMode::OptionalAttached, attached) => {
            (spec.apply)(target, attached)?;
            Ok(Some(i + 1))
        }
    }
}

pub(crate) fn spec_label<P: 'static>(spec: &LiftedSpec<P>) -> String {
    match (spec.short, spec.long) {
        (Some(c), Some(l)) => format!("-{c}/--{l}"),
        (None, Some(l)) => format!("--{l}"),
        (Some(c), None) => format!("-{c}"),
        (None, None) => "<flag>".to_string(),
    }
}

/// Standard subcommand-parser fold: walk `tail` once, feeding flags into
/// `registry` (via [`walk_registry`]) and collecting bare positionals.
/// An unrecognised flag is a hard error. Returns the positional vec.
///
/// da-jflh.2: extracted from the dozen `parse_X_args` functions that all
/// share this exact loop. Parsers with bespoke prefix logic (a multi-arg
/// flag like `--diff A B`, a peek-ahead like `-w` on tail) use
/// [`fold_args_with_pre`] instead.
pub(crate) fn fold_args<'a, P: 'static>(
    tail: &'a [&'a str],
    composed: &mut P,
    registry: &[LiftedSpec<P>],
) -> Result<Vec<&'a str>> {
    fold_args_with_pre(tail, composed, registry, |_, _, _| Ok(None))
}

/// Same as [`fold_args`] but threads a `pre_hook` that runs before the
/// registry walker on each token. If the hook returns `Some(ni)`, the
/// loop jumps to `ni` and skips the registry/positional branches; `None`
/// falls through to the standard handling.
pub(crate) fn fold_args_with_pre<'a, P: 'static>(
    tail: &'a [&'a str],
    composed: &mut P,
    registry: &[LiftedSpec<P>],
    mut pre_hook: impl FnMut(&'a [&'a str], usize, &mut P) -> Result<Option<usize>>,
) -> Result<Vec<&'a str>> {
    let mut pos: Vec<&str> = Vec::new();
    let mut no_more_flags = false;
    let mut i = 0;
    while i < tail.len() {
        // Unix `--` (spec v2 rule 4, docs/trailing-and-slots.md): end of
        // option parsing for the rest of THIS argv — every later token
        // is positional, even dash-prefixed. Side-scoping is automatic
        // (each side parses its own argv); alias-scoping is handled by
        // the region layer before the tokens reach us.
        if no_more_flags {
            pos.push(tail[i]);
            i += 1;
            continue;
        }
        if tail[i] == "--" {
            no_more_flags = true;
            i += 1;
            continue;
        }
        if let Some(ni) = pre_hook(tail, i, composed)? {
            i = ni;
            continue;
        }
        if let Some(ni) = walk_registry(tail, i, composed, registry)? {
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
    Ok(pos)
}

pub(crate) fn parse_conflict_strategy(s: &str) -> Result<ConflictStrategyArg> {
    match s.to_ascii_lowercase().as_str() {
        "update" => Ok(ConflictStrategyArg::Update),
        "merge" => Ok(ConflictStrategyArg::Merge),
        "rekey" => Ok(ConflictStrategyArg::Rekey),
        "ignore" => Ok(ConflictStrategyArg::Ignore),
        other => Err(ParseError::InvalidArg(format!(
            "invalid --conflict value: '{other}' (expected update|merge|rekey|ignore)"
        ))),
    }
}

// ─── Small helpers ───────────────────────────────────────────────────────

pub(crate) fn looks_like_flag(tok: &str) -> bool {
    match split_flag(tok) {
        FlagSplit::Long(_)
        | FlagSplit::LongAttached(_, _)
        | FlagSplit::Short(_)
        | FlagSplit::ShortAttached(_, _) => true,
        FlagSplit::Positional(_) => false,
    }
}

pub(crate) fn unknown_flag(f: FlagSplit<'_>) -> ParseError {
    match f {
        FlagSplit::Long(n) | FlagSplit::LongAttached(n, _) => {
            ParseError::InvalidArg(format!("unexpected flag: --{n}"))
        }
        FlagSplit::Short(c) | FlagSplit::ShortAttached(c, _) => {
            ParseError::InvalidArg(format!("unexpected flag: -{c}"))
        }
        FlagSplit::Positional(s) => {
            ParseError::InvalidArg(format!("unexpected positional argument: {s}"))
        }
    }
}

pub(crate) fn parse_usize(s: &str, label: &str) -> Result<usize> {
    s.parse::<usize>()
        .map_err(|_| ParseError::InvalidArg(format!("invalid number for {label}: '{s}'")))
}

pub(crate) fn parse_u32(s: &str, label: &str) -> Result<u32> {
    s.parse::<u32>()
        .map_err(|_| ParseError::InvalidArg(format!("invalid number for {label}: '{s}'")))
}

pub(crate) fn parse_f64(s: &str, label: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| ParseError::InvalidArg(format!("invalid number for {label}: '{s}'")))
}

pub(crate) fn require_pos(pos: &[&str], label: &str) -> Result<String> {
    pos.first()
        .map(|s| s.to_string())
        .ok_or_else(|| ParseError::InvalidArg(format!("missing required positional: {label}")))
}
