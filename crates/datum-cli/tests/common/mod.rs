//! Shared CLI-level test harness.
//!
//! [`run_cli`] bypasses the process boundary but still routes args through the
//! full arg-parse → command-dispatch path, so behaviors like alias expansion,
//! field/verb swap, and stacked-flag expansion are exercised end-to-end.

use clap::Parser;
use datum_cli::args::{expand_argv_with_db, Cli};
use datum_cli::cmd;
use datum_core::db::Db;

/// Dispatch `args` as if the user typed `datum <args...>`.
///
/// The `db` is used both for the CLI pre-pass (alias resolution) and as the
/// target for command dispatch — matching the binary's behavior, minus
/// `config::db_path()` and process boundaries.
pub fn run_cli(args: &[&str], db: &Db) -> anyhow::Result<()> {
    let argv: Vec<String> = std::iter::once("datum".to_string())
        .chain(args.iter().map(|s| s.to_string()))
        .collect();
    let expanded = expand_argv_with_db(argv, db)?;
    let cli = Cli::try_parse_from(expanded)?;
    cmd::dispatch(cli, db)
}
