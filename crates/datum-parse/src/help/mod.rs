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
//! [`top_level_help`] / [`command_help`] / [`version_banner`] remain as
//! thin shims for the existing CLI entry point.

pub mod commands;
pub mod render;
pub mod spec;

pub use commands::{find_command, COMMANDS, GLOBAL_FLAGS};
pub use render::{render_help_text, render_markdown, render_roff};
pub use spec::{ArgSpec, Category, CommandSpec, ExampleSpec, FlagSpec};

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

/// Version banner (`datum --version`).
pub fn version_banner() -> String {
    format!("datum {DATUM_VERSION}")
}

#[cfg(test)]
mod tests;
