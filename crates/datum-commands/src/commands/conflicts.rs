//! `datum conflicts` — the entries whose ops leave a key contested
//! (da-05rkv): values two copies set without seeing each other's. The sync
//! fold writes them into each entry's `conflicts` column, so every copy that
//! holds the same ops lists the same ones, and a resolution is an ordinary
//! edit that saw every side, which settles them wherever it arrives.
//!
//! - `list` — each such entry, and per key each side: the device that set
//!   it, when, and the value.
//! - `resolve <HID> [KEY=VALUE…]` — that edit: each contested key the value
//!   given, or else the one the entry shows now.

use anyhow::Result;
use datum_core::db::Db;
use serde_json::{Map, Value};

use crate::args::{ConflictsCommand, ConflictsResolveArgs};

pub fn conflicts(cmd: ConflictsCommand, db: &Db) -> Result<()> {
    match cmd {
        ConflictsCommand::List(_) => list(db),
        ConflictsCommand::Resolve(args) => resolve(args, db),
    }
}

fn list(db: &Db) -> Result<()> {
    let rows = db.sync_conflicts()?;
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
            for side in sides.as_array().into_iter().flatten() {
                println!(
                    "      {device:<12} {at}  {value}",
                    device = side["device"].as_str().unwrap_or("?"),
                    at = datum_core::db::chain::hlc_time(side["hlc"].as_str().unwrap_or("")),
                    value = side["value"],
                );
            }
        }
    }
    println!(
        "\nResolve with: datum conflicts resolve <hid> [KEY=VALUE…] \
         (a key not given keeps the value the entry shows)"
    );
    Ok(())
}

fn resolve(args: ConflictsResolveArgs, db: &Db) -> Result<()> {
    let e = db.get_entry_including_deleted(&args.hid)?;
    let picks: Map<String, Value> = args
        .picks
        .into_iter()
        .map(|(k, v)| {
            let v = serde_json::from_str(&v).unwrap_or(Value::String(v));
            (k, v)
        })
        .collect();
    let post = db.resolve_conflicts(e.hid.as_str(), &picks)?;
    println!("Resolved {} ({})", post.slug, post.short_uid());
    Ok(())
}
