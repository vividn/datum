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
fn shell_split(s: &str) -> Vec<String> {
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
            return Err(ParseError::InvalidArg(
                "missing subcommand — try `datum --help`".into(),
            ));
        };
        match split_flag(tok) {
            FlagSplit::LongAttached("db", v) => {
                db = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("db") => {
                let v = tail.get(i + 1).ok_or_else(|| {
                    ParseError::InvalidArg("--db requires a value".into())
                })?;
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

        "track" => Command::Track(parse_track_args(rest)?),
        "ignore" => Command::Ignore(parse_ignore_args(rest)?),

        "migrate" => Command::Migrate(parse_migrate_cmd(rest)?),

        "backup" => Command::Backup(parse_backup_args(rest)?),
        "restore" => Command::Restore(parse_restore_args(rest)?),

        "grep" => Command::Grep(parse_grep_args(rest)?),
        "check" => Command::Check(parse_check_args(rest)?),

        "count" => Command::Count(parse_count_args(rest)?),
        "ids" => Command::Ids(parse_ids_args(rest)?),

        "dayview" => Command::Dayview(parse_dayview_args(rest)?),

        "info" => Command::Info(parse_info_args(rest)?),
        "setup" => Command::Setup(parse_setup_args(rest)?),

        "config" => Command::Config(parse_config_cmd(rest)?),

        "sync" => Command::Sync(parse_sync_args(rest)?),

        "conflicts" => Command::Conflicts(parse_conflicts_cmd(rest)?),

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
    Long(&'a str),                 // --name (no attached value)
    LongAttached(&'a str, &'a str), // --name=value
    Short(char),                   // -c
    ShortAttached(char, &'a str),  // -c=value, or -cVALUE (only `-c=` form used here)
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

// ─── Output-flag parsing (shared across every subcommand) ────────────────

/// Try to consume one `--show` / `-A` / `-o` / `--format-string` /
/// `--show-all` flag at `i`. Returns the new position if consumed, or
/// `None` if the token at `i` isn't one of our output flags.
fn try_consume_output_flag(
    tail: &[&str],
    i: usize,
    out: &mut OutputFlags,
) -> Result<Option<usize>> {
    let tok = tail[i];
    match split_flag(tok) {
        FlagSplit::Long("show") => {
            let (v, ni) = take_value(tail, i, None, "--show")?;
            out.show = v.to_string();
            Ok(Some(ni))
        }
        FlagSplit::LongAttached("show", v) => {
            out.show = v.to_string();
            Ok(Some(i + 1))
        }
        FlagSplit::Long("show-all") | FlagSplit::Short('A') => {
            out.show_all = true;
            Ok(Some(i + 1))
        }
        FlagSplit::LongAttached("show-all", _) => {
            out.show_all = true;
            Ok(Some(i + 1))
        }
        FlagSplit::Long("format-string") | FlagSplit::Short('o') => {
            let lbl = if tok.starts_with("--") { "--format-string" } else { "-o" };
            let (v, ni) = take_value(tail, i, None, lbl)?;
            out.format_string = Some(v.to_string());
            Ok(Some(ni))
        }
        FlagSplit::LongAttached("format-string", v) | FlagSplit::ShortAttached('o', v) => {
            out.format_string = Some(v.to_string());
            Ok(Some(i + 1))
        }
        _ => Ok(None),
    }
}

// ─── Subcommand parsers ──────────────────────────────────────────────────

fn parse_log_args(tail: &[&str]) -> Result<LogArgs> {
    let mut out = LogArgs::fresh(String::new());
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        if let Some(ni) = try_consume_output_flag(tail, i, &mut out.output_flags)? {
            i = ni;
            continue;
        }
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Short('y') => {
                out.yesterday = out.yesterday.saturating_add(1);
                i += 1;
            }
            FlagSplit::Short('q') => {
                out.quick = out.quick.saturating_add(1);
                i += 1;
            }
            FlagSplit::Short('d') => {
                let (v, ni) = take_value(tail, i, None, "-d")?;
                out.date = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('d', v) | FlagSplit::LongAttached("date", v) => {
                out.date = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("date") => {
                let (v, ni) = take_value(tail, i, None, "--date")?;
                out.date = Some(v.to_string());
                i = ni;
            }
            FlagSplit::Short('t') => {
                let (v, ni) = take_value(tail, i, None, "-t")?;
                out.time = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('t', v) | FlagSplit::LongAttached("time", v) => {
                out.time = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("time") => {
                let (v, ni) = take_value(tail, i, None, "--time")?;
                out.time = Some(v.to_string());
                i = ni;
            }
            FlagSplit::Long("dur") => {
                let (v, ni) = take_value(tail, i, None, "--dur")?;
                out.dur = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("dur", v) => {
                out.dur = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('z') | FlagSplit::Long("timezone") => {
                let (v, ni) = take_value(tail, i, None, "-z/--timezone")?;
                out.timezone = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('z', v) | FlagSplit::LongAttached("timezone", v) => {
                out.timezone = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('u') | FlagSplit::Long("unit") => {
                let (v, ni) = take_value(tail, i, None, "-u/--unit")?;
                out.unit = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('u', v) | FlagSplit::LongAttached("unit", v) => {
                out.unit = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('k') | FlagSplit::Long("key") => {
                let (v, ni) = take_value(tail, i, None, "-k/--key")?;
                out.keys.push(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('k', v) | FlagSplit::LongAttached("key", v) => {
                out.keys.push(v.to_string());
                i += 1;
            }
            FlagSplit::Short('K') | FlagSplit::Long("slug-key") => {
                let (v, ni) = take_value(tail, i, None, "-K/--slug-key")?;
                out.slug_keys.push(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('K', v) | FlagSplit::LongAttached("slug-key", v) => {
                out.slug_keys.push(v.to_string());
                i += 1;
            }
            FlagSplit::Short('c') | FlagSplit::Long("comment") => {
                let (v, ni) = take_value(tail, i, None, "-c/--comment")?;
                out.comment = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('c', v) | FlagSplit::LongAttached("comment", v) => {
                out.comment = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("id") => {
                let (v, ni) = take_value(tail, i, None, "--id")?;
                out.id = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("id", v) => {
                out.id = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("id-delimiter") => {
                let (v, ni) = take_value(tail, i, None, "--id-delimiter")?;
                out.id_delimiter = v.to_string();
                i = ni;
            }
            FlagSplit::LongAttached("id-delimiter", v) => {
                out.id_delimiter = v.to_string();
                i += 1;
            }
            FlagSplit::Short('L') | FlagSplit::Long("last-state") => {
                // `require_equals = true` — bare form is no-value.
                out.last_state = Some(String::new());
                i += 1;
            }
            FlagSplit::ShortAttached('L', v) | FlagSplit::LongAttached("last-state", v) => {
                out.last_state = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('m') | FlagSplit::Long("moment") => {
                out.moment = true;
                i += 1;
            }
            FlagSplit::Long("conflict") => {
                let (v, ni) = take_value(tail, i, None, "--conflict")?;
                out.conflict = Some(parse_conflict_strategy(v)?);
                i = ni;
            }
            FlagSplit::LongAttached("conflict", v) => {
                out.conflict = Some(parse_conflict_strategy(v)?);
                i += 1;
            }
            FlagSplit::Long("update") => {
                if out.merge {
                    return Err(ParseError::InvalidArg(
                        "--update conflicts with --merge".into(),
                    ));
                }
                out.update = true;
                i += 1;
            }
            FlagSplit::Long("merge") => {
                if out.update {
                    return Err(ParseError::InvalidArg(
                        "--merge conflicts with --update".into(),
                    ));
                }
                out.merge = true;
                i += 1;
            }
            FlagSplit::Short('M') | FlagSplit::Long("no-metadata") => {
                out.no_metadata = true;
                i += 1;
            }
            FlagSplit::Short('T') | FlagSplit::Long("no-timestamp") => {
                out.no_timestamp = true;
                i += 1;
            }
            FlagSplit::Long(name) | FlagSplit::LongAttached(name, _) => {
                return Err(ParseError::InvalidArg(format!(
                    "unexpected flag: --{name}"
                )));
            }
            FlagSplit::Short(c) | FlagSplit::ShortAttached(c, _) => {
                return Err(ParseError::InvalidArg(format!(
                    "unexpected flag: -{c}"
                )));
            }
        }
    }
    // Required positional: field_or_verb.
    let mut it = pos.into_iter();
    out.field_or_verb = it
        .next()
        .ok_or_else(|| {
            ParseError::InvalidArg(
                "missing required positional: <FIELD_OR_VERB>".into(),
            )
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

fn parse_tail_args(tail: &[&str]) -> Result<TailArgs> {
    let mut out = TailArgs::fresh();
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        if let Some(ni) = try_consume_output_flag(tail, i, &mut out.output_flags)? {
            i = ni;
            continue;
        }
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Short('y') => { out.yesterday = out.yesterday.saturating_add(1); i += 1; }
            FlagSplit::Short('q') => { out.quick = out.quick.saturating_add(1); i += 1; }
            FlagSplit::Short('n') => {
                let (v, ni) = take_value(tail, i, None, "-n")?;
                out.count = Some(parse_usize(v, "-n")?);
                i = ni;
            }
            FlagSplit::ShortAttached('n', v) => {
                out.count = Some(parse_usize(v, "-n")?);
                i += 1;
            }
            FlagSplit::Short('m') | FlagSplit::Long("metric") => {
                let (v, ni) = take_value(tail, i, None, "-m/--metric")?;
                out.metric = v.to_string();
                i = ni;
            }
            FlagSplit::ShortAttached('m', v) | FlagSplit::LongAttached("metric", v) => {
                out.metric = v.to_string();
                i += 1;
            }
            FlagSplit::Short('r') | FlagSplit::Long("reverse") => {
                out.reverse = true;
                i += 1;
            }
            FlagSplit::Long("column") => {
                let (v, ni) = take_value(tail, i, None, "--column")?;
                out.columns.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("column", v) => {
                out.columns.push(v.to_string());
                i += 1;
            }
            FlagSplit::Short('d') | FlagSplit::Long("date") => {
                let (v, ni) = take_value(tail, i, None, "-d/--date")?;
                out.date = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('d', v) | FlagSplit::LongAttached("date", v) => {
                out.date = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('t') | FlagSplit::Long("time") => {
                let (v, ni) = take_value(tail, i, None, "-t/--time")?;
                out.time = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('t', v) | FlagSplit::LongAttached("time", v) => {
                out.time = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("oldest-first") => {
                out.oldest_first = true;
                i += 1;
            }
            FlagSplit::Long("table") => {
                out.table = true;
                i += 1;
            }
            FlagSplit::Short('w') | FlagSplit::Long("watch") => {
                // num_args = 0..=1: default_missing_value = 1. Try to peek
                // at the next token; if it looks like a flag / subcommand,
                // it's the no-value form.
                let next = tail.get(i + 1).map(|s| *s);
                match next {
                    Some(v) if !looks_like_flag(v) => {
                        out.watch = Some(parse_f64(v, "-w/--watch")?);
                        i += 2;
                    }
                    _ => {
                        out.watch = Some(1.0);
                        i += 1;
                    }
                }
            }
            FlagSplit::ShortAttached('w', v) | FlagSplit::LongAttached("watch", v) => {
                out.watch = Some(parse_f64(v, "-w/--watch")?);
                i += 1;
            }
            FlagSplit::Long(name) | FlagSplit::LongAttached(name, _) => {
                return Err(ParseError::InvalidArg(format!(
                    "unexpected flag: --{name}"
                )));
            }
            FlagSplit::Short(c) | FlagSplit::ShortAttached(c, _) => {
                return Err(ParseError::InvalidArg(format!(
                    "unexpected flag: -{c}"
                )));
            }
        }
    }
    if let Some(f) = pos.first() {
        out.field = Some(f.to_string());
    }
    Ok(out)
}

fn parse_now_args(tail: &[&str]) -> Result<NowArgs> {
    let mut out = NowArgs::default();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Short('A') => out.show_all = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

fn parse_get_args(tail: &[&str]) -> Result<GetArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut pretty = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("pretty") => pretty = true,
            f => return Err(unknown_flag(f)),
        }
    }
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(GetArgs { id_or_slug, pretty })
}

fn parse_delete_args(tail: &[&str]) -> Result<DeleteArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut out = DeleteArgs {
        id_or_slug: String::new(),
        force: false,
        dry_run: false,
        strict: false,
        output_flags: OutputFlags::fresh(),
    };
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        if let Some(ni) = try_consume_output_flag(tail, i, &mut out.output_flags)? {
            i = ni;
            continue;
        }
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Long("force") => { out.force = true; i += 1; }
            FlagSplit::Long("dry-run") => { out.dry_run = true; i += 1; }
            FlagSplit::Long("strict") => { out.strict = true; i += 1; }
            f => return Err(unknown_flag(f)),
        }
    }
    out.id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(out)
}

fn parse_undo_args(tail: &[&str]) -> Result<UndoArgs> {
    let mut out = UndoArgs::fresh();
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        if let Some(ni) = try_consume_output_flag(tail, i, &mut out.output_flags)? {
            i = ni;
            continue;
        }
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Long("to-version") => {
                let (v, ni) = take_value(tail, i, None, "--to-version")?;
                out.to_version = Some(parse_u32(v, "--to-version")?);
                i = ni;
            }
            FlagSplit::LongAttached("to-version", v) => {
                out.to_version = Some(parse_u32(v, "--to-version")?);
                i += 1;
            }
            FlagSplit::Long("force") => { out.force = true; i += 1; }
            FlagSplit::Long("window") => {
                let (v, ni) = take_value(tail, i, None, "--window")?;
                out.window = v.to_string();
                i = ni;
            }
            FlagSplit::LongAttached("window", v) => {
                out.window = v.to_string();
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if let Some(p) = pos.first() {
        out.id_or_slug = Some(p.to_string());
    }
    Ok(out)
}

fn parse_history_args(tail: &[&str]) -> Result<HistoryArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut version: Option<u32> = None;
    let mut diff: Option<Vec<u32>> = None;
    let mut json = false;
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Long("version") => {
                let (v, ni) = take_value(tail, i, None, "--version")?;
                version = Some(parse_u32(v, "--version")?);
                i = ni;
            }
            FlagSplit::LongAttached("version", v) => {
                version = Some(parse_u32(v, "--version")?);
                i += 1;
            }
            FlagSplit::Long("diff") => {
                let a = tail.get(i + 1).ok_or_else(|| {
                    ParseError::InvalidArg("--diff requires two values".into())
                })?;
                let b = tail.get(i + 2).ok_or_else(|| {
                    ParseError::InvalidArg("--diff requires two values".into())
                })?;
                diff = Some(vec![parse_u32(a, "--diff")?, parse_u32(b, "--diff")?]);
                i += 3;
            }
            FlagSplit::Long("json") => { json = true; i += 1; }
            f => return Err(unknown_flag(f)),
        }
    }
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(HistoryArgs { id_or_slug, version, diff, json })
}

fn parse_why_args(tail: &[&str]) -> Result<WhyArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut full = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("full") => full = true,
            f => return Err(unknown_flag(f)),
        }
    }
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(WhyArgs { id_or_slug, full })
}

fn parse_edit_args(tail: &[&str]) -> Result<EditArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut out_flags = OutputFlags::fresh();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        if let Some(ni) = try_consume_output_flag(tail, i, &mut out_flags)? {
            i = ni;
            continue;
        }
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            f => return Err(unknown_flag(f)),
        }
    }
    let id_or_slug = require_pos(&pos, "<ID_OR_SLUG>")?;
    Ok(EditArgs { id_or_slug, output_flags: out_flags })
}

fn parse_update_args(tail: &[&str]) -> Result<UpdateArgs> {
    let mut out = UpdateArgs::fresh(String::new());
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        if let Some(ni) = try_consume_output_flag(tail, i, &mut out.output_flags)? {
            i = ni;
            continue;
        }
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Long("strategy") => {
                let (v, ni) = take_value(tail, i, None, "--strategy")?;
                out.strategy = Some(parse_conflict_strategy(v)?);
                i = ni;
            }
            FlagSplit::LongAttached("strategy", v) => {
                out.strategy = Some(parse_conflict_strategy(v)?);
                i += 1;
            }
            FlagSplit::Long("id") => {
                let (v, ni) = take_value(tail, i, None, "--id")?;
                out.id = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("id", v) => {
                out.id = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("id-delimiter") => {
                let (v, ni) = take_value(tail, i, None, "--id-delimiter")?;
                out.id_delimiter = v.to_string();
                i = ni;
            }
            FlagSplit::LongAttached("id-delimiter", v) => {
                out.id_delimiter = v.to_string();
                i += 1;
            }
            FlagSplit::Short('k') | FlagSplit::Long("key") => {
                let (v, ni) = take_value(tail, i, None, "-k/--key")?;
                out.keys.push(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('k', v) | FlagSplit::LongAttached("key", v) => {
                out.keys.push(v.to_string());
                i += 1;
            }
            FlagSplit::Short('c') | FlagSplit::Long("comment") => {
                let (v, ni) = take_value(tail, i, None, "-c/--comment")?;
                out.comment = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('c', v) | FlagSplit::LongAttached("comment", v) => {
                out.comment = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("dry-run") => { out.dry_run = true; i += 1; }
            FlagSplit::Long("strict") => { out.strict = true; i += 1; }
            f => return Err(unknown_flag(f)),
        }
    }
    let mut it = pos.into_iter();
    out.id_or_slug = it
        .next()
        .ok_or_else(|| ParseError::InvalidArg("missing required positional: <ID_OR_SLUG>".into()))?
        .to_string();
    out.rest = it.map(|s| s.to_string()).collect();
    Ok(out)
}

fn parse_retime_args(tail: &[&str]) -> Result<RetimeArgs> {
    let mut out = RetimeArgs {
        slug: None,
        offset: None,
        time: None,
        field: None,
        after: None,
        before: None,
    };
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Long("offset") => {
                let (v, ni) = take_value(tail, i, None, "--offset")?;
                out.offset = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("offset", v) => {
                out.offset = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('t') | FlagSplit::Long("time") => {
                let (v, ni) = take_value(tail, i, None, "-t/--time")?;
                out.time = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('t', v) | FlagSplit::LongAttached("time", v) => {
                out.time = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Short('f') | FlagSplit::Long("field") => {
                let (v, ni) = take_value(tail, i, None, "-f/--field")?;
                out.field = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('f', v) | FlagSplit::LongAttached("field", v) => {
                out.field = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("after") => {
                let (v, ni) = take_value(tail, i, None, "--after")?;
                out.after = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("after", v) => {
                out.after = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("before") => {
                let (v, ni) = take_value(tail, i, None, "--before")?;
                out.before = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("before", v) => {
                out.before = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if out.offset.is_some() && out.time.is_some() {
        return Err(ParseError::InvalidArg(
            "--offset and --time are mutually exclusive".into(),
        ));
    }
    if let Some(p) = pos.first() {
        out.slug = Some(p.to_string());
    }
    Ok(out)
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
        "set" => Ok(SpecCommand::Set(parse_spec_set(rest)?)),
        "list" => Ok(SpecCommand::List),
        "remove" => {
            let field = require_first_pos(rest, "<FIELD>")?;
            Ok(SpecCommand::Remove { field })
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown spec subcommand: '{other}'"
        ))),
    }
}

fn parse_spec_set(tail: &[&str]) -> Result<SpecSetArgs> {
    let mut out = SpecSetArgs::fresh(String::new());
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
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
            FlagSplit::Long("track") => { out.track = true; i += 1; }
            FlagSplit::Long("ignore") => { out.ignore = true; i += 1; }
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
            FlagSplit::Long("no-autoalias") => { out.no_autoalias = true; i += 1; }
            f => return Err(unknown_flag(f)),
        }
    }
    out.field = require_first_pos(&pos.iter().copied().collect::<Vec<_>>(), "<FIELD>")?;
    Ok(out)
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

fn parse_track_args(tail: &[&str]) -> Result<TrackArgs> {
    let mut out = TrackArgs {
        field: String::new(),
        color: None,
        state_colors: Vec::new(),
    };
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Long("color") => {
                let (v, ni) = take_value(tail, i, None, "--color")?;
                out.color = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("color", v) => {
                out.color = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("state") => {
                let (v, ni) = take_value(tail, i, None, "--state")?;
                out.state_colors.push(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("state", v) => {
                out.state_colors.push(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    out.field = require_first_pos(&pos, "<FIELD>")?;
    Ok(out)
}

fn parse_ignore_args(tail: &[&str]) -> Result<IgnoreArgs> {
    let mut pos: Vec<&str> = Vec::new();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            f => return Err(unknown_flag(f)),
        }
    }
    let field = require_first_pos(&pos, "<FIELD>")?;
    Ok(IgnoreArgs { field })
}

fn parse_migrate_cmd(tail: &[&str]) -> Result<MigrateCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing migrate subcommand (run|status|edit|import)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "run" => Ok(MigrateCommand::Run),
        "status" => Ok(MigrateCommand::Status),
        "edit" => Ok(MigrateCommand::Edit),
        "import" => Ok(MigrateCommand::Import(parse_migrate_import(rest)?)),
        other => Err(ParseError::InvalidArg(format!(
            "unknown migrate subcommand: '{other}'"
        ))),
    }
}

fn parse_migrate_import(tail: &[&str]) -> Result<MigrateImportArgs> {
    let mut out = MigrateImportArgs::default();
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Long("from") | FlagSplit::Long("backup") => {
                let (v, ni) = take_value(tail, i, None, "--from")?;
                out.backup_file = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("from", v) | FlagSplit::LongAttached("backup", v) => {
                out.backup_file = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("dry-run") => { out.dry_run = true; i += 1; }
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
    Ok(RestoreArgs { path, force, dry_run, json })
}

fn parse_grep_args(tail: &[&str]) -> Result<GrepArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut field: Option<String> = None;
    let mut out_flags = OutputFlags::fresh();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        if let Some(ni) = try_consume_output_flag(tail, i, &mut out_flags)? {
            i = ni;
            continue;
        }
        match split_flag(tok) {
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Short('f') | FlagSplit::Long("field") => {
                let (v, ni) = take_value(tail, i, None, "-f/--field")?;
                field = Some(v.to_string());
                i = ni;
            }
            FlagSplit::ShortAttached('f', v) | FlagSplit::LongAttached("field", v) => {
                field = Some(v.to_string());
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
        field,
        output_flags: out_flags,
    })
}

fn parse_check_args(tail: &[&str]) -> Result<CheckArgs> {
    let mut out = CheckArgs::default();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("json") => out.json = true,
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
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
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
            FlagSplit::Long("json") => { out.json = true; i += 1; }
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
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Short('n') | FlagSplit::Long("limit") => {
                let (v, ni) = take_value(tail, i, None, "-n/--limit")?;
                out.limit = Some(parse_usize(v, "-n/--limit")?);
                i = ni;
            }
            FlagSplit::ShortAttached('n', v) | FlagSplit::LongAttached("limit", v) => {
                out.limit = Some(parse_usize(v, "-n/--limit")?);
                i += 1;
            }
            FlagSplit::Long("oldest-first") => { out.oldest_first = true; i += 1; }
            FlagSplit::Long("since") => {
                let (v, ni) = take_value(tail, i, None, "--since")?;
                out.since = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("since", v) => {
                out.since = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("slug") => { out.slug = true; i += 1; }
            FlagSplit::Long("json") => { out.json = true; i += 1; }
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
            FlagSplit::Long("date") => {
                let (v, ni) = take_value(tail, i, None, "--date")?;
                out.date = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("date", v) => {
                out.date = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("output") => {
                let (v, ni) = take_value(tail, i, None, "--output")?;
                out.output = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("output", v) => {
                out.output = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("json") => { out.json = true; i += 1; }
            FlagSplit::Long("width") => {
                let (v, ni) = take_value(tail, i, None, "--width")?;
                out.width = parse_f64(v, "--width")?;
                i = ni;
            }
            FlagSplit::LongAttached("width", v) => {
                out.width = parse_f64(v, "--width")?;
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

fn parse_setup_args(tail: &[&str]) -> Result<SetupArgs> {
    let mut force_recreate = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("force-recreate") => force_recreate = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(SetupArgs { force_recreate })
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
                        "install-daemon" => {
                            SyncCommand::InstallDaemon(parse_sync_install(rest)?)
                        }
                        _ => unreachable!(),
                    });
                    return Ok(args);
                }
                return Err(ParseError::InvalidArg(format!(
                    "unexpected positional: {s}"
                )));
            }
            FlagSplit::Long("push") => { args.push = true; i += 1; }
            FlagSplit::Long("pull") => { args.pull = true; i += 1; }
            FlagSplit::Long("dry-run") => { args.dry_run = true; i += 1; }
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
                    FlagSplit::Long("force") => { force = true; i += 1; }
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
            FlagSplit::Long("all") => { out.all = true; i += 1; }
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
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
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
            FlagSplit::Positional(s) => { pos.push(s); i += 1; }
            FlagSplit::Long("keep") => {
                let (v, ni) = take_value(tail, i, None, "--keep")?;
                keep = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("keep", v) => {
                keep = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("close-older") => { close_older = true; i += 1; }
            FlagSplit::Long("take") => {
                let (v, ni) = take_value(tail, i, None, "--take")?;
                take = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("take", v) => {
                take = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("edit") => { edit = true; i += 1; }
            FlagSplit::Long("skip") => { skip = true; i += 1; }
            f => return Err(unknown_flag(f)),
        }
    }
    let hid = require_first_pos(&pos, "<HID>")?;
    let chosen = [
        keep.is_some(),
        close_older,
        take.is_some(),
        edit,
        skip,
    ]
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
    Ok(ConflictsResolveArgs { hid, keep, close_older, take, edit, skip })
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
    let mut out = ConflictsAutoResolveArgs { min_age: DEFAULT_AUTO_RESOLVE_MIN_AGE };
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
        FlagSplit::Positional(s) => ParseError::InvalidArg(format!(
            "unexpected positional argument: {s}"
        )),
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
            Command::Occur(a) => assert_eq!(a.time.as_deref(), Some("-5")),
            _ => panic!(),
        }
    }

    #[test]
    fn short_eq_form_negative() {
        // -t=-5
        let cli = parse_argv(&argv(&["occur", "-t=-5", "coffee"])).unwrap();
        match cli.command {
            Command::Occur(a) => assert_eq!(a.time.as_deref(), Some("-5")),
            _ => panic!(),
        }
    }

    #[test]
    fn del_alias() {
        let cli = parse_argv(&argv(&["del", "some:id", "--force"])).unwrap();
        match cli.command {
            Command::Delete(a) => {
                assert_eq!(a.id_or_slug, "some:id");
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
