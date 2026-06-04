//! `datum why <id_or_slug>` — show provenance (raw_input + alias_chain).

use anyhow::{Context, Result};
use datum_core::db::Db;

use crate::args::WhyArgs;
use crate::commands::helpers::{format_version_row, resolve_prefix_or_tombstone};
use crate::render::action::now_local;

pub fn why(args: WhyArgs, db: &Db) -> Result<()> {
    let current = resolve_prefix_or_tombstone(db, &args.id_or_slug)
        .with_context(|| format!("entry not found: {}", args.id_or_slug))?;

    println!("hid: {}  slug: {}", current.hid, current.slug);
    if let Some(raw) = current.raw_input.as_deref().filter(|s| !s.is_empty()) {
        println!("  raw_input:   {raw}");
    }
    if let Some(chain) = &current.alias_chain {
        println!("  alias_chain: {}", chain);
    }

    if !args.full {
        return Ok(());
    }

    let versions =
        datum_core::history::list(db.conn(), &current.hid, true).map_err(anyhow::Error::from)?;
    println!();
    println!("history (newest first):");
    if versions.is_empty() {
        println!("  (no history recorded)");
        return Ok(());
    }
    let now = now_local();
    for v in versions {
        let raw = match v.raw_input.as_deref() {
            None | Some("") => "<no raw_input captured>".to_string(),
            Some(s) => s.to_string(),
        };
        println!("{}", format_version_row(&v, &now, &raw));
        if let Some(chain) = &v.alias_chain {
            println!("       alias_chain: {chain}");
        }
    }
    Ok(())
}
