//! `datum debug` — test situations made in the current database
//! (da-4dvk7). The work is datum-core's (`db/sync/debug.rs`); this prints it.

use anyhow::{Context, Result};
use datum_core::db::Db;
use serde_json::Value;

use crate::args::DebugCommand;
use crate::commands::helpers::resolve_prefix_or_tombstone;

pub fn debug(cmd: DebugCommand, db: &Db) -> Result<()> {
    match cmd {
        DebugCommand::Conflicts(n) => conflicts(n, db),
        DebugCommand::Ops(id) => ops(&id, db),
    }
}

fn conflicts(n: usize, db: &Db) -> Result<()> {
    let made = db.debug_conflicts(n)?;
    println!(
        "{} sync conflicts filed against debug device {}:",
        made.filed,
        short(&made.device)
    );
    let (deleted, edited) = made.hids.split_last().expect("one entry at least");
    for hid in edited {
        println!("  {hid}  edited here and there");
    }
    println!("  {deleted}  deleted here, edited there");
    println!(
        "\nSee them: datum conflicts. Each entry's ops: datum debug ops <hid>.\n\
         This copy holds the debug device's ops now, so it no longer syncs."
    );
    Ok(())
}

fn ops(id: &str, db: &Db) -> Result<()> {
    let entry =
        resolve_prefix_or_tombstone(db, id).with_context(|| format!("entry not found: {id}"))?;
    let ops = db.debug_ops(entry.hid.as_str())?;
    if ops.is_empty() {
        println!(
            "{} has no ops yet: it is older than ops, and is signed when it first syncs",
            entry.hid
        );
        return Ok(());
    }
    let mine = db.sync_endpoint_id()?;
    println!("{}  {}", entry.hid, entry.slug);
    for op in &ops {
        let endpoint = op["endpoint"].as_str().unwrap_or_default();
        let deps = op["deps"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(e, n)| format!("{}:{n}", short(e)))
            .collect::<Vec<_>>();
        println!(
            "  {hlc}  {by}:{seq}{this}  {kind:<8} deps {{{deps}}}{state}  {effect}",
            hlc = op["hlc"].as_str().unwrap_or_default(),
            by = short(endpoint),
            seq = op["seq"],
            this = if endpoint == mine { " (this copy)" } else { "" },
            kind = op["kind"].as_str().unwrap_or_default(),
            deps = deps.join(", "),
            state = match op["applied"].as_i64() {
                Some(0) => "  WAITING",
                Some(2) => "  REFUSED",
                _ => "",
            },
            effect = effect(&op["effect"]),
        );
    }
    Ok(())
}

/// An endpoint id's first eight hex digits, as a person compares them.
fn short(endpoint: &str) -> &str {
    &endpoint[..endpoint.len().min(8)]
}

/// What an op changed: `create`, or the keys it set and unset, and a
/// delete — leaving out the bookkeeping every edit moves.
fn effect(effect: &Value) -> String {
    if effect.get("create").is_some() {
        return "create".into();
    }
    let mut out = Vec::new();
    for (k, v) in effect["set"].as_object().into_iter().flatten() {
        if !matches!(k.as_str(), "modify_time" | "content_hash") {
            out.push(format!("{k}={v}"));
        }
    }
    for k in effect["unset"].as_array().into_iter().flatten() {
        out.push(format!("-{}", k.as_str().unwrap_or_default()));
    }
    match effect["deleted"].as_bool() {
        Some(true) => out.push("deleted".into()),
        Some(false) => out.push("undeleted".into()),
        None => {}
    }
    out.join(" ")
}
