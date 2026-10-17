//! `datum conflicts` — the entries whose ops leave a key contested
//! (da-05rkv): values two copies set without seeing each other's. The sync
//! fold writes them into each entry's `conflicts` column, so every copy that
//! holds the same ops lists the same ones, and a resolution is an ordinary
//! edit that saw every side, which settles them wherever it arrives.
//!
//! A key's sides are lettered A, B, … in the order the column holds them,
//! the fold's: A the earlier clock, B the later. The letters are read off
//! the column, so they are the same on every copy (da-07594).
//!
//! - `list` — each such entry, and per key each side: its letter, the
//!   device that set it, when, and the value. `--json`: the column as stored.
//! - `resolve <HID> [STRATEGY] [KEY=a|b|VALUE…]` — that edit: each contested
//!   key the side or the value picked for it, the rest what the strategy
//!   named makes of A and B ([`combine`]), or with none named the value the
//!   entry shows: a bare `resolve` settles and changes no data.

use anyhow::{anyhow, bail, Result};
use datum_core::db::Db;
use datum_parse::combine::combine;
use datum_parse::input::infer_type;
use serde_json::{json, Map, Value};

use crate::args::{ConflictsCommand, ConflictsResolveArgs, ShowFlag};
use crate::commands::helpers::resolve_prefix_or_tombstone;

pub fn conflicts(cmd: ConflictsCommand, db: &Db) -> Result<()> {
    match cmd {
        ConflictsCommand::List(args) => list(db, args.show == ShowFlag::Json),
        ConflictsCommand::Resolve(args) => resolve(args, db),
    }
}

fn list(db: &Db, json: bool) -> Result<()> {
    let rows = db.sync_conflicts()?;
    if json {
        let rows: Vec<Value> = rows
            .iter()
            .map(|e| {
                json!({
                    "hid": e.hid, "slug": e.slug,
                    "deleted": e.deleted_at.is_some(), "conflicts": e.conflicts,
                })
            })
            .collect();
        println!("{}", Value::Array(rows));
        return Ok(());
    }
    if rows.is_empty() {
        println!("No sync conflicts. (`datum check` reports integrity findings.)");
        return Ok(());
    }
    for e in &rows {
        println!(
            "{hid}  {slug}{deleted}",
            hid = e.short_uid(),
            slug = e.slug,
            deleted = if e.deleted_at.is_some() {
                "  (deleted)"
            } else {
                ""
            },
        );
        let conflicts = e.conflicts.as_ref().and_then(Value::as_object);
        for (key, sides) in conflicts.into_iter().flatten() {
            println!("    {key}");
            for (side, letter) in sides.as_array().into_iter().flatten().zip('A'..) {
                let device = side["device"].as_str().unwrap_or("?");
                let at = datum_core::db::chain::hlc_time(side["hlc"].as_str().unwrap_or(""));
                // Two sides often land within a minute, so to the second.
                let at = crate::commands::sync::on_readers_clock(at, "%H:%M:%S");
                // `deleted`'s sides are a delete and what would keep the
                // entry: an edit made without seeing it, or an undelete.
                let said = match (key.as_str(), side["value"].as_bool()) {
                    ("deleted", Some(true)) => format!("deleted on {device}, {at}"),
                    ("deleted", Some(false)) => format!("kept on {device}, {at}"),
                    _ => format!("{device:<12} {at}  {}", side["value"]),
                };
                println!("      {letter}  {said}");
            }
        }
    }
    println!(
        "\nResolve with: datum conflicts resolve <hid> \
         [--use-a | --use-b | --prefer-a | --prefer-b | --merge] [KEY=a|b|VALUE…]\n\
         (a key not picked keeps what the entry shows unless a strategy is named)"
    );
    Ok(())
}

fn resolve(args: ConflictsResolveArgs, db: &Db) -> Result<()> {
    let e = resolve_prefix_or_tombstone(db, &args.hid)?;
    let none = Map::new();
    let conflicts = e.conflicts.as_ref().and_then(Value::as_object);
    let conflicts = conflicts.unwrap_or(&none);
    // A data key is picked as it is typed, `cups` for `data.cups`.
    let mut picks = Map::new();
    for (k, v) in args.picks {
        let (key, sides) = [k.clone(), format!("data.{k}")]
            .into_iter()
            .find_map(|key| {
                let sides = conflicts.get(&key)?.as_array()?;
                Some((key, sides))
            })
            .ok_or_else(|| anyhow!("{} has no conflict on {k}", e.short_uid()))?;
        // One letter names a side, in either case; anything else is a value.
        let value = match v.as_bytes() {
            [c] if c.is_ascii_alphabetic() => {
                let side = sides.get((c.to_ascii_lowercase() - b'a') as usize);
                side.map(|s| s["value"].clone()).ok_or_else(|| {
                    anyhow!(
                        "{key} has {n} sides, and {v} is none of them; \
                         for the text itself: {k}='\"{v}\"'",
                        n = sides.len()
                    )
                })?
            }
            _ => infer_type(&v).unwrap_or(Value::Null),
        };
        picks.insert(key, value);
    }
    // The rest are the strategy's, over the two sides of each: a side that
    // unset the key does not have it. With no strategy they are left out,
    // and `resolve_conflicts` keeps what the entry shows.
    if let Some(how) = args.strategy {
        let (mut a, mut b) = (Map::new(), Map::new());
        for (key, sides) in conflicts.iter().filter(|(k, _)| !picks.contains_key(*k)) {
            let [sa, sb] = sides.as_array().map(Vec::as_slice).unwrap_or_default() else {
                bail!("{key} has more than two sides: pick one, {key}=a, or give a value");
            };
            for (version, side) in [(&mut a, sa), (&mut b, sb)] {
                if !side["value"].is_null() {
                    version.insert(key.clone(), side["value"].clone());
                }
            }
        }
        let combined = combine(&a, &b, how)
            .map_err(|err| anyhow!("{err}; pick that key instead: KEY=a, KEY=b or KEY=VALUE"))?;
        for key in a.keys().chain(b.keys()) {
            let value = combined.get(key).cloned().unwrap_or(Value::Null);
            picks.entry(key.clone()).or_insert(value);
        }
    }
    let post = db.resolve_conflicts(e.hid.as_str(), &picks)?;
    for (key, value) in &picks {
        match (key.as_str(), value.as_bool()) {
            ("deleted", Some(true)) => println!("    deleted"),
            ("deleted", Some(false)) => println!("    kept"),
            _ => println!("    {key}  {value}"),
        }
    }
    println!("Resolved {} ({})", post.slug, post.short_uid());
    Ok(())
}
