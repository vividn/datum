//! `datum rename OLD NEW` (da-ftls7): a field's entries, spec and derived
//! aliases under another name, in one write ([`Db::rename_field`]).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::RenameArgs;

pub fn rename(args: RenameArgs, db: &Db) -> Result<()> {
    let RenameArgs {
        old,
        new,
        dry_run,
        merge,
    } = args;
    crate::commands::helpers::refuse_hid_named_field(db, &new)?;
    let done = db.rename_field(&old, &new, dry_run, merge)?;
    crate::cmd::stamp_batch(db);
    let (tag, moved) = if dry_run {
        ("[dry-run] would rename", "would move")
    } else {
        ("RENAME", "moved")
    };
    let entries = match done.entries {
        1 => "1 entry".to_string(),
        n => format!("{n} entries"),
    };
    println!("{tag} {old} → {new}: {entries}");
    if done.became_one > 0 {
        let n = done.became_one;
        let verb = if dry_run { "would become" } else { "became" };
        println!("  {n} identical to an entry of {new}: {verb} one with it");
    }
    if let Some(differs) = &done.spec_went {
        let verb = if dry_run { "would go" } else { "went" };
        println!("  spec:{new} stays and spec:{old} {verb}");
        for (key, gone, kept) in differs {
            println!("    {key}: {old} said {gone}, {new} says {kept}");
        }
    } else if done.spec {
        println!("  spec:{old} {moved} to spec:{new}");
        if !done.derived.is_empty() {
            println!("  aliases now: {}", done.derived.join(", "));
        }
    }
    for alias in &done.naming_old {
        println!(
            "  alias {} still names {old}: «{}»",
            alias.name, alias.expands_to
        );
    }
    for c in &done.clashes {
        let keys = c.keys.join(", ");
        let now = if dry_run {
            String::new()
        } else {
            format!(", now {}", c.now)
        };
        println!(
            "  conflict at {}: {}{now} and {} differ on {keys}",
            c.slug, c.hid, c.with
        );
    }
    if !done.clashes.is_empty() && !dry_run {
        println!("  see them: datum conflicts list");
    }
    Ok(())
}
