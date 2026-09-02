//! `datum ids` — list entry ids (or slugs) in time order.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::IdsArgs;
use crate::commands::time_bounds::parse_since;

pub fn ids(args: IdsArgs, db: &Db) -> Result<()> {
    let ids = compute_ids(&args, db)?;
    if args.json {
        let arr: Vec<serde_json::Value> = ids
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect();
        println!("{}", serde_json::Value::Array(arr));
    } else {
        for id in ids {
            println!("{id}");
        }
    }
    Ok(())
}

pub fn compute_ids(args: &IdsArgs, db: &Db) -> Result<Vec<String>> {
    let since_utc = args.since.as_deref().map(parse_since).transpose()?;
    Ok(db.list_entry_ids(
        args.field.as_deref(),
        since_utc,
        args.limit,
        args.oldest_first,
        args.slug,
    )?)
}
