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
use crate::input::{self, Show};

mod alias;
mod conflicts;
mod core;
mod debug;
mod flag;
mod get_delete;
mod history;
mod info_db;
mod kit;
pub(crate) mod log;
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
use self::conflicts::*;
use self::core::*;
use self::debug::*;
use self::flag::*;
use self::get_delete::*;
use self::history::*;
use self::info_db::*;
use self::kit::*;
use self::log::*;
use self::sync::*;
// Re-exported so the help-drift guard (`help::tests`) can compare the
// documented log-verb flags against the flags the parser actually accepts.
#[cfg(test)]
pub(crate) use self::log::log_args_registry;
use self::meta::*;
use self::now::*;
use self::retime::*;
use self::search::*;
use self::spec::*;
use self::tail::*;
use self::track_ignore::*;
use self::update::*;
use self::views::*;

/// Entry point: parse an argv (including `argv[0]`) into a [`Cli`].
pub fn parse_argv(argv: &[String]) -> Result<Cli> {
    // Strip argv[0] ("datum" or whatever binary name the caller passed).
    parse_argv_showing(argv, None)
}

/// [`parse_argv`], with a `--show` asked for from outside argv —
/// `DATUM_SHOW`, which the caller reads (da-vx31e). It is the value a
/// command takes when argv names none: a typed `--show` wins, and a command
/// with no rendering for it ignores it rather than failing, so `help` under
/// `DATUM_SHOW=json` is still the help.
pub fn parse_argv_showing(argv: &[String], fallback: Option<Show>) -> Result<Cli> {
    // Strip argv[0] ("datum" or whatever binary name the caller passed).
    let tail: Vec<&str> = argv.iter().skip(1).map(|s| s.as_str()).collect();
    parse_tail(&tail, fallback)
}

/// String-API variant: shell-tokenize, then parse.
///
/// Tokenization is delegated to [`shell_split`](crate::alias::expand::shell_split),
/// the single canonical splitter shared with the alias-expansion pipeline.
pub fn parse_argv_string(s: &str) -> Result<Cli> {
    let argv = shell_split(s);
    let tail: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    parse_tail(&tail, None)
}

fn parse_tail(tail: &[&str], fallback: Option<Show>) -> Result<Cli> {
    // Consume global flags before the subcommand.
    //
    // `--db NAME_OR_PATH` and `--color WHEN` are the pre-subcommand
    // globals. We walk linearly; the first non-flag token (or an unknown
    // flag) is the subcommand name.
    let mut db: Option<String> = None;
    let mut color: Option<bool> = None;
    let mut i = 0;
    let sub_idx = loop {
        let Some(tok) = tail.get(i) else {
            // No subcommand after consuming global flags: drop into the
            // REPL with whatever global context (e.g. `--db NAME`) the
            // caller supplied. `dm` and `dm --db finance` both land here.
            return Ok(Cli {
                db,
                color,
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
            FlagSplit::LongAttached("color", v) => {
                color = parse_color(v)?;
                i += 1;
            }
            FlagSplit::Long("color") => {
                let v = tail
                    .get(i + 1)
                    .ok_or_else(|| ParseError::InvalidArg("--color requires a value".into()))?;
                color = parse_color(v)?;
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
            // `--show` / `--json` in front of the subcommand: stepped over
            // here and read by the shared sweep below, which takes them
            // wherever they appear. Only the missing value is caught here,
            // because the sweep never sees a token this loop skipped past.
            FlagSplit::Long("show") => {
                if tail.get(i + 1).is_none() {
                    return Err(ParseError::InvalidArg("--show requires a value".into()));
                }
                i += 2;
            }
            FlagSplit::LongAttached("show", _) | FlagSplit::Long("json") => i += 1,
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

    // Per-subcommand `--help` / `-h` short-circuit. Walk the tokens once
    // and bail with a HelpRequested if we find either form; this saves
    // every subcommand parser from having to handle them individually,
    // and it runs BEFORE the output sweep so `datum X --help` answers
    // even when the rest of the line is nonsense.
    for tok in &tail[sub_idx + 1..] {
        match *tok {
            // `db purge --help` is purge's page, not db's (da-2bb7z).
            "--help" | "-h" => {
                let page = crate::help::page_name(sub, &tail[sub_idx + 1..]);
                return Err(ParseError::HelpRequested(Some(page)));
            }
            // `datum SUB --man` and (after the field/verb prepass swap)
            // `datum --man SUB` both land here.
            "--man" => return Err(ParseError::ManRequested(Some(sub.to_string()))),
            _ => {}
        }
    }

    // The output family (`--show <V>`, `--json`) is read HERE, for every
    // command, the way `--db` is — not thirteen times by hand in the
    // per-command registries (da-qt6z). Both halves of `tail` are swept,
    // so `datum --show json occur x` and `datum occur x --show json` are
    // the same invocation; the last one wins; and a value the named
    // command has no rendering for is an error rather than a no-op.
    //
    // `spec` is in the family too: its `--json <JSON>` input flag was
    // renamed `--from-json` (vividn, 2026-09-15), so nothing collides.
    //
    // The leading half's survivors are the globals the loop above already
    // consumed, so only its `--show` matters; a later one outranks it.
    let (before, _) = input::take_show(&tail[..sub_idx])?;
    let (after, swept) = input::take_show(&tail[sub_idx + 1..])?;
    let show = after.or(before);
    let rest: &[&str] = &swept;

    let command = match sub {
        "occur" => Command::Occur(parse_log_args(rest)?),
        "start" => Command::Start(parse_log_args(rest)?),
        "stop" | "end" => Command::Stop(parse_log_args(rest)?),
        "switch" => Command::Switch(parse_log_args(rest)?),
        "add" => Command::Add(parse_log_args(rest)?),
        "note" => Command::Note(parse_log_args(rest)?),

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
        "redo" => Command::Redo(parse_redo_args(rest)?),

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
        "conflicts" => Command::Conflicts(parse_conflicts_cmd(rest)?),
        "debug" => Command::Debug(parse_debug_cmd(rest)?),

        "track" => Command::Track(parse_track_args(rest)?),
        "ignore" => Command::Ignore(parse_ignore_args(rest)?),

        "meta" => Command::Meta(parse_meta_cmd(rest)?),

        "flag" => Command::Flag(parse_flag_cmd(rest)?),

        "grep" => Command::Grep(parse_grep_args(rest)?),
        "check" => Command::Check(parse_check_args(rest)?),

        "count" => Command::Count(parse_count_args(rest)?),
        "stats" => Command::Stats(parse_stats_args(rest)?),
        "ids" => Command::Ids(parse_ids_args(rest)?),
        "fields" => Command::Fields(parse_fields_args(rest)?),

        "dayview" => Command::Dayview(parse_dayview_args(rest)?),
        "nowview" => Command::Nowview(parse_nowview_args(rest)?),
        "view" => Command::View(parse_view_args(rest)?),

        "info" => Command::Info(parse_info_args(rest)?),
        // `db purge --keep-sync-chain` is `sync chain purge --data`, spelled
        // where a person looks for a purge (da-2bb7z): one command, two names.
        "db" if rest.first() == Some(&"purge") && rest.contains(&"--keep-sync-chain") => {
            if rest
                .iter()
                .any(|t| matches!(*t, "--leave" | "--rejoin" | "--force"))
            {
                return Err(ParseError::InvalidArg(
                    "db purge: --keep-sync-chain purges every copy in the chain; \
                     --leave and --rejoin purge this copy alone"
                        .into(),
                ));
            }
            let argv: Vec<&str> = ["chain", "purge"]
                .into_iter()
                .chain(rest[1..].iter().map(|t| match *t {
                    "--keep-sync-chain" => "--data",
                    "-y" => "--yes",
                    t => t,
                }))
                .collect();
            Command::Sync(parse_sync_cmd(&argv)?)
        }
        "db" => Command::Db(parse_db_cmd(rest)?),

        "config" => Command::Config(parse_config_cmd(rest)?),

        "sync" => Command::Sync(parse_sync_cmd(rest)?),

        "reparse" => Command::Reparse(parse_reparse_args(rest)?),

        "completions" => Command::Completions(parse_completions_cmd(rest)?),

        "help" => {
            // `datum help --json` → the command table itself (da-r13j).
            if show == Some(Show::Json) {
                return Err(ParseError::GrammarRequested);
            }
            // `datum help <cmd> [<sub>…]` → help request with that page.
            let cmd = rest
                .split_first()
                .map(|(cmd, words)| crate::help::page_name(cmd, words));
            return Err(ParseError::HelpRequested(cmd));
        }

        other => {
            // da-sg3b: this arm is the typo guard, not a gap to fill.
            // Softening it into "log an occurrence on `{other}`" is the
            // one change it must never take: a bare unrecognised token
            // becoming an occurrence made every mistyped field a silent
            // new field. That fallthrough was deliberate very early on
            // and its author repudiated it on 2026-08-10. A friendlier
            // message — "did you mean `datum alias set …`?" — is welcome;
            // doing it automatically is not.
            return Err(ParseError::InvalidArg(format!(
                "unknown subcommand: '{other}'"
            )));
        }
    };

    let mut command = command;
    let typed = show.is_some();
    let show = show.or(fallback.filter(|s| input::shows_for(sub).contains(s)));
    if let Some(show) = show {
        // Only now that the subcommand is known to be real: a typo'd verb
        // deserves the typo guard's message, not a lecture about the
        // renderings it does not have (da-sg3b).
        //
        // A typed `--show` the parsed command has no place for is refused,
        // where it used to be dropped: by the subcommand's own words in a
        // group, whose `json` is the subcommands' that carry it (da-07594,
        // da-8un1i, da-a6p7v). `DATUM_SHOW` they still ignore.
        if !apply_show(&mut command, show) && typed {
            let words = match (sub, rest.first()) {
                ("sync", Some(&("chain" | "admin" | "daemon" | "pipe")))
                | ("db", Some(&"migrate")) => 2,
                ("sync" | "alias" | "config" | "conflicts" | "db" | "spec", _) => 1,
                _ => 0,
            };
            let name = [&[sub], &rest[..words.min(rest.len())]].concat().join(" ");
            input::check_show_in(show, &name, &[Show::Default])?;
        }
        input::check_show(show, sub)?;
    }
    Ok(Cli { db, color, command })
}

/// `--color auto|always|never`, as git, cargo and ripgrep spell it.
fn parse_color(v: &str) -> Result<Option<bool>> {
    match v {
        "auto" => Ok(None),
        "always" => Ok(Some(true)),
        "never" => Ok(Some(false)),
        _ => Err(ParseError::InvalidArg(format!(
            "--color takes auto, always or never, not '{v}'"
        ))),
    }
}

/// Hand the swept `--show` to whichever intent carries the output level.
///
/// Two shapes, because the render layers differ: the commands that draw
/// entry documents or tables resolve through an [`OutputArgs`](input::OutputArgs)
/// (which also carries `-A` and `-o`), while a reader with a JSON
/// document beside its stock output just needs the level. The thirteen
/// readers used to carry a hand-wired `json: bool` each; they carry a
/// `show` now, and `--json` is the value `json` (da-qt6z).
///
/// Returns whether the intent had a place for it.
fn apply_show(command: &mut Command, show: Show) -> bool {
    let named = show.name().to_string();
    match command {
        Command::Occur(a)
        | Command::Start(a)
        | Command::Stop(a)
        | Command::Switch(a)
        | Command::Add(a)
        | Command::Note(a) => a.output.show = Some(named),
        Command::Tail(a) | Command::Head(a) => a.output.show = Some(named),
        Command::Get(a) => a.output.show = Some(named),
        Command::Delete(a) => a.output.show = Some(named),
        Command::Undo(a) => a.output.show = Some(named),
        Command::Redo(a) => a.output.show = Some(named),
        Command::Update(a) | Command::Merge(a) | Command::Rekey(a) => a.output.show = Some(named),
        Command::Edit(a) => a.output.show = Some(named),
        Command::Grep(a) => a.output.show = Some(named),

        Command::Now(a) => a.show = show,
        Command::Nowview(a) => a.show = show,
        Command::Dayview(a) => a.show = show,
        Command::Stats(a) => a.show = show,
        Command::Count(a) => a.show = show,
        Command::Ids(a) => a.show = show,
        Command::Fields(a) => {
            a.show = show;
            // `--show all` is the other spelling of `-A` here (da-1lnf).
            a.include_ignored |= show == Show::All;
        }
        Command::History(a) => a.show = show,
        Command::Check(a) => a.show = show,
        Command::Config(ConfigCommand::Show(a)) => a.show = show,
        // `spec show` prints its document by default: `json` is what it draws.
        Command::Spec(SpecCommand::Show { .. }) => {}
        Command::Alias(AliasCommand::List { show: s }) => *s = show,
        Command::Conflicts(ConflictsCommand::List(a)) => a.show = show,
        Command::Sync(c) => {
            // `sync --as` carries the level its command does.
            let c = match c {
                SyncCommand::As(_, c) => &mut **c,
                c => c,
            };
            match c {
                SyncCommand::Members { show: s, .. }
                | SyncCommand::AdminList { show: s }
                | SyncCommand::Status { show: s, .. }
                | SyncCommand::Apply { show: s }
                | SyncCommand::Diff { show: s, .. }
                | SyncCommand::Refold { show: s }
                | SyncCommand::Admin {
                    act: SyncAdminAct::Revert { show: s, .. },
                    ..
                } => *s = show,
                _ => return false,
            }
        }
        Command::Db(DbCommand::Restore(a)) => a.show = show,
        Command::Db(DbCommand::BackupRotation(a)) => a.show = show,

        // The rest draw `default` alone: there is nowhere to carry a level.
        _ => return false,
    }
    true
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

    /// da-2bb7z: the chain-wide purge's other name is the same parse, not a
    /// second command, and refuses the flags that purge this copy alone.
    #[test]
    fn db_purge_keep_sync_chain_is_sync_chain_purge_data() {
        let cmd = |w: &[&str]| format!("{:?}", parse_argv(&argv(w)).unwrap().command);
        for tail in [&[][..], &["--yes"], &["--admin-key", "k"]] {
            let db = [&["db", "purge", "--keep-sync-chain"][..], tail].concat();
            let chain = [&["sync", "chain", "purge", "--data"][..], tail].concat();
            assert_eq!(cmd(&db), cmd(&chain));
        }
        // `db purge`'s `-y` is `--yes` here too: a spelling, not a flag.
        assert_eq!(
            cmd(&["db", "purge", "--keep-sync-chain", "-y"]),
            cmd(&["sync", "chain", "purge", "--data", "--yes"])
        );
        for local in ["--leave", "--rejoin"] {
            let e = parse_argv(&argv(&["db", "purge", "--keep-sync-chain", local])).unwrap_err();
            assert!(e.to_string().contains("purge this copy alone"), "{e}");
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
    fn db_backup_rotation_flags() {
        // da-d5cmj: --to / --list / --plan are the rotation; a bare path the file.
        let rot = |words: &[&str]| match parse_argv(&argv(words)).map(|c| c.command) {
            Ok(Command::Db(DbCommand::BackupRotation(a))) => a,
            other => panic!("expected a rotation for {words:?}, got {other:?}"),
        };
        let a = rot(&[
            "db",
            "backup",
            "--to",
            "bk",
            "--every=1h",
            "--slots",
            "8",
            "--json",
        ]);
        assert_eq!(a.action, RotationAction::To("bk".into()));
        assert_eq!((a.every, a.slots, a.show), (3600, Some(8), Show::Json));
        assert_eq!(
            rot(&["db", "backup", "--list", "bk"]).action,
            RotationAction::List("bk".into())
        );
        let a = rot(&["db", "backup", "--plan", "--every", "30m", "--span", "2d"]);
        assert_eq!(
            (a.action, a.every, a.slots),
            (RotationAction::Plan { span: 172_800 }, 1800, None)
        );
        assert!(matches!(
            parse_argv(&argv(&["db", "backup", "out.jsonl.gz"]))
                .unwrap()
                .command,
            Command::Db(DbCommand::Backup(_))
        ));
        for bad in [
            &["db", "backup", "--to", "bk"][..],
            &[
                "db", "backup", "--to", "bk", "--every", "1h", "--slots", "0",
            ],
            &[
                "db", "backup", "--to", "bk", "--list", "bk", "--every", "1h",
            ],
            &["db", "backup", "--plan", "--every", "1h"],
            &["db", "backup", "out.jsonl", "--every", "1h"],
            &["db", "backup", "out.jsonl", "--to", "bk", "--every", "1h"],
            &["db", "backup", "--list", "bk", "--slots", "8"],
            &["db", "backup", "--list", "bk", "--every", "1h"],
            &["db", "backup", "--list", "bk", "--span", "2d"],
            &[
                "db", "backup", "--to", "bk", "--every", "1h", "--span", "2d",
            ],
        ] {
            assert!(parse_argv(&argv(bad)).is_err(), "{bad:?} should be refused");
        }
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
        let cli = parse_argv(&argv(&["del", "some:id"])).unwrap();
        match cli.command {
            Command::Delete(a) => {
                assert_eq!(a.id_or_slug.as_deref(), Some("some:id"));
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
                assert!(!a.relative);
                assert!(a.time.time.is_none() && a.time.yesterday == 0 && a.time.quick == 0);
            }
            _ => panic!("expected Retime"),
        }
    }

    /// da-jix52: the positional after the slug is a `-t` value in every
    /// form `-t` takes, and lands in `-t`'s own slot.
    #[test]
    fn retime_positional_after_the_slug_is_a_time() {
        for when in ["1040", "-5", "+5", "30", "-1h", "yesterday", "9:15am"] {
            match parse_argv(&argv(&["retime", "coffee:2024", when]))
                .unwrap()
                .command
            {
                Command::Retime(a) => {
                    assert_eq!(a.slug.as_deref(), Some("coffee:2024"), "{when}");
                    assert_eq!(a.time.time.as_deref(), Some(when));
                    let flag = parse_argv(&argv(&["retime", "coffee:2024", "-t", when])).unwrap();
                    let Command::Retime(flag) = flag.command else {
                        panic!("expected Retime")
                    };
                    assert_eq!(a.time, flag.time, "{when}");
                }
                _ => panic!("expected Retime"),
            }
        }
    }

    #[test]
    fn retime_unsigned_number_before_a_slug_is_the_slug() {
        // A short hid is the hid's first five characters, and about one in
        // three hundred is all digits (da-q1k4p): `retime 15437 -t 0930`
        // names entry 15437; it must not read 15437 as a time for `_LAST`.
        for (args, when) in [
            (&["retime", "15437", "-t", "0930"][..], Some("0930")),
            (&["retime", "15437"], None),
            (&["retime", "15437", "30"], Some("30")),
        ] {
            match parse_argv(&argv(args)).unwrap().command {
                Command::Retime(a) => {
                    assert_eq!(a.slug.as_deref(), Some("15437"), "{args:?}");
                    assert_eq!(a.time.time.as_deref(), when, "{args:?}");
                }
                _ => panic!("expected Retime"),
            }
        }
    }

    /// Alone, a signed positional is the time and the slug is `_LAST`.
    #[test]
    fn retime_signed_positional_alone_is_a_time() {
        for when in ["+30", "-5", "-1h"] {
            match parse_argv(&argv(&["retime", when])).unwrap().command {
                Command::Retime(a) => {
                    assert!(a.slug.is_none(), "{when}");
                    assert_eq!(a.time.time.as_deref(), Some(when));
                }
                _ => panic!("expected Retime"),
            }
        }
    }

    #[test]
    fn retime_r_flag_sets_relative_bit() {
        let cli = parse_argv(&argv(&["retime", "coffee:abc", "-r", "-t", "-5"])).unwrap();
        match cli.command {
            Command::Retime(a) => {
                assert!(a.relative);
                assert_eq!(a.time.time.as_deref(), Some("-5"));
            }
            _ => panic!("expected Retime"),
        }
    }

    #[test]
    fn retime_offset_conflicts_with_a_positional_time() {
        let err =
            parse_argv(&argv(&["retime", "coffee:abc", "5", "--offset", "PT5M"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--offset is mutually exclusive"), "got: {msg}");
    }

    #[test]
    fn retime_offset_conflicts_with_time_args() {
        let err =
            parse_argv(&argv(&["retime", "coffee:abc", "--offset", "PT5M", "-r"])).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--offset"), "got: {msg}");
    }

    /// da-1d0v2: `-T` takes the time away, so a flag placing it is refused.
    #[test]
    fn retime_no_timestamp_conflicts_with_placing_flags() {
        for extra in [
            &["-t", "0930"][..],
            &["-r"],
            &["-y"],
            &["-q"],
            &["-d", "-1"],
            &["-D"],
            &["-z", "+4"],
        ] {
            let mut args = vec!["retime", "coffee:abc", "-T"];
            args.extend_from_slice(extra);
            let msg = format!("{}", parse_argv(&argv(&args)).unwrap_err());
            assert!(
                msg.contains("-T/--no-timestamp removes the time"),
                "{extra:?} got: {msg}"
            );
        }
        assert!(parse_argv(&argv(&["retime", "coffee:abc", "-T"])).is_ok());
    }

    /// da-k5oa5: a positional time beside the time flags is not dropped: it
    /// is one more `-t` value, stacking as two `-t`s do. `-T` still refuses.
    #[test]
    fn retime_positional_time_composes_with_time_flags() {
        let cli = parse_argv(&argv(&["retime", "coffee:abc", "-5", "-t", "0930", "-y"])).unwrap();
        match cli.command {
            Command::Retime(a) => {
                assert_eq!(a.time.time.as_deref(), Some("0930"));
                assert_eq!(a.time.time_offsets, ["-5"]);
                assert_eq!(a.time.yesterday, 1);
            }
            _ => panic!("expected Retime"),
        }
        let msg = parse_argv(&argv(&["retime", "coffee:abc", "-5", "-T"]))
            .unwrap_err()
            .to_string();
        assert!(msg.contains("-T/--no-timestamp removes the time"), "{msg}");
    }

    #[test]
    fn retime_two_times_after_the_slug_errors() {
        let err = parse_argv(&argv(&["retime", "coffee:abc", "5", "10"])).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("unexpected positional after the time: '10'"),
            "got: {msg}"
        );
    }

    /// `conflicts` draws JSON for its listing alone (da-07594): typed on
    /// `resolve` it is refused, and from `DATUM_SHOW` it is ignored.
    #[test]
    fn conflicts_resolve_refuses_a_typed_json_and_ignores_the_fallback() {
        let list = parse_argv(&argv(&["conflicts", "--json"])).unwrap();
        assert!(matches!(
            list.command,
            Command::Conflicts(ConflictsCommand::List(a)) if a.show == Show::Json
        ));
        let resolve = ["conflicts", "resolve", "abc", "--use-a"];
        let typed = [&resolve[..], &["--json"]].concat();
        assert_eq!(
            parse_argv(&argv(&typed)).unwrap_err().to_string(),
            "Invalid argument: conflicts resolve: --show json is not a rendering \
             conflicts resolve has (it draws: default)"
        );
        let asked = parse_argv_showing(&argv(&resolve), Some(Show::Json)).unwrap();
        assert!(matches!(
            asked.command,
            Command::Conflicts(ConflictsCommand::Resolve(a)) if a.strategy.is_some()
        ));
    }

    /// da-nhnip: the error names the flag typed and what THAT flag accepts.
    #[test]
    fn strategy_and_conflict_errors_name_their_own_flag_and_values() {
        for (words, want) in [
            (
                &["update", "doc:z", "--strategy", "ignore", "foo=bar"][..],
                "invalid --strategy value: 'ignore' (expected update|merge|rekey|useOld|useNew|\
                 preferOld|preferNew|intersection|removeConflicting|xor|append|prepend|\
                 mergeSort|appendSort)",
            ),
            (
                &["add", "doc", "--conflict", "xor", "foo=bar"][..],
                "invalid --conflict value: 'xor' (expected update|merge|rekey|ignore)",
            ),
        ] {
            let msg = format!("{}", parse_argv(&argv(words)).unwrap_err());
            assert!(msg.ends_with(want), "{words:?} got: {msg}");
        }
        assert!(parse_argv(&argv(&["add", "doc", "--conflict", "ignore"])).is_ok());
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

    /// da-8un1i: `sync` is a group, and a typed `--json` is refused by each
    /// subcommand that draws no document, by its own name, where it used to
    /// be dropped. `DATUM_SHOW=json` those still ignore.
    #[test]
    fn a_typed_json_is_refused_by_a_sync_subcommand_that_draws_none() {
        let words = |name: &str, args: &str| -> Vec<String> {
            let line = format!("{name} {args}");
            argv(&line.split_whitespace().collect::<Vec<_>>())
        };
        let debug = |cli: Result<Cli>| format!("{:?}", cli.unwrap().command);

        for (name, args) in [
            ("sync status", ""),
            ("sync chain list", ""),
            ("sync chain revert", "bob"),
            ("sync admin list", ""),
            ("sync pipe apply", ""),
            ("sync diff", "bob"),
            ("sync refold", ""),
        ] {
            let typed = debug(parse_argv(&words(name, &format!("{args} --json"))));
            assert!(typed.contains("show: Json"), "{name}: {typed}");
            let env = debug(parse_argv_showing(&words(name, args), Some(Show::Json)));
            assert_eq!(env, typed, "{name}");
        }

        let dropped = [
            ("sync", ""),
            ("sync pause", ""),
            ("sync resume", ""),
            ("sync chain invite", ""),
            ("sync chain connect", "ticket"),
            ("sync chain leave", ""),
            ("sync chain rename", "bob Bobby"),
            ("sync chain revoke", "bob"),
            ("sync chain role", "bob reader"),
            ("sync chain ack", "bob"),
            ("sync chain rekey", ""),
            ("sync admin add", "key"),
            ("sync admin remove", "bob"),
            ("sync daemon install", ""),
            ("sync daemon uninstall", ""),
            ("sync daemon status", ""),
            ("sync daemon run", ""),
            ("sync pipe vector", ""),
            ("sync pipe ops", ""),
            ("sync pipe verify", ""),
            ("sync pipe digest", ""),
            ("sync pipe held", ""),
            ("sync run", ""),
            ("sync endpoint-key", ""),
            ("sync holds", ""),
            ("sync record-session", "ab"),
            ("sync handshake", "ab --label l --addr a"),
            ("sync new-invite", ""),
            ("sync approve", "link"),
            ("sync qr", "text"),
        ];
        for (name, args) in dropped {
            let plain = debug(parse_argv(&words(name, args)));
            for flag in ["--json", "--show json", "--show=json"] {
                let err = parse_argv(&words(name, &format!("{args} {flag}"))).unwrap_err();
                assert_eq!(
                    err.to_string(),
                    format!(
                        "Invalid argument: {name}: --show json is not a rendering {name} has (it draws: default)"
                    ),
                );
            }
            // Typed anywhere on the line, the group's word included.
            let mut early = words(name, args);
            early.insert(1, "--json".into());
            assert!(parse_argv(&early).is_err(), "{name}");
            // The rendering it does have is not refused, and the env is ignored.
            let default = parse_argv(&words(name, &format!("{args} --show default")));
            assert_eq!(debug(default), plain, "{name}");
            let env = parse_argv_showing(&words(name, args), Some(Show::Json));
            assert_eq!(debug(env), plain, "{name}");
        }
    }

    /// da-a6p7v: so is it by every command with no place for it. `alias`,
    /// `db` and `spec` are groups as `sync` is, and `redo` carries the
    /// level `undo` does, where it had no arm and dropped it.
    #[test]
    fn a_typed_show_is_refused_by_any_command_with_no_place_for_it() {
        let parse = |line: &str| parse_argv(&argv(&line.split_whitespace().collect::<Vec<_>>()));
        let env = |line: &str| {
            let words = argv(&line.split_whitespace().collect::<Vec<_>>());
            format!(
                "{:?}",
                parse_argv_showing(&words, Some(Show::Json))
                    .unwrap()
                    .command
            )
        };
        let debug = |cli: Result<Cli>| format!("{:?}", cli.unwrap().command);

        for carried in [
            "alias list",
            "config show",
            "db restore f",
            "db backup --to d --every 1h",
            "db backup --list d",
            "db backup --plan --every 1h --span 1d",
            "undo",
            "redo",
        ] {
            let typed = debug(parse(&format!("{carried} --json")));
            assert_ne!(typed, debug(parse(carried)), "{carried}");
            assert_eq!(env(carried), typed, "{carried}");
        }
        // `spec show` draws JSON with or without it.
        assert!(parse("spec show x --json").is_ok());

        for (name, args) in [
            ("alias set", "zz occur"),
            ("alias show", "zz"),
            ("alias remove", "zz"),
            ("db setup", ""),
            ("db purge", ""),
            ("db backup", "f"),
            ("db migrate run", ""),
            ("db migrate status", ""),
            ("db migrate edit", ""),
            ("db v2import", "f"),
            ("spec list", ""),
            ("spec set", "x"),
            ("spec add-state", "x on"),
            ("spec remove", "x"),
        ] {
            let plain = debug(parse(&format!("{name} {args}")));
            for show in ["json", "none"] {
                let err = parse(&format!("{name} {args} --show {show}")).unwrap_err();
                assert_eq!(
                    err.to_string(),
                    format!(
                        "Invalid argument: {name}: --show {show} is not a rendering {name} has (it draws: default)"
                    ),
                );
            }
            assert!(parse(&format!("{name} --json {args}")).is_err(), "{name}");
            // The rendering it does have is not refused, and the env is ignored.
            let default = parse(&format!("{name} {args} --show default"));
            assert_eq!(debug(default), plain, "{name}");
            assert_eq!(env(&format!("{name} {args}")), plain, "{name}");
        }
    }
}
