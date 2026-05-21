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

        "meta" => Command::Meta(parse_meta_cmd(rest)?),

        "flag" => Command::Flag(parse_flag_cmd(rest)?),

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
    // Time concern (-y/-q/-d/-t/-z/-T/-D).
    for spec in input::time_args::FLAG_SPECS {
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
        p.id_delimiter = Some(v.to_string());
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
    r.push(local_switch(None, Some("dry-run"), |p| {
        p.dry_run = true;
        Ok(())
    }));
    r.push(local_switch(None, Some("dryrun"), |p| {
        p.dry_run = true;
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
    show_meta: bool,
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
fn split_field_state(raw: &str) -> (Vec<String>, Option<Vec<String>>) {
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

fn parse_now_args(tail: &[&str]) -> Result<NowArgs> {
    // `now` consumes `-A/--show-all` (via OutputArgs registry, kept
    // canonical) plus its own band-rendering flags: `--bare`, `--no-color`,
    // `--cols`.
    #[derive(Default)]
    struct NowParsing {
        output: OutputArgs,
        show_tracked: bool,
        json: bool,
        bare: bool,
        no_color: bool,
        cols: Option<u16>,
    }
    let mut registry: Vec<LiftedSpec<NowParsing>> = input::output_args::FLAG_SPECS
        .iter()
        .filter(|s| s.short == Some('A'))
        .map(|s| lift(|p: &mut NowParsing| &mut p.output, s))
        .collect();
    registry.push(local_switch(Some('t'), Some("tracked"), |p| {
        p.show_tracked = true;
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
        show_all: composed.output.show_all,
        show_tracked: composed.show_tracked,
        json: composed.json,
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

    // da-yhh1: split positionals into the leading quickId/field plus a
    // tail of `k OP v` matcher tokens. The first positional is always
    // the entry/field target; every later positional must parse as a
    // matcher token. A bare later positional with no operator is a
    // user error — `datum get sleep happy` is meaningless (did they
    // mean `state=happy`?).
    let mut id_or_slug = None;
    let mut matchers: Vec<crate::input::Matcher> = Vec::new();
    for (i, tok) in pos.iter().enumerate() {
        if i == 0 {
            id_or_slug = Some((*tok).to_string());
            continue;
        }
        match crate::input::parse_matcher_token(tok).map_err(ParseError::InvalidArg)? {
            Some(m) => matchers.push(m),
            None => {
                return Err(ParseError::InvalidArg(format!(
                    "`datum get` positional '{tok}' is not a matcher (expected k=v, k>v, k~v, or named forms like k=gt=v)"
                )));
            }
        }
    }

    Ok(GetArgs {
        id_or_slug,
        pretty: composed.pretty,
        output: composed.output,
        matchers,
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
        id_delimiter: composed.id_delimiter,
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
            FlagSplit::Long("strict") => {
                out.strict = Some(true);
                i += 1;
            }
            FlagSplit::Long("no-strict") => {
                out.strict = Some(false);
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

fn parse_meta_cmd(tail: &[&str]) -> Result<MetaCommand> {
    // No flag-level knobs — purely positional. Two forms:
    //   `meta show <field>`  → MetaCommand::Show
    //   `meta <field> <text...>` → MetaCommand::Add (text joined on space)
    let mut pos: Vec<&str> = Vec::new();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            f => return Err(unknown_flag(f)),
        }
    }
    if pos.first().copied() == Some("show") {
        let field = pos
            .get(1)
            .copied()
            .ok_or_else(|| ParseError::InvalidArg("`datum meta show` requires <FIELD>".into()))?;
        if pos.len() > 2 {
            return Err(ParseError::InvalidArg(format!(
                "`datum meta show {field}` takes no further positionals (got {} extra)",
                pos.len() - 2
            )));
        }
        return Ok(MetaCommand::Show {
            field: field.to_string(),
        });
    }
    let field = pos
        .first()
        .copied()
        .ok_or_else(|| ParseError::InvalidArg("`datum meta` requires <FIELD>".into()))?;
    if pos.len() < 2 {
        return Err(ParseError::InvalidArg(format!(
            "`datum meta {field}` requires a note (e.g. `datum meta {field} \"renaming X as Y\"`)"
        )));
    }
    Ok(MetaCommand::Add {
        field: field.to_string(),
        text: pos[1..].join(" "),
    })
}

/// `datum flag <field> [comment]` — field positional + optional
/// comment positionals joined on space + standard timing flags
/// (`-y/-q/-d/-t/-z`). Honors back-dated time so a flag can be placed
/// at a specific past moment.
#[derive(Default)]
struct FlagParsing {
    time: TimeArgs,
}

fn flag_args_registry() -> Vec<LiftedSpec<FlagParsing>> {
    let mut r: Vec<LiftedSpec<FlagParsing>> = Vec::new();
    // Mirror TailParsing: expose -y/-q/-d/-t/-z; suppress -T/-D
    // (a flag without a moment loses its meaning).
    for spec in input::time_args::FLAG_SPECS {
        match (spec.short, spec.long) {
            (Some('y'), _) | (Some('q'), _) | (Some('d'), _) | (Some('t'), _) | (Some('z'), _) => {
                r.push(lift(|p: &mut FlagParsing| &mut p.time, spec));
            }
            _ => {}
        }
    }
    r
}

fn parse_flag_cmd(tail: &[&str]) -> Result<FlagArgs> {
    let registry = flag_args_registry();
    let mut composed = FlagParsing::default();
    let pos = fold_args(tail, &mut composed, &registry)?;
    let field = pos
        .first()
        .copied()
        .ok_or_else(|| ParseError::InvalidArg("`datum flag` requires <FIELD>".into()))?;
    let comment = if pos.len() > 1 {
        pos[1..].join(" ")
    } else {
        String::new()
    };
    Ok(FlagArgs {
        field: field.to_string(),
        comment,
        time: composed.time,
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
    fn tail_bare_field_carries_no_state_filter() {
        // da-plbb: a bare field positional leaves state_filter unset and
        // tail renders in entry mode.
        let cli = parse_argv(&argv(&["tail", "mood"])).unwrap();
        match cli.command {
            Command::Tail(a) => {
                assert_eq!(a.fields, vec!["mood".to_string()]);
                assert_eq!(a.state_filter, None);
            }
            _ => panic!("expected Tail"),
        }
    }

    #[test]
    fn tail_comma_fields_carry_no_state_filter() {
        // da-hrnp: multi-field positional stays in entry mode (no `:`).
        let cli = parse_argv(&argv(&["tail", "mood,project"])).unwrap();
        match cli.command {
            Command::Tail(a) => {
                assert_eq!(a.fields, vec!["mood".to_string(), "project".to_string()]);
                assert_eq!(a.state_filter, None);
            }
            _ => panic!("expected Tail"),
        }
    }

    #[test]
    fn tail_field_colon_state_splits_into_filter() {
        // da-plbb: `tail mood:happy` → fields=["mood"], state_filter=["happy"].
        let cli = parse_argv(&argv(&["tail", "mood:happy"])).unwrap();
        match cli.command {
            Command::Tail(a) => {
                assert_eq!(a.fields, vec!["mood".to_string()]);
                assert_eq!(a.state_filter.as_deref(), Some(&["happy".to_string()][..]));
            }
            _ => panic!("expected Tail"),
        }
    }

    #[test]
    fn tail_field_colon_comma_state_splits_into_parallel_filter() {
        // da-plbb: `tail body:vertical,sit` → parallel-state filter. The
        // presence of `:` flips the comma from field-separator to
        // state-element-separator, so `body` is the only field.
        let cli = parse_argv(&argv(&["tail", "body:vertical,sit"])).unwrap();
        match cli.command {
            Command::Tail(a) => {
                assert_eq!(a.fields, vec!["body".to_string()]);
                assert_eq!(
                    a.state_filter.as_deref(),
                    Some(&["vertical".to_string(), "sit".to_string()][..])
                );
            }
            _ => panic!("expected Tail"),
        }
    }

    #[test]
    fn tail_trailing_colon_is_bare_field() {
        // da-plbb: `tail mood:` (empty state spec) treats the colon as a
        // no-op — there's nothing to filter against.
        let cli = parse_argv(&argv(&["tail", "mood:"])).unwrap();
        match cli.command {
            Command::Tail(a) => {
                assert_eq!(a.fields, vec!["mood".to_string()]);
                assert_eq!(a.state_filter, None);
            }
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

    // ─── da-sd96: `dm check` positional checker UX ──────────────────────

    #[test]
    fn check_bare_runs_full_battery() {
        let cli = parse_argv(&argv(&["check"])).unwrap();
        match cli.command {
            Command::Check(a) => {
                assert!(a.checkers.is_empty());
                assert!(!a.fix);
                assert!(!a.list);
            }
            _ => panic!("expected Check"),
        }
    }

    #[test]
    fn check_positional_verbs_only() {
        let cli = parse_argv(&argv(&["check", "verbs"])).unwrap();
        match cli.command {
            Command::Check(a) => assert_eq!(a.checkers, vec!["verbs".to_string()]),
            _ => panic!(),
        }
    }

    #[test]
    fn check_positional_multiple_with_fix() {
        let cli = parse_argv(&argv(&["check", "aliases", "verbs", "--fix"])).unwrap();
        match cli.command {
            Command::Check(a) => {
                assert_eq!(a.checkers, vec!["aliases".to_string(), "verbs".to_string()]);
                assert!(a.fix);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn check_unknown_checker_errors_with_hint() {
        let err = parse_argv(&argv(&["check", "vrebs"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown checker"), "got: {msg}");
        assert!(msg.contains("verbs"), "hint must list known names: {msg}");
        assert!(msg.contains("aliases"), "hint must list known names: {msg}");
        assert!(msg.contains("slugs"), "hint must list known names: {msg}");
    }

    #[test]
    fn check_positional_slugs_only() {
        let cli = parse_argv(&argv(&["check", "slugs", "--fix"])).unwrap();
        match cli.command {
            Command::Check(a) => {
                assert_eq!(a.checkers, vec!["slugs".to_string()]);
                assert!(a.fix);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn check_list_flag() {
        let cli = parse_argv(&argv(&["check", "--list"])).unwrap();
        match cli.command {
            Command::Check(a) => assert!(a.list),
            _ => panic!(),
        }
    }

    /// da-xn8j: bare `datum meta <field> <text>` lands as a MetaCommand::Add
    /// with the text joined on spaces. Required form for the iterative
    /// classification-decision journal.
    #[test]
    fn meta_add_joins_tail_into_text() {
        let cli = parse_argv(&argv(&[
            "meta",
            "body",
            "Split",
            "sit",
            "into",
            "sit_office",
        ]))
        .unwrap();
        match cli.command {
            Command::Meta(MetaCommand::Add { field, text }) => {
                assert_eq!(field, "body");
                assert_eq!(text, "Split sit into sit_office");
            }
            other => panic!("expected MetaCommand::Add, got {other:?}"),
        }
    }

    #[test]
    fn meta_show_with_field() {
        let cli = parse_argv(&argv(&["meta", "show", "mood"])).unwrap();
        match cli.command {
            Command::Meta(MetaCommand::Show { field }) => assert_eq!(field, "mood"),
            other => panic!("expected MetaCommand::Show, got {other:?}"),
        }
    }

    /// `datum meta` with no positional must error — silently dispatching
    /// nothing would mask a user typo.
    #[test]
    fn meta_without_field_errors() {
        let err = parse_argv(&argv(&["meta"])).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("<FIELD>") || msg.contains("FIELD"),
            "got: {msg}"
        );
    }

    /// `datum meta <field>` without a note must error — an empty note
    /// is not a journal entry.
    #[test]
    fn meta_without_note_errors() {
        let err = parse_argv(&argv(&["meta", "body"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("requires a note"), "got: {msg}");
    }

    /// `datum meta show` without a field is an error.
    #[test]
    fn meta_show_without_field_errors() {
        let err = parse_argv(&argv(&["meta", "show"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("<FIELD>"), "got: {msg}");
    }

    // ─── da-yhh1: `datum get` matcher token detection ────────────────────

    /// Bare `datum get` quickId — no matchers, no change to existing
    /// behavior.
    #[test]
    fn get_bare_quickid_has_no_matchers() {
        let cli = parse_argv(&argv(&["get", "u1hrr"])).unwrap();
        match cli.command {
            Command::Get(a) => {
                assert_eq!(a.id_or_slug.as_deref(), Some("u1hrr"));
                assert!(a.matchers.is_empty());
            }
            _ => panic!("expected Get"),
        }
    }

    #[test]
    fn get_field_with_symbol_matchers() {
        let cli = parse_argv(&argv(&["get", "sleep", "state=happy", "dur>PT4H"])).unwrap();
        match cli.command {
            Command::Get(a) => {
                assert_eq!(a.id_or_slug.as_deref(), Some("sleep"));
                assert_eq!(a.matchers.len(), 2);
                assert_eq!(a.matchers[0].key, "state");
                assert_eq!(a.matchers[1].key, "dur");
            }
            _ => panic!("expected Get"),
        }
    }

    #[test]
    fn get_field_with_named_form_matchers() {
        let cli = parse_argv(&argv(&[
            "get",
            "weight",
            "weight=gt=75",
            "state=ne=ignored",
        ]))
        .unwrap();
        match cli.command {
            Command::Get(a) => {
                use crate::input::MatcherOp;
                assert_eq!(a.matchers[0].op, MatcherOp::Gt);
                assert_eq!(a.matchers[1].op, MatcherOp::Ne);
                assert_eq!(a.matchers[1].value, "ignored");
            }
            _ => panic!("expected Get"),
        }
    }

    /// A bare positional after the field is a user error — we won't
    /// silently treat `datum get sleep happy` as anything.
    #[test]
    fn get_bare_positional_after_field_errors() {
        let err = parse_argv(&argv(&["get", "sleep", "happy"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not a matcher"), "got: {msg}");
    }

    // ─── da-dnnn: `datum flag` ─────────────────────────────────────────

    #[test]
    fn flag_bare_field_has_empty_comment() {
        let cli = parse_argv(&argv(&["flag", "body"])).unwrap();
        match cli.command {
            Command::Flag(a) => {
                assert_eq!(a.field, "body");
                assert_eq!(a.comment, "");
                assert!(a.time.time.is_none());
            }
            _ => panic!("expected Flag"),
        }
    }

    #[test]
    fn flag_joins_comment_positionals_on_space() {
        let cli = parse_argv(&argv(&[
            "flag",
            "mood",
            "double-check",
            "yesterday's",
            "data",
        ]))
        .unwrap();
        match cli.command {
            Command::Flag(a) => {
                assert_eq!(a.field, "mood");
                assert_eq!(a.comment, "double-check yesterday's data");
            }
            _ => panic!("expected Flag"),
        }
    }

    #[test]
    fn flag_accepts_time_flags_for_backdating() {
        let cli = parse_argv(&argv(&["flag", "mood", "double-check", "-t", "9:00", "-y"])).unwrap();
        match cli.command {
            Command::Flag(a) => {
                assert_eq!(a.field, "mood");
                assert_eq!(a.comment, "double-check");
                assert_eq!(a.time.time.as_deref(), Some("9:00"));
                assert_eq!(a.time.yesterday, 1);
            }
            _ => panic!("expected Flag"),
        }
    }

    #[test]
    fn flag_without_field_errors() {
        let err = parse_argv(&argv(&["flag"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("<FIELD>"), "got: {msg}");
    }
}
