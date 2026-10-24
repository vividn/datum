//! `datum rename OLD NEW` (da-ftls7): a field's entries, spec and derived
//! aliases under another name, in one write ([`Db::rename_field`]).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::RenameArgs;

pub fn rename(args: RenameArgs, db: &Db) -> Result<()> {
    let RenameArgs { old, new, dry_run } = args;
    crate::commands::helpers::refuse_hid_named_field(db, &new)?;
    let done = db.rename_field(&old, &new, dry_run)?;
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
    if done.spec {
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
    Ok(())
}
