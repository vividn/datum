//! `help` — structured command-tree introspection + renderers.
//!
//! Historically `--help` was a pile of hand-authored `String::push_str`
//! calls. That was fine for one output format but didn't generalise to
//! Markdown READMEs or man pages — hand-writing those alongside would
//! relocate the drift problem.
//!
//! The new shape:
//! 1. [`spec`] defines the data model — [`CommandSpec`], [`FlagSpec`],
//!    [`ArgSpec`], [`ExampleSpec`], [`Category`].
//! 2. [`commands`] holds the static [`commands::COMMANDS`] table —
//!    every subcommand of `datum`, lifted from the previous
//!    hand-written help text.
//! 3. [`render`] consumes that table to emit `--help` text, Markdown,
//!    or roff.
//!
//! [`top_level_help`] / [`command_help`] remain as
//! thin shims for the existing CLI entry point.

pub mod commands;
pub mod render;
pub mod spec;

pub use commands::{find_command, find_topic, page_name, COMMANDS, ENV_VARS, GLOBAL_FLAGS, TOPICS};
pub use render::{render_help_text, render_json, render_markdown, render_page_json, render_roff};
pub use spec::{
    ArgSpec, Category, CommandSpec, EnvVarSpec, ExampleSpec, FlagSpec, FormGroup, FormSpec,
    SharedFlags, TopicSpec,
};

pub const DATUM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Render the top-level `datum --help` string.
pub fn top_level_help() -> String {
    render_help_text(None)
}

/// Render the per-command help. Unknown commands fall back to the
/// top-level (matching the legacy behaviour).
pub fn command_help(cmd: &str) -> String {
    render_help_text(Some(cmd))
}

/// The text a `--help` / `--version` / `--man` request prints, or `None`
/// for a [`ParseError`](crate::ParseError) that is a real error.
///
/// Those three variants are errors only in the plumbing sense: their doc
/// comments say the caller renders the text and exits 0. Deciding that on
/// the *variant* — never on its `Display` string, which differs between
/// `help requested` and `help requested for 'occur'` and moves whenever
/// anyone rewords an `#[error(...)]` — is what this function is for, and
/// holding the set in one place is what keeps a second caller from
/// honouring a smaller contract than the CLI does. `datum-cli` prints the
/// text to stdout (da-e0m6). `version` is the `--version` line, which is
/// the binary's own — its build.rs reads git — and not this crate's
/// (da-5172a).
pub fn special_output(e: &crate::ParseError, version: &str) -> Option<String> {
    use crate::ParseError as E;
    match e {
        E::HelpRequested(cmd) => Some(match cmd.as_deref() {
            Some(c) => command_help(c),
            None => top_level_help(),
        }),
        // The newline `println!` used to add, now part of the text so
        // every caller writes the same bytes.
        E::VersionRequested => Some(format!("{version}\n")),
        E::ManRequested(cmd) => Some(render_roff(cmd.as_deref())),
        E::GrammarRequested(None) => Some(render_json()),
        E::GrammarRequested(Some(page)) => Some(render_page_json(&command_help(page))),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
