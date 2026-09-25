//! `datum alias` — CLI alias management (set, list, delete).
//!
//! Aliases are stored in the database and resolved in the CLI pre-pass
//! before the parser sees argv (see `crate::args::expand_argv`).
//!
//! What each subcommand *decides* — which row to write, which sentence to
//! print — is `datum_core::field_spec::alias_effect`, which takes the rows
//! rather than the database. This function is the database half: load the
//! rows, ask, perform.
//!
//! An alias is an entry (da-b3752): a write is stamped with this device and
//! leaves `_LAST` on it, so a bare `undo` steps it back like any doc.

use anyhow::{bail, Result};
use datum_core::{
    db::Db,
    field_spec::{alias_effect, user_alias_to_save, AliasEffect},
};
use datum_parse::alias::shell_join;

use crate::args::AliasCommand;

pub fn alias(sub: AliasCommand, db: &Db) -> Result<()> {
    let user = db.list_user_aliases()?;
    let effect = {
        let derived = db.spec_alias_index()?;
        alias_effect(&sub, &user, &derived)
    };
    match effect {
        AliasEffect::Say(text) => println!("{text}"),
        AliasEffect::Save {
            alias,
            text,
            warning,
        } => {
            remember(db, save(db, &alias)?);
            println!("{text}");
            if let Some(w) = warning {
                eprintln!("{w}");
            }
        }
        AliasEffect::Delete { name, text } => {
            if let Some(hid) = db.delete_alias(&name)? {
                remember(db, hid);
            }
            println!("{text}");
        }
        AliasEffect::Refuse(message) => bail!(message),
        AliasEffect::Nothing => {}
    }
    Ok(())
}

/// Write `alias` as this device's entry.
fn save(db: &Db, alias: &datum_core::field_spec::Alias) -> Result<datum_core::ids::Hid> {
    db.set_device_id(&crate::config::device_id()?);
    Ok(db.save_alias(alias)?)
}

/// Point `_LAST` at the alias entry a command just wrote.
fn remember(db: &Db, hid: datum_core::ids::Hid) {
    let _ = datum_core::last_docs::set_last_docs(db, &[hid]);
}

/// Shared save path for user aliases — the single set of guards both
/// creation paths (`alias set` and the inline `--alias` flag) go through:
///
/// * rejects empty names;
/// * when `overwrite` is false, an existing alias is an error explaining
///   how to replace it (the inline path passes its `--alias-overwrite`
///   flag; `alias set` passes `true` — the direct command overwrites
///   silently by design);
/// * saves via [`Db::save_alias`];
/// * warns on stderr when the name shadows a built-in subcommand — the
///   alias is still saved (per da-0b4) but won't fire when a verb is
///   expected.
fn save_user_alias(name: &str, expands_to: String, overwrite: bool, db: &Db) -> Result<()> {
    if !overwrite {
        if let Some(existing) = db.lookup_alias(name)? {
            bail!(
                "alias '{name}' already exists (currently → datum {prev}). \
                 Re-run with --alias-overwrite to replace it.",
                prev = existing.expands_to,
            );
        }
    }

    let (alias, warning) = user_alias_to_save(name, expands_to).map_err(|e| anyhow::anyhow!(e))?;
    save(db, &alias)?;
    if let Some(w) = warning {
        eprintln!("{w}");
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
/// * Saves through [`save_user_alias`], which carries the shared
///   guards: empty-name rejection, the overwrite policy, and the
///   built-in-shadowing warning. The next invocation's pre-pass picks
///   the alias up automatically through `DbAliasResolver`.
pub fn record_inline_alias(
    name: Option<String>,
    overwrite: bool,
    raw_argv: &[String],
    db: &Db,
) -> Result<()> {
    let Some(name) = name else {
        return Ok(());
    };

    let canonical = canonical_form(raw_argv);
    if canonical.is_empty() {
        bail!("--alias: nothing to record (canonical command is empty)");
    }

    save_user_alias(&name, canonical.clone(), overwrite, db)?;

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
        let argv = v(&["start", "sleep", "--alias", "bedtime", "--alias-overwrite"]);
        assert_eq!(canonical_form(&argv), "start sleep");
    }

    #[test]
    fn canonical_quotes_tokens_with_whitespace() {
        let argv = v(&["note", "word", "to run", "--alias=vbp"]);
        // The third token has whitespace, so it needs quoting.
        assert_eq!(canonical_form(&argv), "note word \"to run\"");
    }
}
