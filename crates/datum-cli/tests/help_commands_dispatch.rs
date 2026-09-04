//! Every command `datum --help` names must actually dispatch (da-ttsm).
//!
//! `conflicts` was documented for a month — with its own `--help` page,
//! its three subcommands spelled out — while `datum conflicts` answered
//! `unknown subcommand: 'conflicts'`. Nothing failed, because the help
//! registry and the argv parser are authored separately and nothing bound
//! them together. This test is that binding.
//!
//! It walks `datum_parse::help::COMMANDS` (the same table that renders
//! `--help`) and pushes each name, and each alias, through
//! `parse_invocation` — the entry point `datum_cli::run` calls, one layer
//! above `parse_argv`. A command that parses but is unreachable from the
//! binary's own entry path (da-wdo9's `repl`) fails here just as a
//! phantom does.
//!
//! Any *other* error is fine and expected: `datum spec` alone is
//! "missing spec subcommand", `datum get` is "missing required
//! positional". Those prove the command exists. Only "unknown
//! subcommand" — the parser's typo guard — means the help registry is
//! describing something the CLI cannot do.

use datum_commands::args::parse_invocation_with_db;
use datum_core::db::Db;
use datum_parse::help::COMMANDS;

/// Drive `datum <name>` through the binary's parse path and return the
/// error message, if any. An in-memory db keeps the alias resolver
/// hermetic — no `DATUM_HOME`, no env lock, no user data.
fn parse_error(name: &str) -> Option<String> {
    let db = Db::open_in_memory().expect("in-memory db");
    let argv = vec!["datum".to_string(), name.to_string()];
    parse_invocation_with_db(argv, &db)
        .err()
        .map(|e| e.to_string())
}

#[test]
fn every_documented_command_dispatches() {
    let mut phantoms: Vec<String> = Vec::new();
    for spec in COMMANDS {
        for name in std::iter::once(&spec.name).chain(spec.aliases.iter()) {
            if let Some(msg) = parse_error(name) {
                if msg.contains("unknown subcommand") {
                    phantoms.push(format!("`datum {name}` -> {msg}"));
                }
            }
        }
    }
    assert!(
        phantoms.is_empty(),
        "`datum --help` documents command(s) the CLI cannot run:\n  {}",
        phantoms.join("\n  ")
    );
}

/// Control: the sweep above is only meaningful if "unknown subcommand"
/// is what an undocumented name actually produces. If the parser's typo
/// guard ever changes wording, this fails and the sweep gets fixed with
/// it instead of silently passing on every input.
#[test]
fn an_undocumented_name_still_says_unknown_subcommand() {
    let msg = parse_error("zzznotacommand").expect("nonsense name must not parse");
    assert!(
        msg.contains("unknown subcommand"),
        "typo guard reworded; the sweep in this file no longer detects phantoms: {msg}"
    );
}

/// The command this bead was filed for. Kept as its own case so a
/// regression names `conflicts` directly rather than only appearing in
/// the sweep's list.
#[test]
fn conflicts_is_reachable() {
    if let Some(msg) = parse_error("conflicts") {
        assert!(
            !msg.contains("unknown subcommand"),
            "datum conflicts is documented but unreachable: {msg}"
        );
    }
}
