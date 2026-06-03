//! Interactive REPL — invoked when `datum` is run with no subcommand.
//!
//! Each line is shell-split and dispatched through the same alias →
//! parser → command path as a fresh `datum` invocation, but against a
//! single long-lived `Db` handle. Errors don't kill the loop.

use anyhow::Result;
use datum_commands::{cmd, config};
use datum_core::db::Db;
use datum_parse::{help, shell_split, ParseError};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

const PROMPT: &str = "datum> ";

/// Run the REPL against `db_hint` (the global `--db` value from argv, if
/// any). `None` resolves the default DB from config, matching plain
/// `datum` with no flags.
pub fn run(db_hint: Option<&str>) -> Result<()> {
    let resolved = config::resolve_db(db_hint)?;
    if resolved.created_new {
        if let Some(name) = resolved.name.as_deref() {
            eprintln!(
                "note: created new database '{name}' at {}",
                resolved.path.display()
            );
        }
    }
    let db = Db::open(&resolved.path.to_string_lossy())?;
    if resolved.name.is_some() {
        let _ = config::ensure_registered(&resolved);
    }

    let history_path = config::config_dir().ok().map(|d| d.join("repl_history"));

    let mut rl = DefaultEditor::new()?;
    if let Some(p) = &history_path {
        let _ = rl.load_history(p);
    }

    println!("{}", help::version_banner());
    println!("Type `help` for commands, `exit` or Ctrl-D to quit.");

    loop {
        match rl.readline(PROMPT) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if matches!(trimmed, "exit" | "quit") {
                    break;
                }
                let _ = rl.add_history_entry(trimmed);
                if let Err(e) = run_line(trimmed, &db) {
                    eprintln!("error: {e:#}");
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        }
    }

    if let Some(p) = &history_path {
        let _ = rl.save_history(p);
    }
    Ok(())
}

fn run_line(line: &str, db: &Db) -> Result<()> {
    let mut argv = vec!["datum".to_string()];
    argv.extend(shell_split(line));

    match datum_commands::args::parse_invocation_with_db(argv, db) {
        Ok(invocation) => cmd::dispatch_invocation(invocation, db),
        Err(e) => {
            // The REPL deliberately omits the kit-level-2 fallback that
            // `lib.rs::run` adds for unknown subcommands — an unknown
            // command at the prompt just prints the error and the user
            // retries.
            if let Some(pe) = e.downcast_ref::<ParseError>() {
                if let Some(result) = crate::render_parse_special(pe) {
                    return result;
                }
            }
            Err(e)
        }
    }
}
