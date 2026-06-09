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

use crate::alias::expand::shell_split;
use crate::cli::*;
use crate::error::{ParseError, Result};

mod alias;
mod core;
mod flag;
mod get_delete;
mod history;
mod info_db;
mod kit;
mod log;
mod meta;
mod migrate_io;
mod now;
mod retime;
mod search;
mod spec;
mod sync;
mod tail;
mod track_ignore;
mod update;
mod views;

use self::alias::*;
use self::core::*;
use self::flag::*;
use self::get_delete::*;
use self::history::*;
use self::info_db::*;
use self::kit::*;
use self::log::*;
// Re-exported so the help-drift guard (`help::tests`) can compare the
// documented log-verb flags against the flags the parser actually accepts.
#[cfg(test)]
pub(crate) use self::log::log_args_registry;
use self::meta::*;
use self::now::*;
use self::retime::*;
use self::search::*;
use self::spec::*;
use self::sync::*;
use self::tail::*;
use self::track_ignore::*;
use self::update::*;
use self::views::*;

/// Entry point: parse an argv (including argv[0]) into a [`Cli`].
pub fn parse_argv(argv: &[String]) -> Result<Cli> {
    // Strip argv[0] ("datum" or whatever binary name the caller passed).
    let tail: Vec<&str> = argv.iter().skip(1).map(|s| s.as_str()).collect();
    parse_tail(&tail)
}

/// String-API variant: shell-tokenize, then parse.
///
/// Tokenization is delegated to [`shell_split`](crate::alias::expand::shell_split),
/// the single canonical splitter shared with the alias-expansion pipeline.
pub fn parse_argv_string(s: &str) -> Result<Cli> {
    let argv = shell_split(s);
    let tail: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    parse_tail(&tail)
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
    fn sync_login_parses_cluster_flag() {
        // da-3fnd: `--cluster <name>` overrides the file-name default.
        for words in [
            vec!["sync", "login", "--cluster", "household"],
            vec!["sync", "login", "--cluster=household"],
        ] {
            let cli = parse_argv(&argv(&words)).unwrap();
            match cli.command {
                Command::Sync(SyncArgs {
                    command: Some(SyncCommand::Login(login)),
                    ..
                }) => assert_eq!(login.cluster.as_deref(), Some("household")),
                other => panic!("expected sync login, got {other:?}"),
            }
        }
        // Absent flag → None (file-name default applies downstream).
        let cli = parse_argv(&argv(&["sync", "login"])).unwrap();
        match cli.command {
            Command::Sync(SyncArgs {
                command: Some(SyncCommand::Login(login)),
                ..
            }) => assert_eq!(login.cluster, None),
            other => panic!("expected sync login, got {other:?}"),
        }
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

    // ─── da-rorg: retime parser surface ─────────────────────────────────

    #[test]
    fn retime_bare_invocation_has_no_slug_no_offset() {
        // Bare `retime` with no positionals is valid — the command
        // layer fills in `_LAST` and "set to now".
        let cli = parse_argv(&argv(&["retime"])).unwrap();
        match cli.command {
            Command::Retime(a) => {
                assert!(a.slug.is_none());
                assert!(a.offset.is_none());
                assert!(a.bare_offset_min.is_none());
                assert!(!a.relative);
                assert!(a.time.time.is_none() && a.time.yesterday == 0 && a.time.quick == 0);
            }
            _ => panic!("expected Retime"),
        }
    }

    #[test]
    fn retime_bare_numeric_positional_captures_minutes() {
        // `retime <slug> -5` → slug + bare_offset_min=-5.
        let cli = parse_argv(&argv(&["retime", "coffee:2024", "-5"])).unwrap();
        match cli.command {
            Command::Retime(a) => {
                assert_eq!(a.slug.as_deref(), Some("coffee:2024"));
                assert_eq!(a.bare_offset_min, Some(-5));
                assert!(a.offset.is_none());
            }
            _ => panic!("expected Retime"),
        }
    }

    #[test]
    fn retime_bare_positive_numeric_positional() {
        let cli = parse_argv(&argv(&["retime", "mood:abc", "30"])).unwrap();
        match cli.command {
            Command::Retime(a) => {
                assert_eq!(a.slug.as_deref(), Some("mood:abc"));
                assert_eq!(a.bare_offset_min, Some(30));
            }
            _ => panic!("expected Retime"),
        }
    }

    #[test]
    fn retime_numeric_only_positional_defaults_slug_to_last() {
        // `retime -5` (no slug) — bare numeric goes to `bare_offset_min`;
        // the command layer fills slug=_LAST.
        let cli = parse_argv(&argv(&["retime", "-5"])).unwrap();
        match cli.command {
            Command::Retime(a) => {
                assert!(a.slug.is_none());
                assert_eq!(a.bare_offset_min, Some(-5));
            }
            _ => panic!("expected Retime"),
        }
    }

    #[test]
    fn retime_r_flag_sets_relative_bit() {
        let cli = parse_argv(&argv(&["retime", "coffee:abc", "-r", "-t", "-5"])).unwrap();
        match cli.command {
            Command::Retime(a) => {
                assert!(a.relative);
                assert_eq!(a.time.time.as_deref(), Some("-5"));
                assert!(a.bare_offset_min.is_none());
            }
            _ => panic!("expected Retime"),
        }
    }

    #[test]
    fn retime_offset_conflicts_with_bare_numeric() {
        let err =
            parse_argv(&argv(&["retime", "coffee:abc", "5", "--offset", "PT5M"])).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("--offset") && msg.contains("bare numeric"),
            "got: {msg}"
        );
    }

    #[test]
    fn retime_offset_conflicts_with_time_args() {
        let err =
            parse_argv(&argv(&["retime", "coffee:abc", "--offset", "PT5M", "-r"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--offset"), "got: {msg}");
    }

    #[test]
    fn retime_two_bare_numerics_errors() {
        let err = parse_argv(&argv(&["retime", "coffee:abc", "5", "10"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("at most one bare numeric"), "got: {msg}");
    }

    #[test]
    fn view_list_flag_parses_without_name() {
        let cli = parse_argv(&argv(&["view", "--list"])).unwrap();
        match cli.command {
            Command::View(args) => {
                assert!(args.list);
                assert_eq!(args.name, "");
            }
            other => panic!("expected View, got {other:?}"),
        }
    }

    #[test]
    fn view_list_with_positional_errors() {
        let err = parse_argv(&argv(&["view", "--list", "stray"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--list"), "got: {msg}");
    }

    #[test]
    fn view_without_name_or_list_errors() {
        let err = parse_argv(&argv(&["view"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("requires a name"), "got: {msg}");
    }
}
