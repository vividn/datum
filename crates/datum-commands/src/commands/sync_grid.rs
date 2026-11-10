//! `datum sync grid` (da-n0uj1.3): every device this install's databases
//! know of, against every chain they are in. A row is a device, by the key
//! its members' `describe`s bind them to (da-n0uj1.2), or an endpoint alone
//! where none is bound; a column is a chain, by its id, labelled with the
//! local database's name, the one the command runs on first. Read only.
//!
//! A grant not yet taken shows on its device's row, as this install can read
//! it (da-pofki): in its chain's column, which one no copy here holds has
//! just for that (`held: false`), or, sealed, beneath the row's cells.
//!
//! A member that the column's database cannot sync with, one of the two
//! below the other's sync floor, is marked on its cell with which must
//! update, and said under the grid in the floor's words (da-52vdj).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use datum_core::db::chain::{self, Chain, Member, Role};
use datum_core::db::Apart;
use datum_core::db::Db;
use serde_json::{json, Value};

use crate::config;
use crate::render::colors::{BOLD, GREY_FG};

use super::sync::{GREEN, RED, YELLOW};

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
    // Per column, the members its database cannot sync with (da-52vdj).
    let here = db
        .path()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
    let apart: Vec<BTreeMap<String, Apart>> = columns
        .iter()
        .map(|(_, path, _)| {
            let read = match here.as_ref() == Some(path) {
                true => db.sync_floors_apart(),
                false => {
                    Db::open_read_only(&path.to_string_lossy()).and_then(|d| d.sync_floors_apart())
                }
            };
            read.unwrap_or_default()
        })
        .collect();
    let apart_of = |r: &Row, j: usize| {
        let (_, endpoint) = r.cells.get(j)?.as_ref()?;
        apart.get(j)?.get(endpoint)
    };
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
    let grants = super::sync_grant::seen(&columns);
    // A chain granted to this device and held nowhere here: a column of its own.
    let mut heads: Vec<(String, String, bool)> = columns
        .iter()
        .map(|(name, _, c)| (name.clone(), c.id.clone(), true))
        .collect();
    for w in grants.iter().filter_map(|g| g.what.as_ref()) {
        if heads.iter().all(|(_, id, _)| *id != w.chain) {
            heads.push((w.db.clone(), w.chain.clone(), false));
        }
    }
    let of = |r: &Row, chain: &str| {
        let at = |g: &&super::sync_grant::Seen| {
            r.device.as_deref() == Some(&g.to) && g.what.as_ref().is_some_and(|w| w.chain == chain)
        };
        grants.iter().find(at)
    };
    let sealed = |r: &Row| {
        let of =
            |g: &&super::sync_grant::Seen| g.what.is_none() && r.device.as_deref() == Some(&g.to);
        grants
            .iter()
            .filter(of)
            .map(|g| g.json())
            .collect::<Vec<_>>()
    };
    if json {
        let out = json!({
            "device": mine,
            "chains": heads.iter().map(|(name, id, held)| json!({"chain": id, "db": name, "held": held})).collect::<Vec<_>>(),
            "devices": rows.iter().map(|r| json!({
                "device": r.device,
                "endpoint": r.device.is_none().then_some(&r.endpoint),
                "label": r.label,
                "this": this(r),
                "cells": heads.iter().enumerate().map(|(j, (_, id, _))| (
                    id.clone(),
                    r.cells.get(j).and_then(Option::as_ref).map_or(Value::Null, |(cell, e)| {
                        let apart = apart_of(r, j).map(|a| json!({
                            "updates": if a.this_updates { "this" } else { "it" },
                            "schema": a.schema, "this_schema": a.mine, "floor": a.floor,
                            "words": a.words(&r.label),
                        }));
                        json!({"state": cell.word(), "endpoint": e, "apart": apart})
                    }),
                )).collect::<serde_json::Map<_, _>>(),
                "grants": heads.iter().filter_map(|(_, id, _)| Some((id.clone(), of(r, id)?.json()))).collect::<serde_json::Map<_, _>>(),
                "sealed": sealed(r),
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
    // A device key and an endpoint are each 32 bytes in hex, 64 ASCII
    // characters, or the fold takes neither: their head is safe to cut. A
    // chain id is a ULID, whose head is only its time: shown whole. A table,
    // as `sync status` draws one (da-hub6q).
    let head = |first: &str, second: &str, of: fn(&(String, String, bool)) -> &str, code| {
        [first, second]
            .into_iter()
            .chain(heads.iter().map(of))
            .map(|h| (h.to_string(), code))
            .collect::<Vec<_>>()
    };
    let mut table = vec![
        head("DEVICE", "KEY", |h| h.0.as_str(), BOLD),
        head("", "", |h| h.1.as_str(), GREY_FG),
    ];
    for r in &rows {
        let key = match &r.device {
            Some(d) => d[..10].to_string(),
            None => format!("endpoint {}", &r.endpoint[..10]),
        };
        let mut line = vec![(r.label.clone(), ""), (key, GREY_FG)];
        for (j, (_, id, _)) in heads.iter().enumerate() {
            let cell = r.cells.get(j).and_then(Option::as_ref).map(|c| c.0);
            let apart = apart_of(r, j);
            let standing = cell.map(|c| match apart {
                Some(a) if a.this_updates => format!("{} ⚠ update this", c.word()),
                Some(_) => format!("{} ⚠ update it", c.word()),
                None => c.word().to_string(),
            });
            let grant = of(r, id).map(|g| g.word().to_string());
            let text = match (standing, grant) {
                (Some(s), Some(g)) => format!("{s}, {g}"),
                (s, g) => s.or(g).unwrap_or_else(|| "—".into()),
            };
            let code = match (cell, apart) {
                (_, Some(_)) => YELLOW,
                (Some(Cell::Member | Cell::Reader), _) => GREEN,
                (Some(Cell::Expelled | Cell::Unjoined), _) => RED,
                _ => GREY_FG,
            };
            line.push((text, code));
        }
        line.push((if this(r) { "(this device)" } else { "" }.into(), ""));
        table.push(line);
    }
    for line in super::sync::table(&table, 2) {
        println!("{line}");
    }
    let mut below: Vec<String> = grants
        .iter()
        .filter(|g| g.what.is_none())
        .map(|g| g.line())
        .collect();
    for r in &rows {
        for (j, (name, _, _)) in columns.iter().enumerate() {
            if let Some(a) = apart_of(r, j) {
                below.push(format!("⚠ {name}: {}", a.words(&r.label)));
            }
        }
    }
    super::sync::print_below(&below);
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
    // As each copy serves it: a member admitted across a sync floor, whose
    // admission waits on ops this copy lacks, is in it, and a copy that
    // joined so holds the chain's records alone (da-52vdj).
    if let Some(here) = &here {
        take(name_of(here), here.clone(), db.sync_chain_served()?);
    }
    let mut seen: Vec<PathBuf> = here.into_iter().collect();
    for (_, path) in &known {
        let path = canon(path);
        if seen.contains(&path) || !path.exists() {
            continue;
        }
        seen.push(path.clone());
        let name = name_of(&path);
        match Db::open_read_only(&path.to_string_lossy()).and_then(|d| d.sync_chain_served()) {
            Ok(chain) => take(name, path, chain),
            Err(e) => eprintln!("⚠ {name} could not be read: {e:#}"),
        }
    }
    Ok(out)
}
