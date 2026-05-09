//! `datum alias` — CLI alias management (set, list, delete).
//!
//! Aliases are stored in the database and resolved in the CLI pre-pass
//! before the parser sees argv (see `crate::args::expand_argv`).

use anyhow::{bail, Result};
use datum_core::{db::Db, field_spec::Alias};
use datum_parse::alias::{shell_join, KNOWN_SUBCOMMANDS};

use crate::args::AliasCommand;

pub fn alias(sub: AliasCommand, db: &Db) -> Result<()> {
    match sub {
        AliasCommand::Show { name } => {
            let reg = db.load_registry()?;
            match reg.aliases.get(&name) {
                Some(alias) => println!("{name} → datum {}", alias.expands_to),
                None => println!("No alias '{name}'"),
            }
        }
        AliasCommand::Set { name, expansion } => {
            let alias = Alias { name: name.clone(), expands_to: expansion, description: None };
            db.save_alias(&alias)?;
            println!("✓ Alias '{name}' saved.");
        }
        AliasCommand::List => {
            let reg = db.load_registry()?;
            if reg.aliases.is_empty() {
                println!("No aliases defined.");
            } else {
                for (name, alias) in &reg.aliases {
                    println!("  {name} → datum {}", alias.expands_to);
                }
            }
        }
        AliasCommand::Remove { name } => {
            db.delete_alias(&name)?;
            println!("✓ Alias '{name}' removed (if it was user-created)");
        }
    }
    Ok(())
}

/// If `name` is `Some`, record the current invocation's canonical
/// argv form (with the alias-naming flags stripped) as a user alias.
///
/// Called by `dispatch` after a successful logging command. The caller
/// captures the pre-expansion argv (via
/// [`crate::args::peek_intent_context`]) *before* the command runs,
/// because `log_entry` consumes the thread-local intent context.
///
/// * Strips `--alias <NAME>`, `--alias=<NAME>`, and
///   `--alias-overwrite` tokens from the captured argv.
/// * If a user alias of the same name already exists and `overwrite`
///   is false, returns an error explaining how to overwrite.
/// * Saves the alias via `Db::save_alias`. The next invocation's
///   pre-pass picks it up automatically through `DbAliasResolver`.
/// * If the name collides with a built-in subcommand, the alias is
///   still saved (per da-0b4) but a stderr warning explains that
///   the alias won't fire when a verb is expected.
pub fn record_inline_alias(
    name: Option<String>,
    overwrite: bool,
    raw_argv: &[String],
    db: &Db,
) -> Result<()> {
    let Some(name) = name else {
        return Ok(());
    };

    if name.is_empty() {
        bail!("--alias requires a non-empty name");
    }

    let canonical = canonical_form(raw_argv);
    if canonical.is_empty() {
        bail!("--alias: nothing to record (canonical command is empty)");
    }

    if let Some(existing) = db.lookup_alias(&name)? {
        if !overwrite {
            bail!(
                "alias '{name}' already exists (currently → datum {prev}). \
                 Re-run with --alias-overwrite to replace it.",
                name = name,
                prev = existing.expands_to,
            );
        }
    }

    let alias = Alias {
        name: name.clone(),
        expands_to: canonical.clone(),
        description: None,
    };
    db.save_alias(&alias)?;

    if KNOWN_SUBCOMMANDS.contains(&name.as_str()) {
        eprintln!(
            "warning: alias '{name}' shadows a built-in subcommand and will not fire when a verb is expected"
        );
    }

    eprintln!("✓ Alias '{name}' → datum {canonical}");
    Ok(())
}

/// Build the canonical command string from the pre-expansion argv tail
/// (i.e. `argv[1..]`), stripping the alias-naming flags.
///
/// Recognised forms:
///
/// * `--alias <NAME>` (consumes the next token as the name)
/// * `--alias=<NAME>` (single token)
/// * `--alias-overwrite` (switch — no value)
///
/// Tokens are re-joined via [`shell_join`] so values that contain
/// whitespace or quotes round-trip back through `shell_split`.
fn canonical_form(raw_argv: &[String]) -> String {
    let mut keep: Vec<String> = Vec::with_capacity(raw_argv.len());
    let mut i = 0;
    while i < raw_argv.len() {
        let tok = raw_argv[i].as_str();
        if tok == "--alias" {
            // Drop the flag and its detached value (if any).
            i += if i + 1 < raw_argv.len() { 2 } else { 1 };
            continue;
        }
        if let Some(rest) = tok.strip_prefix("--alias=") {
            // Attached form — drop the whole token. (`rest` is the name;
            // we deliberately discard it since the goal is to strip.)
            let _ = rest;
            i += 1;
            continue;
        }
        if tok == "--alias-overwrite" {
            i += 1;
            continue;
        }
        keep.push(raw_argv[i].clone());
        i += 1;
    }
    shell_join(&keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn canonical_strips_detached_alias() {
        let argv = v(&["start", "sleep", "-K", "stage", "--alias", "bedtime"]);
        assert_eq!(canonical_form(&argv), "start sleep -K stage");
    }

    #[test]
    fn canonical_strips_attached_alias() {
        let argv = v(&["occur", "coffee", "--alias=cof"]);
        assert_eq!(canonical_form(&argv), "occur coffee");
    }

    #[test]
    fn canonical_strips_overwrite_switch() {
        let argv = v(&[
            "start", "sleep", "--alias", "bedtime", "--alias-overwrite",
        ]);
        assert_eq!(canonical_form(&argv), "start sleep");
    }

    #[test]
    fn canonical_quotes_tokens_with_whitespace() {
        let argv = v(&["note", "word", "to run", "--alias=vbp"]);
        // The third token has whitespace, so it needs quoting.
        assert_eq!(canonical_form(&argv), "note word \"to run\"");
    }
}
