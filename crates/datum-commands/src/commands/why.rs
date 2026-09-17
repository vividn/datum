//! `datum why <id_or_slug>` — show provenance (raw_input + alias_chain
//! + the reconstructed spec v2.1 wrapped expansion, da-g5su).

use anyhow::{Context, Result};
use datum_core::db::Db;

use crate::args::WhyArgs;
use crate::commands::helpers::{
    assembled_expansion, for_each_id, format_version_row, resolve_prefix_or_tombstone,
};
use crate::render::action::now_local;

/// `why A,B` answers for each entry in turn, each under its own `hid:` line
/// (da-9p3o).
pub fn why(args: WhyArgs, db: &Db) -> Result<()> {
    for_each_id(&args.id_or_slug, |id| why_of(&args, id, db))
}

fn why_of(args: &WhyArgs, id: &str, db: &Db) -> Result<()> {
    let current =
        resolve_prefix_or_tombstone(db, id).with_context(|| format!("entry not found: {id}"))?;

    println!("hid: {}  slug: {}", current.hid, current.slug);
    let raw = current.raw_input.as_deref().filter(|s| !s.is_empty());
    if let Some(raw) = raw {
        println!("  raw_input:   {raw}");
    }
    if let Some(chain) = &current.alias_chain {
        println!("  alias_chain: {}", chain);
        // The assembled v2.1 wrapped expansion — every alias resolve
        // reads as `( body ,, own-trailing )`, so region scope is
        // visible right in the stream. Reconstructed best-effort from
        // raw_input + the stored chain (da-g5su).
        if let Some(expanded) = raw.and_then(|r| assembled_expansion(r, Some(chain))) {
            println!("  expanded:    {expanded}");
        }
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
            let raw = v.raw_input.as_deref().filter(|s| !s.is_empty());
            if let Some(expanded) = raw.and_then(|r| assembled_expansion(r, Some(chain))) {
                println!("       expanded:    {expanded}");
            }
        }
    }
    Ok(())
}
