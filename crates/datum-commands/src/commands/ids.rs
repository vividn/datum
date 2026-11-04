//! `datum ids` — list entry ids (or slugs) in time order.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::{IdsArgs, ShowFlag};
use crate::commands::time_bounds::parse_since;

pub fn ids(args: IdsArgs, db: &Db) -> Result<()> {
    if args.refs {
        return refs(&args, db);
    }
    let ids = compute_ids(&args, db)?;
    if args.show == ShowFlag::None {
        return Ok(());
    }
    if args.show == ShowFlag::Json {
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
        &args.fields,
        since_utc,
        args.limit,
        args.oldest_first,
        args.slug,
    )?)
}

/// `ids --ref`: each `_n` and the hid it names, `_1` first, then each `_+n`,
/// `_+1` first (da-xxmhn); `--json` is one object, `_n` → hid (da-kjikt: what datum.day's hid pill is labeled by).
/// The ordinals are global, so its parser refuses a FIELD, `--since`,
/// `--slug` and `--oldest-first` beside it (`parse_ids_args`); only `-n`
/// applies.
fn refs(args: &IdsArgs, db: &Db) -> Result<()> {
    let mut refs: Vec<(datum_core::ids::QuickRef, String)> = datum_core::ids::quick_ordinals(db)?
        .into_iter()
        .map(|(hid, n)| (n, hid.0))
        .collect();
    refs.sort();
    refs.truncate(args.limit.unwrap_or(usize::MAX));
    match args.show {
        ShowFlag::None => {}
        ShowFlag::Json => {
            let map: serde_json::Map<String, serde_json::Value> = refs
                .into_iter()
                .map(|(r, hid)| (r.to_string(), hid.into()))
                .collect();
            println!("{}", serde_json::Value::Object(map));
        }
        _ => {
            for (r, hid) in refs {
                println!("{r}  {hid}");
            }
        }
    }
    Ok(())
}
