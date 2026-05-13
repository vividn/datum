//! Bespoke argv parser for the `datum` CLI.
//!
//! Walks `argv` token-by-token, dispatches on the subcommand name, and
//! produces a [`Cli`] intent for each known verb.
//!
//! # Scope
//!
//! This parser is the back half of the `datum` command line; the
//! pre-pass (alias expansion, terminal-verb override, field/verb swap,
//! stacked short-flag expansion) runs before `parse_argv` is called.
//! Callers that want the whole pipeline should go through
//! `datum_commands::args::expand_argv` first (which returns the
//! already-expanded argv).
//!
//! # Flag form support
//!
//! * `--long VALUE` and `--long=VALUE`
//! * `-l VALUE` and `-l=VALUE`
//! * Repeatable flags push onto a `Vec<String>`.
//! * Bool flags (no value).
//! * Count-action flags (`-y`, `-q`) are expected to arrive already
//!   single-letter — the pre-pass expands `-yy` → `-y -y`.
//! * Values that start with `-` are allowed only via the `=` form
//!   (default behaviour without explicit allow-hyphen-values opt-in).
//!   A handful of known-hyphen-friendly flags (`--offset`, `-t/--time`
//!   on `retime`, and every `-d`/`-t` with offsets on `occur`/`start`/
//!   `stop`/`switch`/`add`/`tail`) keep a historical quirk: only the
//!   `=` form gets to carry a hyphen prefix, which is what the existing
//!   tests lock in.

use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::{
    self, lift, local_optional, local_switch, local_value, LiftedSpec, OutputArgs, TimeArgs,
    ValueMode,
};

/// Entry point: parse an argv (including argv[0]) into a [`Cli`].
pub fn parse_argv(argv: &[String]) -> Result<Cli> {
    // Strip argv[0] ("datum" or whatever binary name the caller passed).
    let tail: Vec<&str> = argv.iter().skip(1).map(|s| s.as_str()).collect();
    parse_tail(&tail)
}

/// String-API variant: shell-tokenize, then parse.
pub fn parse_argv_string(s: &str) -> Result<Cli> {
    let argv = shell_split(s);
    let tail: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    parse_tail(&tail)
}

/// Minimal shell-style splitter: single and double quotes, backslash
/// escapes. Not POSIX-complete — enough for the in-process test harness
/// and WASM frontend which don't need env-var expansion.
pub fn shell_split(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut has_tok = false;
    while let Some(c) = chars.next() {
        if in_single {
            if c == '\'' {
                in_single = false;
            } else {
                cur.push(c);
            }
        } else if in_double {
            if c == '"' {
                in_double = false;
            } else if c == '\\' {
                if let Some(nc) = chars.next() {
                    cur.push(nc);
                }
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '\'' => {
                    in_single = true;
                    has_tok = true;
                }
                '"' => {
                    in_double = true;
                    has_tok = true;
                }
                '\\' => {
                    if let Some(nc) = chars.next() {
                        cur.push(nc);
                        has_tok = true;
                    }
                }
                c if c.is_whitespace() => {
                    if has_tok {
                        out.push(std::mem::take(&mut cur));
                        has_tok = false;
                    }
                }
                _ => {
                    cur.push(c);
                    has_tok = true;
                }
            }
        }
    }
    if has_tok {
        out.push(cur);
    }
    out
}

fn parse_tail(tail: &[&str]) -> Result<Cli> {
    // Consume global flags before the subcommand.
    //
    // `--db NAME_OR_PATH` is the only pre-subcommand global. We walk
    // linearly; the first non-flag token (or an unknown flag) is the
    // subcommand name.
    let mut db: Option<String> = None;
    let mut i = 0;
    let sub_idx = loop {
        let Some(tok) = tail.get(i) else {
            // No subcommand after consuming global flags: drop into the
            // REPL with whatever global context (e.g. `--db NAME`) the
            // caller supplied. `dm` and `dm --db finance` both land here.
            return Ok(Cli {
                db,
                command: Command::Repl,
            });
        };
        match split_flag(tok) {
            FlagSplit::LongAttached("db", v) => {
                db = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("db") => {
                let v = tail
                    .get(i + 1)
                    .ok_or_else(|| ParseError::InvalidArg("--db requires a value".into()))?;
                db = Some((*v).to_string());
                i += 2;
            }
            FlagSplit::LongAttached("help", _) | FlagSplit::Long("help") => {
                return Err(ParseError::HelpRequested(None));
            }
            FlagSplit::ShortAttached('h', _) | FlagSplit::Short('h') => {
                return Err(ParseError::HelpRequested(None));
            }
            FlagSplit::LongAttached("version", _) | FlagSplit::Long("version") => {
                return Err(ParseError::VersionRequested);
            }
            FlagSplit::ShortAttached('V', _) | FlagSplit::Short('V') => {
                return Err(ParseError::VersionRequested);
            }
            FlagSplit::LongAttached("man", v) => {
                return Err(ParseError::ManRequested(Some(v.to_string())));
            }
            FlagSplit::Long("man") => {
                // Optional positional subcommand: `datum --man tail`.
                // Skip flags so `datum --man --foo` still renders the
                // top-level page.
                let next = tail
                    .get(i + 1)
                    .copied()
                    .filter(|s| !s.starts_with('-'))
                    .map(String::from);
                return Err(ParseError::ManRequested(next));
            }
            FlagSplit::Positional(_) => break i,
            FlagSplit::Long(name) | FlagSplit::LongAttached(name, _) => {
                return Err(ParseError::InvalidArg(format!(
                    "unexpected flag before subcommand: --{name}"
                )));
            }
            FlagSplit::Short(c) | FlagSplit::ShortAttached(c, _) => {
                return Err(ParseError::InvalidArg(format!(
                    "unexpected flag before subcommand: -{c}"
                )));
            }
        }
    };

    let sub = tail[sub_idx];
    let rest: &[&str] = &tail[sub_idx + 1..];

    // Per-subcommand `--help` / `-h` short-circuit. Walk `rest` once and
    // bail with a HelpRequested if we find either form; this saves
    // every subcommand parser from having to handle them individually.
    for tok in rest {
        match *tok {
            "--help" | "-h" => return Err(ParseError::HelpRequested(Some(sub.to_string()))),
            // `datum SUB --man` and (after the field/verb prepass swap)
            // `datum --man SUB` both land here.
            "--man" => return Err(ParseError::ManRequested(Some(sub.to_string()))),
            _ => {}
        }
    }

    let command = match sub {
        "occur" => Command::Occur(parse_log_args(rest)?),
        "start" => Command::Start(parse_log_args(rest)?),
        "stop" | "end" => Command::Stop(parse_log_args(rest)?),
        "switch" => Command::Switch(parse_log_args(rest)?),
        "add" => Command::Add(parse_log_args(rest)?),

        "tail" => Command::Tail(parse_tail_args(rest)?),
        "head" => {
            // `head` is tail with oldest-first: reuse tail's parser and flip
            // the default.
            let mut args = parse_tail_args(rest)?;
            args.oldest_first = true;
            Command::Head(args)
        }
        "now" => Command::Now(parse_now_args(rest)?),

        "get" => Command::Get(parse_get_args(rest)?),
        "delete" | "del" => Command::Delete(parse_delete_args(rest)?),
        "undo" => Command::Undo(parse_undo_args(rest)?),

        "history" => Command::History(parse_history_args(rest)?),
        "why" => Command::Why(parse_why_args(rest)?),

        "edit" => Command::Edit(parse_edit_args(rest)?),

        "update" => Command::Update(parse_update_args(rest)?),
        "merge" => Command::Merge(parse_update_args(rest)?),
        "rekey" => Command::Rekey(parse_update_args(rest)?),

        "retime" | "rt" => Command::Retime(parse_retime_args(rest)?),

        "spec" => Command::Spec(parse_spec_cmd(rest)?),
        "alias" => Command::Alias(parse_alias_cmd(rest)?),
        "kit" => Command::Kit(parse_kit_cmd(rest)?),

        "track" => Command::Track(parse_track_args(rest)?),
        "ignore" => Command::Ignore(parse_ignore_args(rest)?),

        "grep" => Command::Grep(parse_grep_args(rest)?),
        "check" => Command::Check(parse_check_args(rest)?),

        "count" => Command::Count(parse_count_args(rest)?),
        "ids" => Command::Ids(parse_ids_args(rest)?),

        "dayview" => Command::Dayview(parse_dayview_args(rest)?),
        "nowview" => Command::Nowview(parse_nowview_args(rest)?),
        "view" => Command::View(parse_view_args(rest)?),

        "info" => Command::Info(parse_info_args(rest)?),
        "db" => Command::Db(parse_db_cmd(rest)?),

        "config" => Command::Config(parse_config_cmd(rest)?),

        "sync" => Command::Sync(parse_sync_args(rest)?),

        "conflicts" => Command::Conflicts(parse_conflicts_cmd(rest)?),

        "reparse" => Command::Reparse(parse_reparse_args(rest)?),

        "help" => {
            // `datum help <cmd>` → help request with that cmd.
            let cmd = rest.first().map(|s| s.to_string());
            return Err(ParseError::HelpRequested(cmd));
        }

        other => {
            return Err(ParseError::InvalidArg(format!(
                "unknown subcommand: '{other}'"
            )));
        }
    };

    Ok(Cli { db, command })
}

// ─── Flag tokenisation ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlagSplit<'a> {
    Positional(&'a str),
    Long(&'a str),                  // --name (no attached value)
    LongAttached(&'a str, &'a str), // --name=value
    Short(char),                    // -c
    ShortAttached(char, &'a str),   // -c=value, or -cVALUE (only `-c=` form used here)
}

fn split_flag(tok: &str) -> FlagSplit<'_> {
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
fn take_value<'a>(
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
fn walk_registry<P: 'static>(
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

fn spec_label<P: 'static>(spec: &LiftedSpec<P>) -> String {
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
fn fold_args<'a, P: 'static>(
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
fn fold_args_with_pre<'a, P: 'static>(
    tail: &'a [&'a str],
    composed: &mut P,
    registry: &[LiftedSpec<P>],
    mut pre_hook: impl FnMut(&'a [&'a str], usize, &mut P) -> Result<Option<usize>>,
) -> Result<Vec<&'a str>> {
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
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

// ─── Subcommand parsers ──────────────────────────────────────────────────

/// Build the per-command flag registry for logging verbs (occur, start,
/// stop, switch, add). Lifts the timing/duration/output module specs onto
/// the public `LogArgs` shape directly — no intermediate parsing struct
/// since da-e2e exposed the composed bags publicly.
fn log_args_registry() -> Vec<LiftedSpec<LogArgs>> {
    let mut r: Vec<LiftedSpec<LogArgs>> = Vec::new();
    // Time concern (-y/-q/-d/-t/-z/-T). `-D/--full-day` is intentionally
    // omitted; logging verbs don't expose it today.
    for spec in input::time_args::FLAG_SPECS {
        if spec.short == Some('D') {
            continue;
        }
        r.push(lift(|p: &mut LogArgs| &mut p.time, spec));
    }
    // Duration concern (--dur, -m).
    for spec in input::duration_args::FLAG_SPECS {
        r.push(lift(|p: &mut LogArgs| &mut p.duration, spec));
    }
    // Output concern (-A, --show, -o).
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut LogArgs| &mut p.output, spec));
    }
    // Command-scoped extras. We don't lift DataArgs's FLAG_SPECS wholesale
    // because log tracks slug-keys separately from keys; -R/-S/-C are
    // surfaced individually below so daydumb's PRIMARY_VALUE_KEY sentinel
    // can stay in charge when none of them are supplied.
    r.push(local_switch(Some('u'), Some("undo"), |p| {
        p.undo = true;
        Ok(())
    }));
    r.push(local_value(Some('k'), Some("key"), |p, v| {
        p.keys.push(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('K'), Some("slug-key"), |p, v| {
        p.slug_keys.push(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('c'), Some("comment"), |p, v| {
        p.comment.push(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('R'), Some("remainder"), |p, v| {
        p.remainder = Some(v.to_string());
        Ok(())
    }));
    r.push(local_switch(Some('S'), Some("string-remainder"), |p| {
        p.string_remainder = true;
        Ok(())
    }));
    r.push(local_switch(Some('C'), Some("comment-remainder"), |p| {
        p.comment_remainder = true;
        Ok(())
    }));
    r.push(local_value(None, Some("id"), |p, v| {
        p.id = Some(v.to_string());
        Ok(())
    }));
    r.push(local_value(None, Some("id-delimiter"), |p, v| {
        p.id_delimiter = v.to_string();
        Ok(())
    }));
    r.push(local_optional(Some('L'), Some("last-state"), |p, v| {
        // Bare → Some(""), `=value` → Some(value). The empty-string sentinel
        // is the trigger for Stop's close-most-recent behaviour.
        p.last_state = Some(v.unwrap_or("").to_string());
        Ok(())
    }));
    r.push(local_value(None, Some("conflict"), |p, v| {
        p.conflict = Some(parse_conflict_strategy(v)?);
        Ok(())
    }));
    r.push(local_switch(None, Some("update"), |p| {
        if p.merge {
            return Err(ParseError::InvalidArg(
                "--update conflicts with --merge".into(),
            ));
        }
        p.update = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("merge"), |p| {
        if p.update {
            return Err(ParseError::InvalidArg(
                "--merge conflicts with --update".into(),
            ));
        }
        p.merge = true;
        Ok(())
    }));
    r.push(local_switch(Some('M'), Some("no-metadata"), |p| {
        p.no_metadata = true;
        Ok(())
    }));
    r.push(local_value(None, Some("alias"), |p, v| {
        if v.is_empty() {
            return Err(ParseError::InvalidArg(
                "--alias requires a non-empty name".into(),
            ));
        }
        p.alias_name = Some(v.to_string());
        Ok(())
    }));
    r.push(local_switch(None, Some("alias-overwrite"), |p| {
        p.alias_overwrite = true;
        Ok(())
    }));
    r
}

fn parse_log_args(tail: &[&str]) -> Result<LogArgs> {
    let registry = log_args_registry();
    let mut out = LogArgs::fresh(String::new());
    let pos = fold_args(tail, &mut out, &registry)?;
    let mut it = pos.into_iter();
    out.field_or_verb = it
        .next()
        .ok_or_else(|| {
            ParseError::InvalidArg("missing required positional: <FIELD_OR_VERB>".into())
        })?
        .to_string();
    out.rest = it.map(|s| s.to_string()).collect();
    Ok(out)
}

fn parse_conflict_strategy(s: &str) -> Result<ConflictStrategyArg> {
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

// Internal composed shape for tail/head.
#[derive(Default)]
struct TailParsing {
    time: TimeArgs,
    output: OutputArgs,
    extras: TailExtras,
}

#[derive(Default)]
struct TailExtras {
    count: Option<usize>,
    metric: Option<String>,
    reverse: bool,
    columns: Vec<String>,
    oldest_first: bool,
    table: bool,
    watch: Option<f64>,
}

fn tail_args_registry() -> Vec<LiftedSpec<TailParsing>> {
    let mut r: Vec<LiftedSpec<TailParsing>> = Vec::new();
    // Time (-y/-q/-d/-t). `-z`, `-T`, `-D` not exposed by tail.
    for spec in input::time_args::FLAG_SPECS {
        match (spec.short, spec.long) {
            (Some('y'), _) | (Some('q'), _) | (Some('d'), _) | (Some('t'), _) => {
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

fn parse_tail_args(tail: &[&str]) -> Result<TailArgs> {
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
    Ok(TailArgs {
        field: pos.first().map(|s| s.to_string()),
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
        output: composed.output,
    })
}

fn parse_now_args(tail: &[&str]) -> Result<NowArgs> {
    // `now` consumes `-A/--show-all` (via OutputArgs registry, kept
    // canonical) plus its own band-rendering flags: `--bare`, `--no-color`,
    // `--cols`.
    #[derive(Default)]
    struct NowParsing {
        output: OutputArgs,
        bare: bool,
        no_color: bool,
        cols: Option<u16>,
    }
    let mut registry: Vec<LiftedSpec<NowParsing>> = input::output_args::FLAG_SPECS
        .iter()
        .filter(|s| s.short == Some('A'))
        .map(|s| lift(|p: &mut NowParsing| &mut p.output, s))
        .collect();
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
        show_all: composed.output.show_all,
        bare: composed.bare,
        no_color: composed.no_color,
        cols: composed.cols,
    })
}

// Internal composed shape for the read-only `get` command.
#[derive(Default)]
struct GetParsing {
    output: OutputArgs,
    pretty: bool,
}

fn get_args_registry() -> Vec<LiftedSpec<GetParsing>> {
    let mut r: Vec<LiftedSpec<GetParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut GetParsing| &mut p.output, spec));
    }
    r.push(local_switch(None, Some("pretty"), |p| {
        p.pretty = true;
        Ok(())
    }));
    r
}

fn parse_get_args(tail: &[&str]) -> Result<GetArgs> {
    let registry = get_args_registry();
    let mut composed = GetParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(GetArgs {
        id_or_slug: pos.first().map(|s| s.to_string()),
        pretty: composed.pretty,
        output: composed.output,
    })
}

#[derive(Default)]
struct DeleteParsing {
    output: OutputArgs,
    force: bool,
    dry_run: bool,
    strict: bool,
}

fn delete_args_registry() -> Vec<LiftedSpec<DeleteParsing>> {
    let mut r: Vec<LiftedSpec<DeleteParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut DeleteParsing| &mut p.output, spec));
    }
    r.push(local_switch(None, Some("force"), |p| {
        p.force = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("dry-run"), |p| {
        p.dry_run = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("strict"), |p| {
        p.strict = true;
        Ok(())
    }));
    r
}

fn parse_delete_args(tail: &[&str]) -> Result<DeleteArgs> {
    let registry = delete_args_registry();
    let mut composed = DeleteParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(DeleteArgs {
        id_or_slug: pos.first().map(|s| s.to_string()),
        force: composed.force,
        dry_run: composed.dry_run,
        strict: composed.strict,
        output: composed.output,
    })
}

#[derive(Default)]
struct UndoParsing {
    output: OutputArgs,
    to_version: Option<u32>,
    force: bool,
    window: Option<String>,
}

fn undo_args_registry() -> Vec<LiftedSpec<UndoParsing>> {
    let mut r: Vec<LiftedSpec<UndoParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut UndoParsing| &mut p.output, spec));
    }
    r.push(local_value(None, Some("to-version"), |p, v| {
        p.to_version = Some(parse_u32(v, "--to-version")?);
        Ok(())
    }));
    r.push(local_switch(None, Some("force"), |p| {
        p.force = true;
        Ok(())
    }));
    r.push(local_value(None, Some("window"), |p, v| {
        p.window = Some(v.to_string());
        Ok(())
    }));
    r
}

fn parse_undo_args(tail: &[&str]) -> Result<UndoArgs> {
    let registry = undo_args_registry();
    let mut composed = UndoParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    Ok(UndoArgs {
        id_or_slug: pos.first().map(|s| s.to_string()),
        to_version: composed.to_version,
        force: composed.force,
        window: composed.window.unwrap_or_else(|| "15m".to_string()),
        output: composed.output,
    })
}

#[derive(Default)]
struct HistoryParsing {
    version: Option<u32>,
    diff: Option<Vec<u32>>,
    json: bool,
}

fn history_args_registry() -> Vec<LiftedSpec<HistoryParsing>> {
    let mut r: Vec<LiftedSpec<HistoryParsing>> = Vec::new();
    r.push(local_value(None, Some("version"), |p, v| {
        p.version = Some(parse_u32(v, "--version")?);
        Ok(())
    }));
    r.push(local_switch(None, Some("json"), |p| {
        p.json = true;
        Ok(())
    }));
    // `--diff` consumes TWO detached values — handled by the caller as a
    // pre-walker special case; not represented here.
    r
}

fn parse_history_args(tail: &[&str]) -> Result<HistoryArgs> {
    let registry = history_args_registry();
    let mut composed = HistoryParsing::default();
    // `--diff A B` — two-arg form, no central-registry support today.
    let pos = fold_args_with_pre(tail, &mut composed, &registry, |tail, i, p| {
        if matches!(split_flag(tail[i]), FlagSplit::Long("diff")) {
            let a = tail
                .get(i + 1)
                .ok_or_else(|| ParseError::InvalidArg("--diff requires two values".into()))?;
            let b = tail
                .get(i + 2)
                .ok_or_else(|| ParseError::InvalidArg("--diff requires two values".into()))?;
            p.diff = Some(vec![parse_u32(a, "--diff")?, parse_u32(b, "--diff")?]);
            return Ok(Some(i + 3));
        }
        Ok(None)
    })?;
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(HistoryArgs {
        id_or_slug,
        version: composed.version,
        diff: composed.diff,
        json: composed.json,
    })
}

fn parse_why_args(tail: &[&str]) -> Result<WhyArgs> {
    #[derive(Default)]
    struct WhyParsing {
        full: bool,
    }
    let registry: Vec<LiftedSpec<WhyParsing>> = vec![local_switch(None, Some("full"), |p| {
        p.full = true;
        Ok(())
    })];
    let mut composed = WhyParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(WhyArgs {
        id_or_slug,
        full: composed.full,
    })
}

fn parse_edit_args(tail: &[&str]) -> Result<EditArgs> {
    let mut composed = OutputArgs::default();
    let registry: Vec<LiftedSpec<OutputArgs>> = input::output_args::FLAG_SPECS
        .iter()
        .map(|s| lift(|p: &mut OutputArgs| p, s))
        .collect();
    let pos = fold_args(tail, &mut composed, &registry)?;
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(EditArgs {
        id_or_slug,
        output: composed,
    })
}

#[derive(Default)]
struct UpdateParsing {
    output: OutputArgs,
    strategy: Option<ConflictStrategyArg>,
    id: Option<String>,
    id_delimiter: Option<String>,
    keys: Vec<String>,
    comment: Vec<String>,
    dry_run: bool,
    strict: bool,
}

fn update_args_registry() -> Vec<LiftedSpec<UpdateParsing>> {
    let mut r: Vec<LiftedSpec<UpdateParsing>> = Vec::new();
    for spec in input::output_args::FLAG_SPECS {
        r.push(lift(|p: &mut UpdateParsing| &mut p.output, spec));
    }
    r.push(local_value(None, Some("strategy"), |p, v| {
        p.strategy = Some(parse_conflict_strategy(v)?);
        Ok(())
    }));
    r.push(local_value(None, Some("id"), |p, v| {
        p.id = Some(v.to_string());
        Ok(())
    }));
    r.push(local_value(None, Some("id-delimiter"), |p, v| {
        p.id_delimiter = Some(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('k'), Some("key"), |p, v| {
        p.keys.push(v.to_string());
        Ok(())
    }));
    r.push(local_value(Some('c'), Some("comment"), |p, v| {
        p.comment.push(v.to_string());
        Ok(())
    }));
    r.push(local_switch(None, Some("dry-run"), |p| {
        p.dry_run = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("strict"), |p| {
        p.strict = true;
        Ok(())
    }));
    r
}

fn parse_update_args(tail: &[&str]) -> Result<UpdateArgs> {
    let registry = update_args_registry();
    let mut composed = UpdateParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    let mut it = pos.into_iter();
    Ok(UpdateArgs {
        id_or_slug: it.next().map(|s| s.to_string()),
        rest: it.map(|s| s.to_string()).collect(),
        strategy: composed.strategy,
        id: composed.id,
        id_delimiter: composed.id_delimiter.unwrap_or_else(|| ":".to_string()),
        keys: composed.keys,
        comment: composed.comment,
        dry_run: composed.dry_run,
        strict: composed.strict,
        output: composed.output,
    })
}

fn parse_retime_args(tail: &[&str]) -> Result<RetimeArgs> {
    #[derive(Default)]
    struct RetimeParsing {
        offset: Option<String>,
        time: Option<String>,
        field: Option<String>,
        after: Option<String>,
        before: Option<String>,
    }
    let mut registry: Vec<LiftedSpec<RetimeParsing>> = Vec::new();
    registry.push(local_value(None, Some("offset"), |p, v| {
        p.offset = Some(v.to_string());
        Ok(())
    }));
    registry.push(local_value(Some('t'), Some("time"), |p, v| {
        p.time = Some(v.to_string());
        Ok(())
    }));
    registry.push(local_value(Some('f'), Some("field"), |p, v| {
        p.field = Some(v.to_string());
        Ok(())
    }));
    registry.push(local_value(None, Some("after"), |p, v| {
        p.after = Some(v.to_string());
        Ok(())
    }));
    registry.push(local_value(None, Some("before"), |p, v| {
        p.before = Some(v.to_string());
        Ok(())
    }));

    let mut composed = RetimeParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    if composed.offset.is_some() && composed.time.is_some() {
        return Err(ParseError::InvalidArg(
            "--offset and --time are mutually exclusive".into(),
        ));
    }
    Ok(RetimeArgs {
        slug: pos.first().map(|s| s.to_string()),
        offset: composed.offset,
        time: composed.time,
        field: composed.field,
        after: composed.after,
        before: composed.before,
    })
}

fn parse_spec_cmd(tail: &[&str]) -> Result<SpecCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing spec subcommand (show|set|list|remove)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "show" => {
            let field = require_first_pos(rest, "<FIELD>")?;
            Ok(SpecCommand::Show { field })
        }
        "set" => Ok(SpecCommand::Set(Box::new(parse_spec_set(rest)?))),
        "list" => Ok(SpecCommand::List {
            tag: parse_spec_list_tag(rest)?,
        }),
        "remove" => {
            let field = require_first_pos(rest, "<FIELD>")?;
            Ok(SpecCommand::Remove { field })
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown spec subcommand: '{other}'"
        ))),
    }
}

fn parse_spec_list_tag(tail: &[&str]) -> Result<Option<String>> {
    let mut tag: Option<String> = None;
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("tag") => {
                let (v, ni) = take_value(tail, i, None, "--tag")?;
                tag = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("tag", v) => {
                tag = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(tag)
}

fn parse_spec_set(tail: &[&str]) -> Result<SpecSetArgs> {
    let mut out = SpecSetArgs::fresh(String::new());
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Long("json") => {
                let (v, ni) = take_value(tail, i, None, "--json")?;
                out.json = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("json", v) => {
                out.json = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("default-verb") => {
                let (v, ni) = take_value(tail, i, None, "--default-verb")?;
                out.default_verb = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("default-verb", v) => {
                out.default_verb = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state") => {
                let (v, ni) = take_value(tail, i, None, "--state")?;
                out.states.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state", v) => {
                out.states.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("color") => {
                let (v, ni) = take_value(tail, i, None, "--color")?;
                out.color = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("color", v) => {
                out.color = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state-color") => {
                let (v, ni) = take_value(tail, i, None, "--state-color")?;
                out.state_colors.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state-color", v) => {
                out.state_colors.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("id-template") => {
                let (v, ni) = take_value(tail, i, None, "--id-template")?;
                out.id_template = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("id-template", v) => {
                out.id_template = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("description") => {
                let (v, ni) = take_value(tail, i, None, "--description")?;
                out.description = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("description", v) => {
                out.description = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("unit") => {
                let (v, ni) = take_value(tail, i, None, "--unit")?;
                out.unit = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("unit", v) => {
                out.unit = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("track") => {
                out.track = true;
                i += 1;
            }
            FlagSplit::Long("ignore") => {
                out.ignore = true;
                i += 1;
            }
            FlagSplit::Long("id-key") => {
                let (v, ni) = take_value(tail, i, None, "--id-key")?;
                out.id_keys.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("id-key", v) => {
                out.id_keys.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("data-key") => {
                let (v, ni) = take_value(tail, i, None, "--data-key")?;
                out.data_keys.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("data-key", v) => {
                out.data_keys.push(v.to_string());
                i += 1;
            }
            FlagSplit::Long("no-autoalias") => {
                out.no_autoalias = true;
                i += 1;
            }
            FlagSplit::Long("y") => {
                let (v, ni) = take_value(tail, i, None, "--y")?;
                out.y = Some(parse_position_f64(v, "--y")?);
                i = ni;
            }
            FlagSplit::LongAttached("y", v) => {
                out.y = Some(parse_position_f64(v, "--y")?);
                i += 1;
            }
            FlagSplit::Long("height") => {
                let (v, ni) = take_value(tail, i, None, "--height")?;
                out.height = Some(parse_position_f64(v, "--height")?);
                i = ni;
            }
            FlagSplit::LongAttached("height", v) => {
                out.height = Some(parse_position_f64(v, "--height")?);
                i += 1;
            }
            FlagSplit::Long("z-index") => {
                let (v, ni) = take_value(tail, i, None, "--z-index")?;
                out.z_index = Some(parse_position_f64(v, "--z-index")?);
                i = ni;
            }
            FlagSplit::LongAttached("z-index", v) => {
                out.z_index = Some(parse_position_f64(v, "--z-index")?);
                i += 1;
            }
            FlagSplit::Long("tag") => {
                let (v, ni) = take_value(tail, i, None, "--tag")?;
                out.tags.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("tag", v) => {
                out.tags.push(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    out.field = require_first_pos(&pos.to_vec(), "<FIELD>")?;
    Ok(out)
}

fn parse_position_f64(s: &str, flag: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| ParseError::InvalidArg(format!("{flag} expects a number, got '{s}'")))
}

fn parse_alias_cmd(tail: &[&str]) -> Result<AliasCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing alias subcommand (show|set|list|remove)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "show" => {
            let name = require_first_pos(rest, "<NAME>")?;
            Ok(AliasCommand::Show { name })
        }
        "set" => {
            let name = rest
                .first()
                .ok_or_else(|| ParseError::InvalidArg("alias set requires <NAME>".into()))?
                .to_string();
            let expansion = rest
                .get(1)
                .ok_or_else(|| ParseError::InvalidArg("alias set requires <EXPANSION>".into()))?
                .to_string();
            Ok(AliasCommand::Set { name, expansion })
        }
        "list" => Ok(AliasCommand::List),
        "remove" => {
            let name = require_first_pos(rest, "<NAME>")?;
            Ok(AliasCommand::Remove { name })
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown alias subcommand: '{other}'"
        ))),
    }
}

fn parse_kit_cmd(tail: &[&str]) -> Result<KitCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing kit subcommand (install|export|list|uninstall)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "install" => {
            let mut force = false;
            let mut assume_yes = false;
            let mut pos: Vec<&str> = Vec::new();
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Positional(s) => pos.push(s),
                    FlagSplit::Long("force") | FlagSplit::Short('f') => force = true,
                    FlagSplit::Long("yes") | FlagSplit::Short('y') => assume_yes = true,
                    f => return Err(unknown_flag(f)),
                }
            }
            let path = require_first_pos(&pos, "<PATH>")?;
            Ok(KitCommand::Install(KitInstallArgs {
                path,
                force,
                assume_yes,
            }))
        }
        "export" => {
            let mut pos: Vec<&str> = Vec::new();
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Positional(s) => pos.push(s),
                    f => return Err(unknown_flag(f)),
                }
            }
            let name = pos
                .first()
                .ok_or_else(|| ParseError::InvalidArg("kit export requires <NAME>".into()))?
                .to_string();
            let path = pos.get(1).map(|s| s.to_string());
            Ok(KitCommand::Export(KitExportArgs { name, path }))
        }
        "list" => Ok(KitCommand::List),
        "uninstall" => {
            let name = require_first_pos(rest, "<NAME>")?;
            Ok(KitCommand::Uninstall(KitUninstallArgs { name }))
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown kit subcommand: '{other}'"
        ))),
    }
}

fn parse_track_args(tail: &[&str]) -> Result<TrackArgs> {
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
        field: require_first_pos(&pos, "<FIELD>")?,
        color: composed.color,
        state_colors: composed.state_colors,
        tags: composed.tags,
    })
}

fn parse_ignore_args(tail: &[&str]) -> Result<IgnoreArgs> {
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
        field: require_first_pos(&pos, "<FIELD>")?,
    })
}

fn parse_migrate_cmd(tail: &[&str]) -> Result<MigrateCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing migrate subcommand (run|status|edit)".into(),
        ));
    };
    match *sub {
        "run" => Ok(MigrateCommand::Run),
        "status" => Ok(MigrateCommand::Status),
        "edit" => Ok(MigrateCommand::Edit),
        other => Err(ParseError::InvalidArg(format!(
            "unknown migrate subcommand: '{other}'"
        ))),
    }
}

fn parse_db_import(tail: &[&str]) -> Result<DbImportArgs> {
    let mut out = DbImportArgs::default();
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Long("from") | FlagSplit::Long("backup") => {
                let (v, ni) = take_value(tail, i, None, "--from")?;
                out.backup_file = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("from", v) | FlagSplit::LongAttached("backup", v) => {
                out.backup_file = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("dry-run") => {
                out.dry_run = true;
                i += 1;
            }
            FlagSplit::Long("skip-log") => {
                let (v, ni) = take_value(tail, i, None, "--skip-log")?;
                out.skip_log = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("skip-log", v) => {
                out.skip_log = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if let Some(p) = pos.first() {
        out.backup_file_positional = Some(p.to_string());
    }
    Ok(out)
}

fn parse_backup_args(tail: &[&str]) -> Result<BackupArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut overwrite = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("overwrite") => overwrite = true,
            f => return Err(unknown_flag(f)),
        }
    }
    let path = require_first_pos(&pos, "<PATH>")?;
    Ok(BackupArgs { path, overwrite })
}

fn parse_restore_args(tail: &[&str]) -> Result<RestoreArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut force = false;
    let mut dry_run = false;
    let mut json = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("force") => force = true,
            FlagSplit::Long("dry-run") => dry_run = true,
            FlagSplit::Long("json") => json = true,
            f => return Err(unknown_flag(f)),
        }
    }
    let path = require_first_pos(&pos, "<PATH>")?;
    Ok(RestoreArgs {
        path,
        force,
        dry_run,
        json,
    })
}

fn parse_grep_args(tail: &[&str]) -> Result<GrepArgs> {
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

fn parse_check_args(tail: &[&str]) -> Result<CheckArgs> {
    let mut out = CheckArgs::default();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("json") => out.json = true,
            FlagSplit::Long("verbs") => out.verbs = true,
            FlagSplit::Long("aliases") => out.aliases = true,
            FlagSplit::Long("fix") => out.fix = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

fn parse_count_args(tail: &[&str]) -> Result<CountArgs> {
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
            FlagSplit::Long("json") => {
                out.json = true;
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if let Some(f) = pos.first() {
        out.field = Some(f.to_string());
    }
    Ok(out)
}

fn parse_ids_args(tail: &[&str]) -> Result<IdsArgs> {
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
            FlagSplit::Long("json") => {
                out.json = true;
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if let Some(f) = pos.first() {
        out.field = Some(f.to_string());
    }
    Ok(out)
}

fn parse_dayview_args(tail: &[&str]) -> Result<DayviewArgs> {
    let mut out = DayviewArgs::fresh();
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

fn parse_orientation(s: &str) -> Result<DayviewOrientation> {
    match s {
        "horizontal" | "h" => Ok(DayviewOrientation::Horizontal),
        "vertical" | "v" => Ok(DayviewOrientation::Vertical),
        other => Err(ParseError::InvalidArg(format!(
            "--orientation must be 'horizontal' or 'vertical', got '{other}'"
        ))),
    }
}

fn parse_nowview_args(tail: &[&str]) -> Result<NowviewArgs> {
    let mut out = NowviewArgs::fresh();
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
fn parse_view_args(tail: &[&str]) -> Result<ViewArgs> {
    let mut name: Option<String> = None;
    let mut out = ViewArgs::fresh("");
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
            f => return Err(unknown_flag(f)),
        }
    }
    let name = name.ok_or_else(|| {
        ParseError::InvalidArg("view requires a name (e.g. `datum view chores_due`)".into())
    })?;
    out.name = name;
    Ok(out)
}

fn parse_view_format(s: &str) -> Result<ViewFormat> {
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

fn parse_info_args(tail: &[&str]) -> Result<InfoArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut all = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("all") => all = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(InfoArgs {
        target: pos.first().map(|s| s.to_string()),
        all,
    })
}

fn parse_reparse_args(tail: &[&str]) -> Result<ReparseArgs> {
    let mut out = ReparseArgs::default();
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("fix") => {
                out.fix = true;
                i += 1;
            }
            FlagSplit::Positional(tok) => {
                out.hids.push(tok.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

fn parse_setup_args(tail: &[&str]) -> Result<SetupArgs> {
    if let Some(tok) = tail.first() {
        return Err(unknown_flag(split_flag(tok)));
    }
    Ok(SetupArgs {})
}

fn parse_db_cmd(tail: &[&str]) -> Result<DbCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing db subcommand (setup|purge|backup|restore|migrate|import)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "setup" => Ok(DbCommand::Setup(parse_setup_args(rest)?)),
        "purge" => Ok(DbCommand::Purge(parse_db_purge_args(rest)?)),
        "backup" => Ok(DbCommand::Backup(parse_backup_args(rest)?)),
        "restore" => Ok(DbCommand::Restore(parse_restore_args(rest)?)),
        "migrate" => Ok(DbCommand::Migrate(parse_migrate_cmd(rest)?)),
        "import" => Ok(DbCommand::Import(parse_db_import(rest)?)),
        other => Err(ParseError::InvalidArg(format!(
            "unknown db subcommand: '{other}'"
        ))),
    }
}

fn parse_db_purge_args(tail: &[&str]) -> Result<DbPurgeArgs> {
    let mut yes = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("yes") | FlagSplit::Short('y') => yes = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(DbPurgeArgs { yes })
}

fn parse_config_cmd(tail: &[&str]) -> Result<ConfigCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing config subcommand (show)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "show" => {
            let mut args = ConfigShowArgs::default();
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Long("json") => args.json = true,
                    f => return Err(unknown_flag(f)),
                }
            }
            Ok(ConfigCommand::Show(args))
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown config subcommand: '{other}'"
        ))),
    }
}

fn parse_sync_args(tail: &[&str]) -> Result<SyncArgs> {
    // sync is unusual: it has its OWN flags AND an optional subcommand.
    // Walk flags first; the first positional must be one of the known
    // subcommand names (else it's an unexpected positional error).
    let mut args = SyncArgs::default();
    const SUBS: &[&str] = &["login", "config", "key", "status", "install-daemon"];

    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                if SUBS.contains(&s) {
                    let rest: &[&str] = &tail[i + 1..];
                    args.command = Some(match s {
                        "login" => SyncCommand::Login(parse_sync_login(rest)?),
                        "config" => SyncCommand::Config(parse_sync_config(rest)?),
                        "key" => SyncCommand::Key(parse_sync_key(rest)?),
                        "status" => SyncCommand::Status(parse_sync_status(rest)?),
                        "install-daemon" => SyncCommand::InstallDaemon(parse_sync_install(rest)?),
                        _ => unreachable!(),
                    });
                    return Ok(args);
                }
                return Err(ParseError::InvalidArg(format!(
                    "unexpected positional: {s}"
                )));
            }
            FlagSplit::Long("push") => {
                args.push = true;
                i += 1;
            }
            FlagSplit::Long("pull") => {
                args.pull = true;
                i += 1;
            }
            FlagSplit::Long("dry-run") => {
                args.dry_run = true;
                i += 1;
            }
            FlagSplit::Long("server") => {
                let (v, ni) = take_value(tail, i, None, "--server")?;
                args.server = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("server", v) => {
                args.server = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if args.push && args.pull {
        return Err(ParseError::InvalidArg(
            "--push and --pull are mutually exclusive".into(),
        ));
    }
    Ok(args)
}

fn parse_sync_login(tail: &[&str]) -> Result<SyncLoginArgs> {
    let mut out = SyncLoginArgs::default();
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("server") => {
                let (v, ni) = take_value(tail, i, None, "--server")?;
                out.server = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("server", v) => {
                out.server = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

fn parse_sync_status(tail: &[&str]) -> Result<SyncStatusArgs> {
    let mut out = SyncStatusArgs::default();
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("server") => {
                let (v, ni) = take_value(tail, i, None, "--server")?;
                out.server = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("server", v) => {
                out.server = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

fn parse_sync_install(tail: &[&str]) -> Result<SyncInstallDaemonArgs> {
    let mut out = SyncInstallDaemonArgs::default();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("show") => out.show = true,
            FlagSplit::Long("uninstall") => out.uninstall = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

fn parse_sync_config(tail: &[&str]) -> Result<SyncConfigCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing sync config subcommand (set-server|disable|show)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "set-server" => {
            let mut url: Option<String> = None;
            let mut global = false;
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Positional(s) => {
                        if url.is_none() {
                            url = Some(s.to_string());
                        } else {
                            return Err(ParseError::InvalidArg(format!(
                                "unexpected extra positional: {s}"
                            )));
                        }
                    }
                    FlagSplit::Long("global") => global = true,
                    f => return Err(unknown_flag(f)),
                }
            }
            let url = url.ok_or_else(|| {
                ParseError::InvalidArg("sync config set-server requires <URL>".into())
            })?;
            Ok(SyncConfigCommand::SetServer { url, global })
        }
        "disable" => Ok(SyncConfigCommand::Disable),
        "show" => Ok(SyncConfigCommand::Show),
        other => Err(ParseError::InvalidArg(format!(
            "unknown sync config subcommand: '{other}'"
        ))),
    }
}

fn parse_sync_key(tail: &[&str]) -> Result<SyncKeyCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing sync key subcommand (init|unlock|rotate-passphrase|show-fingerprint)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "init" => {
            let mut server: Option<String> = None;
            let mut force = false;
            let mut i = 0;
            while i < rest.len() {
                match split_flag(rest[i]) {
                    FlagSplit::Long("server") => {
                        let (v, ni) = take_value(rest, i, None, "--server")?;
                        server = Some(v.to_string());
                        i = ni;
                    }
                    FlagSplit::LongAttached("server", v) => {
                        server = Some(v.to_string());
                        i += 1;
                    }
                    FlagSplit::Long("force") => {
                        force = true;
                        i += 1;
                    }
                    f => return Err(unknown_flag(f)),
                }
            }
            Ok(SyncKeyCommand::Init { server, force })
        }
        "unlock" => Ok(SyncKeyCommand::Unlock {
            server: take_server_only(rest)?,
        }),
        "rotate-passphrase" => Ok(SyncKeyCommand::RotatePassphrase {
            server: take_server_only(rest)?,
        }),
        "show-fingerprint" => Ok(SyncKeyCommand::ShowFingerprint {
            server: take_server_only(rest)?,
        }),
        other => Err(ParseError::InvalidArg(format!(
            "unknown sync key subcommand: '{other}'"
        ))),
    }
}

fn take_server_only(tail: &[&str]) -> Result<Option<String>> {
    let mut server: Option<String> = None;
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("server") => {
                let (v, ni) = take_value(tail, i, None, "--server")?;
                server = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("server", v) => {
                server = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(server)
}

fn parse_conflicts_cmd(tail: &[&str]) -> Result<ConflictsCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing conflicts subcommand (list|show|resolve|auto-resolve)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "list" => Ok(ConflictsCommand::List(parse_conflicts_list(rest)?)),
        "show" => Ok(ConflictsCommand::Show(parse_conflicts_show(rest)?)),
        "resolve" => Ok(ConflictsCommand::Resolve(parse_conflicts_resolve(rest)?)),
        "auto-resolve" => Ok(ConflictsCommand::AutoResolve(parse_conflicts_auto(rest)?)),
        other => Err(ParseError::InvalidArg(format!(
            "unknown conflicts subcommand: '{other}'"
        ))),
    }
}

fn parse_conflicts_list(tail: &[&str]) -> Result<ConflictsListArgs> {
    let mut out = ConflictsListArgs::default();
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("kind") => {
                let (v, ni) = take_value(tail, i, None, "--kind")?;
                out.kind = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("kind", v) => {
                out.kind = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("all") => {
                out.all = true;
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

fn parse_conflicts_show(tail: &[&str]) -> Result<ConflictsShowArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    let hid = require_first_pos(&pos, "<HID>")?;
    Ok(ConflictsShowArgs { hid })
}

fn parse_conflicts_resolve(tail: &[&str]) -> Result<ConflictsResolveArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut keep: Option<String> = None;
    let mut close_older = false;
    let mut take: Option<String> = None;
    let mut edit = false;
    let mut skip = false;
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Long("keep") => {
                let (v, ni) = take_value(tail, i, None, "--keep")?;
                keep = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("keep", v) => {
                keep = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("close-older") => {
                close_older = true;
                i += 1;
            }
            FlagSplit::Long("take") => {
                let (v, ni) = take_value(tail, i, None, "--take")?;
                take = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("take", v) => {
                take = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("edit") => {
                edit = true;
                i += 1;
            }
            FlagSplit::Long("skip") => {
                skip = true;
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    let hid = require_first_pos(&pos, "<HID>")?;
    let chosen = [keep.is_some(), close_older, take.is_some(), edit, skip]
        .iter()
        .filter(|b| **b)
        .count();
    if chosen > 1 {
        return Err(ParseError::InvalidArg(
            "--keep, --close-older, --take, --edit, --skip are mutually exclusive".into(),
        ));
    }
    if let Some(t) = take.as_deref() {
        if t != "local" && t != "remote" {
            return Err(ParseError::InvalidArg(format!(
                "--take only accepts 'local' or 'remote' (got '{t}')"
            )));
        }
    }
    Ok(ConflictsResolveArgs {
        hid,
        keep,
        close_older,
        take,
        edit,
        skip,
    })
}

fn parse_conflicts_auto(tail: &[&str]) -> Result<ConflictsAutoResolveArgs> {
    // Default lives in datum_core; we accept it as an override only.
    // The callsite that dispatches this fills in the hard-coded default
    // when `min_age == 0` … but 0 is also a valid user choice. To keep
    // the surface stable, we pull the default into the intent here via
    // `datum-core::conflicts::AUTO_RESOLVE_MIN_AGE_SECS` — but that
    // would tie datum-parse to datum-core. Instead, we leave min_age
    // defaulted to 0 here and let the caller `.max(default)` if needed
    // (matching the original behaviour, which also passed the default
    // through).
    let mut out = ConflictsAutoResolveArgs {
        min_age: DEFAULT_AUTO_RESOLVE_MIN_AGE,
    };
    let mut i = 0;
    let mut got_min_age = false;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("min-age") => {
                let (v, ni) = take_value(tail, i, None, "--min-age")?;
                out.min_age = parse_i64(v, "--min-age")?;
                got_min_age = true;
                i = ni;
            }
            FlagSplit::LongAttached("min-age", v) => {
                out.min_age = parse_i64(v, "--min-age")?;
                got_min_age = true;
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    // Touch to silence unused var in the !got_min_age branch.
    let _ = got_min_age;
    Ok(out)
}

/// Mirror of `datum_core::conflicts::AUTO_RESOLVE_MIN_AGE_SECS`. Kept
/// here so the parse crate stays `datum-core`-free; if the datum-core
/// constant changes, update this. Tested for parity in
/// `datum-commands/src/cmd.rs`.
const DEFAULT_AUTO_RESOLVE_MIN_AGE: i64 = 86_400;

// ─── Small helpers ───────────────────────────────────────────────────────

fn looks_like_flag(tok: &str) -> bool {
    match split_flag(tok) {
        FlagSplit::Long(_)
        | FlagSplit::LongAttached(_, _)
        | FlagSplit::Short(_)
        | FlagSplit::ShortAttached(_, _) => true,
        FlagSplit::Positional(_) => false,
    }
}

fn unknown_flag(f: FlagSplit<'_>) -> ParseError {
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

fn parse_usize(s: &str, label: &str) -> Result<usize> {
    s.parse::<usize>()
        .map_err(|_| ParseError::InvalidArg(format!("invalid number for {label}: '{s}'")))
}

fn parse_u32(s: &str, label: &str) -> Result<u32> {
    s.parse::<u32>()
        .map_err(|_| ParseError::InvalidArg(format!("invalid number for {label}: '{s}'")))
}

fn parse_i64(s: &str, label: &str) -> Result<i64> {
    s.parse::<i64>()
        .map_err(|_| ParseError::InvalidArg(format!("invalid number for {label}: '{s}'")))
}

fn parse_f64(s: &str, label: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| ParseError::InvalidArg(format!("invalid number for {label}: '{s}'")))
}

fn require_pos(pos: &[&str], label: &str) -> Result<String> {
    pos.first()
        .map(|s| s.to_string())
        .ok_or_else(|| ParseError::InvalidArg(format!("missing required positional: {label}")))
}

fn require_first_pos(pos: &[&str], label: &str) -> Result<String> {
    require_pos(pos, label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        std::iter::once("datum")
            .chain(words.iter().copied())
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn occur_minimal() {
        let cli = parse_argv(&argv(&["occur", "coffee"])).unwrap();
        match cli.command {
            Command::Occur(a) => {
                assert_eq!(a.field_or_verb, "coffee");
                assert!(a.rest.is_empty());
            }
            _ => panic!("expected Occur"),
        }
    }

    #[test]
    fn occur_with_global_db_flag() {
        let cli = parse_argv(&argv(&["--db", "work", "occur", "coffee"])).unwrap();
        assert_eq!(cli.db.as_deref(), Some("work"));
    }

    #[test]
    fn bare_invocation_is_repl() {
        let cli = parse_argv(&argv(&[])).unwrap();
        assert!(matches!(cli.command, Command::Repl));
        assert!(cli.db.is_none());
    }

    #[test]
    fn db_flag_without_subcommand_is_repl() {
        // da-g6nl: `dm --db finance` should enter REPL with the DB hint,
        // not error on a missing subcommand.
        let cli = parse_argv(&argv(&["--db", "finance"])).unwrap();
        assert!(matches!(cli.command, Command::Repl));
        assert_eq!(cli.db.as_deref(), Some("finance"));
    }

    #[test]
    fn db_flag_eq_form_without_subcommand_is_repl() {
        let cli = parse_argv(&argv(&["--db=finance"])).unwrap();
        assert!(matches!(cli.command, Command::Repl));
        assert_eq!(cli.db.as_deref(), Some("finance"));
    }

    #[test]
    fn db_flag_with_subcommand_still_dispatches() {
        // Counterpart: `--db NAME` followed by a real verb still runs
        // that verb against the named DB.
        let cli = parse_argv(&argv(&["--db", "finance", "get", "xyz"])).unwrap();
        assert!(matches!(cli.command, Command::Get(_)));
        assert_eq!(cli.db.as_deref(), Some("finance"));
    }

    #[test]
    fn tail_watch_default_missing_value() {
        let cli = parse_argv(&argv(&["tail", "-w"])).unwrap();
        match cli.command {
            Command::Tail(a) => assert_eq!(a.watch, Some(1.0)),
            _ => panic!("expected Tail"),
        }
    }

    #[test]
    fn tail_watch_with_value() {
        let cli = parse_argv(&argv(&["tail", "-w", "5"])).unwrap();
        match cli.command {
            Command::Tail(a) => assert_eq!(a.watch, Some(5.0)),
            _ => panic!("expected Tail"),
        }
    }

    #[test]
    fn long_eq_form_negative() {
        // --time=-5 should be accepted even though -5 starts with a hyphen.
        let cli = parse_argv(&argv(&["occur", "--time=-5", "coffee"])).unwrap();
        match cli.command {
            Command::Occur(a) => assert_eq!(a.time.time.as_deref(), Some("-5")),
            _ => panic!(),
        }
    }

    #[test]
    fn short_eq_form_negative() {
        // -t=-5
        let cli = parse_argv(&argv(&["occur", "-t=-5", "coffee"])).unwrap();
        match cli.command {
            Command::Occur(a) => assert_eq!(a.time.time.as_deref(), Some("-5")),
            _ => panic!(),
        }
    }

    #[test]
    fn del_alias() {
        let cli = parse_argv(&argv(&["del", "some:id", "--force"])).unwrap();
        match cli.command {
            Command::Delete(a) => {
                assert_eq!(a.id_or_slug.as_deref(), Some("some:id"));
                assert!(a.force);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn bare_add_missing_field_errors() {
        let err = parse_argv(&argv(&["add"])).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("missing required") || msg.contains("FIELD"),
            "expected required-field error, got: {msg}"
        );
    }
}
