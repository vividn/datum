//! `datum history <id_or_slug>` — show the modification trail for an entry.

use anyhow::{Context, Result};
use datum_core::{db::Db, entry::Entry};
use serde_json::Value as JsonValue;

use crate::args::HistoryArgs;
use crate::cmd::resolve_prefix_or_tombstone;

pub fn history(args: HistoryArgs, db: &Db) -> Result<()> {
    let current = resolve_prefix_or_tombstone(db, &args.id_or_slug)
        .with_context(|| format!("entry not found: {}", args.id_or_slug))?;
    let hid = current.hid.clone();

    // `--version N` wins over `--diff` when both are set. The snapshot is
    // rendered as pretty JSON either way — `--json` is a no-op here but
    // accepted for symmetry with the list/diff forms.
    if let Some(v) = args.version {
        let snap = resolve_version_snapshot(db, &current, v)?;
        println!("{}", serde_json::to_string_pretty(&snap)?);
        return Ok(());
    }

    if let Some(ab) = args.diff.as_ref() {
        let (a, b) = (ab[0], ab[1]);
        let snap_a = resolve_version_snapshot(db, &current, a)?;
        let snap_b = resolve_version_snapshot(db, &current, b)?;
        if args.json {
            let payload = serde_json::json!({
                "a": { "version": a, "snapshot": snap_a },
                "b": { "version": b, "snapshot": snap_b },
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            print_history_diff(a, &snap_a, b, &snap_b);
        }
        return Ok(());
    }

    // Default: list every version, newest first.
    let versions = datum_core::history::list(db.conn(), &hid, true).map_err(anyhow::Error::from)?;

    if args.json {
        let payload: Vec<_> = versions
            .iter()
            .map(|v| {
                serde_json::json!({
                    "version": v.version,
                    "modify_time": v.modify_time.to_rfc3339(),
                    "change_kind": v.change_kind.as_str(),
                    "raw_input": v.raw_input,
                    "alias_chain": v.alias_chain,
                    "device_id": v.device_id,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!("hid: {}", hid);
    if versions.is_empty() {
        println!("  (no history recorded)");
        return Ok(());
    }
    for v in versions {
        let raw = if v.raw_input.is_empty() {
            String::new()
        } else {
            format!("'{}'", v.raw_input)
        };
        println!(
            "  v{}  {}  {:<22} {}",
            v.version,
            v.modify_time.format("%Y-%m-%dT%H:%M"),
            v.change_kind.as_str(),
            raw
        );
    }
    Ok(())
}

/// Resolve "state at version N" for display or diff. Uses
/// [`datum_core::history::state_at_version`] so the call site doesn't need
/// to re-encode the v1-is-special rule.
fn resolve_version_snapshot(db: &Db, current: &Entry, version: u32) -> Result<Entry> {
    datum_core::history::state_at_version(db.conn(), &current.hid, version, Some(current))
        .map_err(anyhow::Error::from)?
        .ok_or_else(|| anyhow::anyhow!("version {version} not found for entry {}", current.hid))
}

fn print_history_diff(a: u32, snap_a: &Entry, b: u32, snap_b: &Entry) {
    let va = serde_json::to_value(snap_a).unwrap_or(serde_json::Value::Null);
    let vb = serde_json::to_value(snap_b).unwrap_or(serde_json::Value::Null);
    println!("--- v{a}\n+++ v{b}");
    diff_json("", &va, &vb);
}

fn diff_json(path: &str, a: &JsonValue, b: &JsonValue) {
    if a == b {
        return;
    }
    match (a, b) {
        (JsonValue::Object(ma), JsonValue::Object(mb)) => {
            let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match (ma.get(k), mb.get(k)) {
                    (Some(va), Some(vb)) => diff_json(&child_path, va, vb),
                    (Some(va), None) => println!("- {child_path}: {}", render_scalar(va)),
                    (None, Some(vb)) => println!("+ {child_path}: {}", render_scalar(vb)),
                    (None, None) => {}
                }
            }
        }
        _ => {
            println!("- {}: {}", path, render_scalar(a));
            println!("+ {}: {}", path, render_scalar(b));
        }
    }
}

fn render_scalar(v: &JsonValue) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| v.to_string())
}
