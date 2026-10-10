//! Sync's diagnostics as a person reads them (da-n0ruf): `datum check
//! sync`'s findings, `datum sync refold`, and `datum sync diff <WHO>`. What
//! they say is `datum_core::db::sync::diagnose`'s; here it is put in words.

use anyhow::{bail, Result};
use datum_core::db::{Db, FoldCheck, Stale};
use serde_json::{json, Map, Value};

use crate::commands::check::{FlaggedEntry, Issue, Severity};

/// How many uncovered entries a finding names before it counts the rest.
const NAMED: usize = 10;

/// A value on one line: its JSON, cut to fit, and `∅` for none.
fn shown(v: &Value) -> String {
    let s = match v {
        Value::Null => return "∅".into(),
        v => v.to_string(),
    };
    match s.chars().count() > 60 {
        true => format!("{}…", s.chars().take(59).collect::<String>()),
        false => s,
    }
}

fn short(hid: &str) -> String {
    hid.chars().take(5).collect()
}

fn n(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// One op on a line: who made it, what it is and when, and how it is held
/// where that is not applied.
fn op_words(op: &Value) -> String {
    let at = |k: &str| op[k].as_str().unwrap_or_default();
    let when = at("hlc").split('/').next().unwrap_or_default();
    let held = match at("held") {
        "applied" => String::new(),
        other => format!(" [{other}]"),
    };
    format!("{} by {} {when}{held}", at("kind"), at("device"))
}

fn ops_words(ops: &[Value]) -> String {
    match ops.is_empty() {
        true => "none".into(),
        false => ops.iter().map(op_words).collect::<Vec<_>>().join("; "),
    }
}

/// A stale row's lines: its name, then each column with both values.
fn stale_lines(s: &Stale, (was, now): (&str, &str)) -> String {
    let mut out = format!("  {} ({})", s.slug, short(&s.hid));
    for (k, (row, ops)) in &s.columns {
        out.push_str(&format!(
            "\n    {k}: {was} {}, {now} {}",
            shown(row),
            shown(ops)
        ));
    }
    out
}

fn issue(
    severity: Severity,
    category: &'static str,
    message: String,
    of: &[FlaggedEntry],
) -> Issue {
    Issue {
        severity,
        category,
        message,
        count: of.len(),
        example: None,
        entries: of.to_vec(),
        rendered_block: String::new(),
    }
}

/// `datum check sync`: this copy's rows read against its ops. An entry with
/// ops and no row, and a row that is not its ops' fold, are errors: either
/// makes this copy differ from one that applied the same ops. So are ops
/// that fold to no row storage takes, which the check cannot read further.
/// A row no op covers is only not yet sent.
pub fn fold_issues(db: &Db) -> Result<(FoldCheck, Vec<Issue>)> {
    let found = db.sync_fold_check()?;
    let mut out = Vec::new();
    if !found.orphans.is_empty() {
        let lines: Vec<String> = found
            .orphans
            .iter()
            .map(|o| format!("  {} ({}): {}", o.slug, short(&o.hid), ops_words(&o.ops)))
            .collect();
        let of: Vec<FlaggedEntry> = found
            .orphans
            .iter()
            .map(|o| FlaggedEntry {
                hid: short(&o.hid),
                field: o.field.clone(),
                state: format!("{} with no row", n(o.ops.len(), "op", "ops")),
            })
            .collect();
        let message = format!(
            "this copy holds the sync ops of {} it has no row for: every copy it syncs with \
             makes {} again from those ops, so their verify never agrees. Nothing removes \
             them, `datum sync refold` included: whether the ops go or the rows come back \
             is not decided.\n{}",
            n(of.len(), "entry", "entries"),
            if of.len() == 1 { "it" } else { "them" },
            lines.join("\n"),
        );
        out.push(issue(Severity::Error, "sync_orphan_ops", message, &of));
    }
    if !found.stale.is_empty() {
        let lines: Vec<String> = found
            .stale
            .iter()
            .map(|s| stale_lines(s, ("row", "ops")))
            .collect();
        let of: Vec<FlaggedEntry> = found
            .stale
            .iter()
            .map(|s| FlaggedEntry {
                hid: short(&s.hid),
                field: s.field.clone(),
                state: s.columns.keys().cloned().collect::<Vec<_>>().join(", "),
            })
            .collect();
        let message = format!(
            "{} not what {} ops fold to, so this copy shows something a copy that applied \
             the same ops does not. `datum sync refold` writes {} again from the ops.\n{}",
            match of.len() {
                1 => "1 row is".into(),
                n => format!("{n} rows are"),
            },
            if of.len() == 1 { "its" } else { "their" },
            if of.len() == 1 { "it" } else { "them" },
            lines.join("\n"),
        );
        out.push(issue(Severity::Error, "sync_stale_row", message, &of));
    }
    if !found.unfoldable.is_empty() {
        let lines: Vec<String> = found
            .unfoldable
            .iter()
            .map(|(e, why)| format!("  {} ({}): {why}", e.slug, short(&e.hid)))
            .collect();
        let of: Vec<FlaggedEntry> = found
            .unfoldable
            .iter()
            .map(|(e, why)| FlaggedEntry {
                hid: short(&e.hid),
                field: e.field.clone(),
                state: why.clone(),
            })
            .collect();
        let message = format!(
            "the ops of {} fold to no row this datum can store, so {} neither checked against \
             {} ops nor refolded; why is beside each.\n{}",
            n(of.len(), "entry", "entries"),
            if of.len() == 1 { "it is" } else { "they are" },
            if of.len() == 1 { "its" } else { "their" },
            lines.join("\n"),
        );
        out.push(issue(Severity::Error, "sync_unfoldable", message, &of));
    }
    if !found.uncovered.is_empty() {
        let count = found.uncovered.len();
        let mut names: Vec<String> = found
            .uncovered
            .iter()
            .take(NAMED)
            .map(|e| format!("  {} ({})", e.slug, short(&e.hid)))
            .collect();
        if count > NAMED {
            names.push(format!("  and {} more", count - NAMED));
        }
        let of: Vec<FlaggedEntry> = found
            .uncovered
            .iter()
            .map(|e| FlaggedEntry {
                hid: short(&e.hid),
                field: e.field.clone(),
                state: "no op".into(),
            })
            .collect();
        let message = format!(
            "{} no sync op, so no other copy holds {} yet: {}.\n{}",
            match count {
                1 => "1 entry has".into(),
                n => format!("{n} entries have"),
            },
            if count == 1 { "it" } else { "them" },
            match found.reader {
                true => "this copy is a reader of its chain and signs nothing, so they stay here",
                false => "this copy's next sync signs each as it stands",
            },
            names.join("\n"),
        );
        out.push(issue(Severity::Warning, "sync_no_op", message, &of));
    }
    Ok((found, out))
}

fn stale_json(s: &Stale, (was, now): (&str, &str)) -> Value {
    let columns: Map<String, Value> = s
        .columns
        .iter()
        .map(|(k, (a, b))| (k.clone(), json!({ was: a, now: b })))
        .collect();
    json!({ "hid": s.hid, "slug": s.slug, "columns": columns })
}

/// `datum sync refold [--json]`. The JSON is `rows` and `ops` examined,
/// `refolded` (per row, each column's `was` and `now`), `still` (rows that
/// differ from their ops after it: a bug in the fold, and exit 1, as a
/// failing `check --json` exits), and how many `orphans` and `unfoldable`
/// entries it left as found.
pub fn refold(db: &Db, json: bool) -> Result<()> {
    let (found, left) = db.sync_refold()?;
    let examined = format!(
        "{}, {}",
        n(found.entries, "row", "rows"),
        n(found.ops, "op", "ops")
    );
    if json {
        let list = |stale: &[Stale], names| -> Vec<Value> {
            stale.iter().map(|s| stale_json(s, names)).collect()
        };
        let out = json!({
            "rows": found.entries, "ops": found.ops,
            "refolded": list(&found.stale, ("was", "now")),
            "still": list(&left, ("row", "ops")),
            "orphans": found.orphans.len(),
            "unfoldable": found.unfoldable.len(),
        });
        println!("{out}");
    } else if found.stale.is_empty() {
        println!("nothing to refold: every row is what its ops fold to ({examined} examined)");
    } else {
        println!(
            "refolded {} from {} ops ({examined} examined):",
            n(found.stale.len(), "row", "rows"),
            if found.stale.len() == 1 {
                "its"
            } else {
                "their"
            },
        );
        for s in &found.stale {
            println!("{}", stale_lines(s, ("was", "now")));
        }
    }
    for (count, what) in [
        (found.orphans.len(), "with ops and no row"),
        (found.unfoldable.len(), "whose ops fold to no row"),
    ] {
        if !json && count > 0 {
            println!(
                "{} {what} left as found: `datum check sync` names them",
                n(count, "entry", "entries")
            );
        }
    }
    if !left.is_empty() {
        for s in &left {
            eprintln!("{}", stale_lines(s, ("row", "ops")));
        }
        bail!(
            "{} still not what {} ops fold to after the refold: a bug in the fold, worth reporting with the lines above",
            match left.len() {
                1 => "1 row is".into(),
                n => format!("{n} rows are"),
            },
            if left.len() == 1 { "its" } else { "their" },
        );
    }
    Ok(())
}

/// The ops one side holds that the other does not, or holds otherwise
/// (applied here, refused there). An op's id is `endpoint/seq`, the `ops`
/// table's key, so a copy holds one op by an id and the map loses none.
#[cfg(feature = "net")]
fn only<'a>(side: &'a [Value], other: &[Value]) -> Vec<&'a Value> {
    let held: std::collections::BTreeMap<&str, &str> = other
        .iter()
        .filter_map(|o| Some((o["op"].as_str()?, o["held"].as_str()?)))
        .collect();
    side.iter()
        .filter(|o| held.get(o["op"].as_str().unwrap_or_default()).copied() != o["held"].as_str())
        .collect()
}

/// `datum sync diff <WHO> [--json]`: what this copy and the member
/// `endpoint`, `peer` in its chain, hold differently, as
/// [`datum_sync::diff`] found it.
#[cfg(feature = "net")]
pub fn diff(db: &Db, endpoint: &str, peer: &str, json: bool) -> Result<()> {
    use std::collections::BTreeMap;
    let found = datum_sync::diff(db, endpoint)?;
    let rows = |v: &Value| v["row"].as_object().cloned();
    let ops = |v: &Value| v["ops"].as_array().cloned().unwrap_or_default();
    // Paired by hid, not by place: an answer that leaves one out, or is in
    // another order, reads as the peer holding nothing of that entry.
    let theirs: BTreeMap<&str, &Value> = found
        .theirs
        .iter()
        .filter_map(|v| Some((v["hid"].as_str()?, v)))
        .collect();
    let mut entries = Vec::new();
    for mine in &found.mine {
        let nothing = json!({ "hid": mine["hid"], "row": null, "ops": [] });
        let theirs = theirs
            .get(mine["hid"].as_str().unwrap_or_default())
            .map_or(nothing, |v| (*v).clone());
        let (here, there) = (rows(mine), rows(&theirs));
        let columns = match (&here, &there) {
            (Some(a), Some(b)) => datum_core::db::row_diff(a, b),
            _ => BTreeMap::new(),
        };
        let slug = here
            .as_ref()
            .or(there.as_ref())
            .and_then(|r| r["slug"].as_str());
        entries.push((slug.unwrap_or_default().to_string(), mine, theirs, columns));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if json {
        let differ: Vec<Value> = entries
            .iter()
            .map(|(slug, mine, theirs, columns)| {
                let columns: Map<String, Value> = columns
                    .iter()
                    .map(|(k, (a, b))| (k.clone(), json!({ "here": a, "there": b })))
                    .collect();
                json!({ "hid": mine["hid"], "slug": slug, "columns": columns,
                        "here": mine, "there": theirs })
            })
            .collect();
        let out = json!({
            "peer": { "label": peer, "endpoint": endpoint },
            "compared": found.entries, "differ": differ,
        });
        println!("{out}");
        return Ok(());
    }
    let compared = n(found.entries, "entry", "entries");
    if entries.is_empty() {
        println!("this copy and {peer} hold the same data ({compared} compared)");
        return Ok(());
    }
    println!(
        "this copy and {peer} hold {} differently, of {compared} compared:",
        n(entries.len(), "entry", "entries")
    );
    for (slug, mine, theirs, columns) in &entries {
        let hid = short(mine["hid"].as_str().unwrap_or_default());
        println!("\n{slug} ({hid})");
        match (mine["row"].is_null(), theirs["row"].is_null()) {
            (true, _) => println!("  no row here; {peer} has one"),
            (_, true) => println!("  no row on {peer}; this copy has one"),
            _ => {}
        }
        for (k, (a, b)) in columns {
            println!("  {k}: here {}, {peer} {}", shown(a), shown(b));
        }
        let (a, b) = (ops(mine), ops(theirs));
        let (mine_only, theirs_only) = (only(&a, &b), only(&b, &a));
        if mine_only.is_empty() && theirs_only.is_empty() {
            println!(
                "  both hold the same {}, so one copy's row is not what they fold to: \
                 `datum check sync` on each says which",
                n(a.len(), "op", "ops")
            );
            continue;
        }
        let words = |ops: &[&Value]| {
            let ops: Vec<Value> = ops.iter().map(|o| (*o).clone()).collect();
            ops_words(&ops)
        };
        println!("  ops only here: {}", words(&mine_only));
        println!("  ops only on {peer}: {}", words(&theirs_only));
    }
    Ok(())
}
