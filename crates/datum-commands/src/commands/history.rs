//! `datum history <id_or_slug>` — an entry's history, a version per op
//! (da-s4erq): read from its ops, so every copy that holds them prints the
//! same trail, forks and joins included. `entry_history` is the undo journal.

use anyhow::{Context, Result};
use datum_core::{db::Db, entry::Entry};
use serde_json::Value as JsonValue;

use crate::args::{HistoryArgs, ShowFlag};
use crate::commands::helpers::{
    for_each_id, is_id_list, names_devices, resolve_prefix_or_tombstone, version_lines,
};
use crate::render::action::now_local;

/// `history A,B` prints each entry's trail in turn, each under its own
/// `hid:` line (da-9p3o). A version number belongs to ONE entry's trail, so
/// `--version` / `--diff` with a list is refused rather than guessed at.
/// `--show json` with a list is ONE array of `{hid, versions}`, one per entry.
pub fn history(args: HistoryArgs, db: &Db) -> Result<()> {
    if is_id_list(db, &args.id_or_slug) && (args.version.is_some() || args.diff.is_some()) {
        anyhow::bail!(
            "--version and --diff name versions of ONE entry's history; \
             give one id, not the list '{}'",
            args.id_or_slug
        );
    }
    if args.show == ShowFlag::Json && is_id_list(db, &args.id_or_slug) {
        let mut trails = Vec::new();
        let done = for_each_id(db, &args.id_or_slug, |id| {
            history_of(&args, id, db, Some(&mut trails))
        });
        println!("{}", serde_json::to_string_pretty(&trails)?);
        return done;
    }
    for_each_id(db, &args.id_or_slug, |id| history_of(&args, id, db, None))
}

/// One entry's history. `trails` collects a list's JSON instead of printing it.
fn history_of(
    args: &HistoryArgs,
    id: &str,
    db: &Db,
    trails: Option<&mut Vec<JsonValue>>,
) -> Result<()> {
    let json = args.show == ShowFlag::Json;
    let current =
        resolve_prefix_or_tombstone(db, id).with_context(|| format!("entry not found: {id}"))?;
    let hid = current.hid.clone();

    if args.show == ShowFlag::None {
        return Ok(());
    }

    // `--version N` wins over `--diff` when both are set. The snapshot is
    // rendered as pretty JSON either way — `--show json` is a no-op here
    // but accepted for symmetry with the list/diff forms.
    let snapshot = |n: u32| {
        db.entry_at_version(&current, n)?
            .ok_or_else(|| anyhow::anyhow!("version {n} not found for entry {hid}"))
    };

    if let Some(v) = args.version {
        println!("{}", serde_json::to_string_pretty(&snapshot(v)?)?);
        return Ok(());
    }

    if let Some(ab) = args.diff.as_ref() {
        let (a, b) = (ab[0], ab[1]);
        let (snap_a, snap_b) = (snapshot(a)?, snapshot(b)?);
        if json {
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
    let mut versions = db.versions(&current)?;
    let devices = names_devices(db, &versions)?;
    versions.reverse();

    if json {
        match trails {
            Some(trails) => trails.push(serde_json::json!({ "hid": hid, "versions": versions })),
            None => println!("{}", serde_json::to_string_pretty(&versions)?),
        }
        return Ok(());
    }

    println!("hid: {}", hid);
    if versions.is_empty() {
        println!("  (no history recorded)");
        return Ok(());
    }
    let now = now_local();
    for v in versions {
        let raw = match v.raw.as_deref() {
            None | Some("") => String::new(),
            Some(s) => format!("'{s}'"),
        };
        let mut lines = version_lines(&v, &now, &raw, devices);
        lines.insert(1, format!("        origin: {}", origin(v.origin.as_ref())));
        println!("{}", lines.join("\n"));
    }
    Ok(())
}

/// Who and what signed a version (da-zjc9w): `user@host`, the program and
/// its version, the installation, the script. A part the op does not carry
/// is said to be unknown, and an op signed before origins says so.
fn origin(o: Option<&JsonValue>) -> String {
    let Some(o) = o else {
        return "not recorded".into();
    };
    let part = |k: &str| o[k].as_str();
    let who = match (part("user"), part("host")) {
        (None, None) => "user and host unknown".to_string(),
        (u, h) => format!(
            "{}@{}",
            u.unwrap_or("(user unknown)"),
            h.unwrap_or("(host unknown)")
        ),
    };
    let program = [part("program"), part("version")].into_iter().flatten();
    let parts = [
        Some(who),
        Some(program.collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()),
        part("installation").map(|i| format!("installation {i}")),
        part("script").map(|s| format!("script {s}")),
    ];
    parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

fn print_history_diff(a: u32, snap_a: &Entry, b: u32, snap_b: &Entry) {
    println!("--- v{a}\n+++ v{b}");
    for line in history_diff_lines(snap_a, snap_b) {
        println!("{line}");
    }
}

/// Diff lines between two snapshots. `content_hash` is derived from the
/// fields being diffed, so its -/+ pair only restates the adjacent real
/// diff lines as hex noise — skip it (da-r8xj). `modify_time` lines stay:
/// they tell the user when each side was written.
fn history_diff_lines(snap_a: &Entry, snap_b: &Entry) -> Vec<String> {
    let va = serde_json::to_value(snap_a).unwrap_or(serde_json::Value::Null);
    let vb = serde_json::to_value(snap_b).unwrap_or(serde_json::Value::Null);
    let mut lines = Vec::new();
    diff_json("", &va, &vb, &mut lines);
    lines
}

fn diff_json(path: &str, a: &JsonValue, b: &JsonValue, lines: &mut Vec<String>) {
    if a == b {
        return;
    }
    match (a, b) {
        (JsonValue::Object(ma), JsonValue::Object(mb)) => {
            let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                // Derived dedup key — restates the real diff as hex noise.
                if path.is_empty() && k == "content_hash" {
                    continue;
                }
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match (ma.get(k), mb.get(k)) {
                    (Some(va), Some(vb)) => diff_json(&child_path, va, vb, lines),
                    (Some(va), None) => {
                        lines.push(format!("- {child_path}: {}", render_scalar(va)))
                    }
                    (None, Some(vb)) => {
                        lines.push(format!("+ {child_path}: {}", render_scalar(vb)))
                    }
                    (None, None) => {}
                }
            }
        }
        _ => {
            lines.push(format!("- {}: {}", path, render_scalar(a)));
            lines.push(format!("+ {}: {}", path, render_scalar(b)));
        }
    }
}

fn render_scalar(v: &JsonValue) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::test_util::entry_builder;
    use serde_json::json;

    /// da-r8xj: the diff of a state change must not include the derived
    /// content_hash -/+ pair — it only restates the adjacent real diff
    /// lines as hex noise.
    #[test]
    fn diff_of_state_change_has_no_content_hash_line() {
        let mut a = entry_builder("mood").build();
        a.state = Some(json!("happy"));
        let mut b = a.clone();
        b.state = Some(json!("sad"));
        b.content_hash = "feedface".into(); // diverged, as a real edit would

        let lines = history_diff_lines(&a, &b);
        assert!(
            lines.iter().any(|l| l.contains("state")),
            "real state diff is present: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("content_hash")),
            "derived content_hash must be skipped: {lines:?}"
        );
    }
}
