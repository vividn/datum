//! `datum why <id_or_slug>` — show provenance (raw_input + alias_chain).

use anyhow::{Context, Result};
use datum_core::db::Db;

use crate::args::WhyArgs;
use crate::cmd::resolve_prefix_or_tombstone;

pub fn why(args: WhyArgs, db: &Db) -> Result<()> {
    let current = resolve_prefix_or_tombstone(db, &args.id_or_slug)
        .with_context(|| format!("entry not found: {}", args.id_or_slug))?;

    println!("hid: {}  slug: {}", current.hid, current.slug);
    if !current.raw_input.is_empty() {
        println!("  raw_input:   {}", current.raw_input);
    }
    if let Some(chain) = &current.alias_chain {
        println!("  alias_chain: {}", chain);
    }

    if !args.full {
        return Ok(());
    }

    let versions = datum_core::history::list(db.conn(), &current.hid, true)
        .map_err(anyhow::Error::from)?;
    println!();
    println!("history (newest first):");
    if versions.is_empty() {
        println!("  (no history recorded)");
        return Ok(());
    }
    for v in versions {
        let raw = if v.raw_input.is_empty() {
            "<no raw_input captured>".to_string()
        } else {
            v.raw_input.clone()
        };
        println!(
            "  v{}  {}  {}  {}",
            v.version,
            v.modify_time.format("%Y-%m-%dT%H:%M"),
            v.change_kind,
            raw
        );
        if let Some(chain) = &v.alias_chain {
            println!("       alias_chain: {chain}");
        }
    }
    Ok(())
}
