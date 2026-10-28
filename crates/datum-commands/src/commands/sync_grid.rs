//! `datum sync grid` (da-n0uj1.3): every device this install's databases
//! know of, against every chain they are in. A row is a device, by the key
//! its members' `describe`s bind them to (da-n0uj1.2), or an endpoint alone
//! where none is bound; a column is a chain, by its id, labelled with the
//! local database's name, the one the command runs on first. Read only.

use std::path::{Path, PathBuf};

use anyhow::Result;
use datum_core::db::chain::{self, Chain, Member, Role};
use datum_core::db::Db;
use serde_json::{json, Value};

use crate::config;

/// A device's standing in one chain, weakest first: of two endpoints of one
/// device in a chain (it left and joined again), the stronger is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Cell {
    Unjoined,
    Wiped,
    Wiping,
    Expelled,
    Reader,
    Member,
}

impl Cell {
    /// One that left is in that chain no more, as `sync status` has it.
    fn of(m: &Member) -> Option<Cell> {
        match () {
            // An unjoined device may be wiped too: its copy is its own.
            _ if m.erased.is_some() => Some(Cell::Wiped),
            _ if m.erase.is_some() => Some(Cell::Wiping),
            _ if m.purged.is_some() => Some(Cell::Unjoined),
            _ if m.left.is_some() => None,
            _ if m.revoked.is_some() => Some(Cell::Expelled),
            _ if m.role() == Role::Reader => Some(Cell::Reader),
            _ => Some(Cell::Member),
        }
    }

    fn word(self) -> &'static str {
        match self {
            Cell::Unjoined => "unjoined",
            Cell::Wiped => "wiped",
            Cell::Wiping => "wiping",
            Cell::Expelled => "expelled",
            Cell::Reader => "reader",
            Cell::Member => "member",
        }
    }
}

struct Row {
    device: Option<String>,
    /// The first endpoint seen: an unbound row's only one.
    endpoint: String,
    label: String,
    /// Per column, the standing and the endpoint holding it.
    cells: Vec<Option<(Cell, String)>>,
}

pub fn grid(db: &Db, json: bool) -> Result<()> {
    let columns = chains(db)?;
    let mine = datum_core::db::device_id();
    let mut rows: Vec<Row> = Vec::new();
    for (j, (_, _, chain)) in columns.iter().enumerate() {
        for m in &chain.members {
            let Some(cell) = Cell::of(m) else { continue };
            // A bound member is its device's row whatever its endpoint, so a
            // device's endpoints in every chain are one row; an unbound one
            // is its endpoint's alone, never a bound row's.
            let at = rows.iter().position(|r| match &m.device {
                Some(d) => r.device.as_ref() == Some(d),
                None => r.device.is_none() && r.endpoint == m.endpoint,
            });
            let row = match at {
                Some(i) => &mut rows[i],
                None => {
                    rows.push(Row {
                        device: m.device.clone(),
                        endpoint: m.endpoint.clone(),
                        label: device_label(m),
                        cells: vec![None; columns.len()],
                    });
                    rows.last_mut().unwrap()
                }
            };
            if row.cells[j].as_ref().is_none_or(|(c, _)| cell > *c) {
                row.cells[j] = Some((cell, m.endpoint.clone()));
            }
        }
    }
    let this = |r: &Row| r.device.is_some() && r.device == mine;
    rows.sort_by_key(|r| !this(r));
    if json {
        let out = json!({
            "device": mine,
            "chains": columns.iter().map(|(name, _, c)| json!({"chain": c.id, "db": name})).collect::<Vec<_>>(),
            "devices": rows.iter().map(|r| json!({
                "device": r.device,
                "endpoint": r.device.is_none().then_some(&r.endpoint),
                "label": r.label,
                "this": this(r),
                "cells": columns.iter().zip(&r.cells).map(|((_, _, c), cell)| (
                    c.id.clone(),
                    cell.as_ref().map_or(Value::Null, |(cell, e)| json!({"state": cell.word(), "endpoint": e})),
                )).collect::<serde_json::Map<_, _>>(),
            })).collect::<Vec<_>>(),
        });
        println!("{out}");
        return Ok(());
    }
    // As `sync chain list`: a program gets an empty grid, a person why.
    anyhow::ensure!(
        !columns.is_empty(),
        "no database of this install is in a sync chain"
    );
    let label = rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    // A device key and an endpoint are each 32 bytes in hex, 64 ASCII
    // characters, or the fold takes neither: their head is safe to cut.
    let names: Vec<String> = rows
        .iter()
        .map(|r| match &r.device {
            Some(d) => format!("{:<label$}  {}", r.label, &d[..10]),
            None => format!("{:<label$}  endpoint {}", r.label, &r.endpoint[..10]),
        })
        .collect();
    let left = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    // A chain id is a ULID, whose head is only its time: shown whole.
    let widths: Vec<usize> = columns
        .iter()
        .map(|(name, _, c)| name.chars().count().max(c.id.chars().count()))
        .collect();
    let line = |first: &str, cells: Vec<&str>| {
        let cells: Vec<String> = cells
            .iter()
            .zip(&widths)
            .map(|(c, w)| format!("{c:<w$}"))
            .collect();
        format!("{first:<left$}  {}", cells.join("  "))
    };
    println!(
        "{}",
        line("", columns.iter().map(|(n, _, _)| n.as_str()).collect()).trim_end()
    );
    println!(
        "{}",
        line("", columns.iter().map(|(_, _, c)| c.id.as_str()).collect()).trim_end()
    );
    for (r, name) in rows.iter().zip(&names) {
        let cells = r
            .cells
            .iter()
            .map(|c| c.as_ref().map_or("—", |c| c.0.word()));
        let mut out = line(name, cells.collect()).trim_end().to_string();
        if this(r) {
            out += "  (this device)";
        }
        println!("{out}");
    }
    Ok(())
}

/// How a device's row is labelled by a member of it: its host, not the
/// database's name a chain may add.
pub(super) fn device_label(m: &Member) -> String {
    match (&m.host, m.auto) {
        (Some(host), true) => chain::assembled(m.user.as_deref(), host, None),
        _ => m.label.clone(),
    }
}

/// Every chain this install holds, once each, by the local database's name
/// and its path: `db`'s first, then each other database datum knows of, in
/// name order. One in no chain, or a dissolved one, has no column; one that
/// cannot be read is said on stderr.
pub(super) fn chains(db: &Db) -> Result<Vec<(String, PathBuf, Chain)>> {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let here = db.path().map(canon);
    let known: Vec<(String, PathBuf)> = config::known_dbs().into_iter().collect();
    let name_of = |path: &Path| {
        let names: Vec<&String> = known
            .iter()
            .filter(|(_, p)| canon(p) == path)
            .map(|(n, _)| n)
            .collect();
        // A registry entry kept by its path (da-zrl7n) is the least of names.
        let name = names.iter().find(|n| !n.contains('/')).or(names.first());
        name.map(|n| n.to_string()).unwrap_or_else(|| {
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
            stem.unwrap_or_else(|| path.display().to_string())
        })
    };
    let mut out: Vec<(String, PathBuf, Chain)> = Vec::new();
    let mut take = |name: String, path: PathBuf, chain: Chain| {
        if chain.exists()
            && chain.dissolved().is_none()
            && out.iter().all(|(_, _, c)| c.id != chain.id)
        {
            out.push((name, path, chain));
        }
    };
    if let Some(here) = &here {
        take(name_of(here), here.clone(), db.sync_chain()?);
    }
    let mut seen: Vec<PathBuf> = here.into_iter().collect();
    for (_, path) in &known {
        let path = canon(path);
        if seen.contains(&path) || !path.exists() {
            continue;
        }
        seen.push(path.clone());
        let name = name_of(&path);
        match Db::open_read_only(&path.to_string_lossy()).and_then(|d| d.sync_chain()) {
            Ok(chain) => take(name, path, chain),
            Err(e) => eprintln!("⚠ {name} could not be read: {e:#}"),
        }
    }
    Ok(out)
}
